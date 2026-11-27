//! Editing in the piano roll (step 3 of add-studio-piano-roll): the pointer
//! on the grid, the keys and the lanes, the keyboard shortcuts and the
//! right-click menu. Every change goes through `Controls::edit_notes`, so
//! the clip recompiles and swaps on the audio thread without a click.

use opendj_studio_engine::note_edit::{self, Snap};

use super::{Editor, Fold, Lane, UNDO_DEPTH};
use crate::clip::Note;
use crate::control::Controls;

/// The pointer's distance from a note's end that grabs its end, in pixels.
const EDGE: f64 = 4.0;
/// The velocity of drawn and heard notes.
const VELOCITY: f32 = 100.0 / 127.0;

/// What a press on the grid or a lane is doing until it is let go.
#[derive(Clone, Debug)]
pub(super) enum Drag {
    /// Moving the selection; `orig` and `sel` as they were at the press.
    Move {
        orig: Vec<Note>,
        sel: Vec<bool>,
        x0: f64,
        y0: f64,
        /// The pressed note's row, to hear its new pitch.
        row: usize,
        moved: bool,
    },
    /// Dragging the selected notes' ends.
    Resize {
        orig: Vec<Note>,
        sel: Vec<bool>,
        x0: f64,
        moved: bool,
    },
    /// A box on empty grid: the notes it touches join `base`.
    Box {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        base: Vec<bool>,
    },
    /// A velocity or chance marker: the pressed note, or the selection when
    /// the pressed note is in it, moves by the drag.
    Lane {
        orig: Vec<Note>,
        sel: Vec<bool>,
        y0: f64,
        h: f64,
        moved: bool,
    },
    /// Draw mode in a lane: notes under the pointer take its height.
    LaneDraw { h: f64 },
}

/// A right-click menu line: the action, its name, its shortcut, a tick,
/// whether it can run now, and a line above it.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: &'static str,
    pub label: &'static str,
    pub keys: String,
    pub ticked: bool,
    pub enabled: bool,
    pub line: bool,
}

/// The command key as menus write it.
fn cmd() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    }
}

/// A lane value (velocity or chance) of a note.
fn lane_value(n: &Note, lane: Lane) -> f32 {
    match lane {
        Lane::Velocity => n.velocity,
        Lane::Chance => n.chance,
    }
}

fn set_lane_value(n: &mut Note, lane: Lane, v: f32) {
    match lane {
        Lane::Velocity => n.velocity = v.clamp(1.0 / 127.0, 1.0),
        Lane::Chance => n.chance = v.clamp(0.0, 1.0),
    }
}

impl Editor {
    // ------------------------------------------------------ the clip

    /// The open clip's notes, with the selection made to fit them.
    pub(super) fn notes(&mut self, c: &Controls) -> Vec<Note> {
        let Some((t, s)) = self.open else {
            return Vec::new();
        };
        let notes = c.notes(t, s).map(|(_, n)| n).unwrap_or_default();
        if self.sel.len() != notes.len() {
            self.sel = vec![false; notes.len()];
        }
        notes
    }

    /// Changes the open clip and the selection, kept for undo.
    pub fn edit(
        &mut self,
        c: &Controls,
        f: impl FnOnce(&mut Vec<Note>, &mut Vec<bool>, &mut f64, &mut f64),
    ) {
        let Some((t, s)) = self.open else { return };
        self.notes(c);
        let mut sel = std::mem::take(&mut self.sel);
        let mut before = None;
        c.edit_notes(t, s, |clip| {
            before = Some((clip.notes.clone(), clip.loop_start, clip.beats));
            sel.resize(clip.notes.len(), false);
            f(
                &mut clip.notes,
                &mut sel,
                &mut clip.loop_start,
                &mut clip.beats,
            );
            note_edit::sort(&mut clip.notes, &mut sel);
        });
        self.sel = sel;
        if let Some(b) = before {
            if b.0 != c.notes(t, s).map(|(_, n)| n).unwrap_or_default()
                || b.1 != c.loop_start(t, s)
                || Some(b.2) != c.clip_info(t, s).map(|(_, beats)| beats)
            {
                self.push_undo(b);
            }
        }
    }

    pub(super) fn push_undo(&mut self, b: super::Snapshot) {
        self.undo.push(b);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Puts notes in the clip as a drag goes, without an undo step each.
    fn set_notes(&mut self, c: &Controls, notes: Vec<Note>, mut sel: Vec<bool>) {
        let Some((t, s)) = self.open else { return };
        let mut notes = notes;
        note_edit::sort(&mut notes, &mut sel);
        c.edit_notes(t, s, |clip| clip.notes = notes);
        self.sel = sel;
    }

    /// Undo (or redo) the last edit.
    pub fn undo(&mut self, c: &Controls, redo: bool) {
        let Some((t, s)) = self.open else { return };
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some((notes, ls, beats)) = from.pop() else {
            return;
        };
        c.edit_notes(t, s, |clip| {
            to.push((clip.notes.clone(), clip.loop_start, clip.beats));
            clip.notes = notes;
            clip.loop_start = ls;
            clip.beats = beats;
        });
        self.sel.clear();
    }

    /// The selection, or every note when nothing is selected: what the
    /// tools work on.
    fn sel_or_all(sel: &[bool]) -> Vec<bool> {
        if sel.iter().any(|&s| s) {
            sel.to_vec()
        } else {
            vec![true; sel.len()]
        }
    }

    pub fn selected(&self) -> usize {
        self.sel.iter().filter(|&&s| s).count()
    }

    // ------------------------------------------------------ hearing

    /// Plays `pitch` on the track (ending the note heard before).
    pub fn hear(&mut self, c: &Controls, pitch: Option<u8>, velocity: f32) {
        let Some((t, _)) = self.open else { return };
        if self.sounding == pitch && pitch.is_some() {
            return;
        }
        if let Some(p) = self.sounding.take() {
            c.note(t, p, 0.0);
        }
        if let Some(p) = pitch {
            if self.hear {
                c.note(t, p, velocity.max(0.05));
                self.sounding = Some(p);
            }
        }
    }

    /// A press (or release) on the keys: plays that key (the keys always
    /// sound; the headphones are for edits).
    pub fn audition(&mut self, c: &Controls, y: f64, down: bool) {
        let Some((t, _)) = self.open else { return };
        if let Some(p) = self.sounding.take() {
            c.note(t, p, 0.0);
        }
        if let Some(p) = self.pitch_at(y).filter(|_| down) {
            c.note(t, p, VELOCITY);
            self.sounding = Some(p);
        }
    }

    // ------------------------------------------------- hit testing

    pub(super) fn beat_at(&self, x: f64) -> f64 {
        self.view.first + x / self.view.ppb.max(1e-6)
    }

    fn row_at(&self, y: f64) -> Option<usize> {
        if self.rh <= 0.0 {
            return None;
        }
        let i = ((y + self.view.top) / self.rh).floor();
        (i >= 0.0 && (i as usize) < self.layout.len()).then_some(i as usize)
    }

    pub(super) fn pitch_at(&self, y: f64) -> Option<u8> {
        self.row_at(y).map(|i| self.layout[i])
    }

    fn row_of(&self, pitch: u8) -> Option<usize> {
        self.layout.iter().position(|&p| p == pitch)
    }

    /// The note under the pointer (the last drawn wins) and whether the
    /// pointer is on its end.
    pub(super) fn note_at(&self, notes: &[Note], x: f64, y: f64) -> Option<(usize, bool)> {
        let pitch = self.pitch_at(y)?;
        let beat = self.beat_at(x);
        let tol = EDGE / self.view.ppb.max(1e-6);
        notes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, n)| n.pitch == pitch && beat >= n.start && beat <= n.end() + tol * 0.5)
            .map(|(i, n)| {
                let w = n.length * self.view.ppb;
                (
                    i,
                    w >= 2.0 * EDGE && (n.end() - beat) * self.view.ppb <= EDGE,
                )
            })
    }

    /// How a drag snaps: to whole grid steps, onto the grid (Shift), or
    /// not at all (Alt, or snap off).
    fn snap_mode(&self, shift: bool, alt: bool) -> Snap {
        match self.grid.snap_step(self.view.ppb) {
            Some(step) if !alt => {
                if shift {
                    Snap::Absolute(step)
                } else {
                    Snap::Relative(step)
                }
            }
            _ => Snap::Free,
        }
    }

    /// What the pointer over the grid would grab: 0 nothing, 1 a note's
    /// end, 2 a note.
    pub fn hover(&mut self, c: &Controls, x: f64, y: f64) {
        let notes = self.notes(c);
        self.cursor = match self.note_at(&notes, x, y) {
            Some((_, true)) => 1,
            Some(_) => 2,
            None => 0,
        };
    }

    // ------------------------------------------------------ pointer

    /// A press on the grid.
    pub fn press(&mut self, c: &Controls, x: f64, y: f64, shift: bool) {
        let notes = self.notes(c);
        let beat = self.beat_at(x);
        match self.note_at(&notes, x, y) {
            Some((i, _)) if self.draw && !shift => {
                // Draw mode: a click on a note takes it away.
                self.edit(c, |notes, sel, _, _| {
                    let mut only = vec![false; notes.len()];
                    only[i] = true;
                    note_edit::delete(notes, &mut only);
                    *sel = only;
                });
            }
            Some((i, edge)) => {
                if shift {
                    self.sel[i] = !self.sel[i];
                } else if !self.sel[i] {
                    self.sel.iter_mut().for_each(|s| *s = false);
                    self.sel[i] = true;
                }
                if self.sel[i] {
                    let n = notes[i];
                    self.hear(c, Some(n.pitch), n.velocity);
                    self.drag = Some(if edge {
                        Drag::Resize {
                            orig: notes,
                            sel: self.sel.clone(),
                            x0: x,
                            moved: false,
                        }
                    } else {
                        Drag::Move {
                            orig: notes,
                            sel: self.sel.clone(),
                            x0: x,
                            y0: y,
                            row: self.row_of(n.pitch).unwrap_or(0),
                            moved: false,
                        }
                    });
                    self.frozen = Some(self.layout.clone());
                }
            }
            None => {
                let step = self.grid.snap_step(self.view.ppb);
                self.insert = step
                    .map_or(beat, |s| note_edit::snap_down(beat, s))
                    .max(0.0);
                if self.draw {
                    if let Some(pitch) = self.pitch_at(y) {
                        self.draw_note(c, beat, pitch);
                        let notes = self.notes(c);
                        self.drag = Some(Drag::Resize {
                            orig: notes,
                            sel: self.sel.clone(),
                            x0: x,
                            moved: true,
                        });
                        self.frozen = Some(self.layout.clone());
                    }
                } else {
                    if !shift {
                        self.sel.iter_mut().for_each(|s| *s = false);
                    }
                    self.drag = Some(Drag::Box {
                        x0: x,
                        y0: y,
                        x1: x,
                        y1: y,
                        base: self.sel.clone(),
                    });
                }
            }
        }
    }

    /// Adds a note one grid step long at `beat`, and hears it.
    fn draw_note(&mut self, c: &Controls, beat: f64, pitch: u8) {
        let step = self
            .grid
            .snap_step(self.view.ppb)
            .or(Some(self.grid.step(self.view.ppb)));
        let snapped = self.grid.snap;
        self.edit(c, |notes, sel, _, _| {
            note_edit::draw(
                notes,
                sel,
                beat,
                pitch,
                VELOCITY,
                if snapped { step } else { None },
            );
        });
        self.hear(c, Some(pitch), VELOCITY);
    }

    /// The pointer moving with the button down.
    pub fn drag_to(&mut self, c: &Controls, x: f64, y: f64, shift: bool, alt: bool) {
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let snap = self.snap_mode(shift, alt);
        match &mut drag {
            Drag::Move {
                orig,
                sel,
                x0,
                y0,
                row,
                moved,
            } => {
                let beats = (x - *x0) / self.view.ppb;
                let rows = ((*y0 - y) / self.rh.max(1.0)).round() as i64;
                if !*moved && (x - *x0).abs() < 3.0 && (y - *y0).abs() < 3.0 {
                    self.drag = Some(drag);
                    return;
                }
                if !*moved {
                    *moved = true;
                    self.push_current_undo(c);
                }
                let layout = self.frozen.clone().unwrap_or_else(|| self.layout.clone());
                let mut notes = orig.clone();
                let mut s = sel.clone();
                let d = shift_rows(&mut notes, &s, &layout, rows);
                note_edit::move_by(&mut notes, &mut s, beats, 0, snap);
                self.set_notes(c, notes, s);
                let pitch = layout.get((*row as i64 - d).max(0) as usize).copied();
                if let Some(p) = pitch {
                    self.hear(c, Some(p), VELOCITY);
                }
            }
            Drag::Resize {
                orig,
                sel,
                x0,
                moved,
            } => {
                if !*moved && (x - *x0).abs() < 3.0 {
                    self.drag = Some(drag);
                    return;
                }
                if !*moved {
                    *moved = true;
                    self.push_current_undo(c);
                }
                let mut notes = orig.clone();
                note_edit::resize_by(&mut notes, sel, (x - *x0) / self.view.ppb, snap);
                self.set_notes(c, notes, sel.clone());
            }
            Drag::Box {
                x0,
                y0,
                x1,
                y1,
                base,
            } => {
                *x1 = x;
                *y1 = y;
                let notes = self.notes(c);
                let (bx0, bx1) = (x0.min(*x1), x0.max(*x1));
                let (by0, by1) = (y0.min(*y1), y0.max(*y1));
                let (b0, b1) = (self.beat_at(bx0), self.beat_at(bx1));
                let mut sel = base.clone();
                sel.resize(notes.len(), false);
                for (i, n) in notes.iter().enumerate() {
                    let Some(r) = self.row_of(n.pitch) else {
                        continue;
                    };
                    let (ny0, ny1) = (
                        r as f64 * self.rh - self.view.top,
                        (r + 1) as f64 * self.rh - self.view.top,
                    );
                    if n.end() > b0 && n.start < b1 && ny1 > by0 && ny0 < by1 {
                        sel[i] = true;
                    }
                }
                self.sel = sel;
            }
            Drag::Lane { .. } | Drag::LaneDraw { .. } => {}
        }
        self.drag = Some(drag);
    }

    /// Keeps the clip as it is now for undo (a drag's first step).
    fn push_current_undo(&mut self, c: &Controls) {
        let Some((t, s)) = self.open else { return };
        if let Some((beats, notes)) = c.notes(t, s) {
            self.push_undo((notes, c.loop_start(t, s), beats));
        }
    }

    /// The button let go.
    pub fn release(&mut self, c: &Controls) {
        self.drag = None;
        self.frozen = None;
        self.hear(c, None, 0.0);
    }

    /// A double-click: a note on empty grid, or a note taken away.
    pub fn double(&mut self, c: &Controls, x: f64, y: f64) {
        self.drag = None;
        self.frozen = None;
        let notes = self.notes(c);
        match self.note_at(&notes, x, y) {
            Some((i, _)) => {
                if self.draw {
                    // The first click of the two already took it away.
                    return;
                }
                self.edit(c, |notes, sel, _, _| {
                    let mut only = vec![false; notes.len()];
                    only[i] = true;
                    note_edit::delete(notes, &mut only);
                    *sel = only;
                });
            }
            None if !self.draw => {
                if let Some(pitch) = self.pitch_at(y) {
                    self.draw_note(c, self.beat_at(x), pitch);
                    self.hear(c, None, 0.0);
                }
            }
            None => {}
        }
    }

    /// The box being drawn, in the grid's pixels.
    pub fn box_rect(&self) -> Option<(f64, f64, f64, f64)> {
        match &self.drag {
            Some(Drag::Box { x0, y0, x1, y1, .. }) => {
                Some((x0.min(*x1), y0.min(*y1), (x1 - x0).abs(), (y1 - y0).abs()))
            }
            _ => None,
        }
    }

    // ------------------------------------------------------- lanes

    /// A press in the velocity or chance lane (`h` its height).
    pub fn lane_press(&mut self, c: &Controls, x: f64, y: f64, h: f64) {
        let notes = self.notes(c);
        if self.draw {
            self.push_current_undo(c);
            self.drag = Some(Drag::LaneDraw { h });
            self.lane_drag(c, x, y);
            return;
        }
        // The nearest marker within a few pixels, selected ones first.
        let near = |want_sel: bool| {
            notes
                .iter()
                .enumerate()
                .filter(|(i, _)| !want_sel || self.sel[*i])
                .map(|(i, n)| (i, (self.view.x(n.start) + 2.0 - x).abs()))
                .filter(|(_, d)| *d <= 5.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        let Some(i) = near(true).or_else(|| near(false)) else {
            return;
        };
        if !self.sel[i] {
            self.sel.iter_mut().for_each(|s| *s = false);
            self.sel[i] = true;
        }
        self.drag = Some(Drag::Lane {
            orig: notes,
            sel: self.sel.clone(),
            y0: y,
            h,
            moved: false,
        });
    }

    /// The pointer moving in a lane with the button down.
    pub fn lane_drag(&mut self, c: &Controls, x: f64, y: f64) {
        let lane = self.lane;
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        match &mut drag {
            Drag::Lane {
                orig,
                sel,
                y0,
                h,
                moved,
            } => {
                if !*moved {
                    *moved = true;
                    self.push_current_undo(c);
                }
                let d = ((*y0 - y) / h.max(1.0)) as f32;
                let mut notes = orig.clone();
                for (n, _) in notes.iter_mut().zip(sel.iter()).filter(|(_, &s)| s) {
                    let v = lane_value(n, lane) + d;
                    set_lane_value(n, lane, v);
                }
                self.set_notes(c, notes, sel.clone());
            }
            Drag::LaneDraw { h, .. } => {
                let v = (1.0 - y / h.max(1.0)) as f32;
                let mut notes = self.notes(c);
                let sel = self.sel.clone();
                let any = sel.iter().any(|&s| s);
                let mut changed = false;
                for (n, &s) in notes.iter_mut().zip(&sel) {
                    if (any && !s) || (self.view.x(n.start) + 2.0 - x).abs() > 4.0 {
                        continue;
                    }
                    set_lane_value(n, lane, v);
                    changed = true;
                }
                if changed {
                    self.set_notes(c, notes, sel);
                }
            }
            _ => {}
        }
        self.drag = Some(drag);
    }

    // ---------------------------------------------- actions and keys

    /// The keyboard: Live's shortcuts. True when the key was used.
    pub fn key(&mut self, c: &Controls, text: &str, shift: bool, cmd: bool, alt: bool) -> bool {
        use slint::platform::Key;
        let is = |k: Key| text == slint::SharedString::from(k).as_str();
        let lower = text.to_lowercase();
        let id = if is(Key::Backspace) || is(Key::Delete) {
            "delete"
        } else if is(Key::Escape) {
            "deselect"
        } else if is(Key::UpArrow) || is(Key::DownArrow) {
            let up = is(Key::UpArrow);
            let steps = if shift { 12 } else { 1 };
            self.arrow_pitch(c, if up { steps } else { -steps });
            return true;
        } else if is(Key::LeftArrow) || is(Key::RightArrow) {
            let dir = if is(Key::RightArrow) { 1.0 } else { -1.0 };
            self.arrow_time(c, dir, alt);
            return true;
        } else if cmd {
            match (lower.as_str(), shift) {
                ("z", false) => "undo",
                ("z", true) => "redo",
                ("a", false) => "select-all",
                ("a", true) => "invert",
                ("c", _) => "copy",
                ("x", _) => "cut",
                ("v", _) => "paste",
                ("d", _) => "duplicate",
                ("u", false) => "quantize",
                ("u", true) => "quantize-settings",
                ("e", _) => "chop",
                ("j", _) => "join",
                ("l", _) => "loop-selection",
                ("1", _) => "narrow",
                ("2", _) => "widen",
                ("3", _) => "triplet",
                ("4", _) => "snap",
                _ => return false,
            }
        } else if alt {
            return false;
        } else {
            match lower.as_str() {
                "0" => "deactivate",
                "z" => "zoom-selection",
                "x" => "zoom-clip",
                "w" => "fit-width",
                "h" => "fit-height",
                "b" => "draw",
                "k" => "highlight",
                _ => return false,
            }
        };
        self.action(c, id);
        true
    }

    /// Up and down: the selection by semitones (or scale steps when folded
    /// to the scale; an octave with Shift).
    fn arrow_pitch(&mut self, c: &Controls, steps: i32) {
        let key = c.key();
        let by_scale = self.fold() == Fold::Scale && steps.abs() == 1;
        self.edit(c, |notes, sel, _, _| {
            if by_scale {
                note_edit::transpose_in_scale(notes, sel, steps, &key);
            } else {
                note_edit::transpose(notes, sel, steps);
            }
        });
        let notes = self.notes(c);
        if let Some(n) = notes
            .iter()
            .zip(&self.sel)
            .find(|(_, &s)| s)
            .map(|(n, _)| *n)
        {
            self.hear(c, Some(n.pitch), n.velocity);
            self.hear(c, None, 0.0);
        }
    }

    /// Left and right: the selection by a grid step (Alt: a sixteenth of
    /// one, to nudge off the grid).
    fn arrow_time(&mut self, c: &Controls, dir: f64, fine: bool) {
        let step = self.grid.step(self.view.ppb);
        let by = if fine { step / 16.0 } else { step } * dir;
        self.edit(c, |notes, sel, _, _| {
            note_edit::move_by(notes, sel, by, 0, Snap::Free);
        });
    }

    /// Runs a menu or keyboard action by its name.
    pub fn action(&mut self, c: &Controls, id: &str) {
        let step = self.grid.step(self.view.ppb);
        let key = c.key();
        match id {
            "undo" => self.undo(c, false),
            "redo" => self.undo(c, true),
            "deselect" => self.sel.iter_mut().for_each(|s| *s = false),
            "select-all" => {
                self.notes(c);
                self.sel.iter_mut().for_each(|s| *s = true);
            }
            "invert" => {
                self.notes(c);
                note_edit::invert(&mut self.sel);
            }
            "copy" => {
                let notes = self.notes(c);
                self.clipboard = note_edit::copy(&notes, &self.sel);
                if let Some((lo, hi)) = note_edit::span(&notes, &self.sel) {
                    // Paste lands after what was copied.
                    self.insert = lo + ((hi - lo) / step - 1e-9).ceil().max(1.0) * step;
                }
            }
            "cut" => {
                let notes = self.notes(c);
                let lo = note_edit::span(&notes, &self.sel).map(|(lo, _)| lo);
                self.action(c, "copy");
                self.action(c, "delete");
                if let Some(lo) = lo {
                    self.insert = lo;
                }
            }
            "paste" => {
                let clip = self.clipboard.clone();
                let at = self.insert;
                if !clip.is_empty() {
                    let len = clip.iter().map(Note::end).fold(0.0, f64::max);
                    self.edit(c, |notes, sel, _, _| {
                        note_edit::paste(notes, sel, &clip, at)
                    });
                    self.insert = at + (len / step - 1e-9).ceil().max(1.0) * step;
                }
            }
            "delete" => self.edit(c, |notes, sel, _, _| note_edit::delete(notes, sel)),
            "duplicate" => self.edit(c, |notes, sel, _, _| note_edit::duplicate(notes, sel, step)),
            "quantize" => {
                let mut q = self.quantize;
                if self.quantize_grid.is_none() {
                    q.step = step;
                }
                self.edit(c, |notes, sel, _, _| {
                    let mut s = Editor::sel_or_all(sel);
                    note_edit::quantize(notes, &mut s, &q);
                    *sel = if sel.iter().any(|&x| x) {
                        s
                    } else {
                        vec![false; notes.len()]
                    };
                });
            }
            "quantize-settings" => self.quantize_open = true,
            "deactivate" => self.edit(c, |notes, sel, _, _| note_edit::toggle_muted(notes, sel)),
            "chop" => self.edit(c, |notes, sel, _, _| note_edit::chop(notes, sel, step)),
            "join" => self.edit(c, |notes, sel, _, _| note_edit::join(notes, sel)),
            "legato" | "reverse" | "double" | "half" | "transpose-down" | "transpose-up" => {
                let fold = self.fold();
                self.edit(c, |notes, sel, ls, beats| {
                    let any = sel.iter().any(|&x| x);
                    let mut s = Editor::sel_or_all(sel);
                    match id {
                        "legato" => note_edit::legato(notes, &s, *ls + *beats),
                        "reverse" => note_edit::reverse(notes, &mut s),
                        "double" => note_edit::scale_time(notes, &mut s, 2.0),
                        "half" => note_edit::scale_time(notes, &mut s, 0.5),
                        _ => {
                            let d = if id == "transpose-up" { 1 } else { -1 };
                            if fold == Fold::Scale {
                                note_edit::transpose_in_scale(notes, &s, d, &key);
                            } else {
                                note_edit::transpose(notes, &mut s, d);
                            }
                        }
                    }
                    *sel = if any { s } else { vec![false; notes.len()] };
                });
            }
            "loop-selection" => {
                let notes = self.notes(c);
                if let Some((lo, hi)) = note_edit::span(&notes, &self.sel) {
                    let start = note_edit::snap_down(lo, step).max(0.0);
                    let len = ((hi - start) / step - 1e-9).ceil().max(1.0) * step;
                    self.edit(c, |_, _, ls, beats| {
                        *ls = start;
                        *beats = len;
                    });
                }
            }
            "zoom-selection" => self.zoom_to_selection(c),
            "zoom-clip" => self.fit = true,
            "fit-width" => self.fit_width(c),
            "fit-height" => self.fit_height(c, false),
            "snap" => self.grid.snap = !self.grid.snap,
            "narrow" => self.grid.narrow(),
            "widen" => self.grid.widen(),
            "triplet" => self.grid.triplet = !self.grid.triplet,
            "draw" => self.draw = !self.draw,
            "highlight" => self.highlight = !self.highlight,
            "fold-scale" => {
                let f = if self.fold() == Fold::Scale {
                    Fold::Off
                } else {
                    Fold::Scale
                };
                self.set_fold(f);
            }
            "hear" => self.hear = !self.hear,
            _ => {}
        }
    }

    /// Fits the view across the selection (or every note), and its rows.
    fn zoom_to_selection(&mut self, c: &Controls) {
        let notes = self.notes(c);
        let sel = Editor::sel_or_all(&self.sel);
        let Some((lo, hi)) = note_edit::span(&notes, &sel) else {
            return;
        };
        let v = &mut self.view;
        if v.w > 0.0 {
            v.ppb = (v.w / ((hi - lo) * 1.1).max(0.25)).clamp(super::MIN_PPB, super::MAX_PPB);
            v.first = (lo - (hi - lo) * 0.05).max(0.0);
        }
        self.fit_height(c, true);
    }

    /// Fits the view across the notes (not the loop).
    fn fit_width(&mut self, c: &Controls) {
        let notes = self.notes(c);
        let all = vec![true; notes.len()];
        let Some((lo, hi)) = note_edit::span(&notes, &all) else {
            self.fit = true;
            return;
        };
        let v = &mut self.view;
        if v.w > 0.0 {
            v.ppb = (v.w / ((hi - lo) * 1.05).max(0.25)).clamp(super::MIN_PPB, super::MAX_PPB);
            v.first = lo.max(0.0);
        }
    }

    /// Row height and scroll so the used rows (or the selection's) fill
    /// the panel's height.
    fn fit_height(&mut self, c: &Controls, selection: bool) {
        let notes = self.notes(c);
        let pick = if selection {
            Editor::sel_or_all(&self.sel)
        } else {
            vec![true; notes.len()]
        };
        let rows: Vec<usize> = notes
            .iter()
            .zip(&pick)
            .filter(|(_, &s)| s)
            .filter_map(|(n, _)| self.row_of(n.pitch))
            .collect();
        let (Some(&top), Some(&bottom)) = (rows.iter().min(), rows.iter().max()) else {
            return;
        };
        let count = (bottom - top + 3) as f64;
        let v = &mut self.view;
        if v.h > 0.0 {
            v.row_h = (v.h / count).clamp(6.0, 40.0);
            v.top = (top as f64 - 1.0).max(0.0) * v.row_h;
        }
    }

    /// The right-click menu as it applies now.
    pub fn menu(&mut self, c: &Controls) -> Vec<MenuItem> {
        let notes = self.notes(c);
        let any = self.sel.iter().any(|&s| s);
        let k = |s: &str| format!("{}{s}", cmd());
        let item = |id, label, keys: String, enabled: bool| MenuItem {
            id,
            label,
            keys,
            ticked: false,
            enabled,
            line: false,
        };
        let line = |mut m: MenuItem| {
            m.line = true;
            m
        };
        let tick = |mut m: MenuItem, on: bool| {
            m.ticked = on;
            m
        };
        vec![
            item("cut", "Cut", k("X"), any),
            item("copy", "Copy", k("C"), any),
            item("paste", "Paste", k("V"), !self.clipboard.is_empty()),
            item("delete", "Delete", "⌫".into(), any),
            item("duplicate", "Duplicate", k("D"), any),
            item("select-all", "Select All", k("A"), !notes.is_empty()),
            item(
                "invert",
                "Invert Selection",
                format!("⇧{}", k("A")),
                !notes.is_empty(),
            ),
            line(item("quantize", "Quantize", k("U"), !notes.is_empty())),
            item(
                "quantize-settings",
                "Quantize Settings…",
                format!("⇧{}", k("U")),
                true,
            ),
            item("deactivate", "Deactivate Note", "0".into(), any),
            line(item("chop", "Chop Notes on Grid", k("E"), any)),
            item(
                "join",
                "Join Notes",
                k("J"),
                note_edit::can_join(&notes, &self.sel),
            ),
            line(item("loop-selection", "Loop Selection", k("L"), any)),
            line(item(
                "zoom-selection",
                "Zoom to Selection",
                "Z".into(),
                !notes.is_empty(),
            )),
            item("zoom-clip", "Zoom Out Clip", "X".into(), true),
            item("fit-width", "Fit to Width", "W".into(), !notes.is_empty()),
            item("fit-height", "Fit to Height", "H".into(), !notes.is_empty()),
            line(tick(
                item("snap", "Snap to Grid", k("4"), true),
                self.grid.snap,
            )),
            item("narrow", "Narrow Grid", k("1"), true),
            item("widen", "Widen Grid", k("2"), true),
            tick(
                item("triplet", "Triplet Grid", k("3"), true),
                self.grid.triplet,
            ),
            line(tick(item("draw", "Draw Mode", "B".into(), true), self.draw)),
            tick(
                item("highlight", "Highlight Scale", "K".into(), !self.drums),
                self.highlight,
            ),
            tick(
                item("fold-scale", "Fold to Scale", String::new(), !self.drums),
                self.fold() == Fold::Scale,
            ),
            tick(item("hear", "Hear Notes", String::new(), true), self.hear),
        ]
    }

    /// A right-click: the note under it joins the selection first.
    pub fn context(&mut self, c: &Controls, x: f64, y: f64) {
        let notes = self.notes(c);
        match self.note_at(&notes, x, y) {
            Some((i, _)) if !self.sel[i] => {
                self.sel.iter_mut().for_each(|s| *s = false);
                self.sel[i] = true;
            }
            Some(_) => {}
            None => {
                let beat = self.beat_at(x);
                let step = self.grid.step(self.view.ppb);
                self.insert = note_edit::snap_down(beat, step).max(0.0);
            }
        }
    }

    /// Quantize Settings: 0 grid (index into the choices, 0 = the current
    /// grid), 1 start, 2 end, 3 amount ±, 4 swing ±, 5 Quantize, 6 close.
    pub fn quantize_settings(&mut self, c: &Controls, what: i32, value: i32) {
        let q = &mut self.quantize;
        match what {
            0 => {
                self.quantize_grid = match value {
                    i if i <= 0 => None,
                    i => super::QUANTIZE_GRIDS.get(i as usize - 1).copied(),
                };
                if let Some(g) = self.quantize_grid {
                    q.step = g;
                }
            }
            1 => {
                q.start = !q.start;
                if !q.start && !q.end {
                    q.end = true;
                }
            }
            2 => {
                q.end = !q.end;
                if !q.start && !q.end {
                    q.start = true;
                }
            }
            3 => {
                q.amount =
                    ((q.amount * 10.0).round() + value.signum() as f64).clamp(1.0, 10.0) / 10.0
            }
            4 => {
                q.swing = ((q.swing * 10.0).round() + value.signum() as f64).clamp(0.0, 10.0) / 10.0
            }
            5 => {
                self.quantize_open = false;
                self.action(c, "quantize");
            }
            _ => self.quantize_open = false,
        }
    }
}

/// Moves the selected notes `rows` rows up the layout (top first), the
/// whole selection stopping at the edge together. Returns the rows moved.
pub(super) fn shift_rows(notes: &mut [Note], sel: &[bool], layout: &[u8], rows: i64) -> i64 {
    if rows == 0 || layout.is_empty() {
        return 0;
    }
    let idx: Vec<Option<usize>> = notes
        .iter()
        .map(|n| layout.iter().position(|&p| p == n.pitch))
        .collect();
    let picked: Vec<usize> = idx
        .iter()
        .zip(sel)
        .filter(|(_, &s)| s)
        .filter_map(|(i, _)| *i)
        .collect();
    let (Some(&lo), Some(&hi)) = (picked.iter().min(), picked.iter().max()) else {
        return 0;
    };
    // Up is towards row 0.
    let d = rows.clamp(hi as i64 - (layout.len() as i64 - 1), lo as i64);
    for ((n, &s), i) in notes.iter_mut().zip(sel).zip(&idx) {
        if let (true, Some(i)) = (s, i) {
            n.pitch = layout[(*i as i64 - d) as usize];
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Command, Studio};

    /// The demo set's studio, with the Lead track's first (empty) slot
    /// open in a 800 x 400 piano roll at 100 px a beat.
    fn open() -> (Controls, rtrb::Consumer<Command>, Editor) {
        let set = crate::clip::demo_set();
        let (tx, rx) = rtrb::RingBuffer::new(4096);
        let monitor = Studio::new(&set, 48_000.0, 120.0).1.monitor;
        let c = Controls::new(&set, &[0xff0000; 4], tx, monitor);
        let t = (0..c.tracks.len())
            .find(|&t| !c.tracks[t].drums() && !c.tracks[t].audio() && !c.has_clip(t, 0))
            .expect("an empty melodic slot");
        c.edit_notes(t, 0, |_| {});
        let mut ed = Editor::default();
        ed.open(t, 0);
        ed.view.w = 800.0;
        ed.view.h = 400.0;
        ed.frame(&c);
        ed.view.ppb = 100.0;
        ed.view.first = 0.0;
        ed.frame(&c);
        (c, rx, ed)
    }

    fn y_of(ed: &Editor, pitch: u8) -> f64 {
        let i = ed.layout.iter().position(|&p| p == pitch).unwrap();
        i as f64 * ed.rh - ed.view.top + ed.rh / 2.0
    }

    fn notes(c: &Controls, ed: &Editor) -> Vec<(f64, f64, u8)> {
        let (t, s) = ed.open.unwrap();
        c.notes(t, s)
            .unwrap()
            .1
            .iter()
            .map(|n| (n.start, n.length, n.pitch))
            .collect()
    }

    #[test]
    fn draw_move_resize_and_undo() {
        let (c, _rx, mut ed) = open();
        let y60 = y_of(&ed, 60);
        // Draw mode: a click adds a note one grid step (1/16 here) long.
        ed.draw = true;
        ed.press(&c, 110.0, y60, false);
        ed.release(&c);
        assert_eq!(notes(&c, &ed), [(1.0, 0.25, 60)]);
        // Select mode: drag it a beat later and a row up.
        ed.draw = false;
        ed.frame(&c);
        ed.press(&c, 110.0, y60, false);
        ed.drag_to(&c, 213.0, y60 - ed.rh, false, false);
        ed.release(&c);
        assert_eq!(notes(&c, &ed), [(2.0, 0.25, 61)]);
        // Its end, a beat longer.
        ed.frame(&c);
        let y61 = y_of(&ed, 61);
        ed.press(&c, 223.0, y61, false);
        ed.drag_to(&c, 323.0, y61, false, false);
        ed.release(&c);
        assert_eq!(notes(&c, &ed), [(2.0, 1.25, 61)]);
        ed.undo(&c, false);
        assert_eq!(notes(&c, &ed), [(2.0, 0.25, 61)]);
        ed.undo(&c, false);
        assert_eq!(notes(&c, &ed), [(1.0, 0.25, 60)]);
        ed.undo(&c, false);
        assert!(notes(&c, &ed).is_empty());
        ed.undo(&c, true);
        assert_eq!(notes(&c, &ed), [(1.0, 0.25, 60)]);
    }

    #[test]
    fn box_select_then_keys_edit_the_selection() {
        let (c, _rx, mut ed) = open();
        ed.edit(&c, |notes, _, _, _| {
            notes.extend([
                Note::new(0.0, 0.5, 60, 0.8),
                Note::new(1.0, 0.5, 62, 0.8),
                Note::new(2.0, 0.5, 64, 0.8),
            ]);
        });
        ed.frame(&c);
        // A box over the first two.
        ed.press(&c, 20.0, y_of(&ed, 59), false);
        ed.drag_to(&c, 160.0, y_of(&ed, 63), false, false);
        assert!(ed.box_rect().is_some());
        ed.release(&c);
        assert_eq!(ed.selected(), 2);
        // Copy, paste after them, then delete the pasted pair.
        assert!(ed.key(&c, "c", false, true, false));
        assert!(ed.key(&c, "v", false, true, false));
        assert_eq!(notes(&c, &ed).len(), 5);
        assert!(notes(&c, &ed).contains(&(1.5, 0.5, 60)));
        assert_eq!(ed.selected(), 2);
        let del = slint::SharedString::from(slint::platform::Key::Backspace);
        assert!(ed.key(&c, del.as_str(), false, false, false));
        assert_eq!(notes(&c, &ed).len(), 3);
        // Up arrow: the whole clip after ⌘A.
        ed.key(&c, "a", false, true, false);
        let up = slint::SharedString::from(slint::platform::Key::UpArrow);
        ed.key(&c, up.as_str(), false, false, false);
        assert_eq!(
            notes(&c, &ed).iter().map(|n| n.2).collect::<Vec<_>>(),
            [61, 63, 65]
        );
        // Keys the piano roll doesn't use go on to the window.
        assert!(!ed.key(&c, "s", false, true, false));
    }

    #[test]
    fn lane_drag_moves_the_selections_velocity() {
        let (c, _rx, mut ed) = open();
        ed.edit(&c, |notes, _, _, _| {
            notes.extend([Note::new(0.0, 0.5, 60, 0.5), Note::new(1.0, 0.5, 62, 0.5)]);
        });
        ed.frame(&c);
        ed.action(&c, "select-all");
        // Drag the second marker up a quarter of the lane: both rise.
        ed.lane_press(&c, 102.0, 30.0, 64.0);
        ed.lane_drag(&c, 102.0, 14.0);
        ed.release(&c);
        let (t, s) = ed.open.unwrap();
        let v: Vec<f32> = c
            .notes(t, s)
            .unwrap()
            .1
            .iter()
            .map(|n| n.velocity)
            .collect();
        assert_eq!(v, [0.75, 0.75]);
        // Chance, one note: drawing sets it outright.
        ed.lane = Lane::Chance;
        ed.draw = true;
        ed.action(&c, "deselect");
        ed.lane_press(&c, 2.0, 48.0, 64.0);
        ed.release(&c);
        let ch: Vec<f32> = c.notes(t, s).unwrap().1.iter().map(|n| n.chance).collect();
        assert_eq!(ch, [0.25, 1.0]);
    }

    #[test]
    fn menu_follows_the_selection() {
        let (c, _rx, mut ed) = open();
        ed.edit(&c, |notes, _, _, _| {
            notes.push(Note::new(0.0, 0.5, 60, 0.8))
        });
        ed.frame(&c);
        let enabled =
            |ed: &mut Editor, id: &str| ed.menu(&c).iter().find(|m| m.id == id).unwrap().enabled;
        assert!(!enabled(&mut ed, "delete"));
        ed.context(&c, 20.0, y_of(&ed, 60));
        assert_eq!(ed.selected(), 1);
        assert!(enabled(&mut ed, "delete"));
        assert!(!enabled(&mut ed, "join"));
        ed.action(&c, "deactivate");
        let (t, s) = ed.open.unwrap();
        assert!(c.notes(t, s).unwrap().1[0].muted);
    }

    #[test]
    fn quantize_settings_pull_starts_part_way() {
        let (c, _rx, mut ed) = open();
        ed.edit(&c, |notes, _, _, _| {
            notes.push(Note::new(0.1, 0.5, 60, 0.8))
        });
        ed.quantize_settings(&c, 0, 3); // 1/16
        for _ in 0..5 {
            ed.quantize_settings(&c, 3, -1); // amount 50%
        }
        ed.quantize_settings(&c, 5, 0);
        let (t, s) = ed.open.unwrap();
        let start = c.notes(t, s).unwrap().1[0].start;
        assert!((start - 0.05).abs() < 1e-9, "{start}");
        assert!(!ed.quantize_open);
    }

    fn n(start: f64, length: f64, pitch: u8) -> Note {
        Note::new(start, length, pitch, 0.8)
    }

    #[test]
    fn moving_rows_follows_the_fold() {
        // Folded rows: 67, 64, 60 (top first).
        let layout = [67, 64, 60];
        let mut notes = vec![n(0.0, 1.0, 60), n(1.0, 1.0, 64)];
        let sel = [true, false];
        assert_eq!(shift_rows(&mut notes, &sel, &layout, 1), 1);
        assert_eq!(notes[0].pitch, 64);
        // The selection stops at the top together.
        let mut notes = vec![n(0.0, 1.0, 60), n(1.0, 1.0, 64)];
        assert_eq!(shift_rows(&mut notes, &[true, true], &layout, 5), 1);
        assert_eq!((notes[0].pitch, notes[1].pitch), (64, 67));
    }
}
