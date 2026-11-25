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

## Racks (planned)

A rack is a device in the chain that holds several chains side by side.

- **Kinds.** An *Audio Effect Rack* sits with the audio effects: its input
  goes to every branch and the branches are summed. An *Instrument Rack*
  sits in the instrument place: each branch has its own MIDI effects,
  instrument and audio effects, with a key range and a velocity range for
  splits and layers. A *MIDI Effect Rack* comes later. Drum Rack becomes an
  Instrument Rack with one branch per pad note (today's `KitPlayer` plays
  its samples meanwhile).
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
