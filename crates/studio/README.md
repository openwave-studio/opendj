# openDJ Studio (test app)

A clip-launching DAW on openDJ's engine, as a separate app. This first test
app is a Session grid: four tracks by eight scenes of MIDI clips playing
built-in sounds (a Groove Box kit and a small synth), launched on the next
bar of the studio's own clock or of an Ableton Link session shared with
openDJ. Plan and later steps: `openspec/changes/add-opendj-studio`.

```sh
cargo run --release -p opendj-studio                    # the window
cargo run --release -p opendj-studio -- --check         # launch timing, CPU, allocations
cargo run --release -p opendj-studio -- --render a.wav  # 32 bars through the scenes
cargo run --release -p opendj-studio -- --link-check    # joins a Link session, checks a launch lands on its bar
cargo build --release -p opendj-plugin-host              # the plug-in helper, once (for plug-ins)
cargo run --release -p opendj-studio -- path/to/set.als # opens an Ableton Live set
cargo run --release -p opendj-studio -- --als-info set.als # prints what a set holds
```

Click a clip to launch it on the next bar (the **Launch** pill switches
between 1 Bar, 1/4 and None); **▶ N** launches a whole scene and stops
tracks with an empty slot in it; **■** stops a track on the next bar. Turn
on **LINK** with openDJ's Link on, and launches land on the bars of the mix.

## Ableton Live sets

Give a Live set's `.als` on the command line to open it. Audio and MIDI
tracks come in with their names, colours, volume and pan, plus the first
eight scenes of Session clips. Audio clips play their recordings (found
next to the set, as Live saves them) following their warp markers at the
studio's tempo, keeping their key. Audio Unit and VST3 plug-ins run (each in
an `opendj-plugin-host` helper process, as in openDJ) with the sound the set
saved; Wavetable becomes Vital. Click a device's name to open its window, its
square to switch it on or off. Live's own devices are listed until they have
built-in matches; MIDI tracks without a plug-in instrument play a built-in
stand-in (a drum kit for Drum Racks). The Arrangement, group
tracks, live inputs and effects aren't brought in yet; the status line says
so. Push shows eight tracks at a time: Left and Right move the selection,
Shift with them jumps by eight.

## How it's built

The engine is its own crate, `crates/studio-engine` (`opendj-studio-engine`),
with no sound card, MIDI port or UI, so openDJ can play a set as its Session
deck. `Studio::process_following` renders on a leader's beat and tempo
(openDJ's master) instead of the studio's own clock or Link. This crate adds
the sound card (`audio.rs`), MIDI ports (`midi.rs`), Push (`push/`), the
settings and the window. The modules below are in the engine crate unless
they're under `src/push/` or named as the app's.

- `clip.rs`: clips and the demo set (the model, no sample rate).
- `engine.rs`: the audio thread's side. The model is compiled into an
  immutable `Program` that arrives over a wait-free queue (the old one goes
  back to be dropped off the audio thread); each track's playing state
  (playing clip, pending launch, sounding notes) lives outside it and
  carries over. Launches are split into the block on their exact frame.
  The clock's beat is worked out from the frame count since the last tempo
  change, or read from Link each block, so it never drifts.
- `link.rs`: Ableton Link, quantum 4 as in openDJ.
- `sound.rs`: the built-in drum kit and synth; fixed voice arrays.
- `audio.rs`: the sound card through cpal, or a timer when there is none.
- `als.rs`: reading Live sets (gzipped XML) into tracks and clips.
- `plugins.rs`: finding and starting a set's plug-ins; the audio thread
  runs them as slots in each track's chain (`engine.rs`).
- `devices.rs`: the studio's own devices matching Live's (Utility, EQ
  Eight, Compressor), run in the chain with the plug-ins.
- Each built-in device lists its settings (`Settings::params`) for its
  panel in the device row; a change reaches the running device with
  `Command::DeviceParam` and retunes it in place.
- `devices.rs` also has Live's External Instrument: the track's notes go
  to its MIDI To port its hardware latency early (the engine reads its
  clips that far ahead), and its sound comes in from its Audio From input.
- A plug-in instrument on a track with a MIDI To port takes the clips'
  notes, and what it sends (a CLAP or VST3 sequencer's notes, through the
  helper's shared memory) goes out that port.
- `fx.rs`: Live's Delay, Echo, Reverb, Gate and Overdrive, and
  Hybrid Reverb (the set's room from Live's app through `conv.rs`, blended with a plate) and Roar (shapers and
  filters in series, no feedback or modulation).
- `settings.rs`: the studio's settings (`studio.json`), starting from
  openDJ's audio choices.
- `midi.rs`: MIDI From and To: every input but Push routed to the tracks
  that listen, and tracks' notes out to external synths.
- `check.rs`: the offline checks, with a counting allocator.

The design follows a review of Suhina, transit, maolan and Ardour
(`notes/daw-architecture-review.md` in the project files).

openDJ never depends on this app crate, only on `opendj-studio-engine`.

## Inputs and outputs

Each track has Live's I/O section above its mixer strip (the I/O button in
the top bar shows or hides it): Audio From and its channel, Monitor
(In, Auto, Off), Audio To (Main, Ext. Out with its pair, Sends Only), and
for MIDI tracks MIDI From with its channel and MIDI To with its channel. A
Live set opens with its routings. Monitor In plays the input always (an
audio track's clips are then quiet); Auto plays it while the track is
armed (a MIDI track also while it is selected and no track is armed).

The sound card is chosen in Settings (the gear in the top bar), on its
Audio page: the output and input devices, the sample rate and the buffer.
Devices change at once, without closing the set; a new sample rate takes
effect when the studio opens again. The choices are kept in `studio.json`
beside openDJ's `settings.json`; until there is one, the studio starts
from openDJ's own audio settings (its main output, first input, rate and
buffer). The input opens only for a set with audio tracks, at the
output's rate. `OPENDJ_NO_INPUT=1` leaves it closed.

## Ableton Push 2

Plug in a Push 2 (with its power supply, for full brightness) and the
studio picks it up within two seconds; Ableton Live doesn't need to run.
If Live is open, set Push's Control Surface to None in Live's preferences,
or Live takes the lights back.

- Pads: the clip grid (Session) or the selected instrument (Note: a
  keyboard in the set's key; Octave buttons shift it).
- Note on a drum track is the drum sequencer: the top four rows are 32
  steps of the selected sound, the bottom-left 4x4 plays and picks sounds,
  the bottom-right 4x4 shows the clip's pages (tap to see one, hold one and
  tap another to set the clip's length). Tap a step to add or remove a
  note; hold a step and tap one to its right for a long note; hold steps
  and turn the first three encoders for velocity, length and chance. Scene
  buttons set the step length (1/4 to 1/32t), Delete + sound pad clears
  that sound, Accent adds at full velocity. Hold Session
  or Note to peek at that view: it goes back when you let go.
- Steps (press Note again, or Layout): the selected slot as a stage
  clip, one pad column per stage. The scene buttons pick what the pads
  set: Pitch (two neighbouring pads for the semitone between), Octave,
  Velocity, Gate (tap pulses; Select + pad sets the stage's length,
  Shift + pad ties it), Chance (the bottom pad skips the stage) and Lock
  amount. Hold stages and turn encoders to lock knobs on them; Delete
  while holding clears their locks. An empty slot becomes a stage clip on
  the first tap; Up/Down or Shift + scene button choose the slot.
- Scene buttons right of the pads launch scenes; Stop stops every clip.
- Buttons over the screen stop a track; buttons under it select one.
- Encoders: the selected instrument's eight knobs (Device), or volume, pan
  and sends across the tracks (Mix; press Mix again or use Page for the
  next control). The master encoder is the main volume, the tempo encoder
  the tempo; hold Shift for fine steps.
- Scale (tap to open and close, hold to peek): the key the keyboard plays
  and stage clips are written in. Over the screen: previous scale, the
  roots C G D A E B, and the pad layout; under it: next scale, the roots
  F Bb Eb Ab Db Gb, and Chromatic (every semitone, notes outside the scale
  dark). The first four encoders turn scale (32 of them), root, layout
  (4th up, 4th right, 3rd up, octave up, sequential) and in key or
  chromatic.
- Play starts and stops the transport; Quantize steps the launch quantise.

`opendj-studio --push-preview push.png` draws what the screen shows.
Code: `src/push/` (`surface.rs` for the controls and lights, with
`surface/view.rs` for what the pads do and `surface/mode.rs` for the
encoders and the buttons by the screen; `display.rs`
and `ui/push.slint` for the screen, drawn by Slint's software renderer),
the engine's `steps.rs` for stage clips and `scale.rs` for scales and layouts; the protocol is Ableton's Push 2 MIDI and Display
Interface Manual.
