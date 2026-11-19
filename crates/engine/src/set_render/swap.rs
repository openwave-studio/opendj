//! Swapping a track of a recorded set for another, and rendering edits
//! next to each other together so they agree.
//!
//! The replacement plays the same stretch of the set as the track it
//! replaces, from where that one came in to where it went out, at the speed
//! that matches the set's tempo there. Its beats land on the replaced
//! track's beats, its bar on that track's bar. The DJ's logged moves (faders,
//! EQ, filters) stay as they were, so its transitions in and out keep their
//! recorded style and length.

use std::path::Path;

use super::*;
use crate::set_chapters::clock;
use crate::set_timeline::Clip;

/// A track of the set swapped for another.
#[derive(Clone, Debug, PartialEq)]
pub struct Swap {
    /// Which track of the set, from 0, as the tracklist counts them.
    pub slot: usize,
    /// The replacement (its file, tempo and names).
    pub track: LoadedTrack,
}

/// Everything changed in a set, so a stretch rendered for one edit also
/// has the others that reach into it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetEdits {
    pub swaps: Vec<Swap>,
    pub transitions: Vec<TransitionEdit>,
}

/// The stretch to render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// A transition mixed again (index into the timeline's).
    Transition(usize),
    /// A track swapped (its slot).
    Swap(usize),
}

/// Fewest and most the replacement may be sped up or slowed down.
const SPEEDS: std::ops::RangeInclusive<f64> = 0.75..=1.34;

/// The timeline's track for the set's `slot`th track: the first one
/// played, then each transition's incoming one.
pub fn slot_track(t: &SetTimeline, slot: usize) -> Option<usize> {
    if slot == 0 {
        t.clips.first().map(|c| c.track)
    } else {
        t.transitions.get(slot - 1).map(|x| x.to_track)
    }
}

/// The transitions into and out of the set's `slot`th track.
pub fn transitions_around(t: &SetTimeline, slot: usize) -> (Option<usize>, Option<usize>) {
    let k = slot_track(t, slot);
    let into = slot.checked_sub(1).filter(|i| *i < t.transitions.len());
    let out = t
        .transitions
        .get(slot)
        .filter(|x| Some(x.from_track) == k)
        .map(|_| slot);
    (into, out)
}

/// One swap worked out: the stretch it plays and the deck's tempo moves.
struct Swapped {
    from: f64,
    to: f64,
    changes: Vec<Change>,
}

/// `t` with `swaps` made: each replaced track's clips become one clip of
/// the replacement. Also gives each swap's stretch and the tempo fader
/// moves that match it to the set. `track` gives a playable track (its
/// beat grid and length are used).
fn swapped(
    t: &SetTimeline,
    swaps: &[Swap],
    track: &mut dyn FnMut(&LoadedTrack) -> Result<Arc<Track>, String>,
) -> Result<(SetTimeline, Vec<Swapped>), String> {
    let mut out = t.clone();
    let mut stretches = Vec::new();
    for s in swaps {
        let k = slot_track(t, s.slot).ok_or("that track isn't in the set")?;
        let (into, _) = transitions_around(t, s.slot);
        let mine: Vec<&Clip> = t.clips.iter().filter(|c| c.track == k).collect();
        let first = *mine.first().ok_or("that track isn't in the set")?;
        let end = mine.iter().map(|c| c.end).fold(first.end, f64::max);
        // From where it came in.
        let a = into
            .map_or(first.start, |i| t.transitions[i].start)
            .max(first.start);
        let clip = mine
            .iter()
            .find(|c| c.start <= a && a < c.end)
            .copied()
            .unwrap_or(first);
        let place = clip.track_start + (a - clip.start) * clip.speed;

        // The set's tempo and bar there, from the replaced track's grid.
        let old = track(&t.tracks[k]).ok();
        let old_bpm = old.as_ref().map_or(0.0, |o| o.bpm_at(place));
        let gridded = old_bpm > 0.0;
        let old_bpm = if gridded { old_bpm } else { t.tracks[k].bpm };
        if old_bpm <= 0.0 {
            return Err(format!(
                "{} has no tempo to match; analyse it first",
                t.tracks[k].title
            ));
        }
        let tempo = old_bpm * clip.speed;
        let phase = match &old {
            Some(o) if gridded => o.beat_at(place).rem_euclid(4.0),
            _ => 0.0,
        };

        // The replacement from its first bar, on the same beat of the bar.
        let new = track(&s.track)?;
        let mut beat = phase;
        while new.seconds_at(beat) < 0.0 {
            beat += 4.0;
        }
        let p0 = new.seconds_at(beat);
        let new_bpm = match new.bpm_at(p0) {
            b if b > 0.0 => b,
            _ => s.track.bpm,
        };
        if new_bpm <= 0.0 {
            return Err(format!(
                "{} has no tempo yet; analyse it first",
                s.track.title
            ));
        }
        let speed = tempo / new_bpm;
        if !SPEEDS.contains(&speed) {
            return Err(format!(
                "{} is {:.0} BPM, too far from the set's {:.0} here",
                s.track.title, new_bpm, tempo
            ));
        }
        let need = p0 + (end - a) * speed;
        if need > new.duration + 1e-3 {
            return Err(format!(
                "{} is {} long; this stretch needs {}",
                s.track.title,
                clock(new.duration),
                clock(need)
            ));
        }

        out.clips.retain(|c| c.track != k);
        out.clips.push(Clip {
            deck: first.deck,
            track: k,
            start: a,
            end,
            track_start: p0,
            speed,
        });
        out.tracks[k] = LoadedTrack {
            deck: t.tracks[k].deck,
            loaded_at: t.tracks[k].loaded_at,
            ..s.track.clone()
        };

        // The tempo fader where it plays the replacement at the set's tempo.
        let g = deck_group(first.deck);
        let range = t
            .value_at(&format!("{g},rateRange"), a)
            .unwrap_or(0.08)
            .max((speed - 1.0).abs() * 1.05)
            .min(1.0);
        let dir = if t.value_at(&format!("{g},rate_dir"), a).unwrap_or(1.0) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let at = (a - 1e-3).max(0.0);
        stretches.push(Swapped {
            from: a,
            to: end,
            // Held to the end of the stretch, so the log's nudges to the
            // old track's tempo aren't applied to the new one.
            changes: vec![
                (at, format!("{g},rateRange"), range),
                (at, format!("{g},rate"), (speed - 1.0) / (range * dir)),
                (end, format!("{g},rateRange"), range),
                (end, format!("{g},rate"), (speed - 1.0) / (range * dir)),
            ],
        });
    }
    out.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok((out, stretches))
}

/// The window to render for the swap of `slot` in `t` (already swapped):
/// from a few bars before the transition into it to a few after the one
/// out of it, clear of the transitions around them.
fn swap_window(t: &SetTimeline, slot: usize) -> Result<Window, String> {
    let (into, out) = transitions_around(t, slot);
    let from = match into {
        Some(i) => {
            let x = &t.transitions[i];
            let bar = bar_seconds(t, x.to_track, x.start);
            let before = i
                .checked_sub(1)
                .and_then(|p| t.transitions.get(p))
                .map_or(0.0, |p| p.end);
            (x.start - SIDE_BARS * bar).max(before + 0.5).min(x.start)
        }
        None => 0.0,
    };
    let to = match out {
        Some(i) => {
            let x = &t.transitions[i];
            let bar = bar_seconds(t, x.from_track, x.start);
            let after = t.transitions.get(i + 1).map_or(t.length, |n| n.start);
            (x.end + SIDE_BARS * bar).min(after - 0.5).max(x.end)
        }
        None => t.length,
    };
    if to <= from {
        return Err("nothing to swap there".into());
    }
    Ok(Window {
        from,
        to,
        ..Default::default()
    })
}

/// The window for `target` with every edit in `edits` that reaches into
/// it, in `t` with the swaps made (also returned).
fn window_for(
    t: &SetTimeline,
    edits: &SetEdits,
    target: Target,
    track: &mut dyn FnMut(&LoadedTrack) -> Result<Arc<Track>, String>,
) -> Result<(SetTimeline, Window), String> {
    let (t2, stretches) = swapped(t, &edits.swaps, track)?;
    let mut window = match target {
        Target::Transition(i) => {
            let edit = edits
                .transitions
                .iter()
                .rev()
                .find(|e| e.transition == i)
                .ok_or("that transition wasn't changed")?;
            let mut w = transition_window(&t2, edit)?;
            w.held = spans(&w.changes);
            w
        }
        Target::Swap(slot) => swap_window(&t2, slot)?,
    };
    for e in &edits.transitions {
        if target == Target::Transition(e.transition) {
            continue;
        }
        let Some(x) = t2.transitions.get(e.transition) else {
            continue;
        };
        if x.start > window.from && x.start < window.to {
            if let Ok(w) = transition_window(&t2, e) {
                window.held.extend(spans(&w.changes));
                window.changes.extend(w.changes);
                window.play_until.extend(w.play_until);
            }
        }
    }
    for s in &stretches {
        if s.from < window.to && s.to > window.from {
            window.held.extend(spans(&s.changes));
            window.changes.extend(s.changes.iter().cloned());
        }
    }
    window.changes.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok((t2, window))
}

/// Checks `target` of `edits` can be made, without rendering: the error
/// to show when it can't.
pub fn check_edit(
    t: &SetTimeline,
    edits: &SetEdits,
    target: Target,
    track: &mut dyn FnMut(&LoadedTrack) -> Result<Arc<Track>, String>,
) -> Result<(), String> {
    window_for(t, edits, target, track).map(|_| ())
}

/// Renders `target` of the set recorded at `audio`, with all of `edits`
/// that reach into it, into `dest` as a WAV for a splice fix. Returns the
/// stretch it covers, in seconds.
pub fn render_edits(
    audio: &Path,
    timeline: &SetTimeline,
    edits: &SetEdits,
    target: Target,
    dest: &Path,
) -> Result<(f64, f64), String> {
    let mut rate = 0u32;
    let _ = crate::decode::Decoder::open(audio)?.stream(|r, _| {
        rate = r;
        false
    });
    if rate == 0 {
        return Err(format!("{}: no audio decoded", audio.display()));
    }
    let mut cache: Vec<(LoadedTrack, Arc<Track>)> = Vec::new();
    let mut track = |t: &LoadedTrack| -> Result<Arc<Track>, String> {
        if let Some((_, p)) = cache.iter().find(|(lt, _)| lt.path == t.path) {
            return Ok(p.clone());
        }
        let p = playable(t)?;
        cache.push((t.clone(), p.clone()));
        Ok(p)
    };
    let (t2, window) = window_for(timeline, edits, target, &mut track)?;
    let frames = render(&t2, &window, rate, &mut track)?;
    write_wav(dest, &frames, rate)?;
    Ok((window.from, window.from + frames.len() as f64 / rate as f64))
}

/// Renders `window` of `t` with `edits` made, for tests.
#[cfg(test)]
pub(super) fn render_target(
    t: &SetTimeline,
    edits: &SetEdits,
    target: Target,
    rate: u32,
    track: &mut dyn FnMut(&LoadedTrack) -> Result<Arc<Track>, String>,
) -> Result<(Window, Vec<[f32; 2]>), String> {
    let (t2, window) = window_for(t, edits, target, track)?;
    let frames = render(&t2, &window, rate, track)?;
    Ok((window, frames))
}
