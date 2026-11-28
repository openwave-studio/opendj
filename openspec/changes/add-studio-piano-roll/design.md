# Piano roll: design notes

Research: `/mnt/project-files/notes/studio-piano-roll-research.md` (project notes). Mockup boards: Studio window, Lead clip, Drum mode, Folded, Right-click menu.

## Where the code goes

- **`opendj-studio-engine::note_edit`** (new, no UI): every edit as a plain function over `&[Note]` and a selection, returning new notes: quantize (grid, start/end, amount, swing), legato, transpose (semitones, scale steps, octaves), reverse, scale time (×2, ÷2), chop on grid, join, duplicate after the selection, invert selection, paste at a beat, move and resize with the snap rules below. Also the grid: `Grid::{Adaptive(level), Fixed(beats)}` with triplet, `step(px_per_beat)` and `snap(beat, mode)`. Unit-tested, and Push can use the same functions later.
- **`crates/studio/src/piano.rs`** (new): the open editor's state (clip, selection, view: px per beat, first beat, top pitch, row height; grid, fold, ghosts, lane, tool, hear notes), pointer handling and hit testing, the undo stack, and building the Slint models.
- **`crates/studio/ui/piano_roll.slint`** (new): the panel, drawn from models Rust builds. `main.slint` swaps it with the device row.

## Data

- `Note` gains `muted: bool` (Deactivate Note). The engine skips muted notes when it compiles a clip. Live sets keep their disabled notes as muted instead of dropping them (`IsEnabled="false"`).
- `Clip` gains `loop_start: f64` (beats). Today `als.rs` shifts notes by `LoopStart`; it keeps their times and sets `loop_start` instead, so notes before the loop stay editable.
- Both fields are `#[serde(default)]` in `set_file.rs`, so older Studio sets still open.
- The editor sends changes through `Controls::edit_notes`, which already recompiles the clip's program and swaps it on the audio thread with the trash queue, so editing while it plays doesn't click.
- Undo: a snapshot of the clip's notes and loop per edit (a clip is small), 200 deep, per clip, cleared when the set changes.
- The scale is the set's `Key` (`Controls::key`), as Push uses it. A scale per clip is later.

## Drawing

- Visible notes only: the note model holds the notes in view (rebuilt on edit, scroll and zoom), each an `{x, y, w, h, color, selected, dim, label}` rectangle. Thousands of notes stay cheap because only the visible ones are elements.
- Grid lines and row shading are one `Path` each, with commands built in Rust, not one element per line.
- Ghost notes are a second model under the notes: dashed outlines in their track's colour.
- Lanes (velocity, chance) are a third model aligned with the notes.
- Colours come from the Studio `Theme` tokens; new ones (ghost tint, scale edge) are added there.

## Input

- One `TouchArea` over the grid sends pointer events (down, move, up, double-click, modifiers) to Rust; `piano.rs` decides what is under the pointer (a note's body, its right edge within 4 px, empty grid) and runs the drag.
- Draw mode or a double-click on empty grid adds a note one grid step long at the snapped beat; dragging while drawing sets its length.
- Moving: the drag distance snaps to whole grid steps and is added to each note, so off-grid timing survives. Shift snaps each note's start to the grid. Alt (or snap off) moves freely. Up and down move by semitone, or by scale step when folded to the scale.
- Resizing snaps the note's end the same way.
- Box select on empty grid in Select mode; Shift adds to the selection.
- Hear notes: clicking a key or a note, and moving a note to a new pitch, plays it on the track's instrument through the same note path as MIDI input (`Command::Note`), with a note off after 250 ms or on release.
- Keyboard: a `FocusScope` on the panel for Live's shortcuts (⌘D, ⌘U, ⌘E, ⌘J, ⌘L, ⌘1-4, 0, Z, X, W, H, B, K, arrows, Delete, ⌘Z / ⇧⌘Z, ⌘C / ⌘V / ⌘X, ⌘A, ⇧⌘A).

## Grid

- Adaptive picks the finest power-of-two division whose lines are at least N px apart (Widest 64, Wide 32, Medium 16, Narrow 8, Narrowest 4), so it follows the zoom.
- Fixed is 8 Bars, 4 Bars, 2 Bars, 1 Bar, 1/2, 1/4, 1/8, 1/16, 1/32. Triplet divides by 3 instead of 2.
- Snap off shows the grid but doesn't snap; the grid button then reads "Off".

## Drum mode

A track whose instrument is a Drum Rack shows its pads as rows (pad name, note), highest note on top, folded to the pads with notes by default. Clicking a pad name plays it.
