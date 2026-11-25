//! openDJ Studio: a clip-launching DAW on openDJ's engine. This is the
//! first test app (OpenSpec change add-opendj-studio, step 1): four tracks
//! by eight scenes of MIDI clips playing built-in sounds, launched on the
//! bar of the studio's own clock or of an Ableton Link session shared with
//! openDJ.
//!
//! `opendj-studio` opens the window. `opendj-studio --check` renders
//! offline and prints launch timing and CPU figures; `--render out.wav`
//! writes 32 bars of the demo set going through its scenes.
//!
//! The engine is in `opendj-studio-engine`, so openDJ can play a set too;
//! this crate adds the sound card, MIDI ports, Push and the window.

// The engine's modules, under their old paths.
pub use opendj_studio_engine::{
    als, clip, control, conv, devices, engine, fx, link, plugins, scale, sound, steps,
};

mod audio;
mod check;
mod midi;
mod push;
mod settings;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};

use clip::{AudioTo, Io, MidiFrom, Monitoring, SCENES, TRACKS};
use control::{Controls, Mix};
use engine::{Quantize, Studio};
use std::path::Path;

slint::include_modules!();

/// The demo set's tempo.
const TEMPO: f64 = 124.0;

const TRACK_COLORS: [u32; TRACKS] = [0xff8a65, 0x4fc3f7, 0xb69cff, 0xf48fb1];

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// What the device row calls a track's instrument.
fn device_name(sound: clip::Sound) -> String {
    match sound {
        clip::Sound::Drums(kit) => format!("Drum Kit · {kit}"),
        clip::Sound::Bass => "Bass Synth".into(),
        clip::Sound::Pad => "Pad Synth".into(),
        clip::Sound::Lead => "Lead Synth".into(),
        clip::Sound::Audio => "Audio".into(),
    }
}

/// A meter's next height (0..1 over -60..+6 dB) from its last height and
/// the peak since: jumps up, falls back about 20 dB a second.
fn meter(last: f32, peak: f32) -> f32 {
    let now = if peak > 0.0 {
        ((20.0 * peak.log10() + 60.0) / 66.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    now.max(last - 0.01)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--check") => std::process::exit(check::run()),
        Some("--render") => {
            let path = args.get(1).map(String::as_str).unwrap_or("studio.wav");
            std::process::exit(check::render(path));
        }
        Some("--link-check") => std::process::exit(check::link()),
        Some("--als-info") => {
            let Some(path) = args.get(1) else {
                eprintln!("--als-info needs a .als file");
                std::process::exit(2);
            };
            match als::read(std::path::Path::new(path)) {
                Ok(set) => print!("{}", als::summary(&set)),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            std::process::exit(0);
        }
        Some("--push-preview") => {
            let path = args.get(1).map(String::as_str).unwrap_or("push.png");
            std::process::exit(check::push_preview(path));
        }
        Some(path) if !path.starts_with("--") => {}
        Some(other) => {
            eprintln!(
                "unknown option {other}; use a .als file, --check, --render <file.wav>, --link-check, --als-info <file.als> or --push-preview <file.png>"
            );
            std::process::exit(2);
        }
        None => {}
    }
    let opened = match args.first() {
        Some(path) => match open_set(Path::new(path)) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("openDJ Studio: {e}");
                std::process::exit(1);
            }
        },
        None => Opened {
            tracks: clip::demo_set(),
            tempo: TEMPO,
            returns: vec![("A Return".into(), 0), ("B Return".into(), 0)],
            note: String::new(),
        },
    };
    if let Err(e) = run_ui(opened) {
        eprintln!("openDJ Studio: {e}");
        std::process::exit(1);
    }
}

/// A set to play: the demo, or a Live set from the command line.
struct Opened {
    tracks: Vec<clip::Track>,
    tempo: f64,
    returns: Vec<(String, u32)>,
    /// For the status line: what didn't come in.
    note: String,
}

fn open_set(path: &Path) -> Result<Opened, String> {
    let mut set = als::read(path)?;
    let mut files: Vec<_> = set
        .tracks
        .iter()
        .flat_map(|t| t.slots.iter().flatten())
        .filter_map(|c| c.audio.as_ref().map(|a| a.path.clone()))
        .collect();
    files.sort();
    files.dedup();
    let files = files.len();
    let missing = als::decode_audio(&mut set.tracks);
    for m in &missing {
        eprintln!("not found: {m}");
    }
    for l in &set.left_out {
        eprintln!("left out: {l}");
    }
    let name = path
        .file_stem()
        .map_or(String::new(), |n| n.to_string_lossy().into());
    let mut note = name;
    if !missing.is_empty() {
        note += &format!(" · {} of {files} recordings not found", missing.len());
    }
    if !set.left_out.is_empty() {
        note += " · Arrangement and some parts left out";
    }
    Ok(Opened {
        tracks: set.tracks,
        tempo: set.tempo,
        returns: set.returns,
        note,
    })
}

fn run_ui(opened: Opened) -> Result<(), slint::PlatformError> {
    let Opened {
        tracks: mut set,
        tempo,
        returns,
        note,
    } = opened;
    let chosen = settings::audio();
    let rate = audio::rate(&chosen);
    // Drum Racks' samples, at the rate the studio runs at.
    let missing = als::load_kits(&mut set, rate);
    for m in &missing {
        eprintln!("drum sample not found: {m}");
    }
    let note = if missing.is_empty() {
        note
    } else {
        format!("{note} · {} drum samples not found", missing.len())
    };
    // Hybrid Reverbs' rooms, from the Live that's installed.
    let rooms = als::load_irs(&mut set, rate);
    for m in &rooms {
        eprintln!("reverb room not found: {m}");
    }
    let note = if rooms.is_empty() {
        note
    } else {
        format!("{note} · {} reverb rooms not found", rooms.len())
    };
    let (mut studio, remote) = Studio::new(&set, rate, tempo);
    let mut link_clock = link::LinkClock::new(tempo, rate);
    let link = link_clock.handle();
    link_clock.set_latency(chosen.buffer as f64 / rate);
    studio.link = Some(link_clock);
    // The sound card input is for audio tracks and External Instruments: a
    // set without them (the demo) never asks for the microphone.
    let wants_input = set.iter().any(|t| {
        t.sound == clip::Sound::Audio
            || t.devices
                .iter()
                .any(|d| matches!(d.builtin, Some(devices::Settings::External(_))))
    });
    let audio = audio::Audio::start(studio, &chosen, wants_input);
    ui(set, &returns, audio, chosen, remote, link, note)
}

/// The I/O pickers' channel lists for the sound card's channels.
fn show_channels(w: &AppWindow, ch: Channels) {
    w.set_in_channels(strings(
        in_channel_options(ch.inputs)
            .into_iter()
            .map(|(name, _)| name),
    ));
    w.set_out_channels(strings(out_pair_options(ch.outputs)));
}

const SAMPLE_RATES: [u32; 4] = opendj_engine::SAMPLE_RATES;
const BUFFER_SIZES: [u32; 6] = opendj_engine::BUFFER_SIZES;

/// The Audio settings page: the lists, the choices and what is running.
fn show_audio(w: &AppWindow, audio: &audio::Audio, c: &settings::AudioSettings) {
    let (outs, ins) = audio::devices();
    let default = "System default".to_string();
    w.set_out_devices(strings(std::iter::once(default.clone()).chain(outs)));
    w.set_in_devices(strings(
        ["None".to_string(), default.clone()].into_iter().chain(ins),
    ));
    w.set_rates(strings(
        std::iter::once("Device default".to_string())
            .chain(SAMPLE_RATES.iter().map(|r| format!("{r} Hz"))),
    ));
    w.set_buffers(strings(BUFFER_SIZES.iter().map(|b| {
        format!("{b} frames ({:.1} ms)", *b as f64 / audio.rate * 1000.0)
    })));
    let or_default = |s: &str| {
        if s.is_empty() {
            default.clone()
        } else {
            s.to_string()
        }
    };
    w.set_out_choice(or_default(&c.output).into());
    w.set_in_choice(c.input.as_deref().map_or("None".into(), or_default).into());
    w.set_rate_choice(
        c.sample_rate
            .map_or("Device default".into(), |r| format!("{r} Hz"))
            .into(),
    );
    w.set_buffer_choice(
        format!(
            "{} frames ({:.1} ms)",
            c.buffer,
            c.buffer as f64 / audio.rate * 1000.0
        )
        .into(),
    );
    let mut lines = vec![if audio.out_name.is_empty() {
        audio.status.clone()
    } else {
        format!(
            "Playing on {}: {} outputs at {} Hz.",
            audio.out_name, audio.out_channels, audio.rate as u32
        )
    }];
    if !audio.in_name.is_empty() {
        lines.push(format!(
            "Listening to {}: {} inputs.",
            audio.in_name, audio.in_channels
        ));
    }
    w.set_audio_info(lines.join("\n").into());
    let wanted = c.sample_rate.unwrap_or(audio.rate as u32);
    w.set_rate_note(if wanted != audio.rate as u32 && c.sample_rate.is_some() {
        "The new rate takes effect when the studio opens again.".into()
    } else {
        SharedString::new()
    });
}

/// The sound card's channel counts, for the I/O pickers.
#[derive(Clone, Copy, Default)]
struct Channels {
    inputs: usize,
    outputs: usize,
}

/// Input channels as Live lists them: each one, then each pair.
fn in_channel_options(n: usize) -> Vec<(String, (u16, bool))> {
    let n = n.max(2) as u16;
    let mono = (0..n).map(|c| (format!("{}", c + 1), (c, false)));
    let pairs = (0..n / 2).map(|p| (format!("{}/{}", 2 * p + 1, 2 * p + 2), (2 * p, true)));
    mono.chain(pairs).collect()
}

/// Output pairs: "1/2", "3/4", …
fn out_pair_options(n: usize) -> Vec<String> {
    (0..n.max(2) / 2)
        .map(|p| format!("{}/{}", 2 * p + 1, 2 * p + 2))
        .collect()
}

fn strings(v: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        v.into_iter().map(SharedString::from).collect::<Vec<_>>(),
    ))
}

/// The MIDI From list: every input, between All Ins and No Input.
fn midi_froms(ports: &midi::Ports) -> Vec<String> {
    std::iter::once("All Ins".to_string())
        .chain(ports.inputs.iter().cloned())
        .chain(std::iter::once("No Input".to_string()))
        .collect()
}

/// The MIDI To list: No Output, then every output.
fn midi_tos(ports: &midi::Ports) -> Vec<String> {
    std::iter::once("No Output".to_string())
        .chain(ports.outputs.iter().cloned())
        .collect()
}

/// Applies a pick from a track's I/O section (see `io-pick` in the window).
fn pick_io(io: &mut Io, which: i32, k: usize, ch: Channels, ports: &midi::Ports) {
    match which {
        0 => io.audio_in = (k == 0).then(|| io.audio_in.unwrap_or((0, false))),
        1 => {
            if let Some((_, c)) = in_channel_options(ch.inputs).get(k) {
                io.audio_in = Some(*c);
            }
        }
        2 => {
            io.audio_to = match k {
                0 => AudioTo::Main,
                1 => match io.audio_to {
                    AudioTo::Ext(a) => AudioTo::Ext(a),
                    _ => AudioTo::Ext(0),
                },
                _ => AudioTo::SendsOnly,
            }
        }
        3 => {
            if matches!(io.audio_to, AudioTo::Ext(_)) {
                io.audio_to = AudioTo::Ext(2 * k as u16);
            }
        }
        4 => {
            io.midi_from = match k {
                0 => MidiFrom::All,
                k => match ports.inputs.get(k - 1) {
                    Some(p) => MidiFrom::Port(p.clone()),
                    None => MidiFrom::None,
                },
            }
        }
        5 => io.midi_channel = k.checked_sub(1).map(|c| c as u8),
        6 => {
            let channel = io.midi_to.as_ref().map_or(0, |m| m.1);
            io.midi_to = k
                .checked_sub(1)
                .and_then(|k| ports.outputs.get(k))
                .map(|p| (p.clone(), channel));
        }
        7 => {
            if let Some(m) = io.midi_to.as_mut() {
                m.1 = k.min(15) as u8;
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn ui(
    set: Vec<clip::Track>,
    returns: &[(String, u32)],
    audio: audio::Audio,
    chosen: settings::AudioSettings,
    remote: engine::Remote,
    link: Arc<rusty_link::AblLink>,
    note: String,
) -> Result<(), slint::PlatformError> {
    let rate = audio.rate;
    let channels = Rc::new(Cell::new(Channels {
        inputs: audio.in_channels,
        outputs: audio.out_channels,
    }));
    let audio = Rc::new(RefCell::new(audio));
    let chosen = Rc::new(RefCell::new(chosen));
    let n = set.len();
    let window = AppWindow::new()?;
    let monitor = remote.monitor.clone();
    let controls = Arc::new(Controls::new(
        &set,
        &TRACK_COLORS,
        remote.commands,
        monitor.clone(),
    ));
    // Keyboards to tracks, tracks' notes to external synths.
    let router = Rc::new(midi::start(controls.clone(), remote.midi_out));
    let mut ports = router.ports();
    show_channels(&window, channels.get());
    window.set_midi_froms(strings(midi_froms(&ports)));
    window.set_midi_tos(strings(midi_tos(&ports)));
    // Plug-ins start in their helpers and join the tracks as they come up.
    let plugins = plugins::Plugins::start(&set, rate as u32, controls.clone());
    let tracks = Rc::new(VecModel::from(
        (0..set.len())
            .map(|i| track_row(&controls, &plugins, i, None))
            .collect::<Vec<_>>(),
    ));
    let slots = Rc::new(VecModel::from(
        (0..SCENES)
            .flat_map(|s| {
                let colors: Vec<u32> = set
                    .iter()
                    .enumerate()
                    .map(|(t, tr)| tr.color.unwrap_or(TRACK_COLORS[t % TRACKS]))
                    .collect();
                set.iter().enumerate().map(move |(t, track)| {
                    let clip = track.slots[s].as_ref();
                    SlotData {
                        name: clip.map_or(SharedString::new(), |c| c.name.as_str().into()),
                        color: color(colors[t]),
                        state: if clip.is_some() {
                            SlotState::Stopped
                        } else {
                            SlotState::Empty
                        },
                        beats: clip.map_or(0.0, |c| c.beats as f32),
                    }
                })
            })
            .collect::<Vec<_>>(),
    ));
    window.set_tracks(ModelRc::from(tracks.clone()));
    window.set_slots(ModelRc::from(slots.clone()));
    window.set_scenes(SCENES as i32);
    window.set_quantize(Quantize::Bar.label().into());
    window.set_main_volume(controls.main_volume());
    for (i, (name, rgb)) in returns.iter().take(2).enumerate() {
        let c = if *rgb == 0 { None } else { Some(color(*rgb)) };
        if i == 0 {
            window.set_return_a(name.as_str().into());
            if let Some(c) = c {
                window.set_return_a_color(c);
            }
        } else {
            window.set_return_b(name.as_str().into());
            if let Some(c) = c {
                window.set_return_b_color(c);
            }
        }
    }

    let trash = RefCell::new(remote.trash);
    let push_status: push::Status = Arc::new(Mutex::new(String::new()));
    let push = push::start(controls.clone(), push_status.clone());

    {
        let c = controls.clone();
        window.on_launch_clip(move |t, s| c.launch(t as usize, s as usize));
    }
    {
        let c = controls.clone();
        window.on_select(move |t, s| c.select(t as usize, usize::try_from(s).ok()));
    }
    {
        let c = controls.clone();
        window.on_launch_scene(move |s| c.launch_scene(s as usize));
    }
    {
        let c = controls.clone();
        window.on_stop_track(move |t| c.stop(t as usize));
    }
    {
        let c = controls.clone();
        window.on_stop_all(move || c.stop_all());
    }
    {
        let c = controls.clone();
        window.on_toggle_play(move || c.toggle_play());
    }
    {
        let c = controls.clone();
        window.on_tempo_step(move |d| c.tempo_step(d as f64));
    }
    {
        let link = link.clone();
        window.on_toggle_link(move || {
            let link = link.clone();
            // Opening and closing sockets: off the UI thread.
            std::thread::spawn(move || link.enable(!link.is_enabled()));
        });
    }
    {
        let c = controls.clone();
        window.on_next_quantize(move || {
            c.next_quantize();
        });
    }
    {
        let c = controls.clone();
        window.on_mix(move |t, which, v| {
            if let Some(&m) = Mix::ALL.get(which as usize) {
                c.set_mix(t as usize, m, v);
            }
        });
    }
    {
        let c = controls.clone();
        window.on_main_volume_changed(move |v| c.set_main_volume(v));
    }
    {
        let c = controls.clone();
        window.on_param(move |t, i, v| c.set_param(t as usize, i as usize, v));
    }
    {
        let c = controls.clone();
        window.on_play_note(move |t, p, v| c.note(t as usize, p.clamp(0, 127) as u8, v));
    }
    {
        let c = controls.clone();
        window.on_toggle_device(move |t, d, on| c.set_device_on(t as usize, d as usize, on));
    }
    {
        let (c, r, ch) = (controls.clone(), router.clone(), channels.clone());
        window.on_io_pick(move |t, which, k| {
            let ports = r.ports();
            c.edit_io(t as usize, |io| {
                pick_io(io, which, k.max(0) as usize, ch.get(), &ports)
            });
        });
    }
    {
        let c = controls.clone();
        window.on_set_monitor(move |t, m| {
            if let Some(&m) = Monitoring::ALL.get(m as usize) {
                c.edit_io(t as usize, |io| io.monitor = m);
            }
        });
    }
    {
        let c = controls.clone();
        window.on_toggle_arm(move |t| c.toggle_arm(t as usize));
    }
    {
        let c = controls.clone();
        window.on_device_param(move |t, d, i, n| {
            let (t, d, i) = (t as usize, d as usize, i as usize);
            if let Some(p) = c.builtin(t, d).and_then(|s| s.params().get(i).cloned()) {
                c.set_device_param(t, d, i, p.at_norm(n));
            }
        });
    }
    {
        let c = controls.clone();
        window.on_device_click(move |t, d, i| {
            let (t, d, i) = (t as usize, d as usize, i as usize);
            if let Some(p) = c.builtin(t, d).and_then(|s| s.params().get(i).cloned()) {
                c.set_device_param(t, d, i, p.next());
            }
        });
    }
    {
        // An EQ Eight band's dot dragged on its graph: frequency across,
        // gain up and down (for the shapes with a gain).
        let c = controls.clone();
        window.on_device_drag(move |t, d, band, x, y| {
            let (t, d, b) = (t as usize, d as usize, band.max(0) as usize);
            let Some(devices::Settings::Eq8(e)) = c.builtin(t, d) else {
                return;
            };
            let Some(bd) = e.bands.get(b) else {
                return;
            };
            let base = b * devices::BAND_PARAMS;
            let freq = devices::graph_freq(x.clamp(0.0, 1.0));
            c.set_device_param(t, d, base + 2, freq);
            if bd.shape.has_gain() {
                let gain = devices::EQ_GAIN - y.clamp(0.0, 1.0) * 2.0 * devices::EQ_GAIN;
                c.set_device_param(t, d, base + 3, gain);
            }
        });
    }
    {
        let p = plugins.clone();
        window.on_open_device(move |t, d| {
            p.toggle_window(t as usize, d as usize);
        });
    }

    // Settings: the Audio page.
    let audio_status = Rc::new(RefCell::new(audio.borrow().status.clone()));
    show_audio(&window, &audio.borrow(), &chosen.borrow());
    {
        let w = window.as_weak();
        let (a, ch) = (audio.clone(), chosen.clone());
        window.on_settings_opened(move || {
            if let Some(w) = w.upgrade() {
                show_audio(&w, &a.borrow(), &ch.borrow());
            }
        });
    }
    {
        let w = window.as_weak();
        let (a, chosen, ch, st) = (
            audio.clone(),
            chosen.clone(),
            channels.clone(),
            audio_status.clone(),
        );
        window.on_settings_pick(move |field, k| {
            let Some(w) = w.upgrade() else { return };
            let k = k.max(0) as usize;
            let (outs, ins) = audio::devices();
            let mut c = chosen.borrow_mut();
            let before = c.clone();
            match field {
                // System default, then the devices.
                0 => {
                    c.output = k
                        .checked_sub(1)
                        .and_then(|k| outs.get(k))
                        .cloned()
                        .unwrap_or_default()
                }
                // None, System default, then the devices.
                1 => {
                    c.input = match k {
                        0 => None,
                        1 => Some(String::new()),
                        k => ins.get(k - 2).cloned().or(Some(String::new())),
                    }
                }
                // Device default, then the rates.
                2 => c.sample_rate = k.checked_sub(1).and_then(|k| SAMPLE_RATES.get(k)).copied(),
                3 => {
                    c.buffer = BUFFER_SIZES
                        .get(k)
                        .copied()
                        .unwrap_or(settings::DEFAULT_BUFFER)
                }
                _ => {}
            }
            if *c == before {
                return;
            }
            settings::save_audio(&c);
            // A new rate waits for the next opening; the rest moves now.
            if field != 2 {
                let mut audio = a.borrow_mut();
                audio.open(&c);
                ch.set(Channels {
                    inputs: audio.in_channels,
                    outputs: audio.out_channels,
                });
                *st.borrow_mut() = audio.status.clone();
                show_channels(&w, ch.get());
            }
            show_audio(&w, &a.borrow(), &c);
        });
    }

    let timer = slint::Timer::default();
    let w = window.as_weak();
    let mut seen = 0u64;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(33),
        move || {
            let Some(w) = w.upgrade() else { return };
            let now = router.ports();
            if now != ports {
                w.set_midi_froms(strings(midi_froms(&now)));
                w.set_midi_tos(strings(midi_tos(&now)));
                ports = now;
            }
            // Programs the audio thread replaced are dropped here.
            while trash.borrow_mut().pop().is_ok() {}
            for s in 0..SCENES {
                for t in 0..n {
                    let i = s * n + t;
                    let state = match monitor.slot(t, s) {
                        engine::SlotState::Empty => SlotState::Empty,
                        engine::SlotState::Stopped => SlotState::Stopped,
                        engine::SlotState::Queued => SlotState::Queued,
                        engine::SlotState::Playing => SlotState::Playing,
                    };
                    if let Some(mut row) = slots.row_data(i) {
                        if row.state != state {
                            row.state = state;
                            slots.set_row_data(i, row);
                        }
                    }
                }
            }
            // Mixer, knobs and selection: from either the window or Push.
            let version = controls.version();
            let changed = version != seen;
            seen = version;
            if changed {
                // Clips Push made or changed in Steps mode.
                for i in 0..slots.row_count() {
                    let (t, sc) = (i % n, i / n);
                    let info = controls.clip_info(t, sc);
                    if let Some(mut row) = slots.row_data(i) {
                        let name: SharedString = info
                            .as_ref()
                            .map_or(SharedString::new(), |(n, _)| n.as_str().into());
                        let beats = info.as_ref().map_or(0.0, |(_, b)| *b as f32);
                        if row.name != name || row.beats != beats {
                            row.name = name;
                            row.beats = beats;
                            slots.set_row_data(i, row);
                        }
                    }
                }
                w.set_selected_track(controls.selected_track() as i32);
                w.set_selected_scene(controls.selected_scene().map_or(-1, |s| s as i32));
                w.set_main_volume(controls.main_volume());
                w.set_quantize(controls.quantize().label().into());
            }
            for t in 0..tracks.row_count() {
                if let Some(mut row) = tracks.row_data(t) {
                    let progress = monitor.progress(t);
                    let stopping = monitor.stopping(t);
                    let level = meter(row.level, monitor.take_peak(t));
                    if changed {
                        row = TrackData {
                            progress: row.progress,
                            stopping: row.stopping,
                            level: row.level,
                            ..track_row(&controls, &plugins, t, Some(&row))
                        };
                    }
                    if changed
                        || (row.progress - progress).abs() > 0.002
                        || row.stopping != stopping
                        || (row.level - level).abs() > 0.005
                    {
                        row.progress = progress;
                        row.stopping = stopping;
                        row.level = level;
                        tracks.set_row_data(t, row);
                    }
                }
            }
            w.set_main_level(meter(w.get_main_level(), monitor.take_main_peak()));
            let beat = monitor.beat();
            let bar = (beat / engine::QUANTUM).floor();
            let in_bar = beat.rem_euclid(engine::QUANTUM).floor();
            w.set_position(format!("{}.{}", bar as i64 + 1, in_bar as i64 + 1).into());
            w.set_tempo(format!("{:.1}", monitor.tempo()).into());
            w.set_playing(monitor.playing());
            w.set_blink(beat.rem_euclid(1.0) < 0.5);
            w.set_link(link.is_enabled());
            w.set_peers(link.num_peers() as i32);
            w.set_load(format!("{:.0}%", monitor.load() * 100.0).into());
            let push = push_status.lock().map(|s| s.clone()).unwrap_or_default();
            let line = [note.as_str(), audio_status.borrow().as_str(), push.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            if w.get_status().as_str() != line {
                w.set_status(line.into());
            }
        },
    );

    let result = window.run();
    // Push back to Live before the sound stops.
    drop(push);
    drop(audio);
    result
}

/// A track's row for the window, from the shared controls.
/// Puts `new` in `old`'s model when it has as many rows, so the lists on
/// screen keep their items (a knob being dragged stays under the pointer).
fn reuse<T: Clone + PartialEq + 'static>(old: Option<&ModelRc<T>>, new: Vec<T>) -> ModelRc<T> {
    if let Some(old) = old {
        if let Some(m) = old.as_any().downcast_ref::<VecModel<T>>() {
            if m.row_count() == new.len() {
                for (i, v) in new.into_iter().enumerate() {
                    if m.row_data(i).as_ref() != Some(&v) {
                        m.set_row_data(i, v);
                    }
                }
                return old.clone();
            }
        }
    }
    ModelRc::new(VecModel::from(new))
}

/// A knob or switch on a built-in device's panel.
fn knob_data(i: usize, p: &devices::Param) -> KnobData {
    KnobData {
        index: i as i32,
        name: p.name.as_str().into(),
        value: p.norm(),
        text: p.text().into(),
        bipolar: p.bipolar,
        toggle: p.unit == devices::Unit::Toggle,
        on: p.unit == devices::Unit::Toggle && p.value >= 0.5,
    }
}

/// Fills a device row's panel from its built-in settings.
fn device_row(
    mut row: DeviceData,
    s: Option<&devices::Settings>,
    old: Option<&DeviceData>,
) -> DeviceData {
    let Some(s) = s else {
        return row;
    };
    let params = s.params();
    let all: Vec<KnobData> = params
        .iter()
        .enumerate()
        .map(|(i, p)| knob_data(i, p))
        .collect();
    let layout = s.knob_layout();
    let knobs: Vec<KnobData> = match &layout {
        Some((order, _)) => order.iter().filter_map(|&i| all.get(i).cloned()).collect(),
        None => params
            .iter()
            .zip(&all)
            .filter(|(p, _)| !p.is_switch())
            .map(|(_, k)| k.clone())
            .collect(),
    };
    let switches: Vec<KnobData> = params
        .iter()
        .zip(&all)
        .filter(|(p, _)| p.is_switch())
        .map(|(_, k)| k.clone())
        .collect();
    let eq = matches!(s, devices::Settings::Eq8(_));
    let rows = if knobs.len() > 8 {
        3
    } else if knobs.len() > 3 {
        2
    } else {
        1
    };
    let cols = layout.map_or(knobs.len().div_ceil(rows).max(1), |(_, c)| c);
    let mut dots = Vec::new();
    if let Some((curve, top, bottom)) = s.curve() {
        let y = |db: f32| ((top - db) / (top - bottom) * 100.0).clamp(-2.0, 102.0);
        let mut path = String::new();
        for (i, db) in curve.iter().enumerate() {
            let x = i as f32 / (curve.len() - 1) as f32 * 300.0;
            path += &format!("{} {x:.1} {:.1} ", if i == 0 { "M" } else { "L" }, y(*db));
        }
        row.curve = path.into();
        row.zero = y(0.0);
        if let devices::Settings::Eq8(e) = s {
            dots = e
                .bands
                .iter()
                .enumerate()
                .map(|(b, band)| {
                    let at = if band.shape.has_gain() {
                        band.gain_db
                    } else {
                        e.response(48_000.0, band.freq as f64) - e.gain_db
                    };
                    DotData {
                        band: b as i32,
                        x: devices::graph_place(band.freq),
                        y: y(at.clamp(bottom, top)) / 100.0,
                        on: band.on,
                        gain: band.shape.has_gain(),
                    }
                })
                .collect();
        }
    }
    let graph = !row.curve.is_empty() && !eq;
    row.panel_width = if eq {
        560.0
    } else {
        (if switches.is_empty() { 0.0 } else { 96.0 }
            + cols as f32 * 58.0
            + if graph { 190.0 } else { 0.0 }
            + 20.0)
            .max(150.0)
    };
    row.builtin = true;
    row.eq = eq;
    row.cols = cols as i32;
    row.rows = rows as i32;
    row.params = reuse(old.map(|o| &o.params), all);
    row.knobs = reuse(old.map(|o| &o.knobs), knobs);
    row.switches = reuse(old.map(|o| &o.switches), switches);
    row.dots = reuse(old.map(|o| &o.dots), dots);
    row
}

fn track_row(
    c: &Controls,
    plugins: &plugins::Plugins,
    t: usize,
    old: Option<&TrackData>,
) -> TrackData {
    let info = &c.tracks[t];
    let params: Vec<ParamData> = (0..sound::PARAMS)
        .map(|i| ParamData {
            name: c.param_name(t, i).into(),
            value: c.param(t, i),
            text: c.param_text(t, i).into(),
        })
        .collect();
    let volume = c.mix(t, Mix::Volume);
    let pan = c.mix(t, Mix::Pan);
    let io = c.io(t);
    TrackData {
        audio_from: if io.audio_in.is_some() {
            "Ext. In"
        } else {
            "No Input"
        }
        .into(),
        in_channel: io.in_channel_text().into(),
        midi_from: match &io.midi_from {
            MidiFrom::All => "All Ins".into(),
            MidiFrom::Port(p) => p.as_str().into(),
            MidiFrom::None => "No Input".into(),
        },
        midi_channel: match io.midi_channel {
            Some(c) => format!("Ch. {}", c + 1).into(),
            None => "All Channels".into(),
        },
        monitor: Monitoring::ALL
            .iter()
            .position(|&m| m == io.monitor)
            .unwrap_or(1) as i32,
        armed: io.armed,
        audio_to: match io.audio_to {
            AudioTo::Main => "Main",
            AudioTo::Ext(_) => "Ext. Out",
            AudioTo::SendsOnly => "Sends Only",
        }
        .into(),
        out_channel: match io.audio_to {
            AudioTo::Ext(a) => format!("{}/{}", a + 1, a + 2).into(),
            _ => SharedString::new(),
        },
        midi_to: io
            .midi_to
            .as_ref()
            .map_or("No Output".into(), |(p, _)| p.as_str().into()),
        midi_to_channel: io
            .midi_to
            .as_ref()
            .map_or(SharedString::new(), |(_, ch)| format!("{}", ch + 1).into()),
        name: info.name.as_str().into(),
        color: color(info.color),
        progress: c.monitor.progress(t),
        stopping: c.monitor.stopping(t),
        device: if info.pads.is_empty() {
            device_name(info.sound)
        } else {
            format!("Drum Rack · {} pads", info.pads.len())
        }
        .into(),
        drums: info.drums(),
        audio: info.audio(),
        external: info.external(),
        kind: if info.audio() { "AUDIO" } else { "MIDI" }.into(),
        input: io.input_text(info.audio()).into(),
        chain: reuse(
            old.map(|o| &o.chain),
            info.devices
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let state = plugins.state(t, i);
                    let running = state == Some(plugins::State::Running);
                    let builtin = c.builtin(t, i);
                    let note = match &state {
                        Some(plugins::State::Running) => {
                            if d.plugin.as_ref().is_some_and(|p| p.state.is_some()) {
                                "Playing, with the set's sound".to_string()
                            } else {
                                "Playing".to_string()
                            }
                        }
                        Some(s) => s.text(),
                        None if d.plugin.is_some() => "Plug-in".to_string(),
                        None if builtin.is_some() => String::new(),
                        None if d.name.ends_with("Drum Rack)") || d.name == "Drum Rack" => {
                            "Its pads play as the track's kit".to_string()
                        }
                        None => "Live device: no built-in match yet".to_string(),
                    };
                    let before = old.and_then(|o| o.chain.row_data(i));
                    device_row(
                        DeviceData {
                            name: d.name.as_str().into(),
                            kind: d.kind.as_str().into(),
                            note: note.into(),
                            on: c.device_on(t, i),
                            can_open: running,
                            ..DeviceData::default()
                        },
                        builtin.as_ref(),
                        before.as_ref(),
                    )
                })
                .collect(),
        ),
        plugin_instrument: info
            .devices
            .iter()
            .any(|d| d.plugin.as_ref().is_some_and(|p| p.instrument)),
        level: 0.0,
        volume,
        volume_text: control::fader_text(volume).into(),
        pan,
        pan_text: control::pan_text(pan).into(),
        send_a: c.mix(t, Mix::SendA),
        send_b: c.mix(t, Mix::SendB),
        params: ModelRc::new(VecModel::from(params)),
    }
}
