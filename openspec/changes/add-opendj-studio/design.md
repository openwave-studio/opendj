# openDJ Studio: design notes

## Device chains

A track's devices run left to right, and moving one changes the sound: a
synth into Delay then Reverb smears the repeats; Reverb then Delay repeats
the reverb's tail.

- **Zones, as in Live.** A chain is MIDI effects, then the instrument, then
  audio effects. A device dropped anywhere snaps into its zone: an effect
  never lands before the instrument. A track plays one instrument; dropping
  another replaces it. Audio tracks take no instrument.
- **Places, not ids.** The audio thread keeps each device's place in the
  chain (`PluginSlot::device`). `Insert`, `Remove`, `Move` and `Transfer`
  (to another track) renumber the devices after the change. Devices not
  playing yet (a plug-in still starting) still hold their place, so a
  plug-in that comes up later is attached where its device is by then.
- **One order everywhere.** `Controls` keeps the chains; every edit changes
  the list and sends its command while the list is locked, so the audio
  thread sees edits in the order they were made. `Plugins` keeps each
  helper's place in step, under its own lock taken first.
- **No clicks.** An edit waits for the end of the block: its tracks fade out
  over the last 128 frames, the edit lands between blocks, and they fade
  back in over the next 128. Removed devices go back to the UI thread to be
  dropped (`Trash::Device`), so the audio thread never frees.
- **Copies.** Option-drag copies a device with its settings now. A copied
  plug-in starts a new helper from the sound the set saved it with (its
  own state can't yet be read back from the helper).

## Devices

Where each device lives, and the bar it has to clear.

- **Native, Pure Data or plug-in.** Devices on most tracks, MIDI effects,
  racks, modulators, the sampler and our synths are native Rust. Pure Data
  holds experimental and community devices. Large synths (Surge XT, Vital,
  Dexed) stay plug-ins, out of process.
- **Order.** The kit, then Limiter, Auto Filter and Saturator, then the MIDI
  effects, then Simpler and Drum Rack. Bass, Pad and Lead become presets of
  one analog-style synth.
- **Modulation, as in Bitwig.** A modulator (LFO, envelope, follower) can
  drive any knob of any device. A modulated knob moves its target; the
  knob's glide (`kit::Smooth`) does the smoothing, so modulation never
  clicks.
- **FunDSP as a toolbox.** Devices use FunDSP's parts inside our own device
  code (Auto Filter's state-variable and ladder filters), not its graph as
  the engine. Dynamics, EQ and the sampler stay hand-written.
- **The bar.** `quality.rs` runs every device in the catalog through the
  same checks: no allocation while it plays or its knobs turn, no runaway
  at its extremes, a tail that dies to true zero with denormals flushed, no
  change at neutral settings, and no click when a gliding knob jumps. The
  bench (`device_bench`) holds each device to a cost per frame.
- **Oversampling.** Devices that bend the sound run their curve at 4x
  through two linear-phase half-band stages (65 taps each, polyphase):
  more than 70 dB less aliasing than without for the rounded curves, at
  1 ms of latency. Lookahead and oversampling report their latency; delay
  compensation across tracks is still to come.
- **Instruments in the chain.** A built-in instrument (Simpler first) takes
  the instrument's place, as a plug-in instrument does: the track's notes
  reach it with their frame offsets and the built-in sound stays quiet.
  Simpler reads its sample with 4-point Hermite interpolation; a stolen
  voice fades out over 3 ms in a spare voice, and a trimmed end fades over
  2 ms, so neither clicks. Instruments pass their own checks (no
  allocation while chords play and steal, finite at the extremes, silent
  after all-notes-off, no clicks on steal or end) and `instrument_bench`
  (Simpler: about 160 ns per frame with eight notes and the filter on).
- **Samples.** The browser's Samples list holds the audio files in the
  set's folder, `~/Music/openDJ/Samples` and a Live User Library's
  Samples. Dropping one on a MIDI track
  plays it on a Simpler named after it; a track that has a Simpler keeps
  its settings and swaps the sample.

## Racks

A rack is a device in the chain that holds several chains side by side
(`rack.rs`).

- **Built so far.** Audio Effect, Instrument and Drum Racks run built-in
  devices in up to 128 chains, each with volume, pan, on, solo and (for
  instruments) key and velocity zones; a note-off follows its note-on to
  the chains that took it. Eight macros turn settings inside, each between
  a low and a high value; a setting inside has an index of its own
  (`rack::inner`), so `DeviceParam` reaches it. A Drum Rack's pad takes
  one note and plays it as its own note (C3), and a pad in a choke group
  stops the others with all-sound-off, which Simpler fades over 3 ms.
  Chains render in 512-frame chunks into their own buffers.
- **In the window.** The chains (on, solo) or a Drum Rack's 16 pads (they
  play on click and take a sample dropped on them) sit beside the macros.
  A click on a chain (or a pad) opens it:
  its devices show beside the rack in a frame, with a place at the end to
  drop more. Devices dragged in from the browser, from the track's chain or
  from another chain land where they're let go; a device dropped on the
  chain list makes a new chain; one dragged out goes onto the track. An
  instrument takes its chain's first place; an Audio Effect Rack takes no
  instrument. Map (on the rack's title) then a macro, then knobs inside:
  each click maps that setting over its whole range or unmaps it; a
  setting follows one macro, and its knob shows the macro's number. Each
  edit swaps the rack for the edited one, so its sound restarts.
- **Not yet.** Plug-ins and MIDI effects inside chains, racks inside
  racks, Live's macro mappings, macros on Push, a range per mapping.
- **Kinds.** An *Audio Effect Rack* sits with the audio effects: its input
  goes to every branch and the branches are summed. An *Instrument Rack*
  sits in the instrument place: each branch has its own MIDI effects,
  instrument and audio effects, with a key range and a velocity range for
  splits and layers. A *MIDI Effect Rack* comes later. A Live set's Drum
  Rack plays on the rack engine (one Simpler per pad, sharing the kit's
  decoded samples); `KitPlayer` still plays the built-in kits.
- **Branches.** Each branch has volume, pan, on and solo. Branches render
  into their own preallocated buffers (`MAX_BLOCK` frames each), so a rack
  adds no allocation on the audio thread. Up to 16 branches.
- **Macros.** Eight knobs on the rack, each turning any settings inside it
  between a low and a high value. Push's encoders show the macros when the
  rack is selected.
- **Addressing.** Commands name a device by a path: its track, then
  (rack place, branch) pairs, then its place. Racks nest up to three deep.
  `Insert`, `Remove`, `Move` and `Transfer` take paths, so devices drag into
  and out of branches the way they do along a chain.
- **In the window.** A rack shows its macros, with a branch list on its
  left. Selecting a branch opens its devices in the device bar, after the
  rack. Dropping on the branch list makes a new branch; dropping into the
  branch's devices puts the device there.
- **Live sets.** `AudioEffectGroupDevice`, `InstrumentGroupDevice` and
  `DrumGroupDevice` are read with their branches, zones and macros
  (`MacroControls.N`, `MacroDisplayNames.N`).
- **Order.** Audio Effect Rack with branches and macros first, then
  Instrument Rack with zones, then reading Live's racks, then Drum Rack on
  the rack engine.
