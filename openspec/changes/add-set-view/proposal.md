# Proposal

Planned for: **v0.9**

## Why

The Mixed Set Editor (add-mixed-set-editor, which comes first in v0.8) shows a recorded set as a timeline but changes nothing. DJs want to fix a set after playing it: move a transition, smooth a fader, take out an FX stab, and get a new mix file. Mix editors such as DJ.Studio do this for mixes built from playlists, with no live mode; openDJ can do it for sets played live because it logs every control change beside the recording and can play that log back through its own engine.

## What Changes

The basics come first; loops, hard moves and the capture work are advanced steps after them.

- **Change a transition**: pick one, set its length in bars and its curve (Smooth, Bass swap, Filter, Cut, the Automix styles), hear it with a few bars either side, keep it or go back to how it was played.
- **Swap a track**: replace one track of the set with another from the library. It comes in beatmatched to the set's tempo on the same bar, plays for the same stretch, and both of its transitions are made again in their recorded style and length.
- **Local render**: an edit re-plays only its own stretch of the log through openDJ's engine, offline, and splices it into the recording at seams where one deck plays alone, on a beat, with a short crossfade. Everything else is copied from the recording, so a first version needs neither a frame-accurate log nor a bit-exact engine. **Save edited mix** writes a new file beside the recording; the recording is never changed.
- Advanced, later: drag a transition earlier or later with beat and phrase snapping, a transition editor with both tracks on one beatgrid and drawn curves, loops (length, repeat count), hard moves (scratches, backspins, brakes) as one block with **Keep from recording** (default) or **Bypass**, frame-accurate capture (control changes stamped with their audio frame, plugin parameters and instrument notes logged, Mic and Aux to their own files, the deck's place every block during a hard move), a whole-set render, and the Ableton export (add-ableton-set-export step 3) on the same timeline.

Mockups (board 2, and the scratch block on board 1): https://claude.ai/artifact/76fLT6FF4ULjWNcXRSQCzv

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `mix-recording`: frame-accurate log, render an edited set

## Out of Scope

- Building a mix from a playlist without playing it (DJ.Studio's main use); it can reuse the render later.
- Video export, stems placed on the timeline, re-ordering a whole set.
- Swapping in a track that was never analysed (it is analysed first).
- Recreating third-party plugin state that was not logged.

## Impact

editing in the Mixed Set Editor (transition length and curve, track swap), an offline render path that drives the engine from a stretch of the log and splices it into the recording; later the engine recorder (frame stamps, plugin and note lines, input files, place lines during hard moves).
