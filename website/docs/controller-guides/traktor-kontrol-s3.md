## Getting started

The S3 is a USB HID controller, not a MIDI one: openDJ talks to it
directly and runs Mixxx's HID mapping for it. Plug it in and it shows up in
**Settings › Controllers** within a second or so, running this mapping. It's
also a sound card: choose it in **Settings › Audio**, with the main output on
channels 1-2 and the headphones on 3-4 (check its manual if the sound comes out of the wrong place).

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
manual, and lists what every control does in each layer. The mapping's own
options (jog wheel feel, pad colours and so on) follow Mixxx's defaults.

## Plays like Traktor

- **SAMPLES** on a track with stems turns the pads into stem pads: pads 1-4
  mute and unmute stems 1-4, and holding pad 5-8 picks that stem so the
  **MOVE** encoder sets its volume. On a track without stems the pads play
  the samplers.
- **SHIFT + CUE** goes back to the start of the track.
- **SHIFT + SYNC** makes the deck the tempo leader, and **SHIFT + KEYLOCK**
  resets the tempo.
- **REV** plays backwards while held and carries on where the track would
  have been; **SHIFT + REV** is a plain reverse.
- Hold **GRID** and turn the jog wheel to move the beat grid. A tap turns
  quantize on or off; **SHIFT + GRID** sets a beat where the deck is.

## When you try it

- Load a track with the browse encoder, play it, and scratch and nudge it with
  the jog wheels.
- Try the pad modes (**HOTCUES**, **SAMPLES**) with and without SHIFT, on a
  track with stems and one without.
- Move every fader and knob, and check the tempo faders and the deck C and D buttons.
- Check that the button and pad lights, and the level meters, follow what's
  on screen.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
