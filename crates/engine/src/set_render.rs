//! Renders a stretch of a recorded set again through openDJ's engine, from
//! its control log, so an edit (a transition made longer or mixed another
//! way) can be heard and saved without touching the rest of the recording.
//!
//! Only the edited stretch is rendered. It starts and ends where one deck
//! plays alone, and is spliced into the recording there with a short
//! equal-power crossfade: both sides of a seam are the same track at the
//! same place, so the small differences between a render from the 5 ms log
//! and the recording itself aren't heard. See the add-set-view design.

use std::sync::Arc;

use opendj_control::ControlRegistry;

use crate::controls::{deck_group, eq_group, quick_effect_group};
use crate::engine::{Command, Engine};
use crate::set_timeline::{LoadedTrack, SetTimeline, Style, Transition};
use crate::track::Track;

/// Rendered before the window and thrown away, so effect tails and the
/// key lock's stretcher have settled when it starts.
const PREROLL: f64 = 2.0;
/// Frames per engine call: control changes land within this.
const BLOCK: usize = 64;
/// The crossfade at each seam, in seconds.
pub const SEAM: f64 = 0.02;
/// Bars kept on each side of an edited transition: one deck alone at the
/// seams, and enough to hear the change in its place.
const SIDE_BARS: f64 = 4.0;

/// One control change: seconds into the recording, `[Group],item`, value.
pub type Change = (f64, String, f64);

/// What to render: `from` to `to` seconds of the set, with `changes` in
/// place of the log's in that stretch (the state at `from` still comes from
/// the log), and decks held playing until a time past their recorded stop.
#[derive(Clone, Debug, Default)]
pub struct Window {
    pub from: f64,
    pub to: f64,
    pub changes: Vec<Change>,
    /// (deck, seconds): the deck keeps playing until then even if the
    /// recording stopped it sooner.
    pub play_until: Vec<(usize, f64)>,
    /// (control, from, to): where the edit takes a control over from the
    /// log. Empty: each control `changes` sets, from its first change to
    /// its last.
    pub held: Vec<(String, f64, f64)>,
}

/// Each control `changes` sets, from its first change to its last.
pub fn spans(changes: &[Change]) -> Vec<(String, f64, f64)> {
    let mut out: Vec<(String, f64, f64)> = Vec::new();
    for (t, name, _) in changes {
        match out.iter_mut().find(|s| &s.0 == name) {
            Some(s) => {
                s.1 = s.1.min(*t);
                s.2 = s.2.max(*t);
            }
            None => out.push((name.clone(), *t, *t)),
        }
    }
    out
}

/// Renders `window` of `timeline` at `rate`. `track` gives the playable
/// track (audio and beat grid) for one the log names.
pub fn render(
    timeline: &SetTimeline,
    window: &Window,
    rate: u32,
    track: &mut dyn FnMut(&LoadedTrack) -> Result<Arc<Track>, String>,
) -> Result<Vec<[f32; 2]>, String> {
    let sr = rate as f64;
    let start = (window.from - PREROLL).max(0.0);
    let reg = ControlRegistry::new();
    let (mut engine, mut input) = Engine::new(&reg, sr);
    // What has been set, to set again on a deck after a load resets it.
    let state = std::cell::RefCell::new(std::collections::HashMap::<String, f64>::new());
    let set = |name: &str, v: f64| {
        if let Some((group, item)) = split(name) {
            if replayed(group, item) {
                if let Some(h) = reg.get(group, item) {
                    h.set(v);
                    state.borrow_mut().insert(name.to_string(), v);
                }
            }
        }
    };

    // What each deck does, from `start`: (seconds, deck, what).
    let mut plays: Vec<(f64, usize, Play)> = Vec::new();
    for (ci, c) in timeline.clips.iter().enumerate() {
        let end = window
            .play_until
            .iter()
            .filter(|(d, _)| *d == c.deck)
            .map(|(_, until)| *until)
            .fold(c.end, f64::max);
        // Only the last clip of a deck is held on past its stop.
        let next = timeline
            .clips
            .iter()
            .find(|n| n.deck == c.deck && n.start >= c.end - 1e-6 && n.start > c.start);
        let end = next.map_or(end, |n| end.min(n.start));
        if end <= start || c.start >= window.to {
            continue;
        }
        plays.push((c.start.max(start), c.deck, Play::Start(c.track, ci)));
        if end < window.to {
            plays.push((end, c.deck, Play::Stop));
        }
    }
    plays.sort_by(|a, b| a.0.total_cmp(&b.0));

    // The tracks, loaded once each.
    let mut loaded: Vec<Option<Arc<Track>>> = vec![None; timeline.tracks.len()];
    for (_, _, p) in &plays {
        if let Play::Start(t, _) = p {
            if loaded[*t].is_none() {
                loaded[*t] = Some(track(&timeline.tracks[*t])?);
            }
        }
    }

    // The decks' first tracks, loaded in one frame before anything else
    // (loading resets a deck's EQ and effects).
    let mut on_deck: Vec<Option<usize>> = vec![None; crate::NUM_DECKS];
    let mut out = Vec::new();
    for (_, deck, what) in &plays {
        if let (Play::Start(tr, _), None) = (what, on_deck[*deck]) {
            if let Some(t) = &loaded[*tr] {
                input
                    .send(Command::Load {
                        deck: *deck,
                        track: t.clone(),
                    })
                    .map_err(|_| "the engine's queue is full".to_string())?;
                on_deck[*deck] = Some(*tr);
            }
        }
    }
    let mut one = [0.0f32; 2];
    engine.process(&mut one, 2);
    out.push(one);
    // The mixer as it was at `start`.
    for (name, v) in timeline.values_at(start) {
        set(name, v);
    }
    // Where the edit holds controls instead of the log.
    let held = if window.held.is_empty() {
        spans(&window.changes)
    } else {
        window.held.clone()
    };
    let mut changes: Vec<(f64, &str, f64)> = timeline
        .changes_between(start, window.from)
        .into_iter()
        .chain(
            timeline
                .changes_between(window.from, window.to)
                .into_iter()
                .filter(|_| window.changes.is_empty()),
        )
        .collect();
    if !window.changes.is_empty() {
        // The log's changes in the window, except where the edit holds
        // the control.
        let holds = |name: &str, t: f64| {
            held.iter()
                .any(|h| h.0 == name && t >= h.1 - 1e-9 && t <= h.2 + 1e-9)
        };
        changes.extend(
            timeline
                .changes_between(window.from, window.to)
                .into_iter()
                .filter(|c| !holds(c.1, c.0)),
        );
        changes.extend(
            window
                .changes
                .iter()
                .filter(|c| c.0 > window.from - 1e-9 && c.0 <= window.to)
                .map(|c| (c.0, c.1.as_str(), c.2)),
        );
        // A control the edit already holds at `from` (a swapped track's
        // tempo) as the edit last set it.
        for (name, _, _) in held
            .iter()
            .filter(|h| h.1 < window.from && h.2 >= window.from)
        {
            let earlier = window
                .changes
                .iter()
                .filter(|c| &c.1 == name && c.0 <= window.from - 1e-9)
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|c| c.2);
            if let Some(v) = earlier.or_else(|| timeline.value_at(name, window.from)) {
                changes.push((window.from - 1e-9, name.as_str(), v));
            }
        }
    }
    changes.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut buf = vec![0.0f32; BLOCK * 2];
    let frames = ((window.to - start) * sr).ceil() as usize;
    let skip = ((window.from - start) * sr).round() as usize;
    let (mut p, mut c) = (0, 0);
    let mut frame = 1;
    while frame < frames {
        let mut reloaded = false;
        while p < plays.len() && ((plays[p].0 - start) * sr).round() as usize <= frame {
            let (_, deck, what) = plays[p];
            let g = deck_group(deck);
            match what {
                Play::Start(tr, ci) => {
                    let Some(audio_track) = loaded[tr].clone() else {
                        p += 1;
                        continue;
                    };
                    if on_deck[deck] != Some(tr) {
                        input
                            .send(Command::Load {
                                deck,
                                track: audio_track.clone(),
                            })
                            .map_err(|_| "the engine's queue is full".to_string())?;
                        on_deck[deck] = Some(tr);
                        reloaded = true;
                        // Takes the load before the place is set.
                        engine.process(&mut one, 2);
                        out.push(one);
                        frame += 1;
                    }
                    let clip = &timeline.clips[ci];
                    let now = start + frame as f64 / sr;
                    let place = clip.track_start + (now - clip.start).max(0.0) * clip.speed;
                    let duration = audio_track.duration.max(1e-6);
                    if let Some(h) = reg.get(&g, "playposition") {
                        h.set((place / duration).clamp(0.0, 1.0));
                    }
                    if let Some(h) = reg.get(&g, "play") {
                        h.set_bool(true);
                    }
                }
                Play::Stop => {
                    if let Some(h) = reg.get(&g, "play") {
                        h.set_bool(false);
                    }
                }
            }
            p += 1;
        }
        if reloaded {
            let again: Vec<(String, f64)> = state
                .borrow()
                .iter()
                .filter(|(name, _)| is_deck_mixer(name))
                .map(|(n, v)| (n.clone(), *v))
                .collect();
            for (name, v) in again {
                set(&name, v);
            }
        }
        while c < changes.len() && ((changes[c].0 - start) * sr).round() as usize <= frame {
            set(changes[c].1, changes[c].2);
            c += 1;
        }
        // Up to the next change, so each lands on its own frame.
        let next = [plays.get(p).map(|x| x.0), changes.get(c).map(|x| x.0)]
            .into_iter()
            .flatten()
            .map(|at| (((at - start) * sr).round() as usize).max(frame + 1))
            .min()
            .unwrap_or(usize::MAX);
        let n = BLOCK.min(frames - frame).min(next - frame);
        engine.process(&mut buf[..n * 2], 2);
        out.extend_from_slice(buf[..n * 2].as_chunks::<2>().0);
        frame += n;
    }
    Ok(out.split_off(skip.min(out.len())))
}

#[derive(Clone, Copy, Debug)]
enum Play {
    /// The track and the clip (indexes into the timeline's).
    Start(usize, usize),
    Stop,
}

fn split(name: &str) -> Option<(&str, &str)> {
    let i = name.find("],")?;
    Some((&name[..=i], &name[i + 2..]))
}

/// The controls a render takes from the log: the mixer and effects, not
/// transport (that comes from the clips) or one-shot actions.
fn replayed(group: &str, item: &str) -> bool {
    if group.starts_with("[Channel") {
        return matches!(
            item,
            "volume" | "pregain" | "rate" | "rateRange" | "rate_dir" | "keylock" | "orientation"
        );
    }
    if group.contains("Equalizer") || group.contains("QuickEffect") || group.contains("EffectRack")
    {
        return matches!(
            item,
            "enabled" | "super1" | "mix" | "meta" | "loaded_effect"
        ) || item.starts_with("parameter")
            || item.starts_with("button_parameter")
            || item.starts_with("group_");
    }
    match group {
        "[Master]" => matches!(item, "crossfader" | "gain" | "balance"),
        "[Mixer]" | "[Mixer Profile]" => true,
        _ => false,
    }
}

/// A deck's own mixer controls, which a load resets.
fn is_deck_mixer(name: &str) -> bool {
    split(name).is_some_and(|(g, i)| {
        (g.starts_with("[Channel") && matches!(i, "volume" | "pregain" | "orientation"))
            || g.contains("Equalizer")
            || g.contains("QuickEffect")
    })
}

/// Splices `render` (from `from` seconds) into `recording` with an
/// equal-power crossfade of [`SEAM`] at each end.
pub fn splice(recording: &mut [[f32; 2]], render: &[[f32; 2]], from: f64, rate: u32) {
    let at = (from * rate as f64).round() as usize;
    let fade = ((SEAM * rate as f64) as usize).max(1);
    let n = render.len().min(recording.len().saturating_sub(at));
    for i in 0..n {
        recording[at + i] = blend(recording[at + i], &render[..n], i, fade);
    }
}

/// Frame `i` of `render` over the recording's frame `rec`, crossfaded over
/// `fade` frames at each end of `render`.
pub fn blend(rec: [f32; 2], render: &[[f32; 2]], i: usize, fade: usize) -> [f32; 2] {
    let r = render[i];
    let edge = i.min(render.len() - 1 - i);
    let w = if edge < fade {
        ((edge as f64 + 0.5) / fade as f64 * std::f64::consts::FRAC_PI_2).sin() as f32
    } else {
        1.0
    };
    let (wr, wo) = (w, (1.0 - w * w).max(0.0).sqrt());
    [r[0] * wr + rec[0] * wo, r[1] * wr + rec[1] * wo]
}

/// A track the log names, ready to play: its file decoded, its beat grid
/// from the analysis cache when there is one.
pub fn playable(t: &LoadedTrack) -> Result<Arc<Track>, String> {
    let path = t
        .path
        .as_ref()
        .ok_or_else(|| format!("{}: no file", t.title))?;
    if !path.exists() {
        return Err(format!("{} isn't at {} any more", t.title, path.display()));
    }
    let audio = crate::decode::decode(path)?;
    let mut track = crate::scan::read_track(0, path);
    track.duration = audio.duration();
    track.audio = Some(Arc::new(audio));
    if let Some(a) = crate::cache::load(path) {
        track.set_grid(a.grid);
    }
    Ok(Arc::new(track))
}

/// Writes `frames` to `dest` as a 24-bit WAV.
fn write_wav(dest: &std::path::Path, frames: &[[f32; 2]], rate: u32) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", dest.display());
    let mut sink =
        crate::recorder::audio_sink(dest, crate::recorder::RecordFormat::Wav, 24, rate as f64)
            .map_err(err)?;
    let flat: Vec<f32> = frames.iter().flatten().copied().collect();
    sink.write(&flat).map_err(err)?;
    sink.finish().map_err(err)
}

/// A transition mixed again: `bars` long from where the incoming track
/// came in, in `style`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionEdit {
    /// Index into the timeline's transitions.
    pub transition: usize,
    pub bars: f64,
    pub style: Style,
}

/// Seconds per bar of `track` at `at` (2 when its tempo isn't known).
pub fn bar_seconds(t: &SetTimeline, track: usize, at: f64) -> f64 {
    let speed = t
        .clips
        .iter()
        .find(|c| c.track == track && c.start <= at && at < c.end.max(c.start + 1.0))
        .map_or(1.0, |c| c.speed);
    let bpm = t.tracks.get(track).map_or(0.0, |lt| lt.bpm) * speed;
    if bpm > 0.0 {
        4.0 * 60.0 / bpm
    } else {
        2.0
    }
}

/// The window to render for `edit`, and the changes that mix it.
pub fn transition_window(t: &SetTimeline, edit: &TransitionEdit) -> Result<Window, String> {
    let x = t
        .transitions
        .get(edit.transition)
        .ok_or("no such transition")?;
    let bar = bar_seconds(t, x.to_track, x.start);
    let s = x.start;
    let e = s + if edit.style == Style::Cut {
        0.0
    } else {
        edit.bars.max(1.0) * bar
    };
    // One deck alone on each side, clear of the transitions around it.
    let before = edit
        .transition
        .checked_sub(1)
        .and_then(|i| t.transitions.get(i))
        .map_or(0.0, |p| p.end);
    let after = t
        .transitions
        .get(edit.transition + 1)
        .map_or(t.length, |n| n.start);
    let from = (s - SIDE_BARS * bar).max(before + 0.5).min(s);
    let to = (x.end.max(e) + SIDE_BARS * bar).min(after - 0.5);
    if to <= e {
        return Err(format!(
            "{} bars runs into the next transition; {} at most",
            edit.bars,
            ((after - 0.5 - s) / bar).floor().max(0.0)
        ));
    }
    Ok(Window {
        from,
        to,
        changes: mix(t, x, s, e, edit.style),
        play_until: vec![(x.from_deck, e)],
        held: Vec::new(),
    })
}

/// The moves that mix `x` from `s` to `e` in `style`: the outgoing deck
/// down, the incoming one up, at the levels each rests at alone.
fn mix(t: &SetTimeline, x: &Transition, s: f64, e: f64, style: Style) -> Vec<Change> {
    let (out, inc) = (x.from_deck, x.to_deck);
    let before = (s - 0.05).max(0.0);
    let after = x.end + 0.05;
    let v = |name: &str, at: f64, d: f64| t.value_at(name, at).unwrap_or(d);
    let vol = |d: usize| format!("{},volume", deck_group(d));
    let low = |d: usize| format!("{},parameter1", eq_group(d));
    let filter = |d: usize| format!("{},super1", quick_effect_group(d));
    let xf = "[Master],crossfader".to_string();
    let out_level = v(&vol(out), before, 1.0);
    let in_level = v(&vol(inc), after, 1.0);
    let out_low = v(&low(out), before, 1.0);
    let in_low = v(&low(inc), after, 1.0);
    let in_filter = v(&filter(inc), after, 0.5);
    let out_filter = v(&filter(out), before, 0.5);
    // A crossfader that moved across the transition carries the levels.
    let (xa, xb) = (v(&xf, before, 0.0), v(&xf, after, 0.0));
    let by_crossfader = (xa - xb).abs() > 0.2;

    let mut c: Vec<Change> = Vec::new();
    // `steps` points of a line from a to b over [t0, t1].
    let ramp = |c: &mut Vec<Change>, name: &str, t0: f64, t1: f64, a: f64, b: f64| {
        let steps = (((t1 - t0) / 0.05).ceil() as usize).clamp(1, 400);
        for k in 0..=steps {
            let f = k as f64 / steps as f64;
            c.push((t0 + (t1 - t0) * f, name.to_string(), a + (b - a) * f));
        }
    };
    let (q1, mid, q3) = (s + (e - s) * 0.25, s + (e - s) * 0.5, s + (e - s) * 0.75);
    // Where each deck's level comes from, before and after.
    let level = |c: &mut Vec<Change>, t0: f64, t1: f64, rising: bool| {
        if by_crossfader {
            let (a, b) = if rising {
                (xa, (xa + xb) / 2.0)
            } else {
                ((xa + xb) / 2.0, xb)
            };
            ramp(c, &xf, t0, t1, a, b);
        } else if rising {
            ramp(c, &vol(inc), t0, t1, 0.0, in_level);
        } else {
            ramp(c, &vol(out), t0, t1, out_level, 0.0);
        }
    };
    // The resting state on both sides.
    c.push((s, low(out), out_low));
    c.push((s, filter(out), out_filter));
    c.push((s, filter(inc), in_filter));
    if by_crossfader {
        c.push((s, vol(out), out_level));
        c.push((s, vol(inc), in_level));
    } else {
        c.push((s, xf.clone(), xa));
    }
    match style {
        Style::Cut => {
            c.push((s, low(inc), in_low));
            if by_crossfader {
                c.push((s, xf.clone(), xb));
            } else {
                c.push((s, vol(out), 0.0));
                c.push((s, vol(inc), in_level));
            }
        }
        Style::Smooth | Style::EchoOut => {
            c.push((s, low(inc), in_low));
            level(&mut c, s, mid, true);
            level(&mut c, mid, e, false);
        }
        Style::BassSwap => {
            // In without its bass, the basses swapped on the middle bar.
            c.push((s, low(inc), 0.0));
            level(&mut c, s, q1, true);
            c.push((mid, low(out), 0.0));
            c.push((mid, low(inc), in_low));
            level(&mut c, q3, e, false);
        }
        Style::Filter => {
            // The outgoing track thinned out with a high-pass sweep.
            c.push((s, low(inc), in_low));
            level(&mut c, s, q1, true);
            ramp(&mut c, &filter(out), s, e, out_filter, 0.95);
            level(&mut c, q3, e, false);
        }
    }
    if by_crossfader {
        c.push((e, xf.clone(), xb));
    }
    c.push((e + 1e-3, vol(out), 0.0));
    c.push((e + 2e-3, filter(out), out_filter));
    c.push((e + 2e-3, low(out), out_low));
    // Held until the recorded transition is over too, so its moves past a
    // shorter new one aren't heard; from there on the log plays as it was.
    let hold = e.max(x.end) + 3e-3;
    c.push((hold, vol(out), 0.0));
    c.push((hold, filter(out), out_filter));
    c.push((hold, low(out), out_low));
    c.push((
        hold,
        vol(inc),
        if by_crossfader {
            v(&vol(inc), after, 1.0)
        } else {
            in_level
        },
    ));
    c.push((hold, low(inc), in_low));
    c.push((hold, filter(inc), in_filter));
    c.push((hold, xf.clone(), if by_crossfader { xb } else { xa }));
    c.sort_by(|a, b| a.0.total_cmp(&b.0));
    c
}

mod swap;
pub use swap::*;

#[cfg(test)]
mod tests;
