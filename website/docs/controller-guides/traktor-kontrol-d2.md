## Getting started

The D2 is a USB HID controller, not a MIDI one. openDJ has its own mapping
for it, written from the controller's published USB report layout, so it
has never met a real D2 yet: please tell us how it goes. Plug it in and it
shows up in **Settings › Controllers** within a second or so. Its display
stays dark for now, and the buttons around it, **FX SELECT**, **BACK**,
**CAPTURE** and **EDIT** do nothing yet.

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

- The D2 plays one deck at a time: deck A to start with. **DECK** switches
  to the deck below (A and C, or B and D) and lights blue on A or B, white
  on C or D. Hold **DECK** and press **A**, **B**, **C** or **D** to pick
  any deck.
- The four knobs and buttons on top play effect unit 1 with decks A and C,
  unit 2 with B and D: dry / wet and the three effects, each on or off
  (SHIFT picks the next effect). **A** to **D** send each deck to that unit;
  with SHIFT, to the second unit below.
- On a track with stems the four faders are the stems' volumes, from silent
  to full, and **ON** sends each stem to the stem effect (SHIFT mutes it).
  The knobs above **ON** play the second unit, 3 or 4, as Traktor does with
  four effect units; on a track without stems **ON** switches its effects.
- Pads: **HOTCUE** plays eight hot cues (SHIFT deletes). **LOOP** plays
  loops of 1/8, 1/4, 1/2 and 1 beat on the top row, and on the bottom row
  jumps back by the loop, back a beat, forward a beat, forward by the loop.
  **FREEZE** plays the slicer, one beat on each pad. **REMIX** plays the
  eight samplers.
- The loop encoder halves or doubles the loop and pushes it on or off; with
  SHIFT it jumps by the loop's length. **BROWSE** scrolls the list and loads
  the track on a push.
- The touch strip bends the tempo while playing and scrubs while paused;
  with SHIFT, a touch jumps to that place in the track. Its lights show how
  much of the track has played. **FLUX** turns flux (slip) on or off.

## When you try it

- Press **PLAY** on each deck, switching with **DECK** and **DECK** + A-D.
- Try each pad mode, and the loop and browse encoders.
- Load a track with stems and move the faders.
- Turn the effect knobs on top and below, and send decks with **A** to
  **D**.
- Check the button and pad lights follow what's on screen.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
