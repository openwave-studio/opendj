## Getting started

The S2 MK1 (the first S2, with black plastic jog wheels you press to
scratch) is a USB HID controller, not a MIDI one: openDJ talks to it
directly and runs Mixxx's community HID mapping for it. Plug it in and it
shows up in **Settings › Controllers** within a second or so, running this
mapping. It reads the controller's own calibration for its faders and
knobs. It's also a sound card: choose it in **Settings › Audio**, with the
main output on channels 1-2 and the headphones on 3-4. On Windows the sound
card needs Native Instruments' driver.

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
Editor, but the editor shows its picture and lists what every control does
in each layer. The picture is a sketch of the layout, not measured.

## How it plays

- The pads are hot cues 1-4 (**SHIFT** deletes one). **SAMPLES ON A** or
  **B** turns that deck's pads into sampler pads (samplers 1-4 and 5-8).
- **GAIN** turns the deck's filter; **SHIFT + GAIN** is the gain. Push and
  turn it to pick another quick effect.
- The left loop encoder beat jumps (push and turn: the jump size, **SHIFT**:
  the key); the right one halves and doubles the loop and pushes it on or
  off.
- **RESET** puts the tempo back to 0%. **SHIFT + PLAY** is key lock,
  **SHIFT + LOAD** ejects the deck.
- The effect sections run effect units 1 and 2; the **FX 1** and **FX 2**
  buttons on each channel send it to them.

## When you try it

- Load a track with the browse encoder, play it, and scratch (press the top)
  and nudge (the edge) with the jog wheels.
- Try the pads with and without SHIFT, in hot cue and sampler mode.
- Move every fader and knob, and check each reaches both ends.
- Check that the button and pad lights, and the level meters, follow what's
  on screen.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
