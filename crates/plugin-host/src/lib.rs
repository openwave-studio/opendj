//! Audio plugins run outside openDJ, in `opendj-plugin-host` helper
//! processes, so a plugin that crashes, hangs or runs slow costs one effect
//! slot, never the decks.
//!
//! - [`shm`]: the block of memory openDJ and a helper share.
//! - [`client`]: openDJ's side. [`spawn`] starts a helper; the audio thread
//!   hands it blocks with [`Connection::process`], which plays the block dry
//!   whenever the helper misses its deadline or is gone.
//! - [`policy`]: restart once, then disable; hangs count as crashes; a
//!   plugin that stays over its CPU budget is switched off.
//! - [`effects`]: what a helper can run. For now only test effects (gain,
//!   crash, hang, slow) that prove the isolation; CLAP, VST3 and AU plugins
//!   follow, as planned in the `add-audio-plugin-hosting` change.
//! - [`pd_patch`]: what a Pure Data patch offers, read from its text;
//!   `pd` (feature `pd`, experimental) runs it in libpd, and `pd_display`
//!   draws its Lua displays.
//! - [`display`]: the picture of a plugin display openDJ shows on the
//!   deck, and the mouse going back.
//! - [`window`] (in the helper): the window a plugin's own editor opens in.

#[cfg(all(feature = "formats", target_os = "macos"))]
pub mod au;
#[cfg(feature = "formats")]
pub mod clap;
pub mod client;
pub mod descriptor;
pub mod display;
pub mod effects;
#[cfg(feature = "pd")]
pub mod pd;
#[cfg(feature = "pd")]
pub mod pd_display;
pub mod pd_patch;
pub mod policy;
pub mod presets;
pub mod shm;
#[cfg(feature = "formats")]
pub mod vst3;
#[cfg(feature = "formats")]
pub mod window;

pub use client::{
    presets, scan, scan_audio_units, spawn, Connection, Editor, Helper, Options, Outcome,
    ScanError, Status, Transport,
};
pub use descriptor::Descriptor;
pub use effects::{Info, Param, Plugin};
pub use policy::{Action, Limits, Policy};
pub use shm::Midi;

/// Ends the helper at once with `code`, after flushing what it printed.
///
/// A plain exit runs every loaded library's static destructors and exit
/// handlers while the plugin's own threads (timers, sample loaders) are
/// still running, and many plugins crash right there: on macOS that made
/// a crash report and a "quit unexpectedly" dialog every time a plugin was
/// scanned, unloaded or openDJ quit. `_exit` skips them; the system frees
/// the process's memory and files anyway.
pub fn leave(code: i32) -> ! {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    #[cfg(unix)]
    // SAFETY: ends the process; nothing runs after it.
    unsafe {
        libc::_exit(code)
    }
    #[cfg(not(unix))]
    std::process::exit(code)
}
