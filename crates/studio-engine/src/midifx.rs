//! Live's MIDI effects: they sit in a track's chain before its instrument
//! and change the notes on their way to it (the clips' notes and those
//! played live). Arpeggiator, Chord, Scale, Note Length, Velocity, Random,
//! Pitch and Note Echo.
//!
//! Each works on a block's notes in beats: the notes due in `lo..hi`, in
//! order, in; the notes it plays in `lo..hi`, in order, out. Notes it
//! plays later (an echo, an arpeggio's next step, a shortened note's end)
//! wait in a fixed-size queue, so nothing allocates on the audio thread.
//! A note an effect moved (to another pitch) ends where it was moved to,
//! however the knobs have changed since.

use crate::devices::{Param, Unit};
use crate::fx;
use crate::scale::SCALES;

/// A note on its way to the instrument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Note {
    /// Pitch and velocity (0..1).
    On(u8, f32),
    Off(u8),
    /// Every note ends (a clip switch, the transport stopping).
    AllOff,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    pub beat: f64,
    pub note: Note,
}

/// Adds `e` to `out` while it has room; a block never grows it.
#[inline]
pub fn push(out: &mut Vec<NoteEvent>, beat: f64, note: Note) {
    if out.len() < out.capacity() {
        out.push(NoteEvent { beat, note });
    }
}

/// Notes an effect plays later, kept in a fixed array.
#[derive(Clone)]
struct Later {
    items: Vec<NoteEvent>,
}

/// How many notes an effect can hold for later.
const LATER: usize = 512;

impl Later {
    fn new() -> Later {
        Later {
            items: Vec::with_capacity(LATER),
        }
    }

    fn add(&mut self, beat: f64, note: Note) {
        push(&mut self.items, beat, note);
    }

    /// The earliest note due before `beat`, taken out.
    fn next_before(&mut self, beat: f64) -> Option<NoteEvent> {
        let (i, _) = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, e)| e.beat < beat)
            .min_by(|a, b| a.1.beat.total_cmp(&b.1.beat))?;
        Some(self.items.swap_remove(i))
    }

    /// Drops a waiting note-off for `pitch`, saying whether there was one.
    fn cancel_off(&mut self, pitch: u8) -> bool {
        match self.items.iter().position(|e| e.note == Note::Off(pitch)) {
            Some(i) => {
                self.items.swap_remove(i);
                true
            }
            None => false,
        }
    }

    fn clear(&mut self) {
        self.items.clear();
    }
}

/// Counts the notes sounding on each pitch, so two notes landing on one
/// pitch (a chord over a scale) start it once and end it when both end.
#[derive(Clone)]
struct Voices {
    count: [u8; 128],
}

impl Voices {
    fn new() -> Voices {
        Voices { count: [0; 128] }
    }

    fn on(&mut self, out: &mut Vec<NoteEvent>, beat: f64, pitch: i32, v: f32) {
        let Ok(p) = u8::try_from(pitch) else { return };
        if p > 127 {
            return;
        }
        // Played again while sounding: end it first, so it starts afresh.
        if self.count[p as usize] > 0 {
            push(out, beat, Note::Off(p));
        }
        self.count[p as usize] = self.count[p as usize].saturating_add(1);
        push(out, beat, Note::On(p, v));
    }

    fn off(&mut self, out: &mut Vec<NoteEvent>, beat: f64, pitch: i32) {
        let Ok(p) = u8::try_from(pitch) else { return };
        if p > 127 || self.count[p as usize] == 0 {
            return;
        }
        self.count[p as usize] -= 1;
        if self.count[p as usize] == 0 {
            push(out, beat, Note::Off(p));
        }
    }

    fn clear(&mut self) {
        self.count = [0; 128];
    }
}

/// A small, fast random source.
#[derive(Clone)]
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32
    }
}

pub const ARP_STYLES: &[&str] = &["Up", "Down", "Up Down", "Down Up", "Played", "Random"];
const SIGNS: &[&str] = &["Add", "Sub", "Bi"];
const LENGTH_MODES: &[&str] = &["Time", "Sync"];

/// The scale names, for the Scale effect's switch.
static SCALE_NAMES: [&str; SCALES.len()] = {
    let mut n = [""; SCALES.len()];
    let mut i = 0;
    while i < SCALES.len() {
        n[i] = SCALES[i].name;
        i += 1;
    }
    n
};

/// What a MIDI effect does and its settings, as a set saves them.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MidiSettings {
    Arpeggiator {
        /// An index into [`ARP_STYLES`].
        style: u8,
        /// Beats per step.
        rate: f64,
        /// How long each step's note lasts, as a share of the step.
        gate: f32,
        /// Octaves the pattern climbs (1..4).
        octaves: u8,
    },
    /// Up to six notes added at these intervals (0 adds none).
    Chord {
        shifts: [i8; 6],
    },
    /// Each note moved to the scale (down to the nearest degree).
    Scale {
        root: u8,
        scale: u8,
    },
    NoteLength {
        sync: bool,
        ms: f32,
        beats: f64,
    },
    Velocity {
        /// -1..1: below 0 softens the middle, above 0 hardens it.
        drive: f32,
        out_low: f32,
        out_high: f32,
        /// 0..1: how far each velocity wanders.
        random: f32,
    },
    Random {
        /// 0..1: how often a note moves.
        chance: f32,
        /// How many steps it can move (1..24).
        choices: u8,
        /// Semitones per step.
        interval: u8,
        /// An index into `SIGNS`.
        sign: u8,
    },
    Pitch {
        shift: i8,
    },
    NoteEcho {
        /// Beats between echoes.
        delay: f64,
        /// 0..0.95: each echo's velocity against the one before.
        feedback: f32,
        /// Semitones each echo moves.
        pitch: i8,
    },
}

impl MidiSettings {
    /// Every MIDI effect, fresh, by Live's name.
    pub fn catalog() -> Vec<(&'static str, MidiSettings)> {
        vec![
            (
                "Arpeggiator",
                MidiSettings::Arpeggiator {
                    style: 0,
                    rate: 0.25,
                    gate: 0.5,
                    octaves: 1,
                },
            ),
            (
                "Chord",
                MidiSettings::Chord {
                    shifts: [4, 7, 0, 0, 0, 0],
                },
            ),
            ("Scale", MidiSettings::Scale { root: 0, scale: 0 }),
            (
                "Note Length",
                MidiSettings::NoteLength {
                    sync: true,
                    ms: 100.0,
                    beats: 0.25,
                },
            ),
            (
                "Velocity",
                MidiSettings::Velocity {
                    drive: 0.0,
                    out_low: 0.0,
                    out_high: 1.0,
                    random: 0.0,
                },
            ),
            (
                "Random",
                MidiSettings::Random {
                    chance: 0.5,
                    choices: 1,
                    interval: 12,
                    sign: 2,
                },
            ),
            ("Pitch", MidiSettings::Pitch { shift: 0 }),
            (
                "Note Echo",
                MidiSettings::NoteEcho {
                    delay: 0.5,
                    feedback: 0.5,
                    pitch: 0,
                },
            ),
        ]
    }

    pub fn summary(&self) -> String {
        match self {
            MidiSettings::Arpeggiator {
                style,
                rate,
                octaves,
                ..
            } => format!(
                "{} · {} · {} oct",
                ARP_STYLES[(*style as usize).min(ARP_STYLES.len() - 1)],
                fx::NOTE_NAMES[fx::nearest_note(*rate)],
                octaves
            ),
            MidiSettings::Chord { shifts } => {
                let s: Vec<String> = shifts
                    .iter()
                    .filter(|s| **s != 0)
                    .map(|s| format!("{s:+}"))
                    .collect();
                if s.is_empty() {
                    "no notes added".into()
                } else {
                    s.join(" ")
                }
            }
            MidiSettings::Scale { root, scale } => format!(
                "{} {}",
                crate::scale::NOTE_NAMES[(*root as usize) % 12],
                SCALES[(*scale as usize).min(SCALES.len() - 1)].name
            ),
            MidiSettings::NoteLength { sync, ms, beats } => {
                if *sync {
                    fx::NOTE_NAMES[fx::nearest_note(*beats)].into()
                } else {
                    format!("{ms:.0} ms")
                }
            }
            MidiSettings::Velocity {
                out_low, out_high, ..
            } => format!("{:.0}..{:.0}", out_low * 127.0, out_high * 127.0),
            MidiSettings::Random { chance, .. } => format!("{:.0}% chance", chance * 100.0),
            MidiSettings::Pitch { shift } => format!("{shift:+} st"),
            MidiSettings::NoteEcho {
                delay, feedback, ..
            } => format!(
                "{} · {:.0}% feedback",
                fx::NOTE_NAMES[fx::nearest_note(*delay)],
                feedback * 100.0
            ),
        }
    }

    pub fn params(&self) -> Vec<Param> {
        let st = |name: &str, v: i8, range: f32| {
            Param::knob(name, v as f32, -range, range, Unit::Semitones).bipolar()
        };
        match self {
            MidiSettings::Arpeggiator {
                style,
                rate,
                gate,
                octaves,
            } => vec![
                Param::choice("Style", *style as usize, ARP_STYLES),
                Param::notes("Rate", *rate),
                Param::knob("Gate", *gate, 0.05, 2.0, Unit::Percent),
                Param::knob("Octaves", *octaves as f32, 1.0, 4.0, Unit::Count),
            ],
            MidiSettings::Chord { shifts } => shifts
                .iter()
                .enumerate()
                .map(|(i, s)| st(&format!("Shift {}", i + 1), *s, 36.0))
                .collect(),
            MidiSettings::Scale { root, scale } => vec![
                Param::choice("Base", *root as usize, &crate::scale::NOTE_NAMES),
                Param::choice("Scale", *scale as usize, &SCALE_NAMES),
            ],
            MidiSettings::NoteLength { sync, ms, beats } => vec![
                Param::choice("Mode", *sync as usize, LENGTH_MODES),
                Param::knob("Length", *ms, 1.0, 10_000.0, Unit::Ms).log(),
                Param::notes("Length (sync)", *beats),
            ],
            MidiSettings::Velocity {
                drive,
                out_low,
                out_high,
                random,
            } => vec![
                Param::knob("Drive", *drive, -1.0, 1.0, Unit::Percent).bipolar(),
                Param::knob("Out Low", *out_low, 0.0, 1.0, Unit::Percent),
                Param::knob("Out High", *out_high, 0.0, 1.0, Unit::Percent),
                Param::knob("Random", *random, 0.0, 1.0, Unit::Percent),
            ],
            MidiSettings::Random {
                chance,
                choices,
                interval,
                sign,
            } => vec![
                Param::knob("Chance", *chance, 0.0, 1.0, Unit::Percent),
                Param::knob("Choices", *choices as f32, 1.0, 24.0, Unit::Count),
                Param::knob("Interval", *interval as f32, 1.0, 24.0, Unit::Semitones),
                Param::choice("Sign", *sign as usize, SIGNS),
            ],
            MidiSettings::Pitch { shift } => vec![st("Pitch", *shift, 48.0)],
            MidiSettings::NoteEcho {
                delay,
                feedback,
                pitch,
            } => vec![
                Param::notes("Delay", *delay),
                Param::knob("Feedback", *feedback, 0.0, 0.95, Unit::Percent),
                st("Pitch", *pitch, 24.0),
            ],
        }
    }

    /// Changes one setting (see [`MidiSettings::params`]). Never allocates.
    pub fn set(&mut self, i: usize, v: f32) {
        let note = |v: f32| fx::NOTES[(v.round().max(0.0) as usize).min(fx::NOTES.len() - 1)];
        let st = |v: f32, r: f32| v.round().clamp(-r, r) as i8;
        match self {
            MidiSettings::Arpeggiator {
                style,
                rate,
                gate,
                octaves,
            } => match i {
                0 => *style = v.round().clamp(0.0, (ARP_STYLES.len() - 1) as f32) as u8,
                1 => *rate = note(v),
                2 => *gate = v.clamp(0.05, 2.0),
                3 => *octaves = v.round().clamp(1.0, 4.0) as u8,
                _ => {}
            },
            MidiSettings::Chord { shifts } => {
                if let Some(s) = shifts.get_mut(i) {
                    *s = st(v, 36.0);
                }
            }
            MidiSettings::Scale { root, scale } => match i {
                0 => *root = v.round().clamp(0.0, 11.0) as u8,
                1 => *scale = v.round().clamp(0.0, (SCALES.len() - 1) as f32) as u8,
                _ => {}
            },
            MidiSettings::NoteLength { sync, ms, beats } => match i {
                0 => *sync = v >= 0.5,
                1 => *ms = v.clamp(1.0, 10_000.0),
                2 => *beats = note(v),
                _ => {}
            },
            MidiSettings::Velocity {
                drive,
                out_low,
                out_high,
                random,
            } => match i {
                0 => *drive = v.clamp(-1.0, 1.0),
                1 => *out_low = v.clamp(0.0, 1.0),
                2 => *out_high = v.clamp(0.0, 1.0),
                3 => *random = v.clamp(0.0, 1.0),
                _ => {}
            },
            MidiSettings::Random {
                chance,
                choices,
                interval,
                sign,
            } => match i {
                0 => *chance = v.clamp(0.0, 1.0),
                1 => *choices = v.round().clamp(1.0, 24.0) as u8,
                2 => *interval = v.round().clamp(1.0, 24.0) as u8,
                3 => *sign = v.round().clamp(0.0, 2.0) as u8,
                _ => {}
            },
            MidiSettings::Pitch { shift } => {
                if i == 0 {
                    *shift = st(v, 48.0);
                }
            }
            MidiSettings::NoteEcho {
                delay,
                feedback,
                pitch,
            } => match i {
                0 => *delay = note(v),
                1 => *feedback = v.clamp(0.0, 0.95),
                2 => *pitch = st(v, 24.0),
                _ => {}
            },
        }
    }
}

/// The most notes the Arpeggiator holds at once.
const MAX_HELD: usize = 32;

/// A MIDI effect as the audio thread runs it.
pub struct MidiFx {
    pub s: MidiSettings,
    later: Later,
    voices: Voices,
    rng: Rng,
    /// Where each held input pitch went: its output pitches (a chord's
    /// notes), or none.
    moved: [[i16; 7]; 128],
    /// The Arpeggiator's held notes: pitch, velocity, order played.
    held: Vec<(u8, f32, u32)>,
    played: u32,
    step: u64,
    /// The beat its next step plays on, while it holds notes.
    next_step: Option<f64>,
}

/// No note: `moved`'s filler.
const NONE: i16 = i16::MIN;

impl MidiFx {
    pub fn new(s: &MidiSettings) -> MidiFx {
        MidiFx {
            s: s.clone(),
            later: Later::new(),
            voices: Voices::new(),
            rng: Rng(0x9e37_79b9),
            moved: [[NONE; 7]; 128],
            held: Vec::with_capacity(MAX_HELD),
            played: 0,
            step: 0,
            next_step: None,
        }
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
    }

    /// Ends everything it holds and plays (the device bypassed, removed).
    pub fn reset(&mut self) {
        self.later.clear();
        self.voices.clear();
        self.moved = [[NONE; 7]; 128];
        self.held.clear();
        self.next_step = None;
    }

    /// The notes `input` (in order, in `lo..hi`) become, into `out` (in
    /// order, in `lo..hi`), at `bpm` (for Note Length's time mode).
    pub fn run(&mut self, input: &[NoteEvent], out: &mut Vec<NoteEvent>, hi: f64, bpm: f64) {
        for e in input {
            self.until(out, e.beat);
            self.note(out, *e, bpm);
        }
        self.until(out, hi);
    }

    /// Plays what's waiting (and the arpeggio's steps) before `beat`.
    fn until(&mut self, out: &mut Vec<NoteEvent>, beat: f64) {
        loop {
            let step = self.next_step.filter(|s| *s < beat);
            let later = self
                .later
                .items
                .iter()
                .filter(|e| e.beat < beat)
                .map(|e| e.beat)
                .fold(f64::INFINITY, f64::min);
            match step {
                // Note-offs due at the same beat go first, so a step's
                // note can end and start again.
                Some(s) if s < later => self.arp_step(out, s),
                _ if later.is_finite() => {
                    if let Some(e) = self.later.next_before(beat) {
                        self.voices_out(out, e);
                    }
                }
                _ => break,
            }
        }
    }

    /// A note from the queue, through the voice counts.
    fn voices_out(&mut self, out: &mut Vec<NoteEvent>, e: NoteEvent) {
        match e.note {
            Note::On(p, v) => self.voices.on(out, e.beat, p as i32, v),
            Note::Off(p) => self.voices.off(out, e.beat, p as i32),
            Note::AllOff => push(out, e.beat, Note::AllOff),
        }
    }

    fn note(&mut self, out: &mut Vec<NoteEvent>, e: NoteEvent, bpm: f64) {
        let beat = e.beat;
        let (p, v) = match e.note {
            Note::AllOff => {
                self.reset();
                push(out, beat, Note::AllOff);
                return;
            }
            Note::Off(p) => {
                self.off(out, beat, p);
                return;
            }
            Note::On(p, v) => (p, v),
        };
        let mut to = [NONE; 7];
        let mut vel = v;
        match &self.s {
            MidiSettings::Arpeggiator { .. } => {
                if self.held.iter().all(|h| h.0 != p) && self.held.len() < MAX_HELD {
                    self.played += 1;
                    self.held.push((p, v, self.played));
                }
                if self.next_step.is_none() {
                    self.step = 0;
                    self.next_step = Some(beat);
                }
                return;
            }
            MidiSettings::Chord { shifts } => {
                to[0] = p as i16;
                for (t, s) in to[1..].iter_mut().zip(shifts) {
                    if *s != 0 {
                        *t = p as i16 + *s as i16;
                    }
                }
            }
            MidiSettings::Scale { root, scale } => {
                let steps = SCALES[(*scale as usize).min(SCALES.len() - 1)].steps;
                let rel = (p as i16 - *root as i16).rem_euclid(12) as u8;
                // Down to the nearest degree at or below.
                let deg = steps
                    .iter()
                    .rev()
                    .find(|s| **s <= rel)
                    .copied()
                    .unwrap_or(0);
                to[0] = p as i16 - (rel - deg) as i16;
            }
            MidiSettings::NoteLength { sync, ms, beats } => {
                let len = if *sync {
                    *beats
                } else {
                    *ms as f64 / 1000.0 * bpm / 60.0
                };
                // Played again before its set length ran out: end it now.
                if self.later.cancel_off(p) {
                    self.voices.off(out, beat, p as i32);
                }
                self.voices.on(out, beat, p as i32, v);
                self.later.add(beat + len.max(1e-4), Note::Off(p));
                return;
            }
            MidiSettings::Velocity {
                drive,
                out_low,
                out_high,
                random,
            } => {
                let (drive, out_low, out_high, random) = (*drive, *out_low, *out_high, *random);
                // Drive bends the curve; the output range then scales it.
                let curved = v.clamp(0.0, 1.0).powf(2f32.powf(-drive * 2.0));
                let wander = (self.rng.next() * 2.0 - 1.0) * random * 0.5;
                let x = (curved + wander).clamp(0.0, 1.0);
                vel = (out_low + x * (out_high - out_low)).clamp(1.0 / 127.0, 1.0);
                to[0] = p as i16;
            }
            MidiSettings::Random {
                chance,
                choices,
                interval,
                sign,
            } => {
                let (chance, choices, interval, sign) = (*chance, *choices, *interval, *sign);
                to[0] = p as i16;
                if self.rng.next() < chance {
                    let k = 1 + (self.rng.next() * choices as f32).min(choices as f32 - 1.0) as i16;
                    let up = match sign {
                        0 => true,
                        1 => false,
                        _ => self.rng.next() < 0.5,
                    };
                    let d = k * interval as i16;
                    to[0] += if up { d } else { -d };
                }
            }
            MidiSettings::Pitch { shift } => to[0] = p as i16 + *shift as i16,
            MidiSettings::NoteEcho {
                delay,
                feedback,
                pitch,
            } => {
                to[0] = p as i16;
                // Echoes at each delay while they're loud enough to hear,
                // each lasting half the delay.
                let (delay, feedback, pitch) = (delay.max(1e-3), *feedback, *pitch as i16);
                let mut ev = v;
                for k in 1..=16 {
                    ev *= feedback;
                    if ev < 0.05 {
                        break;
                    }
                    let q = p as i16 + pitch * k;
                    if !(0..=127).contains(&q) {
                        break;
                    }
                    let at = beat + delay * k as f64;
                    self.later.add(at, Note::On(q as u8, ev));
                    self.later.add(at + delay * 0.5, Note::Off(q as u8));
                }
            }
        }
        // A pitch played again while held: its earlier notes end first.
        self.off(out, beat, p);
        for t in to {
            if t != NONE {
                self.voices.on(out, beat, t as i32, vel);
            }
        }
        self.moved[p as usize] = to;
    }

    fn off(&mut self, out: &mut Vec<NoteEvent>, beat: f64, p: u8) {
        match self.s {
            MidiSettings::Arpeggiator { .. } => {
                self.held.retain(|h| h.0 != p);
                return;
            }
            // Its length sets when it ends, not the key.
            MidiSettings::NoteLength { .. } => return,
            _ => {}
        }
        let to = std::mem::replace(&mut self.moved[p as usize], [NONE; 7]);
        for t in to {
            if t != NONE {
                self.voices.off(out, beat, t as i32);
            }
        }
    }

    /// The arpeggio's step at `beat`: its note on, and off after the gate.
    fn arp_step(&mut self, out: &mut Vec<NoteEvent>, beat: f64) {
        let MidiSettings::Arpeggiator {
            style,
            rate,
            gate,
            octaves,
        } = self.s
        else {
            self.next_step = None;
            return;
        };
        if self.held.is_empty() {
            self.next_step = None;
            return;
        }
        // The pattern: held notes in pitch (or played) order, over the
        // octaves.
        let mut notes = [(0u8, 0f32, 0u32); MAX_HELD];
        let n = self.held.len();
        notes[..n].copy_from_slice(&self.held);
        let notes = &mut notes[..n];
        if style == 4 {
            notes.sort_unstable_by_key(|h| h.2);
        } else {
            notes.sort_unstable_by_key(|h| h.0);
        }
        let octaves = octaves.max(1) as usize;
        let len = n * octaves;
        let at = |i: usize| {
            let (p, v, _) = notes[i % n];
            (p as i32 + 12 * (i / n) as i32, v)
        };
        let i = self.step as usize;
        let pick = match style {
            1 => len - 1 - i % len,
            2 | 3 if len > 1 => {
                // Up then down (or down then up), the ends played once.
                let cycle = 2 * len - 2;
                let k = i % cycle;
                let up = if k < len { k } else { cycle - k };
                if style == 2 {
                    up
                } else {
                    len - 1 - up
                }
            }
            5 => (self.rng.next() * len as f32) as usize % len,
            _ => i % len,
        };
        let (p, v) = at(pick);
        if (0..=127).contains(&p) {
            self.voices.on(out, beat, p, v);
            self.later.add(
                beat + rate * gate.clamp(0.05, 2.0) as f64,
                Note::Off(p as u8),
            );
        }
        self.step += 1;
        self.next_step = Some(beat + rate.max(1e-3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(beat: f64, p: u8) -> NoteEvent {
        NoteEvent {
            beat,
            note: Note::On(p, 0.8),
        }
    }

    fn off(beat: f64, p: u8) -> NoteEvent {
        NoteEvent {
            beat,
            note: Note::Off(p),
        }
    }

    /// Runs `input` through `s` over `beats`, a beat per block.
    fn play(s: MidiSettings, input: &[NoteEvent], beats: usize) -> Vec<NoteEvent> {
        let mut fx = MidiFx::new(&s);
        let mut all = Vec::new();
        let mut out = Vec::with_capacity(1024);
        for b in 0..beats {
            let (lo, hi) = (b as f64, b as f64 + 1.0);
            let block: Vec<NoteEvent> = input
                .iter()
                .filter(|e| e.beat >= lo && e.beat < hi)
                .copied()
                .collect();
            out.clear();
            fx.run(&block, &mut out, hi, 120.0);
            assert!(
                out.windows(2).all(|w| w[0].beat <= w[1].beat),
                "out of order"
            );
            assert!(
                out.iter().all(|e| e.beat >= lo && e.beat < hi),
                "outside its block"
            );
            all.extend_from_slice(&out);
        }
        all
    }

    fn ons(events: &[NoteEvent]) -> Vec<(f64, u8)> {
        events
            .iter()
            .filter_map(|e| match e.note {
                Note::On(p, _) => Some((e.beat, p)),
                _ => None,
            })
            .collect()
    }

    /// Every note that started ended.
    fn balanced(events: &[NoteEvent]) {
        let mut sounding = [0i32; 128];
        for e in events {
            match e.note {
                Note::On(p, _) => sounding[p as usize] += 1,
                Note::Off(p) => sounding[p as usize] -= 1,
                Note::AllOff => sounding = [0; 128],
            }
        }
        assert!(sounding.iter().all(|c| *c == 0), "notes left hanging");
    }

    #[test]
    fn the_arpeggiator_steps_through_held_notes() {
        let s = MidiSettings::Arpeggiator {
            style: 0,
            rate: 0.25,
            gate: 0.5,
            octaves: 2,
        };
        let input = [on(0.0, 64), on(0.0, 60), off(2.0, 60), off(2.0, 64)];
        let out = play(s, &input, 4);
        let notes: Vec<u8> = ons(&out).iter().map(|n| n.1).collect();
        // Up over two octaves, a 16th each, for two beats.
        assert_eq!(notes, [60, 64, 72, 76, 60, 64, 72, 76]);
        assert_eq!(ons(&out)[1].0, 0.25);
        balanced(&out);

        let down_up = MidiSettings::Arpeggiator {
            style: 2,
            rate: 0.5,
            gate: 1.0,
            octaves: 1,
        };
        let input = [
            on(0.0, 60),
            on(0.0, 64),
            on(0.0, 67),
            off(4.0, 60),
            off(4.0, 64),
            off(4.0, 67),
        ];
        let notes: Vec<u8> = ons(&play(down_up, &input, 5)).iter().map(|n| n.1).collect();
        assert_eq!(notes, [60, 64, 67, 64, 60, 64, 67, 64]);
    }

    #[test]
    fn chord_scale_and_pitch_move_notes_and_end_them_where_they_went() {
        let input = [on(0.0, 61), off(1.5, 61)];
        let chord = play(
            MidiSettings::Chord {
                shifts: [4, 7, 0, 0, 0, 0],
            },
            &input,
            2,
        );
        assert_eq!(
            ons(&chord).iter().map(|n| n.1).collect::<Vec<_>>(),
            [61, 65, 68]
        );
        balanced(&chord);
        // C# isn't in C major: down to C.
        let scale = play(MidiSettings::Scale { root: 0, scale: 0 }, &input, 2);
        assert_eq!(ons(&scale)[0].1, 60);
        balanced(&scale);
        let mut fx = MidiFx::new(&MidiSettings::Pitch { shift: 12 });
        let mut out = Vec::with_capacity(16);
        fx.run(&[on(0.0, 60)], &mut out, 1.0, 120.0);
        // The knob moves while the note holds; it still ends where it went.
        fx.set(0, -5.0);
        fx.run(&[off(1.0, 60)], &mut out, 2.0, 120.0);
        assert_eq!(out[0].note, Note::On(72, 0.8));
        assert_eq!(out[1].note, Note::Off(72));
    }

    #[test]
    fn a_chord_over_a_shared_pitch_starts_and_ends_it_once() {
        // 60 with +7, and 67 played too: G sounds twice and ends when both
        // have ended.
        let input = [on(0.0, 60), on(0.0, 67), off(1.0, 60), off(2.0, 67)];
        let out = play(
            MidiSettings::Chord {
                shifts: [7, 0, 0, 0, 0, 0],
            },
            &input,
            3,
        );
        let g_offs: Vec<f64> = out
            .iter()
            .filter(|e| e.note == Note::Off(67))
            .map(|e| e.beat)
            .collect();
        // Played again while sounding, it restarted (one off at 0), then
        // ended once both had ended.
        assert_eq!(g_offs, [0.0, 2.0]);
    }

    #[test]
    fn note_length_and_echo_play_later_notes_in_later_blocks() {
        let input = [on(0.0, 60), off(3.0, 60)];
        let short = play(
            MidiSettings::NoteLength {
                sync: true,
                ms: 0.0,
                beats: 0.5,
            },
            &input,
            4,
        );
        assert_eq!(short[1], off(0.5, 60));
        assert_eq!(short.len(), 2);
        let echo = play(
            MidiSettings::NoteEcho {
                delay: 1.0,
                feedback: 0.5,
                pitch: 12,
            },
            &[on(0.0, 60), off(0.5, 60)],
            6,
        );
        let o = ons(&echo);
        // Velocity 0.8 halves each time: four echoes stay at 0.05 or up.
        assert_eq!(o, [(0.0, 60), (1.0, 72), (2.0, 84), (3.0, 96), (4.0, 108)]);
        balanced(&echo);
    }

    #[test]
    fn velocity_maps_into_its_range() {
        let s = MidiSettings::Velocity {
            drive: 0.0,
            out_low: 0.5,
            out_high: 0.75,
            random: 0.0,
        };
        let out = play(s, &[on(0.0, 60), off(0.5, 60)], 1);
        let Note::On(_, v) = out[0].note else {
            panic!()
        };
        assert!((v - (0.5 + 0.8 * 0.25)).abs() < 1e-6);
    }

    #[test]
    fn random_moves_some_notes_by_its_interval() {
        let s = MidiSettings::Random {
            chance: 0.5,
            choices: 1,
            interval: 12,
            sign: 0,
        };
        let input: Vec<NoteEvent> = (0..200)
            .flat_map(|i| [on(i as f64 * 0.1, 60), off(i as f64 * 0.1 + 0.05, 60)])
            .collect();
        let out = play(s, &input, 21);
        let moved = ons(&out).iter().filter(|n| n.1 == 72).count();
        assert!((60..140).contains(&moved), "{moved} of 200 moved");
        assert!(ons(&out).iter().all(|n| n.1 == 60 || n.1 == 72));
        balanced(&out);
    }

    #[test]
    fn running_never_allocates_past_its_capacity() {
        for (_, s) in MidiSettings::catalog() {
            let mut fx = MidiFx::new(&s);
            let mut out = Vec::with_capacity(8);
            let input: Vec<NoteEvent> = (0..64).map(|i| on(0.0, 40 + i as u8)).collect();
            fx.run(&input, &mut out, 1.0, 120.0);
            assert!(out.capacity() == 8 && out.len() <= 8);
        }
    }
}
