//! Live's Saturator: drives the sound into a curve that rounds or folds
//! its peaks. The curve runs at four times the rate (see
//! [`crate::kit::Oversample`]), so the harmonics it adds above the audio
//! band are filtered out instead of folding back down as aliasing. The dry
//! sound is delayed to match, so Dry/Wet mixes without comb filtering.

use std::f32::consts::FRAC_PI_2;

use opendj_engine::dsp::Biquad;

use crate::devices::{from_db, Param, Unit};
use crate::kit::{Oversample, Smooth};

/// Live's curve types, in its order.
pub const CURVES: &[&str] = &[
    "Analog Clip",
    "Soft Sine",
    "Medium Curve",
    "Hard Curve",
    "Sinoid Fold",
    "Digital Clip",
];

#[derive(Clone, Debug, PartialEq)]
pub struct SaturatorSettings {
    /// Into the curve, dB.
    pub drive_db: f32,
    /// An index into [`CURVES`].
    pub curve: u8,
    /// After the curve, dB.
    pub output_db: f32,
    /// 0..1.
    pub dry_wet: f32,
    /// Takes off DC (and rumble under 10 Hz) before the curve, so an
    /// offset doesn't make it lopsided.
    pub dc: bool,
    /// Rounds the output's peaks, so it never goes over 0 dB.
    pub soft_clip: bool,
}

impl Default for SaturatorSettings {
    fn default() -> Self {
        SaturatorSettings {
            drive_db: 0.0,
            curve: 0,
            output_db: 0.0,
            dry_wet: 1.0,
            dc: true,
            soft_clip: false,
        }
    }
}

impl SaturatorSettings {
    pub fn summary(&self) -> String {
        format!(
            "{} · drive {:+.1} dB · {:.0}% wet",
            CURVES[(self.curve as usize).min(CURVES.len() - 1)],
            self.drive_db,
            self.dry_wet * 100.0
        )
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        vec![
            Param::knob("Drive", self.drive_db, -36.0, 36.0, Unit::Db).bipolar(),
            Param::choice("Curve", self.curve as usize, CURVES),
            Param::knob("Output", self.output_db, -36.0, 0.0, Unit::Db),
            Param::knob("Dry/Wet", self.dry_wet, 0.0, 1.0, Unit::Percent),
            Param::toggle("DC", self.dc),
            Param::toggle("Soft Clip", self.soft_clip),
        ]
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.drive_db = v.clamp(-36.0, 36.0),
            1 => self.curve = v.round().clamp(0.0, (CURVES.len() - 1) as f32) as u8,
            2 => self.output_db = v.clamp(-36.0, 0.0),
            3 => self.dry_wet = v.clamp(0.0, 1.0),
            4 => self.dc = v >= 0.5,
            5 => self.soft_clip = v >= 0.5,
            _ => {}
        }
    }
}

/// The curve, each with a slope of 1 at the middle so the drive alone sets
/// how hard it bites.
#[inline]
pub fn curve(kind: u8, x: f32) -> f32 {
    match kind {
        // Analog Clip: a smooth tanh.
        0 => x.tanh(),
        // Soft Sine: a quarter sine, flat past it.
        1 => x.clamp(-FRAC_PI_2, FRAC_PI_2).sin(),
        // Medium Curve: an algebraic sigmoid, gentler than tanh.
        2 => x / (1.0 + x * x).sqrt(),
        // Hard Curve: a cubic up to the knee, flat past it.
        3 => {
            let k = 1.5;
            let y = x / k;
            if y.abs() >= 1.0 {
                y.signum()
            } else {
                1.5 * y - 0.5 * y * y * y
            }
        }
        // Sinoid Fold: folds back past the top instead of flattening.
        4 => (x * FRAC_PI_2).sin() / FRAC_PI_2,
        // Digital Clip: straight up to 0 dB, then flat.
        _ => x.clamp(-1.0, 1.0),
    }
}

pub struct Saturator {
    pub(crate) s: SaturatorSettings,
    drive: Smooth,
    output: Smooth,
    wet: Smooth,
    over: Oversample,
    dc: [Biquad; 1],
    /// The dry sound, delayed as long as the oversampling takes.
    dry: [[f32; 2]; Oversample::LATENCY],
    at: usize,
}

impl Saturator {
    pub fn new(s: &SaturatorSettings, rate: f64) -> Saturator {
        let mut dc = Biquad::default();
        dc.set_highpass(rate, 10.0, 0.707);
        Saturator {
            s: s.clone(),
            drive: Smooth::knob(from_db(s.drive_db), rate),
            output: Smooth::knob(from_db(s.output_db), rate),
            wet: Smooth::knob(s.dry_wet, rate),
            over: Oversample::new(),
            dc: [dc],
            dry: [[0.0; 2]; Oversample::LATENCY],
            at: 0,
        }
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.drive.set(from_db(self.s.drive_db));
        self.output.set(from_db(self.s.output_db));
        self.wet.set(self.s.dry_wet);
    }

    /// Frames of delay the oversampling adds.
    pub fn latency(&self) -> usize {
        Oversample::LATENCY
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        let kind = self.s.curve;
        for f in buf.iter_mut() {
            let drive = self.drive.tick();
            let out = self.output.tick();
            let wet = self.wet.tick();
            let x = if self.s.dc {
                self.dc[0].process(*f)
            } else {
                *f
            };
            let y = self.over.run(x, |s| {
                [curve(kind, s[0] * drive), curve(kind, s[1] * drive)]
            });
            let dry = std::mem::replace(&mut self.dry[self.at], *f);
            self.at = (self.at + 1) % Oversample::LATENCY;
            let mut o = [
                dry[0] * (1.0 - wet) + y[0] * out * wet,
                dry[1] * (1.0 - wet) + y[1] * out * wet,
            ];
            if self.s.soft_clip {
                o = o.map(f32::tanh);
            }
            *f = o;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Power at `freq` in `x` (48 kHz), by correlation.
    fn power(x: &[f32], freq: f32) -> f32 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in x.iter().enumerate() {
            let p = i as f64 * freq as f64 * std::f64::consts::TAU / 48_000.0;
            re += v as f64 * p.cos();
            im += v as f64 * p.sin();
        }
        ((re * re + im * im).sqrt() * 2.0 / x.len() as f64) as f32
    }

    #[test]
    fn curves_have_unit_slope_and_stay_bounded() {
        for k in 0..CURVES.len() as u8 {
            let slope = curve(k, 1e-3) / 1e-3;
            assert!(
                (slope - 1.0).abs() < 0.01,
                "{}: slope {slope}",
                CURVES[k as usize]
            );
            for x in [-100.0, -3.0, 3.0, 100.0] {
                assert!(curve(k, x).abs() <= 1.0 + 1e-6, "{}", CURVES[k as usize]);
            }
        }
    }

    /// Aliasing against the fundamental, dB, of curve `kind` driven 24 dB
    /// with a 7 kHz sine: ours, and the same curve with no oversampling.
    /// Its harmonics above 24 kHz would fold back between the real ones
    /// (the 5th, 35 kHz, to 13 kHz; the 7th, 49 kHz, to 1 kHz).
    fn aliasing(kind: u8) -> (f32, f32) {
        let n = 48_000;
        let f0 = 7_000.0;
        let input: Vec<[f32; 2]> = (0..n)
            .map(|i| {
                let v = (i as f32 * f0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [v, v]
            })
            .collect();
        let s = SaturatorSettings {
            drive_db: 24.0,
            curve: kind,
            dc: false,
            ..Default::default()
        };
        let mut sat = Saturator::new(&s, 48_000.0);
        let mut buf = input.clone();
        sat.process(&mut buf);
        let wet: Vec<f32> = buf[4_800..].iter().map(|f| f[0]).collect();
        let g = from_db(24.0);
        let raw: Vec<f32> = input[4_800..]
            .iter()
            .map(|f| curve(kind, f[0] * g))
            .collect();
        let alias = |x: &[f32]| power(x, 13_000.0).max(power(x, 1_000.0)) / power(x, f0);
        let db = |v: f32| 20.0 * v.max(1e-12).log10();
        (db(alias(&wet)), db(alias(&raw)))
    }

    #[test]
    fn driving_hard_aliases_far_less_than_without_oversampling() {
        for kind in 0..CURVES.len() as u8 {
            let (ours, raw) = aliasing(kind);
            eprintln!(
                "{:<13} {ours:>7.1} dB  (no oversampling {raw:>6.1} dB)",
                CURVES[kind as usize]
            );
            assert!(
                ours < raw - 40.0,
                "{}: {ours} vs {raw}",
                CURVES[kind as usize]
            );
        }
        // The rounded curves come out more than 70 dB clean; Digital
        // Clip's sharp corner still leaves a little at 4x.
        for kind in 0..5 {
            assert!(aliasing(kind).0 < -70.0, "{}", CURVES[kind as usize]);
        }
        assert!(aliasing(5).0 < -50.0, "Digital Clip at {}", aliasing(5).0);
    }

    #[test]
    fn dry_only_is_the_input_delayed() {
        let s = SaturatorSettings {
            drive_db: 30.0,
            dry_wet: 0.0,
            ..Default::default()
        };
        let mut sat = Saturator::new(&s, 48_000.0);
        let input: Vec<[f32; 2]> = (0..2_000).map(|i| [(i as f32 * 0.01).sin(), 0.0]).collect();
        let mut buf = input.clone();
        sat.process(&mut buf);
        let d = sat.latency();
        for i in d..buf.len() {
            assert_eq!(buf[i][0], input[i - d][0]);
        }
    }
}
