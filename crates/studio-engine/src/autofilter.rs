//! Live's Auto Filter: a resonant filter whose cutoff an LFO and the
//! sound's own level (an envelope follower) move.
//!
//! The filters come from FunDSP: Andrew Simper's state-variable filter
//! (its coefficients, run here in f64 so low cutoffs stay clean) for the
//! Clean circuit, and FunDSP's Moog ladder for the Ladder circuit. The
//! cutoff glides and is recomputed every few frames, so knobs, the LFO and
//! the envelope all sweep it without zipper noise.

use fundsp::audionode::AudioNode;
use fundsp::moog::Moog;
use fundsp::svf::SvfCoefs;
use fundsp::typenum::U1;

use crate::devices::{from_db, Param, Unit};
use crate::fx;
use crate::kit::{synced_step, Follower, Lfo, Smooth, Wave};

pub const TYPES: &[&str] = &["Lowpass", "Highpass", "Bandpass", "Notch", "Morph"];
pub const CIRCUITS: &[&str] = &["Clean", "Ladder"];
const SLOPES: &[&str] = &["12 dB", "24 dB"];

/// Lowest and highest cutoff, Hz.
pub const FREQ: (f32, f32) = (20.0, 20_000.0);
/// How far the LFO and the envelope can move the cutoff at full amount, in
/// octaves either way.
const MOD_OCTAVES: f32 = 5.0;
/// Frames between cutoff updates.
const STRIDE: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct AutoFilterSettings {
    /// An index into [`TYPES`].
    pub kind: u8,
    /// An index into [`CIRCUITS`]. The ladder is a low-pass; other types
    /// use the clean filter.
    pub circuit: u8,
    /// 24 dB runs two filters in series.
    pub steep: bool,
    pub freq: f32,
    /// 0..1.25; past 1 the ladder rings on its own.
    pub resonance: f32,
    /// Morph's blend: 0 low-pass, 0.5 band-pass, 1 high-pass.
    pub morph: f32,
    /// Into the filter, dB, through a soft curve.
    pub drive_db: f32,
    /// -1..1, of [`MOD_OCTAVES`].
    pub env_amount: f32,
    pub env_attack_ms: f32,
    pub env_release_ms: f32,
    /// 0..1, of [`MOD_OCTAVES`].
    pub lfo_amount: f32,
    pub lfo_wave: Wave,
    pub lfo_sync: bool,
    pub lfo_hz: f32,
    /// The synced rate, as the nearest of [`fx::NOTES`].
    pub lfo_beats: f64,
    /// The right side's LFO runs this far (0..1 of a cycle) ahead.
    pub lfo_phase: f32,
    pub dry_wet: f32,
}

impl Default for AutoFilterSettings {
    fn default() -> Self {
        AutoFilterSettings {
            kind: 0,
            circuit: 0,
            steep: false,
            freq: 1_000.0,
            resonance: 0.3,
            morph: 0.0,
            drive_db: 0.0,
            env_amount: 0.0,
            env_attack_ms: 6.0,
            env_release_ms: 200.0,
            lfo_amount: 0.0,
            lfo_wave: Wave::Sine,
            lfo_sync: false,
            lfo_hz: 1.0,
            lfo_beats: 1.0,
            lfo_phase: 0.0,
            dry_wet: 1.0,
        }
    }
}

impl AutoFilterSettings {
    pub fn summary(&self) -> String {
        let mut parts = vec![
            TYPES[(self.kind as usize).min(TYPES.len() - 1)].to_string(),
            if self.freq >= 1000.0 {
                format!("{:.2} kHz", self.freq / 1000.0)
            } else {
                format!("{:.0} Hz", self.freq)
            },
        ];
        if self.lfo_amount > 0.0 {
            parts.push("LFO".into());
        }
        if self.env_amount != 0.0 {
            parts.push("envelope".into());
        }
        parts.join(" · ")
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        vec![
            Param::choice("Type", self.kind as usize, TYPES),
            Param::choice("Circuit", self.circuit as usize, CIRCUITS),
            Param::choice("Slope", self.steep as usize, SLOPES),
            Param::knob("Frequency", self.freq, FREQ.0, FREQ.1, Unit::Hz).log(),
            Param::knob("Resonance", self.resonance, 0.0, 1.25, Unit::Percent),
            Param::knob("Morph", self.morph, 0.0, 1.0, Unit::Percent),
            Param::knob("Drive", self.drive_db, 0.0, 24.0, Unit::Db),
            Param::knob("Env Amount", self.env_amount, -1.0, 1.0, Unit::Percent).bipolar(),
            Param::knob("Attack", self.env_attack_ms, 0.1, 100.0, Unit::Ms).log(),
            Param::knob("Release", self.env_release_ms, 1.0, 3000.0, Unit::Ms).log(),
            Param::knob("LFO Amount", self.lfo_amount, 0.0, 1.0, Unit::Percent),
            Param::choice("LFO Wave", self.lfo_wave.index(), Wave::NAMES),
            Param::toggle("Sync", self.lfo_sync),
            Param::knob("Rate", self.lfo_hz, 0.01, 40.0, Unit::Hz).log(),
            Param::notes("Rate (sync)", self.lfo_beats),
            Param::knob("Phase", self.lfo_phase, 0.0, 1.0, Unit::Percent),
            Param::knob("Dry/Wet", self.dry_wet, 0.0, 1.0, Unit::Percent),
        ]
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        let pick = |v: f32, n: usize| v.round().clamp(0.0, (n - 1) as f32) as u8;
        match i {
            0 => self.kind = pick(v, TYPES.len()),
            1 => self.circuit = pick(v, CIRCUITS.len()),
            2 => self.steep = v >= 0.5,
            3 => self.freq = v.clamp(FREQ.0, FREQ.1),
            4 => self.resonance = v.clamp(0.0, 1.25),
            5 => self.morph = v.clamp(0.0, 1.0),
            6 => self.drive_db = v.clamp(0.0, 24.0),
            7 => self.env_amount = v.clamp(-1.0, 1.0),
            8 => self.env_attack_ms = v.clamp(0.1, 100.0),
            9 => self.env_release_ms = v.clamp(1.0, 3000.0),
            10 => self.lfo_amount = v.clamp(0.0, 1.0),
            11 => self.lfo_wave = Wave::from_index(v.round().max(0.0) as usize),
            12 => self.lfo_sync = v >= 0.5,
            13 => self.lfo_hz = v.clamp(0.01, 40.0),
            14 => {
                self.lfo_beats = fx::NOTES[(v.round().max(0.0) as usize).min(fx::NOTES.len() - 1)]
            }
            15 => self.lfo_phase = v.clamp(0.0, 1.0),
            16 => self.dry_wet = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    /// The Clean filter's mix of its low, band and high outputs: (input,
    /// band, low), as Simper's m0, m1 (times k) and m2.
    fn mix(&self, k: f64) -> (f64, f64, f64) {
        match self.kind {
            0 => (0.0, 0.0, 1.0),
            1 => (1.0, -k, -1.0),
            // Band-pass with its peak at unity whatever the resonance.
            2 => (0.0, k, 0.0),
            3 => (1.0, -k, 0.0),
            _ => {
                // Low (0) to band (0.5) to high (1).
                let m = self.morph.clamp(0.0, 1.0) as f64;
                let (lo, band, hi) = if m < 0.5 {
                    (1.0 - m * 2.0, m * 2.0, 0.0)
                } else {
                    (0.0, 2.0 - m * 2.0, m * 2.0 - 1.0)
                };
                // high = x - k·band - low.
                (hi, band * k - hi * k, lo - hi)
            }
        }
    }
}

/// One side's filters: two SVF stages (state in f64) and two ladders.
#[derive(Clone)]
struct Side {
    svf: [[f64; 2]; 2],
    ladder: [Moog<f64, U1>; 2],
}

pub struct AutoFilter {
    pub(crate) s: AutoFilterSettings,
    rate: f64,
    /// The knob's cutoff, as octaves above [`FREQ`]'s low end.
    octave: Smooth,
    resonance: Smooth,
    drive: Smooth,
    wet: Smooth,
    env_amount: Smooth,
    lfo_amount: Smooth,
    lfo: Lfo,
    env: Follower,
    sides: [Side; 2],
    coefs: [SvfCoefs<f64>; 2],
    /// The clean filter's damping, 1/Q.
    k: f64,
    /// Frames until the cutoff is worked out again.
    count: usize,
}

fn octaves(freq: f32) -> f32 {
    (freq.max(FREQ.0) / FREQ.0).log2()
}

impl AutoFilter {
    pub fn new(s: &AutoFilterSettings, rate: f64) -> AutoFilter {
        let mut ladder = Moog::new(1_000.0, 0.0);
        ladder.set_sample_rate(rate);
        let side = Side {
            svf: [[0.0; 2]; 2],
            ladder: [ladder.clone(), ladder],
        };
        let mut a = AutoFilter {
            s: s.clone(),
            rate,
            octave: Smooth::knob(octaves(s.freq), rate),
            resonance: Smooth::knob(s.resonance, rate),
            drive: Smooth::knob(from_db(s.drive_db), rate),
            wet: Smooth::knob(s.dry_wet, rate),
            env_amount: Smooth::knob(s.env_amount, rate),
            lfo_amount: Smooth::knob(s.lfo_amount, rate),
            lfo: Lfo::new(s.lfo_wave, 0x5eed),
            env: Follower::new(s.env_attack_ms, s.env_release_ms, rate),
            sides: [side.clone(), side],
            coefs: [SvfCoefs::default(), SvfCoefs::default()],
            k: 2.0,
            count: 0,
        };
        a.retune();
        a
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.retune();
    }

    fn retune(&mut self) {
        self.octave.set(octaves(self.s.freq));
        self.resonance.set(self.s.resonance);
        self.drive.set(from_db(self.s.drive_db));
        self.wet.set(self.s.dry_wet);
        self.env_amount.set(self.s.env_amount);
        self.lfo_amount.set(self.s.lfo_amount);
        self.lfo.wave = self.s.lfo_wave;
        self.env
            .set_times(self.s.env_attack_ms, self.s.env_release_ms, self.rate);
        // Work the cutoff out again on the next frame.
        self.count = 0;
    }

    /// The cutoffs (left, right) now, Hz, with the LFO and envelope.
    fn cutoffs(&self, octave: f32, env_amount: f32, lfo_amount: f32) -> [f32; 2] {
        let lfo = lfo_amount * MOD_OCTAVES;
        let env = env_amount * MOD_OCTAVES * self.env.level().min(1.0);
        let top = self.rate as f32 * 0.45;
        let at = |offset: f64| {
            let o = octave + env + lfo * self.lfo.at(offset);
            (FREQ.0 * o.exp2()).clamp(FREQ.0, top.min(FREQ.1 * 1.2))
        };
        [at(0.0), at(self.s.lfo_phase as f64)]
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]], bpm: f64) {
        let step = if self.s.lfo_sync {
            synced_step(self.s.lfo_beats, bpm, self.rate)
        } else {
            self.s.lfo_hz as f64 / self.rate
        };
        let ladder = self.s.circuit == 1 && self.s.kind == 0;
        for f in buf.iter_mut() {
            let octave = self.octave.tick();
            let res = self.resonance.tick();
            let drive = self.drive.tick();
            let wet = self.wet.tick();
            let env_amount = self.env_amount.tick();
            let lfo_amount = self.lfo_amount.tick();
            self.env.next(f[0].abs().max(f[1].abs()));
            self.lfo.advance(step);
            if self.count == 0 {
                self.count = STRIDE;
                let cut = self.cutoffs(octave, env_amount, lfo_amount);
                // Resonance 0..1.25 to Q 0.5..~25 (clean) or 0..1.1
                // (ladder, which rings on its own near 1).
                let q = 0.5 * (50f32).powf(res / 1.25);
                self.k = 1.0 / q as f64;
                for (side, c) in cut.iter().enumerate() {
                    if ladder {
                        for l in &mut self.sides[side].ladder {
                            l.set_cutoff_q(*c as f64, (res as f64 * 0.88).min(1.1));
                        }
                    } else {
                        self.coefs[side] = SvfCoefs::lowpass(self.rate, *c as f64, q as f64);
                    }
                }
            }
            self.count -= 1;
            let stages = if self.s.steep { 2 } else { 1 };
            let mut out = [0.0f32; 2];
            for side in 0..2 {
                let mut x = f[side] as f64;
                if drive > 1.0 {
                    x = (x * drive as f64).tanh() / (drive as f64).tanh().max(1e-6);
                }
                if ladder {
                    for l in self.sides[side].ladder[..stages].iter_mut() {
                        x = l.tick(&[x as f32].into())[0] as f64;
                    }
                } else {
                    let c = &self.coefs[side];
                    let (m0, m1, m2) = self.s.mix(self.k);
                    for st in self.sides[side].svf[..stages].iter_mut() {
                        let v3 = x - st[1];
                        let v1 = c.a1 * st[0] + c.a2 * v3;
                        let v2 = st[1] + c.a2 * st[0] + c.a3 * v3;
                        st[0] = 2.0 * v1 - st[0];
                        st[1] = 2.0 * v2 - st[1];
                        x = m0 * x + m1 * v1 + m2 * v2;
                    }
                }
                out[side] = x as f32;
            }
            *f = [
                f[0] * (1.0 - wet) + out[0] * wet,
                f[1] * (1.0 - wet) + out[1] * wet,
            ];
        }
    }

    /// The filter's response for its panel's graph, at `freq`, dB (with
    /// the cutoff where the knob sets it).
    pub fn response_db(s: &AutoFilterSettings, freq: f64) -> f32 {
        let q = 0.5 * 50f64.powf(s.resonance as f64 / 1.25);
        let k = 1.0 / q;
        let w = freq / s.freq as f64;
        // The analog prototype: s = jw, denominator s² + k·s + 1.
        let (dr, di) = (1.0 - w * w, k * w);
        let den = dr * dr + di * di;
        let (m0, m1, m2) = s.mix(k);
        // Out = m0·1 + m1·(s / D) + m2·(1 / D).
        let (lr, li) = (dr / den, -di / den);
        let (br, bi) = (-w * li, w * lr);
        let (re, im) = (m0 + m1 * br + m2 * lr, m1 * bi + m2 * li);
        let one = 10.0 * (re * re + im * im).max(1e-12).log10();
        (one * if s.steep { 2.0 } else { 1.0 }) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(s: &AutoFilterSettings, freq: f32) -> f32 {
        let mut a = AutoFilter::new(s, 48_000.0);
        let mut buf: Vec<[f32; 2]> = (0..24_000)
            .map(|i| {
                let v = (i as f32 * freq * std::f32::consts::TAU / 48_000.0).sin() * 0.25;
                [v, v]
            })
            .collect();
        a.process(&mut buf, 120.0);
        let peak = buf[12_000..].iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
        20.0 * (peak / 0.25).log10()
    }

    #[test]
    fn each_type_passes_and_cuts_where_it_should() {
        let s = |kind| AutoFilterSettings {
            kind,
            freq: 1_000.0,
            resonance: 0.0,
            ..Default::default()
        };
        // Low-pass.
        assert!(level(&s(0), 100.0) > -1.0);
        assert!(level(&s(0), 10_000.0) < -30.0);
        // High-pass.
        assert!(level(&s(1), 100.0) < -30.0);
        assert!(level(&s(1), 10_000.0) > -1.0);
        // Band-pass: unity at the centre, down either side.
        assert!(level(&s(2), 1_000.0) > -1.0);
        assert!(level(&s(2), 100.0) < -12.0);
        // Notch.
        assert!(level(&s(3), 1_000.0) < -30.0);
        assert!(level(&s(3), 100.0) > -1.0);
        // The ladder low-pass.
        let ladder = AutoFilterSettings { circuit: 1, ..s(0) };
        assert!(level(&ladder, 100.0) > -3.0);
        assert!(level(&ladder, 10_000.0) < -30.0);
    }

    #[test]
    fn the_graph_matches_what_it_plays() {
        for kind in 0..4 {
            let s = AutoFilterSettings {
                kind,
                resonance: 0.4,
                ..Default::default()
            };
            for f in [200.0, 1_000.0, 3_000.0] {
                let (heard, drawn) = (level(&s, f), AutoFilter::response_db(&s, f as f64));
                if drawn > -30.0 {
                    assert!(
                        (heard - drawn).abs() < 1.5,
                        "{kind} at {f}: {heard} vs {drawn}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_lfo_sweeps_the_cutoff() {
        let s = AutoFilterSettings {
            freq: 500.0,
            resonance: 0.0,
            lfo_amount: 0.6,
            lfo_hz: 2.0,
            ..Default::default()
        };
        let mut a = AutoFilter::new(&s, 48_000.0);
        // Steady 2 kHz: the sweep opens and closes on it, so its level
        // moves a lot over a second.
        let mut buf: Vec<[f32; 2]> = (0..48_000)
            .map(|i| {
                let v = (i as f32 * 2_000.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.25;
                [v, v]
            })
            .collect();
        a.process(&mut buf, 120.0);
        let peaks: Vec<f32> = buf
            .chunks(1_200)
            .map(|c| c.iter().fold(0.0f32, |m, f| m.max(f[0].abs())))
            .collect();
        let (lo, hi) = peaks
            .iter()
            .fold((f32::MAX, 0.0f32), |(l, h), &p| (l.min(p), h.max(p)));
        assert!(hi / lo.max(1e-6) > 4.0, "barely moves: {lo}..{hi}");
    }
}
