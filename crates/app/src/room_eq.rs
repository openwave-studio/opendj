//! Settings › Audio › Room EQ: the main output's and the booth's Room EQ,
//! saved in `settings.json` under `room_eq` and played through the
//! `[RoomEQ_Main]` and `[RoomEQ_Booth]` controls.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use opendj_control::ControlRegistry;
use opendj_engine::room_eq::{
    log_freqs, response, Band, Kind, BANDS, BOOTH_GROUP, FLAT, KINDS, MAIN_GROUP, MAX_FREQ,
    MAX_GAIN, MAX_Q, MIN_FREQ, MIN_Q,
};
use opendj_engine::Role;
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{settings, AppWindow, RoomBand, RoomDot, RoomEqUi};

/// One output's Room EQ as saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub on: bool,
    pub bands: [Band; BANDS],
}

impl Default for Curve {
    fn default() -> Self {
        Curve {
            on: false,
            bands: FLAT,
        }
    }
}

/// The settings keys of the two outputs, and their control groups.
const TARGETS: [(&str, &str); 2] = [("main", MAIN_GROUP), ("booth", BOOTH_GROUP)];

/// Band names, in the order of `KINDS`.
const NAMES: [&str; BANDS] = [
    "Low cut",
    "Low shelf",
    "Bell 1",
    "Bell 2",
    "High shelf",
    "High cut",
];

/// The sample rate the curve is drawn at.
const DRAW_RATE: f64 = 48_000.0;
/// How near a dot a press must be to grab it, in pixels.
const GRAB: f32 = 14.0;

fn curve_from(v: &Value) -> Curve {
    let mut c = Curve {
        on: v["on"].as_bool().unwrap_or(false),
        bands: FLAT,
    };
    if let Some(bands) = v["bands"].as_array() {
        for (i, b) in bands.iter().take(BANDS).enumerate() {
            let flat = FLAT[i];
            c.bands[i] = Band {
                on: b["on"].as_bool().unwrap_or(flat.on),
                freq: b["freq"].as_f64().unwrap_or(flat.freq),
                gain: b["gain"].as_f64().unwrap_or(flat.gain),
                q: b["q"].as_f64().unwrap_or(flat.q),
            }
            .clamped(KINDS[i]);
        }
    }
    c
}

fn curve_json(c: &Curve) -> Value {
    json!({
        "on": c.on,
        "bands": c.bands.iter().map(|b| json!({
            "on": b.on, "freq": b.freq, "gain": b.gain, "q": b.q,
        })).collect::<Vec<_>>(),
    })
}

/// The saved Room EQs: main, then booth.
pub fn saved() -> [Curve; 2] {
    let v = settings::load();
    TARGETS.map(|(key, _)| curve_from(&v["room_eq"][key]))
}

fn save(curves: &[Curve; 2]) {
    let v = json!({ "main": curve_json(&curves[0]), "booth": curve_json(&curves[1]) });
    settings::set("room_eq", v);
}

/// Puts `curve` on the controls of output `target` (0 main, 1 booth).
fn play(registry: &ControlRegistry, target: usize, curve: &Curve) {
    let group = TARGETS[target].1;
    let set = |key: &str, value: f64| {
        if let Some(c) = registry.get(group, key) {
            c.set(value);
        }
    };
    for (i, b) in curve.bands.iter().enumerate() {
        let n = i + 1;
        set(&format!("b{n}_freq"), b.freq);
        set(&format!("b{n}_gain"), b.gain);
        set(&format!("b{n}_q"), b.q);
        set(&format!("b{n}_on"), if b.on { 1.0 } else { 0.0 });
    }
    set("enabled", if curve.on { 1.0 } else { 0.0 });
}

/// Applies the saved Room EQs to the engine's controls, at start.
pub fn apply_saved(registry: &ControlRegistry) {
    for (t, c) in saved().iter().enumerate() {
        play(registry, t, c);
    }
}

/// Where `freq` sits on the curve's axis, from 0 (20 Hz) to 1 (20 kHz).
fn x_of(freq: f64) -> f64 {
    (freq / MIN_FREQ).log2() / (MAX_FREQ / MIN_FREQ).log2()
}

fn freq_at(x: f64) -> f64 {
    MIN_FREQ * (MAX_FREQ / MIN_FREQ).powf(x.clamp(0.0, 1.0))
}

/// Where `db` sits, from 0 (+12 dB) to 1 (−12 dB).
fn y_of(db: f64) -> f64 {
    ((MAX_GAIN - db) / (2.0 * MAX_GAIN)).clamp(0.0, 1.0)
}

/// A frequency as the cards show it: "250 Hz", "3.2 kHz".
pub fn hz_text(f: f64) -> String {
    if f >= 1000.0 {
        let k = f / 1000.0;
        if k >= 10.0 {
            format!("{k:.0} kHz")
        } else {
            format!("{k:.1} kHz")
        }
    } else {
        format!("{f:.0} Hz")
    }
}

/// A gain as the cards show it, with a true minus sign.
fn db_text(db: f64) -> String {
    if db.abs() < 0.05 {
        "0 dB".into()
    } else {
        format!("{}{:.1} dB", if db < 0.0 { "−" } else { "+" }, db.abs())
    }
}

/// Rounds a frequency to what's worth showing: 1 Hz under 1 kHz, 10 Hz
/// under 10 kHz, 100 Hz above.
fn round_freq(f: f64) -> f64 {
    let step = if f < 1000.0 {
        1.0
    } else if f < 10_000.0 {
        10.0
    } else {
        100.0
    };
    (f / step).round() * step
}

fn card(i: usize, b: &Band) -> RoomBand {
    let kind = KINDS[i];
    let (freq, detail) = if kind.is_cut() {
        if b.on {
            (hz_text(b.freq), format!("{:.0} dB/oct", b.q))
        } else {
            ("Off".into(), "—".into())
        }
    } else if !b.on {
        ("Off".into(), "—".into())
    } else if kind == Kind::Bell {
        (
            hz_text(b.freq),
            format!("{} · Q {:.1}", db_text(b.gain), b.q),
        )
    } else {
        (hz_text(b.freq), db_text(b.gain))
    };
    RoomBand {
        name: NAMES[i].into(),
        freq: freq.into(),
        detail: detail.into(),
        on: b.on,
    }
}

struct State {
    curves: [Curve; 2],
    target: usize,
    /// The dot being dragged.
    grabbed: Option<usize>,
    /// The plot's size in pixels.
    plot: (f64, f64),
    registry: Arc<ControlRegistry>,
}

impl State {
    fn curve(&mut self) -> &mut Curve {
        &mut self.curves[self.target]
    }

    /// The dots, from 0 to 1 across and down the curve box.
    fn dots(&self) -> Vec<(f64, f64)> {
        let c = &self.curves[self.target];
        c.bands
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let y = if KINDS[i].is_cut() {
                    // A cut has no gain: its dot sits on the curve at its
                    // corner (as if on, so an off one shows where it'd cut).
                    let mut bands = c.bands;
                    bands[i].on = true;
                    y_of(response(&bands, DRAW_RATE, &[b.freq])[0])
                } else {
                    y_of(b.gain)
                };
                (x_of(b.freq), y)
            })
            .collect()
    }

    /// The band whose dot is within reach of (x, y) in a `w` × `h` box.
    fn hit(&self, x: f32, y: f32, w: f32, h: f32) -> Option<usize> {
        self.dots()
            .iter()
            .enumerate()
            .map(|(i, &(dx, dy))| {
                let (px, py) = ((dx as f32 - x) * w, (dy as f32 - y) * h);
                (i, (px * px + py * py).sqrt())
            })
            .filter(|&(_, d)| d <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The controls take the change, and the page shows it.
    fn changed(&self, app: &AppWindow) {
        play(&self.registry, self.target, &self.curves[self.target]);
        self.show(app);
    }

    fn show(&self, app: &AppWindow) {
        let ui = app.global::<RoomEqUi>();
        let c = &self.curves[self.target];
        let freqs = log_freqs(300);
        let db = response(&c.bands, DRAW_RATE, &freqs);
        let mut path = String::new();
        for (k, (f, d)) in freqs.iter().zip(&db).enumerate() {
            path.push_str(&format!(
                "{} {:.1} {:.1} ",
                if k == 0 { "M" } else { "L" },
                x_of(*f) * self.plot.0,
                y_of(*d) * self.plot.1
            ));
        }
        ui.set_curve(SharedString::from(path.trim_end()));
        let dots: Vec<RoomDot> = self
            .dots()
            .into_iter()
            .zip(&c.bands)
            .map(|((x, y), b)| RoomDot {
                x: x as f32,
                y: y as f32,
                on: b.on && c.on,
            })
            .collect();
        ui.set_dots(ModelRc::new(VecModel::from(dots)));
        let cards: Vec<RoomBand> = c
            .bands
            .iter()
            .enumerate()
            .map(|(i, b)| card(i, b))
            .collect();
        ui.set_bands(ModelRc::new(VecModel::from(cards)));
        ui.set_on(c.on);
        ui.set_target(self.target as i32);
        let booth = settings::audio_setup()
            .routes
            .iter()
            .any(|r| r.role == Role::Booth);
        ui.set_note(if self.target == 1 && !booth {
            "No output is the booth yet: give one the Booth role on the Devices tab.".into()
        } else {
            SharedString::new()
        });
    }
}

/// Wires the Room EQ tab to the controls and the saved settings.
pub fn install(app: &AppWindow, registry: Arc<ControlRegistry>) {
    let state = Rc::new(RefCell::new(State {
        curves: saved(),
        target: 0,
        grabbed: None,
        plot: (680.0, 210.0),
        registry,
    }));
    state.borrow().show(app);
    let ui = app.global::<RoomEqUi>();
    // Runs `f` on the state; when it returns true, plays and shows the
    // change, and saves it when `keep`.
    let run = {
        let (state, weak) = (state.clone(), app.as_weak());
        move |keep: bool, f: &dyn Fn(&mut State, &RoomEqUi) -> bool| -> bool {
            let Some(app) = weak.upgrade() else {
                return false;
            };
            let mut s = state.borrow_mut();
            let ui = app.global::<RoomEqUi>();
            let changed = f(&mut s, &ui);
            if changed {
                s.changed(&app);
                if keep {
                    save(&s.curves);
                }
            }
            changed
        }
    };
    let r = run.clone();
    ui.on_resized(move |w, h| {
        r(false, &|s, _| {
            let size = (w as f64, h as f64);
            let changed = size != s.plot && w > 0.0 && h > 0.0;
            if changed {
                s.plot = size;
            }
            changed
        });
    });
    let r = run.clone();
    ui.on_pick_target(move |t| {
        r(false, &|s, _| {
            s.target = t.clamp(0, 1) as usize;
            true
        });
    });
    let r = run.clone();
    ui.on_toggled(move |on| {
        r(true, &|s, _| {
            s.curve().on = on;
            true
        });
    });
    let r = run.clone();
    ui.on_band_toggled(move |i, on| {
        r(true, &|s, ui| {
            let i = i as usize;
            ui.set_selected(i as i32);
            s.curve().bands[i].on = on;
            if on {
                s.curve().on = true;
            }
            true
        });
    });
    let r = run.clone();
    ui.on_select(move |i| {
        r(false, &|_, ui| {
            ui.set_selected(i);
            false
        });
    });
    let r = run.clone();
    ui.on_press(move |x, y, w, h| {
        r(false, &|s, ui| {
            s.grabbed = s.hit(x, y, w, h);
            if let Some(i) = s.grabbed {
                ui.set_selected(i as i32);
            }
            false
        });
        true
    });
    let r = run.clone();
    ui.on_drag(move |x, y| {
        r(false, &|s, _| {
            let Some(i) = s.grabbed else {
                return false;
            };
            // Editing turns the band and the Room EQ on.
            let c = s.curve();
            c.on = true;
            let b = &mut c.bands[i];
            b.on = true;
            b.freq = round_freq(freq_at(x as f64));
            if !KINDS[i].is_cut() {
                let db = MAX_GAIN - 2.0 * MAX_GAIN * (y as f64).clamp(0.0, 1.0);
                b.gain = (db * 10.0).round() / 10.0;
            }
            *b = b.clamped(KINDS[i]);
            true
        });
    });
    let r = run.clone();
    ui.on_released(move || {
        r(true, &|s, _| s.grabbed.take().is_some());
    });
    let r = run.clone();
    ui.on_wheel(move |x, y, w, h, steps| {
        r(true, &|s, ui| {
            let Some(i) = s.hit(x, y, w, h) else {
                return false;
            };
            ui.set_selected(i as i32);
            let b = &mut s.curve().bands[i];
            match KINDS[i] {
                Kind::Bell => b.q = (b.q * 1.15f64.powf(steps as f64)).clamp(MIN_Q, MAX_Q),
                k if k.is_cut() => b.q = if steps > 0.0 { 24.0 } else { 12.0 },
                _ => return false,
            }
            b.q = (b.q * 100.0).round() / 100.0;
            true
        })
    });
    let r = run.clone();
    ui.on_double(move |x, y, w, h| {
        r(true, &|s, ui| {
            let Some(i) = s.hit(x, y, w, h) else {
                return false;
            };
            ui.set_selected(i as i32);
            s.curve().bands[i] = FLAT[i];
            true
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_survives_saving() {
        let mut c = Curve::default();
        c.on = true;
        c.bands[2].gain = -3.0;
        c.bands[5] = Band {
            on: true,
            freq: 16_000.0,
            gain: 0.0,
            q: 24.0,
        };
        assert_eq!(curve_from(&curve_json(&c)), c);
        // Nothing saved: flat and off.
        assert_eq!(curve_from(&Value::Null), Curve::default());
    }

    #[test]
    fn axes_and_labels() {
        assert!((x_of(20.0)).abs() < 1e-9 && (x_of(20_000.0) - 1.0).abs() < 1e-9);
        assert!((freq_at(x_of(250.0)) - 250.0).abs() < 1e-6);
        assert_eq!(y_of(12.0), 0.0);
        assert_eq!(y_of(0.0), 0.5);
        assert_eq!(hz_text(250.0), "250 Hz");
        assert_eq!(hz_text(3200.0), "3.2 kHz");
        assert_eq!(hz_text(12_000.0), "12 kHz");
        assert_eq!(db_text(-3.0), "−3.0 dB");
        assert_eq!(round_freq(3217.0), 3220.0);
    }
}
