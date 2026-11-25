//! The studio's audio engine: the Session grid as the audio thread plays it.
//!
//! Layers, after the architecture review in the add-opendj-studio change:
//!
//! - The UI holds the model ([`crate::clip::Track`]) and compiles it into a
//!   [`Program`]: plain clips, no sample rate, immutable once sent.
//! - The audio thread owns the [`Engine`]: the program, each track's
//!   instrument, and each track's playing state (playing clip, pending
//!   launch, sounding notes). Programs arrive over a wait-free queue and the
//!   old one goes back over a second queue to be dropped off the audio
//!   thread; playing state carries over.
//! - Launches, scenes and stops are small `Copy` commands on the same queue.
//!   They never touch undo.
//! - The engine reports to the UI through [`Monitor`]'s atomics only.
//!
//! Time is in beats (`f64`) on the shared clock: Ableton Link's session
//! beat when Link is on, else the studio's own clock, which computes each
//! block's beat from a frame count since its last tempo change rather than
//! adding block lengths up, so it never drifts. Launches wait for the next
//! launch point (bar, beat or none) and happen on the exact frame: a block
//! is split there.
//!
//! The callback never allocates, locks or does I/O, except for the plug-in
//! slots' hand-over to their helper processes (shared memory and one byte
//! down a pipe per block, as openDJ does).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendj_plugin_host::{Connection, Midi, Transport};

#[cfg(test)]
use crate::clip::TRACKS;
use crate::clip::{AudioTo, Clip, Io, Lock, Monitoring, Sound, Track, MAX_TRACKS, SCENES};
use crate::sound::{self, Drums, Instrument, Synth, PARAMS};
use opendj_engine::keyshift::KeyShifter;

/// Beats in a bar; also the Link quantum, so bars line up with openDJ's.
pub const QUANTUM: f64 = 4.0;

/// Where launches wait for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantize {
    Bar,
    Beat,
    Off,
}

impl Quantize {
    pub fn label(self) -> &'static str {
        match self {
            Quantize::Bar => "1 Bar",
            Quantize::Beat => "1/4",
            Quantize::Off => "None",
        }
    }

    pub fn next(self) -> Quantize {
        match self {
            Quantize::Bar => Quantize::Beat,
            Quantize::Beat => Quantize::Off,
            Quantize::Off => Quantize::Bar,
        }
    }

    fn grid(self) -> f64 {
        match self {
            Quantize::Bar => QUANTUM,
            Quantize::Beat => 1.0,
            Quantize::Off => 0.0,
        }
    }
}

/// The first launch point at or after `beat`.
pub fn next_launch(beat: f64, q: Quantize) -> f64 {
    let grid = q.grid();
    if grid <= 0.0 {
        return beat;
    }
    // A beat a hair past a grid line (from float rounding) counts as on it.
    let n = ((beat - 1e-9) / grid).ceil();
    n * grid
}

/// The clips the audio thread plays, compiled from the model.
pub struct Program {
    pub tracks: Vec<Vec<Option<Clip>>>,
}

impl Program {
    pub fn compile(tracks: &[Track]) -> Box<Program> {
        Box::new(Program {
            tracks: tracks.iter().map(|t| t.slots.clone()).collect(),
        })
    }

    fn clip(&self, track: usize, scene: usize) -> Option<&Clip> {
        self.tracks.get(track)?.get(scene)?.as_ref()
    }
}

/// From the UI to the audio thread.
pub enum Command {
    Launch {
        track: usize,
        scene: usize,
    },
    Scene(usize),
    Stop(usize),
    StopAll,
    Play(bool),
    Tempo(f64),
    Quantize(Quantize),
    /// A track's fader, as a linear gain.
    Volume {
        track: usize,
        gain: f32,
    },
    /// The main fader, as a linear gain.
    MainVolume(f32),
    /// A track's pan knob, 0 (left) to 1 (right).
    Pan {
        track: usize,
        pan: f32,
    },
    /// An instrument knob, 0..1 (see `sound::params`).
    Param {
        track: usize,
        index: usize,
        value: f32,
    },
    /// A note played live on a track's instrument; velocity 0 ends it.
    Note {
        track: usize,
        pitch: u8,
        velocity: f32,
    },
    /// A recompiled set, after an edit (clip editing comes in step 3).
    #[allow(dead_code)]
    Program(Box<Program>),
    /// A plug-in came up: it joins the track's chain.
    Attach {
        track: usize,
        slot: Box<PluginSlot>,
    },
    /// Switches a track's device (by its place in the chain) on or off.
    Bypass {
        track: usize,
        device: usize,
        on: bool,
    },
    /// One setting of a built-in device (see `devices::Settings::params`).
    DeviceParam {
        track: usize,
        device: usize,
        index: usize,
        value: f32,
    },
    /// A track's inputs and outputs.
    Route {
        track: usize,
        routing: Routing,
    },
    /// A MIDI message from a controller (not a note) for a track's
    /// plug-in instrument.
    Midi {
        track: usize,
        data: [u8; 3],
    },
}

/// A track's I/O as the audio thread uses it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Routing {
    pub audio_in: Option<(u16, bool)>,
    pub monitor: Monitoring,
    pub armed: bool,
    pub audio_to: AudioTo,
    /// Notes go out over MIDI in place of the track's own sound.
    pub midi_out: bool,
}

impl From<&Io> for Routing {
    fn from(io: &Io) -> Routing {
        Routing {
            audio_in: io.audio_in,
            monitor: io.monitor,
            armed: io.armed,
            audio_to: io.audio_to,
            midi_out: io.midi_to.is_some(),
        }
    }
}

/// A note (or all-notes-off) a track sends to its MIDI To port, on channel
/// 1: the port's thread puts it on the track's channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiOut {
    pub track: usize,
    pub data: [u8; 3],
}

/// The most sound card channels the studio takes in or sends out.
pub const MAX_CHANNELS: usize = 32;

/// A device in a track's chain, as the audio thread runs it: a plug-in or
/// one of the studio's own devices.
pub struct PluginSlot {
    /// Its place in the track's device chain.
    device: usize,
    /// Plays the track's notes in place of the built-in sound.
    instrument: bool,
    on: bool,
    kind: SlotKind,
}

enum SlotKind {
    Plugin(Connection),
    Builtin(crate::devices::Device),
}

impl PluginSlot {
    pub fn new(device: usize, instrument: bool, on: bool, conn: Connection) -> PluginSlot {
        PluginSlot {
            device,
            instrument,
            on,
            kind: SlotKind::Plugin(conn),
        }
    }

    fn builtin(device: usize, on: bool, d: crate::devices::Device) -> PluginSlot {
        PluginSlot {
            device,
            instrument: false,
            on,
            kind: SlotKind::Builtin(d),
        }
    }
}

/// Plug-ins a track can hold.
const MAX_SLOTS: usize = 16;
/// Note events a plug-in instrument gets per block.
const MAX_MIDI: usize = 256;

/// A slot as the UI shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    Empty = 0,
    Stopped = 1,
    Queued = 2,
    Playing = 3,
}

/// What the audio thread tells the UI.
pub struct Monitor {
    beat: AtomicU64,
    tempo: AtomicU64,
    playing: AtomicBool,
    slots: [AtomicU8; MAX_TRACKS * SCENES],
    progress: [AtomicU32; MAX_TRACKS],
    stopping: [AtomicBool; MAX_TRACKS],
    /// The stage each track's stage clip is on, 255 for none.
    stages: [AtomicU8; MAX_TRACKS],
    /// Each track's peak since the UI last read it, after its fader.
    peaks: [AtomicU32; MAX_TRACKS],
    /// The main output's peak since the UI last read it.
    main_peak: AtomicU32,
    /// Time spent rendering over the block's length, smoothed.
    load: AtomicU32,
}

impl Monitor {
    fn new() -> Monitor {
        Monitor {
            beat: AtomicU64::new(0),
            tempo: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            slots: std::array::from_fn(|_| AtomicU8::new(0)),
            progress: std::array::from_fn(|_| AtomicU32::new((-1f32).to_bits())),
            stopping: std::array::from_fn(|_| AtomicBool::new(false)),
            stages: std::array::from_fn(|_| AtomicU8::new(255)),
            peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            main_peak: AtomicU32::new(0),
            load: AtomicU32::new(0),
        }
    }

    pub fn beat(&self) -> f64 {
        f64::from_bits(self.beat.load(Ordering::Relaxed))
    }

    pub fn tempo(&self) -> f64 {
        f64::from_bits(self.tempo.load(Ordering::Relaxed))
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn slot(&self, track: usize, scene: usize) -> SlotState {
        match self.slots[scene * MAX_TRACKS + track].load(Ordering::Relaxed) {
            1 => SlotState::Stopped,
            2 => SlotState::Queued,
            3 => SlotState::Playing,
            _ => SlotState::Empty,
        }
    }

    /// How far through its clip `track` is (0..1), or -1 when stopped.
    pub fn progress(&self, track: usize) -> f32 {
        f32::from_bits(self.progress[track].load(Ordering::Relaxed))
    }

    pub fn stopping(&self, track: usize) -> bool {
        self.stopping[track].load(Ordering::Relaxed)
    }

    /// The stage `track`'s stage clip is playing.
    pub fn stage(&self, track: usize) -> Option<usize> {
        match self.stages.get(track)?.load(Ordering::Relaxed) {
            255 => None,
            s => Some(s as usize),
        }
    }

    /// `track`'s peak since the last call, and starts a new one.
    pub fn take_peak(&self, track: usize) -> f32 {
        f32::from_bits(self.peaks[track].swap(0, Ordering::Relaxed))
    }

    /// The main output's peak since the last call.
    pub fn take_main_peak(&self) -> f32 {
        f32::from_bits(self.main_peak.swap(0, Ordering::Relaxed))
    }

    pub fn load(&self) -> f32 {
        f32::from_bits(self.load.load(Ordering::Relaxed))
    }

    pub fn set_load(&self, load: f32) {
        self.load.store(load.to_bits(), Ordering::Relaxed);
    }
}

/// One block of the shared clock: its beat range and tempo.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub start: f64,
    pub end: f64,
    pub tempo: f64,
}

/// The studio's own clock, for when Link is off. A block's beat comes from
/// the frames since the last tempo change, so it never drifts.
pub struct Clock {
    rate: f64,
    tempo: f64,
    anchor_beat: f64,
    anchor_frame: u64,
    frame: u64,
}

impl Clock {
    pub fn new(rate: f64, tempo: f64) -> Clock {
        Clock {
            rate,
            tempo,
            anchor_beat: 0.0,
            anchor_frame: 0,
            frame: 0,
        }
    }

    fn beat_at(&self, frame: u64) -> f64 {
        self.anchor_beat + (frame - self.anchor_frame) as f64 * self.tempo / 60.0 / self.rate
    }

    /// Starts the clock at `beat` from now.
    pub fn locate(&mut self, beat: f64) {
        self.anchor_beat = beat;
        self.anchor_frame = self.frame;
    }

    pub fn set_tempo(&mut self, tempo: f64) {
        self.anchor_beat = self.beat_at(self.frame);
        self.anchor_frame = self.frame;
        self.tempo = tempo;
    }

    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// The next `frames` frames.
    pub fn advance(&mut self, frames: usize) -> Span {
        let start = self.beat_at(self.frame);
        self.frame += frames as u64;
        Span {
            start,
            end: self.beat_at(self.frame),
            tempo: self.tempo,
        }
    }
}

/// A launch or stop waiting for its launch point.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pending {
    /// The scene to play, or None to stop.
    scene: Option<usize>,
    at: f64,
}

const MAX_HELD: usize = 64;

#[derive(Clone, Copy)]
struct Held {
    pitch: u8,
    off: f64,
}

/// A track's playing state, which outlives programs.
struct Column {
    /// Scene playing and the beat it launched on.
    playing: Option<(usize, f64)>,
    pending: Option<Pending>,
    /// Notes sounding and the beat each ends on: the note tracker.
    held: [Held; MAX_HELD],
    held_len: usize,
    instrument: Instrument,
    gain: f32,
    /// Left and right gains from the pan knob.
    pan: [f32; 2],
    /// The instrument's knobs as the track has them, under any lock.
    base: [f32; PARAMS],
    /// Knobs a stage clip has locked away from `base`, by bit.
    locked: u16,
    /// For notes that play by chance.
    rng: u32,
    /// An audio track's player: recordings time-stretched to the beat.
    stretch: Option<Box<KeyShifter>>,
    /// The clip (scene) the player last played.
    audio_scene: Option<usize>,
    /// The player's output before its gain.
    audio_buf: Vec<[f32; 2]>,
    /// Plug-ins in chain order.
    // Boxed as they arrive, so taking one in never frees on this thread.
    #[allow(clippy::vec_box)]
    slots: Vec<Box<PluginSlot>>,
    /// Notes for a plug-in instrument in this block.
    midi: Vec<Midi>,
    /// Left and right in, left and right out, for the plug-ins.
    planar: Vec<f32>,
    routing: Routing,
}

impl Column {
    /// Whether a plug-in plays the track's notes (the built-in sound is
    /// then quiet).
    fn plugin_instrument(&self) -> bool {
        self.slots.iter().any(|s| s.instrument)
    }

    fn push_midi(&mut self, m: Midi) {
        if self.midi.len() < MAX_MIDI {
            self.midi.push(m);
        }
    }
}

impl Column {
    /// Puts locked knobs back where the track has them.
    fn unlock(&mut self) {
        for i in 0..PARAMS {
            if self.locked & (1 << i) != 0 {
                self.instrument.set_param(i, self.base[i]);
            }
        }
        self.locked = 0;
    }

    fn lock(&mut self, lock: &Lock) {
        for i in 0..PARAMS {
            match lock.values[i] {
                Some(v) => {
                    let b = self.base[i];
                    self.instrument.set_param(i, b + (v - b) * lock.amount);
                    self.locked |= 1 << i;
                }
                None if self.locked & (1 << i) != 0 => {
                    self.instrument.set_param(i, self.base[i]);
                    self.locked &= !(1 << i);
                }
                None => {}
            }
        }
    }

    /// 0..1, for a note's chance.
    fn roll(&mut self) -> f32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Off(u8),
    On(u8, f32),
    AllOff,
    /// The clip's lock point by index.
    Lock(u16),
    /// Locked knobs go back (a clip switch).
    Unlock,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    beat: f64,
    kind: Kind,
    /// The scene whose clip played the note, for the log.
    scene: u8,
}

/// A note event as played, for tests and `--check`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Logged {
    /// Frames since the engine started.
    pub frame: u64,
    pub track: usize,
    pub pitch: u8,
    pub on: bool,
    /// The scene of the clip that played it (255 for a note-off at a switch).
    pub scene: u8,
}

const MAX_EVENTS: usize = 1024;

/// The master level before the soft clipper: headroom for four tracks.
const MASTER: f32 = 0.7;

pub struct Engine {
    program: Box<Program>,
    columns: Vec<Column>,
    quantize: Quantize,
    playing: bool,
    /// Where the previous block ended, so blocks join up when Link nudges
    /// the beat.
    last_end: Option<f64>,
    events: Vec<Event>,
    scratch: Vec<[f32; 2]>,
    frames_done: u64,
    main_gain: f32,
    monitor: Arc<Monitor>,
    rate: f64,
    /// Programs replaced, on their way back to the UI thread to be dropped.
    trash: rtrb::Producer<Box<Program>>,
    log: Option<Vec<Logged>>,
    /// This block's sound card input, interleaved.
    input: Vec<f32>,
    in_channels: usize,
    /// Tracks sent to Ext. Out pairs, interleaved over every output.
    ext: Vec<f32>,
    out_channels: usize,
    /// Notes for tracks that play external instruments.
    midi_out: Option<rtrb::Producer<MidiOut>>,
}

/// Frames an audio clip fades in or out over where it starts or stops.
const FADE: usize = 128;

/// The largest block rendered in one go; longer callbacks are cut up.
pub const MAX_BLOCK: usize = 4096;

impl Engine {
    pub fn new(
        tracks: &[Track],
        rate: f64,
        monitor: Arc<Monitor>,
        trash: rtrb::Producer<Box<Program>>,
    ) -> Engine {
        let columns = tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    i,
                    t,
                    t.kit
                        .as_ref()
                        .map(|k| crate::sound::KitPlayer::new(k, rate)),
                )
            })
            .map(|(i, t, kit)| Column {
                playing: None,
                pending: None,
                held: [Held { pitch: 0, off: 0.0 }; MAX_HELD],
                held_len: 0,
                gain: t.volume.map_or(1.0, crate::control::fader_gain),
                pan: pan_gains(t.pan),
                base: sound::default_params(t.sound),
                locked: 0,
                stretch: (t.sound == Sound::Audio).then(|| Box::new(KeyShifter::new(rate))),
                audio_scene: None,
                audio_buf: if t.sound == Sound::Audio {
                    vec![[0.0; 2]; MAX_BLOCK]
                } else {
                    Vec::new()
                },
                slots: {
                    let mut slots = Vec::with_capacity(MAX_SLOTS);
                    // The studio's own devices are there from the start; the
                    // plug-ins join in chain order as they come up.
                    for (d, info) in t.devices.iter().enumerate() {
                        if let Some(b) = &info.builtin {
                            if slots.len() < MAX_SLOTS {
                                slots.push(Box::new(PluginSlot::builtin(
                                    d,
                                    info.on,
                                    crate::devices::Device::new(b, rate),
                                )));
                            }
                        }
                    }
                    slots
                },
                midi: Vec::with_capacity(MAX_MIDI),
                planar: if t.devices.iter().any(|d| d.plugin.is_some()) {
                    vec![0.0; 4 * MAX_BLOCK]
                } else {
                    Vec::new()
                },
                routing: Routing::from(&t.io),
                rng: 0x9E37_79B9 ^ (i as u32 + 1).wrapping_mul(0x85EB_CA6B),
                instrument: match t.sound {
                    // A set's Drum Rack plays its own samples when they loaded.
                    Sound::Drums(_) if kit.as_ref().is_some_and(|k| !k.is_empty()) => {
                        Instrument::Kit(Box::new(kit.expect("checked")))
                    }
                    Sound::Drums(kit) => Instrument::Drums(Box::new(Drums::new(kit, rate, 0.45))),
                    Sound::Bass => Instrument::Synth(Box::new(Synth::new(sound::BASS, rate))),
                    Sound::Pad => Instrument::Synth(Box::new(Synth::new(sound::PAD, rate))),
                    Sound::Lead => Instrument::Synth(Box::new(Synth::new(sound::LEAD, rate))),
                    Sound::Audio => Instrument::None,
                },
            })
            .collect();
        Engine {
            program: Program::compile(tracks),
            columns,
            quantize: Quantize::Bar,
            playing: false,
            last_end: None,
            events: Vec::with_capacity(MAX_EVENTS),
            scratch: vec![[0.0; 2]; MAX_BLOCK],
            frames_done: 0,
            main_gain: 1.0,
            monitor,
            rate,
            trash,
            log: None,
            input: Vec::new(),
            in_channels: 0,
            ext: Vec::new(),
            out_channels: 0,
            midi_out: None,
        }
    }

    /// Takes `channels` of sound card input per block (before the audio
    /// starts).
    pub fn set_inputs(&mut self, channels: usize) {
        self.in_channels = channels.min(MAX_CHANNELS);
        self.input = vec![0.0; MAX_BLOCK * self.in_channels];
    }

    /// Sends tracks routed to Ext. Out to `channels` outputs (before the
    /// audio starts).
    pub fn set_outputs(&mut self, channels: usize) {
        self.out_channels = channels.min(MAX_CHANNELS);
        self.ext = vec![0.0; MAX_BLOCK * self.out_channels];
    }

    /// The last block's Ext. Out sound, interleaved over every output.
    pub fn ext(&self, frames: usize) -> &[f32] {
        &self.ext[..(frames * self.out_channels).min(self.ext.len())]
    }

    /// Plays a note on a track: out over MIDI, on a plug-in instrument or
    /// on the built-in sound.
    fn play_note(&mut self, track: usize, pitch: u8, velocity: f32) {
        let Some(c) = self.columns.get_mut(track) else {
            return;
        };
        let v = (velocity.min(1.0) * 127.0).round() as u8;
        if c.routing.midi_out {
            if let Some(q) = self.midi_out.as_mut() {
                let data = if v > 0 {
                    [0x90, pitch, v.max(1)]
                } else {
                    [0x80, pitch, 0]
                };
                let _ = q.push(MidiOut { track, data });
            }
        } else if c.plugin_instrument() {
            c.push_midi(if v > 0 {
                Midi::note_on(0, 0, pitch, v.max(1))
            } else {
                Midi::note_off(0, 0, pitch)
            });
        } else if velocity > 0.0 {
            c.instrument.note_on(pitch, velocity.min(1.0));
        } else {
            c.instrument.note_off(pitch);
        }
    }

    /// Keeps every note event, up to `capacity`, for [`Engine::log`].
    pub fn keep_log(&mut self, capacity: usize) {
        self.log = Some(Vec::with_capacity(capacity));
    }

    pub fn log(&self) -> &[Logged] {
        self.log.as_deref().unwrap_or(&[])
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Applies a command. `now` is the beat the next block starts on.
    /// Returns a tempo or play change the clock must make.
    pub fn command(&mut self, cmd: Command, now: f64) -> Option<ClockChange> {
        match cmd {
            Command::Launch { track, scene } => {
                // An empty slot doesn't start the transport.
                if !self.playing && self.program.clip(track, scene).is_none() {
                    return None;
                }
                let start = self.ensure_playing(now);
                self.launch(track, Some(scene), start.unwrap_or(now));
                start.map(ClockChange::Start)
            }
            Command::Scene(scene) => {
                let start = self.ensure_playing(now);
                for t in 0..self.columns.len() {
                    self.launch(t, Some(scene), start.unwrap_or(now));
                }
                start.map(ClockChange::Start)
            }
            Command::Stop(track) => {
                if self.playing {
                    self.launch(track, None, now);
                }
                None
            }
            Command::StopAll => {
                if self.playing {
                    for t in 0..self.columns.len() {
                        self.launch(t, None, now);
                    }
                }
                None
            }
            Command::Play(on) => {
                if on == self.playing {
                    return None;
                }
                if on {
                    self.ensure_playing(now).map(ClockChange::Start)
                } else {
                    self.playing = false;
                    self.last_end = None;
                    for c in self.columns.iter_mut() {
                        c.playing = None;
                        c.pending = None;
                        c.held_len = 0;
                        c.instrument.all_off();
                        c.unlock();
                    }
                    Some(ClockChange::Stop)
                }
            }
            Command::Tempo(bpm) => Some(ClockChange::Tempo(bpm)),
            Command::Volume { track, gain } => {
                if let Some(c) = self.columns.get_mut(track) {
                    c.gain = gain.max(0.0);
                }
                None
            }
            Command::MainVolume(gain) => {
                self.main_gain = gain.max(0.0);
                None
            }
            Command::Pan { track, pan } => {
                if let Some(c) = self.columns.get_mut(track) {
                    c.pan = pan_gains(pan);
                }
                None
            }
            Command::Param {
                track,
                index,
                value,
            } => {
                if let Some(c) = self.columns.get_mut(track) {
                    if index < PARAMS {
                        c.base[index] = value;
                        // A locked knob takes it when the lock lets go.
                        if c.locked & (1 << index) == 0 {
                            c.instrument.set_param(index, value);
                        }
                    }
                }
                None
            }
            Command::Note {
                track,
                pitch,
                velocity,
            } => {
                self.play_note(track, pitch, velocity);
                None
            }
            Command::Quantize(q) => {
                self.quantize = q;
                None
            }
            Command::Program(p) => {
                let old = std::mem::replace(&mut self.program, p);
                // A clip whose slot emptied stops at the next launch point.
                for t in 0..self.columns.len() {
                    if let Some((scene, _)) = self.columns[t].playing {
                        if self.program.clip(t, scene).is_none() {
                            self.launch(t, None, now);
                        }
                    }
                }
                // Dropped on the UI thread; if its queue is full, here.
                let _ = self.trash.push(old);
                None
            }
            Command::Attach { track, slot } => {
                if let Some(c) = self.columns.get_mut(track) {
                    if c.slots.len() < MAX_SLOTS && !c.planar.is_empty() {
                        let at = c.slots.partition_point(|s| s.device < slot.device);
                        c.slots.insert(at, slot);
                    }
                }
                None
            }
            Command::Bypass { track, device, on } => {
                if let Some(c) = self.columns.get_mut(track) {
                    for s in c.slots.iter_mut().filter(|s| s.device == device) {
                        s.on = on;
                    }
                }
                None
            }
            Command::DeviceParam {
                track,
                device,
                index,
                value,
            } => {
                if let Some(c) = self.columns.get_mut(track) {
                    for s in c.slots.iter_mut().filter(|s| s.device == device) {
                        if let SlotKind::Builtin(d) = &mut s.kind {
                            d.set(index, value);
                        }
                    }
                }
                None
            }
            Command::Route { track, routing } => {
                if let Some(c) = self.columns.get_mut(track) {
                    let was = c.routing.midi_out;
                    c.routing = routing;
                    if routing.midi_out && !was {
                        c.instrument.all_off();
                        c.push_midi(Midi {
                            frame: 0,
                            data: [0xB0, 123, 0],
                        });
                    } else if was && !routing.midi_out {
                        if let Some(q) = self.midi_out.as_mut() {
                            let _ = q.push(MidiOut {
                                track,
                                data: [0xB0, 123, 0],
                            });
                        }
                    }
                }
                None
            }
            Command::Midi { track, data } => {
                if let Some(c) = self.columns.get_mut(track) {
                    if c.routing.midi_out {
                        if let Some(q) = self.midi_out.as_mut() {
                            let _ = q.push(MidiOut { track, data });
                        }
                    } else if c.plugin_instrument() {
                        c.push_midi(Midi { frame: 0, data });
                    }
                }
                None
            }
        }
    }

    /// Starts the transport if it's stopped; returns the beat it starts on.
    fn ensure_playing(&mut self, now: f64) -> Option<f64> {
        if self.playing {
            return None;
        }
        self.playing = true;
        self.last_end = None;
        // The studio's own clock starts on bar 1 at once; Link keeps its
        // own beat (the caller passes Link's beat as `now`).
        Some(now)
    }

    fn launch(&mut self, track: usize, scene: Option<usize>, now: f64) {
        let Some(c) = self.columns.get_mut(track) else {
            return;
        };
        if let Some(s) = scene {
            if self.program.clip(track, s).is_none() {
                // An empty slot stops its track, as a scene row does.
                if c.playing.is_none() && c.pending.is_none() {
                    return;
                }
                c.pending = Some(Pending {
                    scene: None,
                    at: next_launch(now, self.quantize),
                });
                return;
            }
        } else if c.playing.is_none() {
            c.pending = None;
            return;
        }
        c.pending = Some(Pending {
            scene,
            at: next_launch(now, self.quantize),
        });
    }

    /// Renders one block of `out.len()` frames over `span` of the clock,
    /// adding the tracks into `out`.
    pub fn render(&mut self, span: Span, out: &mut [[f32; 2]]) {
        let frames = out.len();
        // Join onto the previous block unless the clock jumped.
        let start = match self.last_end {
            Some(end) if (span.start - end).abs() < 0.25 => end,
            _ => span.start,
        };
        let end = span.end.max(start);
        self.last_end = Some(end);
        let beats_per_frame = if end > start {
            (end - start) / frames as f64
        } else {
            span.tempo / 60.0 / 48_000.0
        };
        // Plug-ins have until three quarters of the block's time is gone;
        // one that is later plays its block dry.
        let deadline = Instant::now() + Duration::from_secs_f64(frames as f64 / self.rate * 0.75);
        let transport = Transport {
            bpm: span.tempo,
            beat: start,
            playing: self.playing,
        };
        self.ext.fill(0.0);
        for t in 0..self.columns.len() {
            let routing = self.columns[t].routing;
            // Notes going out over MIDI leave the track's own sound quiet.
            let plugin = self.columns[t].plugin_instrument() || routing.midi_out;
            let mut switch = None;
            if self.playing {
                self.events.clear();
                switch = self.track_events(t, start, end);
                self.events.sort_unstable_by(|a, b| {
                    a.beat
                        .total_cmp(&b.beat)
                        .then_with(|| order(a.kind).cmp(&order(b.kind)))
                });
            } else {
                self.events.clear();
            }
            let scratch = &mut self.scratch[..frames];
            scratch.fill([0.0; 2]);
            let mut done = 0;
            for i in 0..self.events.len() {
                let e = self.events[i];
                let at = (((e.beat - start) / beats_per_frame).round() as usize).min(frames);
                if at > done {
                    if !plugin {
                        self.columns[t].instrument.render(&mut scratch[done..at]);
                    }
                    done = at;
                }
                let col = &mut self.columns[t];
                if routing.midi_out {
                    let data = match e.kind {
                        Kind::On(p, v) => Some([0x90, p, ((v * 127.0).round() as u8).max(1)]),
                        Kind::Off(p) => Some([0x80, p, 0]),
                        Kind::AllOff => Some([0xB0, 123, 0]),
                        _ => None,
                    };
                    if let (Some(data), Some(q)) = (data, self.midi_out.as_mut()) {
                        let _ = q.push(MidiOut { track: t, data });
                    }
                } else if plugin {
                    let f = at.min(frames.saturating_sub(1)) as u32;
                    match e.kind {
                        Kind::On(p, v) => col.push_midi(Midi::note_on(
                            f,
                            0,
                            p,
                            ((v * 127.0).round() as u8).max(1),
                        )),
                        Kind::Off(p) => col.push_midi(Midi::note_off(f, 0, p)),
                        // All notes off.
                        Kind::AllOff => col.push_midi(Midi {
                            frame: f,
                            data: [0xB0, 123, 0],
                        }),
                        _ => {}
                    }
                }
                match e.kind {
                    Kind::On(..) | Kind::Off(_) if routing.midi_out => {}
                    Kind::On(p, v) => col.instrument.note_on(p, v),
                    Kind::Off(p) => col.instrument.note_off(p),
                    Kind::AllOff => col.instrument.all_off(),
                    Kind::Unlock => col.unlock(),
                    Kind::Lock(i) => {
                        let lock = self
                            .program
                            .clip(t, e.scene as usize)
                            .and_then(|c| c.locks.get(i as usize));
                        if let Some(lock) = lock {
                            col.lock(lock);
                        }
                    }
                }
                if let (Some(log), Kind::On(p, _) | Kind::Off(p)) = (&mut self.log, e.kind) {
                    if log.len() < log.capacity() {
                        log.push(Logged {
                            frame: self.frames_done + at as u64,
                            track: t,
                            pitch: p,
                            on: matches!(e.kind, Kind::On(..)),
                            scene: e.scene,
                        });
                    }
                }
            }
            if done < frames && !plugin {
                self.columns[t].instrument.render(&mut scratch[done..]);
            }
            if self.columns[t].stretch.is_some() {
                let frame =
                    |beat: f64| (((beat - start) / beats_per_frame).round() as usize).min(frames);
                let now = if self.playing {
                    self.columns[t].playing
                } else {
                    None
                };
                match switch {
                    Some((at, before)) => {
                        let f = frame(at);
                        self.audio_segment(t, before, start, (0, f), span.tempo, (false, true));
                        self.audio_segment(t, now, at, (f, frames), span.tempo, (true, false));
                    }
                    None => {
                        self.audio_segment(t, now, start, (0, frames), span.tempo, (false, false))
                    }
                }
                let c = &self.columns[t];
                if c.audio_scene.is_some() {
                    for (o, a) in self.scratch[..frames]
                        .iter_mut()
                        .zip(&c.audio_buf[..frames])
                    {
                        o[0] += a[0];
                        o[1] += a[1];
                    }
                }
            }
            self.listen(t, frames);
            self.run_plugins(t, frames, transport, deadline);
            let gain = self.columns[t].gain;
            let [pl, pr] = self.columns[t].pan;
            let mut peak = 0.0f32;
            let ext = match routing.audio_to {
                AudioTo::Ext(first) if (first as usize) < self.out_channels => Some((
                    first as usize,
                    (first as usize + 1).min(self.out_channels - 1),
                )),
                _ => None,
            };
            let ch = self.out_channels;
            for (i, (o, s)) in out
                .iter_mut()
                .zip(self.scratch[..frames].iter())
                .enumerate()
            {
                let (l, r) = (s[0] * gain * pl, s[1] * gain * pr);
                peak = peak.max(l.abs()).max(r.abs());
                match (routing.audio_to, ext) {
                    (AudioTo::Main, _) => {
                        o[0] += l;
                        o[1] += r;
                    }
                    (_, Some((a, b))) => {
                        self.ext[i * ch + a] += l;
                        self.ext[i * ch + b] += r;
                    }
                    _ => {}
                }
            }
            // Positive floats order like their bits.
            self.monitor.peaks[t].fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
        let gain = MASTER * self.main_gain;
        let mut peak = 0.0f32;
        for o in out.iter_mut() {
            o[0] = soft_clip(o[0] * gain);
            o[1] = soft_clip(o[1] * gain);
            peak = peak.max(o[0].abs()).max(o[1].abs());
        }
        self.monitor
            .main_peak
            .fetch_max(peak.to_bits(), Ordering::Relaxed);
        self.frames_done += frames as u64;
        self.publish(end, span.tempo);
    }

    /// Adds what comes in to an audio track that is listening: with Monitor
    /// on In always (its clips then quiet), on Auto when it is armed and
    /// plays no clip.
    fn listen(&mut self, t: usize, frames: usize) {
        let c = &self.columns[t];
        if c.stretch.is_none() {
            return;
        }
        let r = c.routing;
        let clip = self.playing && c.playing.is_some();
        let listening = match r.monitor {
            Monitoring::In => true,
            Monitoring::Auto => r.armed && !clip,
            Monitoring::Off => false,
        };
        if !listening {
            return;
        }
        let scratch = &mut self.scratch[..frames];
        if r.monitor == Monitoring::In {
            scratch.fill([0.0; 2]);
        }
        let ch = self.in_channels;
        let Some((first, stereo)) = r.audio_in else {
            return;
        };
        let a = first as usize;
        if a >= ch {
            return;
        }
        let b = if stereo { (a + 1).min(ch - 1) } else { a };
        for (i, s) in scratch.iter_mut().enumerate() {
            s[0] += self.input[i * ch + a];
            s[1] += self.input[i * ch + b];
        }
    }

    /// Runs the track's sound through its plug-ins, in chain order: an
    /// instrument replaces it with what it plays, an effect processes it.
    fn run_plugins(&mut self, t: usize, frames: usize, transport: Transport, deadline: Instant) {
        let Column {
            slots,
            midi,
            planar,
            ..
        } = &mut self.columns[t];
        if slots.is_empty() {
            midi.clear();
            return;
        }
        let scratch = &mut self.scratch[..frames];
        // Plug-in buffers: only a track with plug-in devices has them.
        let quarter = planar.len() / 4;
        let (inl, rest) = planar.split_at_mut(quarter);
        let (inr, rest) = rest.split_at_mut(inl.len());
        let (outl, outr) = rest.split_at_mut(inl.len());
        for slot in slots.iter_mut() {
            let conn = match &mut slot.kind {
                SlotKind::Builtin(d) => {
                    if slot.on {
                        d.process(scratch, transport.bpm);
                    }
                    continue;
                }
                SlotKind::Plugin(_) if inl.len() < frames => continue,
                SlotKind::Plugin(conn) => conn,
            };
            if slot.instrument {
                inl[..frames].fill(0.0);
                inr[..frames].fill(0.0);
            } else if !slot.on {
                continue;
            } else {
                for (i, s) in scratch.iter().enumerate() {
                    inl[i] = s[0];
                    inr[i] = s[1];
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            let events: &[Midi] = if slot.instrument && slot.on {
                midi
            } else {
                &[]
            };
            conn.process_events(
                [&inl[..frames], &inr[..frames]],
                [&mut outl[..frames], &mut outr[..frames]],
                transport,
                events,
                left.max(Duration::from_micros(300)),
            );
            if slot.instrument && !slot.on {
                // Off: the track is quiet, as in Live.
                scratch.fill([0.0; 2]);
                continue;
            }
            for (i, s) in scratch.iter_mut().enumerate() {
                *s = [outl[i], outr[i]];
            }
        }
        midi.clear();
    }

    /// Collects `track`'s note events in [`lo`, `hi`) into `self.events`,
    /// switching clips at a pending launch point.
    /// Returns the switch, if there was one: its beat and the clip that
    /// played before it.
    fn track_events(&mut self, t: usize, lo: f64, hi: f64) -> Option<(f64, Option<(usize, f64)>)> {
        let mut from = lo;
        let mut switch = None;
        if let Some(p) = self.columns[t].pending {
            if p.at < hi {
                let at = p.at.max(lo);
                self.clip_events(t, from, at);
                // Every sounding note ends on the switch.
                let c = &mut self.columns[t];
                for h in &c.held[..c.held_len] {
                    push(&mut self.events, at, Kind::Off(h.pitch), 255);
                }
                c.held_len = 0;
                push(&mut self.events, at, Kind::AllOff, 255);
                push(&mut self.events, at, Kind::Unlock, 255);
                switch = Some((at, c.playing));
                c.playing = p.scene.map(|s| (s, at));
                c.pending = None;
                from = at;
            }
        }
        self.clip_events(t, from, hi);
        switch
    }

    /// Plays `t`'s audio clip over beats `from..to`, frames `f0..f1` of
    /// the block; `fade_in`/`fade_out` when the clip starts or stops there.
    #[allow(clippy::too_many_arguments)]
    fn audio_segment(
        &mut self,
        t: usize,
        playing: Option<(usize, f64)>,
        from: f64,
        frames: (usize, usize),
        tempo: f64,
        fades: (bool, bool),
    ) {
        let (f0, f1) = frames;
        let c = &mut self.columns[t];
        let Some(stretch) = c.stretch.as_mut() else {
            return;
        };
        let clip = playing.and_then(|(s, at)| Some((s, at, self.program.clip(t, s)?)));
        let Some((scene, launched, clip)) = clip else {
            stretch.stop();
            c.audio_scene = None;
            return;
        };
        let (Some(a), true) = (clip.audio.as_ref(), clip.beats > 0.0) else {
            return;
        };
        let Some(audio) = a.data.as_deref() else {
            return;
        };
        if f1 <= f0 {
            return;
        }
        if c.audio_scene != Some(scene) {
            stretch.stop();
            c.audio_scene = Some(scene);
        }
        let rate = audio.sample_rate as f64;
        let pos = (from - launched).max(0.0).rem_euclid(clip.beats);
        let (sec, per) = a.seconds(a.start + pos, tempo);
        let (lo, _) = a.seconds(a.start, tempo);
        let (hi, _) = a.seconds(a.start + clip.beats, tempo);
        let speed = (per * tempo / 60.0).clamp(0.25, 4.0);
        let buf = &mut c.audio_buf[f0..f1];
        buf.fill([0.0; 2]);
        stretch.render(
            audio,
            sec * rate,
            speed,
            rate / self.rate,
            1.0,
            Some((lo * rate, hi * rate)),
            buf,
        );
        let n = buf.len();
        let ramp = FADE.min(n);
        for (i, f) in buf.iter_mut().enumerate() {
            let mut g = a.gain;
            if fades.0 && i < ramp {
                g *= i as f32 / ramp as f32;
            }
            if fades.1 && i >= n - ramp {
                g *= (n - i) as f32 / ramp as f32;
            }
            f[0] *= g;
            f[1] *= g;
        }
    }

    fn clip_events(&mut self, t: usize, lo: f64, hi: f64) {
        if hi <= lo {
            return;
        }
        let c = &mut self.columns[t];
        // Note-offs due in this stretch.
        let mut i = 0;
        while i < c.held_len {
            if c.held[i].off < hi {
                push(
                    &mut self.events,
                    c.held[i].off.max(lo),
                    Kind::Off(c.held[i].pitch),
                    255,
                );
                c.held_len -= 1;
                c.held[i] = c.held[c.held_len];
            } else {
                i += 1;
            }
        }
        let Some((scene, launched)) = c.playing else {
            return;
        };
        let Some(clip) = self.program.clip(t, scene) else {
            return;
        };
        if clip.beats <= 0.0 {
            return;
        }
        let first = ((lo - launched) / clip.beats).floor().max(0.0) as i64;
        let last = ((hi - launched) / clip.beats).floor() as i64;
        for k in first..=last {
            let base = launched + k as f64 * clip.beats;
            for (i, l) in clip.locks.iter().enumerate() {
                let at = base + l.start;
                if at >= hi {
                    break;
                }
                if at >= lo {
                    push(&mut self.events, at, Kind::Lock(i as u16), scene as u8);
                }
            }
            for n in &clip.notes {
                let on = base + n.start;
                if on < lo {
                    continue;
                }
                if on >= hi {
                    break;
                }
                if n.chance < 1.0 && c.roll() >= n.chance {
                    continue;
                }
                // Notes are cut at the loop's end.
                let off = on + n.length.min(clip.beats - n.start);
                push(
                    &mut self.events,
                    on,
                    Kind::On(n.pitch, n.velocity),
                    scene as u8,
                );
                if off < hi {
                    push(&mut self.events, off, Kind::Off(n.pitch), scene as u8);
                } else if c.held_len < MAX_HELD {
                    c.held[c.held_len] = Held {
                        pitch: n.pitch,
                        off,
                    };
                    c.held_len += 1;
                }
            }
        }
    }

    fn publish(&self, beat: f64, tempo: f64) {
        let m = &self.monitor;
        m.beat.store(beat.to_bits(), Ordering::Relaxed);
        m.tempo.store(tempo.to_bits(), Ordering::Relaxed);
        m.playing.store(self.playing, Ordering::Relaxed);
        for (t, c) in self.columns.iter().enumerate() {
            for s in 0..SCENES {
                let state = if c.playing.map(|(p, _)| p) == Some(s) {
                    SlotState::Playing
                } else if c.pending.and_then(|p| p.scene) == Some(s) {
                    SlotState::Queued
                } else if self.program.clip(t, s).is_some() {
                    SlotState::Stopped
                } else {
                    SlotState::Empty
                };
                m.slots[s * MAX_TRACKS + t].store(state as u8, Ordering::Relaxed);
            }
            let progress = match c.playing {
                Some((s, at)) => match self.program.clip(t, s) {
                    Some(clip) if clip.beats > 0.0 && beat >= at => {
                        (((beat - at) / clip.beats).fract()) as f32
                    }
                    _ => 0.0,
                },
                None => -1.0,
            };
            m.progress[t].store(progress.to_bits(), Ordering::Relaxed);
            let stage = c
                .playing
                .and_then(|(s, at)| {
                    let clip = self.program.clip(t, s)?;
                    let pos = (beat - at).max(0.0).rem_euclid(clip.beats.max(1e-9));
                    clip.stage_starts
                        .iter()
                        .rev()
                        .find(|(start, _)| *start <= pos)
                        .map(|(_, i)| *i)
                })
                .unwrap_or(255);
            m.stages[t].store(stage, Ordering::Relaxed);
            m.stopping[t].store(
                c.pending.is_some_and(|p| p.scene.is_none()),
                Ordering::Relaxed,
            );
        }
    }
}

/// What the clock should do after a command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClockChange {
    /// The transport started on this beat.
    Start(f64),
    Stop,
    Tempo(f64),
}

fn push(events: &mut Vec<Event>, beat: f64, kind: Kind, scene: u8) {
    // Never grow on the audio thread: a block with more events than this
    // loses the rest.
    if events.len() < events.capacity() {
        events.push(Event { beat, kind, scene });
    }
}

/// Note-offs before all-offs before note-ons at the same beat, so a note
/// that ends where it starts again plays.
fn order(kind: Kind) -> u8 {
    match kind {
        Kind::Off(_) => 0,
        Kind::AllOff | Kind::Unlock => 1,
        Kind::Lock(_) => 2,
        Kind::On(..) => 3,
    }
}

#[inline]
/// Left and right gains for a pan knob at `pan` (0..1): equal power, and
/// unity in both channels at the centre.
fn pan_gains(pan: f32) -> [f32; 2] {
    let angle = pan.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let root2 = std::f32::consts::SQRT_2;
    [angle.cos() * root2, angle.sin() * root2]
}

fn soft_clip(x: f32) -> f32 {
    if x.abs() < 0.8 {
        x
    } else {
        x.signum() * (0.8 + 0.2 * ((x.abs() - 0.8) / 0.2).tanh())
    }
}

/// A leader's clock for one block: see [`Studio::process_following`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lead {
    /// The beat the block's first frame is heard on (bar 1 = beat 0, four
    /// beats a bar, as Link's quantum).
    pub beat: f64,
    /// Beats per minute over the block.
    pub tempo: f64,
}

/// The engine with its clock and command queue: what the audio callback
/// (or the offline check) runs.
pub struct Studio {
    pub engine: Engine,
    pub clock: Clock,
    pub link: Option<crate::link::LinkClock>,
    commands: rtrb::Consumer<Command>,
    pub monitor: Arc<Monitor>,
    rate: f64,
    /// The sound card input, interleaved, from its own stream.
    input: Option<rtrb::Consumer<f32>>,
}

/// The UI's ends of the queues.
pub struct Remote {
    pub commands: rtrb::Producer<Command>,
    pub trash: rtrb::Consumer<Box<Program>>,
    pub monitor: Arc<Monitor>,
    /// Notes for the tracks' MIDI To ports.
    pub midi_out: rtrb::Consumer<MidiOut>,
}

impl Studio {
    pub fn new(tracks: &[Track], rate: f64, tempo: f64) -> (Studio, Remote) {
        let monitor = Arc::new(Monitor::new());
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(256);
        // Room for a burst of edits from Push between UI timer ticks.
        let (trash_tx, trash_rx) = rtrb::RingBuffer::new(64);
        let mut engine = Engine::new(tracks, rate, monitor.clone(), trash_tx);
        let (midi_tx, midi_rx) = rtrb::RingBuffer::new(1024);
        engine.midi_out = Some(midi_tx);
        let mut clock = Clock::new(rate, tempo);
        clock.set_tempo(tempo);
        monitor.tempo.store(tempo.to_bits(), Ordering::Relaxed);
        (
            Studio {
                engine,
                clock,
                link: None,
                commands: cmd_rx,
                monitor: monitor.clone(),
                rate,
                input: None,
            },
            Remote {
                commands: cmd_tx,
                trash: trash_rx,
                monitor,
                midi_out: midi_rx,
            },
        )
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Takes `channels` of sound card input from `ring` (interleaved), as
    /// an input stream fills it.
    pub fn set_input(&mut self, ring: rtrb::Consumer<f32>, channels: usize) {
        self.engine.set_inputs(channels);
        self.input = Some(ring);
    }

    /// Takes no input (until [`Studio::set_input`]).
    pub fn clear_input(&mut self) {
        self.input = None;
        self.engine.in_channels = 0;
    }

    /// Moves a block of input from the ring to the engine. Short of input,
    /// the rest is silence; with more than a few blocks waiting, the oldest
    /// go, so the input never lags behind.
    fn take_input(&mut self, frames: usize) {
        let Some(ring) = self.input.as_mut() else {
            return;
        };
        let ch = self.engine.in_channels;
        if ch == 0 {
            return;
        }
        let want = frames * ch;
        let extra = ring.slots().saturating_sub(want * 4);
        // Whole frames only, so the channels stay in place.
        let extra = extra - extra % ch;
        if extra > 0 {
            if let Ok(c) = ring.read_chunk(extra) {
                c.commit_all();
            }
        }
        let n = ring.slots().min(want);
        let n = n - n % ch;
        let buf = &mut self.engine.input[..want];
        if let Ok(c) = ring.read_chunk(n) {
            let (a, b) = c.as_slices();
            buf[..a.len()].copy_from_slice(a);
            buf[a.len()..a.len() + b.len()].copy_from_slice(b);
            c.commit_all();
        }
        buf[n..].fill(0.0);
    }

    /// Renders `out` (stereo frames), cutting it into blocks of at most
    /// [`MAX_BLOCK`].
    pub fn process(&mut self, out: &mut [[f32; 2]]) {
        out.fill([0.0; 2]);
        for chunk in out.chunks_mut(MAX_BLOCK) {
            self.block(chunk);
        }
    }

    /// Renders `out` following a leader's clock (openDJ's master, for the
    /// Session deck) instead of the studio's own or Link: `lead` is the
    /// beat `out`'s first frame is heard on, and the tempo over it. The
    /// tempo commands the studio gets change nothing while it follows.
    pub fn process_following(&mut self, out: &mut [[f32; 2]], lead: Lead) {
        out.fill([0.0; 2]);
        let per_frame = lead.tempo / 60.0 / self.rate;
        let mut beat = lead.beat;
        for chunk in out.chunks_mut(MAX_BLOCK) {
            let end = beat + chunk.len() as f64 * per_frame;
            self.block_led(
                chunk,
                Some(Span {
                    start: beat,
                    end,
                    tempo: lead.tempo,
                }),
            );
            beat = end;
        }
    }

    fn block(&mut self, out: &mut [[f32; 2]]) {
        self.block_led(out, None);
    }

    /// One block, on `lead`'s beats when given, else Link's or our own.
    fn block_led(&mut self, out: &mut [[f32; 2]], lead: Option<Span>) {
        let frames = out.len();
        let led = lead.is_some();
        let linked = lead.or_else(|| self.link.as_mut().and_then(|l| l.begin(frames)));
        // Commands land at the start of the block.
        let now = match &linked {
            Some(s) => s.start,
            // Stopped, the own clock waits on bar 1.
            None if !self.engine.playing() => 0.0,
            None => self.clock.beat_now(),
        };
        while let Ok(cmd) = self.commands.pop() {
            match self.engine.command(cmd, now) {
                Some(ClockChange::Start(_)) => {
                    if linked.is_none() {
                        // Bar 1 starts now.
                        self.clock.locate(0.0);
                    }
                }
                Some(ClockChange::Stop) => {}
                Some(ClockChange::Tempo(_)) if led => {}
                Some(ClockChange::Tempo(bpm)) => {
                    let bpm = bpm.clamp(40.0, 250.0);
                    self.clock.set_tempo(bpm);
                    if let Some(l) = self.link.as_mut() {
                        l.set_tempo(bpm);
                    }
                }
                None => {}
            }
        }
        let span = match linked {
            Some(s) => {
                // Keep the own clock where Link is, for when Link goes off.
                self.clock.advance(frames);
                self.clock.sync(s.end, s.tempo);
                s
            }
            None => {
                if self.engine.playing() {
                    self.clock.advance(frames)
                } else {
                    let b = self.clock.beat_now();
                    Span {
                        start: b,
                        end: b,
                        tempo: self.clock.tempo(),
                    }
                }
            }
        };
        self.take_input(frames);
        self.engine.render(span, out);
    }
}

impl Clock {
    /// The beat the next block starts on.
    pub fn beat_now(&self) -> f64 {
        self.beat_at(self.frame)
    }

    /// Puts the clock on `beat` at `tempo` from now: after a block Link
    /// timed, so the clock carries on from Link's beat if Link goes off.
    fn sync(&mut self, beat: f64, tempo: f64) {
        self.tempo = tempo;
        self.anchor_frame = self.frame;
        self.anchor_beat = beat;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{Clip, Note};

    fn one_note_set() -> Vec<Track> {
        let clip = |name: &str, pitch: u8| {
            Some(Clip::new(
                name,
                4.0,
                vec![Note {
                    start: 0.0,
                    length: 0.5,
                    pitch,
                    velocity: 1.0,
                    chance: 1.0,
                }],
            ))
        };
        let mut slots = vec![None; SCENES];
        slots[0] = clip("a", 60);
        slots[1] = clip("b", 62);
        let mut tracks = vec![Track::new("Lead", Sound::Lead, slots)];
        for _ in 1..TRACKS {
            tracks.push(Track::new("Empty", Sound::Lead, vec![None; SCENES]));
        }
        tracks
    }

    const RATE: f64 = 48_000.0;
    const BLOCK: usize = 256;

    /// Runs `studio` for `blocks` blocks.
    fn run(studio: &mut Studio, blocks: usize) {
        let mut out = vec![[0.0; 2]; BLOCK];
        for _ in 0..blocks {
            studio.process(&mut out);
        }
    }

    fn frames_per_beat(tempo: f64) -> f64 {
        RATE * 60.0 / tempo
    }

    #[test]
    fn launch_points() {
        assert_eq!(next_launch(5.5, Quantize::Bar), 8.0);
        assert_eq!(next_launch(8.0, Quantize::Bar), 8.0);
        assert_eq!(next_launch(8.0 + 1e-12, Quantize::Bar), 8.0);
        assert_eq!(next_launch(5.5, Quantize::Beat), 6.0);
        assert_eq!(next_launch(5.5, Quantize::Off), 5.5);
    }

    #[test]
    fn first_launch_starts_at_once_and_loops() {
        let (mut s, mut r) = Studio::new(&one_note_set(), RATE, 120.0);
        s.engine.keep_log(64);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        // Three bars at 120 BPM = 6 seconds.
        run(&mut s, (6.0 * RATE) as usize / BLOCK + 1);
        let ons: Vec<u64> = s
            .engine
            .log()
            .iter()
            .filter(|e| e.on)
            .map(|e| e.frame)
            .collect();
        let bar = (4.0 * frames_per_beat(120.0)) as u64;
        assert_eq!(&ons[..3], &[0, bar, 2 * bar]);
    }

    #[test]
    fn following_a_leader_launches_on_its_bar() {
        let (mut s, mut r) = Studio::new(&one_note_set(), RATE, 120.0);
        s.engine.keep_log(64);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        // The leader is on beat 1.5 at 126 BPM; the studio's own 120 and a
        // tempo command are ignored.
        r.commands.push(Command::Tempo(90.0)).ok();
        let tempo = 126.0;
        let mut beat = 1.5;
        let mut out = vec![[0.0; 2]; BLOCK];
        for _ in 0..(8.0 * frames_per_beat(tempo)) as usize / BLOCK {
            s.process_following(&mut out, Lead { beat, tempo });
            beat += BLOCK as f64 * tempo / 60.0 / RATE;
        }
        let ons: Vec<u64> = s
            .engine
            .log()
            .iter()
            .filter(|e| e.on)
            .map(|e| e.frame)
            .collect();
        // Bar 2 (beat 4) is 2.5 beats in, then a bar later.
        let fpb = frames_per_beat(tempo);
        for (got, want) in ons.iter().zip([2.5 * fpb, 6.5 * fpb]) {
            assert!(got.abs_diff(want.round() as u64) <= 1, "{ons:?}");
        }
        assert_eq!(ons.len(), 2, "{ons:?}");
    }

    #[test]
    fn launch_waits_for_the_bar_to_the_sample() {
        let (mut s, mut r) = Studio::new(&one_note_set(), RATE, 124.0);
        s.engine.keep_log(64);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        let fpb = frames_per_beat(124.0);
        // Run to beat 5.5, then launch scene b.
        run(&mut s, (5.5 * fpb) as usize / BLOCK);
        r.commands.push(Command::Launch { track: 0, scene: 1 }).ok();
        run(&mut s, (4.0 * fpb) as usize / BLOCK);
        let b_on = s
            .engine
            .log()
            .iter()
            .find(|e| e.on && e.pitch == 62)
            .expect("b played")
            .frame;
        let want = (8.0 * fpb).round() as u64;
        assert!(b_on.abs_diff(want) <= 1, "{b_on} vs {want}");
        // a's note was not left hanging, and a stopped playing on the switch.
        assert_eq!(r.monitor.slot(0, 1), SlotState::Playing);
        assert_eq!(r.monitor.slot(0, 0), SlotState::Stopped);
    }

    #[test]
    fn stop_waits_for_the_bar_and_releases_notes() {
        let mut set = one_note_set();
        // A note held across the whole bar.
        set[0].slots[0].as_mut().unwrap().notes[0].length = 4.0;
        let (mut s, mut r) = Studio::new(&set, RATE, 120.0);
        s.engine.keep_log(64);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        run(&mut s, 10);
        r.commands.push(Command::Stop(0)).ok();
        run(&mut s, 2);
        assert!(r.monitor.stopping(0));
        run(&mut s, (4.0 * frames_per_beat(120.0)) as usize / BLOCK);
        assert!(!r.monitor.stopping(0));
        assert_eq!(r.monitor.progress(0), -1.0);
        let log = s.engine.log();
        let on = log.iter().filter(|e| e.on).count();
        let off = log.iter().filter(|e| !e.on).count();
        assert_eq!(on, 1);
        assert_eq!(off, 1, "the held note got its note-off");
    }

    #[test]
    fn scene_stops_tracks_with_an_empty_slot() {
        let mut set = one_note_set();
        set[1].slots[0] = set[0].slots[0].clone();
        let (mut s, mut r) = Studio::new(&set, RATE, 120.0);
        r.commands.push(Command::Scene(0)).ok();
        run(&mut s, 4);
        assert_eq!(r.monitor.slot(1, 0), SlotState::Playing);
        // Scene 2 has a clip on track 0 only.
        r.commands.push(Command::Scene(1)).ok();
        run(&mut s, (4.0 * frames_per_beat(120.0)) as usize / BLOCK + 1);
        assert_eq!(r.monitor.slot(0, 1), SlotState::Playing);
        assert_eq!(r.monitor.progress(1), -1.0, "track 2 stopped");
    }

    #[test]
    fn tempo_change_keeps_the_beat_continuous() {
        let mut c = Clock::new(RATE, 120.0);
        let a = c.advance(1000);
        c.set_tempo(90.0);
        let b = c.advance(1000);
        assert!((a.end - b.start).abs() < 1e-12);
        assert!((b.end - b.start - 1000.0 * 90.0 / 60.0 / RATE).abs() < 1e-12);
    }

    #[test]
    fn programs_swap_and_go_back_to_be_dropped() {
        let set = one_note_set();
        let (mut s, mut r) = Studio::new(&set, RATE, 120.0);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        run(&mut s, 4);
        // A new program without clip a: the track stops on the next bar.
        let mut edited = set.clone();
        edited[0].slots[0] = None;
        r.commands
            .push(Command::Program(Program::compile(&edited)))
            .ok();
        run(&mut s, 2);
        assert!(r.trash.pop().is_ok(), "old program came back");
        assert!(r.monitor.stopping(0));
    }

    #[test]
    fn stage_clips_lock_knobs_and_play_by_chance() {
        use crate::steps::{Stages, STAGES};
        let mut st = Stages::default();
        for s in st.stages.iter_mut() {
            s.gates = 1;
        }
        st.stages[1].locks[0] = Some(1.0);
        st.stages[2].chance = 0.0;
        let mut set = one_note_set();
        set[0].slots[0] = Some(st.clip("Steps", set[0].sound, &crate::scale::Key::default()));
        let (mut studio, mut r) = Studio::new(&set, 48_000.0, 120.0);
        studio.engine.keep_log(256);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        // Half a beat per stage at 120 BPM: into the last stage, once.
        run(
            &mut studio,
            (3.75 * frames_per_beat(120.0) / BLOCK as f64) as usize,
        );
        let ons = studio
            .engine
            .log()
            .iter()
            .filter(|l| l.on && l.track == 0)
            .count();
        // Every stage but the one at chance 0.
        assert_eq!(ons, STAGES - 1);
        let c = &studio.engine.columns[0];
        assert_eq!(c.locked, 0, "the lock let go after stage 2");
    }

    #[test]
    fn audio_clips_follow_the_tempo_and_keep_their_key() {
        use crate::clip::AudioClip;
        // Four seconds of 440 Hz, recorded at 120 BPM (half a second a beat).
        let rate = 48_000.0;
        let frames: Vec<[f32; 2]> = (0..4 * 48_000)
            .map(|i| {
                let v = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [v, v]
            })
            .collect();
        let mut clip = Clip::new("take", 8.0, Vec::new());
        clip.audio = Some(AudioClip {
            path: "take.wav".into(),
            data: Some(Arc::new(opendj_engine::track::Audio::from_frames(
                48_000, frames,
            ))),
            warp: vec![(0.0, 0.0), (1.0, 0.5)],
            start: 0.0,
            gain: 1.0,
        });
        let mut slots = vec![None; SCENES];
        slots[0] = Some(clip);
        let set = vec![Track::new("Mic", Sound::Audio, slots)];
        // Played at 100 BPM: slower, same pitch.
        let (mut studio, mut r) = Studio::new(&set, rate, 100.0);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        let mut out = vec![[0.0f32; 2]; BLOCK];
        let mut heard = Vec::new();
        for _ in 0..(rate as usize * 2 / BLOCK) {
            studio.process(&mut out);
            heard.extend(out.iter().map(|f| f[0]));
        }
        // The second second: steady, about 440 crossings up a second.
        let second = &heard[rate as usize..2 * rate as usize];
        let ups = second
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((425..=455).contains(&ups), "{ups} cycles a second");
        let loud = second.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(loud > 0.2, "peak {loud}");
        // Two seconds at 100 BPM is 3.33 beats of the clip's 8.
        assert!((studio.monitor.progress(0) - 3.33 / 8.0).abs() < 0.02);
    }

    #[test]
    fn input_is_heard_as_monitor_and_arm_say() {
        let set = vec![Track::new("Mic", Sound::Audio, vec![None; SCENES])];
        let (mut studio, mut r) = Studio::new(&set, 48_000.0, 120.0);
        let (mut tx, rx) = rtrb::RingBuffer::new(BLOCK * 2 * 8);
        studio.set_input(rx, 2);
        studio.engine.set_outputs(4);
        let mut out = vec![[0.0f32; 2]; BLOCK];
        // A block of input 1 at a quarter, input 2 silent; what comes out.
        let mut play = |studio: &mut Studio, routing: Option<Routing>| {
            if let Some(routing) = routing {
                r.commands.push(Command::Route { track: 0, routing }).ok();
            }
            for _ in 0..BLOCK {
                tx.push(0.25).ok();
                tx.push(0.0).ok();
            }
            studio.process(&mut out);
            let ext = studio.engine.ext(BLOCK);
            (
                out[BLOCK / 2],
                [ext[BLOCK / 2 * 4 + 2], ext[BLOCK / 2 * 4 + 3]],
            )
        };
        let base = Routing::from(&set[0].io);
        assert_eq!(base.monitor, Monitoring::Auto);
        let (main, _) = play(&mut studio, None);
        assert_eq!(main, [0.0, 0.0], "Auto and not armed: quiet");
        let armed = Routing {
            armed: true,
            ..base
        };
        let (main, _) = play(&mut studio, Some(armed));
        assert!(
            main[0] > 0.05 && (main[0] - main[1]).abs() < 1e-6,
            "{main:?}"
        );
        let off = Routing {
            monitor: Monitoring::Off,
            ..armed
        };
        assert_eq!(play(&mut studio, Some(off)).0, [0.0, 0.0]);
        let ext = Routing {
            monitor: Monitoring::In,
            armed: false,
            audio_to: AudioTo::Ext(2),
            ..base
        };
        let (main, ext) = play(&mut studio, Some(ext));
        assert_eq!(main, [0.0, 0.0], "sent to outputs 3/4, not Main");
        assert!(ext[0] > 0.05 && ext[1] > 0.05, "{ext:?}");
    }

    #[test]
    fn midi_to_sends_the_notes_out_and_leaves_the_track_quiet() {
        let mut set = one_note_set();
        set[0].io.midi_to = Some(("Synth".into(), 2));
        let (mut studio, mut r) = Studio::new(&set, 48_000.0, 120.0);
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        let mut out = vec![[0.0f32; 2]; BLOCK];
        let mut loud = 0.0f32;
        for _ in 0..40 {
            studio.process(&mut out);
            loud = out.iter().fold(loud, |m, f| m.max(f[0].abs()));
        }
        assert_eq!(loud, 0.0);
        let sent: Vec<MidiOut> = std::iter::from_fn(|| r.midi_out.pop().ok()).collect();
        assert!(
            sent.iter()
                .any(|m| m.track == 0 && m.data == [0x90, 60, 127]),
            "{sent:?}"
        );
    }

    /// The plug-in helper, built once next to the test binary.
    fn helper() -> std::path::PathBuf {
        static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        PATH.get_or_init(|| {
            let exe = std::env::current_exe().unwrap();
            let target = exe.parent().unwrap().parent().unwrap().parent().unwrap();
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            let ok = std::process::Command::new(cargo)
                .args([
                    "build",
                    "-q",
                    "-p",
                    "opendj-plugin-host",
                    "--bin",
                    "opendj-plugin-host",
                ])
                .arg("--target-dir")
                .arg(target)
                .status()
                .expect("cargo runs")
                .success();
            assert!(ok, "the helper builds");
            target.join("debug").join(format!(
                "opendj-plugin-host{}",
                std::env::consts::EXE_SUFFIX
            ))
        })
        .clone()
    }

    fn slot(
        name: &str,
        device: usize,
        instrument: bool,
    ) -> (Box<PluginSlot>, opendj_plugin_host::Helper) {
        let (conn, helper) = opendj_plugin_host::spawn(&opendj_plugin_host::Options {
            helper: helper(),
            plugin: opendj_plugin_host::Plugin::Test(name.into()),
            sample_rate: 48_000,
            max_frames: MAX_BLOCK,
            title: None,
            state: None,
            picks: Vec::new(),
        })
        .expect("the test plug-in starts");
        (
            Box::new(PluginSlot::new(device, instrument, true, conn)),
            helper,
        )
    }

    #[test]
    fn plugin_instruments_play_the_notes_and_effects_process_them() {
        let mut set = one_note_set();
        // A device list with plug-ins, so the track gets plug-in buffers.
        set[0].devices = vec![
            crate::clip::DeviceInfo {
                name: "Synth".into(),
                kind: "Test".into(),
                on: true,
                plugin: Some(crate::clip::PluginRef {
                    name: "Synth".into(),
                    vendor: String::new(),
                    format: crate::clip::PluginFormat::Named,
                    instrument: true,
                    state: None,
                }),
                builtin: None,
            };
            2
        ];
        let (mut studio, mut r) = Studio::new(&set, 48_000.0, 120.0);
        let (synth, _h1) = slot("test-synth", 0, true);
        let (gain, _h2) = slot("test-gain", 1, false);
        r.commands
            .push(Command::Attach {
                track: 0,
                slot: gain,
            })
            .ok();
        r.commands
            .push(Command::Attach {
                track: 0,
                slot: synth,
            })
            .ok();
        r.commands.push(Command::Launch { track: 0, scene: 0 }).ok();
        // The note (key 60) sounds for half a beat: the test synth plays
        // key/127 as a constant, through the gain at 1.
        let mut out = vec![[0.0f32; 2]; BLOCK];
        let want = soft_clip(60.0 / 127.0 * pan_gains(set[0].pan)[0] * MASTER);
        // A busy machine can make a helper miss a block (it plays dry):
        // allow a few.
        let mut level = 0.0;
        for _ in 0..20 {
            studio.process(&mut out);
            level = out[BLOCK / 2][0];
            if (level - want).abs() < 0.01 {
                break;
            }
        }
        assert!((level - want).abs() < 0.01, "{level} vs {want}");
        // The synth switched off: the track goes quiet.
        r.commands
            .push(Command::Bypass {
                track: 0,
                device: 0,
                on: false,
            })
            .ok();
        studio.process(&mut out);
        assert!(out.iter().all(|f| f[0].abs() < 1e-6));
    }
}
