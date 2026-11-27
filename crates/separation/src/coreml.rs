//! Running fixed-shape Core ML programs, which macOS and iPadOS can put on
//! Apple's Neural Engine: compiling a `.mlpackage` for this machine (seconds,
//! once), loading the compiled `.mlmodelc` folder, and predicting on fp32
//! arrays. [`crate::spleeter::coreml`] and [`crate::htdemucs::coreml`] use it.

#![allow(deprecated)] // MLMultiArray::dataPointer: the block-based reads need block2 for nothing more.

use std::path::Path;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::AnyThread;
use objc2_core_ml::{
    MLComputeUnits, MLDictionaryFeatureProvider, MLFeatureProvider, MLFeatureValue, MLModel,
    MLModelConfiguration, MLMultiArray, MLMultiArrayDataType,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSFileManager, NSNumber, NSString, NSURL};

/// Where Core ML may run a model.
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

/// Compiles `package` (a `.mlpackage` folder) for this machine and moves the
/// result to `compiled` (a `.mlmodelc` folder), replacing what was there.
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

/// A compiled model, loaded.
pub struct Model {
    model: Retained<MLModel>,
}

// Core ML models are safe to call from any thread; predictions take `&self`
// and Core ML serialises them internally.
unsafe impl Send for Model {}
unsafe impl Sync for Model {}

impl Model {
    /// Loads a compiled model (a `.mlmodelc` folder) to run on `units`.
    pub fn load(compiled: &Path, units: Units) -> Result<Self, String> {
        unsafe {
            let config = MLModelConfiguration::new();
            config.setComputeUnits(units.compute_units());
            let model =
                MLModel::modelWithContentsOfURL_configuration_error(&url(compiled), &config)
                    .map_err(message)?;
            Ok(Model { model })
        }
    }

    /// Runs the model on `inputs` (name, shape, fp32 values, first axis
    /// outermost) and gives back `outputs` (name, expected shape) as fp32
    /// in the same layout.
    pub fn predict(
        &self,
        inputs: &[(&str, &[usize], &[f32])],
        outputs: &[(&str, &[usize])],
    ) -> Result<Vec<Vec<f32>>, String> {
        unsafe {
            let mut keys = Vec::new();
            let mut values = Vec::new();
            for &(name, dims, data) in inputs {
                assert_eq!(data.len(), dims.iter().product::<usize>(), "{name}");
                let array = MLMultiArray::initWithShape_dataType_error(
                    MLMultiArray::alloc(),
                    &shape(dims),
                    MLMultiArrayDataType::Float32,
                )
                .map_err(message)?;
                // A new array is contiguous, first axis outermost.
                let dst: NonNull<f32> = array.dataPointer().cast();
                std::ptr::copy_nonoverlapping(data.as_ptr(), dst.as_ptr(), data.len());
                keys.push(NSString::from_str(name));
                values.push(MLFeatureValue::featureValueWithMultiArray(&array));
            }
            let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
            let value_refs: Vec<&AnyObject> = values.iter().map(|v| -> &AnyObject { v }).collect();
            let dict = NSDictionary::<NSString, AnyObject>::from_slices(&key_refs, &value_refs);
            let features = MLDictionaryFeatureProvider::initWithDictionary_error(
                MLDictionaryFeatureProvider::alloc(),
                &dict,
            )
            .map_err(message)?;
            let out = self
                .model
                .predictionFromFeatures_error(ProtocolObject::from_ref(&*features))
                .map_err(message)?;
            outputs
                .iter()
                .map(|&(name, dims)| {
                    let array = out
                        .featureValueForName(&NSString::from_str(name))
                        .and_then(|v| v.multiArrayValue())
                        .ok_or_else(|| format!("the model gave no {name}"))?;
                    read(&array, name, dims)
                })
                .collect()
        }
    }
}

/// `array`'s values in `dims` order, first axis outermost, following its
/// strides (Core ML may pad rows when the Neural Engine wrote them).
unsafe fn read(array: &MLMultiArray, name: &str, dims: &[usize]) -> Result<Vec<f32>, String> {
    let got = numbers(&array.shape());
    let want: Vec<isize> = dims.iter().map(|&d| d as isize).collect();
    if got != want {
        return Err(format!("{name} has shape {got:?}, not {want:?}"));
    }
    let strides = numbers(&array.strides());
    let kind = array.dataType();
    let fp16 = if kind == MLMultiArrayDataType::Float16 {
        true
    } else if kind == MLMultiArrayDataType::Float32 {
        false
    } else {
        return Err(format!("{name} is {kind:?}, not floats"));
    };
    let base = array.dataPointer();
    let n = dims.len();
    let inner = dims[n - 1];
    let inner_stride = strides[n - 1];
    let rows: usize = dims[..n - 1].iter().product();
    let mut out = Vec::with_capacity(rows * inner);
    let mut index = vec![0usize; n - 1];
    for _ in 0..rows {
        let row: isize = index
            .iter()
            .zip(&strides)
            .map(|(&i, &s)| i as isize * s)
            .sum();
        for f in 0..inner as isize {
            let i = row + f * inner_stride;
            out.push(if fp16 {
                half::f16::from_bits(*base.cast::<u16>().as_ptr().offset(i)).to_f32()
            } else {
                *base.cast::<f32>().as_ptr().offset(i)
            });
        }
        // The next row: count up from the last outer axis.
        for a in (0..n - 1).rev() {
            index[a] += 1;
            if index[a] < dims[a] {
                break;
            }
            index[a] = 0;
        }
    }
    Ok(out)
}
