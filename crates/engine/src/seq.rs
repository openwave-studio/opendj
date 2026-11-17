//! The instrument's step sequencer: a riff of up to 16 steps that plays on
//! its own, in time with the mix's beat and in key, in the room the
//! keyboard leaves when it is hidden.
//!
//! **Lanes.** Each step has a value in every lane, after Pigments'
//! sequencer: `on` (the step plays), `chance` (how likely it is to, 0-1),
//! `pitch` (scale degrees from the scale's root, see [`PITCH_RANGE`]),
//! `vel` (velocity 0-1; the top of the range is the accent), `oct` (-1, 0
//! or +1 octave), `gate` (how much of the step the note sounds; [`TIE`] and
//! up holds it into the next step) and `slide` (the note runs into the
//! next one, overlapping it, so a mono synth glides). Every lane can have
//! its own length (`seqS_LANE_len`, 0 for the pattern's), so lanes of
//! different lengths drift against each other.
//!
//! **Notes.** Pitch is counted in the scale the deck plays in, SCALE's or
//! C minor while SCALE is off, from the root in octave 3 (C3 = 48). The
//! notes go in where keys do: Scale, Chord, Arp and HOLD apply to them.
//!
//! **Time.** Steps fall on the target deck's beat grid while it plays (as
//! the arpeggiator's, see [`crate::arp`]) at `seq_rate` (one of
//! [`crate::arp::RATES`]), every second step `seq_swing` of a step late.
//! Started while the beat moves, it waits for the next bar; otherwise it
//! starts at once on its own clock. `seq_slot` picks one of four patterns
//! (A-D); a change waits for the next bar.
//!
//! **Controls**, in `[Instrument1]`: `seq` (play), `seq_slot`, `seq_mode`
//! ([`MODES`]), `seq_rate`, `seq_swing`; per slot S (1-4): `seqS_len`
//! (1-16), `seqS_LANE_N` (step N's value, N from 1), `seqS_LANE_len`, and
//! the offsets `seqS_pitch_offset` (semitones) and `seqS_vel_offset`.
//! Shown: `seq_position` (the step playing, -1 stopped), `seq_waiting`
//! (started, waiting for the bar) and `seq_slot_playing`.

use opendj_control::{Behavior, ControlHandle, ControlRegistry};
use opendj_plugin_host::{Midi, Transport};

use crate::arp::RATES;

/// Steps in a pattern.
pub const STEPS: usize = 16;
/// Patterns, A to D.
pub const SLOTS: usize = 4;
/// The lanes, as the controls name them.
pub const LANES: [&str; 7] = ["on", "chance", "pitch", "vel", "oct", "gate", "slide"];
pub const ON: usize = 0;
pub const CHANCE: usize = 1;
pub const PITCH: usize = 2;
pub const VEL: usize = 3;
pub const OCT: usize = 4;
pub const GATE: usize = 5;
pub const SLIDE: usize = 6;
/// `seq_mode` values.
pub const MODES: [&str; 4] = ["Forward", "Backward", "Ping-pong", "Random"];
pub const FORWARD: u8 = 0;
pub const BACKWARD: u8 = 1;
pub const PING_PONG: u8 = 2;
pub const RANDOM: u8 = 3;
/// Pitch runs this many scale degrees either side of the root.
pub const PITCH_RANGE: f64 = 7.0;
/// A gate this long or longer ties the note into the next step.
pub const TIE: f64 = 1.0;
/// The note pitch 0 plays with the scale on C: C3.
const BASE: i32 = 48;
/// C minor, for while SCALE is off.
const C_MINOR: u16 = 0b0101_1010_1101;
/// The beats in a bar.
const BAR: f64 = 4.0;

/// Each lane's range and the value its steps start at.
const RANGES: [(f64, f64, f64); 7] = [
    (0.0, 1.0, 0.0),
    (0.0, 1.0, 1.0),
    (-PITCH_RANGE, PITCH_RANGE, 0.0),
    (0.0, 1.0, 0.75),
    (-1.0, 1.0, 0.0),
    (0.05, TIE, 0.5),
    (0.0, 1.0, 0.0),
];

pub fn lane_key(slot: usize, lane: usize, step: usize) -> String {
    format!("seq{}_{}_{}", slot + 1, LANES[lane], step + 1)
}

/// One pattern's controls.
pub struct SlotControls {
    pub len: ControlHandle,
    /// Each lane's steps.
    pub steps: [[ControlHandle; STEPS]; LANES.len()],
    /// Each lane's own length; 0 for the pattern's.
    pub lane_len: [ControlHandle; LANES.len()],
    pub pitch_offset: ControlHandle,
    pub vel_offset: ControlHandle,
}

pub struct SeqControls {
    pub play: ControlHandle,
    pub slot: ControlHandle,
    pub mode: ControlHandle,
    pub rate: ControlHandle,
    pub swing: ControlHandle,
    pub position: ControlHandle,
    pub waiting: ControlHandle,
    pub slot_playing: ControlHandle,
    pub slots: [SlotControls; SLOTS],
}

impl SeqControls {
    pub fn new(reg: &ControlRegistry, g: &str) -> Self {
        let last = (STEPS - 1) as f64;
        let slot = |s: usize| SlotControls {
            len: reg.create(
                g,
                &format!("seq{}_len", s + 1),
                STEPS as f64,
                1.0,
                STEPS as f64,
                Behavior::Free,
            ),
            steps: std::array::from_fn(|lane| {
                let (lo, hi, value) = RANGES[lane];
                std::array::from_fn(|i| {
                    // A new pattern plays the root on every beat.
                    let value = if lane == ON {
                        f64::from(u8::from(i % 4 == 0))
                    } else {
                        value
                    };
                    let behavior = if lane == VEL || lane == CHANCE || lane == GATE {
                        Behavior::Linear
                    } else {
                        Behavior::Free
                    };
                    reg.create(g, &lane_key(s, lane, i), value, lo, hi, behavior)
                })
            }),
            lane_len: std::array::from_fn(|lane| {
                let key = format!("seq{}_{}_len", s + 1, LANES[lane]);
                reg.create(g, &key, 0.0, 0.0, STEPS as f64, Behavior::Free)
            }),
            pitch_offset: reg.create(
                g,
                &format!("seq{}_pitch_offset", s + 1),
                0.0,
                -24.0,
                24.0,
                Behavior::Free,
            ),
            vel_offset: reg.create(
                g,
                &format!("seq{}_vel_offset", s + 1),
                0.0,
                -1.0,
                1.0,
                Behavior::Linear,
            ),
        };
        Self {
            play: reg.toggle(g, "seq"),
            slot: reg.create(g, "seq_slot", 0.0, 0.0, (SLOTS - 1) as f64, Behavior::Free),
            mode: reg.create(
                g,
                "seq_mode",
                0.0,
                0.0,
                (MODES.len() - 1) as f64,
                Behavior::Free,
            ),
            rate: reg.create(
                g,
                "seq_rate",
                crate::arp::DEFAULT_RATE as f64,
                0.0,
                (RATES.len() - 1) as f64,
                Behavior::Free,
            ),
            swing: reg.create(g, "seq_swing", 0.0, 0.0, 0.75, Behavior::Linear),
            position: reg.create(g, "seq_position", -1.0, -1.0, last, Behavior::Indicator),
            waiting: reg.indicator(g, "seq_waiting", 1.0),
            slot_playing: reg.indicator(g, "seq_slot_playing", (SLOTS - 1) as f64),
            slots: std::array::from_fn(slot),
        }
    }
}

/// The note pitch `degree` plays: scale degrees up or down from the root
/// of `mask` (a bit per pitch class from C), whose root is `root`. With no
/// scale, C minor.
pub fn note_for(degree: i32, root: u8, mask: u16) -> i32 {
    let (root, mask) = if mask & 0xfff == 0 {
        (0, C_MINOR)
    } else {
        (root % 12, mask)
    };
    let mut offsets = [0i32; 12];
    let mut n = 0;
    for i in 0..12 {
        if mask >> ((root as usize + i) % 12) & 1 == 1 {
            offsets[n] = i as i32;
            n += 1;
        }
    }
    if n == 0 {
        return BASE + root as i32 + degree;
    }
    let n = n as i32;
    BASE + root as i32 + 12 * degree.div_euclid(n) + offsets[degree.rem_euclid(n) as usize]
}

/// Fresh values for a lane's steps (its RANDOM), drawn from `next`. Pitch
/// leans on the root, fifth and third so a random riff still sounds like
/// one; a few notes tie, slide or jump an octave.
pub fn random_lane(lane: usize, mut next: impl FnMut() -> u64) -> [f64; STEPS] {
    const DEGREES: [f64; 12] = [0.0, 0.0, 0.0, 4.0, 4.0, 2.0, 7.0, 3.0, 5.0, -1.0, -3.0, 6.0];
    std::array::from_fn(|i| {
        let r = next();
        let u = (r % 10_000) as f64 / 10_000.0;
        let pick = (r >> 20) as usize;
        match lane {
            ON => f64::from(u8::from(i == 0 || u < 0.6)),
            CHANCE => [1.0, 1.0, 1.0, 0.75, 0.5][pick % 5],
            PITCH => DEGREES[pick % DEGREES.len()],
            VEL => {
                if u < 0.25 {
                    1.0
                } else {
                    0.55 + 0.3 * u
                }
            }
            OCT => {
                if u < 0.12 {
                    1.0
                } else if u < 0.2 {
                    -1.0
                } else {
                    0.0
                }
            }
            GATE => {
                if u < 0.1 {
                    TIE
                } else {
                    0.25 + 0.65 * u
                }
            }
            SLIDE => f64::from(u8::from(u < 0.2)),
            _ => 0.0,
        }
    })
}

/// A lane's steps as RESET leaves them: no steps on, every other lane at
/// its starting value.
pub fn reset_value(lane: usize) -> f64 {
    if lane == ON {
        0.0
    } else {
        RANGES[lane].2
    }
}

/// Where step `k` (from 0, since it started) lands in a lane of `len`
/// steps. `r` is this step's random number.
pub fn position(k: u64, len: usize, mode: u8, r: u32) -> usize {
    let len = len.clamp(1, STEPS) as u64;
    let p = match mode {
        BACKWARD => len - 1 - k % len,
        PING_PONG if len > 1 => {
            let cycle = 2 * len - 2;
            let i = k % cycle;
            if i < len {
                i
            } else {
                cycle - i
            }
        }
        RANDOM => u64::from(r) % len,
        _ => k % len,
    };
    p as usize
}

/// The note sounding.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sounding {
    note: u8,
    /// The beat it ends on; infinite while it waits for the next step
    /// (a tie or a slide).
    end: f64,
    /// It slides: the next note starts before it ends.
    slide: bool,
}

pub struct Seq {
    c: SeqControls,
    running: bool,
    /// Step 0's beat.
    start_at: f64,
    /// Where it is when the mix's beat is not moving.
    beat: f64,
    /// The last step played, so a beat that steps back a hair between
    /// buffers does not play it twice.
    last_k: i64,
    slot: usize,
    sounding: Option<Sounding>,
    rng: u32,
}

fn push(out: &mut Vec<Midi>, m: Midi) {
    if out.len() < out.capacity() {
        out.push(m);
    }
}

impl Seq {
    pub fn new(c: SeqControls) -> Self {
        Self {
            c,
            running: false,
            start_at: 0.0,
            beat: 0.0,
            last_k: -1,
            slot: 0,
            sounding: None,
            rng: 0x2545_f491,
        }
    }

    pub fn controls(&self) -> &SeqControls {
        &self.c
    }

    fn random(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn lane(&self, lane: usize, k: u64, mode: u8, r: u32) -> f64 {
        let s = &self.c.slots[self.slot];
        let own = s.lane_len[lane].get().round() as usize;
        let len = if own == 0 {
            s.len.get().round() as usize
        } else {
            own
        };
        s.steps[lane][position(k, len, mode, r)].get()
    }

    fn end_note(&mut self, frame: u32, out: &mut Vec<Midi>) {
        if let Some(s) = self.sounding.take() {
            push(out, Midi::note_off(frame, 0, s.note));
        }
    }

    /// Adds the notes it plays over `frames` at `sample_rate` to `out`, in
    /// time with `transport`, in the scale `mask` on `root` (see
    /// [`note_for`]).
    pub fn process(
        &mut self,
        frames: usize,
        sample_rate: f32,
        transport: Transport,
        root: u8,
        mask: u16,
        out: &mut Vec<Midi>,
    ) {
        let bpm = if transport.bpm > 0.0 {
            transport.bpm
        } else {
            120.0
        };
        let start = if transport.playing {
            transport.beat
        } else {
            self.beat
        };
        let per_frame = bpm / 60.0 / f64::from(sample_rate);
        let end = start + frames as f64 * per_frame;
        self.beat = end;
        let frame_of = |beat: f64| {
            (((beat - start) / per_frame).max(0.0).round() as usize).min(frames.saturating_sub(1))
                as u32
        };

        let play = self.c.play.get_bool();
        if play && !self.running {
            self.running = true;
            self.last_k = -1;
            self.slot = self.picked();
            // On the beat's next bar, or now on its own clock.
            self.start_at = if transport.playing {
                ((start - 1e-6) / BAR).ceil() * BAR
            } else {
                start
            };
        } else if !play && self.running {
            self.running = false;
            self.end_note(0, out);
        }
        if !self.running {
            self.c.position.set(-1.0);
            self.c.waiting.set_bool(false);
            self.c.slot_playing.set(self.picked() as f64);
            return;
        }

        let per_beat = RATES[(self.c.rate.get().round().max(0.0) as usize).min(RATES.len() - 1)].1;
        let step = 1.0 / per_beat;
        let late = self.c.swing.get().clamp(0.0, 0.75) * step;
        let mode = self.c.mode.get().round().max(0.0) as u8;
        let start_at = self.start_at;
        let time = |k: i64| start_at + k as f64 * step + if k % 2 == 1 { late } else { 0.0 };
        let first = (((start - self.start_at - late) / step).floor() as i64).max(0);
        let last = ((end - self.start_at) / step).ceil() as i64;
        for k in first..=last {
            let t = time(k);
            if t < start - 1e-9 || t >= end - 1e-9 {
                continue;
            }
            if k <= self.last_k && self.last_k - k < 2 {
                continue;
            }
            // A note whose gate closed before this step ends where it closed.
            if let Some(s) = self.sounding {
                if s.end <= t {
                    self.end_note(frame_of(s.end.max(start)), out);
                }
            }
            self.last_k = k;
            self.step(k as u64, t, step, mode, root, mask, frame_of(t), out);
        }
        // A gate closing after the last step in this buffer.
        if let Some(s) = self.sounding {
            if s.end < end {
                self.end_note(frame_of(s.end.max(start)), out);
            }
        }
        self.c.waiting.set_bool(start < self.start_at - 1e-9);
        self.c.slot_playing.set(self.slot as f64);
    }

    fn picked(&self) -> usize {
        (self.c.slot.get().round().max(0.0) as usize).min(SLOTS - 1)
    }

    /// Plays step `k`, starting on beat `t` at `frame`.
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        k: u64,
        t: f64,
        step: f64,
        mode: u8,
        root: u8,
        mask: u16,
        frame: u32,
        out: &mut Vec<Midi>,
    ) {
        // A new slot starts on a bar line.
        let on_bar = ((t - self.start_at) / BAR - ((t - self.start_at) / BAR).round()).abs() < 1e-6;
        if on_bar {
            self.slot = self.picked();
        }
        let r = self.random();
        let len = self.c.slots[self.slot].len.get().round() as usize;
        self.c.position.set(position(k, len, mode, r) as f64);
        let on = self.lane(ON, k, mode, r) > 0.5;
        let chance = self.lane(CHANCE, k, mode, r);
        let plays = on && (chance >= 1.0 || f64::from(self.random() % 10_000) / 10_000.0 < chance);
        if !plays {
            self.end_note(frame, out);
            return;
        }
        let s = &self.c.slots[self.slot];
        let degree = self.lane(PITCH, k, mode, r).round() as i32;
        let octave = self.lane(OCT, k, mode, r).round() as i32;
        let note = note_for(degree, root, mask) + 12 * octave + s.pitch_offset.get().round() as i32;
        let note = note.clamp(0, 127) as u8;
        let vel = (self.lane(VEL, k, mode, r) + s.vel_offset.get()).clamp(0.0, 1.0);
        let vel = ((vel * 127.0).round() as u8).max(1);
        let gate = self.lane(GATE, k, mode, r);
        let slide = self.lane(SLIDE, k, mode, r) > 0.5;
        let held = slide || gate >= TIE;
        let ends = if held { f64::INFINITY } else { t + gate * step };
        match self.sounding {
            // Tied or slid into the same note: it just goes on.
            Some(old) if old.end.is_infinite() && old.note == note => {}
            // Slid into another: the new note starts before the old ends.
            Some(old) if old.end.is_infinite() && old.slide => {
                push(out, Midi::note_on(frame, 0, note, vel));
                push(out, Midi::note_off(frame, 0, old.note));
            }
            _ => {
                self.end_note(frame, out);
                push(out, Midi::note_on(frame, 0, note, vel));
            }
        }
        self.sounding = Some(Sounding {
            note,
            end: ends,
            slide,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const SR: f32 = 48_000.0;

    /// 120 BPM: a beat is 24,000 frames.
    fn transport(beat: f64, playing: bool) -> Transport {
        Transport {
            bpm: 120.0,
            beat,
            playing,
        }
    }

    fn seq() -> (Arc<ControlRegistry>, Seq) {
        let reg = ControlRegistry::new();
        let c = SeqControls::new(&reg, "[Instrument1]");
        (reg, Seq::new(c))
    }

    fn set(reg: &ControlRegistry, key: &str, v: f64) {
        reg.get("[Instrument1]", key).unwrap().set(v);
    }

    /// Plays from `from` for `beats` in buffers of 480 frames; the notes as
    /// (status, note, frame from `from`).
    fn run(s: &mut Seq, from: f64, beats: f64, playing: bool) -> Vec<(u8, u8, usize)> {
        let mut found = Vec::new();
        let buffers = (beats * 24_000.0 / 480.0) as usize;
        for b in 0..buffers {
            let mut out = Vec::with_capacity(64);
            let beat = from + b as f64 * 480.0 / 24_000.0;
            s.process(480, SR, transport(beat, playing), 0, 0, &mut out);
            found.extend(
                out.iter()
                    .map(|m| (m.data[0] & 0xf0, m.data[1], b * 480 + m.frame as usize)),
            );
        }
        found
    }

    #[test]
    fn pitch_is_counted_in_the_scale() {
        // C minor with SCALE off: C D Eb F G Ab Bb.
        assert_eq!(note_for(0, 0, 0), 48);
        assert_eq!(note_for(2, 0, 0), 51);
        assert_eq!(note_for(7, 0, 0), 60);
        assert_eq!(note_for(-1, 0, 0), 46);
        // A minor pentatonic: A C D E G.
        let mask = crate::scale::mask(9, 3);
        assert_eq!(note_for(0, 9, mask), 57);
        assert_eq!(note_for(1, 9, mask), 60);
        assert_eq!(note_for(5, 9, mask), 69);
    }

    #[test]
    fn random_lanes_stay_in_range() {
        let mut state = 7u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 11
        };
        for lane in 0..LANES.len() {
            let (lo, hi, _) = RANGES[lane];
            let steps = random_lane(lane, &mut next);
            assert!(
                steps.iter().all(|v| (lo..=hi).contains(v)),
                "{} {steps:?}",
                LANES[lane]
            );
        }
        assert_eq!(
            random_lane(ON, &mut next)[0],
            1.0,
            "a riff starts on the one"
        );
    }

    #[test]
    fn modes_walk_the_steps() {
        let walk = |mode| (0..8).map(|k| position(k, 4, mode, 0)).collect::<Vec<_>>();
        assert_eq!(walk(FORWARD), [0, 1, 2, 3, 0, 1, 2, 3]);
        assert_eq!(walk(BACKWARD), [3, 2, 1, 0, 3, 2, 1, 0]);
        assert_eq!(walk(PING_PONG), [0, 1, 2, 3, 2, 1, 0, 1]);
    }

    #[test]
    fn it_waits_for_the_bar_and_plays_on_the_grid() {
        let (reg, mut s) = seq();
        set(&reg, "seq", 1.0);
        // Started at beat 1.5 of a playing deck: the first step is beat 4.
        let notes = run(&mut s, 1.5, 3.0, true);
        let ons: Vec<_> = notes.iter().filter(|n| n.0 == 0x90).collect();
        assert_eq!(ons.len(), 1, "{notes:?}");
        // Beat 4 is 2.5 beats in: 60,000 frames.
        assert_eq!(ons[0], &(0x90, 48, 60_000));
        // A half-step gate at 1/16: it ends an eighth of a beat later.
        assert!(notes.contains(&(0x80, 48, 63_000)), "{notes:?}");
    }

    #[test]
    fn steps_off_and_lanes_of_their_own_length() {
        let (reg, mut s) = seq();
        set(&reg, "seq", 1.0);
        for i in 1..=16 {
            set(&reg, &format!("seq1_on_{i}"), 1.0);
        }
        // Pitch has three steps: 0, 1, 2, then again.
        set(&reg, "seq1_pitch_len", 3.0);
        set(&reg, "seq1_pitch_2", 1.0);
        set(&reg, "seq1_pitch_3", 2.0);
        let ons: Vec<u8> = run(&mut s, 0.0, 1.5, false)
            .iter()
            .filter(|n| n.0 == 0x90)
            .map(|n| n.1)
            .collect();
        assert_eq!(ons, [48, 50, 51, 48, 50, 51]);
    }

    #[test]
    fn ties_and_slides_hold_into_the_next_step() {
        let (reg, mut s) = seq();
        set(&reg, "seq", 1.0);
        for i in 1..=4 {
            set(&reg, &format!("seq1_on_{i}"), 1.0);
        }
        set(&reg, "seq1_len", 4.0);
        // Step 1 ties into step 2 (the same note), which slides into step 3.
        set(&reg, "seq1_gate_1", TIE);
        set(&reg, "seq1_slide_2", 1.0);
        set(&reg, "seq1_pitch_3", 2.0);
        let notes = run(&mut s, 0.0, 0.75, false);
        assert_eq!(
            notes,
            [
                (0x90, 48, 0),
                // Step 3 starts before step 2's note ends: a slide.
                (0x90, 51, 12_000),
                (0x80, 48, 12_000),
                (0x80, 51, 15_000),
            ]
        );
    }

    #[test]
    fn stopping_ends_the_note() {
        let (reg, mut s) = seq();
        set(&reg, "seq", 1.0);
        set(&reg, "seq1_gate_1", TIE);
        run(&mut s, 0.0, 0.1, false);
        set(&reg, "seq", 0.0);
        let mut out = Vec::with_capacity(8);
        s.process(480, SR, transport(0.0, false), 0, 0, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data, [0x80, 48, 0]);
        assert_eq!(
            reg.get("[Instrument1]", "seq_position").unwrap().get(),
            -1.0
        );
    }

    #[test]
    fn a_new_slot_waits_for_the_bar() {
        let (reg, mut s) = seq();
        set(&reg, "seq", 1.0);
        set(&reg, "seq2_pitch_1", 3.0);
        set(&reg, "seq2_pitch_5", 3.0);
        run(&mut s, 0.0, 1.5, false);
        set(&reg, "seq_slot", 1.0);
        // Beats 1.5 to 4 still play A (the root); B's starts at the bar.
        let ons: Vec<u8> = run(&mut s, 1.5, 3.0, false)
            .iter()
            .filter(|n| n.0 == 0x90)
            .map(|n| n.1)
            .collect();
        assert_eq!(ons, [48, 48, 53]);
    }
}
