//! Stage clips: eight stages, one per column of Push's pads, each with a
//! note, a number of pulses and which of them play, a chance and
//! parameter locks. A stage clip is edited a layer at a time and turned
//! into an ordinary note clip (plus lock points) for the audio thread.

use crate::clip::{Clip, Lock, Note, Sound};
use crate::scale::Key;
use crate::sound::{DRUM_NOTES, PARAMS};

pub const STAGES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// Scale degree, 0..=7 (7 is the octave). On a drum track, which of
    /// the first eight drum sounds.
    pub degree: u8,
    /// A semitone up: the note between two neighbouring scale degrees.
    pub sharp: bool,
    pub octave: i8,
    pub velocity: f32,
    /// How many pulses the stage lasts, 1..=8.
    pub pulses: u8,
    /// Which pulses play a note: bit 0 the first.
    pub gates: u8,
    /// One note held over the whole stage, from its first gate.
    pub tie: bool,
    /// Left out of the loop altogether: takes no time.
    pub skip: bool,
    /// The chance each of its notes plays, 0..1.
    pub chance: f32,
    /// How much of its locks it plays: 0 none, 1 all.
    pub lock_amount: f32,
    pub locks: [Option<f32>; PARAMS],
}

impl Default for Stage {
    fn default() -> Self {
        Stage {
            degree: 0,
            sharp: false,
            octave: 0,
            velocity: 0.8,
            pulses: 2,
            gates: 0,
            tie: false,
            skip: false,
            chance: 1.0,
            lock_amount: 1.0,
            locks: [None; PARAMS],
        }
    }
}

impl Stage {
    pub fn has_locks(&self) -> bool {
        self.locks.iter().any(Option::is_some)
    }

    pub fn gate(&self, pulse: u8) -> bool {
        pulse < 8 && self.gates & (1 << pulse) != 0
    }

    /// The MIDI note it plays on `sound` in `key`.
    pub fn pitch(&self, sound: Sound, key: &Key) -> u8 {
        if let Sound::Drums(_) = sound {
            return DRUM_NOTES[(self.degree as usize).min(7)];
        }
        let d = self.degree as i32;
        let p = key.home(sound) + 12 * self.octave as i32 + key.degree(d) + self.sharp as i32;
        p.clamp(0, 127) as u8
    }
}

/// What the pads edit in Steps mode, from the top scene button down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Pitch,
    Octave,
    Velocity,
    Gate,
    Chance,
    Locks,
}

impl Layer {
    pub const ALL: [Layer; 6] = [
        Layer::Pitch,
        Layer::Octave,
        Layer::Velocity,
        Layer::Gate,
        Layer::Chance,
        Layer::Locks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Layer::Pitch => "Pitch",
            Layer::Octave => "Octave",
            Layer::Velocity => "Velocity",
            Layer::Gate => "Gate",
            Layer::Chance => "Chance",
            Layer::Locks => "Lock amount",
        }
    }
}

/// A pad press on a layer: `row` from the bottom. With `select` on the
/// gate layer it sets the stage's length, with `shift` a tie.
pub fn press(stage: &mut Stage, layer: Layer, row: u8, select: bool, shift: bool) {
    let row = row.min(7);
    match layer {
        Layer::Pitch => {
            stage.degree = row;
            stage.sharp = false;
        }
        Layer::Octave => stage.octave = row as i8 - 3,
        Layer::Velocity => stage.velocity = (row + 1) as f32 / 8.0,
        Layer::Gate => {
            if select {
                stage.pulses = row + 1;
                stage.gates &= ((1u16 << stage.pulses) - 1) as u8;
            } else if shift {
                stage.tie = !stage.tie;
                if stage.tie && stage.gates == 0 {
                    stage.gates = 1;
                }
            } else {
                // A pulse past the end makes the stage longer.
                if row >= stage.pulses {
                    stage.pulses = row + 1;
                }
                stage.gates ^= 1 << row;
            }
        }
        Layer::Chance => {
            if row == 0 {
                stage.skip = !stage.skip;
            } else {
                stage.skip = false;
                stage.chance = row as f32 / 7.0;
            }
        }
        Layer::Locks => stage.lock_amount = row as f32 / 7.0,
    }
}

/// A stage clip: what Push edits.
#[derive(Clone, Debug, PartialEq)]
pub struct Stages {
    pub stages: [Stage; STAGES],
    /// Beats per pulse.
    pub rate: f64,
}

impl Default for Stages {
    fn default() -> Self {
        Stages {
            stages: [Stage::default(); STAGES],
            rate: 0.25,
        }
    }
}

impl Stages {
    /// The clip the audio thread plays: notes, a lock point at each stage
    /// when any stage has locks, and where each stage starts.
    pub fn clip(&self, name: &str, sound: Sound, key: &Key) -> Clip {
        let locked = self.stages.iter().any(Stage::has_locks);
        let mut notes = Vec::new();
        let mut locks = Vec::new();
        let mut starts = Vec::new();
        let mut t = 0.0;
        for (i, s) in self.stages.iter().enumerate() {
            if s.skip {
                continue;
            }
            let length = s.pulses as f64 * self.rate;
            starts.push((t, i as u8));
            if locked {
                locks.push(Lock {
                    start: t,
                    values: s.locks,
                    amount: s.lock_amount,
                });
            }
            let note = |start: f64, length: f64| Note {
                start,
                length,
                pitch: s.pitch(sound, key),
                velocity: s.velocity,
                chance: s.chance,
            };
            let first = (0..s.pulses).find(|&p| s.gate(p));
            match (s.tie, first) {
                (true, Some(p)) => {
                    let at = p as f64 * self.rate;
                    notes.push(note(t + at, length - at));
                }
                _ => {
                    for p in (0..s.pulses).filter(|&p| s.gate(p)) {
                        notes.push(note(t + p as f64 * self.rate, self.rate * 0.6));
                    }
                }
            }
            t += length;
        }
        let mut clip = Clip::new(name, t, notes);
        clip.locks = locks;
        clip.stage_starts = starts;
        clip.stages = Some(Box::new(self.clone()));
        clip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_become_notes() {
        let mut s = Stages::default();
        // Stage 0: pulses 0 and 1; stage 1 tied over 4 pulses; stage 2
        // skipped.
        press(&mut s.stages[0], Layer::Gate, 0, false, false);
        press(&mut s.stages[0], Layer::Gate, 1, false, false);
        press(&mut s.stages[1], Layer::Gate, 3, false, false);
        press(&mut s.stages[1], Layer::Gate, 0, false, true);
        press(&mut s.stages[1], Layer::Pitch, 2, false, false);
        press(&mut s.stages[2], Layer::Chance, 0, false, false);
        let c = s.clip("t", Sound::Bass, &Key::default());
        // 2 + 4 + 0 (skipped) + 5 x 2 pulses of a sixteenth.
        assert_eq!(c.beats, 16.0 * 0.25);
        assert_eq!(c.notes.len(), 3);
        // Tied from its only gate, the fourth pulse, to the stage's end.
        assert_eq!(c.notes[2].start, 1.25);
        assert_eq!(c.notes[2].length, 0.25);
        assert_eq!(c.notes[2].pitch, 33 + 3);
        assert_eq!(c.stage_starts[2], (1.5, 3));
        assert!(c.locks.is_empty());
    }

    #[test]
    fn length_and_locks() {
        let mut s = Stages::default();
        press(&mut s.stages[0], Layer::Gate, 5, false, false);
        assert_eq!(s.stages[0].pulses, 6);
        press(&mut s.stages[0], Layer::Gate, 2, true, false);
        assert_eq!((s.stages[0].pulses, s.stages[0].gates), (3, 0));
        s.stages[4].locks[0] = Some(0.9);
        let c = s.clip("t", Sound::Lead, &Key::default());
        // A lock point per stage, so the ones without locks go back.
        assert_eq!(c.locks.len(), STAGES);
        assert_eq!(c.locks[4].values[0], Some(0.9));
        assert_eq!(c.locks[3].values[0], None);
    }

    #[test]
    fn drum_rows_are_sounds() {
        let mut st = Stage::default();
        press(&mut st, Layer::Pitch, 1, false, false);
        assert_eq!(st.pitch(Sound::Drums("Punch"), &Key::default()), 38);
        press(&mut st, Layer::Octave, 4, false, false);
        assert_eq!(st.pitch(Sound::Lead, &Key::default()), 57 + 12 + 2);
    }
}
