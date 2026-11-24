//! A plugin's own display (a Pd patch's Lua displays) inside openDJ: the
//! helper draws it into a shared file (`opendj_plugin_host::display`), and
//! openDJ shows that picture, sending clicks and drags back. The
//! instrument deck shows its instrument's display under its DISPLAY tab;
//! an FX slot's opens in a panel from the slot's VIEW button.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use opendj_engine::plugins::plugin_display::{Display, Mouse};
use opendj_engine::{effect_group, AudioEngine, EFFECTS_PER_UNIT, NUM_EFFECT_UNITS};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

use crate::{AppWindow, FxDisplay, InstrumentsUi};

/// How often a shown display is looked at (about 30 pictures a second).
const FRAME: Duration = Duration::from_millis(33);
/// How many frames between looks at which FX slots have a display.
const SLOT_LOOK: u32 = 15;

/// One display on screen: the file it comes from and the picture shown.
#[derive(Default)]
struct Viewer {
    /// The display file and its mapping, once the helper made it.
    open: Option<(PathBuf, Display)>,
    /// The number of the picture on screen.
    seq: u32,
    pixels: Vec<u8>,
}

impl Viewer {
    /// Follows the display at `path`; false while there is none.
    fn follow(&mut self, path: Option<&Path>) -> bool {
        if self.open.as_ref().map(|(p, _)| p.as_path()) != path {
            self.open = None;
            self.seq = 0;
        }
        if self.open.is_none() {
            if let Some(path) = path {
                self.open = Display::open(path).map(|d| (path.to_path_buf(), d));
            }
        }
        self.open.is_some()
    }

    fn display(&self) -> Option<&Display> {
        self.open.as_ref().map(|(_, d)| d)
    }

    /// The display's shape (width / height).
    fn aspect(&self) -> Option<f32> {
        let (w, h) = self.display()?.content();
        (w > 0 && h > 0).then(|| w as f32 / h as f32)
    }

    /// Shows the display in an area `w` by `h` logical pixels (0 hides it),
    /// and returns a new picture once the helper drew one.
    fn frame(&mut self, w: f32, h: f32, scale: f32) -> Option<Image> {
        let Viewer { open, seq, pixels } = self;
        let (_, d) = open.as_ref()?;
        d.set_view(
            (w * scale).round().max(0.0) as u32,
            (h * scale).round().max(0.0) as u32,
        );
        if w <= 0.0 || h <= 0.0 || d.frame_seq() == *seq {
            return None;
        }
        let (fw, fh, new) = d.read_frame(pixels)?;
        *seq = new;
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(pixels, fw, fh);
        Some(Image::from_rgba8(buffer))
    }

    fn hide(&self) {
        if let Some(d) = self.display() {
            d.set_view(0, 0);
        }
    }

    /// A mouse event at (x, y) in an area `w` by `h`, where the picture
    /// sits centred at its own shape.
    fn mouse(&self, x: f32, y: f32, kind: i32, w: f32, h: f32) {
        let Some(d) = self.display() else { return };
        let (cw, ch) = d.content();
        if cw == 0 || ch == 0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let scale = (w / cw as f32).min(h / ch as f32);
        let (ox, oy) = ((w - cw as f32 * scale) / 2.0, (h - ch as f32 * scale) / 2.0);
        let what = match kind {
            0 => Mouse::Down,
            1 => Mouse::Up,
            _ => Mouse::Move,
        };
        d.send_mouse((x - ox) / scale, (y - oy) / scale, what);
    }
}

#[derive(Default)]
struct State {
    instrument: Viewer,
    fx: Viewer,
    /// The FX slots with a display: control group, file, plugin name.
    slots: Vec<(String, PathBuf, String)>,
    /// Frames until the next look at the slots.
    look_in: u32,
    version: i32,
}

/// Keeps the instrument deck's display and the FX display panel in step
/// with the plugins playing; returns the timer, which must live as long
/// as the app.
pub fn install(app: &AppWindow, engine: Rc<AudioEngine>) -> slint::Timer {
    let state = Rc::new(RefCell::new(State::default()));
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.global::<InstrumentsUi>()
            .on_display_mouse(move |x, y, kind| {
                let Some(app) = weak.upgrade() else { return };
                let ui = app.global::<InstrumentsUi>();
                let (w, h) = (ui.get_display_area_width(), ui.get_display_area_height());
                state.borrow().instrument.mouse(x, y, kind, w, h);
            });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.global::<FxDisplay>().on_mouse(move |x, y, kind| {
            let Some(app) = weak.upgrade() else { return };
            let ui = app.global::<FxDisplay>();
            let (w, h) = (ui.get_area_width(), ui.get_area_height());
            state.borrow().fx.mouse(x, y, kind, w, h);
        });
    }
    {
        let state = state.clone();
        app.global::<FxDisplay>()
            .on_has(move |fx, _| state.borrow().slots.iter().any(|(g, _, _)| *g == *fx));
    }
    let timer = slint::Timer::default();
    let weak = app.as_weak();
    timer.start(slint::TimerMode::Repeated, FRAME, move || {
        let Some(app) = weak.upgrade() else { return };
        let scale = app.window().scale_factor();
        let mut state = state.borrow_mut();
        show_instrument(&app, &engine, &mut state.instrument, scale);
        if state.look_in == 0 {
            state.look_in = SLOT_LOOK;
            look_at_slots(&app, &engine, &mut state);
        }
        state.look_in -= 1;
        show_fx(&app, &mut state, scale);
    });
    timer
}

fn show_instrument(app: &AppWindow, engine: &AudioEngine, viewer: &mut Viewer, scale: f32) {
    let ui = app.global::<InstrumentsUi>();
    if !viewer.follow(engine.instrument_display().as_deref()) {
        if ui.get_has_display() {
            ui.set_has_display(false);
            ui.set_display(Image::default());
        }
        return;
    }
    if !ui.get_has_display() {
        ui.set_has_display(true);
        // An instrument with no knobs of its own opens on its display.
        if engine.instrument_parameters().is_empty() {
            ui.set_display_on(true);
        }
    }
    if let Some(aspect) = viewer.aspect() {
        ui.set_display_aspect(aspect);
    }
    if !ui.get_display_on() {
        viewer.hide();
        return;
    }
    let (w, h) = (ui.get_display_area_width(), ui.get_display_area_height());
    if let Some(image) = viewer.frame(w, h, scale) {
        ui.set_display(image);
    }
}

/// Finds the FX slots whose plugin draws a display.
fn look_at_slots(app: &AppWindow, engine: &AudioEngine, state: &mut State) {
    let mut slots = Vec::new();
    for u in 0..NUM_EFFECT_UNITS {
        for s in 0..EFFECTS_PER_UNIT {
            if let Some((path, name)) = engine.slot_display(u, s) {
                if path.exists() {
                    slots.push((effect_group(u, s), path, name));
                }
            }
        }
    }
    if slots != state.slots {
        state.slots = slots;
        state.version += 1;
        app.global::<FxDisplay>().set_version(state.version);
    }
}

fn show_fx(app: &AppWindow, state: &mut State, scale: f32) {
    let ui = app.global::<FxDisplay>();
    let shown = ui.get_shown();
    let slot = state
        .slots
        .iter()
        .find(|(g, _, _)| !shown.is_empty() && *g == *shown);
    let Some((_, path, name)) = slot else {
        if !shown.is_empty() {
            // Its plugin went.
            ui.set_shown("".into());
        }
        if state.fx.open.is_some() {
            state.fx.hide();
            state.fx = Viewer::default();
            ui.set_display(Image::default());
        }
        return;
    };
    if ui.get_title() != name.as_str() {
        ui.set_title(name.as_str().into());
    }
    let path = path.clone();
    if !state.fx.follow(Some(&path)) {
        return;
    }
    if let Some(aspect) = state.fx.aspect() {
        ui.set_aspect(aspect);
    }
    let (w, h) = (ui.get_area_width(), ui.get_area_height());
    if let Some(image) = state.fx.frame(w, h, scale) {
        ui.set_display(image);
    }
}
