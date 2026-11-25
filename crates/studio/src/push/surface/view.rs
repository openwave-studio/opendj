//! Views: what the 64 pads (and in Steps, the scene buttons) do and show.

use super::*;

/// What the pads do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// 8 tracks by 8 scenes of clips.
    Session,
    /// The selected track's instrument: drum pads or a keyboard in key.
    Note,
    /// The selected stage clip: a column per stage, the scene buttons
    /// pick what the pads set.
    Steps,
}

impl Surface {
    pub(super) fn pad(&mut self, index: usize, velocity: u8, c: &Controls) {
        let (row, col) = (index / 8, index % 8);
        match self.view {
            View::Session => {
                if velocity == 0 {
                    return;
                }
                let (track, scene) = (bank(c) + col, 7 - row);
                if track >= c.tracks.len() || scene >= SCENES {
                    return;
                }
                if self.shift {
                    c.select(track, Some(scene));
                } else {
                    c.launch(track, scene);
                }
            }
            View::Note if self.sequencing(c) => self.sequencer_pad(index, velocity, c),
            View::Note => {
                if velocity == 0 {
                    if let Some((track, pitch)) = self.held[index].take() {
                        c.note(track, pitch, 0.0);
                    }
                    return;
                }
                let track = c.selected_track();
                if let Some(pitch) = self.note_pitch(c, track, row, col) {
                    if let Some((t, p)) = self.held[index].take() {
                        c.note(t, p, 0.0);
                    }
                    self.held[index] = Some((track, pitch));
                    c.note(track, pitch, velocity as f32 / 127.0);
                }
            }
            View::Steps => self.step_pad(index, velocity > 0, c),
        }
    }

    fn step_pad(&mut self, index: usize, down: bool, c: &Controls) {
        let (row, col) = (index / 8, index % 8);
        let bit = 1u64 << index;
        let (t, s) = c.steps_slot();
        if down {
            self.down |= bit;
            // Two neighbouring pitch pads: the semitone between.
            if self.layer == Layer::Pitch {
                let below = row > 0 && self.down & (bit >> 8) != 0;
                let above = row < 7 && self.down & (bit << 8) != 0;
                if below || above {
                    let low = if below { row - 1 } else { row };
                    c.edit_stages(t, s, |st| {
                        let stage = &mut st.stages[col];
                        stage.degree = low as u8;
                        stage.sharp = true;
                    });
                    self.used |= bit | if below { bit >> 8 } else { bit << 8 };
                }
            }
            return;
        }
        self.down &= !bit;
        if self.used & bit != 0 {
            self.used &= !bit;
            return;
        }
        let (layer, select, shift) = (self.layer, self.select, self.shift);
        c.edit_stages(t, s, |st| {
            steps::press(&mut st.stages[col], layer, row as u8, select, shift)
        });
    }

    /// The note pad (`row` from the bottom, `col`) plays on `track`.
    fn note_pitch(&self, c: &Controls, track: usize, row: usize, col: usize) -> Option<u8> {
        let sound = c.tracks.get(track)?.sound;
        if let Sound::Drums(_) = sound {
            // A 4x4 block in the bottom left.
            if row >= 4 || col >= 4 {
                return None;
            }
            return c.drum_note(track, row * 4 + col);
        }
        let key = c.key();
        let pitch = key.pad(key.home(sound) + self.octave * 12, row, col);
        u8::try_from(pitch).ok().filter(|p| *p < 128)
    }

    pub(super) fn pad_led(
        &self,
        index: usize,
        c: &Controls,
        stages: Option<&steps::Stages>,
        notes: Option<&(f64, Vec<crate::clip::Note>)>,
    ) -> Led {
        let (row, col) = (index / 8, index % 8);
        let m = &c.monitor;
        match self.view {
            View::Session => {
                let (t, s) = (bank(c) + col, 7 - row);
                if t >= c.tracks.len() {
                    return Led::OFF;
                }
                match m.slot(t, s) {
                    // Before the audio thread's first report.
                    SlotState::Empty if c.has_clip(t, s) => Led::solid(track_dim(t)),
                    SlotState::Empty => Led::OFF,
                    SlotState::Stopped => Led::solid(track_dim(t)),
                    SlotState::Queued => Led::animate(track_dim(t), BLINK_EIGHTH, GREEN),
                    SlotState::Playing if m.stopping(t) => {
                        Led::animate(track_color(t), BLINK_EIGHTH, RED)
                    }
                    SlotState::Playing => Led::animate(track_color(t), PULSE_QUARTER, WHITE),
                }
            }
            View::Note if self.sequencing(c) => self.sequencer_led(index, c, notes),
            View::Note => {
                let t = c.selected_track();
                if self.held[index].is_some() {
                    return Led::solid(GREEN);
                }
                let Some(pitch) = self.note_pitch(c, t, row, col) else {
                    return Led::OFF;
                };
                let key = c.key();
                if c.tracks[t].drums() || key.is_root(pitch as i32) {
                    Led::solid(track_color(t))
                } else if key.in_scale(pitch as i32) {
                    Led::solid(SOFT)
                } else {
                    Led::OFF
                }
            }
            View::Steps => {
                if self.down & (1 << index) != 0 {
                    return Led::solid(GREEN);
                }
                let (t, s) = c.steps_slot();
                let Some(st) = stages else {
                    return Led::OFF;
                };
                let playing = m.slot(t, s) == SlotState::Playing && m.stage(t) == Some(col);
                step_led(&st.stages[col], self.layer, row as u8, t, playing)
            }
        }
    }
}

/// A Steps mode pad: `row` (from the bottom) of `stage` on `layer`.
fn step_led(stage: &Stage, layer: Layer, row: u8, t: usize, playing: bool) -> Led {
    let on = Led::solid(if playing { WHITE } else { track_color(t) });
    let dim = Led::solid(track_dim(t));
    // A bar up to `level` rows (1..=8).
    let bar = |level: u8| {
        if row + 1 == level {
            on
        } else if row + 1 < level {
            dim
        } else {
            Led::OFF
        }
    };
    if stage.skip && layer != Layer::Chance {
        return if row == 0 {
            Led::solid(RED_DIM)
        } else {
            Led::OFF
        };
    }
    match layer {
        Layer::Pitch => {
            if row == stage.degree {
                if stage.sharp {
                    Led::solid(AMBER)
                } else {
                    on
                }
            } else if stage.sharp && row == stage.degree + 1 {
                Led::solid(AMBER)
            } else {
                Led::OFF
            }
        }
        Layer::Octave => {
            let at = (stage.octave + 3).clamp(0, 7) as u8;
            if row == at {
                on
            } else if row == 3 {
                Led::solid(GREY)
            } else {
                Led::OFF
            }
        }
        Layer::Velocity => bar((stage.velocity * 8.0).round().clamp(1.0, 8.0) as u8),
        Layer::Gate => {
            if row >= stage.pulses {
                Led::OFF
            } else if stage.gate(row)
                || (stage.tie && stage.gates != 0 && row > stage.gates.trailing_zeros() as u8)
            {
                if stage.tie && !stage.gate(row) {
                    dim
                } else {
                    on
                }
            } else {
                Led::solid(GREY)
            }
        }
        Layer::Chance => {
            if row == 0 {
                Led::solid(if stage.skip { RED } else { GREY })
            } else if stage.skip {
                Led::OFF
            } else {
                let level = (stage.chance * 7.0).round() as u8 + 1;
                bar(level)
            }
        }
        Layer::Locks => {
            let level = (stage.lock_amount * 7.0).round() as u8 + 1;
            if !stage.has_locks() {
                if row < level {
                    Led::solid(GREY)
                } else {
                    Led::OFF
                }
            } else if row + 1 == level {
                Led::solid(if playing { track_color(t) } else { WHITE })
            } else if row + 1 < level {
                Led::solid(SOFT)
            } else {
                Led::OFF
            }
        }
    }
}
