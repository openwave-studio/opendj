//! The piano roll (OpenSpec change add-studio-piano-roll, step 2): the open
//! clip, the view (zoom and scroll), fold, ghost notes and the grid, and
//! building what `ui/piano_roll.slint` draws. Only what is in view becomes
//! a Slint element; grid lines are three paths.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use opendj_studio_engine::note_edit::{self, Density, Grid, GridSettings, FIXED};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::clip::Note;
use crate::control::Controls;
use crate::engine::SlotState;
use crate::scale::{Key, NOTE_NAMES, SCALES};
use crate::{
    color, AppWindow, Piano, PianoGhost, PianoLegend, PianoMark, PianoNote, PianoRow, PianoWindow,
};

/// The keys' width, and a Drum Rack's pad names'.
pub const KEYS_W: f64 = 38.0;
pub const PADS_W: f64 = 104.0;
/// Row heights: as zoomed, and the most rows stretch to fill the panel.
const ROW_H: f64 = 12.0;
const MAX_FILL_ROW: f64 = 26.0;
/// Pixels per beat, at most zoomed out and in.
const MIN_PPB: f64 = 2.0;
const MAX_PPB: f64 = 640.0;
/// Edits kept for undo.
const UNDO_DEPTH: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Fold {
    /// Every key (a Drum Rack: every pad).
    #[default]
    Off,
    /// The scale's keys only.
    Scale,
    /// Keys with notes (or ghost notes) only.
    Used,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Ghosts {
    Off,
    /// The track's other clips.
    #[default]
    Track,
    /// Also the clips in the same scene on the other MIDI tracks.
    All,
}

/// A key or pad row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub pitch: u8,
    pub label: String,
}

/// A note's name as Live writes it: middle C (60) is C3.
pub fn note_name(pitch: u8) -> String {
    format!(
        "{}{}",
        NOTE_NAMES[pitch as usize % 12],
        pitch as i32 / 12 - 2
    )
}

pub fn is_black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// The rows top to bottom: keys (highest first) or a Drum Rack's pads.
/// `used` holds the pitches with notes or ghost notes.
pub fn rows(fold: Fold, key: &Key, pads: Option<&[(u8, String)]>, used: &BTreeSet<u8>) -> Vec<Row> {
    if let Some(pads) = pads {
        let name = |p: u8| {
            pads.iter()
                .find(|(n, _)| *n == p)
                .map_or_else(|| note_name(p), |(_, s)| s.clone())
        };
        let mut pitches: BTreeSet<u8> = match fold {
            Fold::Used => used.clone(),
            _ => pads
                .iter()
                .map(|(n, _)| *n)
                .chain(used.iter().copied())
                .collect(),
        };
        if pitches.is_empty() {
            pitches = pads.iter().map(|(n, _)| *n).collect();
        }
        return pitches
            .into_iter()
            .rev()
            .map(|p| Row {
                pitch: p,
                label: name(p),
            })
            .collect();
    }
    let keep = |p: u8| match fold {
        Fold::Off => true,
        Fold::Scale => key.in_scale(p as i32),
        Fold::Used => used.contains(&p),
    };
    let mut out: Vec<Row> = (0..=127u8)
        .rev()
        .filter(|&p| keep(p))
        .map(|p| Row {
            pitch: p,
            label: note_name(p),
        })
        .collect();
    if out.is_empty() {
        // Nothing used yet: the scale.
        out = rows(Fold::Scale, key, None, used);
    }
    out
}

/// What is in view: beats across, rows down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The beat at the grid's left edge.
    pub first: f64,
    pub ppb: f64,
    /// Pixels scrolled down from the top row.
    pub top: f64,
    /// The row height as zoomed (rows may stretch to fill).
    pub row_h: f64,
    /// The grid area's size in pixels.
    pub w: f64,
    pub h: f64,
}

impl Default for View {
    fn default() -> View {
        View {
            first: 0.0,
            ppb: 60.0,
            top: 0.0,
            row_h: ROW_H,
            w: 0.0,
            h: 0.0,
        }
    }
}

impl View {
    pub fn x(&self, beat: f64) -> f64 {
        (beat - self.first) * self.ppb
    }

    pub fn last(&self) -> f64 {
        self.first + self.w / self.ppb.max(1e-6)
    }

    /// The row height with `n` rows: they stretch to fill the panel when
    /// few, up to a limit.
    pub fn row_height(&self, n: usize) -> f64 {
        if n == 0 || self.h <= 0.0 {
            return self.row_h;
        }
        let fill = self.h / n as f64;
        if fill > self.row_h {
            fill.min(MAX_FILL_ROW)
        } else {
            self.row_h
        }
    }

    /// Keeps the rows' scroll inside the rows.
    pub fn clamp_top(&mut self, n: usize) {
        let total = n as f64 * self.row_height(n);
        self.top = self.top.clamp(0.0, (total - self.h).max(0.0));
    }

    /// Zooms by `factor` keeping the beat under `at` (pixels) in place.
    pub fn zoom(&mut self, factor: f64, at: f64) {
        let beat = self.first + at / self.ppb;
        self.ppb = (self.ppb * factor).clamp(MIN_PPB, MAX_PPB);
        self.first = (beat - at / self.ppb).max(0.0);
    }
}

/// Grid lines across the view as path commands: bars, beats and the grid's
/// own steps, each only when they are far enough apart to read.
pub fn lines(view: &View, step: f64) -> [String; 3] {
    let mut out = [String::new(), String::new(), String::new()];
    if view.w <= 0.0 || view.h <= 0.0 {
        return out;
    }
    let h = view.h;
    let mut line = |which: usize, beat: f64| {
        let x = view.x(beat).round() + 0.5;
        out[which].push_str(&format!("M {x:.1} 0 L {x:.1} {h:.1} "));
    };
    let on = |beat: f64, every: f64| {
        let r = beat / every;
        (r - r.round()).abs() < 1e-6
    };
    let (lo, hi) = (view.first, view.last());
    if 4.0 * view.ppb >= 6.0 {
        let mut b = (lo / 4.0).floor() * 4.0;
        while b <= hi {
            line(0, b);
            b += 4.0;
        }
    }
    if view.ppb >= 6.0 && step <= 1.0 + 1e-9 {
        let mut b = lo.floor();
        while b <= hi {
            if !on(b, 4.0) {
                line(1, b);
            }
            b += 1.0;
        }
    }
    if !on(step, 1.0) && step * view.ppb >= 3.0 {
        let mut k = (lo / step).floor();
        while k * step <= hi {
            let b = k * step;
            if !on(b, 1.0) {
                line(2, b);
            }
            k += 1.0;
        }
    }
    out
}

/// Bar numbers along the ruler, thinned out when the bars are narrow.
pub fn marks(view: &View) -> Vec<(f64, String)> {
    let bar_px = 4.0 * view.ppb;
    let every = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0]
        .into_iter()
        .find(|e| e * bar_px >= 28.0)
        .unwrap_or(64.0);
    let mut out = Vec::new();
    let mut bar = (view.first / 4.0 / every).floor() * every;
    while bar * 4.0 <= view.last() {
        out.push((view.x(bar * 4.0), format!("{}", bar as i64 + 1)));
        bar += every;
    }
    out
}

/// A position as Live writes it: bar.beat.sixteenth from 1.1.1.
pub fn position_text(beats: f64) -> String {
    let b = beats.max(0.0) + 1e-9;
    let bar = (b / 4.0).floor();
    let beat = (b % 4.0).floor();
    let six = ((b % 1.0) * 4.0).floor();
    format!("{}.{}.{}", bar as i64 + 1, beat as i64 + 1, six as i64 + 1)
}

/// A length as Live writes it: bars.beats.sixteenths.
pub fn length_text(beats: f64) -> String {
    let b = beats.max(0.0) + 1e-9;
    format!(
        "{}.{}.{}",
        (b / 4.0).floor() as i64,
        (b % 4.0).floor() as i64,
        ((b % 1.0) * 4.0).floor() as i64
    )
}

/// Another clip whose notes show as ghosts.
#[derive(Clone, Debug)]
pub struct Source {
    pub label: String,
    pub color: u32,
    pub notes: Vec<Note>,
    pub loop_start: f64,
    pub beats: f64,
}

/// A source's notes laid over this clip: its loop lines up with this
/// clip's loop start and repeats across `span`.
pub fn ghost_notes(src: &Source, loop_start: f64, span: (f64, f64)) -> Vec<Note> {
    if src.beats <= 0.0 {
        return Vec::new();
    }
    let (lo, hi) = (src.loop_start, src.loop_start + src.beats);
    let looped: Vec<Note> = src
        .notes
        .iter()
        .filter(|n| !n.muted && n.start >= lo - 1e-9 && n.start < hi - 1e-9)
        .copied()
        .collect();
    let first = ((span.0 - loop_start) / src.beats).floor() as i64;
    let last = ((span.1 - loop_start) / src.beats).ceil() as i64;
    let mut out = Vec::new();
    for k in first..last.min(first + 64) {
        let shift = loop_start + k as f64 * src.beats - lo;
        out.extend(looped.iter().map(|n| Note {
            start: n.start + shift,
            ..*n
        }));
    }
    out.retain(|n| n.end() > span.0 && n.start < span.1);
    out
}

/// One legend line per track: its clip's name, or how many clips.
pub fn legend(sources: &[Source]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32, usize)> = Vec::new();
    for s in sources {
        let track = s.label.split(" · ").next().unwrap_or_default();
        match out
            .iter_mut()
            .find(|(l, _, _)| l.split(" · ").next() == Some(track))
        {
            Some(line) => {
                line.2 += 1;
                line.0 = format!("{track} · {} clips", line.2);
            }
            None => out.push((s.label.clone(), s.color, 1)),
        }
    }
    out.into_iter().map(|(l, c, _)| (l, c)).collect()
}

/// What the panel draws, built from the clip and the view.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub track_name: String,
    pub clip_name: String,
    pub color: u32,
    pub drums: bool,
    /// Rows in view: y, height, pitch, label.
    pub rows: Vec<(f64, f64, u8, String)>,
    /// Notes in view: x, y, w, h, muted, outside the loop.
    pub notes: Vec<(f64, f64, f64, f64, bool, bool)>,
    /// Ghosts in view: x, y, w, h, colour.
    pub ghosts: Vec<(f64, f64, f64, f64, u32)>,
    pub legend: Vec<(String, u32)>,
    pub marks: Vec<(f64, String)>,
    pub lines: [String; 3],
    pub loop_x: f64,
    pub loop_w: f64,
    pub start_text: String,
    pub length_text: String,
    pub count: usize,
}

/// A clip's notes and loop, as undo keeps them.
type Snapshot = (Vec<Note>, f64, f64);

/// The open piano roll.
pub struct Editor {
    /// The clip: track and scene.
    pub open: Option<(usize, usize)>,
    pub grid: GridSettings,
    pub fold: Fold,
    pub ghosts: Ghosts,
    pub highlight: bool,
    pub hear: bool,
    pub draw: bool,
    pub view: View,
    /// Zoom to the clip once the panel has a size.
    fit: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Default for Editor {
    fn default() -> Editor {
        Editor {
            open: None,
            grid: GridSettings::default(),
            fold: Fold::Off,
            ghosts: Ghosts::Track,
            highlight: true,
            hear: true,
            draw: false,
            view: View::default(),
            fit: true,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

/// How far the clip's material reaches: its loop and its notes.
fn extent(notes: &[Note], loop_start: f64, beats: f64) -> f64 {
    notes
        .iter()
        .map(Note::end)
        .fold(loop_start + beats, f64::max)
        .max(4.0)
}

impl Editor {
    /// Opens a clip, zoomed to fit once the panel has a size.
    pub fn open(&mut self, track: usize, scene: usize) {
        if self.open != Some((track, scene)) {
            self.undo.clear();
            self.redo.clear();
        }
        self.open = Some((track, scene));
        self.fit = true;
    }

    /// The rows for the open clip, and the pitches it and its ghosts use.
    fn rows_for(&self, c: &Controls, notes: &[Note], ghosts: &[(Note, u32)]) -> Vec<Row> {
        let Some((t, _)) = self.open else {
            return Vec::new();
        };
        let used: BTreeSet<u8> = notes
            .iter()
            .map(|n| n.pitch)
            .chain(ghosts.iter().map(|(n, _)| n.pitch))
            .collect();
        let pads = drum_pads(c, t);
        rows(self.fold, &c.key(), pads.as_deref(), &used)
    }

    /// The clips whose notes show as ghosts.
    fn sources(&self, c: &Controls) -> Vec<Source> {
        let Some((t, s)) = self.open else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if self.ghosts == Ghosts::Off {
            return out;
        }
        let mut add = |tr: usize, sc: usize| {
            let Some((name, _)) = c.clip_info(tr, sc) else {
                return;
            };
            let Some((beats, notes)) = c.notes(tr, sc) else {
                return;
            };
            if notes.is_empty() {
                return;
            }
            let info = &c.tracks[tr];
            out.push(Source {
                label: format!("{} {} · {}", tr + 1, info.name, name),
                color: info.color,
                notes,
                loop_start: c.loop_start(tr, sc),
                beats,
            });
        };
        for sc in (0..crate::SCENES).filter(|&sc| sc != s) {
            add(t, sc);
        }
        if self.ghosts == Ghosts::All {
            let drums = c.tracks[t].drums();
            for tr in (0..c.tracks.len()).filter(|&tr| tr != t) {
                let info = &c.tracks[tr];
                if !info.audio() && info.drums() == drums {
                    add(tr, s);
                }
            }
        }
        out
    }

    /// Builds the frame for the open clip; None when it is gone.
    pub fn frame(&mut self, c: &Controls) -> Option<Frame> {
        let (t, s) = self.open?;
        let info = c.tracks.get(t)?;
        if info.audio() {
            return None;
        }
        let (beats, notes) = c.notes(t, s)?;
        let loop_start = c.loop_start(t, s);
        let end = extent(&notes, loop_start, beats);
        let v = &mut self.view;
        if self.fit && v.w > 0.0 {
            v.ppb = (v.w / (end + 0.25)).clamp(MIN_PPB, MAX_PPB);
            v.first = 0.0;
        }
        let span = (v.first, v.last());
        let sources = self.sources(c);
        let ghosts: Vec<(Note, u32)> = sources
            .iter()
            .flat_map(|src| {
                ghost_notes(src, loop_start, span)
                    .into_iter()
                    .map(move |n| (n, src.color))
            })
            .collect();
        let rows = self.rows_for(c, &notes, &ghosts);
        let v = &mut self.view;
        let rh = v.row_height(rows.len());
        if self.fit && v.w > 0.0 && v.h > 0.0 {
            // The notes in the middle, else around middle C.
            let mid = if notes.is_empty() {
                60.0
            } else {
                notes.iter().map(|n| n.pitch as f64).sum::<f64>() / notes.len() as f64
            };
            let i = rows
                .iter()
                .position(|r| (r.pitch as f64) <= mid)
                .unwrap_or(rows.len() / 2);
            v.top = i as f64 * rh - v.h / 2.0;
            self.fit = false;
        }
        v.clamp_top(rows.len());
        let v = self.view;
        let index: HashMap<u8, usize> =
            rows.iter().enumerate().map(|(i, r)| (r.pitch, i)).collect();
        let row_y = |p: u8| index.get(&p).map(|&i| i as f64 * rh - v.top);
        let visible =
            |x: f64, w: f64, y: f64| x + w >= 0.0 && x <= v.w && y + rh >= 0.0 && y <= v.h;
        let mut frame = Frame {
            track_name: format!("{} {}", t + 1, info.name),
            clip_name: c
                .clip_info(t, s)
                .map_or_else(|| "New clip".into(), |(n, _)| n),
            color: info.color,
            drums: info.drums(),
            legend: legend(&sources),
            marks: marks(&v),
            lines: lines(&v, self.grid.step(v.ppb)),
            loop_x: v.x(loop_start),
            loop_w: beats * v.ppb,
            start_text: position_text(loop_start),
            length_text: length_text(beats),
            count: notes.len(),
            ..Frame::default()
        };
        let first_row = (v.top / rh).floor().max(0.0) as usize;
        let last_row = (((v.top + v.h) / rh).ceil() as usize).min(rows.len());
        for (i, r) in rows.iter().enumerate().take(last_row).skip(first_row) {
            frame
                .rows
                .push((i as f64 * rh - v.top, rh, r.pitch, r.label.clone()));
        }
        for n in &notes {
            let Some(y) = row_y(n.pitch) else { continue };
            let (x, w) = (v.x(n.start), n.length * v.ppb);
            if visible(x, w, y) {
                let outside = n.start < loop_start - 1e-9 || n.start >= loop_start + beats - 1e-9;
                frame.notes.push((x, y, w, rh, n.muted, outside));
            }
        }
        for (n, col) in &ghosts {
            let Some(y) = row_y(n.pitch) else { continue };
            let (x, w) = (v.x(n.start), n.length * v.ppb);
            if visible(x, w, y) {
                frame.ghosts.push((x, y, w, rh, *col));
            }
        }
        Some(frame)
    }

    /// The playhead's x in the grid, or below 0 when the clip isn't playing.
    pub fn playhead(&self, c: &Controls) -> f64 {
        let Some((t, s)) = self.open else {
            return -1.0;
        };
        if c.monitor.slot(t, s) != SlotState::Playing {
            return -1.0;
        }
        let progress = c.monitor.progress(t);
        let Some((_, beats)) = c.clip_info(t, s) else {
            return -1.0;
        };
        if progress < 0.0 {
            return -1.0;
        }
        let x = self.view.x(c.loop_start(t, s) + progress as f64 * beats);
        if x < 0.0 || x > self.view.w {
            -1.0
        } else {
            x
        }
    }

    /// Changes the open clip, keeping its notes and loop for undo.
    pub fn edit(&mut self, c: &Controls, f: impl FnOnce(&mut Vec<Note>, &mut f64, &mut f64)) {
        let Some((t, s)) = self.open else { return };
        let mut before = None;
        c.edit_notes(t, s, |clip| {
            before = Some((clip.notes.clone(), clip.loop_start, clip.beats));
            f(&mut clip.notes, &mut clip.loop_start, &mut clip.beats);
        });
        if let Some(b) = before {
            self.undo.push(b);
            if self.undo.len() > UNDO_DEPTH {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
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
    }

    /// A tool from the clip column on every note (selecting comes next).
    pub fn tool(&mut self, c: &Controls, which: i32) {
        let step = self.grid.step(self.view.ppb);
        let key = c.key();
        let fold = self.fold;
        self.edit(c, |notes, ls, beats| {
            let mut sel = vec![true; notes.len()];
            match which {
                0 => note_edit::quantize(
                    notes,
                    &mut sel,
                    &note_edit::Quantize {
                        step,
                        ..Default::default()
                    },
                ),
                1 => note_edit::legato(notes, &sel, *ls + *beats),
                2 | 3 => {
                    let d = if which == 2 { -1 } else { 1 };
                    if fold == Fold::Scale {
                        note_edit::transpose_in_scale(notes, &sel, d, &key);
                    } else {
                        note_edit::transpose(notes, &mut sel, d);
                    }
                }
                4 => note_edit::reverse(notes, &mut sel),
                5 => note_edit::scale_time(notes, &mut sel, 2.0),
                6 => note_edit::scale_time(notes, &mut sel, 0.5),
                _ => {}
            }
        });
    }

    /// The loop start (by a beat) or length (by a bar, or a beat when a bar
    /// or shorter) one step along.
    pub fn nudge(&mut self, c: &Controls, which: i32, dir: i32) {
        let d = dir.signum() as f64;
        self.edit(c, |_, ls, beats| {
            if which == 0 {
                *ls = (*ls + d).max(0.0);
            } else {
                let step = if *beats > 4.0 || (*beats >= 4.0 && d > 0.0) {
                    4.0
                } else {
                    1.0
                };
                *beats = (*beats + d * step).clamp(1.0, 4096.0);
            }
        });
    }

    /// The grid's place in the snap menu: adaptive and fixed index.
    pub fn grid_ticks(&self) -> (i32, i32) {
        match self.grid.grid {
            Grid::Adaptive(d) => (
                Density::ALL
                    .iter()
                    .position(|&x| x == d)
                    .map_or(-1, |i| i as i32),
                -1,
            ),
            Grid::Fixed(b) => (
                -1,
                FIXED
                    .iter()
                    .position(|&f| (f - b).abs() < 1e-9)
                    .map_or(-1, |i| i as i32),
            ),
        }
    }

    /// A pick in the snap menu (see `Piano.grid`).
    pub fn grid_pick(&mut self, what: i32, index: i32) {
        let g = &mut self.grid;
        match what {
            0 => {
                g.grid = Grid::Adaptive(Density::ALL[(index.max(0) as usize).min(4)]);
                g.snap = true;
            }
            1 => {
                g.grid = Grid::Fixed(FIXED[(index.max(0) as usize).min(FIXED.len() - 1)]);
                g.snap = true;
            }
            2 => g.snap = !g.snap,
            3 => g.triplet = !g.triplet,
            4 => g.narrow(),
            5 => g.widen(),
            _ => {}
        }
    }

    /// The wheel over the grid: scrolls, or zooms around the pointer.
    pub fn scroll(&mut self, dx: f64, dy: f64, zoom: bool, at: f64) {
        let v = &mut self.view;
        if zoom {
            let d = if dy.abs() > dx.abs() { dy } else { dx };
            v.zoom((d * 0.006).exp(), at);
        } else {
            v.first = (v.first - dx / v.ppb).max(0.0);
            v.top -= dy;
        }
    }
}

/// A Drum Rack's (or the built-in kit's) pads, lowest note first; None for
/// a track that plays keys.
fn drum_pads(c: &Controls, track: usize) -> Option<Vec<(u8, String)>> {
    let info = c.tracks.get(track)?;
    if !info.drums() {
        return None;
    }
    if !info.pads.is_empty() {
        return Some(info.pads.clone());
    }
    let mut pads: Vec<(u8, String)> = (0..16)
        .filter_map(|i| c.drum_note(track, i).map(|n| (n, c.drum_name(track, i))))
        .collect();
    pads.sort_by_key(|(n, _)| *n);
    pads.dedup_by_key(|(n, _)| *n);
    Some(pads)
}

// ------------------------------------------------------------------ Slint

/// The editor, the studio and where it shows: the main window's bottom
/// panel or its own window.
pub struct Ctx {
    pub editor: RefCell<Editor>,
    pub controls: Arc<Controls>,
    pub main: slint::Weak<AppWindow>,
    pub window: RefCell<Option<PianoWindow>>,
    /// Something changed that the next refresh must redraw.
    pub dirty: std::cell::Cell<bool>,
}

impl Ctx {
    pub fn new(controls: Arc<Controls>, main: &AppWindow) -> Rc<Ctx> {
        let ctx = Rc::new(Ctx {
            editor: RefCell::new(Editor::default()),
            controls,
            main: main.as_weak(),
            window: RefCell::new(None),
            dirty: std::cell::Cell::new(true),
        });
        wire(&main.global::<Piano>(), &ctx);
        let names: Vec<SharedString> = NOTE_NAMES.iter().map(|&n| n.into()).collect();
        let scales: Vec<SharedString> = SCALES.iter().map(|s| s.name.into()).collect();
        let p = main.global::<Piano>();
        p.set_root_names(ModelRc::from(Rc::new(VecModel::from(names))));
        p.set_scale_names(ModelRc::from(Rc::new(VecModel::from(scales))));
        ctx
    }

    fn popped(&self) -> bool {
        self.window
            .borrow()
            .as_ref()
            .is_some_and(|w| w.window().is_visible())
    }

    /// Opens a clip's notes (an empty slot gets a one-bar clip) in the
    /// panel, or the window when it is popped out.
    pub fn open(&self, track: usize, scene: usize) -> bool {
        let c = &self.controls;
        if c.tracks.get(track).is_none_or(|t| t.audio()) || c.notes(track, scene).is_none() {
            return false;
        }
        if !c.has_clip(track, scene) {
            c.edit_notes(track, scene, |_| {});
        }
        c.select(track, Some(scene));
        self.editor.borrow_mut().open(track, scene);
        self.dirty.set(true);
        if !self.popped() {
            if let Some(w) = self.main.upgrade() {
                w.set_notes_shown(true);
            }
        }
        true
    }

    pub fn undo(&self, redo: bool) {
        self.editor.borrow_mut().undo(&self.controls, redo);
        self.dirty.set(true);
    }

    /// Follows the selected slot when it holds a note clip.
    fn follow(&self) {
        let c = &self.controls;
        let Some(s) = c.selected_scene() else { return };
        let t = c.selected_track();
        let open = self.editor.borrow().open;
        if open != Some((t, s))
            && c.has_clip(t, s)
            && c.tracks.get(t).is_some_and(|i| !i.audio())
            && c.notes(t, s).is_some()
        {
            self.editor.borrow_mut().open(t, s);
            self.dirty.set(true);
        }
    }

    /// Called on the window's timer: redraws when anything changed, and
    /// moves the playhead.
    pub fn tick(&self, changed: bool) {
        let Some(main) = self.main.upgrade() else {
            return;
        };
        let popped = self.popped();
        if !popped && !main.get_notes_shown() {
            return;
        }
        if changed {
            self.follow();
            self.dirty.set(true);
        }
        let window = self.window.borrow();
        let p = match (popped, window.as_ref()) {
            (true, Some(w)) => w.global::<Piano>(),
            _ => main.global::<Piano>(),
        };
        let mut ed = self.editor.borrow_mut();
        if self.dirty.replace(false) {
            match ed.frame(&self.controls) {
                Some(f) => show(&p, &ed, &f, &self.controls.key()),
                None => {
                    ed.open = None;
                    main.set_notes_shown(false);
                    return;
                }
            }
        }
        p.set_playhead(ed.playhead(&self.controls) as f32);
    }

    /// Pops the piano roll out into its own window, or docks it back.
    fn pop_out(self: &Rc<Ctx>) {
        let Some(main) = self.main.upgrade() else {
            return;
        };
        if self.popped() {
            if let Some(w) = self.window.borrow().as_ref() {
                let _ = w.hide();
            }
            main.set_notes_shown(true);
        } else {
            if self.window.borrow().is_none() {
                let Ok(w) = PianoWindow::new() else { return };
                wire(&w.global::<Piano>(), self);
                let from = main.global::<Piano>();
                let p = w.global::<Piano>();
                p.set_root_names(from.get_root_names());
                p.set_scale_names(from.get_scale_names());
                p.set_popped(true);
                let ctx = Rc::downgrade(self);
                w.on_undo(move |redo| {
                    if let Some(ctx) = ctx.upgrade() {
                        ctx.editor.borrow_mut().undo(&ctx.controls, redo);
                        ctx.dirty.set(true);
                    }
                });
                let ctx = Rc::downgrade(self);
                w.window().on_close_requested(move || {
                    if let Some(ctx) = ctx.upgrade() {
                        ctx.dirty.set(true);
                        if let Some(m) = ctx.main.upgrade() {
                            m.set_notes_shown(true);
                        }
                    }
                    slint::CloseRequestResponse::HideWindow
                });
                *self.window.borrow_mut() = Some(w);
            }
            if let Some(w) = self.window.borrow().as_ref() {
                let _ = w.show();
            }
            main.set_notes_shown(false);
        }
        self.dirty.set(true);
    }
}

/// Puts a frame on the panel.
fn show(p: &Piano, ed: &Editor, f: &Frame, key: &Key) {
    let px = |v: f64| v as f32;
    p.set_track_name(f.track_name.as_str().into());
    p.set_clip_name(f.clip_name.as_str().into());
    p.set_track_color(color(f.color));
    p.set_drums(f.drums);
    p.set_keys_width(px(if f.drums { PADS_W } else { KEYS_W }));
    p.set_sel_info(
        match f.count {
            1 => "1 note".to_string(),
            n => format!("{n} notes"),
        }
        .into(),
    );
    let rh = f.rows.first().map_or(ROW_H, |r| r.1);
    let rows: Vec<PianoRow> = f
        .rows
        .iter()
        .map(|(y, h, pitch, label)| {
            let p = *pitch as i32;
            // Every C, or every key when the rows are tall enough.
            let shown = f.drums || rh >= 11.0 || p % 12 == 0;
            PianoRow {
                y: px(*y),
                h: px(*h),
                pitch: p,
                label: if shown {
                    label.as_str().into()
                } else {
                    SharedString::new()
                },
                black: is_black(*pitch),
                in_scale: key.in_scale(p),
                root: key.is_root(p),
            }
        })
        .collect();
    p.set_rows(model(rows));
    let col = color(f.color);
    p.set_notes(model(
        f.notes
            .iter()
            .map(|&(x, y, w, h, muted, outside)| PianoNote {
                x: px(x),
                y: px(y),
                w: px(w),
                h: px(h),
                color: col,
                selected: false,
                muted,
                outside,
            })
            .collect(),
    ));
    p.set_ghosts(model(
        f.ghosts
            .iter()
            .map(|&(x, y, w, h, c)| PianoGhost {
                x: px(x),
                y: px(y),
                w: px(w),
                h: px(h),
                color: color(c),
            })
            .collect(),
    ));
    p.set_legend(model(
        f.legend
            .iter()
            .map(|(l, c)| PianoLegend {
                label: l.as_str().into(),
                color: color(*c),
            })
            .collect(),
    ));
    p.set_marks(model(
        f.marks
            .iter()
            .map(|(x, l)| PianoMark {
                x: px(*x),
                label: l.as_str().into(),
            })
            .collect(),
    ));
    p.set_bar_lines(f.lines[0].as_str().into());
    p.set_beat_lines(f.lines[1].as_str().into());
    p.set_sub_lines(f.lines[2].as_str().into());
    p.set_loop_x(px(f.loop_x));
    p.set_loop_w(px(f.loop_w));
    p.set_start_text(f.start_text.as_str().into());
    p.set_length_text(f.length_text.as_str().into());
    p.set_draw(ed.draw);
    p.set_highlight(ed.highlight);
    p.set_hear(ed.hear);
    p.set_fold(match ed.fold {
        Fold::Off => 0,
        Fold::Scale => 1,
        Fold::Used => 2,
    });
    p.set_ghost_mode(match ed.ghosts {
        Ghosts::Off => 0,
        Ghosts::Track => 1,
        Ghosts::All => 2,
    });
    p.set_root_name(NOTE_NAMES[key.root as usize % 12].into());
    p.set_scale_name(key.scale().name.into());
    p.set_grid_label(ed.grid.label(ed.view.ppb).into());
    p.set_snap(ed.grid.snap);
    p.set_triplet(ed.grid.triplet);
    let (a, x) = ed.grid_ticks();
    p.set_adaptive(a);
    p.set_fixed(x);
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

/// Connects a window's piano roll buttons, wheel and size to the editor.
fn wire(p: &Piano, ctx: &Rc<Ctx>) {
    // Each callback changes the editor, then marks the panel for redraw.
    fn on<F: Fn(&Ctx, &mut Editor) + 'static>(ctx: &Rc<Ctx>, f: F) -> impl Fn() + 'static {
        let ctx = Rc::downgrade(ctx);
        move || {
            if let Some(ctx) = ctx.upgrade() {
                f(&ctx, &mut ctx.editor.borrow_mut());
                ctx.dirty.set(true);
            }
        }
    }
    let weak = Rc::downgrade(ctx);
    p.on_resized(move |w, h| {
        if let Some(ctx) = weak.upgrade() {
            let mut ed = ctx.editor.borrow_mut();
            ed.view.w = w as f64;
            ed.view.h = h as f64;
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_scroll(move |dx, dy, zoom, at| {
        if let Some(ctx) = weak.upgrade() {
            ctx.editor
                .borrow_mut()
                .scroll(dx as f64, dy as f64, zoom, at as f64);
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_set_fold(move |i| {
        if let Some(ctx) = weak.upgrade() {
            ctx.editor.borrow_mut().fold = match i {
                1 => Fold::Scale,
                2 => Fold::Used,
                _ => Fold::Off,
            };
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_set_ghosts(move |i| {
        if let Some(ctx) = weak.upgrade() {
            ctx.editor.borrow_mut().ghosts = match i {
                0 => Ghosts::Off,
                2 => Ghosts::All,
                _ => Ghosts::Track,
            };
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_set_draw(move |on| {
        if let Some(ctx) = weak.upgrade() {
            ctx.editor.borrow_mut().draw = on;
            ctx.dirty.set(true);
        }
    });
    p.on_toggle_highlight(on(ctx, |_, ed| ed.highlight = !ed.highlight));
    p.on_toggle_hear(on(ctx, |_, ed| ed.hear = !ed.hear));
    let weak = Rc::downgrade(ctx);
    p.on_pick_root(move |i| {
        if let Some(ctx) = weak.upgrade() {
            ctx.controls.edit_key(|k| k.root = (i.clamp(0, 11)) as u8);
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_pick_scale(move |i| {
        if let Some(ctx) = weak.upgrade() {
            ctx.controls
                .edit_key(|k| k.scale = (i.max(0) as usize).min(SCALES.len() - 1));
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_grid(move |what, i| {
        if let Some(ctx) = weak.upgrade() {
            ctx.editor.borrow_mut().grid_pick(what, i);
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_tool(move |i| {
        if let Some(ctx) = weak.upgrade() {
            ctx.editor.borrow_mut().tool(&ctx.controls, i);
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_nudge(move |which, d| {
        if let Some(ctx) = weak.upgrade() {
            ctx.editor.borrow_mut().nudge(&ctx.controls, which, d);
            ctx.dirty.set(true);
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_close(move || {
        if let Some(ctx) = weak.upgrade() {
            if let Some(w) = ctx.window.borrow().as_ref() {
                let _ = w.hide();
            }
            if let Some(m) = ctx.main.upgrade() {
                m.set_notes_shown(false);
            }
        }
    });
    let weak = Rc::downgrade(ctx);
    p.on_pop_out(move || {
        if let Some(ctx) = weak.upgrade() {
            ctx.pop_out();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(start: f64, length: f64, pitch: u8) -> Note {
        Note::new(start, length, pitch, 0.8)
    }

    #[test]
    fn rows_fold_to_the_scale_or_the_notes() {
        let key = Key::default(); // A minor
        let none = BTreeSet::new();
        let all = rows(Fold::Off, &key, None, &none);
        assert_eq!(all.len(), 128);
        assert_eq!(all[0].pitch, 127);
        assert_eq!(all[127 - 60].label, "C3");
        let scale = rows(Fold::Scale, &key, None, &none);
        assert!(scale.iter().all(|r| key.in_scale(r.pitch as i32)));
        assert!(scale.len() < 80 && scale.len() > 70);
        let used: BTreeSet<u8> = [60, 64, 67].into();
        let folded = rows(Fold::Used, &key, None, &used);
        assert_eq!(
            folded.iter().map(|r| r.pitch).collect::<Vec<_>>(),
            vec![67, 64, 60]
        );
        // Nothing used yet: the scale.
        assert_eq!(rows(Fold::Used, &key, None, &none).len(), scale.len());
    }

    #[test]
    fn drum_rows_are_pads_highest_first() {
        let key = Key::default();
        let pads = vec![
            (36, "Kick".to_string()),
            (38, "Snare".to_string()),
            (42, "Hat".to_string()),
        ];
        let used: BTreeSet<u8> = [36, 42].into();
        let all = rows(Fold::Off, &key, Some(&pads), &used);
        assert_eq!(
            all.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            ["Hat", "Snare", "Kick"]
        );
        let in_use = rows(Fold::Used, &key, Some(&pads), &used);
        assert_eq!(in_use.iter().map(|r| r.pitch).collect::<Vec<_>>(), [42, 36]);
        // A note on no pad still gets a row, named by its key.
        let odd: BTreeSet<u8> = [40].into();
        assert!(rows(Fold::Off, &key, Some(&pads), &odd)
            .iter()
            .any(|r| r.label == "E1"));
    }

    #[test]
    fn grid_lines_follow_the_zoom() {
        let v = View {
            first: 0.0,
            ppb: 40.0,
            top: 0.0,
            row_h: 12.0,
            w: 640.0,
            h: 100.0,
        };
        let [bars, beats, subs] = lines(&v, 0.25);
        // 16 beats in view: bars at 0, 4, 8, 12, 16.
        assert_eq!(bars.matches('M').count(), 5);
        assert_eq!(beats.matches('M').count(), 12);
        assert_eq!(subs.matches('M').count(), 48);
        // Zoomed far out the sixteenths and beats go.
        let far = View { ppb: 2.0, ..v };
        let [bars, beats, subs] = lines(&far, 0.25);
        assert!(!bars.is_empty());
        assert!(beats.is_empty() && subs.is_empty());
    }

    #[test]
    fn ruler_thins_its_numbers() {
        let v = View {
            ppb: 40.0,
            w: 640.0,
            ..View::default()
        };
        let m = marks(&v);
        assert_eq!(
            m.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(),
            ["1", "2", "3", "4", "5"]
        );
        let narrow = View { ppb: 2.0, ..v };
        assert!(marks(&narrow).len() < 25);
        assert_eq!(marks(&narrow)[1].1, "5");
    }

    #[test]
    fn positions_read_as_live_writes_them() {
        assert_eq!(position_text(0.0), "1.1.1");
        assert_eq!(position_text(5.25), "2.2.2");
        assert_eq!(length_text(4.0), "1.0.0");
        assert_eq!(length_text(6.5), "1.2.2");
    }

    #[test]
    fn ghosts_line_up_and_repeat() {
        // A one-bar clip whose loop starts on beat 1.
        let src = Source {
            label: "2 Bass · Hook".into(),
            color: 0xff0000,
            notes: vec![
                n(0.0, 0.5, 40),
                n(1.0, 0.5, 43),
                Note {
                    muted: true,
                    ..n(2.0, 0.5, 45)
                },
            ],
            loop_start: 1.0,
            beats: 4.0,
        };
        let g = ghost_notes(&src, 0.0, (0.0, 8.0));
        // The note before its loop and the muted one stay out; the other
        // repeats each bar from this clip's start.
        assert_eq!(g.iter().map(|n| n.start).collect::<Vec<_>>(), [0.0, 4.0]);
        assert!(g.iter().all(|n| n.pitch == 43));
    }

    #[test]
    fn legend_has_a_line_per_track() {
        let src = |label: &str| Source {
            label: label.into(),
            color: 1,
            notes: Vec::new(),
            loop_start: 0.0,
            beats: 4.0,
        };
        let l = legend(&[
            src("4 Lead · Arp up"),
            src("4 Lead · Arp wide"),
            src("2 Bass · Roots"),
        ]);
        assert_eq!(
            l.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(),
            ["4 Lead · 2 clips", "2 Bass · Roots"]
        );
    }

    #[test]
    fn rows_stretch_when_few_and_scroll_stays_inside() {
        let mut v = View {
            h: 200.0,
            ..View::default()
        };
        assert_eq!(v.row_height(4), MAX_FILL_ROW);
        assert_eq!(v.row_height(10), 20.0);
        assert_eq!(v.row_height(128), ROW_H);
        v.top = 5000.0;
        v.clamp_top(128);
        assert_eq!(v.top, 128.0 * ROW_H - 200.0);
        v.clamp_top(4);
        assert_eq!(v.top, 0.0);
    }

    #[test]
    fn zoom_keeps_the_beat_under_the_pointer() {
        let mut v = View {
            first: 4.0,
            ppb: 40.0,
            w: 800.0,
            ..View::default()
        };
        let at = 200.0;
        let beat = v.first + at / v.ppb;
        v.zoom(2.0, at);
        assert_eq!(v.ppb, 80.0);
        assert!((v.first + at / v.ppb - beat).abs() < 1e-9);
    }

    #[test]
    fn snap_menu_picks_set_the_grid() {
        let mut ed = Editor::default();
        assert_eq!(ed.grid_ticks(), (2, -1));
        ed.grid_pick(1, 7);
        assert_eq!(ed.grid.grid, Grid::Fixed(0.25));
        assert_eq!(ed.grid_ticks(), (-1, 7));
        assert_eq!(ed.grid.label(60.0), "1/16");
        ed.grid_pick(3, 0);
        assert_eq!(ed.grid.label(60.0), "1/16T");
        ed.grid_pick(2, 0);
        assert_eq!(ed.grid.label(60.0), "Off");
        ed.grid_pick(0, 4);
        assert!(ed.grid.snap);
        assert_eq!(ed.grid_ticks(), (4, -1));
    }
}
