//! Note edits for the piano roll, as plain functions over a clip's notes
//! and a selection, so they can be tested without a window and used from
//! Push as well.
//!
//! A selection is a `Vec<bool>` beside the notes, one flag per note. Every
//! edit keeps the two in step and leaves the notes sorted by start.
//!
//! Beats are quarter notes, as everywhere in the studio; bars are 4/4.

use crate::clip::Note;
use crate::scale::Key;

/// Shortest note an edit leaves: a 1/128 note.
pub const MIN_LENGTH: f64 = 1.0 / 32.0;

// ------------------------------------------------------------------ grid

/// How wide an adaptive grid is: lines at least this far apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Density {
    Widest,
    Wide,
    Medium,
    Narrow,
    Narrowest,
}

impl Density {
    pub const ALL: [Density; 5] = [
        Density::Widest,
        Density::Wide,
        Density::Medium,
        Density::Narrow,
        Density::Narrowest,
    ];

    /// The least space between two lines, in pixels.
    pub fn min_px(self) -> f64 {
        match self {
            Density::Widest => 64.0,
            Density::Wide => 32.0,
            Density::Medium => 16.0,
            Density::Narrow => 8.0,
            Density::Narrowest => 4.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Density::Widest => "Widest",
            Density::Wide => "Wide",
            Density::Medium => "Medium",
            Density::Narrow => "Narrow",
            Density::Narrowest => "Narrowest",
        }
    }
}

/// The fixed grid sizes, widest first, in beats: 8 bars down to 1/32.
pub const FIXED: [f64; 9] = [32.0, 16.0, 8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125];

/// A fixed grid size's name: "2 Bars", "1/16".
pub fn fixed_label(beats: f64) -> String {
    if beats >= 4.0 {
        let bars = (beats / 4.0).round() as u32;
        if bars == 1 {
            "1 Bar".into()
        } else {
            format!("{bars} Bars")
        }
    } else {
        format!("1/{}", (4.0 / beats).round() as u32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Grid {
    /// Follows the zoom.
    Adaptive(Density),
    /// One size, in beats.
    Fixed(f64),
}

/// The piano roll's grid: its size, triplets, and whether edits snap to it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GridSettings {
    pub grid: Grid,
    pub triplet: bool,
    pub snap: bool,
}

impl Default for GridSettings {
    fn default() -> GridSettings {
        GridSettings {
            grid: Grid::Adaptive(Density::Medium),
            triplet: false,
            snap: true,
        }
    }
}

impl GridSettings {
    /// The grid step in beats at a zoom of `px_per_beat`.
    pub fn step(&self, px_per_beat: f64) -> f64 {
        let straight = match self.grid {
            Grid::Fixed(b) => b,
            Grid::Adaptive(d) => {
                let min = d.min_px() / px_per_beat.max(1e-6);
                // The finest size whose lines are at least `min` apart.
                FIXED
                    .iter()
                    .rev()
                    .copied()
                    .find(|&b| b * if self.triplet { 2.0 / 3.0 } else { 1.0 } >= min - 1e-9)
                    .unwrap_or(FIXED[0])
            }
        };
        if self.triplet {
            straight * 2.0 / 3.0
        } else {
            straight
        }
    }

    /// The step edits snap to: the grid's, or none when snap is off.
    pub fn snap_step(&self, px_per_beat: f64) -> Option<f64> {
        self.snap.then(|| self.step(px_per_beat))
    }

    /// The grid button's text: "1/16", "1/16T", "Off".
    pub fn label(&self, px_per_beat: f64) -> String {
        if !self.snap {
            return "Off".into();
        }
        let straight = if self.triplet {
            self.step(px_per_beat) * 1.5
        } else {
            self.step(px_per_beat)
        };
        let mut s = fixed_label(straight);
        if self.triplet {
            s.push('T');
        }
        s
    }

    /// Narrow Grid (⌘1): a finer fixed size, or a denser adaptive grid.
    pub fn narrow(&mut self) {
        self.grid = match self.grid {
            Grid::Fixed(b) => Grid::Fixed(
                FIXED
                    .iter()
                    .copied()
                    .find(|&f| f < b - 1e-9)
                    .unwrap_or(FIXED[FIXED.len() - 1]),
            ),
            Grid::Adaptive(d) => {
                let i = Density::ALL.iter().position(|&x| x == d).unwrap_or(2);
                Grid::Adaptive(Density::ALL[(i + 1).min(Density::ALL.len() - 1)])
            }
        };
    }

    /// Widen Grid (⌘2): a coarser fixed size, or a sparser adaptive grid.
    pub fn widen(&mut self) {
        self.grid = match self.grid {
            Grid::Fixed(b) => Grid::Fixed(
                FIXED
                    .iter()
                    .rev()
                    .copied()
                    .find(|&f| f > b + 1e-9)
                    .unwrap_or(FIXED[0]),
            ),
            Grid::Adaptive(d) => {
                let i = Density::ALL.iter().position(|&x| x == d).unwrap_or(2);
                Grid::Adaptive(Density::ALL[i.saturating_sub(1)])
            }
        };
    }
}

/// The grid line nearest `beat`.
pub fn snap(beat: f64, step: f64) -> f64 {
    (beat / step).round() * step
}

/// The grid line at or before `beat`: where a drawn note starts.
pub fn snap_down(beat: f64, step: f64) -> f64 {
    (beat / step + 1e-9).floor() * step
}

// --------------------------------------------------------------- selection

/// Sorts the notes by start (then pitch), keeping each flag with its note.
pub fn sort(notes: &mut Vec<Note>, sel: &mut Vec<bool>) {
    sel.resize(notes.len(), false);
    let mut both: Vec<(Note, bool)> = notes.drain(..).zip(sel.drain(..)).collect();
    both.sort_by(|a, b| {
        a.0.start
            .total_cmp(&b.0.start)
            .then(a.0.pitch.cmp(&b.0.pitch))
    });
    for (n, s) in both {
        notes.push(n);
        sel.push(s);
    }
}

pub fn invert(sel: &mut [bool]) {
    for s in sel {
        *s = !*s;
    }
}

/// Where the selection starts and ends, in beats.
pub fn span(notes: &[Note], sel: &[bool]) -> Option<(f64, f64)> {
    let mut it = notes.iter().zip(sel).filter(|(_, &s)| s).map(|(n, _)| n);
    let first = it.next()?;
    let (lo, hi) = it.fold((first.start, first.end()), |(lo, hi), n| {
        (lo.min(n.start), hi.max(n.end()))
    });
    Some((lo, hi))
}

pub fn delete(notes: &mut Vec<Note>, sel: &mut Vec<bool>) {
    let mut i = 0;
    notes.retain(|_| {
        let keep = !sel.get(i).copied().unwrap_or(false);
        i += 1;
        keep
    });
    sel.clear();
    sel.resize(notes.len(), false);
}

/// The selected notes, moved so the first starts at 0: what Copy keeps.
pub fn copy(notes: &[Note], sel: &[bool]) -> Vec<Note> {
    let Some((lo, _)) = span(notes, sel) else {
        return Vec::new();
    };
    notes
        .iter()
        .zip(sel)
        .filter(|(_, &s)| s)
        .map(|(n, _)| Note {
            start: n.start - lo,
            ..*n
        })
        .collect()
}

/// Adds `clip` (from [`copy`]) at beat `at`; the pasted notes become the
/// selection.
pub fn paste(notes: &mut Vec<Note>, sel: &mut Vec<bool>, clip: &[Note], at: f64) {
    sel.iter_mut().for_each(|s| *s = false);
    for n in clip {
        notes.push(Note {
            start: (n.start + at).max(0.0),
            ..*n
        });
        sel.push(true);
    }
    sort(notes, sel);
}

/// Duplicate (⌘D): a copy of the selection right after it. The copy's
/// offset is the selection's length rounded up to whole grid steps.
pub fn duplicate(notes: &mut Vec<Note>, sel: &mut Vec<bool>, step: f64) {
    let Some((lo, hi)) = span(notes, sel) else {
        return;
    };
    let len = ((hi - lo) / step - 1e-9).ceil().max(1.0) * step;
    let clip = copy(notes, sel);
    paste(notes, sel, &clip, lo + len);
}

// -------------------------------------------------------------- moving

/// How a drag snaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Snap {
    /// Moves by whole grid steps, keeping each note's offset from the grid.
    Relative(f64),
    /// Puts each note's start (or end) on the nearest grid line.
    Absolute(f64),
    /// Not at all.
    Free,
}

/// Moves the selected notes by `beats` and `semitones`. Notes never go
/// before beat 0 or out of MIDI's range; the whole selection stops at the
/// edge together, so chords keep their shape.
pub fn move_by(
    notes: &mut Vec<Note>,
    sel: &mut Vec<bool>,
    beats: f64,
    semitones: i32,
    snap_mode: Snap,
) {
    let Some((lo, _)) = span(notes, sel) else {
        return;
    };
    let (low, high) = pitch_range(notes, sel);
    let semitones = semitones.clamp(-(low as i32), 127 - high as i32);
    let delta = match snap_mode {
        Snap::Relative(step) => snap(beats, step),
        Snap::Absolute(_) | Snap::Free => beats,
    };
    let delta = delta.max(-lo);
    for (n, _) in notes.iter_mut().zip(sel.iter()).filter(|(_, &s)| s) {
        n.start += delta;
        if let Snap::Absolute(step) = snap_mode {
            n.start = snap(n.start, step).max(0.0);
        }
        n.pitch = (n.pitch as i32 + semitones) as u8;
    }
    sort(notes, sel);
}

fn pitch_range(notes: &[Note], sel: &[bool]) -> (u8, u8) {
    notes
        .iter()
        .zip(sel)
        .filter(|(_, &s)| s)
        .fold((127, 0), |(lo, hi), (n, _)| {
            (lo.min(n.pitch), hi.max(n.pitch))
        })
}

/// Changes the selected notes' lengths by `beats`, moving their ends.
pub fn resize_by(notes: &mut [Note], sel: &[bool], beats: f64, snap_mode: Snap) {
    for (n, _) in notes.iter_mut().zip(sel).filter(|(_, &s)| s) {
        let end = match snap_mode {
            Snap::Relative(step) => n.end() + snap(beats, step),
            Snap::Absolute(step) => snap(n.end() + beats, step),
            Snap::Free => n.end() + beats,
        };
        let min = match snap_mode {
            Snap::Relative(step) | Snap::Absolute(step) => step.min(n.length).max(MIN_LENGTH),
            Snap::Free => MIN_LENGTH,
        };
        n.length = (end - n.start).max(min);
    }
}

/// Transpose by semitones (the whole selection stops at MIDI's edge).
pub fn transpose(notes: &mut Vec<Note>, sel: &mut Vec<bool>, semitones: i32) {
    move_by(notes, sel, 0.0, semitones, Snap::Free);
}

/// The pitch `steps` scale steps from `pitch`; a pitch outside the scale
/// first lands on the scale in the direction of travel.
pub fn scale_step(pitch: u8, steps: i32, key: &Key) -> u8 {
    let dir = steps.signum();
    let mut p = pitch as i32;
    let mut left = steps.abs();
    while left > 0 {
        p += dir;
        if !(0..=127).contains(&p) {
            return pitch;
        }
        if key.in_scale(p) {
            left -= 1;
        }
    }
    p as u8
}

/// Transpose by scale steps, as when the rows are folded to the scale.
pub fn transpose_in_scale(notes: &mut [Note], sel: &[bool], steps: i32, key: &Key) {
    if steps == 0 {
        return;
    }
    let moved: Vec<u8> = notes
        .iter()
        .zip(sel.iter())
        .filter(|(_, &s)| s)
        .map(|(n, _)| scale_step(n.pitch, steps, key))
        .collect();
    // A note at MIDI's edge comes back unchanged: then nothing moves.
    if notes
        .iter()
        .zip(sel.iter())
        .filter(|(_, &s)| s)
        .zip(&moved)
        .any(|((n, _), &m)| m == n.pitch)
    {
        return;
    }
    let mut m = moved.into_iter();
    for (n, _) in notes.iter_mut().zip(sel.iter()).filter(|(_, &s)| s) {
        n.pitch = m.next().unwrap_or(n.pitch);
    }
}

// --------------------------------------------------------------- tools

/// Quantize Settings (⇧⌘U).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Quantize {
    /// The grid it pulls to, in beats.
    pub step: f64,
    pub start: bool,
    pub end: bool,
    /// How far towards the grid, 0..1.
    pub amount: f64,
    /// 0..1: every second grid line moves later, up to a triplet feel.
    pub swing: f64,
}

impl Default for Quantize {
    fn default() -> Quantize {
        Quantize {
            step: 0.25,
            start: true,
            end: false,
            amount: 1.0,
            swing: 0.0,
        }
    }
}

impl Quantize {
    /// The grid line nearest `beat`, with swing.
    fn target(&self, beat: f64) -> f64 {
        let i = (beat / self.step).round();
        let mut t = i * self.step;
        if (i as i64).rem_euclid(2) == 1 {
            t += self.swing.clamp(0.0, 1.0) * self.step / 3.0;
        }
        t
    }
}

/// Quantize (⌘U): pulls the selected notes' starts and/or ends towards
/// the grid. A note keeps at least one grid step if its end is quantized
/// onto its start.
pub fn quantize(notes: &mut Vec<Note>, sel: &mut Vec<bool>, q: &Quantize) {
    let amount = q.amount.clamp(0.0, 1.0);
    for (n, _) in notes.iter_mut().zip(sel.iter()).filter(|(_, &s)| s) {
        let end = n.end();
        if q.start {
            n.start = (n.start + (q.target(n.start) - n.start) * amount).max(0.0);
        }
        let new_end = if q.end {
            end + (q.target(end) - end) * amount
        } else if q.start {
            // Starts only: the note keeps its length.
            n.start + n.length
        } else {
            end
        };
        let min = if q.end { q.step * amount } else { MIN_LENGTH };
        n.length = (new_end - n.start).max(min.max(MIN_LENGTH));
    }
    sort(notes, sel);
}

/// Legato: each selected note runs to the start of the next selected note
/// after it; the last ones run to `loop_end`.
pub fn legato(notes: &mut [Note], sel: &[bool], loop_end: f64) {
    let starts: Vec<f64> = notes
        .iter()
        .zip(sel)
        .filter(|(_, &s)| s)
        .map(|(n, _)| n.start)
        .collect();
    for (n, _) in notes.iter_mut().zip(sel).filter(|(_, &s)| s) {
        let next = starts
            .iter()
            .copied()
            .filter(|&s| s > n.start + 1e-9)
            .fold(f64::INFINITY, f64::min);
        let to = if next.is_finite() {
            next
        } else {
            loop_end.max(n.end())
        };
        n.length = (to - n.start).max(MIN_LENGTH);
    }
}

/// Reverse: the selection plays backwards over the same span.
pub fn reverse(notes: &mut Vec<Note>, sel: &mut Vec<bool>) {
    let Some((lo, hi)) = span(notes, sel) else {
        return;
    };
    for (n, _) in notes.iter_mut().zip(sel.iter()).filter(|(_, &s)| s) {
        n.start = lo + hi - n.end();
    }
    sort(notes, sel);
}

/// ×2 and ÷2: stretches the selection's times by `factor` from its start.
pub fn scale_time(notes: &mut Vec<Note>, sel: &mut Vec<bool>, factor: f64) {
    let Some((lo, _)) = span(notes, sel) else {
        return;
    };
    for (n, _) in notes.iter_mut().zip(sel.iter()).filter(|(_, &s)| s) {
        n.start = lo + (n.start - lo) * factor;
        n.length = (n.length * factor).max(MIN_LENGTH);
    }
    sort(notes, sel);
}

/// Chop Notes on Grid (⌘E): cuts each selected note at every grid line it
/// crosses; the pieces stay selected.
pub fn chop(notes: &mut Vec<Note>, sel: &mut Vec<bool>, step: f64) {
    let mut out = Vec::with_capacity(notes.len());
    let mut out_sel = Vec::with_capacity(notes.len());
    for (n, &s) in notes.iter().zip(sel.iter()) {
        if !s {
            out.push(*n);
            out_sel.push(false);
            continue;
        }
        let mut at = n.start;
        let end = n.end();
        while at < end - 1e-9 {
            let next = (snap_down(at, step) + step).min(end);
            out.push(Note {
                start: at,
                length: next - at,
                ..*n
            });
            out_sel.push(true);
            at = next;
        }
    }
    *notes = out;
    *sel = out_sel;
    sort(notes, sel);
}

/// Join Notes (⌘J): selected notes of one pitch become one note from the
/// first's start to the last's end, with the first's velocity and chance.
pub fn join(notes: &mut Vec<Note>, sel: &mut Vec<bool>) {
    let mut out: Vec<Note> = Vec::with_capacity(notes.len());
    let mut out_sel = Vec::with_capacity(notes.len());
    let mut joined: Vec<Note> = Vec::new();
    for (n, &s) in notes.iter().zip(sel.iter()) {
        if !s {
            out.push(*n);
            out_sel.push(false);
        } else if let Some(j) = joined.iter_mut().find(|j| j.pitch == n.pitch) {
            j.length = j.end().max(n.end()) - j.start;
        } else {
            joined.push(*n);
        }
    }
    for j in joined {
        out.push(j);
        out_sel.push(true);
    }
    *notes = out;
    *sel = out_sel;
    sort(notes, sel);
}

/// Can [`join`] change anything: two selected notes share a pitch.
pub fn can_join(notes: &[Note], sel: &[bool]) -> bool {
    let mut seen = [false; 128];
    for (n, _) in notes.iter().zip(sel).filter(|(_, &s)| s) {
        if seen[n.pitch as usize & 127] {
            return true;
        }
        seen[n.pitch as usize & 127] = true;
    }
    false
}

/// Deactivate Note (0): mutes the selection, or unmutes it when every
/// selected note is muted already.
pub fn toggle_muted(notes: &mut [Note], sel: &[bool]) {
    let all = notes
        .iter()
        .zip(sel)
        .filter(|(_, &s)| s)
        .all(|(n, _)| n.muted);
    for (n, _) in notes.iter_mut().zip(sel).filter(|(_, &s)| s) {
        n.muted = !all;
    }
}

/// A new note drawn at `beat`: on the grid line at or before it, one grid
/// step long. The new note is the only one selected.
pub fn draw(
    notes: &mut Vec<Note>,
    sel: &mut Vec<bool>,
    beat: f64,
    pitch: u8,
    velocity: f32,
    step: Option<f64>,
) {
    let (start, length) = match step {
        Some(s) => (snap_down(beat.max(0.0), s), s),
        None => (beat.max(0.0), 0.25),
    };
    sel.iter_mut().for_each(|s| *s = false);
    notes.push(Note::new(start, length, pitch.min(127), velocity));
    sel.push(true);
    sort(notes, sel);
}

/// The velocity lane's line: notes starting between the two points take
/// velocities along the line from (`b0`, `v0`) to (`b1`, `v1`). With a
/// selection, only selected notes change.
pub fn velocity_ramp(notes: &mut [Note], sel: &[bool], (b0, v0): (f64, f32), (b1, v1): (f64, f32)) {
    let (lo, hi) = if b0 <= b1 { (b0, b1) } else { (b1, b0) };
    let any = sel.iter().any(|&s| s);
    for (n, &s) in notes.iter_mut().zip(sel) {
        if (any && !s) || n.start < lo - 1e-9 || n.start > hi + 1e-9 {
            continue;
        }
        let t = if hi - lo < 1e-9 {
            1.0
        } else {
            ((n.start - b0) / (b1 - b0)) as f32
        };
        n.velocity = (v0 + (v1 - v0) * t).clamp(1.0 / 127.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(start: f64, length: f64, pitch: u8) -> Note {
        Note::new(start, length, pitch, 0.8)
    }

    fn all(notes: &[Note]) -> Vec<bool> {
        vec![true; notes.len()]
    }

    fn starts(notes: &[Note]) -> Vec<f64> {
        notes.iter().map(|n| n.start).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn adaptive_grid_follows_the_zoom() {
        let g = GridSettings::default(); // Medium: lines >= 16 px apart
        assert_eq!(g.step(64.0), 0.25); // 16 px per 1/16
        assert_eq!(g.step(32.0), 0.5);
        assert_eq!(g.step(1000.0), 0.125); // never finer than 1/32
        assert_eq!(g.label(64.0), "1/16");
        let mut t = g;
        t.triplet = true;
        assert!(close(t.step(64.0), 0.5 * 2.0 / 3.0)); // 1/16T is 10.7 px: too close
        assert_eq!(t.label(64.0), "1/8T");
        let mut off = g;
        off.snap = false;
        assert_eq!(off.label(64.0), "Off");
        assert_eq!(off.snap_step(64.0), None);
    }

    #[test]
    fn fixed_grid_and_narrow_widen() {
        let mut g = GridSettings {
            grid: Grid::Fixed(0.25),
            triplet: false,
            snap: true,
        };
        assert_eq!(g.step(1.0), 0.25);
        assert_eq!(g.label(1.0), "1/16");
        g.narrow();
        assert_eq!(g.grid, Grid::Fixed(0.125));
        g.narrow();
        assert_eq!(g.grid, Grid::Fixed(0.125));
        g.widen();
        g.widen();
        assert_eq!(g.grid, Grid::Fixed(0.5));
        assert_eq!(fixed_label(4.0), "1 Bar");
        assert_eq!(fixed_label(32.0), "8 Bars");
        let mut a = GridSettings::default();
        a.narrow();
        assert_eq!(a.grid, Grid::Adaptive(Density::Narrow));
        a.widen();
        a.widen();
        assert_eq!(a.grid, Grid::Adaptive(Density::Wide));
    }

    #[test]
    fn drawing_lands_on_the_grid() {
        let (mut notes, mut sel) = (vec![n(0.0, 0.25, 60)], vec![true]);
        draw(&mut notes, &mut sel, 1.1, 64, 0.8, Some(0.25));
        assert_eq!(notes[1].start, 1.0);
        assert_eq!(notes[1].length, 0.25);
        assert_eq!(sel, vec![false, true]);
    }

    #[test]
    fn moving_keeps_the_groove_unless_shift() {
        // 10 ms late at 120 BPM is 0.02 beats.
        let mut notes = vec![n(3.02, 0.25, 60)];
        let mut sel = all(&notes);
        move_by(&mut notes, &mut sel, 0.27, 0, Snap::Relative(0.25));
        assert!(close(notes[0].start, 3.27));
        move_by(&mut notes, &mut sel, 0.26, 2, Snap::Absolute(0.25));
        assert!(close(notes[0].start, 3.5));
        assert_eq!(notes[0].pitch, 62);
        move_by(&mut notes, &mut sel, 0.013, 0, Snap::Free);
        assert!(close(notes[0].start, 3.513));
    }

    #[test]
    fn moves_stop_at_the_edges_together() {
        let mut notes = vec![n(0.5, 0.25, 60), n(1.0, 0.25, 125)];
        let mut sel = all(&notes);
        move_by(&mut notes, &mut sel, -2.0, 5, Snap::Relative(0.25));
        assert_eq!(starts(&notes), vec![0.0, 0.5]);
        assert_eq!((notes[0].pitch, notes[1].pitch), (62, 127));
    }

    #[test]
    fn resizing_snaps_the_end() {
        let mut notes = vec![n(1.0, 0.3, 60)];
        resize_by(&mut notes, &[true], 0.3, Snap::Absolute(0.25));
        assert!(close(notes[0].end(), 1.5));
        resize_by(&mut notes, &[true], -5.0, Snap::Relative(0.25));
        assert!(close(notes[0].length, 0.25));
    }

    #[test]
    fn half_quantize() {
        // 40 ms late at 120 BPM: 0.08 beats.
        let mut notes = vec![n(1.08, 0.25, 60)];
        let mut sel = all(&notes);
        let q = Quantize {
            amount: 0.5,
            ..Quantize::default()
        };
        quantize(&mut notes, &mut sel, &q);
        assert!(close(notes[0].start, 1.04));
        assert!(close(notes[0].length, 0.25));
    }

    #[test]
    fn quantize_ends_and_swing() {
        let mut notes = vec![n(0.02, 0.3, 60), n(0.26, 0.1, 62)];
        let mut sel = all(&notes);
        let q = Quantize {
            end: true,
            swing: 1.0,
            ..Quantize::default()
        };
        quantize(&mut notes, &mut sel, &q);
        assert!(close(notes[0].start, 0.0));
        // Its end 0.32 goes to line 1 (an off line), swung a third later.
        assert!(close(notes[0].end(), 0.25 + 0.25 / 3.0));
        assert!(close(notes[1].start, 0.25 + 0.25 / 3.0));
        // An end quantized onto the start keeps a step.
        assert!(notes[1].length >= 0.25 - 1e-9);
    }

    #[test]
    fn only_selected_notes_change() {
        let mut notes = vec![n(0.1, 0.25, 60), n(1.1, 0.25, 62)];
        let mut sel = vec![false, true];
        quantize(&mut notes, &mut sel, &Quantize::default());
        assert_eq!(starts(&notes), vec![0.1, 1.0]);
        assert_eq!(sel, vec![false, true]);
    }

    #[test]
    fn legato_runs_to_the_next_note() {
        let mut notes = vec![
            n(0.0, 0.1, 60),
            n(0.0, 0.1, 64),
            n(1.0, 0.1, 62),
            n(2.5, 0.1, 60),
        ];
        let sel = all(&notes);
        legato(&mut notes, &sel, 4.0);
        let lens: Vec<f64> = notes.iter().map(|n| n.length).collect();
        assert_eq!(lens, vec![1.0, 1.0, 1.5, 1.5]);
    }

    #[test]
    fn reverse_and_scale_time() {
        let mut notes = vec![n(0.0, 0.5, 60), n(1.0, 1.0, 62)];
        let mut sel = all(&notes);
        reverse(&mut notes, &mut sel);
        assert_eq!(starts(&notes), vec![0.0, 1.5]);
        assert_eq!(notes[0].pitch, 62);
        scale_time(&mut notes, &mut sel, 2.0);
        assert_eq!(starts(&notes), vec![0.0, 3.0]);
        assert_eq!(notes[0].length, 2.0);
        scale_time(&mut notes, &mut sel, 0.5);
        assert_eq!(starts(&notes), vec![0.0, 1.5]);
    }

    #[test]
    fn chop_then_join() {
        let mut notes = vec![n(0.1, 0.9, 60), n(2.0, 0.25, 64)];
        let mut sel = vec![true, false];
        chop(&mut notes, &mut sel, 0.25);
        assert_eq!(notes.len(), 5);
        let pieces: Vec<(f64, f64)> = notes[..4].iter().map(|n| (n.start, n.end())).collect();
        assert!(close(pieces[0].0, 0.1) && close(pieces[0].1, 0.25));
        assert!(close(pieces[3].0, 0.75) && close(pieces[3].1, 1.0));
        assert_eq!(sel, vec![true, true, true, true, false]);
        assert!(can_join(&notes, &sel));
        join(&mut notes, &mut sel);
        assert_eq!(notes.len(), 2);
        assert!(close(notes[0].start, 0.1) && close(notes[0].end(), 1.0));
        assert!(!can_join(&notes, &sel));
    }

    #[test]
    fn duplicate_copy_paste_delete() {
        let mut notes = vec![n(0.0, 0.25, 60), n(0.5, 0.3, 62)];
        let mut sel = all(&notes);
        duplicate(&mut notes, &mut sel, 0.25);
        // The selection spans 0.8 beats: the copy goes 1 beat later.
        assert_eq!(starts(&notes), vec![0.0, 0.5, 1.0, 1.5]);
        assert_eq!(sel, vec![false, false, true, true]);
        let clip = copy(&notes, &sel);
        assert_eq!(starts(&clip), vec![0.0, 0.5]);
        paste(&mut notes, &mut sel, &clip, 3.0);
        assert_eq!(starts(&notes), vec![0.0, 0.5, 1.0, 1.5, 3.0, 3.5]);
        delete(&mut notes, &mut sel);
        assert_eq!(starts(&notes), vec![0.0, 0.5, 1.0, 1.5]);
        assert_eq!(sel, vec![false; 4]);
        invert(&mut sel);
        assert_eq!(sel, vec![true; 4]);
    }

    #[test]
    fn transpose_by_scale_steps() {
        // C minor: C D Eb F G Ab Bb.
        let key = Key {
            scale: 1,
            root: 0,
            ..Key::default()
        };
        assert!(key.in_scale(63) && !key.in_scale(64));
        assert_eq!(scale_step(60, 1, &key), 62);
        assert_eq!(scale_step(62, 1, &key), 63);
        assert_eq!(scale_step(60, -1, &key), 58);
        assert_eq!(scale_step(64, 1, &key), 65); // E is out: lands on F
        let mut notes = vec![n(0.0, 1.0, 60), n(0.0, 1.0, 63), n(0.0, 1.0, 67)];
        let mut sel = all(&notes);
        transpose_in_scale(&mut notes, &sel, 2, &key);
        let p: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
        assert_eq!(p, vec![63, 67, 70]);
        transpose(&mut notes, &mut sel, 12);
        assert_eq!(notes[0].pitch, 75);
    }

    #[test]
    fn deactivating_toggles() {
        let mut notes = vec![n(0.0, 1.0, 60), n(1.0, 1.0, 62)];
        toggle_muted(&mut notes, &[true, false]);
        assert!(notes[0].muted && !notes[1].muted);
        toggle_muted(&mut notes, &[true, true]);
        assert!(notes[0].muted && notes[1].muted);
        toggle_muted(&mut notes, &[true, true]);
        assert!(!notes[0].muted && !notes[1].muted);
    }

    #[test]
    fn a_velocity_ramp() {
        let mut notes = vec![
            n(0.0, 0.25, 60),
            n(2.0, 0.25, 60),
            n(4.0, 0.25, 60),
            n(6.0, 0.25, 60),
        ];
        let sel = vec![false; 4];
        velocity_ramp(&mut notes, &sel, (0.0, 0.2), (4.0, 1.0));
        let v: Vec<f32> = notes.iter().map(|n| n.velocity).collect();
        assert!(
            (v[0] - 0.2).abs() < 1e-6 && (v[1] - 0.6).abs() < 1e-6 && (v[2] - 1.0).abs() < 1e-6
        );
        assert_eq!(v[3], 0.8);
    }
}
