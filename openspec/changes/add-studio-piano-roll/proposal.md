# Proposal

Planned for: **v0.9.5**. Issue #1150, a user request (2026-11-26), after research and mockups.

## Why

openDJ Studio plays MIDI clips from Live sets, Push and the stage sequencer, but a clip's notes can't be seen or changed in the window. A piano roll is the core of writing music in a DAW, and the one thing still missing before the Studio can replace Live for making a track. No open-source piano roll can be dropped in: each one is tied to its own UI toolkit (egui, Qt, JUCE, React). The Studio already has what the editing tools need underneath: scales, the Chord/Arp/Scale MIDI effects, a note model with chance, and a way to swap a clip's notes on the audio thread without a click.

## What Changes

- **Bottom panel.** Double-clicking a MIDI clip opens its notes in the bottom panel, where the devices sit; Tab switches between notes and devices. The panel can be resized and popped out into its own window.
- **Clip column (left).** Start, length and loop; Draw/Select; the scale (highlight, root, scale); the tools (Quantize, Legato, Transpose, Reverse, ×2, ÷2); ghost notes Off, Track or All.
- **Toolbar over the keys.** Headphones (hear notes while editing), Fold (Off, Scale, Used), and the grid button at the top right ("1/16 ▾") opening the snap menu.
- **Editing.** Draw, select (click, box, Shift), move, resize, duplicate, delete, copy and paste, undo and redo. Notes land on the grid; dragging moves notes in grid steps and keeps their offset, Shift-drag lands them on the grid, Alt drags freely.
- **Grid and snap, as in Live.** Snap to Grid (⌘4), Adaptive (Widest to Narrowest, following the zoom), Fixed (8 Bars to 1/32), Narrow and Widen (⌘1, ⌘2), Triplet (⌘3).
- **Scale.** Rows outside the scale are darker, keys in the scale have a lilac edge, the root is brighter; Fold to scale or to the notes used.
- **Ghost notes.** Other clips on the same track, or on every MIDI track, drawn as dashed outlines in their track's colour.
- **Lanes.** Velocity (drag a bar, draw a ramp) and Chance (the note model's chance).
- **Right-click menu.** Live's names and shortcuts: Cut, Copy, Paste, Delete, Select All, Invert Selection, Duplicate, Quantize, Quantize Settings (grid, start/end, amount, swing), Deactivate Note, Chop Notes on Grid, Join Notes, Loop Selection, Set Loop Start/End, the four zoom commands, the grid section, Draw Mode, Highlight Scale.
- **Drum mode.** On a Drum Rack track the rows are the rack's pads by name, folded to the pads in use.
- **Engine.** Notes can be deactivated (kept but silent); a clip's loop can start after its first beat; both are read from Live sets and saved with Studio sets.

## Out of scope (later)

Strum, Arpeggiate, Chord stamp, Humanize and the generators; Bitwig-style per-note operators (Repeats, Occurrence, Recurrence); release velocity and velocity deviation; MPE and automation lanes; editing several clips at once; Time Warp and Fit to Time Range; Crop Clip, start/end markers separate from the loop and Set 1.1.1 Here; Extract Groove; a scale per clip; quantize that keeps the notes' original timing to dial back later; note editing from Push.

## Capabilities

### Modified Capabilities

- `studio`: a piano roll for MIDI clips.
