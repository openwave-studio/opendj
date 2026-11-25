# Proposal

Planned for: **v2.0**. Issue #1102, a user request (2026-11-25).

## Why

openDJ Studio (`add-opendj-studio`) launches clips in an Ableton-style Session, but it is a separate app for production. Playing live, a DJ wants the same clip launching inside openDJ, in time with the decks: loops, MIDI clips on plugin instruments, devices and effects, controllers such as Push and Launchpad. Its engine is a crate of its own (`opendj-studio-engine`, no UI), so openDJ can play it without becoming a DAW. Arrangement, piano roll and audio editing stay in the Studio app.

## What Changes

- **Session channel.** The Session plays on a channel of its own, next to the microphone and the auxiliary inputs: a mixer fader, EQ, effect units, cueing, and it is in recordings. A deck can also play it, as it can play the Groove Box. It is labelled Session.
- **In time with the decks.** The Session follows openDJ's leading tempo and beat (the master deck, Link or a clock coming in), so launches land on the decks' bars. With nothing playing, it keeps its own tempo.
- **Its own audio thread.** The Session renders on a real-time thread of its own, one block ahead of openDJ's output. A heavy set can only make the Session drop out: openDJ's decks never wait for it. Its load is shown.
- **Live sets.** A Live set (.als) opens in the Session: clips, tracks, Drum Racks, built-in devices and plugins, as the Studio app reads them.
- **Session view.** The Session takes the browser area and both side panels; the left rail stays. The Jam tree can sit on the left (Instruments, Presets, Effects) or hide for a wider grid; the rail's Library switches that panel to a compact track list. The selected track's devices sit along the bottom.
- **Session bar.** Leaving the view never stops the music: a slim bar above the footer shows the playing scene, what comes next, each track's state, volume and STOP ALL, and opens the view again.
- **Adding to the Session.** Dragging an instrument, preset or effect onto a track, a slot or the empty space for a new track.
- **Tracks into clips.** Dropping a track from the library onto a slot makes one warped clip from its first downbeat; dragging a deck's waveform onto a slot takes its active loop, or the next 8 bars. A clip never changes the file: it is the file, a start and a loop (8 bars by default), edited with a loop bracket in the bottom panel.
- **Recording.** MIDI from a controller and audio from a deck (after its effects) or the main mix into clips, on the bar, with overdub and capture of the last bars.
- **Instrument deck as one track.** An instrument deck can show a Session track's instrument on its own (a one-track view).

## Out of scope

- An arrangement view, piano roll, or audio editing (the Studio app has those).
- A fifth deck: the Session is a channel, as the microphone is.
- Clips from hot cues or phrases, and stems across tracks (later).

## Capabilities

### Modified Capabilities

- `mic-aux-inputs`: the Session joins the inputs as a channel of its own.
