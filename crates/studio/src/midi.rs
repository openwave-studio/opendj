//! Keyboards and external synths: each track's MIDI From (an input, or all
//! of them, and a channel) and MIDI To (an output and a channel), as in
//! Live's I/O section.
//!
//! Every MIDI input but Push (which plays the selected track itself) is
//! opened, and new ones are picked up as they are plugged in. A message
//! goes to each track that takes that input and channel and is listening:
//! Monitor on In, or on Auto when the track is armed (or, with no track
//! armed, when it is the selected one, as Live's arm-on-select).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use crate::clip::{Io, MidiFrom, Monitoring};
use crate::control::Controls;
use crate::engine::MidiOut;

/// How often new ports are looked for.
const RESCAN: Duration = Duration::from_secs(2);

/// The MIDI ports on this computer, for the I/O pickers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ports {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Runs MIDI routing until dropped.
pub struct Router {
    ports: Arc<Mutex<Ports>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Router {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Router {
    pub fn ports(&self) -> Ports {
        self.ports.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

/// Ports the studio leaves alone: Push's (its own module drives it) and the
/// studio's own.
fn ignored(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("push 2") || n.contains("ableton push") || n.contains("opendj studio")
}

/// A name's words, lower case, without the ones every port name carries.
fn words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| {
            !w.is_empty()
                && !matches!(
                    w.as_str(),
                    "midi" | "in" | "out" | "port" | "input" | "output"
                )
        })
        .collect()
}

/// Whether the computer's port `port` is the one a set calls `want`. Live
/// and each system name ports a little differently ("Arturia KeyLab
/// Essential 61 (MIDI In)", "KeyLab Essential 61 MIDI In", "KeyLab
/// Essential 61:KeyLab Essential 61 MIDI 1 20:0"), so one name's words all
/// being in the other's is enough.
pub fn port_matches(port: &str, want: &str) -> bool {
    let (p, w) = (words(port), words(want));
    if p.is_empty() || w.is_empty() {
        return false;
    }
    w.iter().all(|x| p.contains(x)) || p.iter().all(|x| w.contains(x))
}

/// The tracks a message from `port` on `channel` (0..16) plays.
pub fn targets(io: &[Io], selected: usize, port: &str, channel: u8) -> Vec<usize> {
    let any_armed = io.iter().any(|i| i.armed);
    io.iter()
        .enumerate()
        .filter(|(t, i)| {
            let from = match &i.midi_from {
                MidiFrom::All => true,
                MidiFrom::Port(p) => port_matches(port, p),
                MidiFrom::None => false,
            };
            let listening = match i.monitor {
                Monitoring::In => true,
                Monitoring::Auto => i.armed || (!any_armed && *t == selected),
                Monitoring::Off => false,
            };
            from && listening && i.midi_channel.is_none_or(|c| c == channel)
        })
        .map(|(t, _)| t)
        .collect()
}

/// Plays a message from `port` on the tracks that take it.
fn route(c: &Controls, port: &str, msg: &[u8]) {
    let Some(&status) = msg.first() else {
        return;
    };
    if !(0x80..0xF0).contains(&status) {
        return;
    }
    let (kind, channel) = (status & 0xF0, status & 0x0F);
    let a = msg.get(1).copied().unwrap_or(0);
    let b = msg.get(2).copied().unwrap_or(0);
    for t in targets(&c.all_io(), c.selected_track(), port, channel) {
        match kind {
            0x90 if b > 0 => c.note(t, a, b as f32 / 127.0),
            0x80 | 0x90 => c.note(t, a, 0.0),
            // Pressure, controllers, programs and bends: for plug-ins and
            // external synths, on channel 1 as the notes are.
            _ => c.midi(t, [kind, a, b]),
        }
    }
}

/// Starts routing: inputs to tracks, and the tracks' notes in `out` to
/// their MIDI To ports.
pub fn start(controls: Arc<Controls>, mut out: rtrb::Consumer<MidiOut>) -> Router {
    let ports = Arc::new(Mutex::new(Ports::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (p, s) = (ports.clone(), stop.clone());
    let _ = std::thread::Builder::new()
        .name("studio-midi".into())
        .spawn(move || {
            let mut inputs: HashMap<String, MidiInputConnection<()>> = HashMap::new();
            let mut outputs: HashMap<String, MidiOutputConnection> = HashMap::new();
            let mut scanned: Option<Instant> = None;
            // No MIDI on this computer (a server without ALSA's sequencer):
            // looked for once.
            let mut midi = true;
            while !s.load(Ordering::Relaxed) {
                if midi && scanned.is_none_or(|t| t.elapsed() >= RESCAN) {
                    scanned = Some(Instant::now());
                    match scan(&controls, &mut inputs) {
                        Some(found) => {
                            if let Ok(mut p) = p.lock() {
                                *p = found;
                            }
                        }
                        None => midi = false,
                    }
                }
                while let Ok(m) = out.pop() {
                    send(&controls, &mut outputs, m);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
    Router { ports, stop }
}

/// Opens inputs that came since last time and lets go of those gone;
/// returns every port's name, or None when there is no MIDI.
fn scan(
    controls: &Arc<Controls>,
    open: &mut HashMap<String, MidiInputConnection<()>>,
) -> Option<Ports> {
    let mut found = Ports::default();
    let probe = MidiInput::new("openDJ Studio scan").ok()?;
    {
        for port in probe.ports() {
            let Ok(name) = probe.port_name(&port) else {
                continue;
            };
            if ignored(&name) {
                continue;
            }
            found.inputs.push(name.clone());
            if open.contains_key(&name) {
                continue;
            }
            let Ok(input) = MidiInput::new("openDJ Studio in") else {
                continue;
            };
            let (c, n) = (controls.clone(), name.clone());
            match input.connect(
                &port,
                "openDJ Studio in",
                move |_, msg, _| route(&c, &n, msg),
                (),
            ) {
                Ok(conn) => {
                    open.insert(name, conn);
                }
                Err(e) => eprintln!("MIDI input {name}: {e}"),
            }
        }
    }
    open.retain(|name, _| found.inputs.contains(name));
    if let Ok(probe) = MidiOutput::new("openDJ Studio scan") {
        for port in probe.ports() {
            if let Ok(name) = probe.port_name(&port) {
                if !ignored(&name) {
                    found.outputs.push(name);
                }
            }
        }
    }
    Some(found)
}

/// Sends a track's note to its MIDI To port, on its channel.
fn send(controls: &Controls, open: &mut HashMap<String, MidiOutputConnection>, m: MidiOut) {
    let Some((want, channel)) = controls.io(m.track).midi_to else {
        return;
    };
    let name = match open.keys().find(|n| port_matches(n, &want)) {
        Some(n) => n.clone(),
        None => {
            let Ok(probe) = MidiOutput::new("openDJ Studio out") else {
                return;
            };
            let Some((port, name)) = probe.ports().into_iter().find_map(|p| {
                let n = probe.port_name(&p).ok()?;
                (port_matches(&n, &want) && !ignored(&n)).then_some((p, n))
            }) else {
                return;
            };
            match probe.connect(&port, "openDJ Studio out") {
                Ok(conn) => {
                    open.insert(name.clone(), conn);
                    name
                }
                Err(e) => {
                    eprintln!("MIDI output {want}: {e}");
                    return;
                }
            }
        }
    };
    let mut data = m.data;
    data[0] = (data[0] & 0xF0) | (channel & 0x0F);
    let len = if matches!(data[0] & 0xF0, 0xC0 | 0xD0) {
        2
    } else {
        3
    };
    if let Some(conn) = open.get_mut(&name) {
        if conn.send(&data[..len]).is_err() {
            open.remove(&name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_match_across_systems() {
        let want = "Arturia KeyLab Essential 61";
        assert!(port_matches("Arturia KeyLab Essential 61 MIDI In", want));
        assert!(port_matches("KeyLab Essential 61 MIDI In", want));
        assert!(port_matches(
            "KOMPLETE KONTROL M32:KOMPLETE KONTROL M32 MIDI 1 24:0",
            "KOMPLETE KONTROL M32"
        ));
        assert!(!port_matches("MiniFreak", want));
        assert!(ignored("Ableton Push 2 Live Port"));
    }

    #[test]
    fn messages_go_to_tracks_that_listen() {
        let keys = Io {
            midi_from: MidiFrom::Port("KeyLab Essential 61".into()),
            monitor: Monitoring::In,
            ..Io::default()
        };
        let auto = Io::default();
        let ch2 = Io {
            monitor: Monitoring::In,
            midi_channel: Some(1),
            ..Io::default()
        };
        let off = Io {
            monitor: Monitoring::Off,
            ..Io::default()
        };
        let io = vec![keys, auto, ch2, off];
        // Selected track 1 on Auto listens while nothing is armed.
        assert_eq!(targets(&io, 1, "KeyLab Essential 61 MIDI In", 0), [0, 1]);
        assert_eq!(targets(&io, 1, "MiniFreak", 1), [1, 2]);
        assert_eq!(targets(&io, 0, "MiniFreak", 0), Vec::<usize>::new());
        // An armed track takes over from the selection.
        let mut io = io;
        io[3].armed = true;
        io[3].monitor = Monitoring::Auto;
        assert_eq!(targets(&io, 1, "MiniFreak", 0), [3]);
    }
}
