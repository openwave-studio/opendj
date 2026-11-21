## Getting started

The MX2 is a USB HID controller, not a MIDI one: openDJ talks to it
directly and runs Mixxx's HID mapping for it. Plug it in and it shows up in
**Settings › Controllers** within a second or so, running this mapping. It's
also a sound card: choose it in **Settings › Audio**, with the main output on
channels 1-2 and the headphones on 3-4.

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

- The pad mode buttons are Traktor's: **HOTCUES**, **STEMS** and **LOOPS**.
  In **STEMS**, pads 1-4 mute and unmute stems 1-4, and holding pad 5-8 puts
  that stem on the encoders: the left one sets its volume and the right one
  its filter. In **LOOPS** the pads roll loops from 1/16 to 8 beats. Traktor
  plays down to 1/32, which openDJ doesn't have, and **PATTERNS** does
  nothing yet.
- **SHIFT + CUE** goes back to the start of the track.
- **REV** plays backwards while held and carries on where the track would
  have been; **SHIFT + REV** is a plain reverse.
- Hold the key button and turn the loop encoder to change the key.

## When you try it

- Load a track with the browse encoder, play it, and scratch and nudge it with
  the jog wheels.
- Try the pad modes (**HOTCUES**, **STEMS**, **LOOPS**) with and without SHIFT, on a
  track with stems and one without.
- Move every fader and knob, and check the tempo faders.
- Check that the button and pad lights, and the level meters, follow what's
  on screen.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
