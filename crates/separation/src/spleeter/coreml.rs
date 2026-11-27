//! Spleeter's U-Nets through Core ML, which can run them on Apple's Neural
//! Engine (macOS and iPadOS only).
//!
//! The model is `scripts/models/convert_spleeter_coreml.py`'s
//! `spleeter-4stems.mlpackage`: all four U-Nets in one fixed-shape fp16
//! program, from the same weights as the other back ends. Core ML compiles a
//! package for this machine on [`Net::compile`] (seconds, once); the
//! compiled `.mlmodelc` folder then loads quickly with [`Net::load`].

#![allow(deprecated)] // MLMultiArray::dataPointer: the block-based reads need block2 for nothing more.

use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::AnyThread;
use objc2_core_ml::{
    MLComputeUnits, MLDictionaryFeatureProvider, MLFeatureProvider, MLFeatureValue, MLModel,
    MLModelConfiguration, MLMultiArray, MLMultiArrayDataType,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSFileManager, NSNumber, NSString, NSURL};

use super::{F, STEMS, T};

/// The model's input and output names (see the conversion script).
const INPUT: &str = "magnitudes";
const OUTPUT: &str = "estimates";

/// Where Core ML may run the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    /// The Neural Engine, with the CPU for anything it can't run.
    NeuralEngine,
    /// The GPU, with the CPU.
    Gpu,
    /// The CPU only.
    Cpu,
    /// Whatever Core ML picks.
    All,
}

impl Units {
    fn compute_units(self) -> MLComputeUnits {
        match self {
            Units::NeuralEngine => MLComputeUnits::CPUAndNeuralEngine,
            Units::Gpu => MLComputeUnits::CPUAndGPU,
            Units::Cpu => MLComputeUnits::CPUOnly,
            Units::All => MLComputeUnits::All,
        }
    }
}

fn message(e: Retained<NSError>) -> String {
    e.localizedDescription().to_string()
}

fn url(path: &Path) -> Retained<NSURL> {
    let s = NSString::from_str(&path.to_string_lossy());
    NSURL::fileURLWithPath(&s)
}

fn shape(dims: &[usize]) -> Retained<NSArray<NSNumber>> {
    let numbers: Vec<Retained<NSNumber>> = dims
        .iter()
        .map(|&d| NSNumber::new_isize(d as isize))
        .collect();
    NSArray::from_retained_slice(&numbers)
}

fn numbers(array: &NSArray<NSNumber>) -> Vec<isize> {
    array.iter().map(|n| n.integerValue()).collect()
}

/// The four U-Nets, loaded into Core ML.
pub struct Net {
    model: Retained<MLModel>,
}

// Core ML models are safe to call from any thread, one prediction at a time
// per caller; `estimate` takes `&self` and Core ML serialises internally.
unsafe impl Send for Net {}
unsafe impl Sync for Net {}

impl Net {
    /// Compiles `package` (a `.mlpackage` folder) for this machine and
    /// moves the result to `compiled` (a `.mlmodelc` folder), replacing
    /// what was there.
    pub fn compile(package: &Path, compiled: &Path) -> Result<(), String> {
        unsafe {
            let temp = MLModel::compileModelAtURL_error(&url(package)).map_err(message)?;
            let files = NSFileManager::defaultManager();
            if compiled.exists() {
                std::fs::remove_dir_all(compiled).map_err(|e| e.to_string())?;
            }
            if let Some(parent) = compiled.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            files
                .moveItemAtURL_toURL_error(&temp, &url(compiled))
                .map_err(message)?;
        }
        Ok(())
    }

    /// Loads a compiled model (a `.mlmodelc` folder) to run on `units`.
    pub fn load(compiled: &Path, units: Units) -> Result<Self, String> {
        unsafe {
            let config = MLModelConfiguration::new();
            config.setComputeUnits(units.compute_units());
            let model =
                MLModel::modelWithContentsOfURL_configuration_error(&url(compiled), &config)
                    .map_err(message)?;
            Ok(Net { model })
        }
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
        unsafe {
            let input = MLMultiArray::initWithShape_dataType_error(
                MLMultiArray::alloc(),
                &shape(&[1, 2, T, F]),
                MLMultiArrayDataType::Float32,
            )
            .map_err(message)?;
            // A new array is contiguous, first axis outermost.
            let dst: NonNull<f32> = input.dataPointer().cast();
            std::ptr::copy_nonoverlapping(magnitudes.as_ptr(), dst.as_ptr(), magnitudes.len());
            let value = MLFeatureValue::featureValueWithMultiArray(&input);
            let key = NSString::from_str(INPUT);
            let value_obj: &AnyObject = &value;
            let dict = NSDictionary::<NSString, AnyObject>::from_slices(&[&*key], &[value_obj]);
            let features = MLDictionaryFeatureProvider::initWithDictionary_error(
                MLDictionaryFeatureProvider::alloc(),
                &dict,
            )
            .map_err(message)?;
            let out = self
                .model
                .predictionFromFeatures_error(ProtocolObject::from_ref(&*features))
                .map_err(message)?;
            let out = out
                .featureValueForName(&NSString::from_str(OUTPUT))
                .and_then(|v| v.multiArrayValue())
                .ok_or("the model gave no estimates")?;
            read(&out)
        }
    }
}

/// `array`'s values as [stem][channel][frame][bin], following its strides
/// (Core ML may pad rows when the Neural Engine wrote them).
unsafe fn read(array: &MLMultiArray) -> Result<Vec<f32>, String> {
    let dims = numbers(&array.shape());
    let strides = numbers(&array.strides());
    let want = [STEMS.len() as isize, 2, T as isize, F as isize];
    if dims != want {
        return Err(format!("estimates have shape {dims:?}, not {want:?}"));
    }
    let kind = array.dataType();
    let base = array.dataPointer();
    let mut out = Vec::with_capacity(STEMS.len() * 2 * T * F);
    for s in 0..want[0] {
        for c in 0..want[1] {
            for t in 0..want[2] {
                let row = s * strides[0] + c * strides[1] + t * strides[2];
                for f in 0..want[3] {
                    let i = row + f * strides[3];
                    out.push(if kind == MLMultiArrayDataType::Float32 {
                        *base.cast::<f32>().as_ptr().offset(i)
                    } else if kind == MLMultiArrayDataType::Float16 {
                        half::f16::from_bits(*base.cast::<u16>().as_ptr().offset(i)).to_f32()
                    } else {
                        return Err(format!("estimates are {kind:?}, not floats"));
                    });
                }
            }
        }
    }
    Ok(out)
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
