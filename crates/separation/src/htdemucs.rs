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

    /// Separates a stereo track at 44.1 kHz into [`STEMS`]:
    /// `[stem][channel][sample]`.
    pub fn separate(&self, channels: [&[f32]; 2]) -> Result<Vec<[Vec<f32>; 2]>, String> {
        let stems = pollster::block_on(self.model.separate(
            channels[0],
            channels[1],
            crate::SAMPLE_RATE,
        ))
        .map_err(|e| e.to_string())?;
        Ok(stems.into_iter().map(|s| [s.left, s.right]).collect())
    }

    /// As [`Self::separate`], telling `progress` each time one of the
    /// model's chunks is done (chunks done, of how many) and giving up with
    /// [`STOPPED`] once `keep_going` says no. `progress` may wait, which
    /// pauses the separation between chunks.
    pub fn separate_watched(
        &self,
        channels: [&[f32]; 2],
        progress: &mut dyn FnMut(usize, usize),
        keep_going: &dyn Fn() -> bool,
    ) -> Result<Vec<[Vec<f32>; 2]>, String> {
        let mut watch = Watch {
            progress,
            keep_going,
        };
        let stems = pollster::block_on(self.model.separate_with_listener(
            channels[0],
            channels[1],
            crate::SAMPLE_RATE,
            &mut watch,
        ))
        .map_err(|e| match e {
            demucs_core::DemucsError::Cancelled => STOPPED.to_string(),
            e => e.to_string(),
        })?;
        Ok(stems.into_iter().map(|s| [s.left, s.right]).collect())
    }
}

/// The error a separation stopped by the DJ ends with.
pub const STOPPED: &str = "stopped";

struct Watch<'a> {
    progress: &'a mut dyn FnMut(usize, usize),
    keep_going: &'a dyn Fn() -> bool,
}

impl demucs_core::listener::ForwardListener for Watch<'_> {
    fn on_event(&mut self, event: demucs_core::listener::ForwardEvent) {
        if let demucs_core::listener::ForwardEvent::ChunkDone { index, total } = event {
            (self.progress)(index + 1, total);
        }
    }

    fn is_cancelled(&self) -> bool {
        !(self.keep_going)()
    }
}
