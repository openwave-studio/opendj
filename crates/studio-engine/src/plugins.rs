//! Plug-ins on tracks: each one runs in an `opendj-plugin-host` helper
//! process (as in openDJ), started from the sound the set saved it with.
//! The audio thread gets each helper's [`Connection`] over the command
//! queue; this side keeps the [`Helper`]s, for their windows and health.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use opendj_plugin_host::{Descriptor, Helper, Plugin};

use crate::clip::{PluginFormat, PluginRef, Track};
use crate::control::Controls;
use crate::engine::{Command, PluginSlot, MAX_BLOCK};

/// How long scanning one plug-in file may take.
const SCAN_TIMEOUT: Duration = Duration::from_secs(20);

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
    track: usize,
    device: usize,
    name: String,
    state: State,
    helper: Option<Helper>,
}

/// The plug-ins of a set.
#[derive(Clone, Default)]
pub struct Plugins {
    entries: Arc<Mutex<Vec<Entry>>>,
}

/// The helper binary, next to the studio's own.
pub fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = format!("opendj-plugin-host{}", std::env::consts::EXE_SUFFIX);
    let path = exe.parent()?.join(name);
    path.is_file().then_some(path)
}

impl Plugins {
    /// Starts every plug-in in `tracks`, each on its own thread; they reach
    /// the audio thread as they come up.
    pub fn start(tracks: &[Track], rate: u32, controls: Arc<Controls>) -> Plugins {
        let plugins = Plugins::default();
        let helper = helper_path();
        let mut jobs = Vec::new();
        {
            let mut entries = plugins.entries.lock().expect("fresh lock");
            for (t, track) in tracks.iter().enumerate() {
                for (d, device) in track.devices.iter().enumerate() {
                    let Some(p) = &device.plugin else { continue };
                    let state = if helper.is_none() {
                        State::Failed(
                            "Build the plug-in helper: cargo build --release -p opendj-plugin-host"
                                .into(),
                        )
                    } else {
                        State::Loading
                    };
                    entries.push(Entry {
                        track: t,
                        device: d,
                        name: p.name.clone(),
                        state,
                        helper: None,
                    });
                    jobs.push((t, d, p.clone(), device.on));
                }
            }
        }
        let Some(helper) = helper else {
            return plugins;
        };
        for (t, d, p, on) in jobs {
            let (plugins2, helper, controls) = (plugins.clone(), helper.clone(), controls.clone());
            let _ = std::thread::Builder::new()
                .name(format!("plugin {}", p.name))
                .spawn(move || {
                    let state = match run(&helper, &p, rate) {
                        Ok((conn, h)) => {
                            controls.send(Command::Attach {
                                track: t,
                                slot: Box::new(PluginSlot::new(d, p.instrument, on, conn)),
                            });
                            plugins2.set(t, d, State::Running, Some(h));
                            controls.bump();
                            return;
                        }
                        Err(e) => e,
                    };
                    eprintln!("{}: {}", p.name, state.text());
                    plugins2.set(t, d, state, None);
                    controls.bump();
                });
        }
        plugins
    }

    fn set(&self, track: usize, device: usize, state: State, helper: Option<Helper>) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(e) = entries
                .iter_mut()
                .find(|e| e.track == track && e.device == device)
            {
                e.state = state;
                e.helper = helper;
            }
        }
    }

    /// The state of a device's plug-in; None when it has none.
    pub fn state(&self, track: usize, device: usize) -> Option<State> {
        let mut entries = self.entries.lock().ok()?;
        let e = entries
            .iter_mut()
            .find(|e| e.track == track && e.device == device)?;
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
        let Ok(entries) = self.entries.lock() else {
            return false;
        };
        match entries
            .iter()
            .find(|e| e.track == track && e.device == device)
            .and_then(|e| e.helper.as_ref())
        {
            Some(h) => {
                h.set_editor(!h.editor_asked());
                true
            }
            None => false,
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
}
