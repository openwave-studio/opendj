# Tasks

## 1. Test app: a Session grid of MIDI clips

- [x] 1.1 `crates/studio` app: 4 tracks x 8 scenes of demo MIDI clips, clip, scene and stop buttons, transport, tempo
- [x] 1.2 Launches quantised to the bar (or beat, or immediately), sample-accurate in the audio callback, with tests
- [x] 1.3 Built-in sounds: a Groove Box kit for drums and a small polyphonic synth for bass, chords and lead
- [x] 1.4 Ableton Link: join the session, follow its tempo and phase, change the session's tempo
- [x] 1.5 Offline render and a timing and CPU check (`opendj-studio --check`)
- [ ] 1.6 Try it on macOS next to openDJ over Link
- [x] 1.7 Window laid out like a Session view: browser, thinner tracks with mixer strips (volume and meters work), returns A and B and Main, and a device row for the selected track (sends, returns, browser and effects are not connected yet)

## 2. Instruments and sets

- [x] 2.1 Plugin instruments on tracks through `opendj-plugin-host`
- [x] 2.2 Save and open a set
- [x] 2.3 Ableton Push 2 without Live: User mode, pads launch clips (Session) or play the selected instrument (Note), scene, stop and track buttons, transport, lights in track colours kept in time by MIDI clock, encoders on the instrument knobs or the mixer, and the screen over USB (`opendj-studio --push-preview`)
- [x] 2.4 Instrument knobs and pan work, shared by the window and Push
- [x] 2.5 Try Push 2 on hardware (macOS)
- [ ] 2.6 Novation Launchpad X in Programmer mode
- [x] 2.7 Push screen drawn with Slint's software renderer on the Push thread: knob arcs, track tabs with clip progress, page toasts, animated
- [ ] 2.8 Try the Slint-drawn Push screen on hardware (macOS)
- [x] 2.9 Steps pad mode on Push: stage clips (pitch, octave, velocity, gate, chance, lock amount) with parameter locks
- [ ] 2.10 Try Steps mode on hardware (macOS)
- [x] 2.11 Open an Ableton Live set (.als): tracks with colours, volume and pan, return names, MIDI clips, audio clips following their warp markers (key kept), device lists shown; Push pages tracks in banks of 8
- [ ] 2.12 Open a real Live set with its recordings on hardware (macOS)
- [x] 2.13 Push views (pads) and modes (encoders, screen buttons) apart; buttons report press, release and hold; held Session, Note or Scale only peek
- [x] 2.14 Scales mode on Push: 32 scales, root, five pad layouts, in key or chromatic; stage clips follow the key
- [ ] 2.15 Try Scales mode and peeking on hardware (macOS)
- [x] 2.16 Drum sequencer on Push (Note view on drum tracks): 32 steps, sound pads, clip pages and length, step length on the scene buttons, long notes, note velocity, length and chance on the encoders while steps are held
- [ ] 2.17 Try the drum sequencer on hardware (macOS)
- [x] 2.18 A chain of plug-in slots per track: a Live set's Audio Units and VST3s start in helpers with the sound the set saved, found by id or name; Vital plays in place of Wavetable; devices switch on and off and open their windows
- [ ] 2.19 Open the set with its plug-ins on macOS
- [x] 2.20 Audio inputs per track: Audio From with channel, Monitor In/Auto/Off, Audio To
- [x] 2.21 MIDI From and To per MIDI track, with channel
- [ ] 2.22 Play the set's live inputs and keyboards through their tracks on macOS
- [x] 2.23 Settings with an Audio page: output and input devices, sample rate and buffer, starting from openDJ's audio settings; devices change without closing the set
- [x] 2.24 Drum Racks play their own samples (Live's Core Library or the set's), with each pad's volume, pan, tuning and choke; Push's drum pads follow the rack
- [x] 2.25 The studio's own Utility, EQ Eight and Compressor, with the set's settings, in each track's chain with the plug-ins
- [x] 2.26 The studio's own Echo, Delay, Reverb, Gate and Overdrive with the set's settings; approximations of Hybrid Reverb and Roar
- [ ] 2.27 Hear the set's 808 and 606 kits and its EQ, Utility, Compressor, Delay, Echo, reverbs, Overdrive and Roar on macOS
- [x] 2.28 Panels for the studio's own devices: their settings as knobs and switches, editable while playing; EQ Eight's curve with draggable bands; filter curves for Delay, Overdrive and the reverbs
- [x] 2.29 Hybrid Reverb's convolution: the set's room from Live's app, blended with the plate in its routing
- [ ] 2.30 Chord and Scale MIDI effects, and a Wavetable instrument read from the set
- [x] 2.31 External Instrument: a MIDI track's notes out to a hardware synth and its sound back from an input, sent early by the hardware's latency, read from the set
- [x] 2.32 Plug-ins' MIDI out passed back from the helper (CLAP and VST3), to a track's MIDI To
- [ ] 2.34 MIDI out from Audio Units and Pd patches
- [ ] 2.33 Play a hardware synth through an External Instrument on macOS
- [x] 2.35 Devices dragged from the browser onto a track or a place in its device bar, moved along the chain or to another track, copied with Option, removed with ×; effects stay after the instrument and an instrument replaces the track's; edits fade so they don't click (design.md, Device chains)
- [ ] 2.36 Built-in synths and plug-ins dragged from the browser; dropping on empty space makes a track
- [x] 2.37 Audio Effect and Instrument Racks: branches, zones and macros, read from the set (design.md, Racks)
- [ ] 2.38 Drag, reorder and copy devices on macOS
- [x] 2.39 Device kit: knobs that glide, LFO, envelope follower and 4x oversampling shared by devices; every device checked for no allocation, no runaway, silent tails, a clean bypass and no clicks, and benched against a budget (design.md, Devices)
- [x] 2.40 Limiter, Auto Filter (FunDSP filters) and Saturator (4x oversampled), read from the set
- [x] 2.41 MIDI effects: Arpeggiator, Chord, Scale, Note Length, Velocity, Random, Pitch, Note Echo, in the chain before the instrument, on clip notes and live keys (their own clock runs while stopped), read from the set
- [x] 2.42 Simpler: a sample played across the keys (Classic and One-Shot, loop, envelope, filter), from the browser's Samples or read from the set
- [ ] 2.43 Modulators (LFO, envelope, envelope follower) that drive any knob of any device, as in Bitwig
- [ ] 2.44 Delay compensation for devices that look ahead or oversample (Limiter, Saturator), so tracks stay in time
- [ ] 2.45 One analog-style synth with Bass, Pad and Lead as its presets
- [ ] 2.46 Hear Limiter, Auto Filter, Saturator, the MIDI effects and Simpler on macOS
- [x] 2.47 Drum Rack on the rack engine: each pad a Simpler with its own chain, choke groups; samples dropped on its pads
- [x] 2.48 Racks in the window: open a chain beside its rack, drag devices into a chain (and move them, take them out), new chains by dropping on the chain list, map knobs inside to a macro (Map)
- [x] 2.49 MIDI effects inside rack chains: they lead their chain, before its instrument, and follow the song's beat
- [x] 2.50 Plug-ins inside rack chains: each in its own helper, joining its place once it's up; a rack edit restarts them with the sound they had
- [x] 2.51 Plug-ins in the browser (openDJ's scanned list), to drag onto tracks and into chains
- [ ] 2.52 Racks, next: Live's macro mappings, macros on Push's encoders, macro ranges, keeping plug-ins running through a rack edit

## 3. Clip editing

- [ ] 3.1 New and duplicate clips, clip length
- [ ] 3.2 Piano roll
- [ ] 3.3 Record MIDI into a slot

## 4. Audio clips

- [ ] 4.1 Audio clips warped to the tempo from beatgrids
- [ ] 4.2 Drag from openDJ's library; stems to tracks

## 5. Shared transport

- [ ] 5.1 Link, MIDI clock and transport split out of `opendj-engine` into a crate both apps use
- [x] 5.2 The Studio engine in its own crate with no sound card or UI (`opendj-studio-engine`), able to follow a leader's beat and tempo
- [ ] 5.3 openDJ's Session deck: a Studio set on a fifth deck (its own mixer channel, labelled Session), on its own real-time thread a block ahead, following openDJ's master tempo
- [ ] 5.4 Recording MIDI and audio (a deck or the master) into clips, and capturing the last bars, from the Session deck

## 6. Arrangement

- [ ] 6.1 Arrangement timeline and recording the Session into it
- [ ] 6.2 Automation and export
