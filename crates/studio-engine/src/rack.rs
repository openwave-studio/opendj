//! Racks: a device that holds several chains (branches) side by side.
//!
//! - An **Audio Effect Rack** sends its input into every branch and sums
//!   what comes out.
//! - An **Instrument Rack** plays the track's notes: each branch has an
//!   instrument and audio effects after it, and takes the notes inside its
//!   key and velocity zones (splits and layers).
//! - A **Drum Rack** is an instrument rack with one branch per pad: a
//!   branch takes one note, plays it as its own note (C3 by default) and
//!   cuts off the other pads in its choke group.
//!
//! In an instrument or drum rack, a branch's MIDI effects come before its
//! instrument and change the notes it plays, on the song's beat.
//!
//! Every branch renders into its own buffer, made before the audio starts,
//! so a rack never allocates while it plays. Eight macro knobs turn
//! settings of the devices inside it, each between a low and a high value.
//! A branch's structure (its devices) is fixed while it plays: adding or
//! removing a device inside a rack replaces the rack.

use opendj_plugin_host::shm::Midi;

use crate::clip::DeviceInfo;
use crate::devices::{from_db, Device, Param, Settings, Unit};
use crate::kit::Smooth;
use crate::midifx::{push as add, Note, NoteEvent};
use opendj_plugin_host::Transport;
use std::time::{Duration, Instant};

/// The most branches a rack holds.
pub const MAX_BRANCHES: usize = 128;
/// The most devices in a branch.
pub const MAX_DEVICES: usize = 16;
/// Macro knobs on every rack.
pub const MACROS: usize = 8;
/// Frames a branch renders at once; a longer block is split.
const CHUNK: usize = 512;
/// Note events a branch takes per chunk.
const MAX_NOTES: usize = 256;
/// Settings of each branch on the rack's own list (see `Settings::params`):
/// volume, pan, on, solo.
pub const BRANCH_PARAMS: usize = 4;

/// Where inner settings start: `INNER | branch << 14 | device << 9 | param`
/// names one setting of a device inside the rack.
pub const INNER: usize = 1 << 22;

/// The inner "setting" that switches a device inside on or off.
pub const ON: usize = 0x1FF;

/// The index [`RackSettings::set`] takes for a setting of a device inside.
pub fn inner(branch: usize, device: usize, param: usize) -> usize {
    INNER | (branch & 0xFF) << 14 | (device & 0x1F) << 9 | (param & 0x1FF)
}

/// (branch, device, param) of an inner index.
pub fn split(i: usize) -> Option<(usize, usize, usize)> {
    (i & INNER != 0).then_some(((i >> 14) & 0xFF, (i >> 9) & 0x1F, i & 0x1FF))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Audio,
    Instrument,
    Drum,
}

/// A chain inside a rack.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Branch {
    pub name: String,
    pub devices: Vec<DeviceInfo>,
    pub volume_db: f32,
    /// -1 left to 1 right.
    pub pan: f32,
    pub on: bool,
    pub solo: bool,
    /// Instrument Rack: the notes it takes, lowest and highest.
    pub keys: (u8, u8),
    /// Instrument Rack: the velocities it takes, 1..127.
    pub velocities: (u8, u8),
    /// Drum Rack: the note it takes.
    pub note: u8,
    /// Drum Rack: the note its instrument plays (C3 = 60).
    pub play: u8,
    /// Drum Rack: pads in the same group (not 0) cut each other off.
    pub choke: u8,
}

impl Default for Branch {
    fn default() -> Self {
        Branch {
            name: "Chain".into(),
            devices: Vec::new(),
            volume_db: 0.0,
            pan: 0.0,
            on: true,
            solo: false,
            keys: (0, 127),
            velocities: (1, 127),
            note: 36,
            play: 60,
            choke: 0,
        }
    }
}

/// One setting a macro turns.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Target {
    pub branch: u8,
    pub device: u8,
    pub param: u16,
    /// The setting at the macro's bottom and top.
    pub low: f32,
    pub high: f32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Macro {
    pub name: String,
    /// 0..1.
    pub value: f32,
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RackSettings {
    pub kind: Kind,
    pub branches: Vec<Branch>,
    pub macros: Vec<Macro>,
}

impl RackSettings {
    pub fn new(kind: Kind) -> RackSettings {
        RackSettings {
            kind,
            branches: Vec::new(),
            macros: (0..MACROS)
                .map(|i| Macro {
                    name: format!("Macro {}", i + 1),
                    value: 0.0,
                    targets: Vec::new(),
                })
                .collect(),
        }
    }

    pub fn summary(&self) -> String {
        let n = self.branches.len();
        match (self.kind, n) {
            (Kind::Drum, 1) => "1 pad".into(),
            (Kind::Drum, _) => format!("{n} pads"),
            (_, 1) => "1 chain".into(),
            _ => format!("{n} chains"),
        }
    }

    /// The macros, then each branch's volume, pan, on and solo.
    pub fn params(&self) -> Vec<Param> {
        let mut list: Vec<Param> = self
            .macros
            .iter()
            .map(|m| Param::knob(m.name.clone(), m.value, 0.0, 1.0, Unit::Percent))
            .collect();
        for b in &self.branches {
            list.push(Param::knob(
                format!("{} Volume", b.name),
                b.volume_db,
                -70.0,
                6.0,
                Unit::Db,
            ));
            list.push(
                Param::knob(format!("{} Pan", b.name), b.pan, -1.0, 1.0, Unit::Pan).bipolar(),
            );
            list.push(Param::toggle(format!("{} On", b.name), b.on));
            list.push(Param::toggle(format!("{} Solo", b.name), b.solo));
        }
        list
    }

    /// Changes a macro (moving its targets), a branch's setting, or one
    /// setting of a device inside (an [`inner`] index).
    pub fn set(&mut self, i: usize, v: f32) {
        if let Some((b, d, p)) = split(i) {
            let Some(dev) = self.branches.get_mut(b).and_then(|b| b.devices.get_mut(d)) else {
                return;
            };
            if p == ON {
                dev.on = v >= 0.5;
            } else if let Some(s) = dev.builtin.as_mut() {
                s.set(p, v);
            }
            return;
        }
        if i < self.macros.len() {
            let v = v.clamp(0.0, 1.0);
            self.macros[i].value = v;
            for t in self.macros[i].targets.clone() {
                let to = t.low + (t.high - t.low) * v;
                self.set(
                    inner(t.branch as usize, t.device as usize, t.param as usize),
                    to,
                );
            }
            return;
        }
        let k = i - self.macros.len();
        let Some(b) = self.branches.get_mut(k / BRANCH_PARAMS) else {
            return;
        };
        match k % BRANCH_PARAMS {
            0 => b.volume_db = v.clamp(-70.0, 6.0),
            1 => b.pan = v.clamp(-1.0, 1.0),
            2 => b.on = v >= 0.5,
            _ => b.solo = v >= 0.5,
        }
    }

    /// Puts a device in chain `b` at `at` (clamped to its end). An
    /// instrument takes the place of the chain's own. Returns where it went.
    pub fn insert(&mut self, b: usize, at: usize, d: DeviceInfo) -> Option<usize> {
        if !self.accepts(&d) {
            return None;
        }
        let chain = &mut self.branches.get_mut(b)?.devices;
        if chain.len() >= MAX_DEVICES {
            return None;
        }
        if is_instrument(&d) {
            if let Some(i) = chain.iter().position(is_instrument) {
                chain[i] = d;
                self.shift_targets(b, |x| if x == i { None } else { Some(x) });
                return Some(i);
            }
        }
        let (lo, hi) = zone(chain, &d);
        let at = at.clamp(lo, hi);
        chain.insert(at, d);
        self.shift_targets(b, |x| Some(if x >= at { x + 1 } else { x }));
        Some(at)
    }

    /// Takes device `d` out of chain `b`, with the macro mappings to it.
    pub fn remove(&mut self, b: usize, d: usize) -> Option<DeviceInfo> {
        let chain = &mut self.branches.get_mut(b)?.devices;
        if d >= chain.len() {
            return None;
        }
        let gone = chain.remove(d);
        self.shift_targets(b, |x| match x.cmp(&d) {
            std::cmp::Ordering::Less => Some(x),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(x - 1),
        });
        Some(gone)
    }

    /// Moves device `from` of chain `b` to `to` (its place once moved),
    /// keeping its macro mappings.
    pub fn move_device(&mut self, b: usize, from: usize, to: usize) -> bool {
        let Some(chain) = self.branches.get_mut(b).map(|b| &mut b.devices) else {
            return false;
        };
        if from >= chain.len() {
            return false;
        }
        let d = chain.remove(from);
        let (lo, hi) = zone(chain, &d);
        let to = to.clamp(lo, hi);
        chain.insert(to, d);
        self.shift_targets(b, |x| {
            Some(if x == from {
                to
            } else if from < to && x > from && x <= to {
                x - 1
            } else if to < from && x >= to && x < from {
                x + 1
            } else {
                x
            })
        });
        true
    }

    /// Whether `d` can go in a chain: the studio's own audio devices and
    /// effect plug-ins, and instruments (plug-ins too) and MIDI effects in
    /// a rack that plays notes. Racks inside racks come later.
    pub fn accepts(&self, d: &DeviceInfo) -> bool {
        let Some(s) = d.builtin.as_ref() else {
            return d
                .plugin
                .as_ref()
                .is_some_and(|p| !p.instrument || self.plays_notes());
        };
        if matches!(s, Settings::Rack(_) | Settings::External(_)) {
            return false;
        }
        !(s.plays_notes() || s.is_midi_effect()) || self.plays_notes()
    }

    /// A new chain with `d` in it, at the end. Returns its place.
    pub fn add_chain(&mut self, d: Option<DeviceInfo>) -> Option<usize> {
        if self.branches.len() >= MAX_BRANCHES || d.as_ref().is_some_and(|d| !self.accepts(d)) {
            return None;
        }
        let n = self.branches.len();
        let name = d
            .as_ref()
            .map_or_else(|| format!("Chain {}", n + 1), |d| d.name.clone());
        self.branches.push(Branch {
            name,
            devices: d.into_iter().collect(),
            note: (36 + n).min(127) as u8,
            ..Default::default()
        });
        Some(n)
    }

    /// Takes chain `b` out, with the macro mappings into it.
    pub fn remove_chain(&mut self, b: usize) -> Option<Branch> {
        if b >= self.branches.len() {
            return None;
        }
        let gone = self.branches.remove(b);
        for m in self.macros.iter_mut() {
            m.targets.retain(|t| t.branch as usize != b);
            for t in m.targets.iter_mut().filter(|t| t.branch as usize > b) {
                t.branch -= 1;
            }
        }
        Some(gone)
    }

    /// Maps setting `param` of device `d` in chain `b` to macro `m`, over
    /// the setting's whole range, or unmaps it if it was mapped. Returns
    /// whether it is mapped now.
    pub fn toggle_map(&mut self, m: usize, b: usize, d: usize, param: usize) -> bool {
        let Some(p) = self
            .branches
            .get(b)
            .and_then(|b| b.devices.get(d))
            .and_then(|d| d.builtin.as_ref())
            .and_then(|s| s.params().into_iter().nth(param))
        else {
            return false;
        };
        let Some(mac) = self.macros.get_mut(m) else {
            return false;
        };
        let at = |t: &Target| {
            t.branch as usize == b && t.device as usize == d && t.param as usize == param
        };
        if let Some(i) = mac.targets.iter().position(at) {
            mac.targets.remove(i);
            return false;
        }
        // A setting follows one macro.
        for other in self.macros.iter_mut() {
            other.targets.retain(|t| !at(t));
        }
        self.macros[m].targets.push(Target {
            branch: b as u8,
            device: d as u8,
            param: param as u16,
            low: p.min,
            high: p.max,
        });
        true
    }

    /// The macro a setting inside follows, if any.
    pub fn mapped(&self, b: usize, d: usize, param: usize) -> Option<usize> {
        self.macros.iter().position(|m| {
            m.targets.iter().any(|t| {
                t.branch as usize == b && t.device as usize == d && t.param as usize == param
            })
        })
    }

    /// Renumbers the macro targets on chain `b`'s devices (None drops one).
    fn shift_targets(&mut self, b: usize, f: impl Fn(usize) -> Option<usize>) {
        for m in self.macros.iter_mut() {
            m.targets.retain_mut(|t| {
                if t.branch as usize != b {
                    return true;
                }
                match f(t.device as usize) {
                    Some(d) => {
                        t.device = d as u8;
                        true
                    }
                    None => false,
                }
            });
        }
    }

    /// Whether it plays notes (an instrument or drum rack).
    pub fn plays_notes(&self) -> bool {
        self.kind != Kind::Audio
    }

    /// The branch a Drum Rack plays for `note`.
    pub fn pad(&self, note: u8) -> Option<usize> {
        self.branches.iter().position(|b| b.note == note)
    }
}

fn is_instrument(x: &DeviceInfo) -> bool {
    x.builtin.as_ref().is_some_and(Settings::plays_notes)
        || x.plugin.as_ref().is_some_and(|p| p.instrument)
}

fn is_midi_effect(x: &DeviceInfo) -> bool {
    x.builtin.as_ref().is_some_and(Settings::is_midi_effect)
}

/// The places `d` can go in `chain`, as in Live: MIDI effects, then the
/// instrument, then audio effects.
fn zone(chain: &[DeviceInfo], d: &DeviceInfo) -> (usize, usize) {
    let midi = chain.iter().take_while(|x| is_midi_effect(x)).count();
    let after = chain
        .iter()
        .rposition(is_instrument)
        .map_or(midi, |i| i + 1)
        .max(midi);
    if is_midi_effect(d) {
        (0, midi)
    } else if is_instrument(d) {
        (midi, midi)
    } else {
        (after, chain.len())
    }
}

/// A plug-in inside a rack. Its helper starts on its own thread; until it
/// joins, an effect passes the sound through and an instrument is quiet.
pub struct Hosted {
    pub instrument: bool,
    slot: Option<Box<crate::engine::PluginSlot>>,
    /// Its buffers, one channel each: in left, in right, out left, out
    /// right.
    planar: Vec<f32>,
}

impl Hosted {
    pub fn new(instrument: bool) -> Hosted {
        Hosted {
            instrument,
            slot: None,
            planar: vec![0.0; 4 * CHUNK],
        }
    }

    /// Runs one chunk (at most [`CHUNK`] frames) through the plug-in, an
    /// instrument playing `notes`. Off, an effect passes the sound and an
    /// instrument is quiet.
    fn run(
        &mut self,
        buf: &mut [[f32; 2]],
        notes: &[Midi],
        on: bool,
        transport: Transport,
        deadline: Instant,
    ) {
        let n = buf.len().min(CHUNK);
        let conn = match self.slot.as_mut().and_then(|s| s.conn_mut()) {
            Some(c) if on => c,
            _ => {
                if self.instrument {
                    buf.fill([0.0; 2]);
                }
                return;
            }
        };
        let (inl, rest) = self.planar.split_at_mut(CHUNK);
        let (inr, rest) = rest.split_at_mut(CHUNK);
        let (outl, outr) = rest.split_at_mut(CHUNK);
        for (i, f) in buf[..n].iter().enumerate() {
            let (l, r) = if self.instrument {
                (0.0, 0.0)
            } else {
                (f[0], f[1])
            };
            inl[i] = l;
            inr[i] = r;
        }
        let events: &[Midi] = if self.instrument { notes } else { &[] };
        let left = deadline.saturating_duration_since(Instant::now());
        conn.process_events(
            [&inl[..n], &inr[..n]],
            [&mut outl[..n], &mut outr[..n]],
            transport.for_plugins(),
            events,
            left.max(Duration::from_micros(300)),
        );
        for (i, f) in buf[..n].iter_mut().enumerate() {
            *f = [outl[i], outr[i]];
        }
    }
}

/// A branch as the audio thread runs it.
struct Run {
    devices: Vec<(bool, Device)>,
    buf: Vec<[f32; 2]>,
    notes: Vec<Midi>,
    gain: Smooth,
    pan: f32,
    on: bool,
    solo: bool,
    keys: (u8, u8),
    velocities: (u8, u8),
    note: u8,
    play: u8,
    choke: u8,
    /// Its MIDI effects' notes, on the beat: in and out.
    fx_in: Vec<NoteEvent>,
    fx_out: Vec<NoteEvent>,
    /// The beat its MIDI effects last ran up to (NaN: start afresh).
    fx_beat: f64,
    /// A MIDI effect was switched off: the instrument's notes end.
    flush: bool,
}

pub struct Rack {
    kind: Kind,
    runs: Vec<Run>,
    macros: Vec<Macro>,
    /// Which branches each key's note went to, so its note-off follows.
    held: [u128; 128],
    rate: f64,
    /// The song's beat at the start of the next block (see [`Rack::at_beat`]).
    beat: f64,
    /// Whether the song plays, and when this block must be done by: the
    /// plug-ins inside are told (see [`Rack::until`]).
    playing: bool,
    deadline: Option<Instant>,
}

impl Rack {
    pub fn new(s: &RackSettings, rate: f64) -> Rack {
        let runs = s
            .branches
            .iter()
            .take(MAX_BRANCHES)
            .map(|b| Run {
                devices: b
                    .devices
                    .iter()
                    .take(MAX_DEVICES)
                    .map(|d| match (&d.builtin, &d.plugin) {
                        (Some(b), _) => (d.on, Device::new(b, rate)),
                        // Its helper joins once it's up (see Rack::attach).
                        (None, Some(p)) => {
                            (d.on, Device::Plugin(Box::new(Hosted::new(p.instrument))))
                        }
                        // A device that doesn't play here: a silent stand-in
                        // keeps the places in step.
                        (None, None) => (
                            false,
                            Device::Utility(crate::devices::UtilitySettings::default()),
                        ),
                    })
                    .collect(),
                buf: vec![[0.0; 2]; CHUNK],
                notes: Vec::with_capacity(MAX_NOTES),
                gain: Smooth::knob(from_db(b.volume_db), rate),
                pan: b.pan,
                on: b.on,
                solo: b.solo,
                keys: b.keys,
                velocities: b.velocities,
                note: b.note,
                play: b.play,
                choke: b.choke,
                fx_in: Vec::with_capacity(MAX_NOTES),
                fx_out: Vec::with_capacity(MAX_NOTES),
                fx_beat: f64::NAN,
                flush: false,
            })
            .collect();
        Rack {
            kind: s.kind,
            runs,
            macros: s.macros.clone(),
            held: [0; 128],
            rate,
            beat: 0.0,
            playing: false,
            deadline: None,
        }
    }

    /// See [`RackSettings::set`]. Never allocates.
    pub fn set(&mut self, i: usize, v: f32) {
        if let Some((b, d, p)) = split(i) {
            if let Some(r) = self.runs.get_mut(b) {
                let Some((on, dev)) = r.devices.get_mut(d) else {
                    return;
                };
                if p == ON {
                    let was = *on;
                    *on = v >= 0.5;
                    // A MIDI effect switched off lets go of what it holds.
                    if was && !*on {
                        if let Some(fx) = dev.midi_mut() {
                            fx.reset();
                            r.flush = true;
                        }
                    }
                } else {
                    dev.set(p, v);
                }
            }
            return;
        }
        if i < self.macros.len() {
            let v = v.clamp(0.0, 1.0);
            self.macros[i].value = v;
            for k in 0..self.macros[i].targets.len() {
                let t = self.macros[i].targets[k];
                let to = t.low + (t.high - t.low) * v;
                if let Some((_, dev)) = self
                    .runs
                    .get_mut(t.branch as usize)
                    .and_then(|r| r.devices.get_mut(t.device as usize))
                {
                    dev.set(t.param as usize, to);
                }
            }
            return;
        }
        let k = i - self.macros.len();
        let Some(r) = self.runs.get_mut(k / BRANCH_PARAMS) else {
            return;
        };
        match k % BRANCH_PARAMS {
            0 => r.gain.set(from_db(v.clamp(-70.0, 6.0))),
            1 => r.pan = v.clamp(-1.0, 1.0),
            2 => r.on = v >= 0.5,
            _ => r.solo = v >= 0.5,
        }
    }

    /// Frames of delay inside: the slowest branch.
    pub fn latency(&self) -> usize {
        self.runs
            .iter()
            .map(|r| r.devices.iter().map(|(_, d)| d.latency()).sum::<usize>())
            .max()
            .unwrap_or(0)
    }

    pub fn plays_notes(&self) -> bool {
        self.kind != Kind::Audio
    }

    /// Audio Effect Rack: every branch processes `buf`, and their sum
    /// takes its place.
    pub fn process(&mut self, buf: &mut [[f32; 2]], tempo: f64) {
        if self.plays_notes() {
            return;
        }
        if self.runs.is_empty() {
            // No chains: the sound passes through, as in Live.
            return;
        }
        let per_frame = self.per_frame(tempo);
        for chunk in buf.chunks_mut(CHUNK) {
            let lo = self.beat;
            self.mix(chunk, tempo, lo, true);
            self.beat += chunk.len() as f64 * per_frame;
        }
    }

    fn per_frame(&self, tempo: f64) -> f64 {
        if tempo.is_finite() && tempo > 0.0 {
            tempo / 60.0 / self.rate
        } else {
            0.0
        }
    }

    /// Whether the song plays, and when the block must be done by: the
    /// plug-ins inside have until then.
    pub fn until(&mut self, playing: bool, deadline: Instant) {
        self.playing = playing;
        self.deadline = Some(deadline);
    }

    /// A plug-in inside came up: it takes its place in chain `b` at `d`.
    /// Gives the slot back when that place holds no plug-in waiting for
    /// one (the rack was edited while it started).
    pub fn attach(
        &mut self,
        b: usize,
        d: usize,
        slot: Box<crate::engine::PluginSlot>,
    ) -> Option<Box<crate::engine::PluginSlot>> {
        match self.runs.get_mut(b).and_then(|r| r.devices.get_mut(d)) {
            Some((_, Device::Plugin(h))) if h.slot.is_none() => {
                h.slot = Some(slot);
                None
            }
            _ => Some(slot),
        }
    }

    /// The song's beat at the start of the next block, for the MIDI
    /// effects inside. Without it the rack counts beats from where it was.
    /// Close to where the rack's own count got to, it keeps its own (no
    /// note is played twice or lost to rounding).
    pub fn at_beat(&mut self, beat: f64) {
        let near = (beat - self.beat).abs() < 0.25;
        if !near {
            self.beat = beat;
        }
    }

    /// Instrument or Drum Rack: plays the block's notes into `buf`.
    pub fn play(&mut self, midi: &[Midi], buf: &mut [[f32; 2]], tempo: f64) {
        buf.fill([0.0; 2]);
        if !self.plays_notes() {
            return;
        }
        let per_frame = self.per_frame(tempo);
        let mut from = 0;
        let mut at = 0;
        while at < buf.len() {
            let n = CHUNK.min(buf.len() - at);
            let end = at + n;
            let mut to = from;
            while to < midi.len() && (midi[to].frame as usize) < end {
                to += 1;
            }
            self.route(&midi[from..to], at as u32);
            let lo = self.beat + at as f64 * per_frame;
            for r in self.runs.iter_mut() {
                r.midi_effects(lo, per_frame, n, tempo);
            }
            self.mix(&mut buf[at..end], tempo, lo, false);
            from = to;
            at = end;
        }
        self.beat += buf.len() as f64 * per_frame;
    }

    /// Hands each branch the notes it takes, at frames from `offset`.
    fn route(&mut self, midi: &[Midi], offset: u32) {
        for r in self.runs.iter_mut() {
            r.notes.clear();
        }
        let drum = self.kind == Kind::Drum;
        for m in midi {
            let frame = m.frame.saturating_sub(offset);
            let status = m.data[0] & 0xF0;
            let key = m.data[1] & 127;
            let on = status == 0x90 && m.data[2] > 0;
            let off = status == 0x80 || (status == 0x90 && m.data[2] == 0);
            if !on && !off {
                // Controllers and the rest go to every branch.
                for r in self.runs.iter_mut() {
                    push(
                        &mut r.notes,
                        Midi {
                            frame,
                            data: m.data,
                        },
                    );
                }
                if status == 0xB0 && (m.data[1] == 123 || m.data[1] == 120) {
                    self.held = [0; 128];
                }
                continue;
            }
            if off {
                let mask = self.held[key as usize];
                self.held[key as usize] = 0;
                for (i, r) in self.runs.iter_mut().enumerate() {
                    if mask & (1 << i) != 0 {
                        let k = if drum { r.play } else { key };
                        push(
                            &mut r.notes,
                            Midi {
                                frame,
                                data: [m.data[0], k, m.data[2]],
                            },
                        );
                    }
                }
                continue;
            }
            let vel = m.data[2];
            let mut choke = 0;
            let mut mask = 0u128;
            for (i, r) in self.runs.iter_mut().enumerate() {
                let takes = if drum {
                    r.note == key
                } else {
                    (r.keys.0..=r.keys.1).contains(&key)
                        && (r.velocities.0..=r.velocities.1).contains(&vel)
                };
                if !takes {
                    continue;
                }
                mask |= 1 << i;
                let k = if drum { r.play } else { key };
                push(
                    &mut r.notes,
                    Midi {
                        frame,
                        data: [m.data[0], k, vel],
                    },
                );
                if drum && r.choke != 0 {
                    choke = r.choke;
                }
            }
            self.held[key as usize] |= mask;
            if choke != 0 {
                // The other pads of its group stop at once (fast fade).
                for (i, r) in self.runs.iter_mut().enumerate() {
                    if r.choke == choke && mask & (1 << i) == 0 {
                        push(
                            &mut r.notes,
                            Midi {
                                frame,
                                data: [0xB0, 120, 0],
                            },
                        );
                    }
                }
            }
        }
    }

    /// Runs every branch over `out`'s frames and sums them into it: from
    /// `out` itself for an effect rack, from the notes for an instrument.
    fn mix(&mut self, out: &mut [[f32; 2]], tempo: f64, beat: f64, effect: bool) {
        let n = out.len();
        let transport = Transport {
            bpm: tempo,
            beat,
            playing: self.playing,
        };
        // Each plug-in round trip gets what's left of the block's time.
        let deadline = self.deadline.unwrap_or_else(|| {
            Instant::now() + Duration::from_secs_f64(n as f64 / self.rate * 0.75)
        });
        let any_solo = self.runs.iter().any(|r| r.solo);
        let mut sum = [[0.0f32; 2]; CHUNK];
        let sum = &mut sum[..n];
        for r in self.runs.iter_mut() {
            let heard = r.on && (!any_solo || r.solo);
            let buf = &mut r.buf[..n];
            if effect {
                buf.copy_from_slice(out);
            } else {
                buf.fill([0.0; 2]);
            }
            // A branch that's off or not soloed still runs its instrument,
            // so its notes end and tails keep their state; it just isn't
            // heard.
            for (on, d) in r.devices.iter_mut() {
                if let Device::Plugin(h) = d {
                    h.run(buf, &r.notes, *on, transport, deadline);
                    continue;
                }
                if !*on {
                    continue;
                }
                if d.plays_notes() {
                    d.play(&r.notes, buf, tempo);
                } else {
                    d.process(buf, tempo);
                }
            }
            let (l, rr) = ((1.0 - r.pan).min(1.0), (1.0 + r.pan).min(1.0));
            for (s, f) in sum.iter_mut().zip(buf.iter()) {
                let g = r.gain.tick();
                if heard {
                    s[0] += f[0] * g * l;
                    s[1] += f[1] * g * rr;
                }
            }
        }
        out.copy_from_slice(sum);
    }

    /// How many chains it runs.
    #[cfg(test)]
    pub fn branches(&self) -> usize {
        self.runs.len()
    }
}

impl Run {
    /// Runs the branch's notes for `n` frames from beat `lo` through its
    /// MIDI effects (those that are on, in order), leaving what comes out
    /// in `notes`. Controllers other than All Notes Off pass by them.
    fn midi_effects(&mut self, lo: f64, per_frame: f64, n: usize, tempo: f64) {
        if self.flush {
            self.flush = false;
            if self.notes.len() == self.notes.capacity() {
                self.notes.pop();
            }
            self.notes.insert(
                0,
                Midi {
                    frame: 0,
                    data: [0xB0, 123, 0],
                },
            );
        }
        let any = self
            .devices
            .iter()
            .any(|(on, d)| *on && matches!(d, Device::Midi(_)));
        if !any || per_frame <= 0.0 {
            return;
        }
        let hi = lo + n as f64 * per_frame;
        self.fx_in.clear();
        // The song jumped (play started or stopped, a relocate): what
        // the effects hold ends.
        let joined = (lo - self.fx_beat).abs() < 1e-6;
        if !joined {
            add(&mut self.fx_in, lo, Note::AllOff);
        }
        self.fx_beat = hi;
        let mut kept = 0;
        for i in 0..self.notes.len() {
            let m = self.notes[i];
            let beat = lo + m.frame as f64 * per_frame;
            let status = m.data[0] & 0xF0;
            let note = match status {
                0x90 if m.data[2] > 0 => Note::On(m.data[1] & 127, m.data[2] as f32 / 127.0),
                0x80 | 0x90 => Note::Off(m.data[1] & 127),
                0xB0 if m.data[1] == 123 => Note::AllOff,
                _ => {
                    self.notes[kept] = m;
                    kept += 1;
                    continue;
                }
            };
            add(&mut self.fx_in, beat, note);
        }
        self.notes.truncate(kept);
        for (on, d) in self.devices.iter_mut() {
            if !*on {
                continue;
            }
            if let Some(fx) = d.midi_mut() {
                self.fx_out.clear();
                fx.run(&self.fx_in, &mut self.fx_out, hi, tempo);
                std::mem::swap(&mut self.fx_in, &mut self.fx_out);
            }
        }
        for e in &self.fx_in {
            let frame = ((e.beat - lo) / per_frame)
                .floor()
                .clamp(0.0, (n - 1) as f64) as u32;
            let data = match e.note {
                Note::On(p, v) => [0x90, p, (v * 127.0).round().clamp(1.0, 127.0) as u8],
                Note::Off(p) => [0x80, p, 0],
                Note::AllOff => [0xB0, 123, 0],
            };
            push(&mut self.notes, Midi { frame, data });
        }
        // By frame, keeping the order within one (an insertion sort: no
        // allocation, and the lists are short).
        for i in 1..self.notes.len() {
            let mut j = i;
            while j > 0 && self.notes[j - 1].frame > self.notes[j].frame {
                self.notes.swap(j - 1, j);
                j -= 1;
            }
        }
    }
}

/// Adds a note to a branch's list, dropping it if the list is full rather
/// than growing.
fn push(list: &mut Vec<Midi>, m: Midi) {
    if list.len() < list.capacity() {
        list.push(m);
    }
}

/// Puts a sample on a Drum Rack pad: the pad at `note`, else the first
/// free one from C1 (36). A pad that has a Simpler keeps its settings and
/// takes the new sample. Returns the pad's note, or None when it's full.
pub fn put_sample(
    rack: &mut RackSettings,
    note: Option<u8>,
    path: std::path::PathBuf,
    sample: std::sync::Arc<opendj_engine::sampler::Sample>,
) -> Option<u8> {
    use crate::simpler::SimplerSettings;
    let note = match note {
        Some(n) => n,
        None => (36u8..128).find(|n| rack.pad(*n).is_none())?,
    };
    let name = path
        .file_stem()
        .map_or("Pad".into(), |n| n.to_string_lossy().to_string());
    let i = match rack.pad(note) {
        Some(i) => i,
        None if rack.branches.len() < MAX_BRANCHES => {
            rack.branches.push(Branch {
                note,
                ..Default::default()
            });
            rack.branches.len() - 1
        }
        None => return None,
    };
    let b = &mut rack.branches[i];
    b.name = name;
    let simpler = b.devices.iter_mut().find_map(|d| match d.builtin.as_mut() {
        Some(Settings::Simpler(s)) => Some(s),
        _ => None,
    });
    match simpler {
        Some(s) => {
            s.path = Some(path);
            s.sample = Some(sample);
        }
        None => b.devices.insert(
            0,
            DeviceInfo {
                name: "Simpler".into(),
                kind: "Built-in".into(),
                on: true,
                plugin: None,
                builtin: Some(Settings::Simpler(SimplerSettings {
                    path: Some(path),
                    sample: Some(sample),
                    // Drums play to their end, as in Live.
                    mode: 1,
                    ..Default::default()
                })),
            },
        ),
    }
    Some(note)
}

/// A Drum Rack from a set's pads: each pad a Simpler in One-Shot mode.
pub fn drum_rack(kit: &crate::clip::Kit) -> RackSettings {
    use crate::simpler::SimplerSettings;
    let mut rack = RackSettings::new(Kind::Drum);
    for p in kit.pads.iter().take(MAX_BRANCHES) {
        let len = p.sample.as_ref().map_or(0.0, |s| s.seconds());
        let share = |t: f64| {
            if len > 0.0 {
                (t / len).clamp(0.0, 1.0) as f32
            } else {
                0.0
            }
        };
        let simpler = SimplerSettings {
            path: Some(p.path.clone()),
            sample: p.sample.clone(),
            mode: 1,
            root: 60,
            start: share(p.start),
            end: p.end.map_or(1.0, share),
            attack_ms: 0.1,
            release_ms: 50.0,
            transpose: p.transpose.round(),
            detune: (p.transpose - p.transpose.round()) * 100.0,
            volume_db: 20.0 * p.gain.max(1e-6).log10(),
            velocity: p.velocity,
            ..Default::default()
        };
        rack.branches.push(Branch {
            name: p.name.clone(),
            devices: vec![DeviceInfo {
                name: "Simpler".into(),
                kind: "Built-in".into(),
                on: true,
                plugin: None,
                builtin: Some(Settings::Simpler(simpler)),
            }],
            pan: p.pan,
            on: p.on,
            note: p.note,
            play: 60,
            choke: p.choke,
            ..Default::default()
        });
    }
    rack
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::UtilitySettings;
    use crate::simpler::SimplerSettings;
    use std::sync::Arc;

    const RATE: f64 = 48_000.0;

    fn sample(freq: f32) -> Arc<opendj_engine::sampler::Sample> {
        let frames: Vec<[f32; 2]> = (0..RATE as usize)
            .map(|i| {
                let v = (i as f32 * freq * std::f32::consts::TAU / RATE as f32).sin() * 0.5;
                [v, v]
            })
            .collect();
        let audio = opendj_engine::track::Audio::from_frames(RATE as u32, frames);
        Arc::new(opendj_engine::sampler::Sample::from_audio(&audio, RATE))
    }

    fn simpler(freq: f32, mode: u8) -> DeviceInfo {
        DeviceInfo {
            name: "Simpler".into(),
            kind: "Built-in".into(),
            on: true,
            plugin: None,
            builtin: Some(Settings::Simpler(SimplerSettings {
                sample: Some(sample(freq)),
                mode,
                volume_db: 0.0,
                velocity: 0.0,
                release_ms: 5.0,
                ..Default::default()
            })),
        }
    }

    fn utility(gain: f32) -> DeviceInfo {
        DeviceInfo {
            name: "Utility".into(),
            kind: "Built-in".into(),
            on: true,
            plugin: None,
            builtin: Some(Settings::Utility(UtilitySettings {
                gain,
                ..Default::default()
            })),
        }
    }

    fn level(buf: &[[f32; 2]]) -> f32 {
        buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()))
    }

    #[test]
    fn an_effect_rack_sums_its_branches() {
        let mut s = RackSettings::new(Kind::Audio);
        for g in [0.5, 0.25] {
            s.branches.push(Branch {
                devices: vec![utility(g)],
                ..Default::default()
            });
        }
        let mut r = Rack::new(&s, RATE);
        let mut buf = vec![[1.0f32; 2]; 1_000];
        r.process(&mut buf, 120.0);
        assert!((buf[999][0] - 0.75).abs() < 1e-4, "{}", buf[999][0]);
        // Muting one leaves the other.
        r.set(MACROS + 2, 0.0);
        let mut buf = vec![[1.0f32; 2]; 1_000];
        r.process(&mut buf, 120.0);
        assert!((buf[999][0] - 0.25).abs() < 1e-4, "{}", buf[999][0]);
        // Soloing the first leaves only it.
        r.set(MACROS + 2, 1.0);
        r.set(MACROS + 3, 1.0);
        let mut buf = vec![[1.0f32; 2]; 1_000];
        r.process(&mut buf, 120.0);
        assert!((buf[999][0] - 0.5).abs() < 1e-4, "{}", buf[999][0]);
    }

    #[test]
    fn an_empty_effect_rack_passes_the_sound() {
        let mut r = Rack::new(&RackSettings::new(Kind::Audio), RATE);
        let mut buf = vec![[0.3f32; 2]; 64];
        r.process(&mut buf, 120.0);
        assert!(buf.iter().all(|f| *f == [0.3; 2]));
    }

    #[test]
    fn instrument_zones_split_the_keyboard() {
        let mut s = RackSettings::new(Kind::Instrument);
        s.branches.push(Branch {
            devices: vec![simpler(220.0, 0)],
            keys: (0, 59),
            ..Default::default()
        });
        s.branches.push(Branch {
            devices: vec![simpler(220.0, 0)],
            keys: (60, 127),
            velocities: (100, 127),
            ..Default::default()
        });
        let mut r = Rack::new(&s, RATE);
        let mut buf = vec![[0.0f32; 2]; 2_048];
        // A high soft note: neither branch takes it.
        r.play(&[Midi::note_on(0, 0, 72, 50)], &mut buf, 120.0);
        assert_eq!(level(&buf), 0.0);
        // A low note: the first branch.
        r.play(&[Midi::note_on(0, 0, 48, 50)], &mut buf, 120.0);
        assert!(level(&buf[1_000..]) > 0.2);
        // Its note-off reaches it, whatever the zones.
        r.play(&[Midi::note_off(0, 0, 48)], &mut buf, 120.0);
        let mut buf = vec![[0.0f32; 2]; 4_096];
        r.play(&[], &mut buf, 120.0);
        assert!(level(&buf[2_000..]) < 1e-3);
    }

    #[test]
    fn a_macro_turns_settings_inside() {
        let mut s = RackSettings::new(Kind::Audio);
        s.branches.push(Branch {
            devices: vec![utility(1.0)],
            ..Default::default()
        });
        // Utility's gain (setting 0) from -24 dB to 0 dB.
        s.macros[0].targets.push(Target {
            branch: 0,
            device: 0,
            param: 0,
            low: -24.0,
            high: 0.0,
        });
        s.set(0, 0.25);
        let Some(Settings::Utility(u)) = &s.branches[0].devices[0].builtin else {
            panic!()
        };
        assert!((u.gain - from_db(-18.0)).abs() < 1e-6, "{}", u.gain);
        let mut r = Rack::new(&s, RATE);
        r.set(0, 0.5);
        let mut buf = vec![[1.0f32; 2]; 4_800];
        r.process(&mut buf, 120.0);
        let want = from_db(-12.0);
        assert!((buf[4_799][0] - want).abs() < 0.01, "{}", buf[4_799][0]);
    }

    #[test]
    fn drum_pads_play_their_note_and_choke_each_other() {
        let mut s = RackSettings::new(Kind::Drum);
        for (note, choke) in [(42, 1), (46, 1), (36, 0)] {
            s.branches.push(Branch {
                name: format!("{note}"),
                devices: vec![simpler(220.0, 1)],
                note,
                choke,
                ..Default::default()
            });
        }
        let mut r = Rack::new(&s, RATE);
        let mut buf = vec![[0.0f32; 2]; 2_048];
        // An open hat, then the closed hat chokes it.
        r.play(&[Midi::note_on(0, 0, 46, 127)], &mut buf, 120.0);
        assert!(level(&buf[1_000..]) > 0.2);
        let count = |r: &Rack| {
            r.runs
                .iter()
                .map(|x| match &x.devices[0].1 {
                    Device::Simpler(s) => s.sounding(),
                    _ => 0,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(count(&r), [0, 1, 0]);
        r.play(&[Midi::note_on(0, 0, 42, 127)], &mut buf, 120.0);
        assert_eq!(count(&r), [1, 0, 0]);
        // A note no pad takes plays nothing new.
        r.play(&[Midi::note_on(0, 0, 50, 127)], &mut buf, 120.0);
        assert_eq!(count(&r), [1, 0, 0]);
        // The kick isn't in the group.
        r.play(&[Midi::note_on(0, 0, 36, 127)], &mut buf, 120.0);
        assert_eq!(count(&r), [1, 0, 1]);
        assert_eq!(r.branches(), 3);
    }

    #[test]
    fn samples_fill_the_next_free_pad_or_the_one_named() {
        let mut r = RackSettings::new(Kind::Drum);
        let kick = sample(60.0);
        assert_eq!(
            put_sample(&mut r, None, "a/Kick.wav".into(), kick.clone()),
            Some(36)
        );
        assert_eq!(
            put_sample(&mut r, None, "a/Snare.wav".into(), kick.clone()),
            Some(37)
        );
        // Onto the kick's pad: it keeps its settings, the sample changes.
        if let Some(Settings::Simpler(s)) = r.branches[0].devices[0].builtin.as_mut() {
            s.transpose = 3.0;
        }
        assert_eq!(
            put_sample(&mut r, Some(36), "a/Kick 2.wav".into(), kick),
            Some(36)
        );
        assert_eq!(r.branches.len(), 2);
        assert_eq!(r.branches[0].name, "Kick 2");
        let Some(Settings::Simpler(s)) = &r.branches[0].devices[0].builtin else {
            panic!()
        };
        assert_eq!((s.transpose, s.mode), (3.0, 1));
    }

    #[test]
    fn chains_are_edited_and_mappings_follow_their_devices() {
        let mut r = RackSettings::new(Kind::Audio);
        assert_eq!(r.add_chain(Some(utility(1.0))), Some(0));
        assert_eq!(r.insert(0, 0, utility(0.5)), Some(0));
        // Utility 1.0 is now second; map its gain to macro 2.
        assert!(r.toggle_map(2, 0, 1, 0));
        assert_eq!(r.mapped(0, 1, 0), Some(2));
        // Mapping it to another macro moves it.
        assert!(r.toggle_map(3, 0, 1, 0));
        assert_eq!(r.mapped(0, 1, 0), Some(3));
        assert!(r.macros[2].targets.is_empty());
        // Moving it first takes its mapping along.
        assert!(r.move_device(0, 1, 0));
        assert_eq!(r.mapped(0, 0, 0), Some(3));
        assert_eq!(r.mapped(0, 1, 0), None);
        // Removing it drops the mapping.
        r.remove(0, 0);
        assert!(r.macros[3].targets.is_empty());
        // Toggling again unmaps.
        assert!(r.toggle_map(0, 0, 0, 0));
        assert!(!r.toggle_map(0, 0, 0, 0));
        // A device inside switches off.
        r.set(inner(0, 0, ON), 0.0);
        assert!(!r.branches[0].devices[0].on);
        let mut x = Rack::new(&r, RATE);
        let mut buf = vec![[0.3f32; 2]; 64];
        x.process(&mut buf, 120.0);
        assert!(
            (buf[63][0] - 0.3).abs() < 1e-6,
            "off still plays: {}",
            buf[63][0]
        );
        assert_eq!(r.remove_chain(0).map(|b| b.name), Some("Utility".into()));
    }

    #[test]
    fn an_instrument_takes_its_chains_first_place() {
        let mut r = RackSettings::new(Kind::Instrument);
        r.add_chain(None);
        assert_eq!(r.insert(0, 0, utility(1.0)), Some(0));
        // The instrument goes first; an effect dropped before it lands after.
        assert_eq!(r.insert(0, 5, simpler(220.0, 0)), Some(0));
        assert_eq!(r.insert(0, 0, utility(0.5)), Some(1));
        // A second instrument replaces the first.
        assert_eq!(r.insert(0, 2, simpler(440.0, 0)), Some(0));
        assert_eq!(r.branches[0].devices.len(), 3);
        // An effect rack plays no notes, and nothing takes a rack inside.
        let mut fx = RackSettings::new(Kind::Audio);
        assert_eq!(fx.add_chain(Some(simpler(220.0, 0))), None);
        fx.add_chain(None);
        assert_eq!(fx.insert(0, 0, simpler(220.0, 0)), None);
        let rack = DeviceInfo {
            builtin: Some(Settings::Rack(RackSettings::new(Kind::Audio))),
            ..utility(1.0)
        };
        assert_eq!(fx.insert(0, 0, rack), None);
    }

    #[test]
    fn inner_indices_round_trip() {
        assert_eq!(split(inner(12, 3, 17)), Some((12, 3, 17)));
        assert_eq!(split(5), None);
    }

    fn midi(name: &str, m: crate::midifx::MidiSettings) -> DeviceInfo {
        DeviceInfo {
            name: name.into(),
            kind: "Built-in".into(),
            on: true,
            plugin: None,
            builtin: Some(Settings::Midi(m)),
        }
    }

    /// Sign changes in the left channel: twice the pitch, twice as many.
    fn crossings(buf: &[[f32; 2]]) -> usize {
        buf.windows(2)
            .filter(|w| (w[0][0] >= 0.0) != (w[1][0] >= 0.0))
            .count()
    }

    #[test]
    fn midi_effects_go_first_in_a_chain_and_change_its_notes() {
        use crate::midifx::MidiSettings;
        let mut r = RackSettings::new(Kind::Instrument);
        r.add_chain(Some(simpler(220.0, 0)));
        // A MIDI effect dropped after the instrument still goes before it,
        // and an audio effect dropped first goes after it.
        assert_eq!(
            r.insert(0, 5, midi("Pitch", MidiSettings::Pitch { shift: 12 })),
            Some(0)
        );
        assert_eq!(r.insert(0, 0, utility(1.0)), Some(2));
        assert!(!r.move_device(0, 0, 2) || is_midi_effect(&r.branches[0].devices[0]));
        // Not in an effect rack.
        let mut fx = RackSettings::new(Kind::Audio);
        fx.add_chain(None);
        assert_eq!(
            fx.insert(0, 0, midi("Pitch", MidiSettings::Pitch { shift: 12 })),
            None
        );

        let play = |r: &RackSettings| {
            let mut rack = Rack::new(r, RATE);
            let mut buf = vec![[0.0f32; 2]; 4800];
            rack.play(&[Midi::note_on(0, 0, 60, 100)], &mut buf, 120.0);
            crossings(&buf[480..])
        };
        let up = play(&r);
        r.set(inner(0, 0, ON), 0.0);
        let plain = play(&r);
        // An octave up: about twice as many crossings.
        assert!(plain > 0, "the Simpler plays");
        let ratio = up as f32 / plain as f32;
        assert!((1.8..2.2).contains(&ratio), "{up} vs {plain}");
    }

    #[test]
    fn an_arpeggiator_in_a_chain_plays_steps_on_the_beat() {
        use crate::midifx::MidiSettings;
        let arp = crate::midifx::MidiSettings::catalog()
            .into_iter()
            .find(|(n, _)| *n == "Arpeggiator")
            .map(|(_, s)| s)
            .unwrap();
        assert!(matches!(arp, MidiSettings::Arpeggiator { .. }));
        let mut r = RackSettings::new(Kind::Instrument);
        r.add_chain(Some(simpler(220.0, 0)));
        r.insert(0, 0, midi("Arpeggiator", arp));
        let mut rack = Rack::new(&r, RATE);
        rack.at_beat(0.0);
        // Hold a chord for two beats at 120 BPM (one second), in 10 ms
        // blocks, counting where sound starts after a quiet block.
        let mut starts = 0;
        let mut quiet = true;
        for b in 0..(48_000 / 480) {
            let notes = if b == 0 {
                vec![Midi::note_on(0, 0, 60, 100), Midi::note_on(0, 0, 64, 100)]
            } else {
                vec![]
            };
            let mut buf = vec![[0.0f32; 2]; 480];
            rack.play(&notes, &mut buf, 120.0);
            let loud = buf.iter().any(|f| f[0].abs() > 0.01);
            if loud && quiet {
                starts += 1;
            }
            quiet = !loud;
        }
        // Steps of a quarter beat, each half as long: eight notes start in
        // two beats (a held chord without it would start once).
        assert!((7..=9).contains(&starts), "{starts}");
        // Its clock follows the song: it went on from beat 0 to beat 2.
        assert!((rack.beat - 2.0).abs() < 1e-6, "{}", rack.beat);
    }

    #[test]
    fn plugins_go_in_chains_where_their_kind_fits() {
        let plugin = |instrument: bool| DeviceInfo {
            name: "Plug".into(),
            kind: "VST3".into(),
            on: true,
            plugin: Some(crate::clip::PluginRef {
                name: "Plug".into(),
                vendor: String::new(),
                format: crate::clip::PluginFormat::Named,
                instrument,
                state: None,
            }),
            builtin: None,
        };
        let mut fx = RackSettings::new(Kind::Audio);
        fx.add_chain(None);
        assert_eq!(fx.insert(0, 0, plugin(false)), Some(0));
        assert_eq!(fx.insert(0, 0, plugin(true)), None);
        let mut inst = RackSettings::new(Kind::Instrument);
        inst.add_chain(Some(utility(1.0)));
        // A plug-in instrument takes the chain's first place.
        assert_eq!(inst.insert(0, 1, plugin(true)), Some(0));
        // Its place waits for its helper: an effect passes sound through.
        let rack = Rack::new(&fx, RATE);
        assert!(matches!(rack.runs[0].devices[0].1, Device::Plugin(_)));
    }
}
