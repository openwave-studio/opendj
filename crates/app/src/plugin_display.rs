//! A plugin's own display on the deck (a Pd patch's Lua displays): the
//! helper draws it into a shared file (`opendj_plugin_host::display`), and
//! the instrument deck's DISPLAY tab shows that picture, sending clicks and
//! drags back.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use opendj_engine::plugins::plugin_display::{Display, Mouse};
use opendj_engine::AudioEngine;
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

use crate::{AppWindow, InstrumentsUi};

/// How often a shown display is looked at (about 30 pictures a second).
const FRAME: Duration = Duration::from_millis(33);

#[derive(Default)]
struct Shown {
    /// The display file and its mapping, once the helper made it.
    open: Option<(PathBuf, Display)>,
    /// The number of the picture on screen.
    seq: u32,
    pixels: Vec<u8>,
}

/// Keeps the instrument deck's display in step with the playing
/// instrument's; returns the timer, which must live as long as the app.
pub fn install(app: &AppWindow, engine: Rc<AudioEngine>) -> slint::Timer {
    let shown = Rc::new(RefCell::new(Shown::default()));
    {
        let shown = shown.clone();
        let weak = app.as_weak();
        app.global::<InstrumentsUi>()
            .on_display_mouse(move |x, y, kind| {
                let Some(app) = weak.upgrade() else { return };
                let ui = app.global::<InstrumentsUi>();
                let shown = shown.borrow();
                let Some((_, d)) = shown.open.as_ref() else {
                    return;
                };
                let (cw, ch) = d.content();
                let (aw, ah) = (ui.get_display_area_width(), ui.get_display_area_height());
                if cw == 0 || ch == 0 || aw <= 0.0 || ah <= 0.0 {
                    return;
                }
                // The picture sits centred in the area, at its own shape.
                let scale = (aw / cw as f32).min(ah / ch as f32);
                let (ox, oy) = (
                    (aw - cw as f32 * scale) / 2.0,
                    (ah - ch as f32 * scale) / 2.0,
                );
                let what = match kind {
                    0 => Mouse::Down,
                    1 => Mouse::Up,
                    _ => Mouse::Move,
                };
                d.send_mouse((x - ox) / scale, (y - oy) / scale, what);
            });
    }
    let timer = slint::Timer::default();
    let weak = app.as_weak();
    timer.start(slint::TimerMode::Repeated, FRAME, move || {
        let Some(app) = weak.upgrade() else { return };
        let ui = app.global::<InstrumentsUi>();
        let mut shown = shown.borrow_mut();
        let path = engine.instrument_display();
        if shown.open.as_ref().map(|(p, _)| p) != path.as_ref() {
            shown.open = None;
            shown.seq = 0;
        }
        if shown.open.is_none() {
            if let Some(path) = path {
                if let Some(d) = Display::open(&path) {
                    shown.open = Some((path, d));
                }
            }
        }
        let Shown {
            open,
            seq: on_screen,
            pixels,
        } = &mut *shown;
        let Some((_, d)) = open.as_ref() else {
            if ui.get_has_display() {
                ui.set_has_display(false);
                ui.set_display(Image::default());
            }
            return;
        };
        if !ui.get_has_display() {
            ui.set_has_display(true);
            // An instrument with no knobs of its own opens on its display.
            if engine.instrument_parameters().is_empty() {
                ui.set_display_on(true);
            }
        }
        let (cw, ch) = d.content();
        if ch > 0 {
            ui.set_display_aspect(cw as f32 / ch as f32);
        }
        if !ui.get_display_on() {
            d.set_view(0, 0);
            return;
        }
        let scale = app.window().scale_factor();
        let (aw, ah) = (ui.get_display_area_width(), ui.get_display_area_height());
        d.set_view(
            (aw * scale).round().max(0.0) as u32,
            (ah * scale).round().max(0.0) as u32,
        );
        if d.frame_seq() == *on_screen {
            return;
        }
        if let Some((w, h, seq)) = d.read_frame(pixels) {
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(pixels, w, h);
            ui.set_display(Image::from_rgba8(buffer));
            *on_screen = seq;
        }
    });
    timer
}
