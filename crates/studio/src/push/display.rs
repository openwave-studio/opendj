//! Push 2's screen: 960 x 160 pixels sent over USB, drawn by Slint's
//! software renderer (`ui/push.slint`) on the Push thread.
//!
//! Slint keeps its platform per thread, so the Push thread installs its own:
//! one off-screen software window, while the main thread keeps the normal
//! windowing backend. Frames are encoded as the manual's "Display Interface"
//! chapter asks: a 16-byte header, then 160 lines of 1920 bytes of pixels
//! and 128 filler bytes, XORed with a fixed pattern.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, WindowAdapter};
use slint::{Color, ComponentHandle, ModelRc, VecModel};

use super::surface::{Mode, Surface, View, RESOLUTIONS};
use crate::control::{fader_text, pan_text, Controls, Mix};
use crate::sound::PARAMS;
use crate::{PushKnob, PushMenu, PushScreen, PushTrack};

pub const WIDTH: usize = 960;
pub const HEIGHT: usize = 160;
const LINE_BYTES: usize = 2048;

const VENDOR: u16 = 0x2982;
const PRODUCT: u16 = 0x1967;
const ENDPOINT: u8 = 0x01;
const HEADER: [u8; 16] = [0xFF, 0xCC, 0xAA, 0x88, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const MASK: [u8; 4] = [0xE7, 0xF3, 0xE7, 0xFF];

/// A pixel as Push wants it: 16 bits, blue high, red low.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bgr565(pub u16);

impl Bgr565 {
    fn rgb(self) -> (u8, u8, u8) {
        let v = self.0;
        let r = (v & 0x1F) as u8;
        let g = ((v >> 5) & 0x3F) as u8;
        let b = (v >> 11) as u8;
        (r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2)
    }
}

impl TargetPixel for Bgr565 {
    fn blend(&mut self, c: PremultipliedRgbaColor) {
        let (r, g, b) = self.rgb();
        let a = (255 - c.alpha) as u16;
        let mix = |x: u8, add: u8| ((x as u16 * a / 255) as u8).saturating_add(add);
        *self = Bgr565::from_rgb(mix(r, c.red), mix(g, c.green), mix(b, c.blue));
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Bgr565((b as u16 >> 3) << 11 | (g as u16 >> 2) << 5 | r as u16 >> 3)
    }
}

/// One screen's pixels, top line first.
pub struct Frame {
    pixels: Vec<Bgr565>,
}

impl Default for Frame {
    fn default() -> Self {
        Frame::new()
    }
}

impl Frame {
    pub fn new() -> Frame {
        Frame {
            pixels: vec![Bgr565(0); WIDTH * HEIGHT],
        }
    }

    /// The bytes Push wants after the header.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; LINE_BYTES * HEIGHT];
        for (y, line) in self.pixels.chunks(WIDTH).enumerate() {
            let row = &mut out[y * LINE_BYTES..(y + 1) * LINE_BYTES];
            for (x, p) in line.iter().enumerate() {
                row[2 * x..2 * x + 2].copy_from_slice(&p.0.to_le_bytes());
            }
        }
        for (i, b) in out.iter_mut().enumerate() {
            *b ^= MASK[i % 4];
        }
        out
    }

    /// The frame as 8-bit RGB, for a preview image.
    pub fn rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WIDTH * HEIGHT * 3);
        for p in &self.pixels {
            let (r, g, b) = p.rgb();
            out.extend_from_slice(&[r, g, b]);
        }
        out
    }
}

// ------------------------------------------------------------ rendering

/// The Push thread's Slint platform: one off-screen window.
struct Offscreen {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> Duration {
        self.start.elapsed()
    }
}

thread_local! {
    static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

/// This thread's off-screen window, installing the platform the first time.
fn window() -> Result<Rc<MinimalSoftwareWindow>, String> {
    WINDOW.with(|w| {
        if let Some(w) = w.borrow().as_ref() {
            return Ok(w.clone());
        }
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(Offscreen {
            window: window.clone(),
            start: Instant::now(),
        }))
        .map_err(|e| format!("Push screen: {e}"))?;
        *w.borrow_mut() = Some(window.clone());
        Ok(window)
    })
}

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

const ACCENT: u32 = 0xb69cff;

/// Draws the Push screen. Lives on one thread (Slint components can't move
/// between threads).
pub struct Renderer {
    window: Rc<MinimalSoftwareWindow>,
    ui: PushScreen,
    tracks: Rc<VecModel<PushTrack>>,
    knobs: Rc<VecModel<PushKnob>>,
    upper: Rc<VecModel<PushMenu>>,
    lower: Rc<VecModel<PushMenu>>,
    frame: Frame,
    /// The page last shown, to flash its name when it changes.
    page: Option<(View, Mode, crate::steps::Layer)>,
    toast_until: Option<Instant>,
}

impl Renderer {
    pub fn new() -> Result<Renderer, String> {
        let window = window()?;
        let ui = PushScreen::new().map_err(|e| e.to_string())?;
        ui.show().map_err(|e| e.to_string())?;
        window.set_size(slint::PhysicalSize::new(WIDTH as u32, HEIGHT as u32));
        let tracks = Rc::new(VecModel::default());
        let knobs = Rc::new(VecModel::default());
        ui.set_tracks(ModelRc::from(tracks.clone()));
        ui.set_knobs(ModelRc::from(knobs.clone()));
        let (upper, lower) = (Rc::new(VecModel::default()), Rc::new(VecModel::default()));
        ui.set_upper_menu(ModelRc::from(upper.clone()));
        ui.set_lower_menu(ModelRc::from(lower.clone()));
        Ok(Renderer {
            window,
            ui,
            tracks,
            knobs,
            upper,
            lower,
            frame: Frame::new(),
            page: None,
            toast_until: None,
        })
    }

    /// Copies the studio's state into the screen.
    pub fn update(&mut self, c: &Controls, surface: &Surface) {
        let m = &c.monitor;
        let selected = c.selected_track();
        let tracks: Vec<PushTrack> = c
            .tracks
            .iter()
            .enumerate()
            .skip(super::surface::bank(c))
            .take(8)
            .map(|(t, info)| PushTrack {
                name: info.name.as_str().into(),
                color: color(info.color),
                selected: t == selected,
                progress: m.progress(t),
                stopping: m.stopping(t),
            })
            .collect();
        set_rows(&self.tracks, tracks);

        // Steps mode with stages held: the knobs show (and set) their locks.
        let (st, sc) = c.steps_slot();
        let stages = if surface.view == View::Steps {
            c.stages(st, sc)
        } else {
            None
        };
        let held = surface.held_stages();
        let lock_view = stages.as_ref().zip(held.first());
        // Drum sequencer with steps held: their notes' velocity, length and
        // chance.
        let held_notes = if surface.sequencing(c) {
            surface.held_notes(c)
        } else {
            Vec::new()
        };
        let knobs: Vec<PushKnob> = match surface.mode {
            _ if !held_notes.is_empty() => {
                let n = held_notes[0];
                let rgb = color(c.tracks[selected].color);
                let knob = |name: &str, text: String, value: f32| PushKnob {
                    name: name.into(),
                    text: text.into(),
                    value,
                    bipolar: false,
                    color: rgb,
                };
                let steps = n.length / surface.resolution();
                vec![
                    knob("Velocity", format!("{:.0}", n.velocity * 127.0), n.velocity),
                    knob(
                        "Length",
                        format!("{steps:.2} steps"),
                        (steps / 16.0).min(1.0) as f32,
                    ),
                    knob("Chance", format!("{:.0}%", n.chance * 100.0), n.chance),
                ]
            }
            _ if lock_view.is_some() => {
                let (stages, &h) = lock_view.expect("checked");
                let stage = &stages.stages[h];
                (0..PARAMS)
                    .map(|i| {
                        let lock = stage.locks[i];
                        let v = lock.unwrap_or_else(|| c.param(st, i));
                        PushKnob {
                            name: c.param_name(st, i).into(),
                            text: c.param_text_at(st, i, v).into(),
                            value: v,
                            bipolar: false,
                            // Locked knobs in white.
                            color: if lock.is_some() {
                                color(0xffffff)
                            } else {
                                color(0x5a5d62)
                            },
                        }
                    })
                    .collect()
            }
            Mode::Device => {
                let rgb = color(c.tracks[selected].color);
                (0..PARAMS)
                    .map(|i| PushKnob {
                        name: c.param_name(selected, i).into(),
                        text: c.param_text(selected, i).into(),
                        value: c.param(selected, i),
                        bipolar: false,
                        color: rgb,
                    })
                    .collect()
            }
            Mode::Mix(mix) => {
                let mut v: Vec<PushKnob> = c
                    .tracks
                    .iter()
                    .enumerate()
                    .skip(super::surface::bank(c))
                    .take(8)
                    .map(|(t, info)| {
                        let x = c.mix(t, mix);
                        PushKnob {
                            name: info.name.as_str().into(),
                            text: match mix {
                                Mix::Volume => format!("{} dB", fader_text(x)),
                                Mix::Pan => pan_text(x),
                                Mix::SendA | Mix::SendB => format!("{:.0}%", x * 100.0),
                            }
                            .into(),
                            value: x,
                            bipolar: mix == Mix::Pan,
                            color: color(info.color),
                        }
                    })
                    .collect();
                while v.len() < 7 {
                    v.push(PushKnob::default());
                }
                if v.len() < 8 {
                    let x = c.main_volume();
                    v.push(PushKnob {
                        name: "Main".into(),
                        text: format!("{} dB", fader_text(x)).into(),
                        value: x,
                        bipolar: false,
                        color: color(ACCENT),
                    });
                }
                v
            }
            Mode::Scales => {
                let key = c.key();
                let n = crate::scale::SCALES.len();
                let layouts = crate::scale::Layout::ALL;
                let at = layouts.iter().position(|l| *l == key.layout).unwrap_or(0);
                let knob = |name: &str, text: String, value: f32| PushKnob {
                    name: name.into(),
                    text: text.into(),
                    value,
                    bipolar: false,
                    color: color(ACCENT),
                };
                vec![
                    knob(
                        "Scale",
                        key.scale().name.to_string(),
                        key.scale as f32 / (n - 1) as f32,
                    ),
                    knob(
                        "Root",
                        crate::scale::NOTE_NAMES[key.root as usize].to_string(),
                        key.root as f32 / 11.0,
                    ),
                    knob(
                        "Layout",
                        key.layout.label().to_string(),
                        at as f32 / (layouts.len() - 1) as f32,
                    ),
                    knob(
                        "Pads",
                        if key.chromatic { "Chromatic" } else { "In key" }.to_string(),
                        key.chromatic as u8 as f32,
                    ),
                ]
            }
        };
        set_rows(&self.knobs, knobs);
        let (upper, lower) = surface.menus(c).unwrap_or_default();
        let menu = |m: Vec<super::surface::Menu>| {
            m.into_iter()
                .map(|m| PushMenu {
                    label: m.label.into(),
                    on: m.on,
                })
                .collect()
        };
        set_rows(&self.upper, menu(upper));
        set_rows(&self.lower, menu(lower));

        let name = &c.tracks[selected].name;
        let clip_name = c
            .clip_info(st, sc)
            .map_or_else(|| "new".to_string(), |(n, _)| n);
        let title = match (surface.view, surface.mode) {
            (_, Mode::Scales) => format!("Scale · {}", c.key().name()),
            (View::Note, _) if surface.sequencing(c) => {
                let sound = c.drum_name(selected, surface.drum);
                let held = surface.held_steps();
                match held.first() {
                    Some(h) => format!(
                        "Note · {sound} · step {}{}",
                        surface.page * 32 + h + 1,
                        if held_notes.len() > 1 { " +" } else { "" }
                    ),
                    None => {
                        let (beats, _) = c.notes(st, sc).unwrap_or((4.0, Vec::new()));
                        let pages = (beats / (32.0 * surface.resolution()) - 1e-6)
                            .ceil()
                            .max(1.0);
                        format!(
                            "{sound} · {} · page {}/{pages} · {name} {}",
                            RESOLUTIONS[surface.res].1,
                            surface.page + 1,
                            sc + 1
                        )
                    }
                }
            }
            (View::Note, _) => format!("Note · {name}"),
            (View::Steps, _) if stages.is_none() => {
                format!("Steps · {name} {} holds notes: pick another scene", sc + 1)
            }
            (View::Steps, _) if !held.is_empty() => format!(
                "Locks · stage {} · {name} {}",
                held.iter()
                    .map(|h| (h + 1).to_string())
                    .collect::<Vec<_>>()
                    .join("+"),
                sc + 1
            ),
            (View::Steps, _) => format!(
                "Steps · {} · {name} {} {clip_name}",
                surface.layer.label(),
                sc + 1
            ),
            (_, Mode::Device) => format!("Device · {name}"),
            (_, Mode::Mix(mix)) => format!("Mix · {}", mix.label()),
        };
        let page = (surface.view, surface.mode, surface.layer);
        if self.page.is_some_and(|p| p != page) {
            self.ui.set_toast(title.as_str().into());
            self.ui.set_toast_visible(true);
            self.toast_until = Some(Instant::now() + Duration::from_millis(900));
        }
        self.page = Some(page);
        if self.toast_until.is_some_and(|t| Instant::now() > t) {
            self.ui.set_toast_visible(false);
            self.toast_until = None;
        }
        self.ui.set_heading(title.into());
        self.ui
            .set_touched(surface.touched().map_or(-1, |t| t as i32));
        let beat = m.beat();
        let bar = (beat / 4.0).floor() as i64 + 1;
        let in_bar = beat.rem_euclid(4.0).floor() as i64 + 1;
        self.ui.set_tempo(format!("{:.1} BPM", m.tempo()).into());
        self.ui.set_position(format!("{bar}.{in_bar}").into());
        self.ui.set_playing(m.playing());
        self.ui.set_beat(beat as f32);
    }

    /// Runs Slint's animations and redraws what changed; true when the
    /// frame changed.
    pub fn draw(&mut self) -> bool {
        slint::platform::update_timers_and_animations();
        let frame = &mut self.frame;
        self.window.draw_if_needed(|r| {
            r.render(&mut frame.pixels, WIDTH);
        })
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }
}

/// Replaces `model`'s rows, touching only those that changed.
fn set_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    use slint::Model;
    if model.row_count() != rows.len() {
        model.set_vec(rows);
        return;
    }
    for (i, row) in rows.into_iter().enumerate() {
        if model.row_data(i).as_ref() != Some(&row) {
            model.set_row_data(i, row);
        }
    }
}

// ------------------------------------------------------------------ USB

/// The open screen.
pub struct Screen {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
}

impl Screen {
    /// Opens Push 2's screen, if one is plugged in.
    pub fn open() -> Option<Screen> {
        let handle = rusb::open_device_with_vid_pid(VENDOR, PRODUCT)?;
        handle.claim_interface(0).ok()?;
        Some(Screen { handle })
    }

    pub fn send(&self, frame: &Frame) -> rusb::Result<()> {
        let timeout = Duration::from_millis(1000);
        self.handle.write_bulk(ENDPOINT, &HEADER, timeout)?;
        let data = frame.encode();
        // Larger transfers are faster; 16 KB as in the manual's example.
        for chunk in data.chunks(16 * 1024) {
            self.handle.write_bulk(ENDPOINT, chunk, timeout)?;
        }
        Ok(())
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_encode_as_push_expects() {
        let mut f = Frame::new();
        // Top-left pixel pure red: red is the low 5 bits.
        f.pixels[0] = Bgr565::from_rgb(255, 0, 0);
        assert_eq!(f.pixels[0].0, 0x001F);
        assert_eq!(Bgr565::from_rgb(0, 0, 255).0, 0xF800);
        let data = f.encode();
        assert_eq!(data.len(), 160 * 2048);
        // 0x1F ^ 0xE7, 0x00 ^ 0xF3; black elsewhere is the mask itself.
        assert_eq!(&data[..6], &[0x1F ^ 0xE7, 0xF3, 0xE7, 0xFF, 0xE7, 0xF3]);
        // Lines start on a fresh 2 KB boundary.
        assert_eq!(&data[2048..2052], &MASK);
    }

    #[test]
    fn blending_half_white_over_black() {
        let mut p = Bgr565(0);
        p.blend(PremultipliedRgbaColor {
            red: 128,
            green: 128,
            blue: 128,
            alpha: 128,
        });
        let (r, g, b) = p.rgb();
        assert!((120..=136).contains(&r) && (120..=136).contains(&g) && (120..=136).contains(&b));
    }

    #[test]
    fn draws_the_tracks_and_knobs() {
        let set = crate::clip::demo_set();
        let (tx, _rx) = rtrb::RingBuffer::new(8);
        let monitor = crate::engine::Studio::new(&set, 48_000.0, 120.0).1.monitor;
        let c = Controls::new(&set, &[0xff8a65, 0x4fc3f7, 0xb69cff, 0xf48fb1], tx, monitor);
        let mut r = Renderer::new().expect("off-screen renderer");
        r.update(&c, &Surface::new());
        assert!(r.draw());
        // The selected track's tab is filled with its colour (after its
        // fill animation), and the knobs drew something.
        let lit = r.frame().pixels.iter().filter(|p| p.0 != 0).count();
        assert!(lit > 10_000, "{lit} pixels lit");
    }
}
