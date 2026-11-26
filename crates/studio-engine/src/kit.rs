//! Parts the studio's devices share: a knob that glides instead of
//! jumping, a low-frequency oscillator, an envelope follower, and 4x
//! oversampling for the devices that bend the sound (so their new
//! harmonics don't fold back down as aliasing).
//!
//! All of them are set up before the audio starts and never allocate
//! while it runs. A modulator that drives a knob (an LFO on a filter's
//! cutoff) moves the knob's target; the knob's glide then smooths it.

use std::f32::consts::TAU;

/// How long a knob takes to glide most of the way to a new value. Short
/// enough to feel instant, long enough that a jump doesn't click.
pub const GLIDE_MS: f32 = 20.0;

/// A setting that glides to its target: a one-pole low-pass on the value,
/// so a knob turned fast, an automation step or a modulator never clicks.
/// Kept in f64, so a slow glide still lands instead of stalling a hair
/// short of its target in f32's rounding.
#[derive(Clone, Copy, Debug)]
pub struct Smooth {
    value: f64,
    target: f64,
    coef: f64,
}

impl Smooth {
    /// Starts at `value`, gliding over about `ms` at `rate`.
    pub fn new(value: f32, rate: f64, ms: f32) -> Smooth {
        Smooth {
            value: value as f64,
            target: value as f64,
            coef: (-1.0 / (ms.max(0.01) as f64 * 0.001 * rate)).exp(),
        }
    }

    /// A knob's usual glide.
    pub fn knob(value: f32, rate: f64) -> Smooth {
        Smooth::new(value, rate, GLIDE_MS)
    }

    pub fn set(&mut self, target: f32) {
        self.target = target as f64;
    }

    /// Lands on `value` at once (a fresh device, a reset).
    pub fn jump(&mut self, value: f32) {
        self.value = value as f64;
        self.target = value as f64;
    }

    pub fn target(&self) -> f32 {
        self.target as f32
    }

    pub fn value(&self) -> f32 {
        self.value as f32
    }

    /// Still on its way.
    pub fn moving(&self) -> bool {
        self.value != self.target
    }

    /// The value for the next frame.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.value != self.target {
            let v = self.target + (self.value - self.target) * self.coef;
            // Lands exactly once it's far below hearing (140 dB under the
            // value).
            self.value = if (v - self.target).abs() < 1e-7 * self.target.abs().max(1.0) {
                self.target
            } else {
                v
            };
        }
        self.value as f32
    }
}

/// An LFO's wave.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    /// A new random level each cycle.
    Random,
}

impl Wave {
    pub const NAMES: &'static [&'static str] =
        &["Sine", "Triangle", "Saw Up", "Saw Down", "Square", "Random"];

    pub fn from_index(i: usize) -> Wave {
        match i {
            1 => Wave::Triangle,
            2 => Wave::SawUp,
            3 => Wave::SawDown,
            4 => Wave::Square,
            5 => Wave::Random,
            _ => Wave::Sine,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// A low-frequency oscillator, -1..1, free in Hz or synced to the tempo.
#[derive(Clone, Debug)]
pub struct Lfo {
    pub wave: Wave,
    /// 0..1 through the cycle.
    phase: f64,
    held: f32,
    seed: u32,
}

impl Lfo {
    pub fn new(wave: Wave, seed: u32) -> Lfo {
        Lfo {
            wave,
            phase: 0.0,
            held: 0.0,
            seed: seed | 1,
        }
    }

    /// Starts its cycle again (at `phase`, 0..1).
    pub fn reset(&mut self, phase: f64) {
        self.phase = phase.rem_euclid(1.0);
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// The value at the cycle's place plus `offset` (0..1, Live's stereo
    /// Phase knob), without moving on.
    pub fn at(&self, offset: f64) -> f32 {
        let p = (self.phase + offset).rem_euclid(1.0) as f32;
        match self.wave {
            Wave::Sine => (p * TAU).sin(),
            Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            Wave::SawUp => 2.0 * p - 1.0,
            Wave::SawDown => 1.0 - 2.0 * p,
            Wave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Random => self.held,
        }
    }

    /// Moves on by `hz / rate` of a cycle; a new random level when a cycle
    /// ends.
    #[inline]
    pub fn advance(&mut self, step: f64) {
        self.phase += step;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 17;
            self.seed ^= self.seed << 5;
            self.held = self.seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
        }
    }
}

/// How far a synced LFO moves per frame: one cycle every `beats` at
/// `bpm`.
pub fn synced_step(beats: f64, bpm: f64, rate: f64) -> f64 {
    bpm / 60.0 / beats.max(1e-3) / rate
}

/// Follows a signal's level, rising at the attack and falling at the
/// release (a peak follower, as Live's envelope sections use).
#[derive(Clone, Copy, Debug)]
pub struct Follower {
    level: f32,
    attack: f32,
    release: f32,
}

impl Follower {
    pub fn new(attack_ms: f32, release_ms: f32, rate: f64) -> Follower {
        let mut f = Follower {
            level: 0.0,
            attack: 0.0,
            release: 0.0,
        };
        f.set_times(attack_ms, release_ms, rate);
        f
    }

    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32, rate: f64) {
        let coef = |ms: f32| (-1.0 / (ms.max(0.01) * 0.001 * rate as f32)).exp();
        self.attack = coef(attack_ms);
        self.release = coef(release_ms);
    }

    #[inline]
    pub fn next(&mut self, x: f32) -> f32 {
        let x = x.abs();
        let c = if x > self.level {
            self.attack
        } else {
            self.release
        };
        self.level = x + (self.level - x) * c;
        if self.level < 1e-9 {
            self.level = 0.0;
        }
        self.level
    }

    pub fn level(&self) -> f32 {
        self.level
    }
}

/// Taps in each half-band filter of [`Oversample`]: odd, so the middle tap
/// sits on a sample, and one more than a multiple of four, so the whole
/// round trip delays by a whole number of frames.
const HALF_TAPS: usize = 65;

/// The zeroth-order modified Bessel function, for the Kaiser window.
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term) = (1.0, 1.0);
    for k in 1..40 {
        term *= (x / (2.0 * k as f64)).powi(2);
        sum += term;
    }
    sum
}

/// A linear-phase half-band low-pass, cutting at a quarter of the rate it
/// runs at: a Kaiser-windowed sinc with every other tap zero, so it costs
/// half its length. Flat to 0.2 of its rate, more than 90 dB down from
/// 0.3 up.
fn half_band() -> [f32; HALF_TAPS] {
    let mid = (HALF_TAPS / 2) as f64;
    let beta = 9.0;
    let mut h = [0.0f64; HALF_TAPS];
    for (i, h) in h.iter_mut().enumerate() {
        let n = i as f64 - mid;
        let sinc = if n == 0.0 {
            0.5
        } else {
            (std::f64::consts::FRAC_PI_2 * n).sin() / (std::f64::consts::PI * n)
        };
        let r = n / mid;
        let win = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / bessel_i0(beta);
        *h = sinc * win;
    }
    let sum: f64 = h.iter().sum();
    h.map(|v| (v / sum) as f32)
}

/// Taps on one side of the half-band's middle that aren't zero: every
/// other one, from the middle out.
const ODD_TAPS: usize = (HALF_TAPS - 1) / 2;

/// The last [`ODD_TAPS`] stereo samples, kept twice over so the newest
/// run is always one contiguous slice.
#[derive(Clone, Debug)]
struct History {
    buf: [[f32; 2]; 2 * ODD_TAPS],
    pos: usize,
}

impl History {
    fn new() -> History {
        History {
            buf: [[0.0; 2]; 2 * ODD_TAPS],
            pos: 0,
        }
    }

    #[inline]
    fn push(&mut self, x: [f32; 2]) {
        self.buf[self.pos] = x;
        self.buf[self.pos + ODD_TAPS] = x;
        self.pos = (self.pos + 1) % ODD_TAPS;
    }

    /// The last [`ODD_TAPS`] samples, oldest first.
    #[inline]
    fn last(&self) -> &[[f32; 2]] {
        &self.buf[self.pos..self.pos + ODD_TAPS]
    }

    /// The sample `j` pushes ago (0 is the newest).
    #[inline]
    fn ago(&self, j: usize) -> [f32; 2] {
        self.buf[self.pos + ODD_TAPS - 1 - j]
    }
}

#[inline]
fn dot(x: &[[f32; 2]], c: &[f32; ODD_TAPS]) -> [f32; 2] {
    let (mut l, mut r) = (0.0f32, 0.0f32);
    for (s, c) in x.iter().zip(c) {
        l += s[0] * c;
        r += s[1] * c;
    }
    [l, r]
}

/// One stage of 2x, split into the half-band's two phases: the middle tap
/// alone (a plain delay) and the odd taps. Up, each frame in gives the
/// delayed frame then the odd taps' sum; down, each pair in gives one
/// frame.
#[derive(Clone, Debug)]
struct Stage {
    input: History,
    evens: History,
    odds: History,
}

/// Where the middle tap reads: half the odd taps back.
const MIDDLE: usize = ODD_TAPS / 2;

impl Stage {
    fn new() -> Stage {
        Stage {
            input: History::new(),
            evens: History::new(),
            odds: History::new(),
        }
    }

    #[inline]
    fn up(&mut self, x: [f32; 2], f: &Filter) -> [[f32; 2]; 2] {
        self.input.push(x);
        let d = self.input.ago(MIDDLE);
        [
            [d[0] * f.middle * 2.0, d[1] * f.middle * 2.0],
            dot(self.input.last(), &f.up),
        ]
    }

    #[inline]
    fn down(&mut self, x: [[f32; 2]; 2], f: &Filter) -> [f32; 2] {
        self.evens.push(x[0]);
        let d = self.evens.ago(MIDDLE);
        let o = dot(self.odds.last(), &f.down);
        self.odds.push(x[1]);
        [d[0] * f.middle + o[0], d[1] * f.middle + o[1]]
    }
}

/// The half-band's taps, split for [`Stage`].
#[derive(Clone, Debug)]
struct Filter {
    middle: f32,
    /// The odd taps, oldest sample first, doubled for the zero-stuffed
    /// input.
    up: [f32; ODD_TAPS],
    down: [f32; ODD_TAPS],
}

impl Filter {
    fn new() -> Filter {
        let h = half_band();
        // The taps are symmetric, so oldest-first is the same order.
        let odd: [f32; ODD_TAPS] = std::array::from_fn(|i| h[2 * i + 1]);
        Filter {
            middle: h[HALF_TAPS / 2],
            up: odd.map(|c| c * 2.0),
            down: odd,
        }
    }
}

/// Runs a stereo shaper at four times the rate: up 2x twice, the shaper on
/// each of the four frames, down 2x twice. Adds [`Oversample::LATENCY`]
/// frames of delay.
#[derive(Clone, Debug)]
pub struct Oversample {
    f: Filter,
    first: Stage,
    second: Stage,
}

impl Default for Oversample {
    fn default() -> Self {
        Oversample::new()
    }
}

impl Oversample {
    /// Frames of delay at the device's rate: each half-band delays by half
    /// its length at the rate it runs at, up and down, at 2x then 4x.
    pub const LATENCY: usize = (HALF_TAPS - 1) / 2 + (HALF_TAPS - 1) / 4;

    pub fn new() -> Oversample {
        Oversample {
            f: Filter::new(),
            first: Stage::new(),
            second: Stage::new(),
        }
    }

    pub fn reset(&mut self) {
        self.first = Stage::new();
        self.second = Stage::new();
    }

    /// One frame through `shape` at 4x.
    #[inline]
    pub fn run(&mut self, x: [f32; 2], mut shape: impl FnMut([f32; 2]) -> [f32; 2]) -> [f32; 2] {
        let f = &self.f;
        let up1 = self.first.up(x, f);
        let mut back = [[0.0f32; 2]; 2];
        for (b, u) in back.iter_mut().zip(up1) {
            let up2 = self.second.up(u, f);
            let shaped = [shape(up2[0]), shape(up2[1])];
            *b = self.second.down(shaped, f);
        }
        self.first.down(back, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_knob_glides_and_lands() {
        let mut s = Smooth::knob(0.0, 48_000.0);
        s.set(1.0);
        let first = s.tick();
        assert!(first > 0.0 && first < 0.01, "jumped: {first}");
        for _ in 0..48_000 {
            s.tick();
        }
        assert_eq!(s.value(), 1.0, "never landed");
        assert!(!s.moving());
    }

    #[test]
    fn lfo_waves_stay_in_range_and_cycle() {
        for i in 0..Wave::NAMES.len() {
            let mut l = Lfo::new(Wave::from_index(i), 7);
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for _ in 0..4_800 {
                l.advance(1.0 / 480.0);
                let v = l.at(0.0);
                lo = lo.min(v);
                hi = hi.max(v);
            }
            assert!(lo >= -1.0 && hi <= 1.0, "{i}: {lo}..{hi}");
            assert!(hi - lo > 0.5, "{i} barely moves: {lo}..{hi}");
        }
        // A quarter note at 120 BPM is 2 Hz.
        assert!((synced_step(1.0, 120.0, 48_000.0) - 2.0 / 48_000.0).abs() < 1e-12);
    }

    #[test]
    fn the_half_band_passes_the_audio_band_and_stops_images() {
        let h = half_band();
        let gain = |f: f32| {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (k, c) in h.iter().enumerate() {
                re += c * (TAU * f * k as f32).cos();
                im -= c * (TAU * f * k as f32).sin();
            }
            20.0 * (re * re + im * im).sqrt().log10()
        };
        // Fractions of the rate the filter runs at.
        assert!(gain(0.0).abs() < 0.01);
        assert!(gain(0.2).abs() < 0.2, "passband sags {}", gain(0.2));
        for f in [0.3, 0.35, 0.4, 0.45, 0.5] {
            assert!(gain(f) < -80.0, "{f}: only {} dB down", gain(f));
        }
    }

    #[test]
    fn oversampling_passes_a_clean_signal_through_delayed() {
        let mut o = Oversample::new();
        let n = 4_800;
        let x: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 1000.0 * TAU / 48_000.0).sin() * 0.5)
            .collect();
        let y: Vec<f32> = x.iter().map(|&v| o.run([v, v], |s| s)[0]).collect();
        let d = Oversample::LATENCY;
        let err = (1_000..n)
            .map(|i| (y[i] - x[i - d]).abs())
            .fold(0.0f32, f32::max);
        assert!(err < 0.01, "differs by {err} after {d} frames");
    }
}
