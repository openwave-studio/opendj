//! The drum sequencer: the Note view on a drum track, laid out as Push
//! lays it out. The top four rows are 32 steps of the selected sound, the
//! bottom-left 4x4 plays and picks sounds, the bottom-right 4x4 holds the
//! clip's pages of 32 steps, and the scene buttons pick the step length.
//!
//! - Tap an empty step to add a note, tap a note to remove it.
//! - Hold a step and tap one to its right: one long note across them.
//! - Hold steps and turn the encoders: velocity, length and chance of their
//!   notes.
//! - Tap a page to see it; hold one and tap another to set the clip's
//!   length to that many pages.
//! - Delete + sound pad clears that sound from the clip; Accent plays and
//!   adds at full velocity.

use super::*;
use crate::clip::Note;

/// Step lengths on the scene buttons, top to bottom: (beats, label).
pub const RESOLUTIONS: [(f64, &str); 8] = [
    (1.0, "1/4"),
    (2.0 / 3.0, "1/4t"),
    (0.5, "1/8"),
    (1.0 / 3.0, "1/8t"),
    (0.25, "1/16"),
    (1.0 / 6.0, "1/16t"),
    (0.125, "1/32"),
    (1.0 / 12.0, "1/32t"),
];

/// Steps on the pads at once.
pub const STEPS: usize = 32;
/// Pad bits of the step rows (the top four).
const STEP_PADS: u64 = 0xFFFF_FFFF_0000_0000;

/// What a pad is in the drum sequencer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqPad {
    /// 0..32, top-left first.
    Step(usize),
    /// 0..16, bottom-left first (as the Note view's drum pads).
    Sound(usize),
    /// 0..16, top-left first.
    Page(usize),
}

pub fn seq_pad(index: usize) -> SeqPad {
    let (row, col) = (index / 8, index % 8);
    if row >= 4 {
        SeqPad::Step((7 - row) * 8 + col)
    } else if col < 4 {
        SeqPad::Sound(row * 4 + col)
    } else {
        SeqPad::Page((3 - row) * 4 + col - 4)
    }
}

/// Whether `n` starts within the step at `start` lasting `len` beats.
fn in_step(n: &Note, pitch: u8, start: f64, len: f64) -> bool {
    n.pitch == pitch && n.start >= start - 1e-6 && n.start < start + len - 1e-6
}

impl Surface {
    /// Whether the pads are the drum sequencer: the Note view on a drum
    /// track.
    pub fn sequencing(&self, c: &Controls) -> bool {
        self.view == View::Note && c.tracks.get(c.selected_track()).is_some_and(|t| t.drums())
    }

    /// The step length in beats.
    pub fn resolution(&self) -> f64 {
        RESOLUTIONS[self.res].0
    }

    /// The selected sound's note.
    pub fn drum_pitch(&self, c: &Controls) -> Option<u8> {
        c.drum_note(c.selected_track(), self.drum)
    }

    /// Where step `step` of the shown page starts, in beats.
    fn step_start(&self, step: usize) -> f64 {
        (self.page * STEPS + step) as f64 * self.resolution()
    }

    /// The steps held down, lowest first.
    pub fn held_steps(&self) -> Vec<usize> {
        (32..64)
            .filter(|i| self.down & (1 << i) != 0)
            .filter_map(|i| match seq_pad(i) {
                SeqPad::Step(s) => Some(s),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// The notes of the selected sound in the held steps.
    pub fn held_notes(&self, c: &Controls) -> Vec<Note> {
        let (Some(pitch), held) = (self.drum_pitch(c), self.held_steps()) else {
            return Vec::new();
        };
        let (t, s) = c.steps_slot();
        let Some((_, notes)) = c.notes(t, s) else {
            return Vec::new();
        };
        let len = self.resolution();
        notes
            .into_iter()
            .filter(|n| {
                held.iter()
                    .any(|&h| in_step(n, pitch, self.step_start(h), len))
            })
            .collect()
    }

    pub(super) fn sequencer_pad(&mut self, index: usize, velocity: u8, c: &Controls) {
        let bit = 1u64 << index;
        let down = velocity > 0;
        let (t, s) = c.steps_slot();
        let len = self.resolution();
        match seq_pad(index) {
            SeqPad::Step(step) => {
                let Some(pitch) = self.drum_pitch(c) else {
                    return;
                };
                let start = self.step_start(step);
                if !down {
                    self.down &= !bit;
                    let quiet = (self.used | self.added) & bit != 0;
                    self.used &= !bit;
                    self.added &= !bit;
                    if !quiet {
                        c.edit_notes(t, s, |clip| {
                            clip.notes.retain(|n| !in_step(n, pitch, start, len))
                        });
                    }
                    return;
                }
                // A held step to the left: one note from it to here.
                let first = self.held_steps().into_iter().find(|&h| h < step);
                self.down |= bit;
                if let Some(first) = first {
                    let from = self.step_start(first);
                    c.edit_notes(t, s, |clip| {
                        if !clip.notes.iter().any(|n| in_step(n, pitch, from, len)) {
                            clip.notes.push(self.new_note(from, pitch, velocity));
                        }
                        for n in clip
                            .notes
                            .iter_mut()
                            .filter(|n| in_step(n, pitch, from, len))
                        {
                            n.length = start + len - n.start;
                        }
                    });
                    self.used |= self.down & STEP_PADS;
                    return;
                }
                let Some((beats, notes)) = c.notes(t, s) else {
                    return;
                };
                if start >= beats - 1e-6 {
                    self.used |= bit;
                    return;
                }
                if !notes.iter().any(|n| in_step(n, pitch, start, len)) {
                    let note = self.new_note(start, pitch, velocity);
                    c.edit_notes(t, s, |clip| clip.notes.push(note));
                    self.added |= bit;
                }
            }
            SeqPad::Sound(i) => {
                if !down {
                    if let Some((track, pitch)) = self.held[index].take() {
                        c.note(track, pitch, 0.0);
                    }
                    return;
                }
                let Some(pitch) = c.drum_note(c.selected_track(), i) else {
                    return;
                };
                if self.pressed[DELETE as usize].is_some() {
                    c.edit_notes(t, s, |clip| clip.notes.retain(|n| n.pitch != pitch));
                    return;
                }
                self.drum = i;
                let v = if self.accent {
                    1.0
                } else {
                    velocity as f32 / 127.0
                };
                self.held[index] = Some((t, pitch));
                c.note(t, pitch, v);
            }
            SeqPad::Page(i) => {
                if down {
                    // Another page held: the clip runs to the later one.
                    let other = (0..32)
                        .filter(|&p| p != index && self.down & (1 << p) != 0)
                        .find_map(|p| match seq_pad(p) {
                            SeqPad::Page(o) => Some(o),
                            _ => None,
                        });
                    self.down |= bit;
                    if let Some(o) = other {
                        let pages = i.max(o) + 1;
                        let beats = (pages * STEPS) as f64 * len;
                        c.edit_notes(t, s, |clip| clip.beats = beats);
                        self.used |= self.down & !STEP_PADS;
                    }
                } else {
                    self.down &= !bit;
                    if self.used & bit != 0 {
                        self.used &= !bit;
                    } else {
                        self.page = i;
                    }
                }
            }
        }
    }

    fn new_note(&self, start: f64, pitch: u8, velocity: u8) -> Note {
        Note {
            start,
            length: self.resolution(),
            pitch,
            velocity: if self.accent {
                1.0
            } else {
                (velocity as f32 / 127.0).max(0.05)
            },
            chance: 1.0,
        }
    }

    /// Encoder `i` turned while steps are held: their notes' velocity,
    /// length or chance.
    pub(super) fn edit_held_notes(&mut self, i: usize, step: f32, c: &Controls) {
        let Some(pitch) = self.drum_pitch(c) else {
            return;
        };
        let (t, s) = c.steps_slot();
        let len = self.resolution();
        let starts: Vec<f64> = self
            .held_steps()
            .iter()
            .map(|&h| self.step_start(h))
            .collect();
        c.edit_notes(t, s, |clip| {
            let beats = clip.beats;
            for n in clip
                .notes
                .iter_mut()
                .filter(|n| starts.iter().any(|&st| in_step(n, pitch, st, len)))
            {
                match i {
                    0 => n.velocity = (n.velocity + step).clamp(0.01, 1.0),
                    1 => {
                        // A sixteenth of a step per notch.
                        let l = n.length + step as f64 / 0.005 * len / 16.0;
                        n.length = l.clamp(len / 16.0, beats);
                    }
                    2 => n.chance = (n.chance + step).clamp(0.0, 1.0),
                    _ => {}
                }
            }
        });
        self.used |= self.down & STEP_PADS;
    }

    pub(super) fn sequencer_led(
        &self,
        index: usize,
        c: &Controls,
        clip: Option<&(f64, Vec<Note>)>,
    ) -> Led {
        let (t, s) = c.steps_slot();
        let m = &c.monitor;
        let Some((beats, notes)) = clip else {
            return Led::OFF;
        };
        let len = self.resolution();
        let playing = m.slot(t, s) == SlotState::Playing;
        let at = playing.then(|| m.progress(t).max(0.0) as f64 * beats);
        if self.down & (1 << index) != 0 {
            return Led::solid(GREEN);
        }
        match seq_pad(index) {
            SeqPad::Step(step) => {
                let Some(pitch) = self.drum_pitch(c) else {
                    return Led::OFF;
                };
                let start = self.step_start(step);
                if start >= beats - 1e-6 {
                    return Led::OFF;
                }
                if at.is_some_and(|b| b >= start && b < start + len) {
                    return Led::solid(WHITE);
                }
                if notes.iter().any(|n| in_step(n, pitch, start, len)) {
                    Led::solid(track_color(t))
                } else if notes.iter().any(|n| {
                    n.pitch == pitch && n.start < start && n.start + n.length > start + 1e-6
                }) {
                    // Held over from a note before.
                    Led::solid(track_dim(t))
                } else {
                    Led::solid(GREY)
                }
            }
            SeqPad::Sound(i) => {
                if self.held[index].is_some() {
                    return Led::solid(GREEN);
                }
                let Some(pitch) = c.drum_note(c.selected_track(), i) else {
                    return Led::OFF;
                };
                if i == self.drum {
                    Led::solid(track_color(t))
                } else if notes.iter().any(|n| n.pitch == pitch) {
                    Led::solid(SOFT)
                } else {
                    Led::solid(track_dim(t))
                }
            }
            SeqPad::Page(i) => {
                let page_beats = STEPS as f64 * len;
                let pages = (beats / page_beats - 1e-6).ceil().max(1.0) as usize;
                if i >= pages {
                    Led::OFF
                } else if i == self.page {
                    Led::solid(WHITE)
                } else if at.is_some_and(|b| (b / page_beats) as usize == i) {
                    Led::solid(GREEN)
                } else {
                    Led::solid(track_dim(t))
                }
            }
        }
    }
}
