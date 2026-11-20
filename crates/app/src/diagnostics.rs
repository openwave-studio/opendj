//! Logs and crash reports, so there is something to attach to a bug report.
//!
//! When openDJ isn't started from a terminal (the macOS app, a desktop
//! launcher), everything it prints goes to `logs/opendj.log` in the
//! settings folder, and the previous run's log is kept as `opendj.1.log`.
//! A panic on any thread also writes `logs/crash-<time>.txt` with the
//! backtrace, and so, on Windows, does a crash outside Rust (a GPU or audio
//! driver), with the module it happened in.

use std::backtrace::Backtrace;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = "https://github.com/openwave-studio/opendj";

pub fn logs_dir() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("logs"))
}

/// Starts logging and installs the crash handler. Call first thing.
pub fn install() {
    let Some(dir) = logs_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if !std::io::stderr().is_terminal() {
        redirect_stderr(&dir);
    }
    eprintln!("openDJ {VERSION} on {}", platform());

    #[cfg(windows)]
    native_crash::install(dir.clone());
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let thread = std::thread::current();
        let report = format!(
            "openDJ {VERSION} on {}\nThread: {}\n\n{info}\n\n{}\n",
            platform(),
            thread.name().unwrap_or("unnamed"),
            Backtrace::force_capture(),
        );
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
        let path = dir.join(format!("crash-{stamp}.txt"));
        if let Ok(mut file) = std::fs::File::create(&path) {
            let _ = file.write_all(report.as_bytes());
            eprintln!("crash report written to {}", path.display());
        }
    }));
}

/// Where the logs are, for messages that point people at them.
#[cfg(windows)]
fn logs_hint() -> String {
    logs_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "the openDJ settings folder".into())
}

/// openDJ couldn't start: says why in the log and, without a terminal to
/// see it in, in a message too, with where the logs are. `graphics` says the
/// window hadn't drawn yet, so the next start tries another graphics mode.
pub fn startup_failed(error: &dyn std::fmt::Display, graphics: bool) {
    eprintln!("openDJ couldn't start: {error}");
    #[cfg(windows)]
    if !std::io::stderr().is_terminal() {
        let again = if graphics {
            "Open it again to try another graphics mode. If it still doesn't start, the"
        } else {
            "The"
        };
        let text = format!(
            "openDJ couldn't start: {error}\n\n{again} logs in {} say more; \
             please attach them to a bug report.",
            logs_hint()
        );
        message_box("openDJ", &text);
    }
    #[cfg(not(windows))]
    let _ = graphics;
}

#[cfg(windows)]
fn message_box(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (title, text) = (wide(title), wide(text));
    // SAFETY: both strings are NUL-terminated and outlive the call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// A crash in native code (an access violation in a GPU driver, say) never
/// reaches the panic hook: Windows ends the process without a word. This
/// writes a crash report first, naming the DLL it happened in.
#[cfg(windows)]
mod native_crash {
    use std::backtrace::Backtrace;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use windows_sys::Win32::System::Diagnostics::Debug::{
        SetUnhandledExceptionFilter, EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };

    static DIR: OnceLock<PathBuf> = OnceLock::new();
    static PREVIOUS: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();

    pub fn install(dir: PathBuf) {
        let _ = DIR.set(dir);
        // SAFETY: `filter` has the signature Windows expects and only reads
        // the record it is given.
        let previous = unsafe { SetUnhandledExceptionFilter(Some(filter)) };
        let _ = PREVIOUS.set(previous);
    }

    /// The DLL or EXE that holds `address`, and how far into it.
    fn module_of(address: usize) -> Option<(String, usize)> {
        let mut module = std::ptr::null_mut();
        // SAFETY: with FROM_ADDRESS the "name" is an address; UNCHANGED_REFCOUNT
        // leaves nothing to release.
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                address as *const u16,
                &mut module,
            )
        };
        if found == 0 {
            return None;
        }
        let mut name = [0u16; 512];
        // SAFETY: the buffer is as long as said.
        let len = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) };
        let name = String::from_utf16_lossy(&name[..len as usize]);
        Some((name, address.wrapping_sub(module as usize)))
    }

    fn describe(code: u32) -> &'static str {
        match code {
            0xC000_0005 => "access violation",
            0xC000_001D => "illegal instruction",
            0xC000_0094 => "integer divide by zero",
            0xC000_00FD => "stack overflow",
            0xC000_0409 => "stack buffer overrun",
            0xC000_0374 => "heap corruption",
            0x8000_0003 => "breakpoint",
            _ => "unknown",
        }
    }

    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        // SAFETY: Windows passes the exception being handled.
        let record = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) };
        if let (Some(record), Some(dir)) = (record, DIR.get()) {
            let code = record.ExceptionCode as u32;
            let address = record.ExceptionAddress as usize;
            let place = match module_of(address) {
                Some((module, offset)) => format!("{module} + {offset:#x}"),
                None => format!("{address:#x}"),
            };
            let thread = std::thread::current();
            let report = format!(
                "openDJ {} on {}\nThread: {}\n\nNative crash: {} ({code:#010x}) in {place}\n\n{}\n",
                super::VERSION,
                super::platform(),
                thread.name().unwrap_or("unnamed"),
                describe(code),
                Backtrace::force_capture(),
            );
            let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
            let path = dir.join(format!("crash-{stamp}.txt"));
            if let Ok(mut file) = std::fs::File::create(&path) {
                let _ = file.write_all(report.as_bytes());
            }
            eprintln!(
                "native crash: {} ({code:#010x}) in {place}; report written to {}",
                describe(code),
                path.display()
            );
        }
        match PREVIOUS.get().copied().flatten() {
            // SAFETY: the filter that was there before ours, given the same record.
            Some(previous) => unsafe { previous(info) },
            // EXCEPTION_CONTINUE_SEARCH: Windows ends the process as before.
            None => 0,
        }
    }
}

/// Keeps the last run's log and sends stderr to a new one.
#[cfg(unix)]
fn redirect_stderr(dir: &Path) {
    use std::os::fd::AsRawFd;
    let log = dir.join("opendj.log");
    let _ = std::fs::rename(&log, dir.join("opendj.1.log"));
    let Ok(file) = std::fs::File::create(&log) else {
        return;
    };
    // SAFETY: dup2 onto stderr with a descriptor that is open; the file is
    // leaked below so the descriptor stays valid for the whole run.
    if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } >= 0 {
        std::mem::forget(file);
    }
}

/// Keeps the last run's log and sends stderr to a new one. Rust looks the
/// stderr handle up on every write, so swapping it is enough.
#[cfg(windows)]
fn redirect_stderr(dir: &Path) {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};
    let log = dir.join("opendj.log");
    let _ = std::fs::rename(&log, dir.join("opendj.1.log"));
    let Ok(file) = std::fs::File::create(&log) else {
        return;
    };
    // SAFETY: the handle is open and, taken out of the File, stays open for
    // the whole run.
    unsafe {
        SetStdHandle(STD_ERROR_HANDLE, file.into_raw_handle());
    }
}

#[cfg(not(any(unix, windows)))]
fn redirect_stderr(_dir: &Path) {}

/// "macOS aarch64", "Linux x86_64 (Ubuntu 24.04)" and so on.
pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macOS".to_string(),
        "linux" => "Linux".to_string(),
        "windows" => "Windows".to_string(),
        other => other.to_string(),
    };
    let distro = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| format!(" ({})", v.trim_matches('"')))
        })
        .unwrap_or_default();
    format!("{os} {}{distro}", std::env::consts::ARCH)
}

/// The graphics adapter the window renders with, once it is known.
static GRAPHICS: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Lists the controllers plugged in now.
type ControllerList = Box<dyn Fn() -> Vec<String>>;

thread_local! {
    /// Where bug reports read the controllers plugged in now.
    static CONTROLLERS: std::cell::RefCell<Option<ControllerList>> =
        const { std::cell::RefCell::new(None) };
}

/// Remembers the graphics adapter, e.g. "Apple M2 (Metal)", for bug reports.
pub fn set_graphics(adapter: String) {
    if let Ok(mut g) = GRAPHICS.lock() {
        *g = adapter;
    }
}

/// Where bug reports read the controllers plugged in now. UI thread only.
pub fn set_controllers(list: impl Fn() -> Vec<String> + 'static) {
    CONTROLLERS.with(|c| *c.borrow_mut() = Some(Box::new(list)));
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A new GitHub issue from one of the templates, with the version and
/// system filled in.
pub fn issue_url(template: &str) -> String {
    format!(
        "{REPOSITORY}/issues/new?template={template}&version={}&system={}",
        encode(VERSION),
        encode(&platform())
    )
}

/// A new bug report with everything openDJ knows about the setup filled
/// in: version, system, graphics, audio outputs and controllers.
pub fn bug_report_url() -> String {
    let graphics = GRAPHICS.lock().map(|g| g.clone()).unwrap_or_default();
    let controllers = CONTROLLERS.with(|c| c.borrow().as_ref().map(|f| f()).unwrap_or_default());
    let setup = crate::settings::audio_setup();
    let default = opendj_engine::default_output_device();
    let audio = audio_summary(&setup, default.as_deref());
    bug_report_url_with(&graphics, &audio, &controllers)
}

fn bug_report_url_with(graphics: &str, audio: &str, controllers: &[String]) -> String {
    let mut url = issue_url("bug_report.yml");
    for (field, value) in [
        ("graphics", graphics.to_string()),
        ("audio", audio.to_string()),
        ("controller", controllers.join(", ")),
    ] {
        if !value.is_empty() {
            url.push_str(&format!("&{field}={}", encode(&value)));
        }
    }
    url
}

/// The output devices in use, e.g. "Built-in Output (system default),
/// 512 frames".
fn audio_summary(setup: &opendj_engine::AudioSetup, default: Option<&str>) -> String {
    let mut devices: Vec<String> = Vec::new();
    for route in setup.routes.iter().filter(|r| setup.in_use(r)) {
        let name = if route.device.is_empty() {
            match default {
                Some(d) => format!("{d} (system default)"),
                None => "system default".to_string(),
            }
        } else {
            route.device.clone()
        };
        if !devices.contains(&name) {
            devices.push(name);
        }
    }
    let mut out = devices.join(", ");
    if let Some(frames) = setup.buffer_frames {
        out.push_str(&format!(", {frames} frames"));
    }
    if setup.external_mixer {
        out.push_str(", external mixer");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_links_carry_the_version() {
        let url = issue_url("bug_report.yml");
        assert!(url.starts_with(
            "https://github.com/openwave-studio/opendj/issues/new?template=bug_report.yml&version="
        ));
        assert!(url.contains(VERSION));
        assert!(!url.contains(' '));
    }

    #[test]
    fn bug_reports_carry_the_setup() {
        let url = bug_report_url_with(
            "Apple M2 (Metal)",
            "Built-in Output (system default)",
            &["DDJ-FLX4 (Pioneer DDJ-FLX4)".to_string(), "X1".to_string()],
        );
        assert!(url.contains("template=bug_report.yml"));
        assert!(url.contains("&graphics=Apple%20M2%20%28Metal%29"));
        assert!(url.contains("&audio=Built-in%20Output%20%28system%20default%29"));
        assert!(url.contains("&controller=DDJ-FLX4%20%28Pioneer%20DDJ-FLX4%29%2C%20X1"));
        // Empty facts are left out, so the form shows its placeholder.
        let bare = bug_report_url_with("", "", &[]);
        assert!(!bare.contains("graphics=") && !bare.contains("controller="));
    }

    #[test]
    fn audio_summary_names_each_device_once() {
        use opendj_engine::{AudioSetup, Role, Route};
        let route = |role, device: &str| Route {
            role,
            device: device.into(),
            pair: 0,
        };
        let setup = AudioSetup {
            routes: vec![
                route(Role::Master, ""),
                route(Role::Headphones, "DDJ-FLX4"),
                route(Role::Booth, ""),
            ],
            buffer_frames: Some(256),
            ..AudioSetup::default()
        };
        assert_eq!(
            audio_summary(&setup, Some("Speakers")),
            "Speakers (system default), DDJ-FLX4, 256 frames"
        );
    }
}
