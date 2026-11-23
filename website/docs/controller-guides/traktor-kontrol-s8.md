## Getting started

The S8 is a USB HID controller, not a MIDI one. openDJ has its own mapping
for it, but a partial one: the only published layout of the S8's USB
reports leaves gaps. Plug it in and it shows up in
**Settings › Controllers** within a second or so. It's also a sound card:
choose it in **Settings › Audio**.

What doesn't work yet:

- Turning the **BROWSE** and **LOOP** encoders. Their pushes are mapped
  (load the track, loop on or off), but where they sit in the reports is a
  guess.
- **SNAP**, **MIC 2**, the **TEMPO** encoder, **FX SELECT**, **BACK**,
  **CAPTURE**, **EDIT** and the buttons around the displays.
- The displays stay dark. The deck lights follow the D2's layout and may
  be off.
- **MAIN**, **BOOTH**, **CUE MIX** and **CUE VOL** set levels on the device
  itself.

Setup by system:

- **macOS:** quit Traktor first. If the controller still isn't picked up,
  press **Refresh** under the list.
- **Windows:** close any other DJ app that has the controller open.
- **Linux:** the system only lets root open HID devices until you add
  openDJ's udev rule, `70-opendj-hid-controllers.rules` (in `share/opendj`
  in the download, `res/linux` in the source). Copy it to
  `/etc/udev/rules.d/`, run
  `sudo udevadm control --reload-rules && sudo udevadm trigger`, and plug the
  controller in again.

HID mappings are all script, so it can't be remapped in the Controller
Editor, but the editor shows its picture, drawn from Native Instruments'
manual, and lists what every control does in each layer.

## Plays like Traktor

- The left deck section plays deck A, and **DECK** switches it to C. The
  right one plays B, and **DECK** switches it to D. Each section works as
  the [D2's](traktor-kontrol-d2.md) does: an effect unit on top (1 on the
  left, 2 on the right), stems on the performance faders, and pads in
  **HOTCUE**, **LOOP**, **FREEZE** (slicer) and **REMIX** (samples) modes.
- The mixer's channels are C, A, B and D from left to right. Each has gain,
  EQ, a filter knob that plays the Mixer FX (on or off with **FILTER**),
  **CUE** and a volume fader. **FX 1** and **FX 2** send the channel to
  effect units 1 and 2, or 3 and 4 with SHIFT.
- **QUANT** turns quantize on or off for every deck. **MIC 1** is the mic
  talkover. The crossfader assign switches on the front pick each
  channel's side.

## When you try it

- Press **PLAY** on each section, and switch decks with **DECK**.
- Move every mixer knob and fader, especially the **LOW** knobs on
  channels C and D, and tell us if any does nothing or something else.
- Turn the **BROWSE** and **LOOP** encoders and tell us if anything moves.
- Try each pad mode, and check the pad and button lights.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
