//! Room EQ: six bands correcting the room an output plays into, one for the
//! main output and one for the booth. Recordings, broadcasts and the
//! headphones never hear it (see `Engine::process`).
//!
//! The bands are a low cut, a low shelf, two bells, a high shelf and a high
//! cut. A change glides over about 20 ms, so dragging a band doesn't zip,
//! and a band that changes nothing is skipped: a flat Room EQ costs nothing.

use crate::dsp::Biquad;
use opendj_control::{Behavior, ControlHandle, ControlRegistry};

/// Bands in a Room EQ.
pub const BANDS: usize = 6;

/// What a band does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    LowCut,
    LowShelf,
    Bell,
    HighShelf,
    HighCut,
}

impl Kind {
    /// Low and high cuts have a slope instead of a gain.
    pub fn is_cut(self) -> bool {
        matches!(self, Kind::LowCut | Kind::HighCut)
    }
}

/// Each band's kind, in order.
pub const KINDS: [Kind; BANDS] = [
    Kind::LowCut,
    Kind::LowShelf,
    Kind::Bell,
    Kind::Bell,
    Kind::HighShelf,
    Kind::HighCut,
];

pub const MIN_FREQ: f64 = 20.0;
pub const MAX_FREQ: f64 = 20_000.0;
/// Shelves and bells boost or cut up to this, in dB.
pub const MAX_GAIN: f64 = 12.0;
/// A bell's width, as Q: broad to narrow.
pub const MIN_Q: f64 = 0.3;
pub const MAX_Q: f64 = 8.0;
/// The cuts' slopes, in dB per octave.
pub const SLOPES: [f64; 2] = [12.0, 24.0];

/// The shelves' fixed slope: the steepest without a bump.
const SHELF_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// A 24 dB/octave cut is a fourth-order Butterworth: two biquads with these Qs.
const BUTTERWORTH4_Q: [f64; 2] = [0.541_196_1, 1.306_563];

/// One band's setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub on: bool,
    /// Hz.
    pub freq: f64,
    /// dB, for shelves and bells.
    pub gain: f64,
    /// A bell's Q, or a cut's slope in dB/octave (12 or 24); shelves ignore it.
    pub q: f64,
}

impl Band {
    /// The band with every value in its range.
    pub fn clamped(self, kind: Kind) -> Band {
        Band {
            on: self.on,
            freq: self.freq.clamp(MIN_FREQ, MAX_FREQ),
            gain: if kind.is_cut() {
                0.0
            } else {
                self.gain.clamp(-MAX_GAIN, MAX_GAIN)
            },
            q: match kind {
                _ if kind.is_cut() => {
                    if self.q > 18.0 {
                        24.0
                    } else {
                        12.0
                    }
                }
                Kind::Bell => self.q.clamp(MIN_Q, MAX_Q),
                _ => SHELF_Q,
            },
        }
    }

    /// Whether the band changes the sound.
    pub fn audible(&self, kind: Kind) -> bool {
        self.on && (kind.is_cut() || self.gain.abs() >= 0.01)
    }
}

/// Every band at rest: the cuts off at the ends, the rest at 0 dB.
pub const FLAT: [Band; BANDS] = [
    Band {
        on: false,
        freq: 30.0,
        gain: 0.0,
        q: 24.0,
    },
    Band {
        on: true,
        freq: 100.0,
        gain: 0.0,
        q: SHELF_Q,
    },
    Band {
        on: true,
        freq: 250.0,
        gain: 0.0,
        q: 1.0,
    },
    Band {
        on: true,
        freq: 2500.0,
        gain: 0.0,
        q: 1.0,
    },
    Band {
        on: true,
        freq: 8000.0,
        gain: 0.0,
        q: SHELF_Q,
    },
    Band {
        on: false,
        freq: 18_000.0,
        gain: 0.0,
        q: 12.0,
    },
];

/// What a band's filters are set to: frequency (as octaves above 1 Hz, so
/// a glide is even across the spectrum), gain, Q, how much of a cut is
/// heard, and how much of a cut's second biquad.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Params {
    octave: f64,
    gain: f64,
    q: f64,
    mix: f64,
    mix2: f64,
}

impl Params {
    fn of(kind: Kind, band: Band, enabled: bool) -> Params {
        let band = band.clamped(kind);
        let on = enabled && band.on;
        let steep = band.q > 18.0;
        Params {
            octave: band.freq.log2(),
            gain: if on { band.gain } else { 0.0 },
            q: match kind {
                Kind::Bell => band.q,
                _ if kind.is_cut() && steep => BUTTERWORTH4_Q[0],
                _ => SHELF_Q,
            },
            mix: if kind.is_cut() && !on { 0.0 } else { 1.0 },
            mix2: if kind.is_cut() && steep { 1.0 } else { 0.0 },
        }
    }

    /// Changes nothing, so the band can be skipped.
    fn idle(&self, kind: Kind) -> bool {
        if kind.is_cut() {
            self.mix == 0.0
        } else {
            self.gain == 0.0
        }
    }

    fn set(&self, kind: Kind, sample_rate: f64, f: &mut [Biquad; 2]) {
        let freq = self.octave.exp2();
        match kind {
            Kind::LowCut => {
                f[0].set_highpass(sample_rate, freq, self.q);
                f[1].set_highpass(sample_rate, freq, BUTTERWORTH4_Q[1]);
            }
            Kind::HighCut => {
                f[0].set_lowpass(sample_rate, freq, self.q);
                f[1].set_lowpass(sample_rate, freq, BUTTERWORTH4_Q[1]);
            }
            Kind::LowShelf => f[0].set_low_shelf(sample_rate, freq, self.q, self.gain),
            Kind::Bell => f[0].set_peaking(sample_rate, freq, self.q, self.gain),
            Kind::HighShelf => f[0].set_high_shelf(sample_rate, freq, self.q, self.gain),
        }
    }

    /// The gain at `freq`, in dB.
    fn response(&self, kind: Kind, sample_rate: f64, freq: f64) -> f64 {
        if self.idle(kind) {
            return 0.0;
        }
        let mut f = [Biquad::default(); 2];
        self.set(kind, sample_rate, &mut f);
        let mut db = f[0].magnitude_db(sample_rate, freq);
        if kind.is_cut() && self.mix2 > 0.5 {
            db += f[1].magnitude_db(sample_rate, freq);
        }
        db
    }
}

/// Frames between coefficient updates while a band glides.
const STEP: usize = 32;
/// The glides' time constant, in seconds: about 98% there after 20 ms.
const GLIDE: f64 = 0.005;

#[derive(Clone, Copy, Debug)]
struct Live {
    kind: Kind,
    target: Params,
    now: Params,
    filters: [Biquad; 2],
    /// Its filters run: it changes the sound, or is gliding.
    running: bool,
}

impl Live {
    /// Glides one step towards the target; false once it's idle.
    fn step(&mut self, k: f64, sample_rate: f64) -> bool {
        let (t, n) = (self.target, &mut self.now);
        if n.idle(self.kind) && t.idle(self.kind) {
            // Nothing to hear: jump, and start clean when heard again.
            *n = t;
            if self.running {
                self.filters.iter_mut().for_each(Biquad::reset);
                self.running = false;
            }
            return false;
        }
        let glide = |now: &mut f64, to: f64, eps: f64| {
            *now += (to - *now) * k;
            if (to - *now).abs() < eps {
                *now = to;
            }
        };
        glide(&mut n.octave, t.octave, 1e-4);
        glide(&mut n.gain, t.gain, 1e-3);
        glide(&mut n.q, t.q, 1e-4);
        glide(&mut n.mix, t.mix, 1e-4);
        let had_second = n.mix2 > 0.0;
        glide(&mut n.mix2, t.mix2, 1e-4);
        if had_second && n.mix2 == 0.0 {
            self.filters[1].reset();
        }
        n.set(self.kind, sample_rate, &mut self.filters);
        self.running = !(n.idle(self.kind) && t.idle(self.kind));
        if !self.running {
            self.filters.iter_mut().for_each(Biquad::reset);
        }
        self.running
    }

    #[inline]
    fn process(&mut self, x: [f32; 2]) -> [f32; 2] {
        let y = self.filters[0].process(x);
        if !self.kind.is_cut() {
            return y;
        }
        let m2 = self.now.mix2 as f32;
        let y = if m2 > 0.0 {
            let y2 = self.filters[1].process(y);
            [0, 1].map(|c| y[c] + m2 * (y2[c] - y[c]))
        } else {
            y
        };
        let m = self.now.mix as f32;
        [0, 1].map(|c| x[c] + m * (y[c] - x[c]))
    }
}

/// A Room EQ as the audio thread runs it.
pub struct RoomEq {
    sample_rate: f64,
    glide: f64,
    enabled: bool,
    bands: [Band; BANDS],
    live: [Live; BANDS],
    /// Frames until the next glide step.
    left: usize,
    /// Some band runs or must be stepped.
    busy: bool,
}

impl RoomEq {
    pub fn new(sample_rate: f64) -> Self {
        let live = std::array::from_fn(|i| {
            let p = Params::of(KINDS[i], FLAT[i], false);
            Live {
                kind: KINDS[i],
                target: p,
                now: p,
                filters: [Biquad::default(); 2],
                running: false,
            }
        });
        Self {
            sample_rate,
            glide: 1.0 - (-(STEP as f64) / (GLIDE * sample_rate)).exp(),
            enabled: false,
            bands: FLAT,
            live,
            left: 0,
            busy: false,
        }
    }

    /// Takes new settings; the sound glides to them.
    pub fn set(&mut self, enabled: bool, bands: &[Band; BANDS]) {
        if enabled == self.enabled && *bands == self.bands {
            return;
        }
        self.enabled = enabled;
        self.bands = *bands;
        let mut busy = false;
        for (i, l) in self.live.iter_mut().enumerate() {
            l.target = Params::of(KINDS[i], bands[i], enabled);
            if l.running || !l.target.idle(l.kind) {
                busy = true;
            } else {
                // Silent before and after: nothing to glide.
                l.now = l.target;
            }
        }
        if busy {
            self.busy = true;
            self.left = 0;
        }
    }

    /// Whether it changes the sound now or is about to: off and flat, the
    /// audio passes untouched.
    pub fn is_active(&self) -> bool {
        self.busy
    }

    #[inline]
    pub fn process(&mut self, x: [f32; 2]) -> [f32; 2] {
        if !self.busy {
            return x;
        }
        if self.left == 0 {
            let mut any = false;
            for l in &mut self.live {
                any |= l.step(self.glide, self.sample_rate);
            }
            self.busy = any;
            self.left = STEP;
        }
        self.left -= 1;
        let mut y = x;
        for l in &mut self.live {
            if l.running {
                y = l.process(y);
            }
        }
        y
    }
}

/// The Room EQ's gain at each of `freqs`, in dB, once settled: the curve
/// the settings page draws.
pub fn response(bands: &[Band; BANDS], sample_rate: f64, freqs: &[f64]) -> Vec<f64> {
    let params: Vec<Params> = (0..BANDS)
        .map(|i| Params::of(KINDS[i], bands[i], true))
        .collect();
    freqs
        .iter()
        .map(|&f| {
            params
                .iter()
                .zip(KINDS)
                .map(|(p, k)| p.response(k, sample_rate, f))
                .sum()
        })
        .collect()
}

/// `n` frequencies evenly spaced on a log axis from 20 Hz to 20 kHz.
pub fn log_freqs(n: usize) -> Vec<f64> {
    let (lo, hi) = (MIN_FREQ.log2(), MAX_FREQ.log2());
    (0..n)
        .map(|i| (lo + (hi - lo) * i as f64 / (n.max(2) - 1) as f64).exp2())
        .collect()
}

/// The control groups of the two Room EQs.
pub const MAIN_GROUP: &str = "[RoomEQ_Main]";
pub const BOOTH_GROUP: &str = "[RoomEQ_Booth]";

/// One Room EQ's controls: `enabled` and `b1_on`, `b1_freq`, `b1_gain`,
/// `b1_q` to `b6_*`, in the order of [`KINDS`].
pub(crate) struct RoomEqControls {
    enabled: ControlHandle,
    bands: [[ControlHandle; 4]; BANDS],
}

impl RoomEqControls {
    pub(crate) fn create(reg: &ControlRegistry, group: &str) -> Self {
        let enabled = reg.toggle(group, "enabled");
        let bands = std::array::from_fn(|i| {
            let (kind, flat) = (KINDS[i], FLAT[i]);
            let key = |k: &str| format!("b{}_{k}", i + 1);
            let on = reg.toggle(group, &key("on"));
            on.set_bool(flat.on);
            let (q_min, q_max) = if kind.is_cut() {
                (SLOPES[0], SLOPES[1])
            } else {
                (MIN_Q, MAX_Q)
            };
            [
                on,
                reg.create(
                    group,
                    &key("freq"),
                    flat.freq,
                    MIN_FREQ,
                    MAX_FREQ,
                    Behavior::Free,
                ),
                reg.create(
                    group,
                    &key("gain"),
                    0.0,
                    -MAX_GAIN,
                    MAX_GAIN,
                    Behavior::Free,
                ),
                reg.create(group, &key("q"), flat.q, q_min, q_max, Behavior::Free),
            ]
        });
        Self { enabled, bands }
    }

    /// The settings as the controls hold them now.
    pub(crate) fn read(&self) -> (bool, [Band; BANDS]) {
        (
            self.enabled.get_bool(),
            std::array::from_fn(|i| {
                let [on, freq, gain, q] = &self.bands[i];
                Band {
                    on: on.get_bool(),
                    freq: freq.get(),
                    gain: gain.get(),
                    q: q.get(),
                }
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn sine(hz: f64, frames: usize) -> impl Iterator<Item = [f32; 2]> {
        (0..frames).map(move |i| {
            let v = (std::f64::consts::TAU * hz * i as f64 / SR).sin() as f32 * 0.5;
            [v, v]
        })
    }

    /// The level change at `hz` once settled, in dB, measured on audio.
    fn measured_db(eq: &mut RoomEq, hz: f64) -> f64 {
        let n = (SR * 0.5) as usize;
        let (mut inp, mut out) = (0.0f64, 0.0f64);
        for (i, x) in sine(hz, n).enumerate() {
            let y = eq.process(x);
            if i > n / 2 {
                inp += (x[0] as f64).powi(2);
                out += (y[0] as f64).powi(2);
            }
        }
        10.0 * (out / inp).log10()
    }

    fn with(i: usize, band: Band) -> [Band; BANDS] {
        let mut b = FLAT;
        b[i] = band;
        b
    }

    #[test]
    fn flat_or_off_passes_the_audio_untouched() {
        let mut eq = RoomEq::new(SR);
        eq.set(true, &FLAT);
        let input: Vec<_> = sine(440.0, 4800).collect();
        for x in &input {
            assert_eq!(eq.process(*x), *x);
        }
        assert!(!eq.is_active());
        // Off with a cut: also untouched, once glided away.
        let bands = with(
            2,
            Band {
                gain: -6.0,
                ..FLAT[2]
            },
        );
        eq.set(false, &bands);
        assert!(!eq.is_active());
        eq.set(true, &bands);
        assert!(eq.is_active());
        eq.set(false, &bands);
        for x in sine(440.0, 4800) {
            eq.process(x);
        }
        assert!(!eq.is_active());
        for x in &input {
            assert_eq!(eq.process(*x), *x);
        }
        assert!(response(&FLAT, SR, &log_freqs(64))
            .iter()
            .all(|&d| d == 0.0));
    }

    #[test]
    fn each_band_matches_its_curve() {
        let cases = [
            (
                0,
                Band {
                    on: true,
                    freq: 100.0,
                    ..FLAT[0]
                },
                30.0,
            ),
            (
                1,
                Band {
                    gain: 6.0,
                    ..FLAT[1]
                },
                40.0,
            ),
            (
                2,
                Band {
                    gain: -3.0,
                    ..FLAT[2]
                },
                250.0,
            ),
            (
                3,
                Band {
                    gain: 4.0,
                    q: 4.0,
                    ..FLAT[3]
                },
                2500.0,
            ),
            (
                4,
                Band {
                    gain: -4.0,
                    ..FLAT[4]
                },
                14_000.0,
            ),
            (
                5,
                Band {
                    on: true,
                    freq: 2000.0,
                    q: 12.0,
                    ..FLAT[5]
                },
                6000.0,
            ),
        ];
        for (i, band, hz) in cases {
            let bands = with(i, band);
            let mut eq = RoomEq::new(SR);
            eq.set(true, &bands);
            let got = measured_db(&mut eq, hz);
            let curve = response(&bands, SR, &[hz])[0];
            assert!(curve.abs() > 2.0, "band {i} does something: {curve}");
            assert!((got - curve).abs() < 0.3, "band {i}: {got} vs {curve}");
            // Far from the band, nothing changes.
            if (2..4).contains(&i) {
                let far = response(&bands, SR, &[hz * 16.0])[0];
                assert!(far.abs() < 0.3, "band {i} at {}: {far}", hz * 16.0);
            }
        }
        // The shelves hold their gain past the corner.
        let shelf = with(
            1,
            Band {
                gain: -6.0,
                ..FLAT[1]
            },
        );
        let r = response(&shelf, SR, &[25.0, 10_000.0]);
        assert!((r[0] + 6.0).abs() < 0.3 && r[1].abs() < 0.1, "{r:?}");
    }

    #[test]
    fn a_steeper_cut_cuts_twice_as_much_an_octave_down() {
        let at = |slope| {
            let b = with(
                0,
                Band {
                    on: true,
                    freq: 200.0,
                    gain: 0.0,
                    q: slope,
                },
            );
            response(&b, SR, &[50.0])[0]
        };
        let (gentle, steep) = (at(12.0), at(24.0));
        // Two octaves below: about -24 and -48 dB.
        assert!((gentle + 24.0).abs() < 1.5, "{gentle}");
        assert!((steep + 48.0).abs() < 2.0, "{steep}");
    }

    #[test]
    fn a_sudden_change_glides_without_a_click() {
        let mut eq = RoomEq::new(SR);
        eq.set(true, &FLAT);
        let tone = 1000.0;
        // The largest step a 0.5 sine makes from one sample to the next.
        let max_step = 0.5 * std::f64::consts::TAU * tone / SR;
        let mut last = 0.0f32;
        let mut worst = 0.0f32;
        let mut sweep = with(
            3,
            Band {
                gain: -12.0,
                freq: 1000.0,
                q: 2.0,
                ..FLAT[3]
            },
        );
        for (i, x) in sine(tone, (SR * 0.6) as usize).enumerate() {
            match i {
                4800 => eq.set(true, &sweep),
                9600 => {
                    // Off and on again, then the cuts in at once.
                    sweep[0] = Band {
                        on: true,
                        freq: 900.0,
                        ..FLAT[0]
                    };
                    sweep[5] = Band {
                        on: true,
                        freq: 1100.0,
                        ..FLAT[5]
                    };
                    eq.set(true, &sweep);
                }
                14_400 => eq.set(false, &sweep),
                _ => {}
            }
            let y = eq.process(x)[0];
            if i > 0 {
                worst = worst.max((y - last).abs());
            }
            last = y;
        }
        assert!((worst as f64) < max_step * 1.05, "{worst} vs {max_step}");
        // And it settles: idle again once off.
        assert!(!eq.is_active());
    }

    #[test]
    fn the_curve_is_the_bands_together() {
        let mut bands = FLAT;
        bands[1].gain = -3.0;
        bands[2].gain = -3.0;
        let freqs = log_freqs(256);
        assert_eq!(freqs.len(), 256);
        assert!((freqs[0] - MIN_FREQ).abs() < 1e-9 && (freqs[255] - MAX_FREQ).abs() < 1e-6);
        let both = response(&bands, SR, &freqs);
        let one = |i: usize| response(&with(i, bands[i]), SR, &freqs);
        let (a, b) = (one(1), one(2));
        for k in 0..freqs.len() {
            assert!((both[k] - a[k] - b[k]).abs() < 1e-9);
        }
    }

    /// What a Room EQ costs with every band on, and the recording's own
    /// limiter beside it, as a share of one core in real time. Run with
    /// `cargo test --release -p opendj-engine room_eq_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn room_eq_cost() {
        let mut bands = FLAT;
        for b in &mut bands {
            b.on = true;
            b.gain = -2.0;
        }
        let mut eq = RoomEq::new(SR);
        eq.set(true, &bands);
        let mut limiter = crate::dsp::Limiter::new(SR, 0.9);
        let input: Vec<_> = sine(440.0, SR as usize).collect();
        let time = |f: &mut dyn FnMut([f32; 2]) -> [f32; 2]| {
            let mut best = f64::MAX;
            for _ in 0..10 {
                let start = std::time::Instant::now();
                let mut sum = 0.0f32;
                for x in &input {
                    sum += f(*x)[0];
                }
                std::hint::black_box(sum);
                best = best.min(start.elapsed().as_secs_f64());
            }
            best * 100.0
        };
        let eq_cost = time(&mut |x| eq.process(x));
        let limiter_cost = time(&mut |x| limiter.process(x));
        println!(
            "one second of audio: six bands {eq_cost:.3}% of a core, \
             the recording's limiter {limiter_cost:.3}%"
        );
    }
}
