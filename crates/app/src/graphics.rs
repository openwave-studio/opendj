//! Picks the graphics backend, and falls back to another one when the last
//! start never got as far as drawing the window.
//!
//! Some GPU drivers take the whole process down while wgpu sets up or draws
//! its first frame (old Intel drivers on Windows, laptops with both Intel and
//! NVIDIA graphics), before any Rust code can catch it. So just before the
//! window opens, openDJ writes `graphics-starting` in the settings folder
//! with the backend it is about to try, and removes it once the first frame
//! is drawn. When the file is still there at the next start, the last start
//! died on the way, and this one tries the next backend in [`CHAIN`]. The
//! one that worked is kept in `graphics.txt` for the starts after.
//!
//! `OPENDJ_GRAPHICS` (dx12, vulkan, metal or gl) picks one by hand and turns
//! the fallback off; `OPENDJ_GRAPHICS=auto` forgets a kept one and starts
//! from the top of [`CHAIN`] again.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use slint::wgpu_30::{wgpu, WGPUConfiguration, WGPUSettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Whatever wgpu picks (Vulkan first on Windows and Linux).
    Auto,
    Dx12,
    Vulkan,
    Metal,
    Gl,
}

/// The order backends are tried in after a start that didn't draw.
#[cfg(windows)]
// DirectX 12 first: Intel's Vulkan drivers have closed openDJ while it
// starts, and DX12 is what Windows drivers are tested hardest with.
const CHAIN: &[Backend] = &[Backend::Dx12, Backend::Vulkan, Backend::Gl];
#[cfg(all(unix, not(target_os = "macos")))]
const CHAIN: &[Backend] = &[Backend::Auto, Backend::Gl];
#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
const CHAIN: &[Backend] = &[Backend::Auto];

const STARTING: &str = "graphics-starting";
const KEPT: &str = "graphics.txt";

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Auto => "auto",
            Backend::Dx12 => "dx12",
            Backend::Vulkan => "vulkan",
            Backend::Metal => "metal",
            Backend::Gl => "gl",
        }
    }

    pub fn parse(s: &str) -> Option<Backend> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(Backend::Auto),
            "dx12" | "d3d12" | "directx" => Some(Backend::Dx12),
            "vulkan" | "vk" => Some(Backend::Vulkan),
            "metal" => Some(Backend::Metal),
            "gl" | "opengl" | "gles" => Some(Backend::Gl),
            _ => None,
        }
    }

    /// The one after this in [`CHAIN`], round to the first.
    fn next(self) -> Backend {
        let at = CHAIN.iter().position(|b| *b == self);
        match at {
            Some(i) => CHAIN[(i + 1) % CHAIN.len()],
            None => CHAIN[0],
        }
    }

    pub fn configuration(self) -> WGPUConfiguration {
        let mut settings = WGPUSettings::default();
        settings.power_preference = power_preference(std::env::var_os("WGPU_POWER_PREF").is_some());
        settings.backends = match self {
            Backend::Auto => settings.backends,
            Backend::Dx12 => wgpu::Backends::DX12,
            Backend::Vulkan => wgpu::Backends::VULKAN,
            Backend::Metal => wgpu::Backends::METAL,
            Backend::Gl => wgpu::Backends::GL,
        };
        WGPUConfiguration::Automatic(settings)
    }
}

/// The fast GPU on a computer with two: wgpu otherwise takes the first one
/// the system lists, which on Windows laptops with Intel or AMD graphics
/// beside an NVIDIA one is usually the slower built-in GPU, so the
/// waveforms drop frames. wgpu's own `WGPU_POWER_PREF` still wins.
fn power_preference(set_by_hand: bool) -> wgpu::PowerPreference {
    if set_by_hand {
        wgpu::PowerPreference::from_env().unwrap_or_default()
    } else {
        wgpu::PowerPreference::HighPerformance
    }
}

/// What this start uses, and the file that says it is starting.
pub struct Choice {
    pub backend: Backend,
    dir: Option<PathBuf>,
    armed: Cell<bool>,
    drawn: Cell<bool>,
}

/// The backend for this start: the one set by hand, the next one after a
/// start that didn't draw, or the one that worked last.
pub fn choose(dir: Option<&Path>) -> Choice {
    if let Some(by_hand) = std::env::var("OPENDJ_GRAPHICS")
        .ok()
        .and_then(|v| Backend::parse(&v))
    {
        match (by_hand, dir) {
            // Back to the usual order, forgetting the one kept.
            (Backend::Auto, Some(dir)) => {
                eprintln!("graphics: back to the default (OPENDJ_GRAPHICS=auto)");
                let _ = std::fs::remove_file(dir.join(KEPT));
                let _ = std::fs::remove_file(dir.join(STARTING));
            }
            _ => {
                eprintln!("graphics: {} (OPENDJ_GRAPHICS)", by_hand.name());
                return Choice::new(by_hand, None);
            }
        }
    }
    // wgpu's own WGPU_BACKEND, likewise.
    if std::env::var_os("WGPU_BACKEND").is_some() {
        eprintln!("graphics: as WGPU_BACKEND says");
        return Choice::new(Backend::Auto, None);
    }
    let Some(dir) = dir else {
        return Choice::new(CHAIN[0], None);
    };
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .ok()
            .and_then(|s| Backend::parse(&s))
    };
    let backend = match read(STARTING) {
        Some(failed) => {
            let next = failed.next();
            eprintln!(
                "graphics: the last start stopped before drawing with {}; trying {}",
                failed.name(),
                next.name()
            );
            next
        }
        None => read(KEPT).unwrap_or(CHAIN[0]),
    };
    eprintln!("graphics: {}", backend.name());
    Choice::new(backend, Some(dir.to_path_buf()))
}

impl Choice {
    fn new(backend: Backend, dir: Option<PathBuf>) -> Choice {
        Choice {
            backend,
            dir,
            armed: Cell::new(false),
            drawn: Cell::new(false),
        }
    }

    /// The window was about to open but hasn't drawn yet.
    pub fn pending(&self) -> bool {
        self.armed.get() && !self.drawn.get()
    }

    /// Just before the window opens: until [`Choice::drew`], this start
    /// counts as one that didn't draw.
    pub fn arm(&self) {
        self.armed.set(true);
        if let Some(dir) = &self.dir {
            let _ = std::fs::write(dir.join(STARTING), self.backend.name());
        }
    }

    /// The first frame is on screen: this backend works here.
    pub fn drew(&self) {
        if self.drawn.replace(true) {
            return;
        }
        let Some(dir) = &self.dir else {
            return;
        };
        let _ = std::fs::remove_file(dir.join(STARTING));
        let kept = dir.join(KEPT);
        if self.backend == CHAIN[0] {
            let _ = std::fs::remove_file(kept);
        } else {
            let _ = std::fs::write(kept, self.backend.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("opendj-graphics-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn names_read_back() {
        for b in [
            Backend::Auto,
            Backend::Dx12,
            Backend::Vulkan,
            Backend::Metal,
            Backend::Gl,
        ] {
            assert_eq!(Backend::parse(b.name()), Some(b));
        }
        assert_eq!(Backend::parse("OpenGL\n"), Some(Backend::Gl));
        assert_eq!(Backend::parse("glide"), None);
    }

    #[test]
    fn the_fast_gpu_is_asked_for_unless_set_by_hand() {
        assert_eq!(
            power_preference(false),
            wgpu::PowerPreference::HighPerformance
        );
    }

    #[test]
    fn the_chain_goes_round() {
        let mut b = CHAIN[0];
        for _ in 0..CHAIN.len() {
            b = b.next();
        }
        assert_eq!(b, CHAIN[0]);
    }

    // OPENDJ_GRAPHICS and WGPU_BACKEND aren't set in the tests.
    #[test]
    fn a_start_that_didnt_draw_moves_on_and_the_one_that_did_is_kept() {
        let dir = temp_dir("chain");
        let first = choose(Some(&dir));
        assert_eq!(first.backend, CHAIN[0]);
        first.arm();
        // The process dies here; the next start moves on.
        let second = choose(Some(&dir));
        assert_eq!(second.backend, CHAIN[1 % CHAIN.len()]);
        second.arm();
        second.drew();
        assert!(!dir.join(STARTING).exists());
        // And the one after keeps it.
        assert_eq!(choose(Some(&dir)).backend, second.backend);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
