//! What the window and a hardware controller both change: the mixer, the
//! instrument knobs and the selection. Either side calls these methods; each
//! stores the new value for the other side to show and sends the matching
//! command to the audio thread.

use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::{Arc, Mutex};

use crate::clip::{Clip, DeviceInfo, Io, Note, Sound, Track, MAX_TRACKS, SCENES};
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
/// (Its devices do: see [`Controls::devices`].)
#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub name: String,
    pub color: u32,
    pub sound: Sound,
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
}

/// Whether a device plays the track's notes (an instrument plug-in or an
/// External Instrument) rather than working on its sound.
pub fn is_instrument(d: &DeviceInfo) -> bool {
    d.plugin.as_ref().is_some_and(|p| p.instrument)
        || d.builtin.as_ref().is_some_and(Settings::is_instrument)
}

/// Whether a device changes the notes on their way to the instrument.
pub fn is_midi_effect(d: &DeviceInfo) -> bool {
    d.builtin.as_ref().is_some_and(Settings::is_midi_effect)
}

/// How many MIDI effects lead the chain: the places before the instrument.
pub fn midi_effects(chain: &[DeviceInfo]) -> usize {
    chain.iter().take_while(|d| is_midi_effect(d)).count()
}

/// The first place in a chain an audio effect can go: after its
/// instruments and the MIDI effects before them.
pub fn first_effect_place(chain: &[DeviceInfo]) -> usize {
    chain
        .iter()
        .rposition(is_instrument)
        .map_or(0, |i| i + 1)
        .max(midi_effects(chain))
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
    /// Each track's inputs and outputs.
    io: Mutex<Vec<Io>>,
    /// Each track's devices in chain order, as they are now: switched on or
    /// not, and a built-in device's settings. Edits are sent to the audio
    /// thread while this is locked, so they reach it in the order made.
    chains: Mutex<Vec<Vec<DeviceInfo>>>,
    /// Goes up on every change, so a screen can tell when to redraw.
    version: AtomicU64,
    metronome: AtomicBool,
    /// Tap Tempo's taps, latest last.
    taps: Mutex<Vec<std::time::Instant>>,
    /// The slot being recorded into, if any.
    recording: Mutex<Option<Recording>>,
    /// Bars a new recording stops at, 0 for none (Fixed Length off).
    fixed_bars: AtomicU32,
    /// The set before each clip or mixer edit, latest last, and the ones
    /// undone.
    undo: Mutex<History>,
}

/// Recording notes into a slot.
#[derive(Clone, Debug)]
struct Recording {
    track: usize,
    scene: usize,
    /// The beat the clip starts playing on.
    start: f64,
    /// A new clip (rather than overdubbing one), and its length when Fixed
    /// Length is on.
    new: bool,
    fixed: Option<f64>,
    /// Notes down: pitch, where in the clip, velocity, the song's beat.
    held: Vec<(u8, f64, f32, f64)>,
}

/// How long a new clip being recorded is, until recording stops and it
/// takes its real length.
const OPEN_LENGTH: f64 = 1024.0;

#[derive(Default)]
struct History {
    undo: Vec<Vec<Track>>,
    redo: Vec<Vec<Track>>,
    /// What the last checkpoint was for and when: edits to the same thing
    /// close together are one step (a knob drag, notes being drawn).
    last: Option<(Edit, std::time::Instant)>,
}

/// What an edit changed, so edits close together to the same thing undo
/// as one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    Clip(usize, usize),
    Mixer,
    Record,
    Other,
}

/// Steps of undo kept.
const UNDO_STEPS: usize = 64;
/// Edits to the same thing closer together than this undo as one.
const UNDO_JOIN: std::time::Duration = std::time::Duration::from_millis(1200);

/// Taps further apart than this start a new count.
const TAP_GAP: std::time::Duration = std::time::Duration::from_secs(2);

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
            let defaults = set.get(t).map_or([0.5; PARAMS], |tr| {
                tr.params.unwrap_or_else(|| sound::default_params(tr.sound))
            });
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
                store(&a, track.map_or(0.0, |t| t.sends[0]));
                store(&b, track.map_or(0.0, |t| t.sends[1]));
                [v, p, a, b]
            }),
            main: AtomicU32::new(UNITY.to_bits()),
            params,
            selected_track: AtomicUsize::new(0),
            selected_scene: AtomicI32::new(-1),
            quantize: AtomicU8::new(0),
            key: Mutex::new(Key::default()),
            io: Mutex::new(set.iter().map(|t| t.io.clone()).collect()),
            chains: Mutex::new(set.iter().map(|t| t.devices.clone()).collect()),
            version: AtomicU64::new(1),
            metronome: AtomicBool::new(false),
            taps: Mutex::new(Vec::new()),
            recording: Mutex::new(None),
            fixed_bars: AtomicU32::new(0),
            undo: Mutex::new(History::default()),
        }
    }

    /// The set as it is now, for saving: the clips, each track's devices,
    /// I/O, mixer and knobs. Plug-ins' sounds are as last kept (see
    /// `Plugins::keep_sounds`).
    pub fn snapshot(&self) -> Vec<Track> {
        let mut set = self.set.lock().map(|s| s.clone()).unwrap_or_default();
        let chains = self.chains.lock().map(|c| c.clone()).unwrap_or_default();
        let io = self.io.lock().map(|c| c.clone()).unwrap_or_default();
        for (t, track) in set.iter_mut().enumerate() {
            if let Some(chain) = chains.get(t) {
                track.devices = chain.clone();
            }
            if let Some(io) = io.get(t) {
                track.io = io.clone();
            }
            track.volume = Some(self.mix(t, Mix::Volume));
            track.pan = self.mix(t, Mix::Pan);
            track.sends = [self.mix(t, Mix::SendA), self.mix(t, Mix::SendB)];
            track.params = Some(std::array::from_fn(|i| self.param(t, i)));
            if track.color.is_none() {
                track.color = self.tracks.get(t).map(|i| i.color);
            }
        }
        set
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

    pub fn bump(&self) {
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

    /// Where a note clip's loop starts, in beats from its first beat (0 for
    /// an empty slot).
    pub fn loop_start(&self, track: usize, scene: usize) -> f64 {
        let Ok(set) = self.set.lock() else {
            return 0.0;
        };
        set.get(track)
            .and_then(|t| t.slots.get(scene))
            .and_then(|s| s.as_ref())
            .map_or(0.0, |c| c.loop_start)
    }

    /// Changes the note clip in a slot (making a one-bar clip in an empty
    /// slot) and sends the set to the audio thread. False for a stage or
    /// audio clip.
    pub fn edit_notes(&self, track: usize, scene: usize, f: impl FnOnce(&mut Clip)) -> bool {
        let Ok(mut set) = self.set.lock() else {
            return false;
        };
        let editable = match set.get(track).and_then(|t| t.slots.get(scene)) {
            None => false,
            Some(None) => true,
            Some(Some(c)) => c.stages.is_none() && c.audio.is_none(),
        };
        if !editable {
            return false;
        }
        self.checkpoint(&set, Edit::Clip(track, scene));
        let Some(slot) = set.get_mut(track).and_then(|t| t.slots.get_mut(scene)) else {
            return false;
        };
        let clip = slot.get_or_insert_with(|| Clip::new("Clip", 4.0, Vec::new()));
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
        let clip = stages.clip(&name, sound, &self.key());
        self.checkpoint(&set, Edit::Clip(track, scene));
        set[track].slots[scene] = Some(clip);
        self.send(Command::Program(Program::compile(&set)));
        drop(set);
        self.bump();
        true
    }

    // --------------------------------------------------------- devices

    /// A track's devices in chain order, as they are now.
    pub fn devices(&self, track: usize) -> Vec<DeviceInfo> {
        self.chains
            .lock()
            .ok()
            .and_then(|c| c.get(track).cloned())
            .unwrap_or_default()
    }

    /// Whether an External Instrument plays the track: hardware out over
    /// MIDI, its sound back in from the sound card.
    pub fn external(&self, track: usize) -> bool {
        self.devices(track)
            .iter()
            .any(|d| matches!(d.builtin, Some(Settings::External(_))))
    }

    pub fn device_on(&self, track: usize, device: usize) -> bool {
        self.chains
            .lock()
            .ok()
            .and_then(|c| c.get(track)?.get(device).map(|d| d.on))
            .unwrap_or(false)
    }

    /// A built-in device's settings as they are now.
    pub fn builtin(&self, track: usize, device: usize) -> Option<Settings> {
        self.chains
            .lock()
            .ok()?
            .get(track)?
            .get(device)?
            .builtin
            .clone()
    }

    /// Changes one setting of a built-in device (see `Settings::params`).
    pub fn set_device_param(&self, track: usize, device: usize, index: usize, value: f32) {
        let Ok(mut all) = self.chains.lock() else {
            return;
        };
        let Some(s) = all
            .get_mut(track)
            .and_then(|t| t.get_mut(device))
            .and_then(|d| d.builtin.as_mut())
        else {
            return;
        };
        s.set(index, value);
        self.send(Command::DeviceParam {
            track,
            device,
            index,
            value,
        });
        drop(all);
        self.bump();
    }

    /// Switches a device in a track's chain on or off.
    pub fn set_device_on(&self, track: usize, device: usize, on: bool) {
        let Ok(mut all) = self.chains.lock() else {
            return;
        };
        if let Some(d) = all.get_mut(track).and_then(|t| t.get_mut(device)) {
            d.on = on;
            self.send(Command::Bypass { track, device, on });
            drop(all);
            self.bump();
        }
    }

    /// Edits the chains: `f` changes the lists and sends the matching
    /// commands while they are locked (see [`ChainEdit`]).
    pub fn edit_chains<R>(&self, f: impl FnOnce(&mut ChainEdit) -> R) -> Option<R> {
        let mut all = self.chains.lock().ok()?;
        let r = f(&mut ChainEdit {
            chains: &mut all,
            controls: self,
        });
        drop(all);
        self.bump();
        Some(r)
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

    /// Sets the tempo, in BPM (40 to 250, to a tenth).
    pub fn set_tempo(&self, bpm: f64) {
        let bpm = (bpm * 10.0).round() / 10.0;
        self.send(Command::Tempo(bpm.clamp(40.0, 250.0)));
        self.bump();
    }

    /// A Tap Tempo tap at `now`. From the second tap on, the tempo follows
    /// the last few taps; a pause of two seconds starts a new count.
    /// Returns the tempo it set.
    pub fn tap_tempo(&self, now: std::time::Instant) -> Option<f64> {
        let mut taps = self.taps.lock().ok()?;
        if taps
            .last()
            .is_some_and(|&last| now.saturating_duration_since(last) > TAP_GAP)
        {
            taps.clear();
        }
        taps.push(now);
        if taps.len() > 5 {
            taps.remove(0);
        }
        if taps.len() < 2 {
            return None;
        }
        let span = taps[taps.len() - 1].saturating_duration_since(taps[0]);
        let beat = span.as_secs_f64() / (taps.len() - 1) as f64;
        if beat <= 0.0 {
            return None;
        }
        let bpm = (60.0 / beat).clamp(40.0, 250.0);
        drop(taps);
        self.set_tempo(bpm);
        Some(bpm)
    }

    pub fn metronome(&self) -> bool {
        self.metronome.load(Ordering::Relaxed)
    }

    pub fn toggle_metronome(&self) {
        let on = !self.metronome();
        self.metronome.store(on, Ordering::Relaxed);
        self.send(Command::Metronome(on));
        self.bump();
    }

    // ------------------------------------------------------------ undo

    /// Keeps the set as it is before an edit, for undo. Edits to the same
    /// thing close together are one step.
    fn checkpoint(&self, set: &[Track], what: Edit) {
        let Ok(mut h) = self.undo.lock() else {
            return;
        };
        let now = std::time::Instant::now();
        let joined = what != Edit::Other
            && h.last
                .is_some_and(|(w, at)| w == what && now.duration_since(at) < UNDO_JOIN);
        h.last = Some((what, now));
        if joined {
            return;
        }
        h.undo.push(set.to_vec());
        if h.undo.len() > UNDO_STEPS {
            h.undo.remove(0);
        }
        h.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        self.undo.lock().is_ok_and(|h| !h.undo.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        self.undo.lock().is_ok_and(|h| !h.redo.is_empty())
    }

    /// Takes back the last clip or mixer edit (or, with `redo`, puts back
    /// the last one taken back). Device edits aren't in it. Returns whether
    /// there was one.
    pub fn undo(&self, redo: bool) -> bool {
        self.end_recording(false);
        let Ok(mut set) = self.set.lock() else {
            return false;
        };
        let Ok(mut h) = self.undo.lock() else {
            return false;
        };
        let Some(back) = (if redo { h.redo.pop() } else { h.undo.pop() }) else {
            return false;
        };
        let now = std::mem::replace(&mut *set, back);
        if redo {
            h.undo.push(now);
        } else {
            h.redo.push(now);
        }
        h.last = None;
        drop(h);
        self.send(Command::Program(Program::compile(&set)));
        self.send(Command::Audible(crate::engine::audible(&set)));
        drop(set);
        self.bump();
        true
    }

    // ------------------------------------------------------- recording

    /// The slot being recorded into: track and scene.
    pub fn recording(&self) -> Option<(usize, usize)> {
        self.recording
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(|r| (r.track, r.scene)))
    }

    /// Bars a new recording stops at (Fixed Length), None when it is off.
    pub fn fixed_length(&self) -> Option<u32> {
        match self.fixed_bars.load(Ordering::Relaxed) {
            0 => None,
            n => Some(n),
        }
    }

    pub fn set_fixed_length(&self, bars: Option<u32>) {
        self.fixed_bars.store(bars.unwrap_or(0), Ordering::Relaxed);
        self.bump();
    }

    /// Record on a slot: into an empty one a new clip, from the next launch
    /// point (the song starts if it was stopped); on a note clip, notes
    /// played are added to it as it loops (overdub). On the slot being
    /// recorded, recording stops and the clip plays on. False when the slot
    /// can't take notes (a stage or audio clip, an audio track).
    pub fn record(&self, track: usize, scene: usize) -> bool {
        if self.recording() == Some((track, scene)) {
            self.end_recording(true);
            return true;
        }
        self.end_recording(true);
        let Some(info) = self.tracks.get(track) else {
            return false;
        };
        if info.sound == Sound::Audio || scene >= SCENES {
            return false;
        }
        let new = !self.has_clip(track, scene);
        if !new && self.notes(track, scene).is_none() {
            return false;
        }
        let playing = self.monitor.playing();
        // Overdubbing a clip that already plays starts at once.
        let already = self.monitor.slot(track, scene) == crate::engine::SlotState::Playing;
        let start = if already {
            self.monitor.beat()
        } else if playing {
            crate::engine::next_launch(self.monitor.beat(), self.quantize())
        } else {
            0.0
        };
        let fixed = self.fixed_length().map(|b| b as f64 * 4.0);
        if new {
            let Ok(mut set) = self.set.lock() else {
                return false;
            };
            self.checkpoint(&set, Edit::Record);
            set[track].slots[scene] = Some(Clip::new(
                "Recording",
                fixed.unwrap_or(OPEN_LENGTH),
                Vec::new(),
            ));
            self.send(Command::Program(Program::compile(&set)));
        } else if let Ok(set) = self.set.lock() {
            self.checkpoint(&set, Edit::Record);
        }
        if !already {
            if !playing {
                self.set_playing(true);
            }
            self.launch(track, scene);
        }
        if let Ok(mut r) = self.recording.lock() {
            *r = Some(Recording {
                track,
                scene,
                start,
                new,
                fixed: fixed.filter(|_| new),
                held: Vec::new(),
            });
        }
        self.bump();
        true
    }

    /// Stops recording. A new clip takes its length: the bars recorded
    /// (at least one), or Fixed Length's. `keep` false drops a new clip
    /// (undo while recording).
    fn end_recording(&self, keep: bool) {
        let Some(mut r) = self.recording.lock().ok().and_then(|mut r| r.take()) else {
            return;
        };
        let beat = self.monitor.beat();
        // Notes still held end now.
        for (pitch, at, velocity, on) in std::mem::take(&mut r.held) {
            self.add_recorded(&r, pitch, at, velocity, (beat - on).max(0.05));
        }
        if r.new && keep {
            let elapsed = (beat - r.start).max(0.0);
            let beats = r
                .fixed
                .unwrap_or_else(|| ((elapsed / 4.0).ceil().max(1.0)) * 4.0);
            if let Ok(mut set) = self.set.lock() {
                if let Some(Some(c)) = set.get_mut(r.track).and_then(|t| t.slots.get_mut(r.scene)) {
                    c.beats = beats;
                    c.name = "Clip".into();
                    for n in &mut c.notes {
                        n.length = n.length.min(beats - n.start).max(0.01);
                    }
                    c.notes.retain(|n| n.start < beats);
                }
                self.send(Command::Program(Program::compile(&set)));
            }
        }
        self.bump();
    }

    /// Stops recording when a fixed-length recording has its length, or
    /// its clip stopped; called every few milliseconds.
    pub fn record_tick(&self) {
        let beat = self.monitor.beat();
        let done = self.recording.lock().ok().is_some_and(|r| {
            r.as_ref().is_some_and(|r| {
                let full = r.fixed.is_some_and(|len| beat >= r.start + len - 1e-6);
                // The clip was stopped, or the song.
                let stopped = beat > r.start + 0.5
                    && self.monitor.slot(r.track, r.scene) != crate::engine::SlotState::Playing;
                full || stopped
            })
        });
        if done {
            self.end_recording(true);
        }
    }

    /// A live note on `track`, into the slot being recorded.
    fn record_note(&self, track: usize, pitch: u8, velocity: f32) {
        let Ok(mut guard) = self.recording.lock() else {
            return;
        };
        let Some(r) = guard.as_mut().filter(|r| r.track == track) else {
            return;
        };
        let beat = self.monitor.beat();
        if velocity > 0.0 {
            // Before the clip starts (the count to the launch point): not
            // recorded.
            let state = self.monitor.slot(r.track, r.scene);
            if state != crate::engine::SlotState::Playing || beat < r.start - 0.05 {
                return;
            }
            let Some(at) = self.clip_position(r) else {
                return;
            };
            r.held.retain(|h| h.0 != pitch);
            r.held.push((pitch, at, velocity, beat));
            return;
        }
        let Some(i) = r.held.iter().position(|h| h.0 == pitch) else {
            return;
        };
        let (_, at, v, on) = r.held.remove(i);
        let r = r.clone();
        drop(guard);
        self.add_recorded(&r, pitch, at, v, (beat - on).max(0.05));
    }

    /// Where in the recorded clip the song is now, in clip beats.
    fn clip_position(&self, r: &Recording) -> Option<f64> {
        let (beats, start) = self.with_clip(r.track, r.scene, |c| (c.beats, c.loop_start))?;
        let p = self.monitor.progress(r.track) as f64;
        if r.new {
            Some((self.monitor.beat() - r.start).max(0.0))
        } else if p >= 0.0 {
            Some(start + p * beats)
        } else {
            None
        }
    }

    fn add_recorded(&self, r: &Recording, pitch: u8, at: f64, velocity: f32, length: f64) {
        let Ok(mut set) = self.set.lock() else {
            return;
        };
        let Some(Some(c)) = set.get_mut(r.track).and_then(|t| t.slots.get_mut(r.scene)) else {
            return;
        };
        c.notes.push(Note {
            start: at,
            length,
            pitch,
            velocity: velocity.clamp(0.0, 1.0),
            chance: 1.0,
            muted: false,
        });
        c.notes.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.send(Command::Program(Program::compile(&set)));
        drop(set);
        self.bump();
    }

    // ------------------------------------------------- mute and solo

    /// Whether the track's activator is off.
    pub fn muted(&self, track: usize) -> bool {
        self.set
            .lock()
            .ok()
            .and_then(|s| s.get(track).map(|t| t.muted))
            .unwrap_or(false)
    }

    pub fn soloed(&self, track: usize) -> bool {
        self.set
            .lock()
            .ok()
            .and_then(|s| s.get(track).map(|t| t.solo))
            .unwrap_or(false)
    }

    /// Whether any track is soloed.
    pub fn any_solo(&self) -> bool {
        self.set.lock().is_ok_and(|s| s.iter().any(|t| t.solo))
    }

    /// Switches the track's activator.
    pub fn toggle_mute(&self, track: usize) {
        self.edit_mixer(|set| {
            if let Some(t) = set.get_mut(track) {
                t.muted = !t.muted;
            }
        });
    }

    /// Solos the track or takes its solo off. `add` keeps the other solos;
    /// otherwise soloing a track takes theirs off.
    pub fn toggle_solo(&self, track: usize, add: bool) {
        self.edit_mixer(|set| {
            let Some(on) = set.get(track).map(|t| !t.solo) else {
                return;
            };
            if !add {
                for t in set.iter_mut() {
                    t.solo = false;
                }
            }
            set[track].solo = on;
        });
    }

    fn edit_mixer(&self, f: impl FnOnce(&mut Vec<Track>)) {
        let Ok(mut set) = self.set.lock() else {
            return;
        };
        self.checkpoint(&set, Edit::Mixer);
        f(&mut set);
        self.send(Command::Audible(crate::engine::audible(&set)));
        drop(set);
        self.bump();
    }

    // -------------------------------------------------------- clips

    /// The first empty slot on the track from `from` down.
    pub fn next_empty(&self, track: usize, from: usize) -> Option<usize> {
        let set = self.set.lock().ok()?;
        let slots = &set.get(track)?.slots;
        (from..slots.len()).find(|&s| slots[s].is_none())
    }

    /// Copies the clip in `from_track`'s slot `from` to `to_track`'s slot
    /// `to`, over what is there. Recordings only go to audio tracks, and
    /// note clips only to the others.
    pub fn copy_clip(&self, from_track: usize, from: usize, to_track: usize, to: usize) -> bool {
        if (from_track, from) == (to_track, to) {
            return false;
        }
        let Ok(mut set) = self.set.lock() else {
            return false;
        };
        let Some(clip) = set
            .get(from_track)
            .and_then(|t| t.slots.get(from).cloned().flatten())
        else {
            return false;
        };
        let fits = set.get(to_track).is_some_and(|t| {
            (t.sound == Sound::Audio) == clip.audio.is_some() && to < t.slots.len()
        });
        if !fits {
            return false;
        }
        self.checkpoint(&set, Edit::Other);
        set[to_track].slots[to] = Some(clip);
        self.send(Command::Program(Program::compile(&set)));
        drop(set);
        self.bump();
        true
    }

    /// Doubles a note clip's loop: what plays in it is copied after it,
    /// over any notes there.
    pub fn double_loop(&self, track: usize, scene: usize) -> bool {
        if !self
            .with_clip(track, scene, |c| c.stages.is_none() && c.audio.is_none())
            .unwrap_or(false)
        {
            return false;
        }
        self.edit_notes(track, scene, |c| {
            let (from, len) = (c.loop_start, c.beats);
            let (to, end) = (from + len, from + 2.0 * len);
            c.notes.retain(|n| n.start < to || n.start >= end);
            let copies: Vec<Note> = c
                .notes
                .iter()
                .filter(|n| n.start >= from && n.start < to)
                .map(|n| Note {
                    start: n.start + len,
                    ..*n
                })
                .collect();
            c.notes.extend(copies);
            c.beats = 2.0 * len;
        })
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
            self.record_note(track, pitch, velocity);
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

/// The chains, locked for an edit: each change goes to the list and to the
/// audio thread together.
pub struct ChainEdit<'a> {
    chains: &'a mut Vec<Vec<DeviceInfo>>,
    controls: &'a Controls,
}

impl ChainEdit<'_> {
    pub fn chain(&self, track: usize) -> &[DeviceInfo] {
        self.chains.get(track).map_or(&[], |c| c.as_slice())
    }

    /// Puts `device` in `track`'s chain at `at` (or its end); `slot` is
    /// what plays it, None for a plug-in that is still starting. Returns
    /// where it went.
    pub fn insert(
        &mut self,
        track: usize,
        at: usize,
        device: DeviceInfo,
        slot: Option<Box<crate::engine::PluginSlot>>,
    ) -> Option<usize> {
        let chain = self.chains.get_mut(track)?;
        let at = at.min(chain.len());
        chain.insert(at, device);
        self.controls.send(Command::Insert { track, at, slot });
        Some(at)
    }

    /// Takes the device at `at` out of `track`'s chain.
    pub fn remove(&mut self, track: usize, at: usize) -> Option<DeviceInfo> {
        let chain = self.chains.get_mut(track)?;
        if at >= chain.len() {
            return None;
        }
        let d = chain.remove(at);
        self.controls.send(Command::Remove { track, at });
        Some(d)
    }

    /// Swaps the device at `at` for `device` in the same place; `slot`
    /// plays it. Returns the one it replaced.
    pub fn replace(
        &mut self,
        track: usize,
        at: usize,
        device: DeviceInfo,
        slot: Option<Box<crate::engine::PluginSlot>>,
    ) -> Option<DeviceInfo> {
        let place = self.chains.get_mut(track)?.get_mut(at)?;
        let old = std::mem::replace(place, device);
        self.controls.send(Command::Replace { track, at, slot });
        Some(old)
    }

    /// Moves a device: to another place in its chain, or to `to_track`'s
    /// chain at `to`. `to` counts places once the device has left.
    pub fn move_device(&mut self, track: usize, from: usize, to_track: usize, to: usize) -> bool {
        if to_track >= self.chains.len() || from >= self.chain(track).len() {
            return false;
        }
        let d = self.chains[track].remove(from);
        let to = to.min(self.chains[to_track].len());
        self.chains[to_track].insert(to, d);
        if to_track == track {
            if to != from {
                self.controls.send(Command::Move { track, from, to });
            }
        } else {
            self.controls.send(Command::Transfer {
                from_track: track,
                from,
                to_track,
                to,
            });
        }
        true
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

    #[test]
    fn mute_solo_tap_tempo_and_clip_copies() {
        let mut set = crate::clip::demo_set();
        set[0].slots[6] = None;
        set[0].slots[7] = None;
        let (tx, mut rx) = rtrb::RingBuffer::new(64);
        let monitor = crate::engine::Studio::new(&set, 48_000.0, 120.0).1.monitor;
        let c = Controls::new(&set, &[0; 4], tx, monitor);
        c.toggle_mute(1);
        assert!(c.muted(1));
        assert!(matches!(rx.pop(), Ok(Command::Audible(0b1101))));
        c.toggle_solo(2, false);
        c.toggle_solo(3, true);
        assert!(c.soloed(2) && c.soloed(3) && c.any_solo());
        // Soloing alone takes the others' solos off.
        c.toggle_solo(0, false);
        assert!(c.soloed(0) && !c.soloed(2) && !c.soloed(3));
        assert!(c.snapshot()[1].muted && c.snapshot()[0].solo);

        // Taps half a second apart: 120 BPM.
        let t0 = std::time::Instant::now();
        let half = std::time::Duration::from_millis(500);
        assert_eq!(c.tap_tempo(t0), None);
        assert_eq!(c.tap_tempo(t0 + half).map(|b| b.round()), Some(120.0));
        assert_eq!(c.tap_tempo(t0 + half * 2).map(|b| b.round()), Some(120.0));
        // After a long pause the count starts again.
        assert_eq!(c.tap_tempo(t0 + half * 10), None);

        c.toggle_metronome();
        assert!(c.metronome());

        // Copy into the first empty slot, then double its loop.
        let (beats, notes) = c.notes(0, 0).expect("a note clip");
        let to = c.next_empty(0, 0).expect("an empty slot");
        assert!(c.copy_clip(0, 0, 0, to));
        assert!(!c.copy_clip(0, 0, 0, 0), "not onto itself");
        assert_eq!(
            c.notes(0, to).map(|(b, n)| (b, n.len())),
            Some((beats, notes.len()))
        );
        assert!(c.double_loop(0, to));
        let (b2, n2) = c.notes(0, to).expect("still a note clip");
        assert_eq!(b2, beats * 2.0);
        assert_eq!(n2.len(), notes.len() * 2);
        assert!(n2.iter().any(|n| n.start >= beats));
        assert!(!c.double_loop(0, c.next_empty(0, 0).expect("empty")));
    }

    /// A studio and its controls, with the audio thread run by hand.
    fn live(set: &[Track]) -> (crate::engine::Studio, Controls) {
        let (studio, r) = crate::engine::Studio::new(set, 48_000.0, 120.0);
        let c = Controls::new(set, &[0; 4], r.commands, r.monitor);
        (studio, c)
    }

    /// Runs the studio on until the song's beat is past `beat`.
    fn until(studio: &mut crate::engine::Studio, c: &Controls, beat: f64) {
        let mut out = vec![[0.0f32; 2]; 256];
        for _ in 0..20_000 {
            studio.process(&mut out);
            c.record_tick();
            if c.monitor.beat() >= beat {
                return;
            }
        }
        panic!("the song never got to beat {beat}");
    }

    #[test]
    fn recording_makes_a_clip_of_the_bars_played() {
        let mut set = crate::clip::demo_set();
        set[0].slots[7] = None;
        let (mut studio, c) = live(&set);
        assert!(c.record(0, 7));
        assert_eq!(c.recording(), Some((0, 7)));
        until(&mut studio, &c, 1.0);
        c.note(0, 60, 0.8);
        until(&mut studio, &c, 1.5);
        c.note(0, 60, 0.0);
        until(&mut studio, &c, 5.0);
        c.note(0, 64, 0.5);
        until(&mut studio, &c, 5.25);
        c.note(0, 64, 0.0);
        // Record again: it stops and the clip takes two bars.
        assert!(c.record(0, 7));
        assert_eq!(c.recording(), None);
        let (beats, notes) = c.notes(0, 7).expect("a note clip");
        assert_eq!(beats, 8.0);
        assert_eq!(notes.len(), 2);
        assert!((notes[0].start - 1.0).abs() < 0.05, "{:?}", notes[0]);
        assert!((notes[0].length - 0.5).abs() < 0.05, "{:?}", notes[0]);
        assert_eq!(notes[1].pitch, 64);
        assert!((notes[1].start - 5.0).abs() < 0.05);
        // Undo takes the recording away; redo brings it back.
        assert!(c.undo(false));
        assert!(!c.has_clip(0, 7));
        assert!(c.undo(true));
        assert_eq!(c.notes(0, 7).map(|n| n.1.len()), Some(2));
    }

    #[test]
    fn fixed_length_stops_the_recording_and_overdub_adds_notes() {
        let mut set = crate::clip::demo_set();
        set[0].slots[7] = None;
        let (mut studio, c) = live(&set);
        c.set_fixed_length(Some(1));
        assert!(c.record(0, 7));
        until(&mut studio, &c, 2.0);
        c.note(0, 62, 1.0);
        until(&mut studio, &c, 2.5);
        c.note(0, 62, 0.0);
        until(&mut studio, &c, 4.1);
        assert_eq!(c.recording(), None, "a bar in, it stopped");
        assert_eq!(c.notes(0, 7).map(|n| (n.0, n.1.len())), Some((4.0, 1)));
        // Overdub: the clip loops on, and a note played lands in it.
        assert!(c.record(0, 7));
        until(&mut studio, &c, 7.0);
        c.note(0, 67, 1.0);
        until(&mut studio, &c, 7.5);
        c.note(0, 67, 0.0);
        assert!(c.record(0, 7));
        let (beats, notes) = c.notes(0, 7).expect("still there");
        assert_eq!(beats, 4.0);
        let n = notes.iter().find(|n| n.pitch == 67).expect("overdubbed");
        assert!((n.start - 3.0).abs() < 0.05, "{n:?}");
    }

    #[test]
    fn undo_joins_quick_edits_to_the_same_clip() {
        let set = crate::clip::demo_set();
        let (_studio, c) = live(&set);
        let before = c.notes(0, 0).map(|n| n.1.len());
        for _ in 0..3 {
            c.edit_notes(0, 0, |clip| clip.notes.pop().map(|_| ()).unwrap_or(()));
        }
        c.toggle_mute(1);
        assert!(c.undo(false), "the mute");
        assert!(!c.muted(1));
        assert!(c.undo(false), "the three edits, as one");
        assert_eq!(c.notes(0, 0).map(|n| n.1.len()), before);
        assert!(!c.undo(false));
    }

    #[test]
    fn a_snapshot_keeps_the_mixer_and_knobs_and_opens_with_them() {
        let set = crate::clip::demo_set();
        let (tx, _rx) = rtrb::RingBuffer::new(64);
        let monitor = crate::engine::Studio::new(&set, 48_000.0, 120.0).1.monitor;
        let c = Controls::new(&set, &[0; 4], tx, monitor);
        c.set_mix(2, Mix::Volume, 0.4);
        c.set_mix(2, Mix::SendB, 0.6);
        c.set_param(1, 3, 0.25);
        c.edit_io(3, |io| io.armed = true);
        let saved = c.snapshot();
        assert_eq!(saved[2].volume, Some(0.4));
        assert_eq!(saved[2].sends, [0.0, 0.6]);
        assert_eq!(saved[1].params.map(|p| p[3]), Some(0.25));
        assert!(saved[3].io.armed);

        let (tx, _rx) = rtrb::RingBuffer::new(64);
        let monitor = crate::engine::Studio::new(&saved, 48_000.0, 120.0)
            .1
            .monitor;
        let again = Controls::new(&saved, &[0; 4], tx, monitor);
        assert_eq!(again.mix(2, Mix::SendB), 0.6);
        assert_eq!(again.param(1, 3), 0.25);
        assert!(again.io(3).armed);
    }
}
