//! MIDI clips and the demo set the test app opens with.

use crate::sound::{CLAP, CLOSED_HAT, KICK, OPEN_HAT, PARAMS, RIM, SNARE};
use crate::steps::{self, Layer, Stages};
use opendj_engine::track::Audio;
use std::path::PathBuf;
use std::sync::Arc;

/// A note in a clip, in beats from the clip's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub start: f64,
    pub length: f64,
    pub pitch: u8,
    pub velocity: f32,
    /// The chance it plays each time round, 0..1.
    pub chance: f32,
}

/// Where a stage clip sets the instrument's knobs: each lock or, for
/// `None`, the knob as the track has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lock {
    pub start: f64,
    pub values: [Option<f32>; PARAMS],
    /// How far towards the locked values, 0..1.
    pub amount: f32,
}

/// A looping MIDI clip.
#[derive(Clone, Debug)]
pub struct Clip {
    pub name: String,
    /// Loop length in beats.
    pub beats: f64,
    /// Sorted by start.
    pub notes: Vec<Note>,
    /// Knob changes by stage (stage clips only), sorted by start.
    pub locks: Vec<Lock>,
    /// Where each stage starts and which it is (stage clips only).
    pub stage_starts: Vec<(f64, u8)>,
    /// What it was made from, when it is a stage clip.
    pub stages: Option<Box<Stages>>,
    /// The recording it plays, when it is an audio clip.
    pub audio: Option<AudioClip>,
}

/// An audio clip: a file played through a warp map, so its beats land on
/// the set's beats at any tempo.
#[derive(Clone, Debug)]
pub struct AudioClip {
    pub path: PathBuf,
    /// Decoded before the set plays; None when the file is missing.
    pub data: Option<Arc<Audio>>,
    /// Warp markers: (beat in the clip, second in the file), sorted. With
    /// fewer than two, the file plays at its own speed.
    pub warp: Vec<(f64, f64)>,
    /// The clip's loop start, in clip beats.
    pub start: f64,
    pub gain: f32,
}

impl AudioClip {
    /// The second in the file at clip beat `beat`, and seconds per beat
    /// there, following the warp markers (extended past the ends).
    pub fn seconds(&self, beat: f64, tempo: f64) -> (f64, f64) {
        let w = &self.warp;
        if w.len() < 2 {
            let per = 60.0 / tempo.max(1.0);
            return (beat * per, per);
        }
        let i = w
            .windows(2)
            .position(|p| beat < p[1].0)
            .unwrap_or(w.len() - 2);
        let (a, b) = (w[i], w[i + 1]);
        let per = (b.1 - a.1) / (b.0 - a.0).max(1e-9);
        (a.1 + (beat - a.0) * per, per)
    }
}

impl Clip {
    pub fn new(name: &str, beats: f64, mut notes: Vec<Note>) -> Clip {
        notes.retain(|n| n.start >= 0.0 && n.start < beats && n.length > 0.0);
        notes.sort_by(|a, b| a.start.total_cmp(&b.start));
        Clip {
            name: name.to_string(),
            beats,
            notes,
            locks: Vec::new(),
            stage_starts: Vec::new(),
            stages: None,
            audio: None,
        }
    }
}

/// What plays a track's notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    /// A Groove Box kit, by name.
    Drums(&'static str),
    Bass,
    Pad,
    Lead,
    /// An audio track: its clips are recordings.
    Audio,
}

/// A device on a track, as a set lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub name: String,
    /// "Live", "AU", "VST3", "VST" or "Built-in".
    pub kind: String,
    /// Switched on in the set.
    pub on: bool,
    /// A plug-in the studio can run in its place.
    pub plugin: Option<PluginRef>,
}

/// Which plug-in a device is, and the sound it was saved with.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginRef {
    pub name: String,
    pub vendor: String,
    pub format: PluginFormat,
    /// Plays notes (rather than processing sound).
    pub instrument: bool,
    /// The plug-in's own saved state, as the plug-in host loads it: an
    /// Audio Unit's ClassInfo property list, or a VST3's component and
    /// controller states packed together.
    pub state: Option<Arc<Vec<u8>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginFormat {
    /// An Audio Unit by its `type:subtype:manufacturer` codes.
    Au(String),
    /// A VST3 class by its id (32 hex digits).
    Vst3(String),
    /// Whatever installed plug-in carries this name (VST3, then CLAP, then
    /// AU): for a device swapped for another plug-in.
    Named,
}

/// A track: a column of clip slots.
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub sound: Sound,
    pub slots: Vec<Option<Clip>>,
    /// Its own colour (0xRRGGBB), when the set gives one.
    pub color: Option<u32>,
    /// Fader position (0..1) and pan (0..1), when the set gives them.
    pub volume: Option<f32>,
    pub pan: f32,
    /// Its devices in order, for show until they can load.
    pub devices: Vec<DeviceInfo>,
    /// Where it listens, as the set names it (audio tracks).
    pub input: String,
}

impl Track {
    pub fn new(name: &str, sound: Sound, slots: Vec<Option<Clip>>) -> Track {
        Track {
            name: name.into(),
            sound,
            slots,
            color: None,
            volume: None,
            pan: 0.5,
            devices: Vec::new(),
            input: String::new(),
        }
    }
}

/// The demo set's track count.
pub const TRACKS: usize = 4;
/// The most tracks a set can have.
pub const MAX_TRACKS: usize = 16;
pub const SCENES: usize = 8;

fn n(start: f64, length: f64, pitch: u8, velocity: f32) -> Note {
    Note {
        start,
        length,
        pitch,
        velocity,
        chance: 1.0,
    }
}

/// A drum pattern from rows of 16 steps (`x` loud, `o` soft), one bar per
/// 16 steps.
fn drums(name: &str, rows: &[(u8, &str)]) -> Clip {
    let steps = rows.iter().map(|(_, s)| s.len()).max().unwrap_or(16);
    let mut notes = Vec::new();
    for &(pitch, row) in rows {
        for (i, c) in row.chars().enumerate() {
            let velocity = match c {
                'x' => 1.0,
                'o' => 0.55,
                _ => continue,
            };
            notes.push(n(i as f64 * 0.25, 0.2, pitch, velocity));
        }
    }
    Clip::new(name, steps as f64 / 4.0, notes)
}

/// A note line: (beat, length, pitch) triples at one velocity.
fn line(name: &str, beats: f64, velocity: f32, notes: &[(f64, f64, u8)]) -> Clip {
    Clip::new(
        name,
        beats,
        notes
            .iter()
            .map(|&(s, l, p)| n(s, l, p, velocity))
            .collect(),
    )
}

/// Chords: (beat, length, pitches).
fn chords(name: &str, beats: f64, chords: &[(f64, f64, &[u8])]) -> Clip {
    let mut notes = Vec::new();
    for &(s, l, ps) in chords {
        for &p in ps {
            notes.push(n(s, l, p, 0.8));
        }
    }
    Clip::new(name, beats, notes)
}

// A minor: Am F C G.
const AM: &[u8] = &[57, 60, 64];
const F: &[u8] = &[53, 57, 60];
const C: &[u8] = &[55, 60, 64];
const G: &[u8] = &[55, 59, 62];

/// A stage clip from (degree, gates) per stage, two pulses each.
fn stages(name: &str, sound: Sound, rows: [(u8, u8); 8], tweak: impl Fn(&mut Stages)) -> Clip {
    let mut s = Stages::default();
    for (stage, (degree, gates)) in s.stages.iter_mut().zip(rows) {
        steps::press(stage, Layer::Pitch, degree, false, false);
        stage.gates = gates;
    }
    tweak(&mut s);
    s.clip(name, sound, &crate::scale::Key::default())
}

/// Four tracks of eight scenes in A minor at 124 BPM: drums, bass, chords,
/// lead.
pub fn demo_set() -> Vec<Track> {
    let k = KICK;
    let drums_col = vec![
        Some(drums("Kick", &[(k, "x---x---x---x---")])),
        Some(drums(
            "Kick + hats",
            &[(k, "x---x---x---x---"), (CLOSED_HAT, "--x---x---x---x-")],
        )),
        Some(drums(
            "Groove",
            &[
                (k, "x---x---x---x---"),
                (CLAP, "----x-------x---"),
                (CLOSED_HAT, "o-x-o-x-o-x-o-xo"),
                (OPEN_HAT, "--------------x-"),
            ],
        )),
        Some(drums(
            "Broken",
            &[
                (k, "x-----x---x--x--"),
                (SNARE, "----x-------x--o"),
                (CLOSED_HAT, "x-x-x-x-x-x-x-x-"),
            ],
        )),
        Some(drums(
            "Claps",
            &[
                (k, "x---x---x---x---"),
                (CLAP, "----x--o----x---"),
                (RIM, "--o----o--o---o-"),
                (OPEN_HAT, "--x---x---x---x-"),
            ],
        )),
        Some(drums(
            "Rolling",
            &[
                (k, "x---x---x---x---"),
                (CLAP, "----x-------x---"),
                (CLOSED_HAT, "xoxoxoxoxoxoxoxo"),
                (RIM, "---o--o----o--o-"),
            ],
        )),
        Some(drums(
            "Fill",
            &[
                (k, "x---x---x---x-x-"),
                (SNARE, "------o-o-oxoxxx"),
                (CLOSED_HAT, "x-x-x-x---------"),
            ],
        )),
        Some(stages(
            "Steps",
            Sound::Drums("Punch"),
            [
                (0, 1),
                (2, 1),
                (1, 1),
                (2, 3),
                (0, 1),
                (2, 1),
                (1, 1),
                (3, 1),
            ],
            |s| s.stages[3].chance = 0.6,
        )),
    ];
    // Bass, two bars, roots of Am F C G.
    let roots = [45u8, 41, 48, 43];
    let per_beat = |name: &str, pattern: &[(f64, f64, i8)]| {
        let mut notes = Vec::new();
        for (bar, &root) in roots.iter().enumerate() {
            for &(s, l, o) in pattern {
                notes.push((bar as f64 * 4.0 + s, l, (root as i8 + o) as u8));
            }
        }
        line(name, 16.0, 0.9, &notes)
    };
    let bass_col = vec![
        Some(stages(
            "Steps",
            Sound::Bass,
            [
                (0, 1),
                (0, 2),
                (4, 1),
                (3, 3),
                (0, 1),
                (6, 2),
                (5, 1),
                (4, 1),
            ],
            |s| {
                // The cutoff opens on the sixth stage.
                s.stages[5].locks[0] = Some(0.75);
                s.stages[3].tie = true;
            },
        )),
        Some(per_beat("Roots", &[(0.0, 3.5, 0)])),
        Some(per_beat(
            "Offbeat",
            &[(0.5, 0.4, 0), (1.5, 0.4, 0), (2.5, 0.4, 0), (3.5, 0.4, 0)],
        )),
        Some(per_beat(
            "Walk",
            &[(0.0, 0.9, 0), (1.0, 0.9, 7), (2.0, 0.9, 12), (3.0, 0.9, 10)],
        )),
        Some(per_beat(
            "Octaves",
            &[
                (0.0, 0.2, 0),
                (0.5, 0.2, 12),
                (1.0, 0.2, 0),
                (1.5, 0.2, 12),
                (2.0, 0.2, 0),
                (2.5, 0.2, 12),
                (3.0, 0.2, 0),
                (3.5, 0.2, 12),
            ],
        )),
        Some(per_beat(
            "Sixteenths",
            &[
                (0.0, 0.2, 0),
                (0.25, 0.2, 0),
                (0.75, 0.2, 12),
                (1.5, 0.2, 0),
                (2.0, 0.2, 3),
                (2.5, 0.2, 0),
                (3.0, 0.2, 10),
                (3.5, 0.2, 0),
            ],
        )),
        Some(per_beat("Roots", &[(0.0, 3.5, 0)])),
        Some(line("Drone", 16.0, 0.8, &[(0.0, 15.5, 33)])),
    ];
    let chord_col = vec![
        None,
        None,
        Some(chords(
            "Pad",
            16.0,
            &[(0.0, 3.9, AM), (4.0, 3.9, F), (8.0, 3.9, C), (12.0, 3.9, G)],
        )),
        Some(chords(
            "Stabs",
            16.0,
            &[
                (0.5, 0.3, AM),
                (2.5, 0.3, AM),
                (4.5, 0.3, F),
                (6.5, 0.3, F),
                (8.5, 0.3, C),
                (10.5, 0.3, C),
                (12.5, 0.3, G),
                (14.5, 0.3, G),
            ],
        )),
        Some(chords(
            "Pad",
            16.0,
            &[(0.0, 3.9, AM), (4.0, 3.9, F), (8.0, 3.9, C), (12.0, 3.9, G)],
        )),
        Some(chords(
            "Pulse",
            16.0,
            &(0..16)
                .map(|b| {
                    let c = [AM, F, C, G][b / 4];
                    (b as f64 + 0.5, 0.25, c)
                })
                .collect::<Vec<_>>(),
        )),
        Some(chords("Hold", 16.0, &[(0.0, 15.9, AM)])),
        None,
    ];
    let arp = |name: &str, octave: u8, steps: &[u8]| {
        let mut notes = Vec::new();
        for (bar, chord) in [AM, F, C, G].iter().enumerate() {
            for (i, &s) in steps.iter().enumerate() {
                let p = chord[s as usize % 3] + 12 * (octave + s / 3);
                notes.push((bar as f64 * 4.0 + i as f64 * 0.5, 0.4, p));
            }
        }
        line(name, 16.0, 0.75, &notes)
    };
    let lead_col = vec![
        None,
        None,
        None,
        Some(arp("Arp up", 1, &[0, 1, 2, 3, 2, 1, 0, 1])),
        Some(arp("Arp wide", 1, &[0, 2, 4, 2, 1, 3, 5, 3])),
        Some(line(
            "Hook",
            8.0,
            0.8,
            &[
                (0.0, 0.5, 76),
                (0.75, 0.25, 74),
                (1.0, 1.0, 72),
                (2.5, 0.5, 71),
                (3.0, 1.0, 69),
                (4.0, 0.5, 76),
                (4.75, 0.25, 77),
                (5.0, 1.5, 76),
                (6.5, 0.5, 74),
                (7.0, 1.0, 72),
            ],
        )),
        Some(arp("Arp fast", 2, &[0, 1, 2, 1, 0, 1, 2, 1])),
        None,
    ];
    vec![
        Track::new("Drums", Sound::Drums("Punch"), drums_col),
        Track::new("Bass", Sound::Bass, bass_col),
        Track::new("Chords", Sound::Pad, chord_col),
        Track::new("Lead", Sound::Lead, lead_col),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_set_is_four_by_eight() {
        let set = demo_set();
        assert_eq!(set.len(), TRACKS);
        for t in &set {
            assert_eq!(t.slots.len(), SCENES, "{}", t.name);
            for c in t.slots.iter().flatten() {
                assert!(!c.notes.is_empty(), "{} {}", t.name, c.name);
                assert!(c.notes.windows(2).all(|w| w[0].start <= w[1].start));
            }
        }
    }

    #[test]
    fn drum_rows_become_sixteenths() {
        let c = drums("t", &[(KICK, "x---o---")]);
        assert_eq!(c.beats, 2.0);
        assert_eq!(c.notes.len(), 2);
        assert_eq!(c.notes[1].start, 1.0);
        assert_eq!(c.notes[1].velocity, 0.55);
    }
}
