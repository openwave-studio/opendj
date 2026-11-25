//! openDJ Studio's engine: tracks of clips and scenes, the clock (its own,
//! Ableton Link, or a leader's beat), built-in devices and plug-ins, and
//! Live sets. No sound card, MIDI port or UI here, so the Studio app and
//! openDJ's Session deck both run it.
//!
//! [`engine::Studio`] renders on the audio thread; [`engine::Remote`] is
//! the UI's end: commands in, the [`engine::Monitor`] to read.

pub mod als;
pub mod clip;
pub mod control;
pub mod conv;
pub mod devices;
pub mod engine;
pub mod fx;
pub mod link;
pub mod plugins;
pub mod scale;
pub mod sound;
pub mod steps;
