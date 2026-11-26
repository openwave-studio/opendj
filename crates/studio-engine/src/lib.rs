//! openDJ Studio's engine: tracks of clips and scenes, the clock (its own,
//! Ableton Link, or a leader's beat), built-in devices and plug-ins, and
//! Live sets. No sound card, MIDI port or UI here, so the Studio app and
//! openDJ's Session deck both run it.
//!
//! [`engine::Studio`] renders on the audio thread; [`engine::Remote`] is
//! the UI's end: commands in, the [`engine::Monitor`] to read.

pub mod als;
pub mod autofilter;
pub mod clip;
pub mod control;
pub mod conv;
pub mod devices;
pub mod engine;
pub mod fx;
pub mod kit;
pub mod limiter;
#[cfg(feature = "link")]
pub mod link;
pub mod midifx;
/// Without Ableton Link (openDJ's Session): a clock that can't exist, so
/// the engine's Link paths compile away.
#[cfg(not(feature = "link"))]
pub mod link {
    use crate::engine::Span;

    pub enum LinkClock {}

    impl LinkClock {
        pub fn begin(&mut self, _frames: usize) -> Option<Span> {
            match *self {}
        }

        pub fn set_tempo(&mut self, _bpm: f64) {
            match *self {}
        }
    }
}
pub mod plugins;
#[cfg(test)]
mod quality;
pub mod rack;
pub mod saturator;
pub mod scale;
pub mod session;
pub mod simpler;
pub mod sound;
pub mod steps;
