//! The Remix deck's X-PAD: four drum pads (kick, snare, clap, hat from a
//! Groove Box kit) that hit on the beat grid and roll while held.
//!
//! A press lands on the next 1/4 beat, or at once when it comes just after
//! one (so a hit a little late still counts as on the beat, and network
//! latency only moves a hit to the next grid point). While a pad is held
//! with a roll rate set it repeats at that rate; a second rate held at the
//! same time (`roll_rate2`) makes a groove roll: hits on both grids, louder
//! where they meet. Without a moving grid (the source stopped, or a mic)
//! hits play at once and rolls keep their own time at the source's tempo.
//!
//! The hits play on the deck's own few voices, after the band split, so
//! they follow the Remix deck's fader in Send and Insert alike.

use std::sync::Arc;

use opendj_control::ControlHandle;

use crate::kits::Kit;
use crate::sampler::Sample;

/// The pads' names, left to right.
pub const PAD_NAMES: [&str; 4] = ["Kick", "Snare", "Clap", "Hat"];
/// The kit sound each pad plays (see [`crate::kits::ROLES`]).
const PAD_SOUNDS: [usize; 4] = [0, 1, 4, 2];

/// The roll rates, by `roll_rate` from 1 (0 is off), in beats.
pub const ROLL_RATES: [&str; 8] = ["1", "1/2", "1/3", "1/4", "1/6", "1/8", "1/12", "1/16"];
const ROLL_BEATS: [f64; 8] = [
    1.0,
    1.0 / 2.0,
    1.0 / 3.0,
    1.0 / 4.0,
    1.0 / 6.0,
    1.0 / 8.0,
    1.0 / 12.0,
    1.0 / 16.0,
];

/// A press waits for the next grid point of this many beats...
const QUANTUM: f64 = 0.25;
/// ...unless it came within this share of a step after the last one.
const LATE: f64 = 0.25;
/// A groove roll's hits off the other grid.
const OFF_ACCENT: f32 = 0.6;
const VOICES: usize = 8;
/// Frames over which a cut-off hit fades out.
const FADE_FRAMES: f32 = 96.0;
/// Further than this between the grid and where the pads thought it was
/// (a seek, or the clock changing) and the waiting hits move with it.
const JUMP: f64 = 0.5;

/// `[RemixN]`'s pad controls.
pub struct PadControls {
    /// Held 1, let go 0.
    pub pad: [ControlHandle; 4],
    /// Semitones, -12..12.
    pub pitch: [ControlHandle; 4],
    /// 0 off, 1-8 [`ROLL_RATES`].
    pub roll_rate: ControlHandle,
    pub roll_rate2: ControlHandle,
    /// The Groove Box kit the pads play.
    pub pad_bank: ControlHandle,
}

struct Voice {
    sample: Arc<Sample>,
    pad: usize,
    pos: f64,
    step: f64,
    gain: f32,
    /// Frames of fade left once cut off.
    fade: Option<f32>,
}

impl Voice {
    #[inline]
    fn next(&mut self) -> Option<[f32; 2]> {
        let len = self.sample.len();
        let i = self.pos as usize;
        if i + 1 >= len {
            return None;
        }
        let t = self.pos.fract() as f32;
        let (a, b) = (self.sample.frame(i), self.sample.frame(i + 1));
        let mut g = self.gain;
        if let Some(left) = self.fade.as_mut() {
            if *left <= 0.0 {
                return None;
            }
            g *= *left / FADE_FRAMES;
            *left -= 1.0;
        }
        self.pos += self.step;
        Some([
            (a[0] + (b[0] - a[0]) * t) * g,
            (a[1] + (b[1] - a[1]) * t) * g,
        ])
    }
}

/// One pad's state: held or not, and when it hits next on each grid.
#[derive(Clone, Copy, Default)]
struct Pad {
    held: bool,
    next: Option<f64>,
    next2: Option<f64>,
}

pub struct Pads {
    kits: Arc<[Kit]>,
    sample_rate: f64,
    voices: [Option<Voice>; VOICES],
    pads: [Pad; 4],
    rate: Option<f64>,
    rate2: Option<f64>,
    bank: usize,
    step: [f64; 4],
    /// Where in beats this frame is, and beats per frame.
    pos: f64,
    per_frame: f64,
    /// The source's grid is moving: hits wait for it.
    on_grid: bool,
    last_grid: Option<f64>,
    /// Hits so far (for tests and the meters).
    pub hits: usize,
}

fn rate(v: f64) -> Option<f64> {
    let i = v.round() as i64;
    (1..=ROLL_BEATS.len() as i64)
        .contains(&i)
        .then(|| ROLL_BEATS[i as usize - 1])
}

/// The first grid point of `step` beats at or after `pos`.
fn next_point(pos: f64, step: f64) -> f64 {
    (pos / step - 1e-9).ceil() * step
}

impl Pads {
    pub fn new(sample_rate: f64, kits: Arc<[Kit]>) -> Self {
        Self {
            kits,
            sample_rate,
            voices: std::array::from_fn(|_| None),
            pads: [Pad::default(); 4],
            rate: None,
            rate2: None,
            bank: 0,
            step: [1.0; 4],
            pos: 0.0,
            per_frame: 0.0,
            on_grid: false,
            last_grid: None,
            hits: 0,
        }
    }

    /// When a press or a new rate hits first.
    fn first(&self, step: f64) -> f64 {
        if !self.on_grid {
            return self.pos;
        }
        let at = next_point(self.pos, step);
        if at - self.pos > (1.0 - LATE) * step {
            self.pos
        } else {
            at
        }
    }

    /// Reads the controls for the next buffer: `beat_seconds` long beats,
    /// at `beat` in the source's grid if it has one.
    pub fn prepare(&mut self, c: &PadControls, beat_seconds: f64, beat: Option<f64>) {
        let beat_seconds = if beat_seconds > 0.0 {
            beat_seconds
        } else {
            0.5
        };
        self.per_frame = 1.0 / (beat_seconds * self.sample_rate);
        // The grid counts only while it moves.
        let moving = matches!((beat, self.last_grid), (Some(b), Some(l)) if b != l);
        self.last_grid = beat;
        self.on_grid = moving;
        if let (true, Some(b)) = (moving, beat) {
            let delta = b - self.pos;
            if delta.abs() > JUMP {
                for p in &mut self.pads {
                    p.next = p.next.map(|n| n + delta);
                    p.next2 = p.next2.map(|n| n + delta);
                }
            }
            self.pos = b;
        }
        let kits = self.kits.len().max(1);
        self.bank = (c.pad_bank.get().round().max(0.0) as usize).min(kits - 1);
        for (s, p) in self.step.iter_mut().zip(&c.pitch) {
            *s = 2f64.powf(p.get().clamp(-12.0, 12.0) / 12.0);
        }
        let (rate1, rate2) = (rate(c.roll_rate.get()), rate(c.roll_rate2.get()));
        let changed = (rate1 != self.rate, rate2 != self.rate2);
        self.rate = rate1;
        self.rate2 = rate2;
        for k in 0..4 {
            let down = c.pad[k].get() >= 0.5;
            let mut p = self.pads[k];
            if down && !p.held {
                p.next = Some(self.first(rate1.unwrap_or(QUANTUM)));
                p.next2 = rate2.map(|r| self.first(r));
            } else if !down {
                p.next = None;
                p.next2 = None;
            } else {
                // Held while a rate changes: the roll moves to the new grid.
                if changed.0 {
                    p.next = rate1.map(|r| next_point(self.pos, r));
                }
                if changed.1 {
                    p.next2 = rate2.map(|r| next_point(self.pos, r));
                }
            }
            p.held = down;
            self.pads[k] = p;
        }
    }

    fn trigger(&mut self, k: usize, gain: f32) {
        self.hits += 1;
        let Some(kit) = self.kits.get(self.bank) else {
            return;
        };
        let Some(sound) = kit.sounds.get(PAD_SOUNDS[k]) else {
            return;
        };
        // The pad's last hit makes way.
        for v in self.voices.iter_mut().flatten() {
            if v.pad == k && v.fade.is_none() {
                v.fade = Some(FADE_FRAMES);
            }
        }
        let free = self
            .voices
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                // All busy: the one furthest through its sound.
                (0..VOICES)
                    .max_by(|&a, &b| {
                        let at = |i: usize| self.voices[i].as_ref().map_or(0.0, |v| v.pos);
                        at(a).total_cmp(&at(b))
                    })
                    .unwrap_or(0)
            });
        self.voices[free] = Some(Voice {
            sample: sound.sample.clone(),
            pad: k,
            pos: 0.0,
            step: self.step[k],
            gain,
            fade: None,
        });
    }

    /// The pads' next frame.
    #[inline]
    pub fn play(&mut self) -> [f32; 2] {
        // Hits due by the middle of this frame.
        let due = self.pos + 0.5 * self.per_frame;
        for k in 0..4 {
            let mut p = self.pads[k];
            let one = p.next.is_some_and(|n| n <= due);
            let two = p.next2.is_some_and(|n| n <= due);
            if one {
                p.next = self.rate.map(|r| {
                    let mut n = p.next.unwrap_or(self.pos) + r;
                    while n <= due {
                        n += r;
                    }
                    n
                });
            }
            if two {
                p.next2 = self.rate2.map(|r| {
                    let mut n = p.next2.unwrap_or(self.pos) + r;
                    while n <= due {
                        n += r;
                    }
                    n
                });
            }
            self.pads[k] = p;
            if one || two {
                let groove = self.rate.is_some() && self.rate2.is_some();
                self.trigger(
                    k,
                    if groove && !(one && two) {
                        OFF_ACCENT
                    } else {
                        1.0
                    },
                );
            }
        }
        self.pos += self.per_frame;
        let mut out = [0.0f32; 2];
        for slot in &mut self.voices {
            if let Some(v) = slot {
                match v.next() {
                    Some(f) => {
                        out[0] += f[0];
                        out[1] += f[1];
                    }
                    None => *slot = None,
                }
            }
        }
        out
    }

    /// Something is still sounding.
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        self.voices.iter().any(Option::is_some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendj_control::{Behavior, ControlRegistry};

    const SR: f64 = 48_000.0;

    fn setup() -> (Arc<ControlRegistry>, PadControls, Pads) {
        let reg = ControlRegistry::new();
        let g = "[RemixTest]";
        let c = PadControls {
            pad: std::array::from_fn(|k| {
                reg.create(g, &format!("pad{}", k + 1), 0.0, 0.0, 1.0, Behavior::Free)
            }),
            pitch: std::array::from_fn(|k| {
                reg.create(
                    g,
                    &format!("pad{}_pitch", k + 1),
                    0.0,
                    -12.0,
                    12.0,
                    Behavior::Free,
                )
            }),
            roll_rate: reg.create(g, "roll_rate", 0.0, 0.0, 8.0, Behavior::Free),
            roll_rate2: reg.create(g, "roll_rate2", 0.0, 0.0, 8.0, Behavior::Free),
            pad_bank: reg.create(g, "pad_bank", 0.0, 0.0, 5.0, Behavior::Free),
        };
        let kits: Arc<[Kit]> = crate::kits::build(SR).into();
        (reg, c, Pads::new(SR, kits))
    }

    /// Runs `beats` beats at 120 BPM from `from`, on a moving grid or not,
    /// and returns the frames where hits started.
    fn run(pads: &mut Pads, c: &PadControls, from: f64, beats: f64, grid: bool) -> Vec<usize> {
        let frames = (beats * 0.5 * SR) as usize;
        let mut starts = Vec::new();
        let mut f = 0;
        while f < frames {
            let beat = from + f as f64 / (0.5 * SR);
            pads.prepare(c, 0.5, grid.then_some(beat));
            for _ in 0..256.min(frames - f) {
                let before = pads.hits;
                pads.play();
                if pads.hits > before {
                    starts.push(f);
                }
                f += 1;
            }
        }
        starts
    }

    #[test]
    fn a_press_waits_for_the_next_quarter_beat() {
        let (_reg, c, mut pads) = setup();
        // The grid starts moving.
        run(&mut pads, &c, 0.0, 0.1, true);
        c.pad[0].set(1.0);
        // Pressed at beat 0.1: the hit lands on 0.25.
        let starts = run(&mut pads, &c, 0.1, 0.5, true);
        assert_eq!(starts.len(), 1, "one hit without a roll");
        let at = 0.1 + starts[0] as f64 / (0.5 * SR);
        assert!((at - 0.25).abs() < 0.002, "on the grid: {at}");
    }

    #[test]
    fn a_press_just_after_a_grid_point_plays_at_once() {
        let (_reg, c, mut pads) = setup();
        run(&mut pads, &c, 0.0, 0.26, true);
        c.pad[1].set(1.0);
        let starts = run(&mut pads, &c, 0.26, 0.5, true);
        assert_eq!(starts.first(), Some(&0));
    }

    #[test]
    fn a_held_pad_rolls_at_the_rate_and_stops_when_let_go() {
        let (_reg, c, mut pads) = setup();
        run(&mut pads, &c, 0.0, 0.5, true);
        c.roll_rate.set(4.0); // 1/4 beat
        c.pad[0].set(1.0);
        let starts = run(&mut pads, &c, 0.5, 1.0, true);
        assert_eq!(starts.len(), 4, "four hits in a beat: {starts:?}");
        c.pad[0].set(0.0);
        assert!(run(&mut pads, &c, 1.5, 1.0, true).is_empty());
    }

    #[test]
    fn a_groove_roll_hits_on_both_grids_louder_where_they_meet() {
        let (_reg, c, mut pads) = setup();
        run(&mut pads, &c, 0.0, 0.99, true);
        c.roll_rate.set(4.0); // 1/4
        c.roll_rate2.set(3.0); // 1/3
        c.pad[2].set(1.0);
        // From beat 1: 1, 1.25, 1.33, 1.5, 1.67, 1.75, then 2.
        let starts = run(&mut pads, &c, 0.99, 1.0, true);
        assert_eq!(starts.len(), 6, "{starts:?}");
    }

    #[test]
    fn without_a_moving_grid_hits_play_at_once() {
        let (_reg, c, mut pads) = setup();
        run(&mut pads, &c, 0.3, 0.1, false);
        c.pad[3].set(1.0);
        let starts = run(&mut pads, &c, 0.3, 0.2, false);
        assert_eq!(starts, vec![0]);
    }

    #[test]
    fn pads_play_the_kit_sound_pitched() {
        let (_reg, c, mut pads) = setup();
        c.pad[0].set(1.0);
        pads.prepare(&c, 0.5, None);
        let quiet = |p: &mut Pads| {
            let mut n = 0;
            while p.busy() || n == 0 {
                p.play();
                n += 1;
            }
            n
        };
        let plain = quiet(&mut pads);
        c.pad[0].set(0.0);
        c.pitch[0].set(12.0);
        pads.prepare(&c, 0.5, None);
        c.pad[0].set(1.0);
        pads.prepare(&c, 0.5, None);
        let up = quiet(&mut pads);
        assert!(plain > 1000, "the kick sounds");
        assert!(
            (up as f64 / plain as f64 - 0.5).abs() < 0.01,
            "an octave up is twice as fast: {up} / {plain}"
        );
    }
}
