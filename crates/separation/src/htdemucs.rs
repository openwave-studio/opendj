//! Meta's HTDemucs v4 (MIT weights), through the vendored `demucs-core`
//! (third_party/demucs-core). Weights are the safetensors conversion
//! `htdemucs.safetensors` (84 MB).

use burn::prelude::Backend;
use demucs_core::{Demucs, ModelOptions};

pub use demucs_core::model::metadata::{download_url, HTDEMUCS};

/// The stems in the order [`HtDemucs::separate`] returns them.
pub const STEMS: [&str; 4] = ["drums", "bass", "other", "vocals"];

pub struct HtDemucs<B: Backend> {
    model: Demucs<B>,
}

impl<B: Backend> HtDemucs<B> {
    pub fn load(bytes: &[u8], device: &B::Device) -> Result<Self, String> {
        Demucs::from_bytes(ModelOptions::FourStem, bytes, device.clone())
            .map(|model| HtDemucs { model })
            .map_err(|e| e.to_string())
    }

    /// Compiles the GPU kernels for the model's shapes, which otherwise
    /// happens during the first separation.
    pub fn warm_up(&self) {
        pollster::block_on(self.model.warmup());
    }

    /// Separates a stereo track sampled at `rate` into [`STEMS`]:
    /// `[stem][channel][sample]`, at the same rate and length. The model
    /// runs at 44.1 kHz; other rates are resampled to it and back, so the
    /// stems line up with the track sample for sample.
    pub fn separate(&self, channels: [&[f32]; 2], rate: u32) -> Result<Vec<[Vec<f32>; 2]>, String> {
        let stems = pollster::block_on(self.model.separate(channels[0], channels[1], rate))
            .map_err(|e| e.to_string())?;
        Ok(stems.into_iter().map(|s| [s.left, s.right]).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few seconds of a drum-and-bass-ish loop at `rate`: kicks, a bass
    /// tone and a noisy hat, stereo.
    fn loop_at(rate: u32, seconds: f32) -> [Vec<f32>; 2] {
        let n = (rate as f32 * seconds) as usize;
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        let (mut l, mut r) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for i in 0..n {
            let t = i as f32 / rate as f32;
            let beat = t % 0.5;
            let kick = (std::f32::consts::TAU * 55.0 * beat).sin() * (-beat * 30.0).exp();
            let bass = (std::f32::consts::TAU * 110.0 * t).sin() * 0.2;
            let off = (t + 0.25) % 0.5;
            let hat = noise() * (-off * 200.0).exp() * 0.3;
            l.push(0.5 * kick + bass + hat);
            r.push(0.5 * kick + bass * 0.8 + hat * 0.6);
        }
        [l, r]
    }

    /// How far the stems' sum is from the track, against the track: near 0
    /// when they line up, near 1 or more when they drift.
    fn residual(track: &[Vec<f32>; 2], stems: &[[Vec<f32>; 2]]) -> f32 {
        let (mut err, mut sig) = (0.0f64, 0.0f64);
        for c in 0..2 {
            for (i, x) in track[c].iter().enumerate() {
                let sum: f32 = stems
                    .iter()
                    .map(|s| s[c].get(i).copied().unwrap_or(0.0))
                    .sum();
                err += f64::from(x - sum).powi(2);
                sig += f64::from(*x).powi(2);
            }
        }
        (err / sig).sqrt() as f32
    }

    /// Needs the weights and takes a minute on the CPU:
    /// `OPENDJ_MODELS=<folder with htdemucs.safetensors> cargo test -p
    /// opendj-separation --release -- --ignored stems_of_a_48`.
    #[test]
    #[ignore]
    fn stems_of_a_48_khz_track_line_up_with_it() {
        let dir = std::env::var("OPENDJ_MODELS").expect("OPENDJ_MODELS");
        let bytes = std::fs::read(std::path::Path::new(&dir).join("htdemucs.safetensors")).unwrap();
        let model = HtDemucs::<crate::Cpu>::load(&bytes, &Default::default()).unwrap();
        let track = loop_at(48_000, 6.0);
        let stems = model.separate([&track[0], &track[1]], 48_000).unwrap();
        assert!(stems.iter().all(|s| s[0].len() == track[0].len()));
        let aligned = residual(&track, &stems);
        // As before the fix: told 44.1 kHz, then stretched back to 48 kHz.
        let told = model
            .separate([&track[0], &track[1]], crate::SAMPLE_RATE)
            .unwrap();
        let stretched: Vec<[Vec<f32>; 2]> = told
            .iter()
            .map(|s| {
                s.clone()
                    .map(|x| crate::resample(&x, crate::SAMPLE_RATE, 48_000).unwrap())
            })
            .collect();
        let drifted = residual(&track, &stretched);
        eprintln!("residual: aligned {aligned:.3}, stretched {drifted:.3}");
        assert!(aligned < 0.2, "{aligned}");
        assert!(drifted > 0.5, "{drifted}");
    }
}
