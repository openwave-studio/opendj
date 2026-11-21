## Getting started

The S5 is a USB HID controller, not a MIDI one. openDJ has its own mapping
for it, but a partial one, and partly a guess: the only published layout
of the S5's USB reports leaves gaps and mostly copies the S8's. Nobody has
tried this mapping on a real S5 yet, so please tell us how it goes. Plug it
in and it shows up in **Settings › Controllers** within a second or so.
It's also a sound card: choose it in **Settings › Audio**.

What doesn't work yet:

- Turning the **BROWSE** and **LOOP** encoders. Their pushes are mapped
  (load the track, loop on or off), but where they sit in the reports is a
  guess.
- Channel D's **LOW** and **FILTER** knobs.
- **AUX**, **S** (snap), the **TEMPO** encoder, the **◄** button by
  **BROWSE** and the buttons around the displays.
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
  right one plays B, and **DECK** switches it to D.
- The four knobs and buttons on top play effect unit 1 on the left and 2
  on the right: dry / wet and the three effects, each on or off (SHIFT
  picks the next effect).
- Pads: **HOTCUE** plays eight hot cues (SHIFT deletes), **FREEZE** plays
  the slicer, one beat on each pad, and **REMIX** plays the eight
  samplers.
- The touch strip bends the tempo while playing and scrubs while paused;
  with SHIFT, a touch jumps to that place in the track. **FLUX** turns flux
  (slip) on or off.
- The mixer's channels are C, A, B and D from left to right. Each has gain,
  EQ, a filter knob that plays the Mixer FX (on or off with **FILTER**),
  **CUE** and a volume fader. The **◄ ►** buttons under **GAIN** send the
  channel to effect units 1 and 2, or 3 and 4 with SHIFT. **Q** turns
  quantize on or off for every deck.

## When you try it

- Press **PLAY** on each section, and switch decks with **DECK**.
- Move every mixer knob and fader, and tell us if any does nothing or
  something else.
- Turn the **BROWSE** and **LOOP** encoders and tell us if anything moves.
- Try each pad mode, and check the pad and button lights.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
