//! Plug-ins on tracks: each one runs in an `opendj-plugin-host` helper
//! process (as in openDJ), started from the sound the set saved it with.
//! The audio thread gets each helper's [`Connection`] over the command
//! queue; this side keeps the [`Helper`]s, for their windows and health.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use opendj_plugin_host::{Descriptor, Helper, Plugin};

use crate::clip::{DeviceInfo, PluginFormat, PluginRef, Track};
use crate::control::Controls;
use crate::engine::{Command, PluginSlot, MAX_BLOCK};

/// How long scanning one plug-in file may take.
const SCAN_TIMEOUT: Duration = Duration::from_secs(20);

/// Where a dragged device came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum From {
    /// The browser: a place in `devices::catalog()`.
    Browser(usize),
    /// A track's chain: the track and the device's place.
    Chain(usize, usize),
}

/// A device let go over a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dropped {
    pub from: From,
    pub to_track: usize,
    /// A place in the chain (before the device there); None for its end.
    pub place: Option<usize>,
    /// Option held: a device from a chain is copied rather than moved.
    pub copy: bool,
}

/// What became of a device's plug-in, as the device row shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Loading,
    Running,
    /// Not on this computer (or not found where plug-ins live).
    Missing(String),
    /// Found but it didn't start, or it stopped.
    Failed(String),
}

impl State {
    pub fn text(&self) -> String {
        match self {
            State::Loading => "Loading…".into(),
            State::Running => String::new(),
            State::Missing(why) | State::Failed(why) => why.clone(),
        }
    }
}

struct Entry {
    /// Stays the same while the device moves about.
    id: u64,
    track: usize,
    device: usize,
    /// Inside a rack (at `device`): the chain and the place in it.
    inner: Option<(usize, usize)>,
    name: String,
    state: State,
    helper: Option<Helper>,
}

/// The plug-ins a device holds: its own, or those in a rack's chains (by
/// chain and place).
fn plugins_of(d: &DeviceInfo) -> Vec<(Option<(usize, usize)>, PluginRef)> {
    if let Some(p) = &d.plugin {
        return vec![(None, p.clone())];
    }
    let Some(crate::devices::Settings::Rack(r)) = &d.builtin else {
        return Vec::new();
    };
    let mut list = Vec::new();
    for (b, chain) in r.branches.iter().enumerate() {
        for (i, x) in chain.devices.iter().enumerate() {
            if let Some(p) = &x.plugin {
                list.push((Some((b, i)), p.clone()));
            }
        }
    }
    list
}

/// The plug-ins of a set.
#[derive(Clone)]
pub struct Plugins {
    entries: Arc<Mutex<Book>>,
    controls: Arc<Controls>,
    helper: Option<PathBuf>,
    rate: u32,
}

#[derive(Default)]
struct Book {
    entries: Vec<Entry>,
    next_id: u64,
}

impl Book {
    fn add(
        &mut self,
        track: usize,
        device: usize,
        inner: Option<(usize, usize)>,
        name: &str,
        state: State,
    ) -> u64 {
        self.next_id += 1;
        self.entries.push(Entry {
            id: self.next_id,
            track,
            device,
            inner,
            name: name.into(),
            state,
            helper: None,
        });
        self.next_id
    }

    fn shift(&mut self, track: usize, from: usize, by: isize) {
        for e in self
            .entries
            .iter_mut()
            .filter(|e| e.track == track && e.device >= from)
        {
            e.device = e.device.saturating_add_signed(by);
        }
    }
}

/// The helper binary: next to the studio's own, else (run from source) one
/// built with another profile in the same target folder.
pub fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = format!("opendj-plugin-host{}", std::env::consts::EXE_SUFFIX);
    let dir = exe.parent()?;
    let path = dir.join(&name);
    if path.is_file() {
        return Some(path);
    }
    let target = dir.parent()?;
    ["release", "fast", "debug"]
        .iter()
        .map(|profile| target.join(profile).join(&name))
        .find(|p| p.is_file())
}

impl Plugins {
    /// Starts every plug-in in `tracks`, each on its own thread; they reach
    /// the audio thread as they come up.
    pub fn start(tracks: &[Track], rate: u32, controls: Arc<Controls>) -> Plugins {
        let plugins = Plugins {
            entries: Arc::default(),
            controls,
            helper: helper_path(),
            rate,
        };
        let mut jobs = Vec::new();
        {
            let mut book = plugins.entries.lock().expect("fresh lock");
            for (t, track) in tracks.iter().enumerate() {
                for (d, device) in track.devices.iter().enumerate() {
                    for (inner, p) in plugins_of(device) {
                        let id = book.add(t, d, inner, &p.name, plugins.first_state());
                        jobs.push((id, p));
                    }
                }
            }
        }
        for (id, p) in jobs {
            plugins.launch(id, p);
        }
        plugins
    }

    /// Puts a dropped device in its track's chain. An effect goes where it was
    /// let go, but never before the track's instrument; an instrument takes
    /// the place of the one the track has (a track plays one). Returns whether
    /// an External Instrument came in, or None when nothing changed.
    pub fn drop_device(&self, d: Dropped) -> Option<bool> {
        let c = &*self.controls;
        let (device, moving) = match d.from {
            From::Browser(i) => {
                let (name, s) = crate::devices::catalog().into_iter().nth(i)?;
                (
                    DeviceInfo {
                        name: name.into(),
                        kind: "Built-in".into(),
                        on: true,
                        plugin: None,
                        builtin: Some(s),
                    },
                    None,
                )
            }
            From::Chain(t, at) => (c.devices(t).get(at)?.clone(), (!d.copy).then_some((t, at))),
        };
        self.place(d, device, moving)
    }

    /// Puts a new device (a Simpler made from a sample, say) in a track's
    /// chain, as a drop from the browser would.
    pub fn drop_new(
        &self,
        to_track: usize,
        place: Option<usize>,
        device: DeviceInfo,
    ) -> Option<bool> {
        let d = Dropped {
            from: From::Browser(0),
            to_track,
            place,
            copy: false,
        };
        self.place(d, device, None)
    }

    fn place(
        &self,
        d: Dropped,
        device: DeviceInfo,
        moving: Option<(usize, usize)>,
    ) -> Option<bool> {
        let (c, p) = (&*self.controls, self);
        let track = c.tracks.get(d.to_track)?;
        let instrument = crate::control::is_instrument(&device);
        let midi = crate::control::is_midi_effect(&device);
        // An audio track plays recordings: no instrument or MIDI effect
        // there.
        if (instrument || midi) && track.audio() {
            return None;
        }
        let external = matches!(device.builtin, Some(crate::devices::Settings::External(_)));
        // The chain as it will be once the device has left its place.
        let mut chain = c.devices(d.to_track);
        let mut place = d.place.unwrap_or(chain.len());
        if let Some((t, at)) = moving {
            if t == d.to_track {
                chain.remove(at);
                if place > at {
                    place -= 1;
                }
            }
        }
        if instrument {
            // Takes the place of the track's instruments.
            let first = chain.iter().position(crate::control::is_instrument);
            let olds: Vec<usize> = chain
                .iter()
                .enumerate()
                .filter(|(_, x)| crate::control::is_instrument(x))
                .map(|(i, _)| i)
                .collect();
            if moving.is_some_and(|(t, _)| t == d.to_track) {
                // An instrument moved along its own chain stays where it is.
                return None;
            }
            for &i in olds.iter().rev() {
                p.remove(d.to_track, i);
            }
            place = first.unwrap_or(crate::control::midi_effects(&chain));
        } else if midi {
            // Among the MIDI effects, before the instrument.
            place = place.min(crate::control::midi_effects(&chain));
        } else {
            place = place.max(crate::control::first_effect_place(&chain));
        }
        match moving {
            Some((t, at)) => {
                if !p.move_device(t, at, d.to_track, place) {
                    return None;
                }
            }
            None => {
                p.insert(d.to_track, place, device)?;
            }
        }
        Some(external)
    }

    fn first_state(&self) -> State {
        if self.helper.is_none() {
            State::Failed(
                "Build the plug-in helper: cargo build --profile fast -p opendj-plugin-host".into(),
            )
        } else {
            State::Loading
        }
    }

    /// Starts entry `id`'s plug-in on its own thread. It joins the chain
    /// wherever the device is by the time it's up, or is let go if the
    /// device was removed meanwhile.
    fn launch(&self, id: u64, p: PluginRef) {
        let Some(helper) = self.helper.clone() else {
            return;
        };
        let (plugins, rate) = (self.clone(), self.rate);
        let _ = std::thread::Builder::new()
            .name(format!("plugin {}", p.name))
            .spawn(move || {
                let result = run(&helper, &p, rate);
                let Ok(mut book) = plugins.entries.lock() else {
                    return;
                };
                let Some(e) = book.entries.iter_mut().find(|e| e.id == id) else {
                    // Removed while it started: its helper stops as it drops.
                    return;
                };
                match result {
                    Ok((conn, h)) => {
                        let on = plugins.controls.device_on(e.track, e.device);
                        let slot = Box::new(PluginSlot::new(e.device, p.instrument, on, conn));
                        plugins.controls.send(match e.inner {
                            Some((branch, inner)) => Command::AttachInner {
                                track: e.track,
                                device: e.device,
                                branch,
                                inner,
                                slot,
                            },
                            None => Command::Attach {
                                track: e.track,
                                slot,
                            },
                        });
                        e.state = State::Running;
                        e.helper = Some(h);
                    }
                    Err(state) => {
                        eprintln!("{}: {}", p.name, state.text());
                        e.state = state;
                    }
                }
                drop(book);
                plugins.controls.bump();
            });
    }

    /// Swaps the device at `at` for `device` in the same place (a rack
    /// whose chains changed). The track fades around it, as for any edit.
    pub fn replace(&self, track: usize, at: usize, device: DeviceInfo) -> bool {
        self.remove(track, at).is_some() && self.insert(track, at, device).is_some()
    }

    /// Puts a device in `track`'s chain at `at` (clamped to its end): a
    /// built-in one plays at once, a plug-in starts a helper with the sound
    /// it carries. Returns where it went.
    pub fn insert(&self, track: usize, at: usize, device: DeviceInfo) -> Option<usize> {
        let mut book = self.entries.lock().ok()?;
        let slot = device.builtin.as_ref().map(|b| {
            Box::new(PluginSlot::device(
                at,
                device.on,
                crate::devices::Device::new(b, self.rate as f64),
            ))
        });
        let plugins = plugins_of(&device);
        let at = self
            .controls
            .edit_chains(|c| c.insert(track, at, device, slot))
            .flatten()?;
        book.shift(track, at, 1);
        let jobs: Vec<(u64, PluginRef)> = plugins
            .into_iter()
            .map(|(inner, p)| (book.add(track, at, inner, &p.name, self.first_state()), p))
            .collect();
        drop(book);
        for (id, p) in jobs {
            self.launch(id, p);
        }
        Some(at)
    }

    /// Takes the device at `at` out of `track`'s chain; a plug-in's helper
    /// stops.
    pub fn remove(&self, track: usize, at: usize) -> Option<DeviceInfo> {
        let mut book = self.entries.lock().ok()?;
        let d = self
            .controls
            .edit_chains(|c| c.remove(track, at))
            .flatten()?;
        let gone: Vec<Entry> = {
            let (gone, keep) = std::mem::take(&mut book.entries)
                .into_iter()
                .partition(|e| e.track == track && e.device == at);
            book.entries = keep;
            gone
        };
        book.shift(track, at + 1, -1);
        drop(book);
        drop(gone);
        Some(d)
    }

    /// Moves a device along its chain or to another track's (`to` counts
    /// places once it has left); its plug-in goes with it.
    pub fn move_device(&self, track: usize, from: usize, to_track: usize, to: usize) -> bool {
        let Ok(mut book) = self.entries.lock() else {
            return false;
        };
        let to = to.min(
            self.controls
                .devices(to_track)
                .len()
                .saturating_sub(usize::from(to_track == track)),
        );
        if !self
            .controls
            .edit_chains(|c| c.move_device(track, from, to_track, to))
            .unwrap_or(false)
        {
            return false;
        }
        // A rack's plug-ins go with it, as well as a device's own.
        let moving: Vec<usize> = book
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.track == track && e.device == from)
            .map(|(i, _)| i)
            .collect();
        for &i in &moving {
            book.entries[i].device = usize::MAX;
        }
        book.shift(track, from + 1, -1);
        book.shift(to_track, to, 1);
        for &i in &moving {
            book.entries[i].track = to_track;
            book.entries[i].device = to;
        }
        true
    }

    /// The state of a device's plug-in; None when it has none.
    pub fn state(&self, track: usize, device: usize) -> Option<State> {
        self.state_of(track, device, None)
    }

    /// The state of a plug-in inside the rack at `device`: chain `b`,
    /// place `d`.
    pub fn inner_state(&self, track: usize, device: usize, b: usize, d: usize) -> Option<State> {
        self.state_of(track, device, Some((b, d)))
    }

    fn state_of(
        &self,
        track: usize,
        device: usize,
        inner: Option<(usize, usize)>,
    ) -> Option<State> {
        let mut book = self.entries.lock().ok()?;
        let e = book
            .entries
            .iter_mut()
            .find(|e| e.track == track && e.device == device && e.inner == inner)?;
        if let Some(h) = &e.helper {
            if h.is_gone() {
                e.state = State::Failed(format!("{} stopped working", e.name));
            }
        }
        Some(e.state.clone())
    }

    /// Opens (or closes) a device's plug-in window. False when there is no
    /// running plug-in to show.
    pub fn toggle_window(&self, track: usize, device: usize) -> bool {
        self.toggle(track, device, None)
    }

    /// As [`Plugins::toggle_window`], for a plug-in inside a rack.
    pub fn toggle_inner_window(&self, track: usize, device: usize, b: usize, d: usize) -> bool {
        self.toggle(track, device, Some((b, d)))
    }

    fn toggle(&self, track: usize, device: usize, inner: Option<(usize, usize)>) -> bool {
        let Ok(book) = self.entries.lock() else {
            return false;
        };
        match book
            .entries
            .iter()
            .find(|e| e.track == track && e.device == device && e.inner == inner)
            .and_then(|e| e.helper.as_ref())
        {
            Some(h) => {
                h.set_editor(!h.editor_asked());
                true
            }
            None => false,
        }
    }

    /// Puts the sound each running plug-in of the device at `device` has
    /// now (its own, or those inside a rack) into `info`, so a copy or a
    /// rebuilt rack starts where they are rather than where the set
    /// left them.
    pub fn keep_sounds(&self, track: usize, device: usize, info: &mut DeviceInfo) {
        let Ok(book) = self.entries.lock() else {
            return;
        };
        for e in book
            .entries
            .iter()
            .filter(|e| e.track == track && e.device == device)
        {
            let Some(h) = &e.helper else { continue };
            let target = match (e.inner, &mut info.builtin) {
                (None, _) => info.plugin.as_mut(),
                (Some((b, d)), Some(crate::devices::Settings::Rack(r))) => r
                    .branches
                    .get_mut(b)
                    .and_then(|c| c.devices.get_mut(d))
                    .and_then(|x| x.plugin.as_mut()),
                _ => None,
            };
            if let (Some(p), Ok(data)) = (target, h.save_state()) {
                p.state = Some(Arc::new(data));
            }
        }
    }
}

/// Finds the plug-in and starts its helper with the saved sound.
fn run(
    helper: &Path,
    p: &PluginRef,
    rate: u32,
) -> Result<(opendj_plugin_host::Connection, Helper), State> {
    let plugin = resolve(helper, p)?;
    let opts = opendj_plugin_host::Options {
        helper: helper.to_path_buf(),
        plugin,
        sample_rate: rate,
        max_frames: MAX_BLOCK,
        title: Some(p.name.clone()),
        state: p.state.as_deref().cloned(),
        picks: Vec::new(),
    };
    let started = opendj_plugin_host::spawn(&opts);
    // A saved sound the plug-in can't take: start it fresh rather than
    // not at all.
    let started = match started {
        Err(e) if opts.state.is_some() => {
            eprintln!(
                "{}: the saved sound didn't load ({e}); starting fresh",
                p.name
            );
            opendj_plugin_host::spawn(&opendj_plugin_host::Options {
                state: None,
                ..opts.clone()
            })
        }
        other => other,
    };
    started.map_err(|e| State::Failed(format!("Didn't start: {e}")))
}

/// Where a plug-in lives on this computer.
fn resolve(helper: &Path, p: &PluginRef) -> Result<Plugin, State> {
    match &p.format {
        PluginFormat::Au(id) => {
            if cfg!(target_os = "macos") {
                Ok(Plugin::Au { id: id.clone() })
            } else {
                Err(State::Missing("Audio Units run on macOS only".into()))
            }
        }
        PluginFormat::Vst3(id) => {
            find(helper, &p.name, "vst3", Some(id)).ok_or_else(|| not_installed(&p.name))
        }
        PluginFormat::Clap { file, id } | PluginFormat::Vst3File { file, id } => {
            if !file.exists() {
                return Err(not_installed(&p.name));
            }
            let (file, id) = (file.clone(), id.clone());
            Ok(if matches!(p.format, PluginFormat::Clap { .. }) {
                Plugin::Clap { file, id }
            } else {
                Plugin::Vst3 { file, id }
            })
        }
        PluginFormat::Named => find(helper, &p.name, "vst3", None)
            .or_else(|| find(helper, &p.name, "clap", None))
            .or_else(|| find_au(helper, &p.name))
            .ok_or_else(|| not_installed(&p.name)),
    }
}

fn not_installed(name: &str) -> State {
    State::Missing(format!("{name} isn't installed"))
}

/// The folders plug-ins of a format (`vst3` or `clap`) are installed in.
fn folders(ext: &str) -> Vec<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default();
    let upper = ext.to_uppercase();
    if cfg!(target_os = "macos") {
        vec![
            home.join(format!("Library/Audio/Plug-Ins/{upper}")),
            PathBuf::from(format!("/Library/Audio/Plug-Ins/{upper}")),
        ]
    } else if cfg!(windows) {
        let common = std::env::var_os("CommonProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Common Files"));
        vec![common.join(upper)]
    } else {
        vec![
            home.join(format!(".{ext}")),
            PathBuf::from(format!("/usr/lib/{ext}")),
            PathBuf::from(format!("/usr/local/lib/{ext}")),
        ]
    }
}

/// Plug-in files of a format, two folders deep (makers keep theirs in a
/// folder of their own).
fn files(ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn go(dir: &Path, depth: usize, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
                out.push(p);
            } else if depth > 0 && p.is_dir() {
                go(&p, depth - 1, ext, out);
            }
        }
    }
    for f in folders(ext) {
        go(&f, 2, ext, &mut out);
    }
    out
}

/// Letters and digits only, lower case: "Stage-73 V2" and "stage73v2"
/// match.
fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A VST3 or CLAP plug-in by name (and, for VST3, class id): files named
/// like it first, then every file.
fn find(helper: &Path, name: &str, ext: &str, id: Option<&str>) -> Option<Plugin> {
    let all = files(ext);
    let want = squash(name);
    let (named, rest): (Vec<_>, Vec<_>) = all.into_iter().partition(|f| {
        f.file_stem()
            .is_some_and(|s| squash(&s.to_string_lossy()) == want)
    });
    let pick = |found: &[Descriptor]| -> Option<String> {
        found
            .iter()
            .find(|d| id.is_some_and(|id| d.id.eq_ignore_ascii_case(id)))
            .or_else(|| found.iter().find(|d| squash(&d.name) == want))
            .map(|d| d.id.clone())
    };
    let make = |file: PathBuf, id: String| match ext {
        "clap" => Plugin::Clap { file, id },
        _ => Plugin::Vst3 { file, id },
    };
    for f in &named {
        let found = describe(helper, f, ext);
        if let Some(id) = pick(&found).or_else(|| found.first().map(|d| d.id.clone())) {
            return Some(make(f.clone(), id));
        }
    }
    // Only VST3 bundles describe themselves without running code; others
    // aren't opened by guesswork.
    if ext == "vst3" {
        for f in rest {
            if let Some(found) = opendj_plugin_host::descriptor::scan_moduleinfo(&f) {
                if let Some(id) = pick(&found) {
                    return Some(make(f, id));
                }
            }
        }
    }
    None
}

fn describe(helper: &Path, file: &Path, ext: &str) -> Vec<Descriptor> {
    if ext == "vst3" {
        if let Some(d) = opendj_plugin_host::descriptor::scan_moduleinfo(file) {
            return d;
        }
    }
    opendj_plugin_host::scan(helper, file, SCAN_TIMEOUT).unwrap_or_default()
}

/// An Audio Unit by name (macOS), from one scan of the registry.
fn find_au(helper: &Path, name: &str) -> Option<Plugin> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    static UNITS: OnceLock<Vec<Descriptor>> = OnceLock::new();
    let units = UNITS.get_or_init(|| {
        opendj_plugin_host::scan_audio_units(helper, SCAN_TIMEOUT).unwrap_or_default()
    });
    let want = squash(name);
    units
        .iter()
        .find(|d| squash(&d.name) == want)
        .map(|d| Plugin::Au { id: d.id.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_loosely() {
        assert_eq!(squash("Stage-73 V2"), squash("Stage73 V2"));
        assert_eq!(squash("TONE3000"), "tone3000");
    }

    fn names(p: &Plugins, track: usize) -> Vec<String> {
        p.controls
            .devices(track)
            .into_iter()
            .map(|d| d.name)
            .collect()
    }

    fn catalog_place(name: &str) -> usize {
        crate::devices::catalog()
            .iter()
            .position(|(n, _)| *n == name)
            .expect("in the catalog")
    }

    #[test]
    fn dropped_devices_keep_to_their_zones() {
        use crate::clip::{Sound, SCENES};
        let device = |name: &str| {
            let (_, s) = crate::devices::catalog()
                .into_iter()
                .nth(catalog_place(name))
                .unwrap();
            DeviceInfo {
                name: name.into(),
                kind: "Built-in".into(),
                on: true,
                plugin: None,
                builtin: Some(s),
            }
        };
        let mut synth = Track::new("Synth", Sound::Lead, vec![None; SCENES]);
        synth.devices = vec![device("External Instrument"), device("Utility")];
        let set = vec![synth, Track::new("Mic", Sound::Audio, vec![None; SCENES])];
        let (tx, mut rx) = rtrb::RingBuffer::new(256);
        let controls = Arc::new(Controls::new(
            &set,
            &[0, 0],
            tx,
            Arc::new(crate::engine::Monitor::new()),
        ));
        let p = Plugins::start(&set, 48_000, controls);
        let drop = |from, to_track, place: Option<usize>, copy| {
            p.drop_device(Dropped {
                from,
                to_track,
                place,
                copy,
            })
        };
        // An effect let go before the instrument lands after it.
        let delay = From::Browser(catalog_place("Delay"));
        assert_eq!(drop(delay, 0, Some(0), false), Some(false));
        assert_eq!(names(&p, 0), ["External Instrument", "Delay", "Utility"]);
        // Utility moves before Delay.
        assert_eq!(drop(From::Chain(0, 2), 0, Some(1), false), Some(false));
        assert_eq!(names(&p, 0), ["External Instrument", "Utility", "Delay"]);
        // An instrument takes the place of the track's.
        let external = From::Browser(catalog_place("External Instrument"));
        assert_eq!(drop(external, 0, None, false), Some(true));
        assert_eq!(names(&p, 0), ["External Instrument", "Utility", "Delay"]);
        // Option-drag copies Delay to the other track; a move takes Utility.
        assert!(drop(From::Chain(0, 2), 1, None, true).is_some());
        assert!(drop(From::Chain(0, 1), 1, Some(0), false).is_some());
        assert_eq!(names(&p, 0), ["External Instrument", "Delay"]);
        assert_eq!(names(&p, 1), ["Utility", "Delay"]);
        // An audio track takes no instrument.
        assert_eq!(drop(external, 1, None, false), None);
        // Every edit reached the audio thread's queue.
        let mut sent = 0;
        while rx.pop().is_ok() {
            sent += 1;
        }
        assert!(sent >= 6, "{sent} commands");
    }
}
