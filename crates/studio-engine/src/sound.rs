//! The studio's built-in sounds: a drum kit from openDJ's Groove Box and a
//! small polyphonic synth.
//!
//! Both are made ready before the audio thread starts and never allocate
//! while playing: voices live in fixed arrays and are stolen when all are
//! busy.

use std::f32::consts::PI;
use std::sync::Arc;

use opendj_engine::kits;
use opendj_engine::sampler::Sample;

/// A track's instrument.
/// The built-in kit's sounds, in drum pad order.
pub const DRUM_NAMES: [&str; 9] = [
    "Kick",
    "Snare",
    "Closed hat",
    "Open hat",
    "Clap",
    "Rim",
    "Cowbell",
    "Crash",
    "Ride",
];

pub enum Instrument {
    Drums(Box<Drums>),
    /// A Drum Rack from a set, its own samples on its pads.
    Kit(Box<KitPlayer>),
    Synth(Box<Synth>),
    /// An audio track's: notes do nothing.
    None,
}

impl Instrument {
    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        match self {
            Instrument::Drums(d) => d.note_on(pitch, velocity),
            Instrument::Kit(k) => k.note_on(pitch, velocity),
            Instrument::Synth(s) => s.note_on(pitch, velocity),
            Instrument::None => {}
        }
    }

    pub fn note_off(&mut self, pitch: u8) {
        if let Instrument::Synth(s) = self {
            s.note_off(pitch);
        }
    }

    /// Releases every sounding note (a clip stopped).
    pub fn all_off(&mut self) {
        match self {
            Instrument::Drums(_) | Instrument::None => {}
            Instrument::Kit(k) => k.all_off(),
            Instrument::Synth(s) => s.all_off(),
        }
    }

    /// Sets knob `index` (see [`params`]) to `value`, 0..1.
    pub fn set_param(&mut self, index: usize, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match self {
            Instrument::Drums(d) => d.set_param(index, v),
            Instrument::Kit(k) => k.set_param(index, v),
            Instrument::Synth(s) => s.preset.set_param(index, v),
            Instrument::None => {}
        }
    }

    /// Adds `out.len()` frames of sound to `out`.
    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        match self {
            Instrument::Drums(d) => d.render(out),
            Instrument::Kit(k) => k.render(out),
            Instrument::Synth(s) => s.render(out),
            Instrument::None => {}
        }
    }
}

// --------------------------------------------------------------- knobs

/// Knobs per instrument: one page of Push's eight encoders.
pub const PARAMS: usize = 8;

pub const DRUM_PARAMS: [&str; PARAMS] = [
    "Volume", "Length", "Kick", "Snare", "Hats", "Claps", "Perc", "Cymbals",
];

pub const SYNTH_PARAMS: [&str; PARAMS] = [
    "Cutoff",
    "Resonance",
    "Env amount",
    "Sub",
    "Attack",
    "Decay",
    "Sustain",
    "Release",
];

/// The knob names for an instrument.
pub fn params(drums: bool) -> &'static [&'static str; PARAMS] {
    if drums {
        &DRUM_PARAMS
    } else {
        &SYNTH_PARAMS
    }
}

/// 0..1 on a log scale between `lo` and `hi`.
fn log_to(v: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(v)
}

fn log_from(x: f32, lo: f32, hi: f32) -> f32 {
    ((x.max(lo) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0)
}

const CUTOFF: (f32, f32) = (40.0, 12_000.0);
const RESONANCE: f32 = 0.95;
const ENV_AMOUNT: f32 = 8000.0;
const ATTACK: (f32, f32) = (0.001, 2.0);
const DECAY: (f32, f32) = (0.01, 4.0);

/// A drum level knob: 0.5 is as the kit was made, 1 is +12 dB.
fn level(v: f32) -> f32 {
    (v * 2.0) * (v * 2.0)
}

fn db(gain: f32) -> String {
    if gain <= 1e-4 {
        "-inf dB".into()
    } else {
        format!("{:+.1} dB", 20.0 * gain.log10())
    }
}

fn seconds(s: f32) -> String {
    if s < 1.0 {
        format!("{:.0} ms", s * 1000.0)
    } else {
        format!("{s:.2} s")
    }
}

/// How knob `index` at `value` reads on screen.
pub fn param_text(drums: bool, index: usize, value: f32) -> String {
    let v = value.clamp(0.0, 1.0);
    let pct = || format!("{:.0}%", v * 100.0);
    if drums {
        return match index {
            1 => pct(),
            _ => db(level(v)),
        };
    }
    match index {
        0 => {
            let hz = log_to(v, CUTOFF.0, CUTOFF.1);
            if hz >= 1000.0 {
                format!("{:.1} kHz", hz / 1000.0)
            } else {
                format!("{hz:.0} Hz")
            }
        }
        2 => format!("{:.0} Hz", v * v * ENV_AMOUNT),
        4 => seconds(log_to(v, ATTACK.0, ATTACK.1)),
        5 | 7 => seconds(log_to(v, DECAY.0, DECAY.1)),
        _ => pct(),
    }
}

/// The knobs' starting positions for a track's sound.
pub fn default_params(sound: crate::clip::Sound) -> [f32; PARAMS] {
    use crate::clip::Sound;
    let preset = match sound {
        Sound::Drums(_) => return [0.5, 1.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
        Sound::Bass => BASS,
        Sound::Pad => PAD,
        Sound::Lead | Sound::Audio => LEAD,
    };
    preset.params()
}

impl Preset {
    fn params(&self) -> [f32; PARAMS] {
        [
            log_from(self.cutoff, CUTOFF.0, CUTOFF.1),
            (self.resonance / RESONANCE).clamp(0.0, 1.0),
            (self.env_amount / ENV_AMOUNT).clamp(0.0, 1.0).sqrt(),
            self.sub.clamp(0.0, 1.0),
            log_from(self.attack, ATTACK.0, ATTACK.1),
            log_from(self.decay, DECAY.0, DECAY.1),
            self.sustain.clamp(0.0, 1.0),
            log_from(self.release, DECAY.0, DECAY.1),
        ]
    }

    fn set_param(&mut self, index: usize, v: f32) {
        match index {
            0 => self.cutoff = log_to(v, CUTOFF.0, CUTOFF.1),
            1 => self.resonance = v * RESONANCE,
            2 => self.env_amount = v * v * ENV_AMOUNT,
            3 => self.sub = v,
            4 => self.attack = log_to(v, ATTACK.0, ATTACK.1),
            5 => self.decay = log_to(v, DECAY.0, DECAY.1),
            6 => self.sustain = v,
            7 => self.release = log_to(v, DECAY.0, DECAY.1),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- drums

/// General MIDI drum notes for the kit's nine roles (see [`kits::ROLES`]):
/// kick, snare, closed hat, open hat, clap, rim, cowbell, crash, ride.
pub const DRUM_NOTES: [u8; 9] = [36, 38, 42, 46, 39, 37, 56, 49, 51];

pub const KICK: u8 = 36;
pub const SNARE: u8 = 38;
pub const CLOSED_HAT: u8 = 42;
pub const OPEN_HAT: u8 = 46;
pub const CLAP: u8 = 39;
pub const RIM: u8 = 37;

const DRUM_VOICES: usize = 24;

#[derive(Clone, Copy, Default)]
struct DrumVoice {
    sound: usize,
    pos: usize,
    gain: f32,
    /// Fading out after a choke (open hat cut by a closed hat).
    fade: f32,
    active: bool,
}

/// One Groove Box kit, played by General MIDI drum notes.
pub struct Drums {
    sounds: Vec<Arc<Sample>>,
    voices: [DrumVoice; DRUM_VOICES],
    gain: f32,
    /// The Volume knob, as a gain.
    volume: f32,
    /// How much of each sound plays before it fades, 0..1.
    length: f32,
    /// Level knobs: kick, snare, hats, claps, perc, cymbals.
    levels: [f32; 6],
}

/// Which level knob each of [`DRUM_NOTES`] follows.
const DRUM_GROUP: [usize; 9] = [0, 1, 2, 2, 3, 4, 4, 5, 5];

impl Drums {
    /// Kit `name` (one of [`kits::KITS`]) at the engine's `rate`.
    pub fn new(name: &str, rate: f64, gain: f32) -> Drums {
        let all = kits::build(rate);
        let kit = all
            .into_iter()
            .find(|k| k.name == name)
            .expect("kit exists");
        Drums {
            sounds: kit.sounds.iter().map(|s| s.sample.clone()).collect(),
            voices: [DrumVoice::default(); DRUM_VOICES],
            gain,
            volume: 1.0,
            length: 1.0,
            levels: [1.0; 6],
        }
    }

    fn set_param(&mut self, index: usize, v: f32) {
        match index {
            0 => self.volume = level(v),
            1 => self.length = v,
            2..=7 => self.levels[index - 2] = level(v),
            _ => {}
        }
    }

    fn note_on(&mut self, pitch: u8, velocity: f32) {
        let Some(sound) = DRUM_NOTES.iter().position(|&n| n == pitch) else {
            return;
        };
        if pitch == CLOSED_HAT {
            let open = DRUM_NOTES.iter().position(|&n| n == OPEN_HAT);
            for v in self.voices.iter_mut().filter(|v| v.active) {
                if Some(v.sound) == open {
                    v.fade = v.fade.clamp(0.999, 1.0);
                }
            }
        }
        let slot = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                // Steal the voice that has played longest.
                (0..DRUM_VOICES)
                    .max_by_key(|&i| self.voices[i].pos)
                    .unwrap_or(0)
            });
        self.voices[slot] = DrumVoice {
            sound,
            pos: 0,
            gain: velocity * self.gain * self.levels[DRUM_GROUP[sound]],
            fade: 2.0,
            active: true,
        };
    }

    fn render(&mut self, out: &mut [[f32; 2]]) {
        let volume = self.volume;
        let length = self.length;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let sample = &self.sounds[v.sound];
            let len = sample.len();
            // Past the Length knob's share of the sound, fade it out.
            let cut = (len as f32 * length * length) as usize;
            for frame in out.iter_mut() {
                if v.pos >= len || v.fade < 1e-4 {
                    v.active = false;
                    break;
                }
                if v.pos >= cut && v.fade > 1.0 {
                    v.fade = 1.0;
                }
                let [l, r] = sample.frame(v.pos);
                let g = v.gain * volume * v.fade.min(1.0);
                frame[0] += l * g;
                frame[1] += r * g;
                v.pos += 1;
                if v.fade < 1.5 {
                    v.fade *= 0.999;
                }
            }
        }
    }
}

// ------------------------------------------------------------ drum rack

const KIT_VOICES: usize = 32;
/// Frames a choked or stopped pad fades over, and an ending one.
const KIT_FADE: f64 = 96.0;

#[derive(Clone, Copy, Default)]
struct KitVoice {
    pad: usize,
    /// Where in the sample, in frames at the engine's rate.
    pos: f64,
    /// Frames per frame: the pad's tuning.
    step: f64,
    end: f64,
    gain: [f32; 2],
    /// 1 while playing, falling to 0 once cut off.
    fade: f32,
    cut: bool,
    active: bool,
}

/// A pad as the audio thread plays it.
struct KitPad {
    note: u8,
    sample: Arc<Sample>,
    gain: [f32; 2],
    step: f64,
    start: f64,
    end: f64,
    velocity: f32,
    choke: u8,
    /// Which level knob it follows (see [`DRUM_PARAMS`]).
    group: usize,
}

/// A Drum Rack: each pad plays its own sample, as Simpler in one-shot
/// mode does (to the end of the sample, whatever the note's length).
pub struct KitPlayer {
    pads: Vec<KitPad>,
    voices: [KitVoice; KIT_VOICES],
    volume: f32,
    length: f32,
    levels: [f32; 6],
}

/// The level knob a General MIDI drum note goes with: kick, snare, hats,
/// claps, perc, cymbals.
fn drum_group(note: u8) -> usize {
    match note {
        35 | 36 => 0,
        37 | 38 | 40 => 1,
        42 | 44 | 46 => 2,
        39 => 3,
        49 | 51 | 52 | 53 | 55 | 57 | 59 => 5,
        _ => 4,
    }
}

impl KitPlayer {
    /// The pads of `kit` that have their sample, for an engine at `rate`.
    pub fn new(kit: &crate::clip::Kit, rate: f64) -> KitPlayer {
        let pads = kit
            .pads
            .iter()
            .filter(|p| p.on)
            .filter_map(|p| {
                let sample = p.sample.clone()?;
                let len = sample.len() as f64;
                let end = p.end.map_or(len, |e| (e * rate).min(len));
                let (l, r) = ((1.0 - p.pan).min(1.0), (1.0 + p.pan).min(1.0));
                Some(KitPad {
                    note: p.note,
                    gain: [p.gain * l, p.gain * r],
                    step: 2f64.powf(p.transpose as f64 / 12.0),
                    start: (p.start * rate).min(end),
                    end,
                    velocity: p.velocity,
                    choke: p.choke,
                    group: drum_group(p.note),
                    sample,
                })
            })
            .collect();
        KitPlayer {
            pads,
            voices: [KitVoice::default(); KIT_VOICES],
            volume: 1.0,
            length: 1.0,
            levels: [1.0; 6],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pads.is_empty()
    }

    fn set_param(&mut self, index: usize, v: f32) {
        match index {
            0 => self.volume = level(v),
            1 => self.length = v,
            2..=7 => self.levels[index - 2] = level(v),
            _ => {}
        }
    }

    fn note_on(&mut self, pitch: u8, velocity: f32) {
        let Some(i) = self.pads.iter().position(|p| p.note == pitch) else {
            return;
        };
        let pad = &self.pads[i];
        if pad.choke != 0 {
            for v in self.voices.iter_mut().filter(|v| v.active) {
                if self.pads[v.pad].choke == pad.choke {
                    v.cut = true;
                }
            }
        }
        let slot = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                // Steal the voice furthest through its sound.
                (0..KIT_VOICES)
                    .max_by(|&a, &b| self.voices[a].pos.total_cmp(&self.voices[b].pos))
                    .unwrap_or(0)
            });
        let vel = 1.0 - pad.velocity * (1.0 - velocity.clamp(0.0, 1.0));
        let g = vel * self.levels[pad.group];
        self.voices[slot] = KitVoice {
            pad: i,
            pos: pad.start,
            step: pad.step,
            end: pad.end,
            gain: [pad.gain[0] * g, pad.gain[1] * g],
            fade: 1.0,
            cut: false,
            active: true,
        };
    }

    fn all_off(&mut self) {
        for v in self.voices.iter_mut() {
            v.cut = true;
        }
    }

    fn render(&mut self, out: &mut [[f32; 2]]) {
        let volume = self.volume;
        // The Length knob shortens every pad, as the built-in kit's does.
        let length = (self.length * self.length) as f64;
        let fall = (1.0 / KIT_FADE) as f32;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let pad = &self.pads[v.pad];
            let start = pad.start;
            let end = start + (v.end - start) * length.max(0.001);
            for frame in out.iter_mut() {
                if v.pos >= end || v.fade <= 0.0 {
                    v.active = false;
                    break;
                }
                if v.cut || v.pos >= end - KIT_FADE * v.step {
                    v.fade -= fall;
                }
                let i = v.pos as usize;
                let t = (v.pos - i as f64) as f32;
                let [a0, a1] = pad.sample.frame(i);
                let [b0, b1] = pad.sample.frame(i + 1);
                let g = volume * v.fade.max(0.0);
                frame[0] += (a0 + (b0 - a0) * t) * v.gain[0] * g;
                frame[1] += (a1 + (b1 - a1) * t) * v.gain[1] * g;
                v.pos += v.step;
            }
        }
    }
}

// ---------------------------------------------------------------- synth

/// How a synth track sounds.
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    /// Filter cutoff with the filter envelope closed, in Hz.
    pub cutoff: f32,
    /// How far the filter envelope opens it, in Hz.
    pub env_amount: f32,
    pub filter_decay: f32,
    /// 0 (none) to 1 (self-oscillating).
    pub resonance: f32,
    /// Second saw's detune, in cents.
    pub detune: f32,
    /// Square-wave sub oscillator an octave down.
    pub sub: f32,
    pub gain: f32,
    /// One note at a time, the newest wins.
    pub mono: bool,
}

pub const BASS: Preset = Preset {
    attack: 0.002,
    decay: 0.25,
    sustain: 0.6,
    release: 0.06,
    cutoff: 180.0,
    env_amount: 1400.0,
    filter_decay: 0.18,
    resonance: 0.45,
    detune: 6.0,
    sub: 0.6,
    gain: 0.5,
    mono: true,
};

pub const PAD: Preset = Preset {
    attack: 0.03,
    decay: 0.4,
    sustain: 0.7,
    release: 0.35,
    cutoff: 900.0,
    env_amount: 1800.0,
    filter_decay: 0.5,
    resonance: 0.2,
    detune: 14.0,
    sub: 0.0,
    gain: 0.18,
    mono: false,
};

pub const LEAD: Preset = Preset {
    attack: 0.003,
    decay: 0.18,
    sustain: 0.35,
    release: 0.12,
    cutoff: 1200.0,
    env_amount: 4200.0,
    filter_decay: 0.12,
    resonance: 0.35,
    detune: 9.0,
    sub: 0.0,
    gain: 0.22,
    mono: false,
};

const SYNTH_VOICES: usize = 12;
/// Samples between filter coefficient updates.
const CONTROL_RATE: usize = 16;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
struct Voice {
    note: u8,
    velocity: f32,
    stage: Stage,
    level: f32,
    filter_env: f32,
    phase: [f32; 2],
    sub_phase: f32,
    inc: [f32; 2],
    /// State-variable filter memory, per channel.
    ic1: [f32; 2],
    ic2: [f32; 2],
    /// Order the voice started in, to steal the oldest.
    born: u64,
}

impl Voice {
    const OFF: Voice = Voice {
        note: 0,
        velocity: 0.0,
        stage: Stage::Off,
        level: 0.0,
        filter_env: 0.0,
        phase: [0.0; 2],
        sub_phase: 0.0,
        inc: [0.0; 2],
        ic1: [0.0; 2],
        ic2: [0.0; 2],
        born: 0,
    };
}

pub struct Synth {
    preset: Preset,
    rate: f32,
    voices: [Voice; SYNTH_VOICES],
    births: u64,
}

impl Synth {
    pub fn new(preset: Preset, rate: f64) -> Synth {
        Synth {
            preset,
            rate: rate as f32,
            voices: [Voice::OFF; SYNTH_VOICES],
            births: 0,
        }
    }

    fn note_on(&mut self, pitch: u8, velocity: f32) {
        let slot = if self.preset.mono {
            0
        } else if let Some(i) = self.voices.iter().position(|v| v.stage == Stage::Off) {
            i
        } else {
            (0..SYNTH_VOICES)
                .min_by_key(|&i| self.voices[i].born)
                .unwrap_or(0)
        };
        let freq = 440.0 * 2f32.powf((pitch as f32 - 69.0) / 12.0);
        let spread = 2f32.powf(self.preset.detune / 1200.0);
        let v = &mut self.voices[slot];
        let legato = self.preset.mono && v.stage != Stage::Off;
        v.note = pitch;
        v.velocity = velocity;
        v.inc = [freq / self.rate, freq * spread / self.rate];
        v.stage = Stage::Attack;
        v.filter_env = 1.0;
        if !legato {
            v.level = 0.0;
            v.phase = [0.0, 0.37];
            v.sub_phase = 0.0;
            v.ic1 = [0.0; 2];
            v.ic2 = [0.0; 2];
        }
        self.births += 1;
        v.born = self.births;
    }

    fn note_off(&mut self, pitch: u8) {
        for v in self.voices.iter_mut() {
            if v.note == pitch && v.stage != Stage::Off && v.stage != Stage::Release {
                v.stage = Stage::Release;
            }
        }
    }

    fn all_off(&mut self) {
        for v in self.voices.iter_mut() {
            if v.stage != Stage::Off {
                v.stage = Stage::Release;
            }
        }
    }

    fn render(&mut self, out: &mut [[f32; 2]]) {
        let p = self.preset;
        let sr = self.rate;
        let step = |secs: f32| 1.0 / (secs.max(0.0005) * sr);
        let (a, d, r) = (step(p.attack), step(p.decay), step(p.release));
        let fdecay = (-1.0 / (p.filter_decay.max(0.001) * sr)).exp();
        let k = 2.0 - 2.0 * p.resonance.clamp(0.0, 0.98);
        for v in self.voices.iter_mut().filter(|v| v.stage != Stage::Off) {
            let (mut a1, mut a2, mut a3) = (0.0, 0.0, 0.0);
            for (i, frame) in out.iter_mut().enumerate() {
                if i % CONTROL_RATE == 0 {
                    let cutoff = (p.cutoff + p.env_amount * v.filter_env * v.velocity)
                        .clamp(20.0, sr * 0.45);
                    let g = (PI * cutoff / sr).tan();
                    a1 = 1.0 / (1.0 + g * (g + k));
                    a2 = g * a1;
                    a3 = g * a2;
                }
                // Amplitude envelope.
                match v.stage {
                    Stage::Attack => {
                        v.level += a;
                        if v.level >= 1.0 {
                            v.level = 1.0;
                            v.stage = Stage::Decay;
                        }
                    }
                    Stage::Decay => {
                        v.level -= d * (1.0 - p.sustain);
                        if v.level <= p.sustain {
                            v.level = p.sustain;
                            v.stage = Stage::Sustain;
                        }
                    }
                    Stage::Sustain => {}
                    Stage::Release => {
                        v.level -= r;
                        if v.level <= 0.0 {
                            v.level = 0.0;
                            v.stage = Stage::Off;
                            break;
                        }
                    }
                    Stage::Off => break,
                }
                v.filter_env *= fdecay;
                let mut osc = 0.0;
                for o in 0..2 {
                    osc += saw(v.phase[o], v.inc[o]);
                    v.phase[o] += v.inc[o];
                    if v.phase[o] >= 1.0 {
                        v.phase[o] -= 1.0;
                    }
                }
                osc *= 0.5;
                if p.sub > 0.0 {
                    osc += p.sub * if v.sub_phase < 0.5 { 0.5 } else { -0.5 };
                    v.sub_phase += v.inc[0] * 0.5;
                    if v.sub_phase >= 1.0 {
                        v.sub_phase -= 1.0;
                    }
                }
                let amp = v.level * v.velocity * p.gain;
                for (c, out) in frame.iter_mut().enumerate() {
                    // Two slightly different inputs give the voice width.
                    let x = if c == 0 { osc } else { osc * 0.98 };
                    let v3 = x - v.ic2[c];
                    let v1 = a1 * v.ic1[c] + a2 * v3;
                    let v2 = v.ic2[c] + a2 * v.ic1[c] + a3 * v3;
                    v.ic1[c] = 2.0 * v1 - v.ic1[c];
                    v.ic2[c] = 2.0 * v2 - v.ic2[c];
                    *out += v2 * amp;
                }
            }
        }
    }
}

/// A band-limited sawtooth (polyBLEP) at `phase` (0..1) stepping by `inc`.
#[inline]
fn saw(phase: f32, inc: f32) -> f32 {
    let mut y = 2.0 * phase - 1.0;
    if phase < inc {
        let t = phase / inc;
        y -= t + t - t * t - 1.0;
    } else if phase > 1.0 - inc {
        let t = (phase - 1.0) / inc;
        y -= t * t + t + t + 1.0;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(out: &[[f32; 2]]) -> f32 {
        out.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum()
    }

    fn pad(note: u8, choke: u8, value: f32) -> crate::clip::Pad {
        let frames = vec![[value, value]; 4800];
        let audio = opendj_engine::track::Audio::from_frames(48_000, frames);
        crate::clip::Pad {
            note,
            name: format!("pad {note}"),
            path: Default::default(),
            gain: 1.0,
            pan: 0.0,
            transpose: 0.0,
            velocity: 0.0,
            choke,
            start: 0.0,
            end: None,
            on: true,
            sample: Some(Arc::new(Sample::from_audio(&audio, 48_000.0))),
        }
    }

    #[test]
    fn drum_rack_pads_play_their_samples_and_choke() {
        let kit = crate::clip::Kit {
            pads: vec![pad(42, 1, 0.5), pad(46, 1, 0.25), pad(36, 0, 0.1)],
        };
        let mut k = KitPlayer::new(&kit, 48_000.0);
        let mut out = vec![[0.0; 2]; 64];
        k.note_on(46, 1.0);
        k.note_on(36, 1.0);
        k.render(&mut out);
        assert!((out[10][0] - 0.35).abs() < 0.01, "{:?}", out[10]);
        // The closed hat cuts the open one; the kick plays on.
        k.note_on(42, 1.0);
        let mut out = vec![[0.0; 2]; 256];
        k.render(&mut out);
        assert!((out[200][0] - 0.6).abs() < 0.01, "{:?}", out[200]);
        // A note no pad has plays nothing; every sample ends.
        k.note_on(99, 1.0);
        let mut tail = vec![[0.0; 2]; 9600];
        k.render(&mut tail);
        assert!(k.voices.iter().all(|v| !v.active));
    }

    #[test]
    fn synth_plays_and_releases() {
        let mut s = Synth::new(LEAD, 48_000.0);
        let mut out = vec![[0.0; 2]; 4800];
        s.note_on(60, 1.0);
        s.render(&mut out);
        assert!(energy(&out) > 1.0);
        s.note_off(60);
        let mut tail = vec![[0.0; 2]; 48_000];
        s.render(&mut tail);
        assert!(s.voices.iter().all(|v| v.stage == Stage::Off));
        assert!(tail.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    }

    #[test]
    fn knobs_start_where_the_presets_are() {
        for preset in [BASS, PAD, LEAD] {
            let mut p = preset;
            for (i, v) in preset.params().into_iter().enumerate() {
                p.set_param(i, v);
            }
            assert!((p.cutoff - preset.cutoff).abs() < 0.5);
            assert!((p.attack - preset.attack).abs() < 1e-4);
            assert!((p.release - preset.release).abs() < 1e-3);
            assert!((p.env_amount - preset.env_amount).abs() < 1.0);
        }
        assert_eq!(
            param_text(false, 0, log_from(1200.0, 40.0, 12_000.0)),
            "1.2 kHz"
        );
        assert_eq!(param_text(true, 0, 0.5), "+0.0 dB");
    }

    #[test]
    fn drums_play_a_kick() {
        let mut d = Drums::new("Punch", 48_000.0, 1.0);
        let mut out = vec![[0.0; 2]; 4800];
        d.note_on(KICK, 1.0);
        d.render(&mut out);
        assert!(energy(&out) > 1.0);
        // Notes outside the kit are ignored.
        d.note_on(100, 1.0);
        assert_eq!(d.voices.iter().filter(|v| v.active).count(), 1);
    }
}
