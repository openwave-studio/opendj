//! Quitting: the steps that let go of devices, run while every thread is
//! still alive and watched, so a driver that never lets go can't leave
//! openDJ running without a window.

use std::cell::Cell;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long all the steps together may take before the process is ended.
const LIMIT: Duration = Duration::from_secs(10);

/// The quit in progress; [`Shutdown::finish`] once every step has run.
pub struct Shutdown {
    step: Arc<Mutex<&'static str>>,
    /// Dropped with this, which tells the watchdog the steps finished.
    _done: mpsc::Sender<()>,
    /// A step left a thread behind (see [`Shutdown::abandon`]).
    abandoned: Cell<bool>,
}

impl Shutdown {
    /// Starts the watchdog: if the steps aren't done in [`LIMIT`], it
    /// logs the step that hangs and ends the process.
    pub fn start() -> Self {
        eprintln!("quitting");
        let step = Arc::new(Mutex::new("starting"));
        let (done, finished) = mpsc::channel::<()>();
        let watched = step.clone();
        let spawned = std::thread::Builder::new()
            .name("opendj-quit-watchdog".into())
            .spawn(move || {
                // Sending is never done: the sender dropping ends the wait.
                if let Err(mpsc::RecvTimeoutError::Timeout) = finished.recv_timeout(LIMIT) {
                    let step = *watched.lock().unwrap_or_else(|e| e.into_inner());
                    eprintln!(
                        "quitting: still {step} after {} s; ending the process",
                        LIMIT.as_secs()
                    );
                    end_process(1);
                }
            });
        if let Err(e) = spawned {
            eprintln!("quitting without a watchdog: {e}");
        }
        Self {
            step,
            _done: done,
            abandoned: Cell::new(false),
        }
    }

    /// Runs one step, logging how long it took.
    pub fn step(&self, name: &'static str, f: impl FnOnce()) {
        *self.step.lock().unwrap_or_else(|e| e.into_inner()) = name;
        let started = Instant::now();
        f();
        eprintln!("quitting: {name} took {} ms", started.elapsed().as_millis());
    }

    /// A step gave up waiting for a thread stuck in a driver or system
    /// service: the process ends at once when the steps are done.
    pub fn abandon(&self, what: &str) {
        eprintln!("quitting: left {what}");
        self.abandoned.set(true);
    }

    /// Every step has run. With a thread left behind, the process ends
    /// here: the clean-up on the way out would wait on that thread again.
    pub fn finish(self) {
        let abandoned = self.abandoned.get();
        drop(self);
        if abandoned {
            eprintln!("quitting: done; ending at once");
            end_process(0);
        }
        eprintln!("quitting: done");
    }
}

/// Ends the process at once, with `code`, without the clean-up that runs
/// on the way out: that clean-up is where a stuck driver would hang again.
#[cfg(windows)]
fn end_process(code: u32) -> ! {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
    // SAFETY: ends this process; the pseudo-handle needs no closing.
    unsafe {
        TerminateProcess(GetCurrentProcess(), code);
    }
    std::process::exit(code as i32)
}

#[cfg(not(windows))]
fn end_process(code: u32) -> ! {
    std::process::exit(code as i32)
}
