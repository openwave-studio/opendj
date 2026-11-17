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

use crate::{settings, AppWindow, Preferences, RoomBand, RoomDot, RoomEqUi};

/// One output's Room EQ as saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub on: bool,
    pub bands: [Band; BANDS],
    /// The preset it was loaded from or saved as; empty for none.
    pub preset: String,
}

impl Default for Curve {
    fn default() -> Self {
        Curve {
            on: false,
            bands: FLAT,
            preset: String::new(),
        }
    }
}

/// A named curve: built in, or saved by the DJ for a venue.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub bands: [Band; BANDS],
}

/// FLAT with some bands changed.
fn flat_with(changes: &[(usize, f64, f64)]) -> [Band; BANDS] {
    let mut b = FLAT;
    for &(i, freq, gain) in changes {
        b[i].on = true;
        b[i].freq = freq;
        b[i].gain = gain;
    }
    b
}

/// The presets that come with openDJ.
pub fn built_in() -> Vec<Preset> {
    [
        ("Flat", FLAT),
        // Boomy low mids: a little less bass and 250 Hz.
        (
            "Muddy room",
            flat_with(&[(1, 120.0, -3.0), (2, 250.0, -3.0)]),
        ),
        // Hard walls and bright horns.
        (
            "Harsh highs",
            flat_with(&[(3, 3000.0, -3.0), (4, 10_000.0, -2.0)]),
        ),
        // No deep bass to give: cut under it, a little lift above.
        (
            "Small speakers",
            flat_with(&[(0, 60.0, 0.0), (2, 120.0, 2.0)]),
        ),
    ]
    .into_iter()
    .map(|(name, bands)| Preset {
        name: name.into(),
        bands,
    })
    .collect()
}

/// The DJ's own presets, as saved.
pub fn own() -> Vec<Preset> {
    settings::load()["room_eq_presets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    Some(Preset {
                        name: p["name"].as_str()?.to_string(),
                        bands: bands_from(&p["bands"]),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save_own(presets: &[Preset]) {
    let v: Vec<Value> = presets
        .iter()
        .map(|p| json!({ "name": p.name, "bands": bands_json(&p.bands) }))
        .collect();
    settings::set("room_eq_presets", json!(v));
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

fn bands_from(v: &Value) -> [Band; BANDS] {
    let mut bands = FLAT;
    if let Some(a) = v.as_array() {
        for (i, b) in a.iter().take(BANDS).enumerate() {
            let flat = FLAT[i];
            bands[i] = Band {
                on: b["on"].as_bool().unwrap_or(flat.on),
                freq: b["freq"].as_f64().unwrap_or(flat.freq),
                gain: b["gain"].as_f64().unwrap_or(flat.gain),
                q: b["q"].as_f64().unwrap_or(flat.q),
            }
            .clamped(KINDS[i]);
        }
    }
    bands
}

fn bands_json(bands: &[Band; BANDS]) -> Value {
    json!(bands
        .iter()
        .map(|b| json!({ "on": b.on, "freq": b.freq, "gain": b.gain, "q": b.q }))
        .collect::<Vec<_>>())
}

fn curve_from(v: &Value) -> Curve {
    Curve {
        on: v["on"].as_bool().unwrap_or(false),
        bands: bands_from(&v["bands"]),
        preset: v["preset"].as_str().unwrap_or("").to_string(),
    }
}

fn curve_json(c: &Curve) -> Value {
    json!({
        "on": c.on,
        "bands": bands_json(&c.bands),
        "preset": c.preset,
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

/// Where `db` sits on a curve `range` dB up and down, from 0 (+range) to
/// 1 (−range).
fn y_of(db: f64, range: f64) -> f64 {
    ((range - db) / (2.0 * range)).clamp(0.0, 1.0)
}

/// The gain at `y` (0 top, 1 bottom) on a curve `range` dB up and down.
fn db_at(y: f64, range: f64) -> f64 {
    range - 2.0 * range * y.clamp(0.0, 1.0)
}

/// The curve's ranges: ±12 dB, or ±6 to zoom in.
const RANGES: [f64; 2] = [MAX_GAIN / 2.0, MAX_GAIN];

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

/// What picking an entry of the preset menu does.
#[derive(Clone, Debug, PartialEq)]
enum MenuAction {
    Load(Box<Preset>),
    /// Saves the curve over the own preset it came from.
    Save,
    SaveAs,
    /// Back to the preset as saved.
    Revert,
    Delete,
}

/// The preset `curve` came from, if it still exists, and whether it is the
/// DJ's own.
fn preset_of(curve: &Curve, own: &[Preset]) -> Option<(Preset, bool)> {
    if curve.preset.is_empty() {
        return None;
    }
    if let Some(p) = own.iter().find(|p| p.name == curve.preset) {
        return Some((p.clone(), true));
    }
    built_in()
        .into_iter()
        .find(|p| p.name == curve.preset)
        .map(|p| (p, false))
}

/// The preset menu's entries for `curve`, and which one is in use.
fn menu(curve: &Curve, own: &[Preset]) -> (Vec<(String, MenuAction)>, Option<usize>) {
    let mut items: Vec<(String, MenuAction)> = built_in()
        .into_iter()
        .chain(own.iter().cloned())
        .map(|p| (p.name.clone(), MenuAction::Load(Box::new(p))))
        .collect();
    let chosen = items.iter().position(|(n, _)| *n == curve.preset);
    let from = preset_of(curve, own);
    let modified = from.as_ref().is_some_and(|(p, _)| p.bands != curve.bands);
    let mine = from.as_ref().is_some_and(|(_, mine)| *mine);
    if mine && modified {
        items.push((format!("Save “{}”", curve.preset), MenuAction::Save));
    }
    items.push(("Save as…".into(), MenuAction::SaveAs));
    if modified {
        items.push(("Revert".into(), MenuAction::Revert));
    }
    if mine {
        items.push((format!("Delete “{}”", curve.preset), MenuAction::Delete));
    }
    (items, chosen)
}

/// The preset in use as the menu shows it: its name, with an asterisk once
/// changed; Custom for none.
fn preset_label(curve: &Curve, own: &[Preset]) -> String {
    match preset_of(curve, own) {
        Some((p, _)) if p.bands != curve.bands => format!("{} *", p.name),
        Some((p, _)) => p.name,
        None => "Custom".into(),
    }
}

/// A name for a new preset that isn't taken: "Venue 1", "Venue 2"…
fn free_name(own: &[Preset]) -> String {
    (1..)
        .map(|n| format!("Venue {n}"))
        .find(|n| !own.iter().any(|p| p.name == *n))
        .unwrap_or_default()
}

/// A dot being dragged: its band, and how far it sits from the pointer
/// (from 0 to 1 across and down), so it moves with the pointer rather than
/// jumping to it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Grab {
    band: usize,
    dx: f64,
    dy: f64,
}

/// The spectrum's points' frequencies: the middle of each of `n` equal
/// steps from 20 Hz to 20 kHz, as the spectrum is drawn.
fn point_freqs(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| freq_at((i as f64 + 0.5) / n as f64))
        .collect()
}

/// The spectrum to draw, from the main output's (`post`, at `freqs`): the
/// mix before any Room EQ, or what `target` plays after its own.
fn view_levels(
    post: &[f64],
    freqs: &[f64],
    curves: &[Curve; 2],
    target: usize,
    after: bool,
) -> Vec<f64> {
    let floor = opendj_engine::room_eq::Spectrum::FLOOR;
    let gain = |c: &Curve| -> Vec<f64> {
        if c.on {
            response(&c.bands, DRAW_RATE, freqs)
        } else {
            vec![0.0; freqs.len()]
        }
    };
    if after && target == 0 {
        return post.to_vec();
    }
    let main = gain(&curves[0]);
    let own = if after {
        gain(&curves[target])
    } else {
        vec![0.0; freqs.len()]
    };
    post.iter()
        .zip(main.iter().zip(&own))
        .map(|(l, (m, o))| {
            if *l <= floor {
                floor
            } else {
                (l - m + o).max(floor)
            }
        })
        .collect()
}

/// The loudest point within `reach` points of point `i`.
fn peak_near(levels: &[f64], i: usize, reach: usize) -> usize {
    let lo = i.saturating_sub(reach);
    let hi = (i + reach + 1).min(levels.len());
    (lo..hi)
        .max_by(|a, b| levels[*a].total_cmp(&levels[*b]))
        .unwrap_or(i)
}

/// How deep a cut dropped on a spectrum peak starts, and how narrow.
const PEAK_CUT: f64 = -3.0;
const PEAK_Q: f64 = 4.0;

/// Drops a bell cutting `freq` into `curve`: an off bell if there is one,
/// else the bell nearest `freq`. Returns the band.
fn drop_cut(curve: &mut Curve, freq: f64) -> usize {
    let bells: Vec<usize> = (0..BANDS).filter(|&i| KINDS[i] == Kind::Bell).collect();
    let band = bells
        .iter()
        .copied()
        .find(|&i| !curve.bands[i].on)
        .unwrap_or_else(|| {
            bells
                .iter()
                .copied()
                .min_by(|&a, &b| {
                    let d = |i: usize| (curve.bands[i].freq / freq).log2().abs();
                    d(a).total_cmp(&d(b))
                })
                .unwrap_or(bells[0])
        });
    curve.on = true;
    curve.bands[band] = Band {
        on: true,
        freq: round_freq(freq),
        gain: PEAK_CUT,
        q: PEAK_Q,
    }
    .clamped(KINDS[band]);
    band
}

/// Switches `curve` to the other of its two slots (A and B), keeping the
/// one it leaves in `other`. The first switch copies the curve.
fn switch_slot(curve: &mut Curve, other: &mut Option<Curve>) {
    let next = other.take().unwrap_or_else(|| curve.clone());
    *other = Some(std::mem::replace(curve, next));
}

struct State {
    /// Shown under the curve until the next change, such as why a name
    /// can't be used.
    message: String,
    curves: [Curve; 2],
    target: usize,
    /// The dot being dragged.
    grabbed: Option<Grab>,
    /// The plot's size in pixels.
    plot: (f64, f64),
    /// The curve's range up and down, in dB.
    range: f64,
    /// Each output's slot in use (0 A, 1 B), and the curve in the other.
    slots: [usize; 2],
    others: [Option<Curve>; 2],
    /// The spectrum as drawn, one level per point; empty while hidden.
    levels: Vec<f64>,
    /// A spectrum kept to compare with.
    frozen: Option<Vec<f64>>,
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
                    y_of(response(&bands, DRAW_RATE, &[b.freq])[0], self.range)
                } else {
                    y_of(b.gain, self.range)
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

    /// The spectrum peak a press at (x, y) is on, as a frequency: a press
    /// on or under the spectrum, near its loudest point within a third of
    /// an octave. `None` with no spectrum drawn.
    fn peak_at(&self, x: f64, y: f64) -> Option<f64> {
        let n = self.levels.len();
        if n == 0 {
            return None;
        }
        let i = ((x.clamp(0.0, 1.0) * n as f64) as usize).min(n - 1);
        let floor = opendj_engine::room_eq::Spectrum::FLOOR;
        // Where the spectrum's top is, from 0 to 1 down, with a little
        // slack above it.
        let top = (self.levels[i] / floor).clamp(0.0, 1.0) - 12.0 / self.plot.1.max(1.0);
        if self.levels[i] <= floor || y < top {
            return None;
        }
        // Points to a third of an octave.
        let reach = (n as f64 / (MAX_FREQ / MIN_FREQ).log2() / 3.0).round() as usize;
        let peak = peak_near(&self.levels, i, reach);
        Some(point_freqs(n)[peak])
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
                y_of(*d, self.range) * self.plot.1
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
        ui.set_range(self.range as i32);
        ui.set_slot(self.slots[self.target] as i32);
        let booth = settings::audio_setup()
            .routes
            .iter()
            .any(|r| r.role == Role::Booth);
        let own = own();
        let (items, chosen) = menu(c, &own);
        let names: Vec<SharedString> = items.iter().map(|(n, _)| n.into()).collect();
        ui.set_preset_menu(ModelRc::new(VecModel::from(names)));
        ui.set_preset_chosen(chosen.map_or(-1, |i| i as i32));
        ui.set_preset(preset_label(c, &own).into());
        let main = &self.curves[0];
        ui.set_master_text(if main.on {
            preset_label(main, &own).into()
        } else {
            "Off".into()
        });
        ui.set_note(if !self.message.is_empty() {
            self.message.as_str().into()
        } else if self.target == 1 && !booth {
            "No output is the booth yet: give one the Booth role on the Devices tab.".into()
        } else {
            SharedString::new()
        });
    }
}

/// The Room EQ tab's place among Settings › Audio's tabs.
const ROOM_EQ_TAB: i32 = 2;
/// Points across the spectrum.
const SPECTRUM_POINTS: usize = 160;

/// The spectrum as a filled shape on the plot: `levels` from the floor
/// (bottom) to 0 dB (top), in a `w` × `h` plot.
fn spectrum_path(levels: &[f64], w: f64, h: f64) -> String {
    let n = levels.len().max(1) as f64;
    let floor = opendj_engine::room_eq::Spectrum::FLOOR;
    let mut p = format!("M 0 {h:.1}");
    for (i, db) in levels.iter().enumerate() {
        let x = (i as f64 + 0.5) / n * w;
        let y = h * (db / floor).clamp(0.0, 1.0);
        p.push_str(&format!(" L {x:.1} {y:.1}"));
    }
    p.push_str(&format!(" L {w:.1} {h:.1} Z"));
    p
}

/// A spectrum as a line on the plot, as `spectrum_path` places it.
fn spectrum_line(levels: &[f64], w: f64, h: f64) -> String {
    let n = levels.len().max(1) as f64;
    let floor = opendj_engine::room_eq::Spectrum::FLOOR;
    let mut p = String::new();
    for (i, db) in levels.iter().enumerate() {
        let x = (i as f64 + 0.5) / n * w;
        let y = h * (db / floor).clamp(0.0, 1.0);
        p.push_str(&format!(
            "{} {x:.1} {y:.1} ",
            if i == 0 { "M" } else { "L" }
        ));
    }
    p.trim_end().to_string()
}

/// Wires the Room EQ tab to the controls and the saved settings, and draws
/// the main output's spectrum behind the curve while the tab is open.
/// Keep the timer it returns.
pub fn install(
    app: &AppWindow,
    registry: Arc<ControlRegistry>,
    engine: &opendj_engine::AudioEngine,
) -> slint::Timer {
    let state = Rc::new(RefCell::new(State {
        message: String::new(),
        curves: saved(),
        target: 0,
        grabbed: None,
        plot: (680.0, 210.0),
        range: if settings::load()["room_eq_range"].as_f64() == Some(RANGES[0]) {
            RANGES[0]
        } else {
            RANGES[1]
        },
        slots: [0; 2],
        others: [None, None],
        levels: Vec::new(),
        frozen: None,
        registry,
    }));
    state.borrow().show(app);
    let ui = app.global::<RoomEqUi>();
    ui.set_show_spectrum(settings::flag("room_eq_spectrum", true));
    ui.on_spectrum_toggled(|on| settings::set("room_eq_spectrum", json!(on)));
    ui.set_view(if settings::string("room_eq_spectrum_view") == "before" {
        0
    } else {
        1
    });
    {
        let weak = app.as_weak();
        ui.on_view_picked(move |v| {
            if let Some(app) = weak.upgrade() {
                app.global::<RoomEqUi>().set_view(v);
            }
            settings::set(
                "room_eq_spectrum_view",
                json!(if v == 0 { "before" } else { "after" }),
            );
        });
    }
    {
        let state = state.clone();
        ui.on_freeze(move || {
            let mut s = state.borrow_mut();
            s.frozen = match s.frozen {
                Some(_) => None,
                None if !s.levels.is_empty() => Some(s.levels.clone()),
                None => None,
            };
        });
    }
    // The spectrum, 30 times a second while it shows.
    let timer = slint::Timer::default();
    if let Some(mut feed) = engine.take_output_feed() {
        let mut spectrum =
            opendj_engine::room_eq::Spectrum::new(engine.sample_rate(), SPECTRUM_POINTS);
        let (weak, state) = (app.as_weak(), state.clone());
        let mut copying = false;
        let mut buf = Vec::new();
        let freqs = point_freqs(SPECTRUM_POINTS);
        let tick = std::time::Duration::from_millis(33);
        timer.start(slint::TimerMode::Repeated, tick, move || {
            let Some(app) = weak.upgrade() else { return };
            let prefs = app.global::<Preferences>();
            let ui = app.global::<RoomEqUi>();
            let shows = prefs.get_open()
                && prefs.get_query().is_empty()
                && prefs.get_page() == crate::prefs::AUDIO_PAGE
                && prefs.get_tab() == ROOM_EQ_TAB
                && ui.get_show_spectrum();
            if shows != copying {
                feed.set_active(shows);
                copying = shows;
                if !shows {
                    state.borrow_mut().levels.clear();
                    ui.set_spectrum(SharedString::new());
                    ui.set_frozen(SharedString::new());
                }
            }
            if !shows {
                return;
            }
            buf.clear();
            feed.drain_into(&mut buf);
            spectrum.push(&buf);
            let post = spectrum.levels(tick.as_secs_f64());
            let mut s = state.borrow_mut();
            let levels = view_levels(post, &freqs, &s.curves, s.target, ui.get_view() == 1);
            s.levels = levels;
            let (w, h) = s.plot;
            ui.set_spectrum(spectrum_path(&s.levels, w, h).into());
            ui.set_frozen(
                s.frozen
                    .as_ref()
                    .map(|f| spectrum_line(f, w, h))
                    .unwrap_or_default()
                    .into(),
            );
        });
    }
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
    ui.on_preset_picked(move |i| {
        r(true, &|s, ui| {
            s.message.clear();
            let mut own = own();
            let (items, _) = menu(&s.curves[s.target], &own);
            let Some((_, action)) = items.get(i as usize).cloned() else {
                return false;
            };
            let c = s.curve();
            match action {
                MenuAction::Load(p) => {
                    c.bands = p.bands;
                    c.preset = p.name;
                    c.on = true;
                }
                MenuAction::Save => {
                    if let Some(p) = own.iter_mut().find(|p| p.name == c.preset) {
                        p.bands = c.bands;
                    }
                    save_own(&own);
                }
                MenuAction::SaveAs => {
                    let mine = own.iter().any(|p| p.name == c.preset);
                    ui.set_name_hint(
                        if mine {
                            c.preset.clone()
                        } else {
                            free_name(&own)
                        }
                        .into(),
                    );
                    ui.set_naming(true);
                    return false;
                }
                MenuAction::Revert => {
                    if let Some((p, _)) = preset_of(c, &own) {
                        c.bands = p.bands;
                    }
                }
                MenuAction::Delete => {
                    let name = c.preset.clone();
                    own.retain(|p| p.name != name);
                    save_own(&own);
                    for c in &mut s.curves {
                        if c.preset == name {
                            c.preset.clear();
                        }
                    }
                }
            }
            true
        });
    });
    let r = run.clone();
    ui.on_save_as(move |name| {
        r(true, &|s, ui| {
            let name = name.trim().to_string();
            if name.is_empty() {
                return false;
            }
            if built_in()
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&name))
            {
                s.message = format!("“{name}” is a built-in preset: pick another name.");
                return true;
            }
            s.message.clear();
            let mut own = own();
            let bands = s.curves[s.target].bands;
            match own.iter_mut().find(|p| p.name == name) {
                Some(p) => p.bands = bands,
                None => own.push(Preset {
                    name: name.clone(),
                    bands,
                }),
            }
            save_own(&own);
            s.curve().preset = name;
            ui.set_naming(false);
            true
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
            let (px, py) = (x as f64, y as f64);
            // On a dot: grab it. On the spectrum: cut its peak there with
            // a bell, and grab that.
            let (band, dropped) = match s.hit(x, y, w, h) {
                Some(i) => (i, false),
                None => match s.peak_at(px, py) {
                    Some(f) => (drop_cut(s.curve(), f), true),
                    None => {
                        s.grabbed = None;
                        return false;
                    }
                },
            };
            let (dx, dy) = s.dots()[band];
            s.grabbed = Some(Grab {
                band,
                dx: dx - px,
                dy: dy - py,
            });
            ui.set_selected(band as i32);
            dropped
        });
        true
    });
    let r = run.clone();
    ui.on_drag(move |x, y| {
        r(false, &|s, _| {
            let Some(g) = s.grabbed else {
                return false;
            };
            let i = g.band;
            let range = s.range;
            // Editing turns the band and the Room EQ on.
            let c = s.curve();
            c.on = true;
            let b = &mut c.bands[i];
            b.on = true;
            b.freq = round_freq(freq_at(x as f64 + g.dx));
            if !KINDS[i].is_cut() {
                let db = db_at(y as f64 + g.dy, range);
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
    ui.on_range_picked(move |range| {
        r(false, &|s, _| {
            let range = if range as f64 == RANGES[0] {
                RANGES[0]
            } else {
                RANGES[1]
            };
            settings::set("room_eq_range", json!(range));
            s.range = range;
            true
        });
    });
    let r = run.clone();
    ui.on_pick_slot(move |slot| {
        r(true, &|s, _| {
            let (t, slot) = (s.target, slot.clamp(0, 1) as usize);
            if s.slots[t] == slot {
                return false;
            }
            s.slots[t] = slot;
            let mut other = s.others[t].take();
            switch_slot(s.curve(), &mut other);
            s.others[t] = other;
            true
        });
    });
    let r = run.clone();
    ui.on_bypass(move |held| {
        r(false, &|s, ui| {
            ui.set_bypassed(held);
            let mut c = s.curves[s.target].clone();
            c.on &= !held;
            play(&s.registry, s.target, &c);
            false
        });
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
    timer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_survives_saving() {
        let mut c = Curve {
            on: true,
            preset: "Main room".into(),
            ..Curve::default()
        };
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
    fn the_preset_menu_follows_the_curve() {
        let names = |c: &Curve, own: &[Preset]| -> Vec<String> {
            menu(c, own).0.into_iter().map(|(n, _)| n).collect()
        };
        // A built-in preset as it is: the four presets and Save as.
        let mut c = Curve {
            on: true,
            bands: built_in()[1].bands,
            preset: "Muddy room".into(),
        };
        assert_eq!(
            names(&c, &[]),
            [
                "Flat",
                "Muddy room",
                "Harsh highs",
                "Small speakers",
                "Save as…"
            ]
        );
        assert_eq!(menu(&c, &[]).1, Some(1));
        assert_eq!(preset_label(&c, &[]), "Muddy room");
        // Changed: marked, and it can go back.
        c.bands[2].gain = -4.0;
        assert_eq!(preset_label(&c, &[]), "Muddy room *");
        assert!(names(&c, &[]).contains(&"Revert".to_string()));
        // An own preset, changed: it can be saved over, or deleted.
        let own = vec![Preset {
            name: "Main room".into(),
            bands: FLAT,
        }];
        c.preset = "Main room".into();
        let n = names(&c, &own);
        assert_eq!(n[4], "Main room");
        assert!(n.contains(&"Save “Main room”".to_string()));
        assert!(n.contains(&"Delete “Main room”".to_string()));
        // Gone: Custom.
        c.preset = "Old club".into();
        assert_eq!(preset_label(&c, &own), "Custom");
        assert_eq!(free_name(&own), "Venue 1");
    }

    #[test]
    fn built_in_presets_are_in_range() {
        for p in built_in() {
            for (i, b) in p.bands.iter().enumerate() {
                assert_eq!(*b, b.clamped(KINDS[i]), "{} band {i}", p.name);
            }
        }
    }

    #[test]
    fn the_spectrum_shows_before_or_after() {
        let freqs = point_freqs(40);
        assert!((freqs[0] - freq_at(0.5 / 40.0)).abs() < 1e-9);
        let post = vec![-20.0; 40];
        let mut curves = [Curve::default(), Curve::default()];
        // Main cuts 6 dB everywhere near 1 kHz with a bell.
        curves[0].on = true;
        curves[0].bands[2] = Band {
            on: true,
            freq: 1000.0,
            gain: -6.0,
            q: 0.3,
        };
        let k = freqs.iter().position(|f| *f > 1000.0).unwrap();
        let cut = response(&curves[0].bands, DRAW_RATE, &[freqs[k]])[0];
        // After, on main: as measured. Before: the cut put back.
        assert_eq!(view_levels(&post, &freqs, &curves, 0, true), post);
        let before = view_levels(&post, &freqs, &curves, 0, false);
        assert!((before[k] - (-20.0 - cut)).abs() < 1e-9);
        // After, on the booth (flat, off): the mix, as before.
        assert_eq!(view_levels(&post, &freqs, &curves, 1, true), before);
        // Silence stays at the floor.
        let floor = opendj_engine::room_eq::Spectrum::FLOOR;
        assert_eq!(
            view_levels(&vec![floor; 40], &freqs, &curves, 0, false)[k],
            floor
        );
    }

    #[test]
    fn a_spectrum_peak_gets_a_cut() {
        let levels = [-40.0, -30.0, -12.0, -35.0, -50.0, -8.0];
        assert_eq!(peak_near(&levels, 1, 1), 2);
        assert_eq!(peak_near(&levels, 4, 1), 5);
        assert_eq!(peak_near(&levels, 0, 0), 0);
        // An off bell first, then the bell nearest the peak.
        let mut c = Curve::default();
        assert_eq!(drop_cut(&mut c, 180.4), 2);
        assert!(c.on && c.bands[2].on);
        assert_eq!(
            (c.bands[2].freq, c.bands[2].gain, c.bands[2].q),
            (180.0, PEAK_CUT, PEAK_Q)
        );
        assert_eq!(drop_cut(&mut c, 4000.0), 3);
        assert_eq!(drop_cut(&mut c, 3000.0), 3);
        assert_eq!(c.bands[2].freq, 180.0);
    }

    #[test]
    fn a_and_b_keep_two_curves() {
        let mut c = Curve {
            on: true,
            ..Curve::default()
        };
        let mut other = None;
        // The first switch copies A to B.
        switch_slot(&mut c, &mut other);
        assert_eq!(other.as_ref(), Some(&c));
        c.bands[2].gain = -4.0;
        // Back to A: as it was; B keeps its change.
        switch_slot(&mut c, &mut other);
        assert_eq!(c.bands[2].gain, FLAT[2].gain);
        assert_eq!(other.as_ref().unwrap().bands[2].gain, -4.0);
        switch_slot(&mut c, &mut other);
        assert_eq!(c.bands[2].gain, -4.0);
    }

    #[test]
    fn axes_and_labels() {
        assert!((x_of(20.0)).abs() < 1e-9 && (x_of(20_000.0) - 1.0).abs() < 1e-9);
        assert!((freq_at(x_of(250.0)) - 250.0).abs() < 1e-6);
        assert_eq!(y_of(12.0, 12.0), 0.0);
        assert_eq!(y_of(0.0, 12.0), 0.5);
        // Zoomed in, 6 dB is the top, and more stays on the plot.
        assert_eq!(y_of(6.0, 6.0), 0.0);
        assert_eq!(y_of(-9.0, 6.0), 1.0);
        assert!((db_at(y_of(-3.0, 6.0), 6.0) + 3.0).abs() < 1e-9);
        assert_eq!(hz_text(250.0), "250 Hz");
        assert_eq!(hz_text(3200.0), "3.2 kHz");
        assert_eq!(hz_text(12_000.0), "12 kHz");
        assert_eq!(db_text(-3.0), "−3.0 dB");
        assert_eq!(round_freq(3217.0), 3220.0);
    }
}
