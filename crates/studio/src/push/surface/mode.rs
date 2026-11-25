//! Modes: what the eight encoders, the buttons over and under the screen
//! and the screen do and show.

use super::*;

/// What the eight encoders over the display turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The selected track's instrument knobs.
    Device,
    /// One mixer control across the tracks.
    Mix(Mix),
    /// The key: scale, root, pad layout, in key or chromatic.
    Scales,
}

/// A labelled button over (upper) or under (lower) the screen.
pub struct Menu {
    pub label: String,
    pub on: bool,
}

impl Surface {
    pub(super) fn encoder(&mut self, cc: u8, delta: i32, c: &Controls) {
        // About 210 steps a turn: a full sweep in one turn, finer with Shift.
        let step = if self.shift { 0.001 } else { 0.005 } * delta as f32;
        match cc {
            TEMPO_ENCODER => c.tempo_step(if self.shift { 0.1 } else { 1.0 } * delta as f64),
            MASTER_ENCODER => c.set_main_volume(c.main_volume() + step),
            _ => {
                let i = (cc - ENCODER_FIRST) as usize;
                let held = self.held_stages();
                if self.view == View::Steps && !held.is_empty() {
                    // Held stages: a lock on each, from where it was.
                    if i < PARAMS {
                        let (t, s) = c.steps_slot();
                        let now = c.param(t, i);
                        c.edit_stages(t, s, |st| {
                            for &h in &held {
                                let l = &mut st.stages[h].locks[i];
                                *l = Some((l.unwrap_or(now) + step).clamp(0.0, 1.0));
                            }
                        });
                    }
                    self.used |= self.down;
                    return;
                }
                if self.sequencing(c) && !self.held_steps().is_empty() {
                    self.edit_held_notes(i, step, c);
                    return;
                }
                match self.mode {
                    Mode::Device => {
                        if i < PARAMS {
                            let t = c.selected_track();
                            c.set_param(t, i, c.param(t, i) + step);
                        }
                    }
                    Mode::Mix(m) => {
                        let t = bank(c) + i;
                        if t < c.tracks.len() {
                            c.set_mix(t, m, c.mix(t, m) + step);
                        }
                    }
                    Mode::Scales => {
                        // A step per detent's worth of turning.
                        self.turns[i] += delta;
                        let by = self.turns[i] / 6;
                        self.turns[i] %= 6;
                        if by != 0 {
                            c.edit_key(|k| match i {
                                0 => k.next_scale(by),
                                1 => k.root = (k.root as i32 + by).rem_euclid(12) as u8,
                                2 => k.next_layout(by),
                                3 => k.chromatic = by > 0,
                                _ => {}
                            });
                        }
                    }
                }
            }
        }
    }
}

impl Surface {
    /// A button in Scales mode; false when it isn't one of its own.
    ///
    /// Over the screen: previous scale, the roots C G D A E B, the pad
    /// layout. Under it: next scale, the roots F Bb Eb Ab Db Gb, in key or
    /// chromatic. Page left and right step through the scales too.
    pub(super) fn scale_button(&mut self, cc: u8, c: &Controls) -> bool {
        let upper = (UPPER_FIRST..UPPER_FIRST + 8).contains(&cc);
        let lower = (LOWER_FIRST..LOWER_FIRST + 8).contains(&cc);
        if cc == PAGE_LEFT || cc == PAGE_RIGHT {
            c.edit_key(|k| k.next_scale(if cc == PAGE_RIGHT { 1 } else { -1 }));
            return true;
        }
        if !upper && !lower {
            return false;
        }
        let i = (cc - if upper { UPPER_FIRST } else { LOWER_FIRST }) as usize;
        c.edit_key(|k| match (upper, i) {
            (true, 0) => k.next_scale(-1),
            (false, 0) => k.next_scale(1),
            (true, 7) => k.next_layout(1),
            (false, 7) => k.chromatic = !k.chromatic,
            (true, i) => k.root = UPPER_ROOTS[i - 1],
            (false, i) => k.root = LOWER_ROOTS[i - 1],
        });
        true
    }

    /// The buttons over and under the screen in Scales mode.
    pub(super) fn scale_leds(&self, c: &Controls) -> Vec<(u8, Led)> {
        let key = c.key();
        let root = |r: u8| Led::solid(if key.root == r { WHITE } else { GREY });
        let mut v = vec![
            (UPPER_FIRST, Led::solid(SOFT)),
            (LOWER_FIRST, Led::solid(SOFT)),
            (UPPER_FIRST + 7, Led::solid(SOFT)),
            (
                LOWER_FIRST + 7,
                Led::solid(if key.chromatic { WHITE } else { SOFT }),
            ),
        ];
        for i in 0..6 {
            v.push((UPPER_FIRST + 1 + i as u8, root(UPPER_ROOTS[i])));
            v.push((LOWER_FIRST + 1 + i as u8, root(LOWER_ROOTS[i])));
        }
        v
    }

    /// Labels for the buttons over and under the screen, when the mode
    /// gives them their own jobs.
    pub fn menus(&self, c: &Controls) -> Option<(Vec<Menu>, Vec<Menu>)> {
        if self.mode != Mode::Scales {
            return None;
        }
        let key = c.key();
        let item = |label: &str, on: bool| Menu {
            label: label.to_string(),
            on,
        };
        let roots =
            |list: [u8; 6]| list.map(|r| item(crate::scale::NOTE_NAMES[r as usize], key.root == r));
        let mut upper = vec![item("◀ Scale", false)];
        upper.extend(roots(UPPER_ROOTS));
        upper.push(item(key.layout.label(), false));
        let mut lower = vec![item("Scale ▶", false)];
        lower.extend(roots(LOWER_ROOTS));
        lower.push(item("Chromatic", key.chromatic));
        Some((upper, lower))
    }
}
