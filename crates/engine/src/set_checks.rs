//! Check my mix: the things worth a second listen in a recorded set, each
//! with a time, a short text and how bad it is.
//!
//! From the recording itself: clipping, silence, and a jump in level from
//! one track to the next. From the control log (see
//! [`crate::set_timeline`]): decks drifting apart in a transition, an
//! effect left on long after it was needed, and a jump (hot cue, seek)
//! that lands away from a phrase start while the deck is heard. Without a
//! log only the first three run.

use crate::loudness::LoudnessMeter;
use crate::set_timeline::{LaneKind, SetTimeline};

/// A sample at or above this is clipped (0 dBFS, allowing for rounding).
const CLIP: f32 = 0.999;
/// Clipped moments closer than this are one finding.
const CLIP_JOIN: f64 = 1.0;
/// Below -50 dBFS counts as silence...
const SILENT: f32 = 0.003_162_3;
/// ...when it lasts longer than this.
const SILENCE_SECONDS: f64 = 2.0;
/// Two tracks' levels further apart than this, in LU, are a level jump.
const LEVEL_JUMP: f64 = 3.0;
/// The part of a track measured for its level must be at least this long.
const LEVEL_MIN_SECONDS: f64 = 3.0;
/// Beats further apart than this are drifting...
const DRIFT_SECONDS: f64 = 0.020;
/// ...when they stay so for more than this many beats.
const DRIFT_BEATS: f64 = 2.0;
/// How often the decks' beats are compared during a transition.
const DRIFT_STEP: f64 = 0.05;
/// An effect still on this many beats after the phrase change following
/// its start was left on.
const FX_LEFT_BEATS: f64 = 16.0;
/// A jump landing more than this many beats from a phrase start is mid-phrase.
const MID_PHRASE_BEATS: f64 = 1.0;
/// The block the audio is looked at in, in seconds.
const BLOCK: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth a listen.
    Note,
    /// Most listeners will hear it.
    Problem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckKind {
    Clipping,
    Silence,
    LevelJump,
    Drift,
    FxLeftOn,
    MidPhraseJump,
}

impl CheckKind {
    pub fn name(self) -> &'static str {
        match self {
            CheckKind::Clipping => "Clipping",
            CheckKind::Silence => "Silence",
            CheckKind::LevelJump => "Level jump",
            CheckKind::Drift => "Beats drifting",
            CheckKind::FxLeftOn => "Effect left on",
            CheckKind::MidPhraseJump => "Jump mid-phrase",
        }
    }
}

/// Something to listen to again.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub kind: CheckKind,
    /// Seconds into the recording.
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub severity: Severity,
}

/// A stretch of the recording where one track plays alone, for comparing
/// levels.
#[derive(Clone, Debug, PartialEq)]
pub struct Stretch {
    pub start: f64,
    pub end: f64,
    pub title: String,
}

/// The stretches between transitions, each named after the track heard.
pub fn stretches(t: &SetTimeline) -> Vec<Stretch> {
    let mut out = Vec::new();
    let mut from = 0.0;
    let mut track: Option<usize> = t.clips.first().map(|c| c.track);
    for x in &t.transitions {
        if let Some(tr) = track {
            out.push(Stretch {
                start: from,
                end: x.start,
                title: t.tracks[tr].title.clone(),
            });
        }
        from = x.end;
        track = Some(x.to_track);
    }
    if let Some(tr) = track {
        out.push(Stretch {
            start: from,
            end: t.length,
            title: t.tracks[tr].title.clone(),
        });
    }
    out.retain(|s| s.end - s.start >= LEVEL_MIN_SECONDS);
    out
}

/// Clipping, silence and level jumps, from the recording's frames (at
/// `rate`) and the stretches each track plays alone (from [`stretches`],
/// or from a cue sheet's start times).
pub fn check_audio(
    frames: impl IntoIterator<Item = [f32; 2]>,
    rate: f64,
    stretches: &[Stretch],
) -> Vec<Finding> {
    let block = ((rate * BLOCK).round() as usize).max(1);
    let mut findings = Vec::new();
    let mut clipped: Vec<(f64, f64)> = Vec::new();
    let mut silent_since: Option<f64> = None;
    let mut silences: Vec<(f64, f64)> = Vec::new();
    let mut levels: Vec<Option<f64>> = vec![None; stretches.len()];
    let mut stretch = 0;
    let mut meter: Option<LoudnessMeter> = None;
    let (mut n, mut peak) = (0usize, 0f32);
    let mut frame = 0usize;
    for f in frames {
        let t = frame as f64 / rate;
        // The track stretch this frame falls in.
        while stretch < stretches.len() && t >= stretches[stretch].end {
            if let Some(m) = meter.take() {
                levels[stretch] = m.finish().map(|l| l.lufs as f64);
            }
            stretch += 1;
        }
        if stretch < stretches.len() && t >= stretches[stretch].start {
            meter
                .get_or_insert_with(|| LoudnessMeter::new(rate))
                .push(f);
        }
        peak = peak.max(f[0].abs()).max(f[1].abs());
        n += 1;
        frame += 1;
        if n == block {
            let start = (frame - n) as f64 / rate;
            let end = frame as f64 / rate;
            if peak >= CLIP {
                match clipped.last_mut() {
                    Some(last) if start - last.1 <= CLIP_JOIN => last.1 = end,
                    _ => clipped.push((start, end)),
                }
            }
            if peak < SILENT {
                silent_since.get_or_insert(start);
            } else if let Some(s) = silent_since.take() {
                silences.push((s, start));
            }
            n = 0;
            peak = 0.0;
        }
    }
    if let (Some(m), true) = (meter.take(), stretch < stretches.len()) {
        levels[stretch] = m.finish().map(|l| l.lufs as f64);
    }
    for (start, end) in clipped {
        findings.push(Finding {
            kind: CheckKind::Clipping,
            start,
            end,
            text: "The master hit 0 dBFS".into(),
            severity: Severity::Problem,
        });
    }
    // Silence at the very start or end is just the recording running
    // before and after the set; a trim takes it off.
    for (start, end) in silences {
        if end - start > SILENCE_SECONDS && start > 0.0 {
            findings.push(Finding {
                kind: CheckKind::Silence,
                start,
                end,
                text: format!("{:.0} s of silence", end - start),
                severity: Severity::Problem,
            });
        }
    }
    for i in 1..stretches.len() {
        let (Some(before), Some(after)) = (levels[i - 1], levels[i]) else {
            continue;
        };
        let diff = after - before;
        if diff.abs() > LEVEL_JUMP {
            findings.push(Finding {
                kind: CheckKind::LevelJump,
                start: stretches[i].start,
                end: stretches[i].start,
                text: format!(
                    "{} is {:.1} dB {} than {}",
                    stretches[i].title,
                    diff.abs(),
                    if diff > 0.0 { "louder" } else { "quieter" },
                    stretches[i - 1].title
                ),
                severity: Severity::Note,
            });
        }
    }
    findings.sort_by(|a, b| a.start.total_cmp(&b.start));
    findings
}

/// A track's beat grid and phrase starts, from analysis, in seconds into
/// the track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackGrid {
    pub first_beat: f64,
    pub bpm: f64,
    pub phrases: Vec<f64>,
}

impl TrackGrid {
    fn beat_length(&self) -> f64 {
        60.0 / self.bpm
    }

    fn beat_at(&self, s: f64) -> f64 {
        (s - self.first_beat) / self.beat_length()
    }
}

/// Drift, effects left on and mid-phrase jumps, from the control log.
/// `grids` has one entry per track of `t.tracks` (None when the track has
/// no grid: those checks skip it).
pub fn check_log(t: &SetTimeline, grids: &[Option<TrackGrid>]) -> Vec<Finding> {
    let grid = |track: usize| {
        grids
            .get(track)
            .and_then(|g| g.as_ref())
            .filter(|g| g.bpm > 0.0)
    };
    let mut findings = Vec::new();

    // Beats drifting apart while both decks are heard.
    for x in &t.transitions {
        let (Some(ga), Some(gb)) = (grid(x.from_track), grid(x.to_track)) else {
            continue;
        };
        let (Some(ca), Some(cb)) = (
            t.clip_at(x.from_deck, x.start),
            t.clip_at(x.to_deck, x.start),
        ) else {
            continue;
        };
        let beat = ga.beat_length() / ca.speed.abs().max(0.1);
        let mut drifting: Option<f64> = None;
        let mut worst = 0f64;
        let mut s = x.start;
        let mut found = false;
        while s <= x.end && !found {
            let a = ga.beat_at(ca.track_start + (s - ca.start) * ca.speed);
            let b = gb.beat_at(cb.track_start + (s - cb.start) * cb.speed);
            let off = ((a - b) + 0.5).rem_euclid(1.0) - 0.5;
            let seconds = off.abs() * beat;
            if seconds > DRIFT_SECONDS {
                let since = *drifting.get_or_insert(s);
                worst = worst.max(seconds);
                if s - since > DRIFT_BEATS * beat {
                    found = true;
                    findings.push(Finding {
                        kind: CheckKind::Drift,
                        start: since,
                        end: x.end,
                        text: format!(
                            "{} and {} drift {:.0} ms apart",
                            t.tracks[x.from_track].title,
                            t.tracks[x.to_track].title,
                            worst * 1000.0
                        ),
                        severity: Severity::Problem,
                    });
                }
            } else {
                drifting = None;
                worst = 0.0;
            }
            s += DRIFT_STEP;
        }
    }

    // Effects left on: an effect switched on before a phrase change of the
    // deck it's on, still on 4 bars after it.
    for lane in &t.lanes {
        let LaneKind::FxEffect(unit, _) = lane.kind else {
            continue;
        };
        let mut on: Option<f64> = None;
        let mut spans = Vec::new();
        for &(at, v) in &lane.points {
            match (on, v > 0.5) {
                (None, true) => on = Some(at),
                (Some(from), false) => {
                    spans.push((from, at));
                    on = None;
                }
                _ => {}
            }
        }
        if let Some(from) = on {
            spans.push((from, t.length));
        }
        for (from, to) in spans {
            for deck in t.decks() {
                let route = format!(
                    "{},group_{}_enable",
                    crate::controls::effect_unit_group(unit),
                    crate::controls::deck_group(deck)
                );
                if t.value_at(&route, from).unwrap_or(0.0) < 0.5 {
                    continue;
                }
                let Some(clip) = t.clip_at(deck, from) else {
                    continue;
                };
                let Some(g) = grid(clip.track) else { continue };
                let place = clip.track_start + (from - clip.start) * clip.speed;
                let Some(&phrase) = g.phrases.iter().find(|p| **p > place) else {
                    continue;
                };
                let late = phrase + FX_LEFT_BEATS * g.beat_length();
                let speed = clip.speed.abs().max(0.1);
                let at = clip.start + (late - clip.track_start) / speed;
                if at < to && at < clip.end && t.audible_at(deck, at) {
                    findings.push(Finding {
                        kind: CheckKind::FxLeftOn,
                        start: from,
                        end: to,
                        text: format!(
                            "An effect stayed on over {} into its next phrase",
                            t.tracks[clip.track].title
                        ),
                        severity: Severity::Note,
                    });
                }
            }
        }
    }

    // Jumps that land away from a phrase start while the deck is heard.
    for deck in t.decks() {
        let clips: Vec<_> = t.clips.iter().filter(|c| c.deck == deck).collect();
        for w in clips.windows(2) {
            let (a, b) = (w[0], w[1]);
            let jumped = a.track == b.track && (a.end - b.start).abs() < 1e-6;
            if !jumped || !t.audible_at(deck, b.start) {
                continue;
            }
            let Some(g) = grid(b.track) else { continue };
            if g.phrases.is_empty() {
                continue;
            }
            let beat = g.beat_at(b.track_start);
            let nearest = g
                .phrases
                .iter()
                .map(|p| (g.beat_at(*p) - beat).abs())
                .fold(f64::MAX, f64::min);
            if nearest > MID_PHRASE_BEATS {
                findings.push(Finding {
                    kind: CheckKind::MidPhraseJump,
                    start: b.start,
                    end: b.start,
                    text: format!("A jump in {} landed mid-phrase", t.tracks[b.track].title),
                    severity: Severity::Note,
                });
            }
        }
    }
    findings.sort_by(|a, b| a.start.total_cmp(&b.start));
    findings
}

#[cfg(test)]
mod tests;
