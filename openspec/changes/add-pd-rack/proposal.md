# Proposal

Planned for: **v1.0** (experimental, behind the `pd` feature until it's tested on macOS, Windows and Linux). Issue #1005, a user request (2026-11-22).

## Why

Pure Data (Pd) and plugdata let people build effects and instruments by patching, and share them as plain `.pd` files. Hosting the plugdata VST3/CLAP already works through openDJ's plugin host, but every slot then runs a whole plugdata instance, the patch is hidden inside its state, its parameters show as generic slots and its display is a floating window. plugdata's binary is AGPL-3.0 (JUCE), so openDJ can't bundle it.

Running patches with libpd (Pd's engine as a library, BSD-3-Clause) inside openDJ's own plugin helper gives Pd effects the same crash isolation, deadline and FX-slot wiring as CLAP, VST3 and AU, with named knobs read from the patch and, later, the patch's own displays drawn in openDJ.

## What Changes

- **Pd format in the plugin helper** (feature `pd`): one patch per helper process, `[adc~ 1 2]`/`[dac~ 1 2]` for audio, Pd's 64-frame ticks with no added latency when the block is whole ticks and one tick otherwise.
- **Parameters from the patch text**: hvcc's `[r name @hv_param min max default]` and number boxes with a receive name (as an `.amxd` → `.pd` converter writes for Live dials) become knobs with their names and ranges; ids follow the receive name so picked knobs survive edits.
- **Transport and MIDI**: the mix's tempo, beat and play state go to `opendj-bpm`, `opendj-beat`, `opendj-playing`; MIDI notes, CC and pitch bend go to `[notein]`, `[ctlin]`, `[bendin]`. A patch with `[notein]` is an instrument.
- **Pd folder**: patches in the Pd folder (openDJ's music folder › Pd, or `OPENDJ_PD_DIR`) are listed in Settings › Plugins under a Pd chip, and ticked ones show in the FX menu and Instrument input.
- Later steps: patch state and reload on save, "Open in plugdata", cyclone, ELSE and pd-lua built in, plugdata's `[param]`, `.opendj.json` display hints, pd-lua displays drawn natively, converter hand-off.

## Out of scope

- Bundling plugdata or its editor; patches are edited in Pd or plugdata.
- Compiling patches (hvcc) into built-in effects; possible later for curated factory effects.
- Other DSP languages (Faust, Glicol); Glicol could become a second "code FX" format later.

## Capabilities

### Modified Capabilities

- `effects`: effect slots and the Instrument input can run Pure Data patches.

## Impact

- `crates/plugin-host`: `Plugin::Pd`, `pd_patch.rs` (patch reader, always built), `pd.rs` (libpd runtime, feature `pd`), helper `--pd <patch>`, example patches in `crates/plugin-host/pd/`.
- Dependency: `libpd-sys` 0.3.4 (BSD-3-Clause, builds libpd 0.55.2 with CMake; needs `cmake` and `libclang`), optional.
- App (feature `pd`): Pd patches listed beside CLAP, VST3 and AU.
- CI: Linux job builds and tests the `pd` feature of the plugin host.
