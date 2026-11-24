//! The instrument plugin's own editor in the browser's plugin panel. A
//! CLAP, VST3 or AU editor is a window of the plugin's helper process, and
//! another process's window can't be put inside openDJ's, so the helper
//! shows it in a borderless window laid over the panel's area
//! (`[Instrument1],dock_editor` and `dock_x` .. `dock_h`). This keeps that
//! window where the panel is on screen, hides it while openDJ is in the
//! background or minimised, and lets the panel fall back to its sliders
//! when the plugin has no editor.

use std::sync::Arc;
use std::time::Duration;

use opendj_control::{ControlHandle, ControlRegistry};
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

use crate::{AppWindow, InstrumentsUi};

const GROUP: &str = "[Instrument1]";

/// How often the panel's place is looked at.
const FRAME: Duration = Duration::from_millis(33);

/// Frames the docked window stays up after neither openDJ nor it has the
/// keyboard, so a click moving between them doesn't make it blink.
const FOCUS_GRACE: u32 = 6;

struct Handles {
    instrument: ControlHandle,
    dock: ControlHandle,
    place: [ControlHandle; 4],
    focused: ControlHandle,
}

impl Handles {
    fn new(reg: &ControlRegistry) -> Option<Self> {
        let get = |k: &str| reg.get(GROUP, k);
        Some(Self {
            instrument: get("instrument")?,
            dock: get("dock_editor")?,
            place: [
                get("dock_x")?,
                get("dock_y")?,
                get("dock_w")?,
                get("dock_h")?,
            ],
            focused: get("editor_focused")?,
        })
    }

    fn place(&self, rect: (i32, i32, i32, i32)) {
        let (x, y, w, h) = rect;
        for (c, v) in self.place.iter().zip([x, y, w, h]) {
            if c.get() != v as f64 {
                c.set(v as f64);
            }
        }
    }
}

#[derive(Default)]
struct State {
    /// We asked for the docked editor.
    asked: bool,
    /// The plugin it failed for (no editor): the panel shows sliders until
    /// another one plays.
    failed_for: Option<f64>,
    /// Frames left before an unfocused openDJ hides the editor.
    grace: u32,
}

/// Keeps the docked editor over the panel; returns the timer, which must
/// live as long as the app.
pub fn install(app: &AppWindow, registry: Arc<ControlRegistry>) -> slint::Timer {
    let timer = slint::Timer::default();
    let Some(handles) = Handles::new(&registry) else {
        return timer;
    };
    let weak = app.as_weak();
    let mut state = State::default();
    timer.start(slint::TimerMode::Repeated, FRAME, move || {
        if let Some(app) = weak.upgrade() {
            follow(&app, &handles, &mut state);
        }
    });
    timer
}

fn follow(app: &AppWindow, h: &Handles, state: &mut State) {
    let ui = app.global::<InstrumentsUi>();
    let plugin = h.instrument.get();
    // A new plugin gets its chance at an editor again.
    if state.failed_for.is_some_and(|p| p != plugin) {
        state.failed_for = None;
        ui.set_panel_editor_failed(false);
    }
    // The engine let go of our ask: the plugin has no editor to show.
    if state.asked && !h.dock.get_bool() {
        state.asked = false;
        state.failed_for = Some(plugin);
        ui.set_panel_editor_failed(true);
    }
    let wanted = plugin > 0.0
        && ui.get_panel_shown()
        && !ui.get_has_display()
        && !ui.get_panel_controls()
        && state.failed_for.is_none();
    if !wanted {
        if state.asked {
            state.asked = false;
            h.dock.set_bool(false);
        }
        return;
    }
    if !state.asked {
        state.asked = true;
        h.dock.set_bool(true);
    }
    // Shown while openDJ or the editor itself has the keyboard, and
    // openDJ isn't minimised.
    let place = app.window().with_winit_window(|w| {
        let up = w.is_minimized() != Some(true) && w.is_visible() != Some(false);
        let focused = w.has_focus();
        let origin = w.inner_position().ok()?;
        Some((up, focused, origin, w.scale_factor()))
    });
    let Some(Some((up, focused, origin, scale))) = place else {
        // No position to follow (Wayland): the expand button's own window
        // still works.
        h.place((0, 0, 0, 0));
        return;
    };
    if focused || h.focused.get_bool() {
        state.grace = FOCUS_GRACE;
    } else {
        state.grace = state.grace.saturating_sub(1);
    }
    if !up || state.grace == 0 {
        h.place((0, 0, 0, 0));
        return;
    }
    let area = (
        ui.get_editor_area_x(),
        ui.get_editor_area_y(),
        ui.get_editor_area_width(),
        ui.get_editor_area_height(),
    );
    h.place(screen_rect((origin.x, origin.y), scale, area));
}

/// The panel's area on screen, in the units the helper places windows in:
/// points on macOS, pixels elsewhere. `origin` is the window's content in
/// pixels, `area` the panel's place in it in logical pixels.
fn screen_rect(origin: (i32, i32), scale: f64, area: (f32, f32, f32, f32)) -> (i32, i32, i32, i32) {
    let (x, y, w, h) = area;
    let (x, y, w, h) = (x as f64, y as f64, w as f64, h as f64);
    if cfg!(target_os = "macos") {
        let (ox, oy) = (origin.0 as f64 / scale, origin.1 as f64 / scale);
        (
            (ox + x).round() as i32,
            (oy + y).round() as i32,
            w.round() as i32,
            h.round() as i32,
        )
    } else {
        (
            origin.0 + (x * scale).round() as i32,
            origin.1 + (y * scale).round() as i32,
            (w * scale).round() as i32,
            (h * scale).round() as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::screen_rect;

    #[test]
    fn panel_area_on_screen() {
        let r = screen_rect((200, 100), 2.0, (60.0, 500.0, 800.0, 400.0));
        if cfg!(target_os = "macos") {
            // Points: the window's origin halved, the area as it is.
            assert_eq!(r, (160, 550, 800, 400));
        } else {
            assert_eq!(r, (320, 1100, 1600, 800));
        }
    }
}
