//! Hardware synths on the instrument deck: the bundled synth profiles
//! (`opendj_studio_engine::synth_profile`) listed in the Jam column, one
//! loaded onto the engine's hardware synth (`opendj_engine::hardware_synth`)
//! with its audio input routed as the instrument's, its presets stepped by
//! name, and its setup (MIDI out, channel, audio input, latency, clock) kept
//! per synth.
//!
//! The preset picker groups a synth's presets by category when it has them,
//! else by bank. A synth that can tell its preset names and categories
//! (`names_from` in its profile) is asked once in the background when it
//! first loads; the answers are kept in `Instruments/Synth presets.json`
//! and asked again from the picker's ↻.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opendj_control::ControlRegistry;
use opendj_engine::AudioEngine;
use opendj_engine::{input_devices, InputRole, InputRoute};
use opendj_studio_engine::synth_profile::{self, ReadPreset, SynthPreset, SynthProfile};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::instruments::{self, Synth};
use crate::{settings, AppWindow, HardwareUi, HwPreset};

/// Settings: each synth's setup by its profile id.
const KEY: &str = "hardware_synths";

/// How often the Jam column checks which synths are plugged in.
const WATCH: Duration = Duration::from_secs(3);

/// How often a background read of preset names shows its progress.
const READ_TICK: Duration = Duration::from_millis(200);

/// How long the synth gets to answer for one preset's name.
const READ_WAIT: Duration = Duration::from_millis(300);

/// One synth's setup as kept in settings.
#[derive(Clone, Debug, Default, PartialEq)]
struct Setup {
    /// The MIDI output, empty for the profile's own port name.
    port: String,
    channel: u8,
    /// The audio input: its device and channel (from 0), empty for none.
    device: String,
    input: u16,
    latency: Option<f64>,
    clock: Option<bool>,
    /// The input gain (linear) and whether the limiter is on.
    gain: Option<f64>,
    limit: Option<bool>,
    /// The preset picked last, -1 none.
    preset: i32,
}

impl Setup {
    fn read(v: &Value, profile: &SynthProfile) -> Option<Self> {
        let v = v.get(&profile.id)?;
        Some(Self {
            port: v["port"].as_str().unwrap_or_default().to_string(),
            channel: v["channel"]
                .as_u64()
                .map_or(profile.channel, |c| c.min(15) as u8),
            device: v["device"].as_str().unwrap_or_default().to_string(),
            input: v["input"].as_u64().unwrap_or(0).min(u64::from(u16::MAX)) as u16,
            latency: v["latency"].as_f64(),
            clock: v["clock"].as_bool(),
            gain: v["gain"].as_f64(),
            limit: v["limit"].as_bool(),
            preset: v["preset"].as_i64().map_or(-1, |p| p as i32),
        })
    }

    fn write(&self) -> Value {
        json!({
            "port": self.port,
            "channel": self.channel,
            "device": self.device,
            "input": self.input,
            "latency": self.latency,
            "clock": self.clock,
            "gain": self.gain,
            "limit": self.limit,
            "preset": self.preset,
        })
    }
}

/// An audio input channel to pick: "M4 · In 4".
fn input_label(device: &str, channel: u16) -> String {
    format!("{} · In {}", short_device(device), channel + 1)
}

/// A device's name without its maker's long prefix: "MOTU M4" stays, a
/// name like "Focusrite USB (Scarlett 2i2)" keeps what is in brackets.
fn short_device(name: &str) -> &str {
    match (name.find('('), name.rfind(')')) {
        (Some(a), Some(b)) if b > a + 1 => &name[a + 1..b],
        _ => name,
    }
}

/// The MIDI output `wanted` names: exactly, else one holding it (any case),
/// as the engine opens it.
fn port_index(ports: &[String], wanted: &str) -> Option<usize> {
    ports.iter().position(|p| p == wanted).or_else(|| {
        let w = wanted.to_lowercase();
        ports.iter().position(|p| p.to_lowercase().contains(&w))
    })
}

/// The deck bar's text for preset `index`: its bank or category, dimmed
/// ("Pad ›"), then its number and its own name if it has one ("042 Glass
/// Choir"; "3" in a named bank).
fn preset_bar(profile: &SynthProfile, index: usize, p: &SynthPreset) -> (String, String) {
    let group = if profile.presets.categories.is_empty() {
        (profile.presets.banks.len() > 1).then(|| p.bank.clone())
    } else {
        (!p.category.is_empty()).then(|| p.category.clone())
    };
    let number = profile.preset_number(index, p);
    let name = if p.named {
        format!("{number} {}", p.name)
    } else {
        number
    };
    (group.map(|g| format!("{g} ›")).unwrap_or_default(), name)
}

/// The picker's presets in group `group` whose name holds `search` (any
/// case), by their place in `presets`.
fn listed(
    profile: &SynthProfile,
    presets: &[SynthPreset],
    group: usize,
    search: &str,
) -> Vec<usize> {
    let search = search.trim().to_lowercase();
    presets
        .iter()
        .enumerate()
        .filter(|(_, p)| search.is_empty() || p.name.to_lowercase().contains(&search))
        .filter(|(_, p)| !search.is_empty() || profile.in_group(p, group))
        .map(|(i, _)| i)
        .collect()
}

/// The group the picker opens on for preset `at`: its bank, or "All" for
/// a synth grouped by category.
fn group_of(profile: &SynthProfile, presets: &[SynthPreset], at: i32) -> usize {
    let groups = profile.preset_groups();
    match presets.get(at.max(0) as usize) {
        Some(p) if at >= 0 && profile.presets.categories.is_empty() => {
            groups.iter().position(|g| *g == p.bank).unwrap_or(0)
        }
        _ => 0,
    }
}

/// Where the names read from synths are kept.
fn names_file() -> Option<std::path::PathBuf> {
    Some(
        settings::config_dir()?
            .join("Instruments")
            .join("Synth presets.json"),
    )
}

/// The names and categories read from synth `id` before, one per preset.
fn kept_names(id: &str) -> Option<Vec<Option<ReadPreset>>> {
    let text = std::fs::read_to_string(names_file()?).ok()?;
    let all: Value = serde_json::from_str(&text).ok()?;
    serde_json::from_value(all.get(id)?.clone()).ok()
}

fn keep_names(id: &str, names: &[Option<ReadPreset>]) {
    let Some(path) = names_file() else { return };
    let mut all: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    all[id] = json!(names);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(&all) {
        let _ = std::fs::write(&path, text);
    }
}

/// One read's answers, one per preset; `None` when the synth didn't reply.
type Answers = Option<Vec<Option<ReadPreset>>>;

/// A background read of a synth's preset names.
#[derive(Default)]
struct Reading {
    running: AtomicBool,
    done: AtomicUsize,
    total: AtomicUsize,
    /// The synth's id and its answers, once finished (`None` answers when
    /// it didn't reply at all).
    result: Mutex<Option<(String, Answers)>>,
}

/// What the module keeps between callbacks.
struct State {
    profiles: Vec<SynthProfile>,
    /// The loaded synth's place in `profiles`, its presets and setup.
    loaded: Option<usize>,
    presets: Vec<SynthPreset>,
    setup: Setup,
    /// The lists the setup panel shows now.
    ports: Vec<String>,
    /// The audio interfaces with inputs, and how many each has.
    devices: Vec<(String, u16)>,
    /// The picker's group and search, and the presets it lists now.
    group: usize,
    search: String,
    shown: Vec<usize>,
    reading: Arc<Reading>,
}

pub struct Watcher {
    _timer: slint::Timer,
    _reader: slint::Timer,
}

pub fn install(app: &AppWindow, engine: Rc<AudioEngine>, reg: Arc<ControlRegistry>) -> Watcher {
    let state = Rc::new(RefCell::new(State {
        profiles: synth_profile::bundled(),
        loaded: None,
        presets: Vec::new(),
        setup: Setup::default(),
        ports: Vec::new(),
        devices: Vec::new(),
        group: 0,
        search: String::new(),
        shown: Vec::new(),
        reading: Arc::new(Reading::default()),
    }));
    let ui = app.global::<HardwareUi>();

    let (weak, st, e, r) = (app.as_weak(), state.clone(), engine.clone(), reg.clone());
    ui.on_load(move |index| {
        let Some(app) = weak.upgrade() else {
            return;
        };
        load(&app, &e, &r, &mut st.borrow_mut(), index.max(0) as usize);
    });

    let (weak, st, e) = (app.as_weak(), state.clone(), engine.clone());
    ui.on_step_preset(move |delta| {
        let mut s = st.borrow_mut();
        let Some(profile) = s.loaded.and_then(|i| s.profiles.get(i)) else {
            return;
        };
        // Through the picked bank or category only.
        let group = listed(profile, &s.presets, s.group, "");
        let n = group.len() as i32;
        if n == 0 {
            return;
        }
        let here = group.iter().position(|&i| i as i32 == s.setup.preset);
        let at = match here {
            None if delta > 0 => 0,
            None => n - 1,
            Some(h) => (h as i32 + delta).rem_euclid(n),
        };
        let at = group[at as usize] as i32;
        if let Some(app) = weak.upgrade() {
            pick(&app, &e, &mut s, at);
        }
    });
    let (weak, st) = (app.as_weak(), state.clone());
    ui.on_set_group(move |g| {
        let mut s = st.borrow_mut();
        s.group = g.max(0) as usize;
        s.search.clear();
        if let Some(app) = weak.upgrade() {
            show_list(&app, &mut s);
        }
    });
    let (weak, st) = (app.as_weak(), state.clone());
    ui.on_set_search(move |text| {
        let mut s = st.borrow_mut();
        s.search = text.to_string();
        if let Some(app) = weak.upgrade() {
            show_list(&app, &mut s);
        }
    });
    let (weak, st, e) = (app.as_weak(), state.clone(), engine.clone());
    ui.on_read_names(move || {
        if let Some(app) = weak.upgrade() {
            read_names(&app, &e, &st.borrow());
        }
    });
    let (weak, st, e) = (app.as_weak(), state.clone(), engine.clone());
    ui.on_pick_preset(move |index| {
        if let Some(app) = weak.upgrade() {
            pick(&app, &e, &mut st.borrow_mut(), index);
        }
    });

    let (weak, st) = (app.as_weak(), state.clone());
    ui.on_refresh_setup(move || {
        if let Some(app) = weak.upgrade() {
            show_setup(&app, &mut st.borrow_mut());
        }
    });
    let (weak, st, e) = (app.as_weak(), state.clone(), engine.clone());
    ui.on_set_port(move |i| {
        let mut s = st.borrow_mut();
        let Some(port) = s.ports.get(i.max(0) as usize).cloned() else {
            return;
        };
        s.setup.port = port;
        apply(&e, &s);
        save(&s);
        if let Some(app) = weak.upgrade() {
            show_setup(&app, &mut s);
        }
    });
    let (weak, st, e) = (app.as_weak(), state.clone(), engine.clone());
    ui.on_set_channel(move |c| {
        let mut s = st.borrow_mut();
        s.setup.channel = c.clamp(0, 15) as u8;
        apply(&e, &s);
        save(&s);
        if let Some(app) = weak.upgrade() {
            show_setup(&app, &mut s);
        }
    });
    let (weak, st, e) = (app.as_weak(), state.clone(), engine.clone());
    ui.on_set_device(move |i| {
        let mut s = st.borrow_mut();
        let Some((device, channels)) = s.devices.get(i.max(0) as usize).cloned() else {
            return;
        };
        // Another interface starts on its first input; the same keeps its.
        if device != s.setup.device || s.setup.input >= channels {
            s.setup.input = 0;
        }
        s.setup.device = device;
        save(&s);
        if let Some(app) = weak.upgrade() {
            route_input(&app, &e, &s);
            show_setup(&app, &mut s);
        }
    });
    let (weak, st, e) = (app.as_weak(), state.clone(), engine.clone());
    ui.on_set_input(move |i| {
        let mut s = st.borrow_mut();
        if s.setup.device.is_empty() || i < 0 {
            return;
        }
        s.setup.input = i as u16;
        save(&s);
        if let Some(app) = weak.upgrade() {
            route_input(&app, &e, &s);
            show_setup(&app, &mut s);
        }
    });
    let (st, r) = (state.clone(), reg.clone());
    ui.on_save_setup(move || {
        let mut s = st.borrow_mut();
        let get = |item: &str| {
            r.get(opendj_engine::hardware_synth::GROUP, item)
                .map(|c| c.get())
        };
        s.setup.latency = get("hardware_latency");
        s.setup.clock = get("hardware_clock").map(|v| v > 0.5);
        s.setup.gain = get("hardware_gain");
        s.setup.limit = get("hardware_limit").map(|v| v > 0.5);
        save(&s);
    });

    // The Jam column's rows, and which synths are plugged in, kept current.
    let (weak, st) = (app.as_weak(), state.clone());
    let list = move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let ports = opendj_engine::hardware_synth::HardwareSynth::ports();
        let s = st.borrow();
        let synths = s
            .profiles
            .iter()
            .map(|p| Synth {
                id: p.id.clone(),
                name: p.name.clone(),
                maker: p.maker.clone(),
                plugged_in: port_index(&ports, &p.port).is_some(),
            })
            .collect();
        instruments::show_synths(&app, synths);
    };
    list();
    let timer = slint::Timer::default();
    let mut last: Vec<String> = Vec::new();
    timer.start(slint::TimerMode::Repeated, WATCH, move || {
        let ports = opendj_engine::hardware_synth::HardwareSynth::ports();
        if ports != last {
            last = ports;
            list();
        }
    });
    // A background read of preset names: its progress, then its answers.
    let (weak, st) = (app.as_weak(), state.clone());
    let reader = slint::Timer::default();
    reader.start(slint::TimerMode::Repeated, READ_TICK, move || {
        let Some(app) = weak.upgrade() else { return };
        let mut s = st.borrow_mut();
        let r = s.reading.clone();
        let ui = app.global::<HardwareUi>();
        let running = r.running.load(Ordering::Relaxed);
        if running {
            let total = r.total.load(Ordering::Relaxed).max(1);
            ui.set_read_progress(r.done.load(Ordering::Relaxed) as f32 / total as f32);
        }
        if ui.get_reading() != running {
            ui.set_reading(running);
        }
        let Some((id, answers)) = r.result.lock().unwrap().take() else {
            return;
        };
        match answers {
            Some(answers) => {
                keep_names(&id, &answers);
                let profile = s.loaded.and_then(|i| s.profiles.get(i)).cloned();
                if let Some(profile) = profile.filter(|p| p.id == id) {
                    let mut presets = profile.preset_list();
                    profile.apply_names(&mut presets, &answers);
                    s.presets = presets;
                    ui.set_read_note("".into());
                    show_list(&app, &mut s);
                    show_preset(&app, &s);
                }
            }
            None => ui.set_read_note(
                "The synth didn't answer. Check it is plugged in by USB, then ↻.".into(),
            ),
        }
    });
    Watcher {
        _timer: timer,
        _reader: reader,
    }
}

/// Asks the loaded synth for its preset names and categories in the
/// background, if it can tell them and isn't being asked already.
fn read_names(app: &AppWindow, engine: &AudioEngine, s: &State) {
    let Some(profile) = s.loaded.and_then(|i| s.profiles.get(i)).cloned() else {
        return;
    };
    let from = profile.presets.names_from.clone();
    if from.is_empty() || s.reading.running.swap(true, Ordering::Relaxed) {
        return;
    }
    app.global::<HardwareUi>().set_reading(true);
    app.global::<HardwareUi>().set_read_progress(0.0);
    let total = s.presets.len();
    let r = s.reading.clone();
    r.done.store(0, Ordering::Relaxed);
    r.total.store(total, Ordering::Relaxed);
    let synth = engine.hardware_synth().clone();
    let id = profile.id.clone();
    std::thread::spawn(move || {
        // The MIDI thread opens the synth's port when it loads.
        std::thread::sleep(Duration::from_millis(500));
        let seq = |i: usize| (i % 128) as u8;
        let requests: Vec<Vec<u8>> = (0..total)
            .filter_map(|i| synth_profile::name_request(&from, seq(i), i))
            .collect();
        let answers = synth
            .ask(
                &requests,
                READ_WAIT,
                |i, m| synth_profile::read_name(&from, seq(i), m).is_some(),
                |done| {
                    r.done.store(done, Ordering::Relaxed);
                    synth.config().is_some()
                },
            )
            .map(|answers| {
                answers
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        m.as_ref()
                            .and_then(|m| synth_profile::read_name(&from, seq(i), m))
                    })
                    .collect::<Vec<_>>()
            });
        *r.result.lock().unwrap() = Some((id, answers));
        r.running.store(false, Ordering::Relaxed);
    });
}

/// Loads profile `index` onto the instrument deck with its kept setup.
fn load(app: &AppWindow, engine: &AudioEngine, reg: &ControlRegistry, s: &mut State, index: usize) {
    let Some(profile) = s.profiles.get(index).cloned() else {
        return;
    };
    let saved = Setup::read(&settings::load()[KEY], &profile);
    let first = saved.is_none();
    s.setup = saved.unwrap_or(Setup {
        channel: profile.channel,
        preset: -1,
        ..Setup::default()
    });
    s.loaded = Some(index);
    s.presets = profile.preset_list();
    let kept = kept_names(&profile.id);
    if let Some(names) = &kept {
        profile.apply_names(&mut s.presets, names);
    }
    s.group = group_of(&profile, &s.presets, s.setup.preset);
    s.search.clear();
    apply(engine, s);
    let set = |item: &str, v: f64| {
        if let Some(c) = reg.get(opendj_engine::hardware_synth::GROUP, item) {
            c.set(v);
        }
    };
    set(
        "hardware_latency",
        s.setup
            .latency
            .unwrap_or(opendj_engine::hardware_synth::DEFAULT_LATENCY_MS),
    );
    set(
        "hardware_clock",
        if s.setup.clock.unwrap_or(profile.transport.clock) {
            1.0
        } else {
            0.0
        },
    );
    set("hardware_gain", s.setup.gain.unwrap_or(1.0));
    set(
        "hardware_limit",
        if s.setup.limit.unwrap_or(true) {
            1.0
        } else {
            0.0
        },
    );
    route_input(app, engine, s);
    let ui = app.global::<HardwareUi>();
    ui.set_loaded(index as i32);
    ui.set_name(profile.name.as_str().into());
    ui.set_maker(profile.maker.as_str().into());
    ui.set_groups(ModelRc::new(VecModel::from(
        profile
            .preset_groups()
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    ui.set_group_caption(
        if !profile.presets.categories.is_empty() {
            "CATEGORIES"
        } else {
            "BANKS"
        }
        .into(),
    );
    ui.set_can_read(!profile.presets.names_from.is_empty());
    ui.set_read_note("".into());
    show_list(app, s);
    show_preset(app, s);
    show_setup(app, s);
    if first {
        save(s);
        ui.set_setup_open(true);
    }
    // Names from the synth itself, the first time.
    if kept.is_none() {
        read_names(app, engine, s);
    }
}

/// Fills the picker's right column: the group's presets, or every one
/// whose name matches the search.
fn show_list(app: &AppWindow, s: &mut State) {
    let Some(profile) = s.loaded.and_then(|i| s.profiles.get(i)) else {
        return;
    };
    s.shown = listed(profile, &s.presets, s.group, &s.search);
    let rows: Vec<HwPreset> = s
        .shown
        .iter()
        .map(|&i| {
            let p = &s.presets[i];
            HwPreset {
                index: i as i32,
                number: profile.preset_number(i, p).into(),
                name: p.name.as_str().into(),
            }
        })
        .collect();
    let ui = app.global::<HardwareUi>();
    ui.set_group(s.group as i32);
    ui.set_search(s.search.as_str().into());
    let title = if s.search.trim().is_empty() {
        profile
            .preset_groups()
            .get(s.group)
            .cloned()
            .unwrap_or_else(|| "Presets".into())
    } else {
        format!("{} found", rows.len())
    };
    ui.set_list_caption(title.to_uppercase().into());
    ui.set_list(ModelRc::new(VecModel::from(rows)));
}

/// Plays the loaded synth's setup on the engine: its port, channel and knobs.
fn apply(engine: &AudioEngine, s: &State) {
    let Some(profile) = s.loaded.and_then(|i| s.profiles.get(i)) else {
        return;
    };
    engine.hardware_synth().set(Some(
        profile.hardware_config(&s.setup.port, s.setup.channel),
    ));
}

/// Routes the synth's audio input as the instrument's, or none.
fn route_input(app: &AppWindow, engine: &AudioEngine, s: &State) {
    let Some(profile) = s.loaded.and_then(|i| s.profiles.get(i)) else {
        return;
    };
    let mut setup = settings::audio_setup();
    setup.inputs.retain(|r| r.role != InputRole::Instrument);
    if !s.setup.device.is_empty() {
        setup.inputs.push(InputRoute {
            role: InputRole::Instrument,
            device: s.setup.device.clone(),
            channel: s.setup.input,
            mono: profile.audio != "stereo",
        });
    }
    settings::set_audio_setup(&setup);
    engine.set_inputs(&setup);
    instruments::show_routes(app, &engine.input_status(), &engine.status());
}

/// Sends preset `at` to the synth and shows it.
fn pick(app: &AppWindow, engine: &AudioEngine, s: &mut State, at: i32) {
    let Some(p) = s.presets.get(at.max(0) as usize) else {
        return;
    };
    engine.hardware_synth().program(p.bank_msb, p.program);
    s.setup.preset = at;
    save(s);
    show_preset(app, s);
}

fn show_preset(app: &AppWindow, s: &State) {
    let ui = app.global::<HardwareUi>();
    let profile = s.loaded.and_then(|i| s.profiles.get(i));
    let shown = profile.zip(s.presets.get(s.setup.preset.max(0) as usize));
    match shown.filter(|_| s.setup.preset >= 0) {
        Some((profile, p)) => {
            let (group, name) = preset_bar(profile, s.setup.preset as usize, p);
            ui.set_preset(s.setup.preset);
            ui.set_preset_number(group.into());
            ui.set_preset_name(name.into());
        }
        None => {
            ui.set_preset(-1);
            ui.set_preset_number("".into());
            ui.set_preset_name("".into());
        }
    }
}

/// Fills the setup panel's lists and picks.
fn show_setup(app: &AppWindow, s: &mut State) {
    let Some(profile) = s.loaded.and_then(|i| s.profiles.get(i)) else {
        return;
    };
    s.ports = opendj_engine::hardware_synth::HardwareSynth::ports();
    let wanted = if s.setup.port.is_empty() {
        profile.port.clone()
    } else {
        s.setup.port.clone()
    };
    let port = port_index(&s.ports, &wanted);
    // A port picked before that is gone still shows, to see what is missing.
    let mut ports = s.ports.clone();
    let port = match port {
        Some(i) => i as i32,
        None => {
            ports.push(format!("{wanted} (not found)"));
            s.ports.push(wanted);
            ports.len() as i32 - 1
        }
    };
    s.devices = input_devices()
        .into_iter()
        .map(|d| (d.name, d.channels))
        .collect();
    // An interface picked before that is gone still shows, to see what is
    // missing.
    if !s.setup.device.is_empty() && !s.devices.iter().any(|(d, _)| *d == s.setup.device) {
        s.devices.push((s.setup.device.clone(), 0));
    }
    let device = s.devices.iter().position(|(d, _)| *d == s.setup.device);
    let channels = device.map_or(0, |i| s.devices[i].1);
    let input = (device.is_some() && s.setup.input < channels).then_some(s.setup.input);
    let ui = app.global::<HardwareUi>();
    let model = |v: Vec<String>| {
        ModelRc::new(VecModel::from(
            v.into_iter().map(SharedString::from).collect::<Vec<_>>(),
        ))
    };
    ui.set_ports(model(ports));
    ui.set_port(port);
    ui.set_channel(i32::from(s.setup.channel));
    ui.set_devices(model(
        s.devices
            .iter()
            .map(|(d, n)| {
                if *n == 0 {
                    format!("{} (not found)", short_device(d))
                } else {
                    short_device(d).to_string()
                }
            })
            .collect(),
    ));
    ui.set_device(device.map_or(-1, |i| i as i32));
    ui.set_inputs(model(
        (0..channels).map(|c| format!("In {}", c + 1)).collect(),
    ));
    ui.set_input(input.map_or(-1, i32::from));
    ui.set_input_label(
        input
            .map(|c| input_label(&s.setup.device, c))
            .unwrap_or_default()
            .into(),
    );
}

fn save(s: &State) {
    let Some(profile) = s.loaded.and_then(|i| s.profiles.get(i)) else {
        return;
    };
    let mut all = settings::load()[KEY].clone();
    if !all.is_object() {
        all = json!({});
    }
    all[&profile.id] = s.setup.write();
    settings::set(KEY, all);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_is_found_by_its_name_or_part_of_it() {
        let ports = vec!["IAC Bus 1".to_string(), "Arturia MicroFreak".to_string()];
        assert_eq!(port_index(&ports, "Arturia MicroFreak"), Some(1));
        assert_eq!(port_index(&ports, "MicroFreak"), Some(1));
        assert_eq!(port_index(&ports, "microfreak"), Some(1));
        assert_eq!(port_index(&ports, "Messenger"), None);
    }

    #[test]
    fn inputs_read_as_their_device_and_channel() {
        assert_eq!(input_label("M4", 3), "M4 · In 4");
        assert_eq!(
            input_label("Focusrite USB (Scarlett 2i2)", 0),
            "Scarlett 2i2 · In 1"
        );
    }

    #[test]
    fn a_setup_keeps_what_was_picked() {
        let p = synth_profile::find("arturia-microfreak").unwrap();
        let s = Setup {
            port: "MicroFreak".into(),
            channel: 2,
            device: "M4".into(),
            input: 3,
            latency: Some(12.5),
            clock: Some(false),
            gain: Some(2.0),
            limit: Some(true),
            preset: 40,
        };
        let v = json!({ "arturia-microfreak": s.write() });
        assert_eq!(Setup::read(&v, &p), Some(s));
        assert_eq!(Setup::read(&json!({}), &p), None);
    }

    #[test]
    fn a_banked_synths_bar_reads_bank_then_number() {
        let p = synth_profile::find("moog-messenger").unwrap();
        let list = p.preset_list();
        let (group, name) = preset_bar(&p, 18, &list[18]);
        assert_eq!(group, "Classic Bass ›");
        assert_eq!(name, "3");
        assert_eq!(group_of(&p, &list, 18), 1);
        assert_eq!(listed(&p, &list, 1, "").len(), 16);
    }

    #[test]
    fn named_presets_group_by_category_and_search_by_name() {
        let p = synth_profile::find("arturia-microfreak").unwrap();
        let mut list = p.preset_list();
        let mut reads = vec![None; list.len()];
        reads[41] = Some(ReadPreset {
            name: "Glass Choir".into(),
            category: 5,
            empty: false,
        });
        reads[300] = Some(ReadPreset {
            name: "My pad".into(),
            category: 5,
            empty: false,
        });
        p.apply_names(&mut list, &reads);
        let (group, name) = preset_bar(&p, 41, &list[41]);
        assert_eq!(group, "Pad ›");
        assert_eq!(name, "042 Glass Choir");
        // "All", then Pad is the sixth category.
        assert_eq!(listed(&p, &list, 6, ""), [41, 300]);
        assert_eq!(listed(&p, &list, 0, "").len(), 512);
        // A search looks through every group.
        assert_eq!(listed(&p, &list, 1, "choir"), [41]);
        assert_eq!(group_of(&p, &list, 41), 0);
    }
}
