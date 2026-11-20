## Getting started

The Z1 MK2 is a USB HID mixer, not a MIDI one. openDJ has its own mapping
for it, written from the controller's published USB report layout, so it
has never met a real Z1 MK2 yet: please tell us how it goes. Plug it in and
it shows up in **Settings › Controllers** within a second or so. It's also a
sound card: choose it in **Settings › Audio**. Its displays stay dark for
now, and its **MAIN** knob sets the level on the device itself.

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

- The channels play decks 1 and 2; the button at the top between them
  switches to decks 3 and 4.
- **STEMS** (on a track with stems) turns **GAIN**, **HIGH**, **MID** and
  **LOW** into the volumes of the drums, bass, other and vocals stems, from
  silent to full. **EQ** turns them back into the EQ.
- The **FX** knobs play the Mixer FX, on or off with **ON**. **FILTER**
  picks the filter for both channels, and **1** to **4** pick Space, Dub
  Echo, Noise and Crush.
- The knobs pick values up where they are, so nothing jumps after switching
  decks or modes.

## When you try it

- Move every fader and knob, on decks 1 and 2 and then 3 and 4.
- Load a track with stems and try **STEMS**.
- Try each Mixer FX button with the **FX** knobs.
- Check that the button lights and the level meters follow what's on
  screen.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
