//! Settings › Appearance › Interface scale: the whole window drawn smaller
//! or larger, as a browser zooms a page. Slint lays everything out in
//! logical pixels, so the window's scale factor is set to the screen's own
//! times the DJ's choice; the window keeps its size on screen and gets more
//! (or fewer) logical pixels to lay out in. A first start on a screen with
//! less room than the 1280 x 720 the layouts need picks 90%.

use std::cell::Cell;
use std::time::Duration;

use serde_json::json;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

use crate::{settings, AppWindow};

pub const KEY: &str = "ui_scale";
/// In Settings order; the index is what's saved.
pub const STEPS: [f32; 5] = [0.85, 0.9, 1.0, 1.1, 1.25];
pub const LABELS: [&str; 5] = ["85%", "90%", "100%", "110%", "125%"];
const DEFAULT: usize = 2;
/// Picked on a first start when the screen has less room than this
/// (logical pixels, after the menu bar and the Dock or taskbar).
const SMALL_SCREEN: (f64, f64) = (1280.0, 720.0);
const SMALL_STEP: usize = 1;
/// Room the menu bar and the Dock or taskbar take.
const SYSTEM_BARS: f64 = 80.0;

thread_local! {
    static STEP: Cell<usize> = const { Cell::new(DEFAULT) };
    /// Looks for the window until it exists.
    static WAITING: std::cell::RefCell<Option<slint::Timer>> = const { std::cell::RefCell::new(None) };
}

/// The saved choice, as an index into [`STEPS`].
pub fn step() -> usize {
    STEP.with(Cell::get)
}

/// After the window is up: applies the saved scale, or on a first start
/// picks one for the screen. Also keeps it when the window moves to a
/// screen with another scale.
pub fn install(app: &AppWindow, first_run: bool) {
    // The window, and so its screen, only exists a little after the event
    // loop starts: look again until it does.
    let weak = app.as_weak();
    let tries = Cell::new(0);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(20),
        move || {
            let Some(app) = weak.upgrade() else {
                return;
            };
            tries.set(tries.get() + 1);
            let ready = app.window().with_winit_window(|_| ()).is_some();
            if ready || tries.get() > 250 {
                if ready {
                    start(&app, first_run);
                }
                WAITING.with(|t| drop(t.take()));
            }
        },
    );
    WAITING.with(|t| *t.borrow_mut() = Some(timer));
}

fn start(app: &AppWindow, first_run: bool) {
    let saved = settings::load()[KEY].as_u64().map(|s| s as usize);
    let step = match saved {
        Some(s) => s.min(STEPS.len() - 1),
        None if first_run && screen_is_small(app) => {
            settings::set(KEY, json!(SMALL_STEP));
            SMALL_STEP
        }
        None => DEFAULT,
    };
    STEP.with(|s| s.set(step));
    apply(app);
}

/// Picked in Settings.
pub fn choose(app: &AppWindow, index: usize) {
    let step = index.min(STEPS.len() - 1);
    STEP.with(|s| s.set(step));
    settings::set(KEY, json!(step));
    apply(app);
}

/// The window moved to a screen with another scale: Slint has just taken
/// the screen's own, so ours goes back on top once it has.
pub fn screen_changed(app: &AppWindow) {
    if step() == DEFAULT {
        return;
    }
    let weak = app.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(app) = weak.upgrade() {
            apply(&app);
        }
    });
}

fn screen_is_small(app: &AppWindow) -> bool {
    app.window()
        .with_winit_window(|w| {
            let monitor = w.current_monitor()?;
            let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
            Some(is_small(screen.width, screen.height))
        })
        .flatten()
        .unwrap_or(false)
}

/// `step`, or the largest step below it that leaves a screen this big
/// (logical pixels at the screen's own scale) room for the layouts.
fn fitting(step: usize, width: f64, height: f64) -> usize {
    (0..=step)
        .rev()
        .find(|&s| {
            let z = STEPS[s] as f64;
            s <= DEFAULT || !is_small(width / z, height / z)
        })
        .unwrap_or(DEFAULT)
}

/// A screen this big (logical pixels) leaves less than the layouts need.
fn is_small(width: f64, height: f64) -> bool {
    width < SMALL_SCREEN.0 || height - SYSTEM_BARS < SMALL_SCREEN.1
}

fn apply(app: &AppWindow) {
    use slint::platform::WindowEvent;
    let Some((screen, room)) = app.window().with_winit_window(|w| {
        let room = w.current_monitor().map(|m| {
            let s = m.size();
            (s.width as f64, s.height as f64)
        });
        (w.scale_factor() as f32, room)
    }) else {
        return;
    };
    // A larger step than the screen has room for would push the window
    // past its edges: the largest that fits is used instead.
    let step = match room {
        Some((w, h)) => fitting(step(), w / screen as f64, h / screen as f64),
        None => step(),
    };
    let scale = screen * STEPS[step];
    let window = app.window();
    if (window.scale_factor() - scale).abs() < 0.001 {
        return;
    }
    let physical = window.size();
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    // The window keeps its size on screen: its logical size follows.
    window.dispatch_event(WindowEvent::Resized {
        size: physical.to_logical(scale),
    });
    window.request_redraw();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_screens_get_the_smaller_step() {
        // Full HD at 150% (Windows laptops), 1366 x 768 at 100%.
        assert!(is_small(1280.0, 720.0));
        assert!(is_small(1366.0, 768.0));
        // A 13" MacBook Air, a 14" MacBook Pro, Full HD at 125%.
        assert!(!is_small(1440.0, 900.0));
        assert!(!is_small(1512.0, 982.0));
        assert!(!is_small(1536.0, 864.0));
        // 125% needs 1600 x 1000; a 13" MacBook Air gets 110%, a Full HD
        // screen all of it; 100% and below always stay.
        assert_eq!(fitting(4, 1440.0, 900.0), 3);
        assert_eq!(fitting(4, 1920.0, 1080.0), 4);
        assert_eq!(fitting(3, 1512.0, 982.0), 3);
        assert_eq!(fitting(1, 1280.0, 720.0), 1);
        assert_eq!(STEPS[DEFAULT], 1.0);
        assert_eq!(STEPS[SMALL_STEP], 0.9);
        assert_eq!(STEPS.len(), LABELS.len());
    }
}
