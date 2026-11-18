//! openDJ for tablets, iPad first: a very small DJ app on openDJ's engine.
//!
//! Two decks, the tracks in one folder, play/cue/sync, headphone cue, a
//! crossfader and waveforms, in the desktop app's look; and, for now, two
//! controllers: the Pioneer DDJ-FLX4 and the Allen & Heath Xone:K2 (see
//! `controllers.txt`), whose browse knobs and LOAD buttons pick and load
//! tracks. The desktop app's other parts (plugin hosting, add-on programs,
//! stems with Python, the library database, HID controllers) are left out.
//!
//! It runs on the desktop too (`cargo run -p opendj-tablet`), to try
//! changes without an iPad. See `crates/tablet/README.md` for the iPad
//! build.

mod session;
mod wave;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendj_control::{ControlHandle, ControlRegistry};
use opendj_engine::{deck_group, AudioEngine, AudioSetup, Role, Route, Track};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

slint::include_modules!();

// Ableton Link and the time-stretcher bring C++ code; iOS links its C++
// library only when asked.
#[cfg(target_os = "ios")]
#[link(name = "c++")]
unsafe extern "C" {}

const DECKS: usize = 2;
/// Seconds of track across the scrolling waveforms.
const SPAN: f64 = 8.0;
/// How long an alert stays in the status line.
const ALERT_TIME: Duration = Duration::from_secs(6);

/// Where the tracks are: `OPENDJ_MUSIC` if set; on iPadOS the app's own
/// Documents folder, which the Files app shows as On My iPad › openDJ;
/// elsewhere the user's Music folder.
fn music_folder() -> PathBuf {
    if let Some(dir) = std::env::var_os("OPENDJ_MUSIC").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = opendj_engine::paths::home_dir().unwrap_or_default();
    if cfg!(target_os = "ios") {
        home.join("Documents")
    } else {
        opendj_engine::scan::default_music_dir().unwrap_or(home)
    }
}

/// `m:ss`.
fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Takes the presses or steps a controller has left on `c`.
fn take(c: &ControlHandle) -> f64 {
    let v = c.get();
    if v != 0.0 {
        c.set(0.0);
    }
    v
}

/// The controller files the app uses, one per line (see `controllers.txt`).
const CONTROLLER_FILES: &str = include_str!("../controllers.txt");

fn controller_files() -> impl Iterator<Item = &'static str> {
    CONTROLLER_FILES
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// A folder with only the supported controllers' files, copied from the
/// mappings folder (bundled beside the app on iPadOS, the repository's
/// `res/controllers` on the desktop), so no other mapping is picked.
fn controllers_dir() -> Option<PathBuf> {
    let from = opendj_controllers::default_mappings_dir()?;
    let to = opendj_engine::paths::cache_base()?
        .join(opendj_engine::paths::app_folder_name())
        .join("tablet-controllers");
    let _ = std::fs::remove_dir_all(&to);
    std::fs::create_dir_all(&to).ok()?;
    for file in controller_files() {
        if let Err(e) = std::fs::copy(from.join(file), to.join(file)) {
            eprintln!("controller file {file}: {e}");
        }
    }
    Some(to)
}

/// The outputs for a device with `channels` channels: the master on 1-2,
/// and with four or more (a controller's own sound card) the headphones
/// on 3-4.
fn audio_setup(channels: u16) -> AudioSetup {
    let mut setup = AudioSetup::default();
    if channels >= 4 {
        setup.routes.push(Route {
            role: Role::Headphones,
            device: String::new(),
            pair: 1,
        });
    }
    setup
}

/// What the AUDIO pill says for `channels`.
fn outputs_text(channels: u16) -> String {
    if channels >= 4 {
        "Master 1-2 · Phones 3-4".into()
    } else {
        "Master 1-2 · no phones".into()
    }
}

/// Up to two letters for a cover tile: the artist's initials, else the
/// title's.
fn initials(track: &Track) -> String {
    let from = if track.artist.trim().is_empty() {
        &track.title
    } else {
        &track.artist
    };
    from.split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .flat_map(char::to_uppercase)
        .take(2)
        .collect()
}

/// The controls one deck is played with.
struct DeckControls {
    play: ControlHandle,
    cue: ControlHandle,
    cue_lit: ControlHandle,
    sync: ControlHandle,
    position: ControlHandle,
    bpm: ControlHandle,
    pfl: ControlHandle,
    /// A controller's LOAD button for this deck.
    load: ControlHandle,
}

impl DeckControls {
    fn new(reg: &ControlRegistry, deck: usize) -> Self {
        let g = deck_group(deck);
        let get = |item: &str| {
            reg.get(&g, item)
                .unwrap_or_else(|| panic!("the engine has no {g},{item}"))
        };
        Self {
            play: get("play"),
            cue: get("cue_default"),
            cue_lit: get("cue_indicator"),
            sync: get("sync_enabled"),
            position: get("playposition"),
            bpm: get("bpm"),
            pfl: get("pfl"),
            load: reg.free(&g, "LoadSelectedTrack"),
        }
    }
}

/// What the screen shows that isn't in a control.
struct State {
    tracks: Vec<Arc<Track>>,
    selected: Option<usize>,
    /// Each deck's overview, and what it was drawn from: the track (by
    /// address) and how much of its waveform there was.
    overviews: [(usize, usize, Image); DECKS],
    /// Each deck's cover, and the track (by address) it is for.
    covers: [(usize, Option<Image>); DECKS],
    status: Option<(String, Instant)>,
}

fn rows(tracks: &[Arc<Track>]) -> ModelRc<Row> {
    let rows: Vec<Row> = tracks
        .iter()
        .map(|t| Row {
            title: t.title.clone().into(),
            artist: t.artist.clone().into(),
            bpm: if t.bpm > 0.0 {
                format!("{:.0}", t.bpm).into()
            } else {
                SharedString::new()
            },
            key: t.key.clone().into(),
            time: clock(t.duration).into(),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Reads the folder's tracks off the UI thread, then lists them.
fn scan(window: slint::Weak<AppWindow>, state: Rc<RefCell<State>>, folder: PathBuf) {
    if let Some(w) = window.upgrade() {
        w.set_status("Reading your tracks…".into());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut tracks = opendj_engine::scan::scan(&folder, 0);
        tracks.sort_by(|a, b| {
            (a.artist.to_lowercase(), a.title.to_lowercase())
                .cmp(&(b.artist.to_lowercase(), b.title.to_lowercase()))
        });
        let _ = tx.send(tracks);
    });
    // Polled rather than sent across: the tracks stay on this thread.
    let timer = Rc::new(slint::Timer::default());
    let keep = timer.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            let Ok(tracks) = rx.try_recv() else {
                return;
            };
            keep.stop();
            let Some(w) = window.upgrade() else {
                return;
            };
            w.set_rows(rows(&tracks));
            let mut s = state.borrow_mut();
            s.selected = (!tracks.is_empty()).then_some(0);
            w.set_selected(s.selected.map_or(-1, |i| i as i32));
            w.set_status(if tracks.is_empty() {
                "No tracks yet: copy music into openDJ's folder in the Files app, then Rescan"
                    .into()
            } else {
                SharedString::new()
            });
            s.tracks = tracks;
        },
    );
}

fn main() -> Result<(), slint::PlatformError> {
    session::start();
    let mut channels = session::channels();
    let registry = ControlRegistry::new();
    let engine = Rc::new(AudioEngine::start(registry.clone(), &audio_setup(channels)));

    // The browser's controls, before any controller mapping looks for them.
    let decks: [DeckControls; DECKS] = std::array::from_fn(|d| DeckControls::new(&registry, d));
    let decks = Rc::new(decks);
    let browse = [
        (registry.free("[Library]", "MoveVertical"), 1.0),
        (registry.free("[Library]", "ScrollVertical"), 1.0),
        (registry.free("[Library]", "MoveDown"), 1.0),
        (registry.free("[Library]", "MoveUp"), -1.0),
        (registry.free("[Playlist]", "SelectTrackKnob"), 1.0),
        (registry.free("[Playlist]", "SelectNextTrack"), 1.0),
        (registry.free("[Playlist]", "SelectPrevTrack"), -1.0),
    ];
    // There is only a track list: LOAD buttons load rather than open a
    // folder (the Xone:K2 mapping asks which pane has focus; 3 is tracks).
    registry.free("[Library]", "focused_widget").set(3.0);
    let crossfader = registry
        .get("[Master]", "crossfader")
        .expect("the engine has a crossfader");

    let controllers = match controllers_dir() {
        Some(dir) => Some(opendj_controllers::Controllers::start(
            registry.clone(),
            dir,
            Default::default(),
            Default::default(),
        )),
        None => {
            eprintln!("controller mappings not found; controllers are off");
            None
        }
    };

    let window = AppWindow::new()?;
    let folder = music_folder();
    window.set_folder(format!("Tracks in {}", folder.display()).into());
    window.set_outputs(outputs_text(channels).into());
    window.set_phones(channels >= 4);

    let state = Rc::new(RefCell::new(State {
        tracks: Vec::new(),
        selected: None,
        overviews: Default::default(),
        covers: Default::default(),
        status: None,
    }));

    let load = {
        let (engine, state, window) = (engine.clone(), state.clone(), window.as_weak());
        move |deck: usize| {
            let s = state.borrow();
            let Some(track) = s.selected.and_then(|i| s.tracks.get(i)) else {
                return;
            };
            if !engine.load_track(deck, track.clone()) {
                if let Some(w) = window.upgrade() {
                    w.set_status("That deck is playing: pause it first".into());
                }
            }
        }
    };

    {
        let decks = decks.clone();
        window.on_play(move |d| {
            let c = &decks[d as usize].play;
            c.set(if c.get_bool() { 0.0 } else { 1.0 });
        });
    }
    {
        let decks = decks.clone();
        window.on_cue(move |d, down| decks[d as usize].cue.set_bool(down));
    }
    {
        let decks = decks.clone();
        window.on_sync(move |d| decks[d as usize].sync.toggle());
    }
    {
        let decks = decks.clone();
        window.on_pfl(move |d| decks[d as usize].pfl.toggle());
    }
    {
        let decks = decks.clone();
        window.on_seek(move |d, at| decks[d as usize].position.set(at.clamp(0.0, 1.0) as f64));
    }
    {
        let crossfader = crossfader.clone();
        window.on_crossfade(move |v| crossfader.set(v as f64));
    }
    {
        let (state, window_weak) = (state.clone(), window.as_weak());
        window.on_select(move |i| {
            state.borrow_mut().selected = Some(i as usize);
            if let Some(w) = window_weak.upgrade() {
                w.set_selected(i);
            }
        });
    }
    {
        let load = load.clone();
        window.on_load(move |d| load(d as usize));
    }
    {
        let (window_weak, state, folder) = (window.as_weak(), state.clone(), folder.clone());
        window.on_rescan(move || scan(window_weak.clone(), state.clone(), folder.clone()));
    }
    scan(window.as_weak(), state.clone(), folder);

    // Every frame: the controller's browsing, the decks, the waveforms.
    let frame = slint::Timer::default();
    {
        let window_weak = window.as_weak();
        let visual = engine.visual_positions();
        let loaded = engine.loaded_tracks();
        let mut frames = 0u32;
        frame.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(16),
            move || {
                let Some(w) = window_weak.upgrade() else {
                    return;
                };
                frames = frames.wrapping_add(1);
                let now = Instant::now();

                // The browse knob and the LOAD buttons.
                let steps: f64 = browse.iter().map(|(c, sign)| take(c) * sign).sum();
                if steps != 0.0 {
                    let mut s = state.borrow_mut();
                    if !s.tracks.is_empty() {
                        let last = s.tracks.len() as i64 - 1;
                        let at = s.selected.map_or(0, |i| i as i64) + steps.round() as i64;
                        let at = at.clamp(0, last) as usize;
                        s.selected = Some(at);
                        w.set_selected(at as i32);
                        w.invoke_show_row(at as i32);
                    }
                }
                for (d, c) in decks.iter().enumerate() {
                    if take(&c.load) != 0.0 {
                        load(d);
                    }
                }

                // About twice a second: alerts, the controllers, and the
                // audio route (a controller's sound card plugged in or out).
                if frames.is_multiple_of(30) {
                    let mut s = state.borrow_mut();
                    if let Some(alert) = engine.take_alerts().pop() {
                        w.set_status(alert.clone().into());
                        s.status = Some((alert, now));
                    } else if s
                        .status
                        .as_ref()
                        .is_some_and(|(_, at)| now.duration_since(*at) > ALERT_TIME)
                    {
                        s.status = None;
                        w.set_status(SharedString::new());
                    }
                    let names: Vec<String> = controllers
                        .as_ref()
                        .map(|c| c.connected().into_iter().map(|c| c.port).collect())
                        .unwrap_or_default();
                    w.set_controllers(names.join(", ").into());
                    let now_channels = session::channels();
                    if now_channels != channels {
                        channels = now_channels;
                        engine.set_outputs(&audio_setup(channels));
                        w.set_outputs(outputs_text(channels).into());
                        w.set_phones(channels >= 4);
                    }
                }

                // The decks, drawn at the window's real pixel size.
                let scale = w.window().scale_factor();
                let width = (w.window().size().width).clamp(256, 2800);
                let wave_height = (100.0 * scale) as u32;
                let overview_height = (44.0 * scale) as u32;
                let tracks = loaded.read().map(|t| t.clone()).unwrap_or_default();
                let mut views = Vec::with_capacity(DECKS);
                for (d, c) in decks.iter().enumerate() {
                    let Some(track) = &tracks[d] else {
                        views.push(DeckView {
                            pfl: c.pfl.get_bool(),
                            ..Default::default()
                        });
                        continue;
                    };
                    let (seconds, _) = visual.position(d, now);
                    let mut s = state.borrow_mut();
                    let id = Arc::as_ptr(track) as usize;
                    let finest = track.waveform.levels.first().map_or(0, Vec::len);
                    let (drawn_id, drawn_len, image) = &mut s.overviews[d];
                    if *drawn_id != id || *drawn_len != finest {
                        let half = (width / 2).max(1);
                        *image = Image::from_rgba8(wave::overview(track, half, overview_height));
                        (*drawn_id, *drawn_len) = (id, finest);
                    }
                    let overview = image.clone();
                    let (cover_id, cover) = &mut s.covers[d];
                    if *cover_id != id {
                        *cover = track.cover.as_ref().map(|c| {
                            let mut px = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(
                                c.width, c.height,
                            );
                            px.make_mut_bytes().copy_from_slice(&c.rgba);
                            Image::from_rgba8(px)
                        });
                        *cover_id = id;
                    }
                    let bpm = c.bpm.get();
                    views.push(DeckView {
                        loaded: true,
                        title: track.title.clone().into(),
                        artist: track.artist.clone().into(),
                        bpm: if bpm > 0.0 {
                            format!("{bpm:.2}").into()
                        } else {
                            SharedString::new()
                        },
                        key: track.key.clone().into(),
                        elapsed: clock(seconds).into(),
                        remaining: format!("-{}", clock(track.duration - seconds)).into(),
                        playing: c.play.get_bool(),
                        cue_lit: c.cue_lit.get_bool(),
                        sync: c.sync.get_bool(),
                        pfl: c.pfl.get_bool(),
                        has_cover: cover.is_some(),
                        cover: cover.clone().unwrap_or_default(),
                        initials: initials(track).into(),
                        overview,
                        playhead: if track.duration > 0.0 {
                            (seconds / track.duration).clamp(0.0, 1.0) as f32
                        } else {
                            0.0
                        },
                        wave: Image::from_rgba8(wave::detail(
                            track,
                            seconds,
                            SPAN,
                            width,
                            wave_height,
                        )),
                    });
                }
                w.set_decks(ModelRc::new(VecModel::from(views)));
                w.set_crossfader(crossfader.get() as f32);
            },
        );
    }

    window.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn clock_reads_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(61.4), "1:01");
        assert_eq!(clock(-3.0), "0:00");
    }

    #[test]
    fn music_folder_follows_the_environment() {
        // Only checks the override; the defaults depend on the machine.
        let dir = Path::new("/tmp/opendj-tablet-music");
        std::env::set_var("OPENDJ_MUSIC", dir);
        assert_eq!(music_folder(), dir);
        std::env::remove_var("OPENDJ_MUSIC");
    }

    #[test]
    fn initials_come_from_the_artist() {
        let mut t = opendj_engine::scan::read_track(0, Path::new("x.mp3"));
        t.artist = "coastline FM".into();
        assert_eq!(initials(&t), "CF");
        t.artist = String::new();
        t.title = "Midnight Circuit (Dub)".into();
        assert_eq!(initials(&t), "MC");
    }

    #[test]
    fn headphones_only_with_four_channels() {
        assert_eq!(audio_setup(2).routes.len(), 1);
        let four = audio_setup(4);
        assert_eq!(four.routes.len(), 2);
        assert_eq!(
            (four.routes[1].role, four.routes[1].pair),
            (Role::Headphones, 1)
        );
    }

    #[test]
    fn only_the_supported_controllers() {
        let files: Vec<_> = controller_files().collect();
        assert!(files.contains(&"Pioneer-DDJ-FLX4.midi.xml"));
        assert!(files.contains(&"Allen and Heath Xone K2 - 2 Decks.opendj-mapping"));
        let res = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../res/controllers");
        for f in files {
            assert!(res.join(f).is_file(), "{f} is in res/controllers");
        }
    }
}
