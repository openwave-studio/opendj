//! What the window and a hardware controller both change: the mixer, the
//! instrument knobs and the selection. Either side calls these methods; each
//! stores the new value for the other side to show and sends the matching
//! command to the audio thread.

use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::{Arc, Mutex};

use crate::clip::{Clip, Io, Note, Sound, Track, MAX_TRACKS, SCENES};
use crate::devices::Settings;
use crate::engine::{Command, Monitor, Program, Quantize, Routing};
use crate::scale::Key;
use crate::sound::{self, PARAMS};
use crate::steps::Stages;

/// Where a fader sits for 0 dB: positions run -inf..+6 dB, 66 dB over the
/// travel.
pub const UNITY: f32 = 60.0 / 66.0;

/// A fader position (0..1) as a gain: 0 is silence, 1 is +6 dB.
pub fn fader_gain(position: f32) -> f32 {
    if position <= 0.001 {
        0.0
    } else {
        10f32.powf((6.0 - (1.0 - position) * 66.0) / 20.0)
    }
}

/// A fader position as the strip shows it.
pub fn fader_text(position: f32) -> String {
    if position <= 0.001 {
        "-inf".into()
    } else {
        format!("{:.1}", 6.0 - (1.0 - position) * 66.0)
    }
}

/// A pan knob (0..1) as text: C, 25L, 50R.
pub fn pan_text(pan: f32) -> String {
    let p = ((pan - 0.5) * 100.0).round() as i32;
    match p {
        0 => "C".into(),
        p if p < 0 => format!("{}L", -p),
        p => format!("{p}R"),
    }
}

/// What doesn't change while the studio runs: each track's look and sound.
#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub name: String,
    pub color: u32,
    pub sound: Sound,
    pub devices: Vec<crate::clip::DeviceInfo>,
    /// A Drum Rack's pads that play (note and name), lowest note first.
    pub pads: Vec<(u8, String)>,
}

impl TrackInfo {
    pub fn drums(&self) -> bool {
        matches!(self.sound, Sound::Drums(_))
    }

    pub fn audio(&self) -> bool {
        self.sound == Sound::Audio
    }

    /// Whether an External Instrument plays it: hardware out over MIDI,
    /// its sound back in from the sound card.
    pub fn external(&self) -> bool {
        self.devices
            .iter()
            .any(|d| matches!(d.builtin, Some(Settings::External(_))))
    }
}

/// Which of the track's controls a mixer value is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mix {
    Volume,
    Pan,
    SendA,
    SendB,
}

impl Mix {
    pub const ALL: [Mix; 4] = [Mix::Volume, Mix::Pan, Mix::SendA, Mix::SendB];

    pub fn label(self) -> &'static str {
        match self {
            Mix::Volume => "Volume",
            Mix::Pan => "Pan",
            Mix::SendA => "Send A",
            Mix::SendB => "Send B",
        }
    }
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

fn floats<const N: usize>(v: f32) -> [AtomicU32; N] {
    std::array::from_fn(|_| AtomicU32::new(v.to_bits()))
}

pub struct Controls {
    commands: Mutex<rtrb::Producer<Command>>,
    pub monitor: Arc<Monitor>,
    pub tracks: Vec<TrackInfo>,
    /// The clips, as last sent to the audio thread.
    set: Mutex<Vec<Track>>,
    mix: [[AtomicU32; 4]; MAX_TRACKS],
    main: AtomicU32,
    params: [[AtomicU32; PARAMS]; MAX_TRACKS],
    selected_track: AtomicUsize,
    selected_scene: AtomicI32,
    quantize: AtomicU8,
    /// The key the pads play in and stage clips are written in.
    key: Mutex<Key>,
    /// Each track's devices switched on, by place in its chain.
    devices_on: Vec<Vec<AtomicBool>>,
    /// Each track's inputs and outputs.
    io: Mutex<Vec<Io>>,
    /// Each track's built-in devices' settings now, by place in its chain.
    builtin: Mutex<Vec<Vec<Option<Settings>>>>,
    /// Goes up on every change, so a screen can tell when to redraw.
    version: AtomicU64,
}

impl Controls {
    pub fn new(
        set: &[Track],
        colors: &[u32],
        commands: rtrb::Producer<Command>,
        monitor: Arc<Monitor>,
    ) -> Controls {
        let tracks: Vec<TrackInfo> = set
            .iter()
            .enumerate()
            .map(|(i, t)| TrackInfo {
                name: t.name.clone(),
                color: t.color.or(colors.get(i).copied()).unwrap_or(0xb69cff),
                sound: t.sound,
                devices: t.devices.clone(),
                pads: t.kit.as_ref().map_or(Vec::new(), |k| {
                    let mut pads: Vec<(u8, String)> = k
                        .pads
                        .iter()
                        .filter(|p| p.on && p.sample.is_some())
                        .map(|p| (p.note, p.name.clone()))
                        .collect();
                    pads.sort();
                    pads
                }),
            })
            .collect();
        let params = std::array::from_fn(|t| {
            let defaults = set
                .get(t)
                .map_or([0.5; PARAMS], |tr| sound::default_params(tr.sound));
            std::array::from_fn(|i| AtomicU32::new(defaults[i].to_bits()))
        });
        Controls {
            commands: Mutex::new(commands),
            monitor,
            tracks,
            set: Mutex::new(set.to_vec()),
            mix: std::array::from_fn(|t| {
                let [v, p, a, b]: [AtomicU32; 4] = floats(0.0);
                let track = set.get(t);
                store(&v, track.and_then(|t| t.volume).unwrap_or(UNITY));
                store(&p, track.map_or(0.5, |t| t.pan));
                [v, p, a, b]
            }),
            main: AtomicU32::new(UNITY.to_bits()),
            params,
            selected_track: AtomicUsize::new(0),
            selected_scene: AtomicI32::new(-1),
            quantize: AtomicU8::new(0),
            key: Mutex::new(Key::default()),
            devices_on: set
                .iter()
                .map(|t| t.devices.iter().map(|d| AtomicBool::new(d.on)).collect())
                .collect(),
            io: Mutex::new(set.iter().map(|t| t.io.clone()).collect()),
            builtin: Mutex::new(
                set.iter()
                    .map(|t| t.devices.iter().map(|d| d.builtin.clone()).collect())
                    .collect(),
            ),
            version: AtomicU64::new(1),
        }
    }

    /// The note drum pad `i` (0..16, bottom left first) plays on a drum
    /// track: a Drum Rack's pads from C1 up, as Live lays them out, or the
    /// built-in kit's sounds.
    pub fn drum_note(&self, track: usize, i: usize) -> Option<u8> {
        match self.tracks.get(track) {
            Some(t) if !t.pads.is_empty() => (i < 16).then_some(36 + i as u8),
            _ => crate::sound::DRUM_NOTES.get(i).copied(),
        }
    }

    /// The name of the sound drum pad `i` plays.
    pub fn drum_name(&self, track: usize, i: usize) -> String {
        let Some(note) = self.drum_note(track, i) else {
            return "Pad".into();
        };
        match self.tracks.get(track) {
            Some(t) if !t.pads.is_empty() => t
                .pads
                .iter()
                .find(|(n, _)| *n == note)
                .map_or_else(|| "Empty".into(), |(_, name)| name.clone()),
            _ => crate::sound::DRUM_NAMES
                .get(i)
                .copied()
                .unwrap_or("Pad")
                .into(),
        }
    }

    /// A track's inputs and outputs.
    pub fn io(&self, track: usize) -> Io {
        self.io
            .lock()
            .ok()
            .and_then(|io| io.get(track).cloned())
            .unwrap_or_default()
    }

    /// Every track's inputs and outputs.
    pub fn all_io(&self) -> Vec<Io> {
        self.io.lock().map(|io| io.clone()).unwrap_or_default()
    }

    /// Changes a track's inputs and outputs.
    pub fn edit_io(&self, track: usize, f: impl FnOnce(&mut Io)) {
        let routing = {
            let Ok(mut all) = self.io.lock() else {
                return;
            };
            let Some(io) = all.get_mut(track) else {
                return;
            };
            f(io);
            if self.tracks[track].sound != Sound::Audio {
                io.audio_in = None;
            }
            Routing::from(&*io)
        };
        self.send(Command::Route { track, routing });
        self.bump();
    }

    /// Arms a track (disarming the others, as Live's exclusive arm) or
    /// disarms it.
    pub fn toggle_arm(&self, track: usize) {
        let arm = !self.io(track).armed;
        for t in 0..self.tracks.len() {
            let was = self.io(t).armed;
            let now = if t == track { arm } else { was && !arm };
            if now != was {
                self.edit_io(t, |io| io.armed = now);
            }
        }
    }

    /// A controller's MIDI message (other than a note) for a track.
    pub fn midi(&self, track: usize, data: [u8; 3]) {
        if track < self.tracks.len() {
            self.send(Command::Midi { track, data });
        }
    }

    pub(crate) fn send(&self, cmd: Command) {
        let pushed = self.commands.lock().map(|mut q| q.push(cmd).is_ok());
        if !matches!(pushed, Ok(true)) {
            eprintln!("command queue full");
        }
    }

    pub(crate) fn bump(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    fn with_clip<R>(&self, track: usize, scene: usize, f: impl FnOnce(&Clip) -> R) -> Option<R> {
        let set = self.set.lock().ok()?;
        set.get(track)?.slots.get(scene)?.as_ref().map(f)
    }

    pub fn has_clip(&self, track: usize, scene: usize) -> bool {
        self.with_clip(track, scene, |_| ()).is_some()
    }

    /// A clip's name and length in beats.
    pub fn clip_info(&self, track: usize, scene: usize) -> Option<(String, f64)> {
        self.with_clip(track, scene, |c| (c.name.clone(), c.beats))
    }

    // ----------------------------------------------------------- notes

    /// A note clip's length in beats and notes: an empty one-bar clip for
    /// an empty slot, None for a stage or audio clip.
    pub fn notes(&self, track: usize, scene: usize) -> Option<(f64, Vec<Note>)> {
        let set = self.set.lock().ok()?;
        match set.get(track)?.slots.get(scene)? {
            None => Some((4.0, Vec::new())),
            Some(c) if c.stages.is_none() && c.audio.is_none() => Some((c.beats, c.notes.clone())),
            Some(_) => None,
        }
    }

    /// Changes the note clip in a slot (making a one-bar clip in an empty
    /// slot) and sends the set to the audio thread. False for a stage or
    /// audio clip.
    pub fn edit_notes(&self, track: usize, scene: usize, f: impl FnOnce(&mut Clip)) -> bool {
        let Ok(mut set) = self.set.lock() else {
            return false;
        };
        let Some(slot) = set.get_mut(track).and_then(|t| t.slots.get_mut(scene)) else {
            return false;
        };
        let clip = slot.get_or_insert_with(|| Clip::new("Clip", 4.0, Vec::new()));
        if clip.stages.is_some() || clip.audio.is_some() {
            return false;
        }
        f(clip);
        clip.notes.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.send(Command::Program(Program::compile(&set)));
        drop(set);
        self.bump();
        true
    }

    // ---------------------------------------------------------- stages

    /// The slot Steps mode edits: the selected track's selected scene, or
    /// the scene it is playing, or the first.
    pub fn steps_slot(&self) -> (usize, usize) {
        let t = self.selected_track();
        let s = self.selected_scene().unwrap_or_else(|| {
            (0..SCENES)
                .find(|&s| self.monitor.slot(t, s) == crate::engine::SlotState::Playing)
                .unwrap_or(0)
        });
        (t, s)
    }

    /// The stage clip in a slot: None when the slot holds a note clip,
    /// a fresh one when it is empty.
    pub fn stages(&self, track: usize, scene: usize) -> Option<Stages> {
        let set = self.set.lock().ok()?;
        match set.get(track)?.slots.get(scene)? {
            None => Some(Stages::default()),
            Some(c) => c.stages.as_deref().cloned(),
        }
    }

    /// Changes the stage clip in a slot (making one in an empty slot) and
    /// sends the set to the audio thread. False for a note clip.
    pub fn edit_stages(&self, track: usize, scene: usize, f: impl FnOnce(&mut Stages)) -> bool {
        let Ok(mut set) = self.set.lock() else {
            return false;
        };
        let Some(tr) = set.get_mut(track) else {
            return false;
        };
        let sound = tr.sound;
        let Some(slot) = tr.slots.get_mut(scene) else {
            return false;
        };
        let (mut stages, name) = match slot {
            None => (Stages::default(), "Steps".to_string()),
            Some(c) => match c.stages.as_deref() {
                Some(s) => (s.clone(), c.name.clone()),
                None => return false,
            },
        };
        let before = stages.clone();
        f(&mut stages);
        if slot.is_some() && stages == before {
            return true;
        }
        *slot = Some(stages.clip(&name, sound, &self.key()));
        self.send(Command::Program(Program::compile(&set)));
        drop(set);
        self.bump();
        true
    }

    // --------------------------------------------------------- devices

    pub fn device_on(&self, track: usize, device: usize) -> bool {
        self.devices_on
            .get(track)
            .and_then(|t| t.get(device))
            .is_some_and(|d| d.load(Ordering::Relaxed))
    }

    /// A built-in device's settings as they are now.
    pub fn builtin(&self, track: usize, device: usize) -> Option<Settings> {
        self.builtin.lock().ok()?.get(track)?.get(device)?.clone()
    }

    /// Changes one setting of a built-in device (see `Settings::params`).
    pub fn set_device_param(&self, track: usize, device: usize, index: usize, value: f32) {
        let Ok(mut all) = self.builtin.lock() else {
            return;
        };
        let Some(s) = all
            .get_mut(track)
            .and_then(|t| t.get_mut(device))
            .and_then(|d| d.as_mut())
        else {
            return;
        };
        s.set(index, value);
        drop(all);
        self.send(Command::DeviceParam {
            track,
            device,
            index,
            value,
        });
        self.bump();
    }

    /// Switches a device in a track's chain on or off.
    pub fn set_device_on(&self, track: usize, device: usize, on: bool) {
        if let Some(d) = self.devices_on.get(track).and_then(|t| t.get(device)) {
            d.store(on, Ordering::Relaxed);
            self.send(Command::Bypass { track, device, on });
            self.bump();
        }
    }

    // ------------------------------------------------------------- key

    pub fn key(&self) -> Key {
        self.key.lock().map_or(Key::default(), |k| *k)
    }

    /// Changes the key; stage clips are rewritten in it.
    pub fn edit_key(&self, f: impl FnOnce(&mut Key)) {
        let Ok(mut k) = self.key.lock() else {
            return;
        };
        let before = *k;
        f(&mut k);
        let key = *k;
        drop(k);
        if key == before {
            return;
        }
        if let Ok(mut set) = self.set.lock() {
            let mut changed = false;
            for tr in set.iter_mut() {
                let sound = tr.sound;
                for clip in tr.slots.iter_mut().flatten() {
                    if let Some(st) = clip.stages.as_deref() {
                        *clip = st.clip(&clip.name, sound, &key);
                        changed = true;
                    }
                }
            }
            if changed {
                self.send(Command::Program(Program::compile(&set)));
            }
        }
        self.bump();
    }

    // ------------------------------------------------------- launching

    pub fn launch(&self, track: usize, scene: usize) {
        self.select(track, Some(scene));
        self.send(Command::Launch { track, scene });
    }

    pub fn launch_scene(&self, scene: usize) {
        self.send(Command::Scene(scene));
    }

    pub fn stop(&self, track: usize) {
        self.send(Command::Stop(track));
    }

    pub fn stop_all(&self) {
        self.send(Command::StopAll);
    }

    pub fn set_playing(&self, on: bool) {
        self.send(Command::Play(on));
    }

    pub fn toggle_play(&self) {
        self.set_playing(!self.monitor.playing());
    }

    pub fn tempo_step(&self, delta: f64) {
        let bpm = ((self.monitor.tempo() + delta) * 10.0).round() / 10.0;
        self.send(Command::Tempo(bpm.clamp(40.0, 250.0)));
    }

    pub fn quantize(&self) -> Quantize {
        match self.quantize.load(Ordering::Relaxed) {
            1 => Quantize::Beat,
            2 => Quantize::Off,
            _ => Quantize::Bar,
        }
    }

    pub fn next_quantize(&self) -> Quantize {
        let q = self.quantize().next();
        self.quantize.store(
            match q {
                Quantize::Bar => 0,
                Quantize::Beat => 1,
                Quantize::Off => 2,
            },
            Ordering::Relaxed,
        );
        self.send(Command::Quantize(q));
        self.bump();
        q
    }

    /// Plays `pitch` on `track`'s instrument now; velocity 0 ends it.
    pub fn note(&self, track: usize, pitch: u8, velocity: f32) {
        if track < self.tracks.len() {
            self.send(Command::Note {
                track,
                pitch,
                velocity,
            });
        }
    }

    // ------------------------------------------------------- selection

    pub fn selected_track(&self) -> usize {
        self.selected_track.load(Ordering::Relaxed)
    }

    pub fn selected_scene(&self) -> Option<usize> {
        usize::try_from(self.selected_scene.load(Ordering::Relaxed)).ok()
    }

    pub fn select(&self, track: usize, scene: Option<usize>) {
        if track < self.tracks.len() {
            self.selected_track.store(track, Ordering::Relaxed);
        }
        self.selected_scene.store(
            scene.map_or(-1, |s| s.min(SCENES - 1) as i32),
            Ordering::Relaxed,
        );
        self.bump();
    }

    pub fn select_track(&self, track: usize) {
        self.select(track, self.selected_scene());
    }

    // ----------------------------------------------------------- mixer

    pub fn mix(&self, track: usize, which: Mix) -> f32 {
        self.mix
            .get(track)
            .map_or(0.0, |m| load(&m[which as usize]))
    }

    pub fn set_mix(&self, track: usize, which: Mix, value: f32) {
        let Some(m) = self.mix.get(track) else { return };
        let v = value.clamp(0.0, 1.0);
        store(&m[which as usize], v);
        match which {
            Mix::Volume => self.send(Command::Volume {
                track,
                gain: fader_gain(v),
            }),
            Mix::Pan => self.send(Command::Pan { track, pan: v }),
            // Returns come later: the sends are kept and shown.
            Mix::SendA | Mix::SendB => {}
        }
        self.bump();
    }

    pub fn main_volume(&self) -> f32 {
        load(&self.main)
    }

    pub fn set_main_volume(&self, value: f32) {
        let v = value.clamp(0.0, 1.0);
        store(&self.main, v);
        self.send(Command::MainVolume(fader_gain(v)));
        self.bump();
    }

    // ----------------------------------------------------- instruments

    pub fn param(&self, track: usize, index: usize) -> f32 {
        self.params
            .get(track)
            .and_then(|p| p.get(index))
            .map_or(0.0, load)
    }

    pub fn set_param(&self, track: usize, index: usize, value: f32) {
        let Some(a) = self.params.get(track).and_then(|p| p.get(index)) else {
            return;
        };
        let v = value.clamp(0.0, 1.0);
        store(a, v);
        self.send(Command::Param {
            track,
            index,
            value: v,
        });
        self.bump();
    }

    /// Knob `index` of `track` at `value`, as text.
    pub fn param_text_at(&self, track: usize, index: usize, value: f32) -> String {
        let drums = self.tracks.get(track).is_some_and(|t| t.drums());
        sound::param_text(drums, index, value)
    }

    /// Knob `index` of `track` as text.
    pub fn param_text(&self, track: usize, index: usize) -> String {
        let drums = self.tracks.get(track).is_some_and(|t| t.drums());
        sound::param_text(drums, index, self.param(track, index))
    }

    pub fn param_name(&self, track: usize, index: usize) -> &'static str {
        let drums = self.tracks.get(track).is_some_and(|t| t.drums());
        sound::params(drums)[index.min(PARAMS - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faders_and_text() {
        assert_eq!(fader_gain(0.0), 0.0);
        assert!((fader_gain(UNITY) - 1.0).abs() < 1e-5);
        assert_eq!(fader_text(UNITY), "0.0");
        assert_eq!(pan_text(0.5), "C");
        assert_eq!(pan_text(0.25), "25L");
        assert_eq!(pan_text(1.0), "50R");
    }

    #[test]
    fn changes_reach_the_audio_thread() {
        let set = crate::clip::demo_set();
        let (tx, mut rx) = rtrb::RingBuffer::new(16);
        let monitor = crate::engine::Studio::new(&set, 48_000.0, 120.0).1.monitor;
        let c = Controls::new(&set, &[0; 4], tx, monitor);
        let v0 = c.version();
        c.set_mix(1, Mix::Pan, 0.2);
        c.set_param(0, 1, 0.3);
        assert!(c.version() > v0);
        assert!(matches!(rx.pop(), Ok(Command::Pan { track: 1, .. })));
        assert!(matches!(
            rx.pop(),
            Ok(Command::Param {
                track: 0,
                index: 1,
                ..
            })
        ));
        assert_eq!(c.param_name(0, 1), "Length");
        assert!(c.has_clip(0, 0));
    }
}
