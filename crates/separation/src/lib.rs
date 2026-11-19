//! Stem separation inside openDJ, without Python: Spleeter for stems within a
//! second or two of loading a track, HTDemucs for high-quality stems in the
//! background. Both run on burn, on the GPU (Metal on macOS, Vulkan on Linux)
//! or on the CPU. See openspec/changes/improve-stem-separation.

pub mod htdemucs;
pub mod spleeter;
pub mod stft;

/// burn's GPU backend: Metal on macOS, Vulkan elsewhere.
pub type Gpu = burn::backend::Wgpu;
/// burn's CPU backend, multi-threaded.
pub type Cpu = burn::backend::NdArray<f32>;

/// The sample rate both models were trained at.
pub const SAMPLE_RATE: u32 = 44_100;

pub use burn;

/// Keeps the GPU's kernel tuning cache in `dir`. Left alone, burn writes it
/// to `target/` under the working directory, which is `/` for an app opened
/// from the Finder: creating the file there panics while burn holds its
/// tuner lock, and every GPU separation after that fails on the poisoned
/// lock. Call before the first GPU separation; later calls do nothing.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub fn set_gpu_cache_dir(dir: std::path::PathBuf) {
    static SET: std::sync::Once = std::sync::Once::new();
    SET.call_once(|| {
        use burn::cubecl::config::{cache::CacheConfig, GlobalConfig};
        let _ = std::fs::create_dir_all(&dir);
        let mut config = GlobalConfig::default().override_from_env();
        config.autotune.cache = CacheConfig::File(dir);
        // Panics only if the GPU was used already; it keeps its own then.
        let _ = std::panic::catch_unwind(|| GlobalConfig::set(config));
    });
}

/// Does nothing: iPadOS keeps no file cache.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn set_gpu_cache_dir(_dir: std::path::PathBuf) {}

pub use demucs_core::dsp::resample::resample_channel as resample;

#[cfg(all(
    test,
    any(target_os = "macos", target_os = "linux", target_os = "windows")
))]
mod tests {
    use burn::cubecl::config::{cache::CacheConfig, GlobalConfig};

    #[test]
    fn the_gpu_tuning_cache_goes_where_we_say_not_under_the_working_directory() {
        let dir = std::env::temp_dir().join(format!("opendj-gpu-cache-{}", std::process::id()));
        super::set_gpu_cache_dir(dir.clone());
        match &GlobalConfig::get().autotune.cache {
            CacheConfig::File(p) => assert_eq!(p, &dir),
            other => panic!("{other:?}"),
        }
        assert!(dir.is_dir());
        let _ = std::fs::remove_dir_all(dir);
    }
}
