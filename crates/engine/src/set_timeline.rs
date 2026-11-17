//! Reads a recorded set back as a timeline: which track played on which
//! deck and when, the mixer moves, the transitions between tracks and their
//! style, loops, and hard moves (scratches, backspins, brakes).
//!
//! The source is the control log the recorder writes beside the audio
//! (`<name>.controls.jsonl`, see [`crate::recorder`]): a header, a snapshot
//! of every control, then one line per control change, per track loaded and
//! per deck place (`pos`, once a second and on every jump). The Mixed Set
//! Editor draws this model; editing and the Ableton export read the same
//! one, so the log is parsed only here.
//!
//! Recordings without a log (older ones, or one whose log was deleted) open
//! from the `.cue` sheet instead: [`Recording::open`] gives the tracklist
//! only, with no decks, lanes or transitions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::controls::{
    deck_group, effect_group, effect_unit_group, eq_group, quick_effect_group, EFFECTS_PER_UNIT,
    NUM_DECKS, NUM_EFFECT_UNITS,
};
use crate::effects::AVAILABLE_EFFECTS;
use crate::engine::xfader_gains;

/// A deck counts as audible when its level after fader, crossfader and EQ
/// is above -30 dB of its own.
const AUDIBLE: f64 = 0.031_622_8;
/// Audible stretches of one deck closer than this are one stretch, so a
/// fader wobble doesn't split a transition.
const WOBBLE: f64 = 0.5;
/// A place more than this far from where playing would have taken the deck
/// is a jump (a hot cue, a seek, a loop going round, a scratch).
const JUMP: f64 = 0.15;
/// Jumps closer together than this are one hard move, not hot cues.
const HARD_MOVE_GAP: f64 = 1.5;
/// A jump that lands this close to the loop's start while the loop is on is
/// the loop going round.
const LOOP_SNAP: f64 = 0.15;
/// Transitions shorter than this many bars are cuts.
const CUT_BARS: f64 = 2.0;
/// Without a tempo, transitions shorter than this are cuts (2 bars at 120).
const CUT_SECONDS: f64 = 4.0;
/// A bass swap: the low EQs differ by at least this much one way, then the
/// other.
const SWAP_MARGIN: f64 = 0.25;
/// A filter move: the filter swept more than this share of its range.
const FILTER_SWEEP: f64 = 1.0 / 3.0;
/// The effects that make a transition an echo out: Echo, Echo Out and
/// Filter Echo (`loaded_effect` numbers count from 1).
const ECHO_EFFECTS: [&str; 3] = ["Echo", "Echo Out", "Filter Echo"];

/// A track loaded on a deck during the set, as the log names it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadedTrack {
    pub deck: usize,
    pub artist: String,
    pub title: String,
    pub path: Option<PathBuf>,
    pub bpm: f64,
    pub key: String,
    /// When it was loaded, in seconds into the recording.
    pub loaded_at: f64,
}

/// A stretch of playback on one deck: from play, a load or a jump to the
/// next jump, stop or load.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// From 0.
    pub deck: usize,
    /// Index into [`SetTimeline::tracks`].
    pub track: usize,
    /// Seconds into the recording.
    pub start: f64,
    pub end: f64,
    /// Seconds into the track at `start`.
    pub track_start: f64,
    /// Playback speed at `start` (1 = the track's own tempo).
    pub speed: f64,
}

impl Clip {
    pub fn len(&self) -> f64 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// What a lane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LaneKind {
    Volume,
    EqLow,
    EqMid,
    EqHigh,
    Filter,
    Gain,
    Crossfader,
    /// An FX unit (from 0) on or off.
    FxUnit(usize),
    /// An effect slot of a unit on or off.
    FxEffect(usize, usize),
    /// A unit's super knob.
    FxSuper(usize),
}

/// A control's values through the set, straight from the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub kind: LaneKind,
    /// The deck (from 0) for per-deck lanes.
    pub deck: Option<usize>,
    /// The log's name for the control, e.g. `[Channel1],volume`.
    pub control: String,
    /// (seconds into the recording, value), the first at 0.
    pub points: Vec<(f64, f64)>,
}

/// How a transition was mixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Shorter than 2 bars.
    Cut,
    /// The low EQs crossed: one deck's bass went out as the other's came in.
    BassSwap,
    /// A filter moved more than a third of its range.
    Filter,
    /// An echo or delay on the outgoing deck at the end.
    EchoOut,
    /// Volume and crossfader only.
    Smooth,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Cut => "Cut",
            Style::BassSwap => "Bass swap",
            Style::Filter => "Filter",
            Style::EchoOut => "Echo out",
            Style::Smooth => "Smooth",
        }
    }
}

/// Two decks audible at once: from when the incoming one became audible to
/// when the outgoing one stopped being audible.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub from_deck: usize,
    pub to_deck: usize,
    /// Indexes into [`SetTimeline::tracks`].
    pub from_track: usize,
    pub to_track: usize,
    pub start: f64,
    pub end: f64,
    /// Length in bars at the outgoing deck's tempo, when it has one.
    pub bars: Option<f64>,
    pub style: Style,
    /// Where each track was when the incoming one became audible, in
    /// seconds into the track: the pair's out and in points.
    pub out_point: f64,
    pub in_point: f64,
}

impl Transition {
    pub fn len(&self) -> f64 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// A loop from loop on to loop off.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopMark {
    pub deck: usize,
    pub track: usize,
    /// Seconds into the recording.
    pub start: f64,
    pub end: f64,
    /// The loop's bounds, in seconds into the track.
    pub loop_start: f64,
    pub loop_end: f64,
    /// Times the loop went round.
    pub repeats: u32,
}

/// A stretch where a deck's place didn't follow play, tempo and jumps: a
/// scratch, a backspin, a brake.
#[derive(Clone, Debug, PartialEq)]
pub struct HardMove {
    pub deck: usize,
    pub track: Option<usize>,
    pub start: f64,
    pub end: f64,
}

/// A recorded set read from its control log.
#[derive(Clone, Debug, Default)]
pub struct SetTimeline {
    /// When recording started (RFC 3339), from the log's header.
    pub started: String,
    /// The first audio file's name, from the header.
    pub audio: String,
    /// Seconds recorded.
    pub length: f64,
    pub tracks: Vec<LoadedTrack>,
    /// In order of start.
    pub clips: Vec<Clip>,
    pub lanes: Vec<Lane>,
    /// In order of start.
    pub transitions: Vec<Transition>,
    pub loops: Vec<LoopMark>,
    pub hard_moves: Vec<HardMove>,
    values: Series,
}

/// Every control's (time, value) changes.
#[derive(Clone, Debug, Default)]
struct Series(HashMap<String, Vec<(f64, f64)>>);

impl Series {
    fn push(&mut self, name: &str, t: f64, v: f64) {
        let points = self.0.entry(name.to_string()).or_default();
        match points.last_mut() {
            Some(last) if last.0 == t => last.1 = v,
            Some(last) if last.1 == v => {}
            _ => points.push((t, v)),
        }
    }

    fn at(&self, name: &str, t: f64, default: f64) -> f64 {
        let Some(points) = self.0.get(name) else {
            return default;
        };
        let i = points.partition_point(|p| p.0 <= t);
        if i == 0 {
            default
        } else {
            points[i - 1].1
        }
    }

    /// The values `name` takes in `from..=to`, the one in force at `from`
    /// first.
    fn within(&self, name: &str, from: f64, to: f64, default: f64) -> Vec<f64> {
        let mut out = vec![self.at(name, from, default)];
        if let Some(points) = self.0.get(name) {
            out.extend(
                points
                    .iter()
                    .filter(|p| p.0 > from && p.0 <= to)
                    .map(|p| p.1),
            );
        }
        out
    }

    fn times(&self, name: &str) -> &[(f64, f64)] {
        self.0.get(name).map_or(&[], |v| v.as_slice())
    }
}

fn control(group: &str, item: &str) -> String {
    format!("{group},{item}")
}

/// Where a recording's control log and cue sheet are: beside its first
/// audio file, named as it is without the extension and part number.
pub fn log_paths(audio: &Path) -> (PathBuf, PathBuf) {
    let name = audio
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut stem = match name.rsplit_once('.') {
        Some((stem, ext))
            if ["wav", "flac", "mp3", "aiff", "aif"]
                .contains(&ext.to_ascii_lowercase().as_str()) =>
        {
            stem.to_string()
        }
        _ => name.clone(),
    };
    if let Some(i) = stem.rfind(" (part ") {
        if stem.ends_with(')') && stem[i + 7..stem.len() - 1].parse::<u32>().is_ok() {
            stem.truncate(i);
        }
    }
    (
        audio.with_file_name(format!("{stem}.controls.jsonl")),
        audio.with_file_name(format!("{stem}.cue")),
    )
}

/// A track on a cue sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct CueTrack {
    pub title: String,
    pub performer: String,
    /// Seconds into the recording.
    pub start: f64,
}

/// The tracks of a `.cue` sheet, in order. Only the first FILE's tracks
/// are timed from the recording's start; later files' (parts of a long
/// WAV recording) are offset by nothing here, as their length isn't known.
pub fn read_cue(text: &str) -> Vec<CueTrack> {
    let mut tracks: Vec<CueTrack> = Vec::new();
    let mut in_track = false;
    let unquote = |s: &str| s.trim().trim_matches('"').to_string();
    for line in text.lines() {
        let line = line.trim();
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "TRACK" => {
                in_track = true;
                tracks.push(CueTrack {
                    title: String::new(),
                    performer: String::new(),
                    start: 0.0,
                });
            }
            "TITLE" if in_track => tracks.last_mut().unwrap().title = unquote(rest),
            "PERFORMER" if in_track => tracks.last_mut().unwrap().performer = unquote(rest),
            "INDEX" if in_track => {
                let mut parts = rest.split_whitespace();
                if parts.next() == Some("01") {
                    if let Some(at) = parts.next().and_then(cue_seconds) {
                        tracks.last_mut().unwrap().start = at;
                    }
                }
            }
            _ => {}
        }
    }
    tracks
}

/// `mm:ss:ff` (ff in 75ths) as seconds.
fn cue_seconds(s: &str) -> Option<f64> {
    let mut it = s.split(':').map(|p| p.parse::<u32>().ok());
    let (m, sec, f) = (it.next()??, it.next()??, it.next()??);
    Some(m as f64 * 60.0 + sec as f64 + f as f64 / 75.0)
}

/// A recording as the Mixed Set Editor opens it.
#[derive(Clone, Debug)]
pub enum Recording {
    /// With its control log: the whole timeline.
    Logged(Box<SetTimeline>),
    /// No usable log: the cue sheet's tracklist only (maybe empty).
    CueOnly(Vec<CueTrack>),
}

impl Recording {
    /// Opens the recording whose (first) audio file is `audio`. A log that
    /// is missing or can't be read falls back to the cue sheet.
    pub fn open(audio: &Path) -> Recording {
        let (log, cue) = log_paths(audio);
        if let Ok(text) = std::fs::read_to_string(&log) {
            if let Ok(timeline) = SetTimeline::parse(&text) {
                return Recording::Logged(Box::new(timeline));
            }
        }
        let tracks = std::fs::read_to_string(&cue)
            .map(|t| read_cue(&t))
            .unwrap_or_default();
        Recording::CueOnly(tracks)
    }
}

enum Event {
    Control(String, f64),
    Load(LoadedTrack),
    Pos(usize, f64),
}

/// One deck as the log is read.
#[derive(Default)]
struct DeckState {
    deck: usize,
    track: Option<usize>,
    /// The place last known and when: (recording time, track seconds).
    anchor: Option<(f64, f64)>,
    playing: bool,
    clip: Option<Clip>,
    /// Loop on: when, its bounds, and times round.
    looping: Option<(f64, f64, f64, u32)>,
    jumps: Vec<f64>,
}

impl SetTimeline {
    /// Reads a control log.
    pub fn parse(text: &str) -> Result<SetTimeline, String> {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header: Value = lines
            .next()
            .and_then(|l| serde_json::from_str(l).ok())
            .ok_or("empty log")?;
        if header.get("opendj_recording").is_none() {
            return Err("not an openDJ control log".into());
        }
        let mut timeline = SetTimeline {
            started: header["started"].as_str().unwrap_or_default().to_string(),
            audio: header["audio"].as_str().unwrap_or_default().to_string(),
            ..Default::default()
        };
        let mut events: Vec<(f64, Event)> = Vec::new();
        let mut length: f64 = 0.0;
        for line in lines {
            // A log cut short by a crash can end in half a line.
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let t = v["t"].as_f64().unwrap_or(0.0);
            length = length.max(t);
            if let Some(snapshot) = v["snapshot"].as_object() {
                for (name, value) in snapshot {
                    if let Some(value) = value.as_f64() {
                        timeline.values.push(name, 0.0, value);
                    }
                }
            } else if let (Some(c), Some(value)) = (v["c"].as_str(), v["v"].as_f64()) {
                events.push((t, Event::Control(c.to_string(), value)));
            } else if let Some(l) = v.get("load") {
                let deck = l["deck"].as_u64().unwrap_or(1).max(1) as usize - 1;
                events.push((
                    t,
                    Event::Load(LoadedTrack {
                        deck,
                        artist: l["artist"].as_str().unwrap_or_default().to_string(),
                        title: l["title"].as_str().unwrap_or_default().to_string(),
                        path: l["path"].as_str().map(PathBuf::from),
                        bpm: l["bpm"].as_f64().unwrap_or(0.0),
                        key: l["key"].as_str().unwrap_or_default().to_string(),
                        loaded_at: t,
                    }),
                ));
            } else if let Some(p) = v.get("pos") {
                let deck = p["deck"].as_u64().unwrap_or(1).max(1) as usize - 1;
                if let Some(s) = p["s"].as_f64() {
                    events.push((t, Event::Pos(deck, s)));
                }
            }
        }
        timeline.length = length;
        // Lines are written in time order; keep it stable if they aren't.
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        timeline.read_events(events);
        timeline.merge_hard_moves();
        timeline.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
        timeline.find_transitions();
        timeline.build_lanes();
        Ok(timeline)
    }

    /// Reads the control log at `path`.
    pub fn read(path: &Path) -> Result<SetTimeline, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        SetTimeline::parse(&text)
    }

    /// A control's value at `t` seconds into the recording, e.g.
    /// `value_at("[Channel1],volume", 30.0)`.
    pub fn value_at(&self, control: &str, t: f64) -> Option<f64> {
        self.values
            .0
            .contains_key(control)
            .then(|| self.values.at(control, t, 0.0))
    }

    /// Decks (from 0) that played something.
    pub fn decks(&self) -> Vec<usize> {
        let mut decks: Vec<usize> = self.clips.iter().map(|c| c.deck).collect();
        decks.sort_unstable();
        decks.dedup();
        decks
    }

    /// The deck's speed at `t` (1 = the track's own tempo), negative in
    /// reverse.
    fn speed(&self, deck: usize, t: f64) -> f64 {
        let g = deck_group(deck);
        let v = |item: &str, d: f64| self.values.at(&control(&g, item), t, d);
        let dir = if v("rate_dir", 1.0) < 0.0 { -1.0 } else { 1.0 };
        let speed = 1.0 + v("rate", 0.0) * v("rateRange", 0.08) * dir;
        if v("reverse", 0.0) > 0.5 {
            -speed
        } else {
            speed
        }
    }

    fn read_events(&mut self, events: Vec<(f64, Event)>) {
        let mut decks: Vec<DeckState> = (0..NUM_DECKS)
            .map(|deck| DeckState {
                deck,
                ..Default::default()
            })
            .collect();
        // The snapshot says which decks were already playing.
        for (d, deck) in decks.iter_mut().enumerate() {
            deck.playing = self.values.at(&control(&deck_group(d), "play"), 0.0, 0.0) > 0.5;
        }
        let deck_of = |name: &str| -> Option<(usize, String)> {
            let rest = name.strip_prefix("[Channel")?;
            let (n, item) = rest.split_once("],")?;
            let n: usize = n.parse().ok()?;
            (1..=NUM_DECKS)
                .contains(&n)
                .then(|| (n - 1, item.to_string()))
        };
        for (t, event) in events {
            match event {
                Event::Control(name, value) => {
                    let deck = deck_of(&name);
                    if let Some((d, item)) = &deck {
                        let d = *d;
                        if matches!(item.as_str(), "rate" | "rateRange" | "rate_dir" | "reverse") {
                            // Move the anchor up to now at the old speed.
                            let speed = self.speed(d, t);
                            let st = &mut decks[d];
                            if st.playing {
                                if let Some((at, s)) = st.anchor {
                                    st.anchor = Some((t, s + (t - at) * speed));
                                }
                            }
                        }
                    }
                    self.values.push(&name, t, value);
                    let Some((d, item)) = deck else { continue };
                    match item.as_str() {
                        "play" => {
                            let on = value > 0.5;
                            let speed = self.speed(d, t);
                            let st = &mut decks[d];
                            if on && !st.playing {
                                st.playing = true;
                                if let Some(track) = st.track {
                                    let place = st.anchor.map(|a| a.1);
                                    st.anchor = place.map(|s| (t, s));
                                    st.clip = Some(Clip {
                                        deck: d,
                                        track,
                                        start: t,
                                        end: t,
                                        track_start: place.unwrap_or(f64::NAN),
                                        speed,
                                    });
                                }
                            } else if !on && st.playing {
                                if let Some((at, s)) = st.anchor {
                                    st.anchor = Some((t, s + (t - at) * speed));
                                }
                                st.playing = false;
                                close_clip(&mut self.clips, st, t);
                            }
                        }
                        "loop_enabled" => {
                            let g = deck_group(d);
                            let st = &mut decks[d];
                            if value > 0.5 && st.looping.is_none() {
                                let start =
                                    self.values.at(&control(&g, "loop_start_position"), t, 0.0);
                                let end = self.values.at(&control(&g, "loop_end_position"), t, 0.0);
                                st.looping = Some((t, start, end, 0));
                            } else if value <= 0.5 {
                                end_loop(&mut self.loops, st, t);
                            }
                        }
                        "loop_start_position" | "loop_end_position" => {
                            if let Some(l) = &mut decks[d].looping {
                                if item == "loop_start_position" {
                                    l.1 = value;
                                } else {
                                    l.2 = value;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::Load(track) => {
                    let d = track.deck;
                    if d >= NUM_DECKS {
                        continue;
                    }
                    self.tracks.push(track);
                    let st = &mut decks[d];
                    close_clip(&mut self.clips, st, t);
                    end_loop(&mut self.loops, st, t);
                    st.track = Some(self.tracks.len() - 1);
                    st.anchor = None;
                    if st.playing {
                        st.clip = Some(Clip {
                            deck: d,
                            track: self.tracks.len() - 1,
                            start: t,
                            end: t,
                            track_start: f64::NAN,
                            speed: self.speed(d, t),
                        });
                    }
                }
                Event::Pos(d, s) => {
                    if d >= NUM_DECKS {
                        continue;
                    }
                    let speed = self.speed(d, t);
                    let st = &mut decks[d];
                    let expected = st
                        .anchor
                        .map(|(at, a)| if st.playing { a + (t - at) * speed } else { a });
                    st.anchor = Some((t, s));
                    let Some(expected) = expected else {
                        if let Some(clip) = &mut st.clip {
                            if clip.track_start.is_nan() {
                                clip.track_start = s - (t - clip.start) * speed;
                            }
                        }
                        continue;
                    };
                    if (s - expected).abs() <= JUMP {
                        continue;
                    }
                    if let Some(l) = &mut st.looping {
                        if (s - l.1).abs() <= LOOP_SNAP && expected >= l.2 - LOOP_SNAP {
                            l.3 += 1;
                            continue;
                        }
                    }
                    st.jumps.push(t);
                    if st.playing {
                        close_clip(&mut self.clips, st, t);
                        if let Some(track) = st.track {
                            st.clip = Some(Clip {
                                deck: d,
                                track,
                                start: t,
                                end: t,
                                track_start: s,
                                speed,
                            });
                        }
                    } else if let Some(clip) = &mut st.clip {
                        clip.track_start = s;
                    }
                }
            }
        }
        let end = self.length;
        for (d, st) in decks.iter_mut().enumerate() {
            close_clip(&mut self.clips, st, end);
            end_loop(&mut self.loops, st, end);
            // Jumps close together are a hard move; one on its own is a
            // hot cue or a seek.
            let mut i = 0;
            while i < st.jumps.len() {
                let mut j = i;
                while j + 1 < st.jumps.len() && st.jumps[j + 1] - st.jumps[j] <= HARD_MOVE_GAP {
                    j += 1;
                }
                if j > i {
                    self.hard_moves.push(HardMove {
                        deck: d,
                        track: None,
                        start: st.jumps[i],
                        end: st.jumps[j],
                    });
                }
                i = j + 1;
            }
            // Scratching, reverse and backspins switched on in the log.
            let g = deck_group(d);
            for item in ["scratch2_enable", "scrub_enable", "reverse", "reverseroll"] {
                let mut on: Option<f64> = None;
                for &(t, v) in self.values.times(&control(&g, item)) {
                    match (on, v > 0.5) {
                        (None, true) => on = Some(t),
                        (Some(from), false) => {
                            self.hard_moves.push(HardMove {
                                deck: d,
                                track: None,
                                start: from,
                                end: t,
                            });
                            on = None;
                        }
                        _ => {}
                    }
                }
                if let Some(from) = on {
                    self.hard_moves.push(HardMove {
                        deck: d,
                        track: None,
                        start: from,
                        end,
                    });
                }
            }
        }
        for clip in &mut self.clips {
            if clip.track_start.is_nan() {
                clip.track_start = 0.0;
            }
        }
    }

    /// Joins overlapping hard moves on a deck, names their track, and joins
    /// the clips a hard move cut into pieces: a scratch is part of the clip
    /// it happened in.
    fn merge_hard_moves(&mut self) {
        self.hard_moves
            .sort_by(|a, b| (a.deck, a.start).partial_cmp(&(b.deck, b.start)).unwrap());
        let mut merged: Vec<HardMove> = Vec::new();
        for m in self.hard_moves.drain(..) {
            match merged.last_mut() {
                Some(last) if last.deck == m.deck && m.start <= last.end + HARD_MOVE_GAP => {
                    last.end = last.end.max(m.end);
                }
                _ => merged.push(m),
            }
        }
        self.clips
            .sort_by(|a, b| (a.deck, a.start).partial_cmp(&(b.deck, b.start)).unwrap());
        let mut clips: Vec<Clip> = Vec::new();
        for c in self.clips.drain(..) {
            if let Some(last) = clips.last_mut() {
                let joined = last.deck == c.deck
                    && last.track == c.track
                    && (last.end - c.start).abs() < 1e-6
                    && merged.iter().any(|m| {
                        m.deck == c.deck && m.start - 1e-6 <= c.start && c.start <= m.end + 1e-6
                    });
                if joined {
                    last.end = c.end;
                    continue;
                }
            }
            clips.push(c);
        }
        for m in &mut merged {
            m.track = clips
                .iter()
                .find(|c| c.deck == m.deck && c.start <= m.end && m.start <= c.end)
                .map(|c| c.track);
        }
        merged.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.hard_moves = merged;
        self.clips = clips;
    }

    /// The deck's level at `t` after fader, crossfader and EQ (1 = its own
    /// level), not counting whether it plays.
    fn level(&self, deck: usize, t: f64) -> f64 {
        let g = deck_group(deck);
        let v = |name: String, d: f64| self.values.at(&name, t, d);
        let volume = v(control(&g, "volume"), 1.0);
        let xfader = if v(control("[Mixer]", "crossfader_enabled"), 1.0) < 0.5 {
            1.0
        } else {
            let orientation = v(
                control(&g, "orientation"),
                if deck.is_multiple_of(2) { 0.0 } else { 2.0 },
            );
            let mut x = v(control("[Master]", "crossfader"), 0.0);
            if v(control("[Mixer Profile]", "xFaderReverse"), 0.0) > 0.5 {
                x = -x;
            }
            let gains = xfader_gains(
                x as f32,
                v(control("[Mixer Profile]", "xFaderShape"), 0.0).round() as u8,
                v(control("[Mixer Profile]", "xFaderCutIn"), 0.02) as f32,
            );
            match orientation.round() as i32 {
                0 => gains[0] as f64,
                2 => gains[1] as f64,
                _ => 1.0,
            }
        };
        let eqg = eq_group(deck);
        let eq: f64 = (1..=3)
            .map(|n| {
                if v(control(&eqg, &format!("button_parameter{n}")), 0.0) > 0.5 {
                    0.0
                } else {
                    v(control(&eqg, &format!("parameter{n}")), 1.0).clamp(0.0, 1.0)
                }
            })
            .sum::<f64>()
            / 3.0;
        volume * xfader * eq
    }

    /// Each deck's audible stretches, wobbles joined.
    fn audible(&self) -> Vec<Vec<(f64, f64)>> {
        let mut times: Vec<f64> = vec![0.0, self.length];
        for points in self.values.0.values() {
            times.extend(points.iter().map(|p| p.0));
        }
        for c in &self.clips {
            times.push(c.start);
            times.push(c.end);
        }
        times.retain(|t| *t >= 0.0 && *t <= self.length);
        times.sort_by(f64::total_cmp);
        times.dedup();
        let mut runs: Vec<Vec<(f64, f64)>> = vec![Vec::new(); NUM_DECKS];
        for (d, deck_runs) in runs.iter_mut().enumerate() {
            let clips: Vec<&Clip> = self.clips.iter().filter(|c| c.deck == d).collect();
            if clips.is_empty() {
                continue;
            }
            let mut on: Option<f64> = None;
            for w in times.windows(2) {
                let (a, b) = (w[0], w[1]);
                let mid = (a + b) * 0.5;
                let playing = clips.iter().any(|c| c.start <= mid && mid < c.end);
                let audible = playing && self.level(d, a) > AUDIBLE;
                match (on, audible) {
                    (None, true) => on = Some(a),
                    (Some(from), false) => {
                        push_run(deck_runs, from, a);
                        on = None;
                    }
                    _ => {}
                }
            }
            if let Some(from) = on {
                push_run(deck_runs, from, self.length);
            }
        }
        runs
    }

    /// Whether `deck` is heard at `t`: playing, with its fader, crossfader
    /// and EQ above -30 dB.
    pub fn audible_at(&self, deck: usize, t: f64) -> bool {
        self.clips
            .iter()
            .any(|c| c.deck == deck && c.start <= t && t < c.end)
            && self.level(deck, t) > AUDIBLE
    }

    /// The clip playing on `deck` at `t`.
    pub fn clip_at(&self, deck: usize, t: f64) -> Option<&Clip> {
        self.clips
            .iter()
            .rfind(|c| c.deck == deck && c.start <= t + 1e-6 && t < c.end + 1e-6)
    }

    fn find_transitions(&mut self) {
        let runs = self.audible();
        let mut found = Vec::new();
        for (to, to_runs) in runs.iter().enumerate() {
            for &(start, to_end) in to_runs {
                // The outgoing deck: audible before and after `start`, and
                // stopping before the incoming one does. The one audible
                // longest wins on four decks.
                let out = runs
                    .iter()
                    .enumerate()
                    .filter(|(from, _)| *from != to)
                    .flat_map(|(from, r)| r.iter().map(move |&(s, e)| (from, s, e)))
                    .filter(|&(_, s, e)| s < start && start < e && e < to_end)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let Some((from, _, end)) = out else { continue };
                let (Some(out_clip), Some(in_clip)) =
                    (self.clip_at(from, start), self.clip_at(to, start))
                else {
                    continue;
                };
                // The outgoing track may change between start and end only
                // if the DJ loaded over it; take the one at the start.
                let out_point = out_clip.track_start + (start - out_clip.start) * out_clip.speed;
                let in_point = in_clip.track_start + (start - in_clip.start) * in_clip.speed;
                let (from_track, to_track) = (out_clip.track, in_clip.track);
                if from_track == to_track {
                    continue;
                }
                let tempo = [(from, from_track), (to, to_track)]
                    .into_iter()
                    .map(|(d, tr)| self.tracks[tr].bpm * self.speed(d, start).abs())
                    .find(|bpm| *bpm > 0.0);
                let bars = tempo.map(|bpm| (end - start) * bpm / 240.0);
                let style = self.style(from, to, start, end, bars);
                found.push(Transition {
                    from_deck: from,
                    to_deck: to,
                    from_track,
                    to_track,
                    start,
                    end,
                    bars,
                    style,
                    out_point,
                    in_point,
                });
            }
        }
        found.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.transitions = found;
    }

    fn style(&self, from: usize, to: usize, start: f64, end: f64, bars: Option<f64>) -> Style {
        let is_cut = match bars {
            Some(b) => b < CUT_BARS,
            None => end - start < CUT_SECONDS,
        };
        if is_cut {
            return Style::Cut;
        }
        if self.echo_on(from, end) {
            return Style::EchoOut;
        }
        // Bass swap: sample the low EQs at every change inside.
        let low = |d: usize| control(&eq_group(d), "parameter1");
        let kill = |d: usize| control(&eq_group(d), "button_parameter1");
        let mut times: Vec<f64> = vec![start, end];
        for d in [from, to] {
            for name in [low(d), kill(d)] {
                times.extend(
                    self.values
                        .times(&name)
                        .iter()
                        .map(|p| p.0)
                        .filter(|t| *t > start && *t <= end),
                );
            }
        }
        times.sort_by(f64::total_cmp);
        let low_at = |d: usize, t: f64| {
            if self.values.at(&kill(d), t, 0.0) > 0.5 {
                0.0
            } else {
                self.values.at(&low(d), t, 1.0).min(1.0)
            }
        };
        let mut out_ahead = false;
        let mut swapped = false;
        for &t in &times {
            let diff = low_at(from, t) - low_at(to, t);
            if diff >= SWAP_MARGIN {
                out_ahead = true;
            } else if diff <= -SWAP_MARGIN && out_ahead {
                swapped = true;
            }
        }
        if swapped {
            return Style::BassSwap;
        }
        for d in [from, to] {
            let values =
                self.values
                    .within(&control(&quick_effect_group(d), "super1"), start, end, 0.5);
            let (lo, hi) = values
                .iter()
                .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            if hi - lo > FILTER_SWEEP {
                return Style::Filter;
            }
        }
        Style::Smooth
    }

    /// Whether an echo or delay was on `deck` just before `t`.
    fn echo_on(&self, deck: usize, t: f64) -> bool {
        let t = (t - 0.05).max(0.0);
        let route = format!("group_{}_enable", deck_group(deck));
        (0..NUM_EFFECT_UNITS).any(|u| {
            let ug = effect_unit_group(u);
            self.values.at(&control(&ug, "enabled"), t, 1.0) > 0.5
                && self.values.at(&control(&ug, &route), t, 0.0) > 0.5
                && (0..EFFECTS_PER_UNIT).any(|s| {
                    let eg = effect_group(u, s);
                    let effect = self
                        .values
                        .at(&control(&eg, "loaded_effect"), t, 0.0)
                        .round() as usize;
                    self.values.at(&control(&eg, "enabled"), t, 0.0) > 0.5
                        && effect >= 1
                        && AVAILABLE_EFFECTS
                            .get(effect - 1)
                            .is_some_and(|name| ECHO_EFFECTS.contains(name))
                })
        })
    }

    fn build_lanes(&mut self) {
        let mut lanes = Vec::new();
        let lane = |kind, deck, name: String, values: &Series| Lane {
            kind,
            deck,
            points: values.times(&name).to_vec(),
            control: name,
        };
        for d in self.decks() {
            let g = deck_group(d);
            let eqg = eq_group(d);
            for (kind, name) in [
                (LaneKind::Volume, control(&g, "volume")),
                (LaneKind::EqLow, control(&eqg, "parameter1")),
                (LaneKind::EqMid, control(&eqg, "parameter2")),
                (LaneKind::EqHigh, control(&eqg, "parameter3")),
                (LaneKind::Filter, control(&quick_effect_group(d), "super1")),
                (LaneKind::Gain, control(&g, "pregain")),
            ] {
                lanes.push(lane(kind, Some(d), name, &self.values));
            }
        }
        lanes.push(lane(
            LaneKind::Crossfader,
            None,
            control("[Master]", "crossfader"),
            &self.values,
        ));
        // FX lanes only for units that moved during the set.
        for u in 0..NUM_EFFECT_UNITS {
            let ug = effect_unit_group(u);
            let mut unit_lanes = vec![
                lane(
                    LaneKind::FxUnit(u),
                    None,
                    control(&ug, "enabled"),
                    &self.values,
                ),
                lane(
                    LaneKind::FxSuper(u),
                    None,
                    control(&ug, "super1"),
                    &self.values,
                ),
            ];
            for s in 0..EFFECTS_PER_UNIT {
                unit_lanes.push(lane(
                    LaneKind::FxEffect(u, s),
                    None,
                    control(&effect_group(u, s), "enabled"),
                    &self.values,
                ));
            }
            if unit_lanes.iter().any(|l| l.points.len() > 1) {
                lanes.extend(unit_lanes);
            }
        }
        self.lanes = lanes;
    }
}

fn push_run(runs: &mut Vec<(f64, f64)>, from: f64, to: f64) {
    match runs.last_mut() {
        Some(last) if from - last.1 < WOBBLE => last.1 = to,
        _ => runs.push((from, to)),
    }
}

fn close_clip(clips: &mut Vec<Clip>, st: &mut DeckState, t: f64) {
    if let Some(mut clip) = st.clip.take() {
        clip.end = t;
        if clip.end > clip.start {
            clips.push(clip);
        }
    }
}

fn end_loop(loops: &mut Vec<LoopMark>, st: &mut DeckState, t: f64) {
    if let (Some((from, start, end, repeats)), Some(track)) = (st.looping.take(), st.track) {
        loops.push(LoopMark {
            deck: st.deck,
            track,
            start: from,
            end: t,
            loop_start: start,
            loop_end: end,
            repeats,
        });
    }
}

#[cfg(test)]
pub(crate) mod tests;
