//! The Session: a Live set's clips from openDJ Studio's engine, on a
//! channel of their own (`[Session1]`), following the master deck's beat.
//!
//! File › Open Live Set… reads the set on a thread of its own (recordings
//! take a while to decode), then starts the studio on its real-time thread
//! (`opendj_studio_engine::session`) and hands its feed to the audio
//! engine. The Jam source shows its clips to launch while one is open.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use opendj_control::ControlRegistry;
use opendj_engine::AudioEngine;
use opendj_studio_engine::als;
use opendj_studio_engine::clip::{Sound, Track, SCENES};
use opendj_studio_engine::control::{fader_gain, pan_text, UNITY};
use opendj_studio_engine::engine::{Command, Remote, SlotState, Studio};
use opendj_studio_engine::session::{self, SessionThread};
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{AppWindow, SessionSlot, SessionTrack, SessionUi};

/// How often the grid reads the studio's states and meters.
const TICK: Duration = Duration::from_millis(33);

/// A set read from disk, ready to start.
struct Opened {
    tracks: Vec<Track>,
    tempo: f64,
    scenes: usize,
    name: String,
    note: String,
}

/// A set that plays.
struct Running {
    remote: Remote,
    /// Kept for as long as the set is open; dropping it stops the thread.
    _thread: SessionThread,
    tracks: usize,
    scenes: usize,
    slots: Rc<VecModel<SessionSlot>>,
    lanes: Rc<VecModel<SessionTrack>>,
    _timer: slint::Timer,
}

impl Running {
    fn send(&mut self, cmd: Command) {
        let _ = self.remote.commands.push(cmd);
    }

    /// Changes a track's strip, then sends every track's gain: a solo
    /// silences the tracks without one.
    fn strip(&mut self, track: i32, change: impl FnOnce(&mut SessionTrack)) {
        let Some(mut lane) = usize::try_from(track)
            .ok()
            .and_then(|t| self.lanes.row_data(t))
        else {
            return;
        };
        change(&mut lane);
        lane.pan_text = pan_text(lane.pan).into();
        self.lanes.set_row_data(track as usize, lane);
        let soloing = self.lanes.iter().any(|l| l.solo);
        for t in 0..self.tracks {
            let Some(l) = self.lanes.row_data(t) else {
                continue;
            };
            let heard = !l.mute && (l.solo || !soloing);
            self.send(Command::Volume {
                track: t,
                gain: if heard { fader_gain(l.volume) } else { 0.0 },
            });
        }
    }
}

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
    static ENGINE: RefCell<Option<(Rc<AudioEngine>, Arc<ControlRegistry>)>> = const { RefCell::new(None) };
}

/// Hooks the grid's buttons up.
pub fn install(app: &AppWindow, engine: Rc<AudioEngine>, registry: Arc<ControlRegistry>) {
    ENGINE.with(|e| *e.borrow_mut() = Some((engine, registry)));
    let ui = app.global::<SessionUi>();
    let weak = app.as_weak();
    ui.on_open_set(move || {
        if let Some(app) = weak.upgrade() {
            open_dialog(&app);
        }
    });
    let weak = app.as_weak();
    ui.on_close_set(move || {
        close();
        if let Some(app) = weak.upgrade() {
            closed(&app);
        }
    });
    ui.on_launch(|track, scene| {
        with_running(|r| {
            r.send(Command::Launch {
                track: track as usize,
                scene: scene as usize,
            })
        })
    });
    ui.on_stop(|track| with_running(|r| r.send(Command::Stop(track as usize))));
    ui.on_scene(|scene| with_running(|r| r.send(Command::Scene(scene as usize))));
    ui.on_stop_all(|| with_running(|r| r.send(Command::StopAll)));
    ui.on_set_volume(|track, position| {
        with_running(|r| r.strip(track, |l| l.volume = position.clamp(0.0, 1.0)))
    });
    let weak = app.as_weak();
    ui.on_set_main_volume(move |position| {
        let position = position.clamp(0.0, 1.0);
        with_running(|r| r.send(Command::MainVolume(fader_gain(position))));
        if let Some(app) = weak.upgrade() {
            app.global::<SessionUi>().set_main_volume(position);
        }
    });
    ui.on_set_pan(|track, pan| {
        with_running(|r| {
            let pan = pan.clamp(0.0, 1.0);
            // Snaps to the centre near it.
            let pan = if (pan - 0.5).abs() < 0.03 { 0.5 } else { pan };
            r.strip(track, |l| l.pan = pan);
            r.send(Command::Pan {
                track: track as usize,
                pan,
            });
        })
    });
    ui.on_toggle_mute(|track| with_running(|r| r.strip(track, |l| l.mute = !l.mute)));
    ui.on_toggle_solo(|track| with_running(|r| r.strip(track, |l| l.solo = !l.solo)));
}

fn with_running(f: impl FnOnce(&mut Running)) {
    RUNNING.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            f(r);
        }
    });
}

/// Asks for a Live set and opens it.
pub fn open_dialog(app: &AppWindow) {
    let weak = app.as_weak();
    crate::prefs::choose_file("Open a Live Set (.als)", move |path| {
        let _ = weak.upgrade_in_event_loop(move |app| {
            let is_set = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("als"));
            if is_set {
                open(&app, path);
            } else {
                app.global::<SessionUi>()
                    .set_error("That isn't a Live set (.als).".into());
                show(&app);
            }
        });
    });
}

/// Reads the set at `path` off the UI thread, then plays it in place of
/// the one open.
pub fn open(app: &AppWindow, path: PathBuf) {
    let Some(rate) = ENGINE.with(|e| e.borrow().as_ref().map(|(a, _)| a.sample_rate())) else {
        return;
    };
    let ui = app.global::<SessionUi>();
    ui.set_loading(true);
    ui.set_error(SharedString::new());
    show(app);
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let opened = read(&path, rate);
        let _ = weak.upgrade_in_event_loop(move |app| {
            app.global::<SessionUi>().set_loading(false);
            match opened {
                Ok(o) => start(&app, o, rate),
                Err(e) => {
                    eprintln!("can't open {}: {e}", path.display());
                    app.global::<SessionUi>().set_error(e.into());
                }
            }
        });
    });
}

/// Brings the Jam source up, where the clips are.
fn show(app: &AppWindow) {
    let ui = app.global::<SessionUi>();
    ui.set_show_request(ui.get_show_request() + 1);
}

/// The set, its recordings decoded and its drum and reverb samples at
/// `rate`, with a line on what didn't come in.
fn read(path: &Path, rate: f64) -> Result<Opened, String> {
    let mut set = als::read(path)?;
    let mut files: Vec<_> = set
        .tracks
        .iter()
        .flat_map(|t| t.slots.iter().flatten())
        .filter_map(|c| c.audio.as_ref().map(|a| a.path.clone()))
        .collect();
    files.sort();
    files.dedup();
    let missing = als::decode_audio(&mut set.tracks);
    let samples = als::load_kits(&mut set.tracks, rate);
    let mut rooms = als::load_irs(&mut set.tracks, rate);
    rooms.extend(als::load_simplers(&mut set.tracks, rate));
    for m in missing.iter().chain(&samples).chain(&rooms) {
        eprintln!("not found: {m}");
    }
    let mut notes = Vec::new();
    if !missing.is_empty() {
        notes.push(format!(
            "{} of {} recordings not found",
            missing.len(),
            files.len()
        ));
    }
    if !samples.is_empty() {
        notes.push(format!("{} drum samples not found", samples.len()));
    }
    let plugins = set
        .tracks
        .iter()
        .filter(|t| t.devices.iter().any(|d| d.plugin.is_some()))
        .count();
    if plugins > 0 {
        notes.push(format!("{plugins} plug-in tracks silent for now"));
    }
    if !set.left_out.is_empty() {
        notes.push("Arrangement and some parts left out".into());
    }
    Ok(Opened {
        scenes: set.scenes.clamp(1, SCENES),
        tempo: set.tempo,
        tracks: set.tracks,
        name: path
            .file_stem()
            .map_or(String::new(), |n| n.to_string_lossy().into()),
        note: notes.join(" · "),
    })
}

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// A meter's next height (0..1 over -60..+6 dB) from its last height and
/// the peak since: jumps up, falls back about 20 dB a second (as Studio's).
fn meter(last: f32, peak: f32) -> f32 {
    let now = if peak > 0.0 {
        ((20.0 * peak.log10() + 60.0) / 66.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    now.max(last - 0.01)
}

/// Starts `opened` on the Session channel, in place of what played there.
fn start(app: &AppWindow, opened: Opened, rate: f64) {
    let Some((engine, registry)) = ENGINE.with(|e| e.borrow().clone()) else {
        return;
    };
    close();
    let Opened {
        tracks,
        tempo,
        scenes,
        name,
        note,
    } = opened;
    let lanes: Vec<SessionTrack> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| SessionTrack {
            name: t.name.clone().into(),
            color: color(t.color.unwrap_or_else(|| als::color(i))),
            progress: -1.0,
            kind: if t.sound == Sound::Audio {
                "AUDIO"
            } else {
                "MIDI"
            }
            .into(),
            stopping: false,
            level: 0.0,
            volume: t.volume.unwrap_or(UNITY),
            pan: t.pan,
            pan_text: pan_text(t.pan).into(),
            mute: false,
            solo: false,
        })
        .collect();
    let mut slots = Vec::with_capacity(scenes * tracks.len());
    for s in 0..scenes {
        for t in &tracks {
            let clip = t.slots.get(s).and_then(Option::as_ref);
            slots.push(SessionSlot {
                name: clip.map_or(SharedString::new(), |c| c.name.clone().into()),
                state: if clip.is_some() { 1 } else { 0 },
            });
        }
    }
    let count = tracks.len();
    let (studio, remote) = Studio::new(&tracks, rate, tempo);
    drop(tracks);
    let (feed, thread) = match session::start(studio, engine.beat_clock()) {
        Ok(started) => started,
        Err(e) => {
            app.global::<SessionUi>()
                .set_error(format!("The Session couldn't start: {e}").into());
            return;
        }
    };
    // The old feed (none, after close) is dropped here, not on the audio
    // thread.
    drop(engine.set_session(Some(Box::new(feed))));
    if let Some(on) = registry.get("[Session1]", "main_mix") {
        on.set_bool(true);
    }
    let ui = app.global::<SessionUi>();
    let slots = Rc::new(VecModel::from(slots));
    let lanes = Rc::new(VecModel::from(lanes));
    ui.set_name(name.into());
    ui.set_note(note.into());
    ui.set_scenes(scenes as i32);
    ui.set_main_volume(UNITY);
    ui.set_main_level(0.0);
    ui.set_tracks(ModelRc::from(lanes.clone()));
    ui.set_slots(ModelRc::from(slots.clone()));
    ui.set_open(true);
    let timer = slint::Timer::default();
    let weak = app.as_weak();
    timer.start(slint::TimerMode::Repeated, TICK, move || {
        if let Some(app) = weak.upgrade() {
            tick(&app);
        }
    });
    RUNNING.with(|r| {
        *r.borrow_mut() = Some(Running {
            remote,
            _thread: thread,
            tracks: count,
            scenes,
            slots,
            lanes,
            _timer: timer,
        })
    });
}

/// Reads the studio's slot states, progress and meters into the grid.
fn tick(app: &AppWindow) {
    with_running(|r| {
        // Edits the audio thread let go of are freed here.
        while r.remote.trash.pop().is_ok() {}
        while r.remote.midi_out.pop().is_ok() {}
        let m = &r.remote.monitor;
        for s in 0..r.scenes {
            for t in 0..r.tracks {
                let i = s * r.tracks + t;
                let Some(mut slot) = r.slots.row_data(i) else {
                    continue;
                };
                let state = match m.slot(t, s) {
                    SlotState::Empty => 0,
                    SlotState::Stopped => 1,
                    SlotState::Queued => 2,
                    SlotState::Playing => 3,
                };
                if slot.state != state {
                    slot.state = state;
                    r.slots.set_row_data(i, slot);
                }
            }
        }
        for t in 0..r.tracks {
            let Some(mut lane) = r.lanes.row_data(t) else {
                continue;
            };
            let progress = m.progress(t);
            let level = meter(lane.level, m.take_peak(t));
            let stopping = m.stopping(t);
            if (lane.progress - progress).abs() > 0.002
                || (lane.level - level).abs() > 0.002
                || lane.stopping != stopping
            {
                lane.progress = progress;
                lane.level = level;
                lane.stopping = stopping;
                r.lanes.set_row_data(t, lane);
            }
        }
        let ui = app.global::<SessionUi>();
        ui.set_playing(m.playing());
        ui.set_blink(m.beat().rem_euclid(1.0) < 0.5);
        ui.set_main_level(meter(ui.get_main_level(), m.take_main_peak()));
    });
}

/// Stops the set and takes it off the channel.
fn close() {
    let engine = ENGINE.with(|e| e.borrow().as_ref().map(|(a, _)| a.clone()));
    if let Some(engine) = engine {
        drop(engine.set_session(None));
    }
    // After the channel let go of the feed, so its last block isn't waited
    // on; dropping the thread joins it.
    RUNNING.with(|r| r.borrow_mut().take());
}

/// Clears the grid after [`close`].
fn closed(app: &AppWindow) {
    let ui = app.global::<SessionUi>();
    ui.set_open(false);
    ui.set_playing(false);
    ui.set_tracks(ModelRc::default());
    ui.set_slots(ModelRc::default());
}
