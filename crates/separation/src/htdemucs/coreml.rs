//! HTDemucs's network through Core ML, which can run it on Apple's Neural
//! Engine (macOS and iPadOS only).
//!
//! The model is `scripts/models/convert_htdemucs_coreml.py`'s
//! `htdemucs.mlpackage`: the 4-stem network for one training segment, from
//! the same weights as the burn back end. The spectrograms and normalizing
//! stay outside it, in fp32 ([`demucs_core::separate_segment_with`]).

use std::path::{Path, PathBuf};

use demucs_core::{segment_frames, TRAINING_LENGTH};

use super::{SegmentNet, Stem, STEMS};
use crate::coreml::Model;
pub use crate::coreml::Units;

const BINS: usize = 2048;

/// HTDemucs's network, loaded into Core ML.
pub struct Net {
    model: Model,
}

impl Net {
    /// Compiles `package` (a `.mlpackage` folder) for this machine into
    /// `compiled` (a `.mlmodelc` folder), replacing what was there.
    pub fn compile(package: &Path, compiled: &Path) -> Result<(), String> {
        crate::coreml::compile(package, compiled)
    }

    /// Loads a compiled model (a `.mlmodelc` folder) to run on `units`.
    pub fn load(compiled: &Path, units: Units) -> Result<Self, String> {
        Model::load(compiled, units).map(|model| Net { model })
    }

    /// Compiles `package` into `cache` if it isn't there yet, then loads it.
    pub fn load_package(package: &Path, cache: &Path, units: Units) -> Result<Self, String> {
        let compiled: PathBuf = cache.join("htdemucs.mlmodelc");
        if !compiled.is_dir() {
            Self::compile(package, &compiled)?;
        }
        Self::load(&compiled, units)
    }

    /// The network on its own (see [`demucs_core::Network`]).
    pub fn run_network(&self, freq: &[f32], time: &[f32]) -> Result<(Vec<f32>, Vec<f32>), String> {
        let frames = segment_frames();
        let n = STEMS.len();
        let mut out = self.model.predict(
            &[
                ("freq", &[1, 4, BINS, frames], freq),
                ("time", &[1, 2, TRAINING_LENGTH], time),
            ],
            &[
                ("freq_out", &[1, n * 4, BINS, frames]),
                ("time_out", &[1, n * 2, TRAINING_LENGTH]),
            ],
        )?;
        let time = out.pop().ok_or("no time_out")?;
        let freq = out.pop().ok_or("no freq_out")?;
        Ok((freq, time))
    }
}

impl SegmentNet for Net {
    fn separate_segment(&self, left: &[f32], right: &[f32]) -> Result<Vec<Stem>, String> {
        demucs_core::separate_segment_with(left, right, &mut |f, t| {
            self.run_network(&f, &t)
                .map_err(demucs_core::DemucsError::Internal)
        })
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Core ML's stems match burn's on the CPU. Needs the weights and the
    /// package: OPENDJ_MODELS=<folder with htdemucs.safetensors>
    /// HTDEMUCS_MLPACKAGE=…/htdemucs.mlpackage cargo test --release -p
    /// opendj-separation htdemucs_core_ml -- --ignored --nocapture
    #[test]
    #[ignore]
    fn htdemucs_core_ml_matches_burn() {
        let dir = std::env::var("OPENDJ_MODELS").expect("OPENDJ_MODELS");
        let package = PathBuf::from(std::env::var("HTDEMUCS_MLPACKAGE").unwrap());
        let cache = std::env::temp_dir().join(format!("opendj-htdemucs-{}", std::process::id()));
        let bytes = std::fs::read(Path::new(&dir).join("htdemucs.safetensors")).unwrap();
        let burn = super::super::HtDemucs::<crate::Cpu>::load(&bytes, &Default::default()).unwrap();
        // A loop: kicks, a bass tone and noisy hats.
        let n = 6 * 44_100;
        let mut seed = 1u32;
        let (mut l, mut r) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for i in 0..n {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            let t = i as f32 / 44_100.0;
            let beat = t % 0.5;
            let kick = (std::f32::consts::TAU * 55.0 * beat).sin() * (-beat * 30.0).exp();
            let bass = (std::f32::consts::TAU * 110.0 * t).sin() * 0.2;
            let hat = noise * (-((t + 0.25) % 0.5) * 200.0).exp() * 0.3;
            l.push(0.5 * kick + bass + hat);
            r.push(0.5 * kick + bass * 0.8 + hat * 0.6);
        }
        let want = burn.separate_segment(&l, &r).unwrap();
        for units in [Units::Cpu, Units::Gpu, Units::NeuralEngine] {
            let net = Net::load_package(&package, &cache, units).unwrap();
            let started = std::time::Instant::now();
            let got = net.separate_segment(&l, &r).unwrap();
            let took = started.elapsed().as_secs_f64();
            for (a, b) in want.iter().zip(&got) {
                let (mut s, mut e) = (0f64, 0f64);
                for (x, y) in a
                    .left
                    .iter()
                    .zip(&b.left)
                    .chain(a.right.iter().zip(&b.right))
                {
                    s += (*x as f64).powi(2);
                    e += (*x as f64 - *y as f64).powi(2);
                }
                let snr = 10.0 * (s / e.max(1e-30)).log10();
                println!(
                    "{units:?} {:?}: {snr:.1} dB against burn ({took:.2} s a segment)",
                    a.id
                );
                assert!(snr > 20.0, "{units:?} {:?}: only {snr:.1} dB", a.id);
            }
        }
        let _ = std::fs::remove_dir_all(cache);
    }
}
