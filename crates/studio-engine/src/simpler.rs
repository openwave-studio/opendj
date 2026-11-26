//! Live's Simpler: one sample played across the keys, each note pitched
//! from the sample's root key, with an envelope, a low-pass filter and a
//! loop. Classic mode plays while the key is held (looping if asked);
//! One-Shot plays to the end whatever the note's length.
//!
//! Voices read the sample with 4-point Hermite interpolation, so pitched
//! notes stay smooth. Every voice is set up before the audio starts, and
//! the sample is shared (an `Arc`), so nothing allocates while it plays.

use std::path::PathBuf;
use std::sync::Arc;

use fundsp::svf::SvfCoefs;
use opendj_engine::sampler::Sample;
use opendj_plugin_host::shm::Midi;

use crate::devices::{from_db, Param, Unit};
use crate::kit::Smooth;

pub const MODES: &[&str] = &["Classic", "One-Shot"];
/// The most notes it plays at once.
pub const MAX_VOICES: usize = 32;

#[derive(Clone)]
pub struct SimplerSettings {
    /// The sample's file, and the sample decoded at the engine's rate.
    pub path: Option<PathBuf>,
    pub sample: Option<Arc<Sample>>,
    /// 0 Classic, 1 One-Shot.
    pub mode: u8,
    /// The key that plays the sample at its own pitch.
    pub root: u8,
    /// Where it plays from and to, as shares of the sample.
    pub start: f32,
    pub end: f32,
    /// Classic only: loops from `loop_start` to `end` while the key is
    /// held.
    pub looping: bool,
    pub loop_start: f32,
    pub attack_ms: f32,
    pub decay_ms: f32,
    /// 0..1.
    pub sustain: f32,
    pub release_ms: f32,
    pub transpose: f32,
    /// Cents.
    pub detune: f32,
    pub volume_db: f32,
    /// How much a soft note is quieter, 0..1.
    pub velocity: f32,
    pub filter: bool,
    pub cutoff: f32,
    /// 0..1.
    pub resonance: f32,
    /// 1..32.
    pub voices: u8,
}

impl std::fmt::Debug for SimplerSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("SimplerSettings")
            .field("path", &self.path)
            .field("loaded", &self.sample.is_some())
            .field("mode", &self.mode)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl PartialEq for SimplerSettings {
    fn eq(&self, o: &Self) -> bool {
        let same_sample = match (&self.sample, &o.sample) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same_sample
            && self.path == o.path
            && self
                .params()
                .iter()
                .map(|p| p.value)
                .eq(o.params().iter().map(|p| p.value))
    }
}

impl Default for SimplerSettings {
    fn default() -> Self {
        SimplerSettings {
            path: None,
            sample: None,
            mode: 0,
            root: 60,
            start: 0.0,
            end: 1.0,
            looping: false,
            loop_start: 0.0,
            attack_ms: 0.1,
            decay_ms: 600.0,
            sustain: 1.0,
            release_ms: 50.0,
            transpose: 0.0,
            detune: 0.0,
            volume_db: -12.0,
            velocity: 0.35,
            filter: false,
            cutoff: 20_000.0,
            resonance: 0.0,
            voices: 8,
        }
    }
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A key's name, C3 as middle C (60), as Live shows it.
pub fn key_name(k: u8) -> String {
    format!("{}{}", NOTE_NAMES[k as usize % 12], k as i32 / 12 - 2)
}

impl SimplerSettings {
    /// The sample's file name, or what to do.
    pub fn sample_name(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            None => "No sample: drop one from the browser's Samples".into(),
        }
    }

    pub fn summary(&self) -> String {
        if self.path.is_none() {
            return self.sample_name();
        }
        format!(
            "{} · {} · root {}",
            self.sample_name(),
            MODES[(self.mode as usize).min(1)],
            key_name(self.root)
        )
    }

    pub fn params(&self) -> Vec<Param> {
        vec![
            Param::choice("Mode", self.mode as usize, MODES),
            Param::toggle("Loop", self.looping),
            Param::toggle("Filter", self.filter),
            Param::knob("Root", self.root as f32, 0.0, 127.0, Unit::Key),
            Param::knob("Start", self.start, 0.0, 1.0, Unit::Percent),
            Param::knob("End", self.end, 0.0, 1.0, Unit::Percent),
            Param::knob("Loop Start", self.loop_start, 0.0, 1.0, Unit::Percent),
            Param::knob("Attack", self.attack_ms, 0.1, 20_000.0, Unit::Ms).log(),
            Param::knob("Decay", self.decay_ms, 1.0, 60_000.0, Unit::Ms).log(),
            Param::knob("Sustain", self.sustain, 0.0, 1.0, Unit::Percent),
            Param::knob("Release", self.release_ms, 1.0, 60_000.0, Unit::Ms).log(),
            Param::knob("Transpose", self.transpose, -48.0, 48.0, Unit::Semitones).bipolar(),
            Param::knob("Detune", self.detune, -50.0, 50.0, Unit::Cents).bipolar(),
            Param::knob("Volume", self.volume_db, -36.0, 12.0, Unit::Db),
            Param::knob("Vel > Vol", self.velocity, 0.0, 1.0, Unit::Percent),
            Param::knob("Cutoff", self.cutoff, 30.0, 20_000.0, Unit::Hz).log(),
            Param::knob("Resonance", self.resonance, 0.0, 1.0, Unit::Percent),
            Param::knob(
                "Voices",
                self.voices as f32,
                1.0,
                MAX_VOICES as f32,
                Unit::Count,
            ),
        ]
    }

    /// Changes one setting (see [`SimplerSettings::params`]). Never
    /// allocates.
    pub fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.mode = v.round().clamp(0.0, 1.0) as u8,
            1 => self.looping = v >= 0.5,
            2 => self.filter = v >= 0.5,
            3 => self.root = v.round().clamp(0.0, 127.0) as u8,
            4 => self.start = v.clamp(0.0, 1.0),
            5 => self.end = v.clamp(0.0, 1.0),
            6 => self.loop_start = v.clamp(0.0, 1.0),
            7 => self.attack_ms = v.clamp(0.1, 20_000.0),
            8 => self.decay_ms = v.clamp(1.0, 60_000.0),
            9 => self.sustain = v.clamp(0.0, 1.0),
            10 => self.release_ms = v.clamp(1.0, 60_000.0),
            11 => self.transpose = v.round().clamp(-48.0, 48.0),
            12 => self.detune = v.clamp(-50.0, 50.0),
            13 => self.volume_db = v.clamp(-36.0, 12.0),
            14 => self.velocity = v.clamp(0.0, 1.0),
            15 => self.cutoff = v.clamp(30.0, 20_000.0),
            16 => self.resonance = v.clamp(0.0, 1.0),
            17 => self.voices = v.round().clamp(1.0, MAX_VOICES as f32) as u8,
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    key: u8,
    stage: Stage,
    env: f32,
    /// Where it reads in the sample, in frames.
    pos: f64,
    step: f64,
    gain: f32,
    /// Notes started before it, for stealing the oldest.
    age: u64,
    /// Stolen: it fades out fast in a spare voice instead of cutting off.
    fast: bool,
}

impl Voice {
    const IDLE: Voice = Voice {
        key: 0,
        stage: Stage::Idle,
        env: 0.0,
        pos: 0.0,
        step: 1.0,
        gain: 0.0,
        age: 0,
        fast: false,
    };
}

/// A 4-point, 3rd-order Hermite read between frames.
#[inline]
fn hermite(s: &Sample, pos: f64) -> [f32; 2] {
    let i = pos.floor();
    let t = (pos - i) as f32;
    let i = i as isize;
    let at = |k: isize| {
        if k < 0 {
            [0.0; 2]
        } else {
            s.frame(k as usize)
        }
    };
    let (xm1, x0, x1, x2) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    let mut out = [0.0f32; 2];
    for c in 0..2 {
        let c0 = x0[c];
        let c1 = 0.5 * (x1[c] - xm1[c]);
        let c2 = xm1[c] - 2.5 * x0[c] + 2.0 * x1[c] - 0.5 * x2[c];
        let c3 = 0.5 * (x2[c] - xm1[c]) + 1.5 * (x0[c] - x1[c]);
        out[c] = ((c3 * t + c2) * t + c1) * t + c0;
    }
    out
}

pub struct Simpler {
    pub(crate) s: SimplerSettings,
    rate: f64,
    voices: [Voice; MAX_VOICES],
    notes: u64,
    volume: Smooth,
    cutoff: Smooth,
    svf: [[f64; 2]; 2],
    coefs: SvfCoefs<f64>,
    /// Frames until the filter is worked out again.
    count: usize,
}

/// Frames between filter updates.
const STRIDE: usize = 16;

impl Simpler {
    pub fn new(s: &SimplerSettings, rate: f64) -> Simpler {
        Simpler {
            s: s.clone(),
            rate,
            voices: [Voice::IDLE; MAX_VOICES],
            notes: 0,
            volume: Smooth::knob(from_db(s.volume_db), rate),
            cutoff: Smooth::knob(s.cutoff.ln(), rate),
            svf: [[0.0; 2]; 2],
            coefs: SvfCoefs::lowpass(rate, s.cutoff.min(rate as f32 * 0.45) as f64, 0.707),
            count: 0,
        }
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.volume.set(from_db(self.s.volume_db));
        self.cutoff.set(self.s.cutoff.ln());
    }

    /// The part of the sample that plays: (start, end, loop start), in
    /// frames.
    fn span(&self, len: usize) -> (f64, f64, f64) {
        let len = len as f64;
        let start = (self.s.start as f64 * len).min(len);
        let end = (self.s.end as f64 * len).clamp(start, len);
        let loop_start = (self.s.loop_start as f64 * len).clamp(start, end);
        (start, end, loop_start)
    }

    fn note_on(&mut self, key: u8, velocity: f32) {
        let Some(sample) = &self.s.sample else { return };
        let (start, _, _) = self.span(sample.len());
        let allowed = (self.s.voices as usize).clamp(1, MAX_VOICES);
        // A free voice, else the oldest playing one.
        let slot = self.voices[..allowed]
            .iter()
            .position(|v| v.stage == Stage::Idle)
            .unwrap_or_else(|| {
                (0..allowed)
                    .min_by_key(|&i| self.voices[i].age)
                    .unwrap_or(0)
            });
        let semis = key as f64 - self.s.root as f64
            + self.s.transpose as f64
            + self.s.detune as f64 / 100.0;
        let vel = 1.0 - self.s.velocity * (1.0 - velocity.clamp(0.0, 1.0));
        // A playing voice that's stolen fades out in a spare one.
        if self.voices[slot].stage != Stage::Idle {
            if let Some(spare) = self.voices[allowed..]
                .iter()
                .position(|v| v.stage == Stage::Idle)
            {
                let mut old = self.voices[slot];
                old.stage = Stage::Release;
                old.fast = true;
                self.voices[allowed + spare] = old;
            }
        }
        self.notes += 1;
        self.voices[slot] = Voice {
            key,
            stage: Stage::Attack,
            // A stolen voice starts from silence; a short attack still
            // ramps, so it doesn't click.
            env: 0.0,
            pos: start,
            step: 2f64.powf(semis / 12.0),
            gain: vel,
            age: self.notes,
            fast: false,
        };
    }

    fn note_off(&mut self, key: u8) {
        // One-Shot plays on to the end.
        if self.s.mode == 1 {
            return;
        }
        for v in self.voices.iter_mut().filter(|v| v.key == key) {
            if v.stage != Stage::Idle {
                v.stage = Stage::Release;
            }
        }
    }

    fn all_off(&mut self) {
        for v in self.voices.iter_mut().filter(|v| v.stage != Stage::Idle) {
            v.stage = Stage::Release;
        }
    }

    fn choke(&mut self) {
        for v in self.voices.iter_mut().filter(|v| v.stage != Stage::Idle) {
            v.stage = Stage::Release;
            v.fast = true;
        }
    }

    fn event(&mut self, m: &Midi) {
        match (m.data[0] & 0xF0, m.data[2]) {
            (0x90, v) if v > 0 => self.note_on(m.data[1], v as f32 / 127.0),
            (0x90, _) | (0x80, _) => self.note_off(m.data[1]),
            (0xB0, _) if m.data[1] == 123 => self.all_off(),
            // All sound off (a Drum Rack's choke): a fast fade.
            (0xB0, _) if m.data[1] == 120 => self.choke(),
            _ => {}
        }
    }

    /// Plays the block's notes (`midi`, at their frames) into `buf`.
    pub fn play(&mut self, midi: &[Midi], buf: &mut [[f32; 2]]) {
        let mut next = 0;
        let rate = self.rate as f32;
        let coef = |ms: f32| 1.0 - (-1.0 / (ms.max(0.05) * 0.001 * rate)).exp();
        let (attack, decay, release) = (
            coef(self.s.attack_ms),
            coef(self.s.decay_ms),
            coef(self.s.release_ms),
        );
        let sustain = self.s.sustain;
        let classic_loop = self.s.mode == 0 && self.s.looping;
        let fast = coef(3.0);
        // The last few ms before the end fade, so a trimmed end doesn't
        // click.
        let fade = 0.002 * self.rate;
        let q = 0.707 * 20f64.powf(self.s.resonance as f64);
        // Once a block: a new sample only comes with new settings.
        let sample = self.s.sample.clone();
        for (i, f) in buf.iter_mut().enumerate() {
            while next < midi.len() && midi[next].frame as usize <= i {
                let m = midi[next];
                self.event(&m);
                next += 1;
            }
            let volume = self.volume.tick();
            let cutoff = self.cutoff.tick();
            let Some(sample) = sample.as_deref() else {
                continue;
            };
            let (_, end, loop_start) = self.span(sample.len());
            let mut out = [0.0f32; 2];
            for v in self.voices.iter_mut() {
                if v.stage == Stage::Idle {
                    continue;
                }
                // The envelope: exponential segments, as an analog one.
                match v.stage {
                    Stage::Attack => {
                        v.env += (1.2 - v.env) * attack;
                        if v.env >= 1.0 {
                            v.env = 1.0;
                            v.stage = Stage::Decay;
                        }
                    }
                    Stage::Decay => {
                        v.env += (sustain - v.env) * decay;
                        if (v.env - sustain).abs() < 1e-4 {
                            v.stage = Stage::Sustain;
                        }
                    }
                    Stage::Sustain => v.env = sustain,
                    Stage::Release => {
                        v.env -= v.env * if v.fast { fast } else { release };
                        if v.env < 1e-4 {
                            v.stage = Stage::Idle;
                            continue;
                        }
                    }
                    Stage::Idle => {}
                }
                let x = hermite(sample, v.pos);
                let left = (end - v.pos) / (fade * v.step);
                let edge = if classic_loop || left >= 1.0 {
                    1.0
                } else {
                    left.max(0.0) as f32
                };
                let g = v.env * v.gain * edge;
                out[0] += x[0] * g;
                out[1] += x[1] * g;
                v.pos += v.step;
                if v.pos >= end {
                    if classic_loop && end - loop_start > 1.0 {
                        v.pos = loop_start + (v.pos - end) % (end - loop_start);
                    } else {
                        // Its end: a few frames' fade would need the
                        // future, so it stops where the sample does
                        // (samples end on silence, or the envelope is
                        // already down).
                        v.stage = Stage::Idle;
                    }
                }
            }
            if self.s.filter {
                if self.count == 0 {
                    self.count = STRIDE;
                    let hz = (cutoff.exp() as f64).min(self.rate * 0.45);
                    self.coefs = SvfCoefs::lowpass(self.rate, hz, q);
                }
                self.count -= 1;
                let c = &self.coefs;
                for (ch, st) in self.svf.iter_mut().enumerate() {
                    let x = out[ch] as f64;
                    let v3 = x - st[1];
                    let v1 = c.a1 * st[0] + c.a2 * v3;
                    let v2 = st[1] + c.a2 * st[0] + c.a3 * v3;
                    st[0] = 2.0 * v1 - st[0];
                    st[1] = 2.0 * v2 - st[1];
                    out[ch] = v2 as f32;
                }
            }
            f[0] += out[0] * volume;
            f[1] += out[1] * volume;
        }
    }

    /// Notes sounding now (not counting stolen ones fading out).
    pub fn sounding(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.stage != Stage::Idle && !v.fast)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_sample(freq: f32, seconds: f32) -> Arc<Sample> {
        let rate = 48_000.0;
        let n = (seconds * rate) as usize;
        let audio = opendj_engine::track::Audio::from_frames(
            48_000,
            (0..n)
                .map(|i| {
                    let v = (i as f32 * freq * std::f32::consts::TAU / rate).sin() * 0.5;
                    [v, v]
                })
                .collect(),
        );
        Arc::new(Sample::from_audio(&audio, 48_000.0))
    }

    fn simpler(s: SimplerSettings) -> Simpler {
        Simpler::new(
            &SimplerSettings {
                sample: Some(sine_sample(440.0, 1.0)),
                path: Some("sine.wav".into()),
                volume_db: 0.0,
                velocity: 0.0,
                ..s
            },
            48_000.0,
        )
    }

    /// The main frequency of `x`, by counting upward zero crossings.
    fn pitch(x: &[[f32; 2]]) -> f32 {
        let ups = x
            .windows(2)
            .filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0)
            .count();
        ups as f32 / (x.len() as f32 / 48_000.0)
    }

    #[test]
    fn keys_play_the_sample_at_their_pitch() {
        let mut s = simpler(SimplerSettings::default());
        let mut buf = vec![[0.0f32; 2]; 24_000];
        // An octave up from the root, then a fifth down.
        s.play(&[Midi::note_on(0, 0, 72, 100)], &mut buf);
        assert!(
            (pitch(&buf[2_000..]) - 880.0).abs() < 5.0,
            "{}",
            pitch(&buf)
        );
        let mut s = simpler(SimplerSettings::default());
        let mut buf = vec![[0.0f32; 2]; 24_000];
        s.play(&[Midi::note_on(0, 0, 53, 100)], &mut buf);
        let fifth_down = 440.0 * 2f32.powf(-7.0 / 12.0);
        assert!((pitch(&buf[2_000..]) - fifth_down).abs() < 5.0);
    }

    #[test]
    fn classic_stops_at_release_and_one_shot_plays_on() {
        let release = |mode: u8| {
            let mut s = simpler(SimplerSettings {
                mode,
                release_ms: 10.0,
                ..Default::default()
            });
            let mut buf = vec![[0.0f32; 2]; 24_000];
            s.play(
                &[Midi::note_on(0, 0, 60, 100), Midi::note_off(4_800, 0, 60)],
                &mut buf,
            );
            buf[12_000..].iter().fold(0.0f32, |m, f| m.max(f[0].abs()))
        };
        assert!(release(0) < 1e-3, "classic rang on");
        assert!(release(1) > 0.3, "one-shot stopped");
    }

    #[test]
    fn a_held_loop_keeps_playing_past_the_end() {
        let mut s = simpler(SimplerSettings {
            looping: true,
            loop_start: 0.5,
            ..Default::default()
        });
        let mut buf = vec![[0.0f32; 2]; 96_000];
        s.play(&[Midi::note_on(0, 0, 60, 100)], &mut buf);
        // Two seconds from a one-second sample: still sounding at the end.
        let tail = buf[90_000..].iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
        assert!(tail > 0.3, "loop ran out: {tail}");
        assert_eq!(s.sounding(), 1);
    }

    #[test]
    fn voices_are_capped_and_the_oldest_is_stolen() {
        let mut s = simpler(SimplerSettings {
            voices: 2,
            ..Default::default()
        });
        let mut buf = vec![[0.0f32; 2]; 256];
        s.play(
            &[
                Midi::note_on(0, 0, 60, 100),
                Midi::note_on(10, 0, 64, 100),
                Midi::note_on(20, 0, 67, 100),
            ],
            &mut buf,
        );
        assert_eq!(s.sounding(), 2);
        let keys: Vec<u8> = s.voices[..2].iter().map(|v| v.key).collect();
        assert!(keys.contains(&64) && keys.contains(&67), "{keys:?}");
    }

    #[test]
    fn no_sample_plays_nothing() {
        let mut s = Simpler::new(&SimplerSettings::default(), 48_000.0);
        let mut buf = vec![[0.0f32; 2]; 256];
        s.play(&[Midi::note_on(0, 0, 60, 100)], &mut buf);
        assert!(buf.iter().all(|f| *f == [0.0; 2]));
    }
}
