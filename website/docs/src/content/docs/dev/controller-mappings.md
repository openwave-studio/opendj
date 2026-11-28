---
title: Writing a controller mapping
---

This page is for adding a controller openDJ doesn't support yet, and is
written so you can hand it to a coding assistant as its brief. Give the
assistant this page, a clone of the repository, and what you know about the
controller: the manual's MIDI chart if there is one, another DJ app's mapping
file for it, or messages you captured from it (below). It should end with a
pull request that makes the controller work.

If you'd rather not write code, the [Mapping wizard and Map
mode](../guide/controllers.md#mapping-wizard) in the app make a mapping of
your own without touching the repository. Export it (**Export…** in the
Controller Editor's Mapping menu) and attach the `.opendj-mapping` file to an
issue, and we can ship it from there.

## How openDJ runs controllers

- **Mappings are Mixxx's format.** openDJ runs Mixxx's `.midi.xml` mappings
  and their JavaScript as Mixxx does: the `engine`, `midi`, `controller`,
  `console` and `ColorMapper` objects scripts use come from
  `crates/controllers/src/prelude.js`. A Mixxx mapping that works in Mixxx
  should work here unchanged. openDJ's own format, `.opendj-mapping`, is a
  superset (see [Controller mapping format](mapping-format.md)); use it only
  when you need what Mixxx can't say.
- **Files.** Everything shipped lives in `res/controllers/`: the
  `<Brand> <Model>.midi.xml` file and its `<Brand>-<Model>-scripts.js`. USB
  HID controllers use `.hid.xml` with a script built on
  `common-hid-packet-parser.js`.
- **Picking a mapping.** When a controller is plugged in, openDJ matches its
  MIDI port name against each mapping's `<info><name>`
  (`match_mapping` in `crates/controllers/src/manager.rs`): every word of the
  port name has to appear in the mapping's name, and model numbers must
  agree. Name the mapping so the port name fits it.
- **Controls.** Scripts and bindings drive engine controls by group and key,
  `[Channel1],play`, `[EffectRack1_EffectUnit1],mix`. The full list of what
  openDJ has is `website/docs/supported-controls.txt` (the output of
  `cargo run -q -p opendj -- --list-controls`). Mixxx controls missing from
  that list do nothing in openDJ yet; don't rely on them.

## Step by step

1. **Check it isn't there already.** Search `res/controllers/` for the
   model, and check `crates/controllers/support.json`. A controller is often
   covered by a sibling's mapping (the same hardware under another name, or
   an MK2 that sends the same messages); then the fix is a port-name match,
   not a new mapping.
2. **Find the MIDI messages.** In order of preference:
   - the maker's MIDI chart (often in the manual's appendix or on its
     support site);
   - Mixxx's mapping for it, if Mixxx has one newer than the copy here
     (copy it unchanged, with its authors and licence header, and note the
     Mixxx commit in the PR);
   - another app's mapping file (Traktor `.tsi`, rekordbox `.midi.csv`,
     VirtualDJ `.xml`, djay `.djayMidiMapping`): openDJ's importers in
     `crates/controllers/src/import/` read these, and they list every
     message the controller sends;
   - captured messages: run openDJ with `OPENDJ_MIDI_DEBUG=1` set and every
     message in and out is written to the log (**Help › Show Logs and Crash
     Reports**). Press each control once, and note which is which. Write
     down what each LED expects too, if the chart doesn't say.
3. **Write the mapping.** Start from a mapping for a similar controller from
   the same maker and change the messages. Keep to plain `<control>`
   bindings where you can, and use the script for what needs logic: jog
   wheels, shift layers, pad modes, LEDs that follow state. Name the
   `<info><name>` so it matches the port (step above). Put your name in
   `<author>`.
4. **Picture (for the Controller Editor).** Add
   `crates/controllers/layouts/<model>.json`: the controller's outline and
   each control with `id`, `label`, `kind` (`button`, `pad`, `knob`,
   `encoder`, `fader`, `jog`, `switch`), position and size, and the
   messages it sends in `midi` (`"90 0B"`, `"SHIFT|90 47"`). Copy the shape
   of an existing layout. Register it in the `LAYOUTS` list in
   `crates/controllers/src/layout.rs` (bump the array length).
5. **Listing and guide.** Add the controller to
   `crates/controllers/support.json` with status `"beta"` (it becomes
   `"supported"` once someone has tried it on the hardware) and a one-line
   note on anything that doesn't work yet. Write a short guide in
   `website/docs/controller-guides/<model>.md` (see the others: getting
   started, what the pads and FX do, what to check when you try it), and add
   the model to the brand's line in
   `website/docs/src/content/docs/guide/controllers.md`.
6. **Docs figure data.** Run
   `cargo run -p opendj-controllers --example docs_controls -- website/docs/figures/functions`
   and commit the new JSON file it writes for your mapping.
7. **Tests.** In `crates/controllers/tests/mappings.rs`:
   - `every_bundled_mapping_loads_and_takes_its_messages` picks up a new
     file by itself; it must pass.
   - Add a test for your mapping that feeds it MIDI as the hardware would and
     checks the engine controls: see `ddj_flx4` for the pattern (`load`,
     `c.receive(&[status, number, value])`, `get(&reg, group, key)`,
     `settle` for timers, `c.take_output()` for LEDs). Cover play, a fader,
     the jog wheel, shift, and each pad mode.
   - Add the port name to `matches_ports_to_mappings` in
     `crates/controllers/src/manager.rs`, so the match is checked.
8. **Check and open the PR.** Run `cargo fmt --check`,
   `cargo clippy -p opendj-controllers --all-targets -- -D warnings` and
   `cargo test -p opendj-controllers`. Then open the pull request as
   [CONTRIBUTING.md](https://github.com/openwave-studio/opendj/blob/main/CONTRIBUTING.md)
   says: link the issue (`Fixes #N`), list what works and what doesn't yet,
   and say whether it was tried on the hardware.

## Rules that keep a PR easy to merge

- **Stay on your controller.** Don't change shared scripts, other mappings,
  or how controllers behave in general (pick-up, the browser, effects
  menus). If your controller needs an engine change, open it as its own
  small PR.
- **No UI changes** in a mapping PR. If the controller needs something on
  screen, open an issue first.
- **Licence.** Mappings are GPL-2.0-or-later. Copy only from sources you may
  use (Mixxx, the maker's published charts, your own captures), and keep
  existing authors' notices.
- **Say what's untested.** A mapping nobody has tried on the hardware is
  fine: it ships as Beta, and the guide's "When you try it" list tells the
  first tester what to check.
