//! Spleeter's U-Nets through Core ML, which can run them on Apple's Neural
//! Engine (macOS and iPadOS only).
//!
//! The model is `scripts/models/convert_spleeter_coreml.py`'s
//! `spleeter-4stems.mlpackage`: all four U-Nets in one fixed-shape fp16
//! program, from the same weights as the other back ends. Core ML compiles a
//! package for this machine on [`Net::compile`] (seconds, once); the
//! compiled `.mlmodelc` folder then loads quickly with [`Net::load`].

use std::path::{Path, PathBuf};

use super::{F, STEMS, T};
use crate::coreml::Model;
pub use crate::coreml::Units;

/// The model's input and output names (see the conversion script).
const INPUT: &str = "magnitudes";
const OUTPUT: &str = "estimates";

/// The four U-Nets, loaded into Core ML.
pub struct Net {
    model: Model,
}

impl Net {
    /// Compiles `package` (a `.mlpackage` folder) for this machine and
    /// moves the result to `compiled` (a `.mlmodelc` folder), replacing
    /// what was there.
    pub fn compile(package: &Path, compiled: &Path) -> Result<(), String> {
        crate::coreml::compile(package, compiled)
    }

    /// Loads a compiled model (a `.mlmodelc` folder) to run on `units`.
    pub fn load(compiled: &Path, units: Units) -> Result<Self, String> {
        Model::load(compiled, units).map(|model| Net { model })
    }

    /// Compiles `package` into `cache` if it isn't there yet, then loads it.
    pub fn load_package(package: &Path, cache: &Path, units: Units) -> Result<Self, String> {
        let compiled: PathBuf = cache.join("spleeter-4stems.mlmodelc");
        if !compiled.is_dir() {
            Self::compile(package, &compiled)?;
        }
        Self::load(&compiled, units)
    }

    fn predict(&self, magnitudes: &[f32]) -> Result<Vec<f32>, String> {
        assert_eq!(magnitudes.len(), 2 * T * F);
        let mut out = self.model.predict(
            &[(INPUT, &[1, 2, T, F], magnitudes)],
            &[(OUTPUT, &[STEMS.len(), 2, T, F])],
        )?;
        Ok(out.remove(0))
    }
}

impl super::Net for Net {
    fn estimate(&self, magnitudes: Vec<f32>) -> Vec<f32> {
        // A failure after a successful load means Core ML itself broke; the
        // caller has no way to recover mid-separation, so say why.
        self.predict(&magnitudes)
            .unwrap_or_else(|e| panic!("Core ML separation failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spleeter::Net as _;

    /// Core ML's output matches the CPU path's within fp16 error. Needs the
    /// weights and the package: SPLEETER_WEIGHTS=…/spleeter-4stems.safetensors
    /// SPLEETER_MLPACKAGE=…/spleeter-4stems.mlpackage cargo test -p
    /// opendj-separation coreml -- --ignored
    #[test]
    #[ignore]
    fn core_ml_matches_the_cpu_path() {
        let weights = std::fs::read(std::env::var("SPLEETER_WEIGHTS").unwrap()).unwrap();
        let package = PathBuf::from(std::env::var("SPLEETER_MLPACKAGE").unwrap());
        let cache = std::env::temp_dir().join(format!("opendj-coreml-{}", std::process::id()));
        let cpu = crate::spleeter::cpu::Net::load(&weights).unwrap();
        // Spectrogram-like magnitudes: positive, mostly small.
        let mut seed = 1u32;
        let input: Vec<f32> = (0..2 * T * F)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let u = (seed >> 8) as f32 / (1u32 << 24) as f32;
                u * u * 2.0
            })
            .collect();
        let want = cpu.estimate(input.clone());
        for units in [Units::Cpu, Units::Gpu, Units::NeuralEngine] {
            let net = Net::load_package(&package, &cache, units).unwrap();
            let got = net.estimate(input.clone());
            assert_eq!(got.len(), want.len());
            let (mut s, mut e) = (0f64, 0f64);
            for (a, b) in want.iter().zip(&got) {
                s += (*a as f64).powi(2);
                e += (*a as f64 - *b as f64).powi(2);
            }
            let snr = 10.0 * (s / e.max(1e-30)).log10();
            println!("{units:?}: {snr:.1} dB against the CPU path");
            assert!(snr > 30.0, "{units:?}: only {snr:.1} dB");
        }
        let _ = std::fs::remove_dir_all(cache);
    }
}
