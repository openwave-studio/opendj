# Design

The timeline model, the reader of the control log and the read-only Mixed Set Editor come from add-mixed-set-editor (earlier in v0.8). This change adds editing, the render and the capture it needs.

## Local render (the basics)

An edit touches a stretch of the set: a transition, or a swapped track from the start of the transition into it to the end of the transition out of it. Only that stretch is rendered:

1. **Window**: widen the stretch to the nearest beats where one deck plays alone on each side (at least 2 bars of it), from the timeline's clips.
2. **Replay**: an offline engine with no audio device loads the decks' tracks, sets each deck's place, rate and mixer state from the log at the window's start, and replays the log's events in the window, with the edit applied, into a buffer.
3. **Splice**: the render replaces the recording between the seams, with a 20 ms equal-power crossfade at each. Both sides of a seam are the same single track at the same place, so small timing differences between log and recording are not heard.

The 5 ms log is good enough here because only the edited stretch is rebuilt. A check: splicing an unedited window back in must leave the set sounding the same (residual below -40 dBFS in the window).

### Change a transition

Length in bars (2, 4, 8, 16, 32) and a curve from the Automix styles (Smooth, Bass swap, Filter, Cut). The logged fader, crossfader, EQ and filter moves of both decks inside the transition are replaced by the style's moves over the new length, from where the incoming track came in (it still comes in on the same bar); the outgoing track plays on as long as needed (its file, its logged rate). The rendered window (4 bars either side) is kept as a file beside the recording, so Listen plays it and Save edited mix splices it in. Back to as played drops the edit.

### Swap a track

The replacement plays the same stretch of the set as the track it replaces (from where that one came in to where it stopped), at the speed that matches the set's tempo there (the replaced track's BPM at its logged speed over the replacement's BPM; 0.75 to 1.34 times its own). It starts from its first bar, on the same beat of the bar the replaced track was on when it came in, so beats and bars line up. The logged fader, EQ and filter moves are kept, so its transitions in and out keep their recorded style and length; the deck's tempo fader is set to the matching speed. Loops and jumps of the replaced track are not carried over. A replacement shorter than the stretch, or too far from the tempo, is refused with the reason shown. Transitions mixed again next to it are rendered again with it, and its own stretch is spliced in under them. Picker: the library, compatible tempo and key first (as Automix suggests).

## Whole-set render (later)

The engine already runs from controls. An offline render creates an engine with no audio device, loads the set's tracks, replays the (edited) event list at each event's frame, and pulls buffers as fast as it can into a WAV or FLAC writer. Two checks decide whether a render is trustworthy:

1. **Null test**: render an unedited log and subtract it from the recording. Target: residual below -60 dBFS outside plugin effects. Needs frame-accurate stamps (Capture) and deterministic DSP (no random seeds per run, key lock with fixed block sizes).
2. **Missing sources**: a track file that moved or a plugin that is not installed makes the render stop before it starts and says which.

Parts the DJ did not edit could later be copied from the recording instead of rendered; not needed for a first version.

## Hard moves

A hard move is a stretch where a deck's place in its track does not follow play, rate and jumps: the jog in scratch mode (`scratch2_enable`), a backspin or a brake. The recorder (step 2) logs the deck's place every audio block while one lasts. The timeline shows it as one block on the clip, from the first to the last block that did not follow normal play, padded to the nearest beat on each side.

- **Keep from recording** (default): the render copies that stretch of the recording's audio, with a 10 ms crossfade at each edge, in place of rendering it. The copy holds the whole mix at that moment, so if the DJ edited another deck's lanes or a transition overlapping the block, the render plays the logged positions back through the engine instead (exact for openDJ's own processing) and says so on the block.
- **Bypass**: the deck plays straight through at its rate from where it was when the move began. The rest of the clip follows from there, so later cue jumps and loops keep their places in the track; the Mixed Set Editor shows the clip shifted where the place differs.

The Ableton export treats a kept hard move the same way: a rendered audio clip on the deck's track in place of warped clips.

## Frame-accurate log

Control changes made from the UI or a controller are applied by the audio thread at a buffer boundary. The audio thread writes (frame, control id, value) into a lock-free ring the logger drains, in place of the logger's 5 ms poll. Plugin parameter changes and instrument notes go through the same ring, and so do the dense place lines during a hard move. Mic and Aux files are written beside the recording with the same frame base.
