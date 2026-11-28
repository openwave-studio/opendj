//! Live's Limiter: keeps peaks under the ceiling by looking a little
//! ahead, so it turns the gain down before a peak arrives instead of
//! clipping it.
//!
//! The gain each frame needs is the lowest the window ahead asks for (a
//! running minimum), smoothed by a running mean over the same window, so
//! the gain ramps down over the lookahead and reaches what the peak needs
//! exactly as the peak comes out of the delay. It then comes back up at
//! the release. Adds the lookahead (less one frame) of delay.

use crate::devices::{from_db, Param, Unit};
use crate::kit::{Follower, Smooth};

/// Live's lookahead choices, in ms.
pub const LOOKAHEADS: [f32; 3] = [1.5, 3.0, 6.0];
const LOOKAHEAD_NAMES: &[&str] = &["1.5 ms", "3 ms", "6 ms"];
const LINK_NAMES: &[&str] = &["Stereo", "L/R"];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LimiterSettings {
    /// Gain into the limiter, dB.
    pub gain_db: f32,
    /// The highest a peak comes out, dB.
    pub ceiling_db: f32,
    pub release_ms: f32,
    /// Picks the release from the music: short after a lone peak, longer
    /// while it keeps limiting.
    pub auto_release: bool,
    /// An index into [`LOOKAHEADS`].
    pub lookahead: u8,
    /// Each side limited on its own, rather than both together.
    pub split: bool,
}

impl Default for LimiterSettings {
    fn default() -> Self {
        LimiterSettings {
            gain_db: 0.0,
            ceiling_db: -0.3,
            release_ms: 300.0,
            auto_release: true,
            lookahead: 0,
            split: false,
        }
    }
}

impl LimiterSettings {
    pub fn summary(&self) -> String {
        format!(
            "{:+.1} dB · ceiling {:.1} dB · {}",
            self.gain_db,
            self.ceiling_db,
            if self.auto_release {
                "auto release".to_string()
            } else {
                format!("{:.0} ms release", self.release_ms)
            }
        )
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        vec![
            Param::knob("Gain", self.gain_db, -24.0, 24.0, Unit::Db).bipolar(),
            Param::knob("Ceiling", self.ceiling_db, -24.0, 0.0, Unit::Db),
            Param::knob("Release", self.release_ms, 10.0, 3000.0, Unit::Ms).log(),
            Param::toggle("Auto", self.auto_release),
            Param::choice("Lookahead", self.lookahead as usize, LOOKAHEAD_NAMES),
            Param::choice("Link", self.split as usize, LINK_NAMES),
        ]
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.gain_db = v.clamp(-24.0, 24.0),
            1 => self.ceiling_db = v.clamp(-24.0, 0.0),
            2 => self.release_ms = v.clamp(10.0, 3000.0),
            3 => self.auto_release = v >= 0.5,
            4 => self.lookahead = v.round().clamp(0.0, 2.0) as u8,
            5 => self.split = v >= 0.5,
            _ => {}
        }
    }
}

/// One side's gain computer: the running minimum and mean over the
/// window, in rings sized for the longest lookahead.
struct Side {
    /// Gains each frame needs, newest last, for the mean.
    needs: Vec<f32>,
    /// The minimum's candidates: (frame, gain), gains rising from front
    /// to back.
    mins: Vec<(u64, f32)>,
    min_front: usize,
    min_len: usize,
    sum: f64,
    /// The gain applied, after the release.
    gain: f32,
}

impl Side {
    fn new(capacity: usize) -> Side {
        Side {
            needs: vec![1.0; capacity],
            mins: vec![(0, 1.0); capacity],
            min_front: 0,
            min_len: 0,
            sum: capacity as f64,
            gain: 1.0,
        }
    }

    fn reset(&mut self, window: usize) {
        self.needs.fill(1.0);
        self.min_front = 0;
        self.min_len = 0;
        self.sum = window as f64;
        self.gain = 1.0;
    }

    /// The smoothed gain for the frame `window - 1` frames back, given what
    /// frame `n` needs.
    #[inline]
    fn next(&mut self, n: u64, need: f32, window: usize, release: f32) -> f32 {
        let cap = self.mins.len();
        // Running minimum over the last `window` frames.
        while self.min_len > 0 {
            let back = (self.min_front + self.min_len - 1) % cap;
            if self.mins[back].1 >= need {
                self.min_len -= 1;
            } else {
                break;
            }
        }
        let back = (self.min_front + self.min_len) % cap;
        self.mins[back] = (n, need);
        self.min_len += 1;
        while self.mins[self.min_front].0 + window as u64 <= n {
            self.min_front = (self.min_front + 1) % cap;
            self.min_len -= 1;
        }
        let min = self.mins[self.min_front].1;
        // Running mean of the minimum over the same window.
        let at = (n % window as u64) as usize;
        self.sum += min as f64 - self.needs[at] as f64;
        self.needs[at] = min;
        let mean = ((self.sum / window as f64) as f32).min(1.0);
        // Down at once (the mean has already ramped it), up at the release.
        self.gain = if mean < self.gain {
            mean
        } else {
            mean + (self.gain - mean) * release
        };
        self.gain
    }
}

pub struct Limiter {
    pub(crate) s: LimiterSettings,
    rate: f64,
    gain: Smooth,
    ceiling: Smooth,
    /// The input, delayed by the lookahead.
    delay: Vec<[f32; 2]>,
    sides: [Side; 2],
    frame: u64,
    window: usize,
    release: f32,
    /// How much it has been limiting lately, 0..1, for the auto release.
    busy: Follower,
}

impl Limiter {
    pub fn new(s: &LimiterSettings, rate: f64) -> Limiter {
        let capacity = (LOOKAHEADS[2] * 0.001 * rate as f32).ceil() as usize + 1;
        let mut l = Limiter {
            s: s.clone(),
            rate,
            gain: Smooth::knob(from_db(s.gain_db), rate),
            ceiling: Smooth::knob(from_db(s.ceiling_db), rate),
            delay: vec![[0.0; 2]; capacity],
            sides: [Side::new(capacity), Side::new(capacity)],
            frame: 0,
            window: 1,
            release: 0.0,
            busy: Follower::new(50.0, 1000.0, rate),
        };
        l.retune();
        l
    }

    pub fn set(&mut self, i: usize, v: f32) {
        let lookahead = self.s.lookahead;
        self.s.set(i, v);
        if self.s.lookahead != lookahead {
            self.delay.fill([0.0; 2]);
            for s in &mut self.sides {
                s.reset(self.window);
            }
        }
        self.retune();
    }

    fn retune(&mut self) {
        self.gain.set(from_db(self.s.gain_db));
        self.ceiling.set(from_db(self.s.ceiling_db));
        let ms = LOOKAHEADS[self.s.lookahead.min(2) as usize];
        let window = ((ms * 0.001 * self.rate as f32).round() as usize).clamp(1, self.delay.len());
        if window != self.window {
            self.window = window;
            for s in &mut self.sides {
                s.reset(window);
            }
        }
        self.release = coef(self.s.release_ms, self.rate);
    }

    /// Frames of delay the lookahead adds.
    pub fn latency(&self) -> usize {
        self.window - 1
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        let window = self.window;
        let cap = self.delay.len();
        for f in buf.iter_mut() {
            let g = self.gain.tick();
            let ceiling = self.ceiling.tick();
            let x = [f[0] * g, f[1] * g];
            let need = |v: f32| {
                let a = v.abs();
                if a > ceiling {
                    ceiling / a
                } else {
                    1.0
                }
            };
            let release = if self.s.auto_release {
                // 40 ms after a lone peak, up to 800 ms while it keeps
                // limiting, so a dense mix doesn't pump.
                coef(40.0 + 760.0 * self.busy.level().min(1.0), self.rate)
            } else {
                self.release
            };
            let n = self.frame;
            let gains = if self.s.split {
                [
                    self.sides[0].next(n, need(x[0]), window, release),
                    self.sides[1].next(n, need(x[1]), window, release),
                ]
            } else {
                let g = self.sides[0].next(n, need(x[0].abs().max(x[1].abs())), window, release);
                [g, g]
            };
            if self.s.auto_release {
                self.busy.next(1.0 - gains[0].min(gains[1]));
            }
            // The frame the gain was worked out for comes out of the delay
            // now.
            let at = (n % cap as u64) as usize;
            let back = ((n + cap as u64 - (window as u64 - 1)) % cap as u64) as usize;
            self.delay[at] = x;
            let y = self.delay[back];
            // The ramp lands on the ceiling; the clamp only catches the
            // last rounding.
            *f = [
                (y[0] * gains[0]).clamp(-ceiling, ceiling),
                (y[1] * gains[1]).clamp(-ceiling, ceiling),
            ];
            self.frame += 1;
        }
    }

    /// Gain reduction now, dB (0 or less), for its meter.
    pub fn reduction_db(&self) -> f32 {
        crate::devices::db(self.sides[0].gain.min(self.sides[1].gain))
    }
}

fn coef(ms: f32, rate: f64) -> f32 {
    (-1.0 / (ms.max(0.01) * 0.001 * rate as f32)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32, n: usize) -> Vec<[f32; 2]> {
        (0..n)
            .map(|i| {
                let v = (i as f32 * freq * std::f32::consts::TAU / 48_000.0).sin() * amp;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn peaks_never_pass_the_ceiling() {
        for lookahead in 0..3 {
            for split in [false, true] {
                let s = LimiterSettings {
                    gain_db: 12.0,
                    ceiling_db: -1.0,
                    lookahead,
                    split,
                    ..Default::default()
                };
                let mut l = Limiter::new(&s, 48_000.0);
                // Settle the input gain first.
                l.process(&mut vec![[0.0; 2]; 4_800]);
                // Sine bursts and a lone click.
                let mut buf = sine(220.0, 0.9, 48_000);
                buf[20_000] = [1.0, -1.0];
                l.process(&mut buf);
                let ceiling = from_db(-1.0);
                let peak = buf
                    .iter()
                    .fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                assert!(
                    peak <= ceiling + 1e-6,
                    "{lookahead} {split}: {peak} over {ceiling}"
                );
                // And it is limiting, not silencing.
                assert!(peak > ceiling * 0.9, "{lookahead} {split}: only {peak}");
            }
        }
    }

    #[test]
    fn below_the_ceiling_it_only_delays() {
        let mut l = Limiter::new(&LimiterSettings::default(), 48_000.0);
        let input = sine(440.0, 0.5, 9_600);
        let mut buf = input.clone();
        l.process(&mut buf);
        let d = l.latency();
        assert_eq!(d, 71, "1.5 ms at 48 kHz less one frame");
        for i in d..buf.len() {
            assert!(
                (buf[i][0] - input[i - d][0]).abs() < 1e-6,
                "frame {i} changed"
            );
        }
    }

    #[test]
    fn the_gain_ramps_down_before_a_peak_without_a_step() {
        let s = LimiterSettings {
            ceiling_db: -6.0,
            ..Default::default()
        };
        let mut l = Limiter::new(&s, 48_000.0);
        let mut buf = sine(100.0, 0.3, 4_800);
        // A loud stretch from 2,400 on.
        for f in &mut buf[2_400..] {
            f[0] *= 3.0;
            f[1] *= 3.0;
        }
        l.process(&mut buf);
        let step = buf
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).abs())
            .fold(0.0f32, f32::max);
        // A 100 Hz sine at this level moves at most ~0.02 a frame; a hard
        // gain step would jump by far more.
        assert!(step < 0.03, "jumped {step}");
    }
}
