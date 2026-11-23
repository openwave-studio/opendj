# Tasks

## 1. Pd in the plugin helper (PR 1, experimental)

- [x] 1.1 `Plugin::Pd` and helper `--pd <patch>`, running the patch in libpd behind the `pd` feature
- [x] 1.2 Read parameters from the patch text (`@hv_param` receives, number boxes with a receive name), ids from the receive name
- [x] 1.3 Whole-tick blocks with no added latency; other block sizes through a one-tick FIFO
- [x] 1.4 Transport to `opendj-bpm`, `opendj-beat`, `opendj-playing`; MIDI notes, CC and bend in
- [x] 1.5 Example patches (gain, beat-synced echo), helper tests and a round-trip benchmark
- [x] 1.6 App feature `pd`: patches from the Pd folder listed as effects or instruments, with a Pd chip in Settings › Plugins
- [ ] 1.7 Try it on macOS and Windows; turn the feature on in release builds once it builds everywhere

## 2. Patches that last

- [ ] 2.1 Save the patch's knob values with the slot; reload a patch when its file is saved
- [ ] 2.2 "Open in plugdata" (or Pd) from the slot
- [ ] 2.3 Sample-exact MIDI within a block

## 3. Libraries

- [ ] 3.1 Build cyclone and ELSE into the helper
- [ ] 3.2 Understand plugdata's `[param]` objects

## 4. Displays

- [ ] 4.1 `.opendj.json` hints next to a patch: pages, knob, slider, XY pad or switch, units, macro for the FX bar
- [ ] 4.2 pd-lua in the helper; its `paint(g)` drawing forwarded and drawn natively in the deck, with mouse input

## 5. Max for Live devices

- [ ] 5.1 Take an `.amxd` → `.pd` converter's output (patch, params and layout from Live dials) into the Pd folder
