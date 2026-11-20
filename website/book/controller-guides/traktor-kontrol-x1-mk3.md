## Getting started

The X1 MK3 is a USB HID controller, not a MIDI one. openDJ has its own
mapping for it, written from the controller's published USB report layout,
so it has never met a real X1 MK3 yet: please tell us how it goes. Plug it in
and it shows up in **Settings › Controllers** within a second or so. Its five
displays stay dark for now.

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

- The left side plays deck 1 and the right side deck 2.
- **MODE** steps the top section through effect units 1 and 2, units 3 and
  4, and the mixer. With effects, the top knob is dry/wet and the other three
  set each effect's amount; the top button turns the unit on or off and the
  others turn each effect on or off (**SHIFT**: next effect in that slot).
  The arrows send the unit to the left or right deck (**SHIFT**: units 3
  and 4). In mixer mode the knobs are high, mid, low and volume, and the
  buttons cut the highs, mids and lows and turn the headphone cue on.
- The upper encoder browses (**SHIFT**: the sidebar) and loads on a push.
  The lower one sets the loop size and starts or ends the loop on a push;
  with **SHIFT** it jumps by the loop's length, moving an active loop along.
  Hold **SYNC** and turn it to change the tempo, pushed for bigger steps.
- **REV** plays backwards while held and carries on where the track would
  have been; **SHIFT + REV** turns flux (slip) on or off.
- **SHIFT + CUE** goes back to the start, **SHIFT + SYNC** makes the deck the
  tempo leader, **SHIFT** + a hot cue deletes it, and the arrows nudge
  (**SHIFT**: beat jump).

## When you try it

- Load a track from each side, play it, set and use hot cues, and nudge it.
- Step **MODE** through all three modes and turn every knob in each.
- Try the loop encoder with and without **SHIFT**, and with **SYNC** held.
- Check that the button and hot cue lights follow what's on screen, and the
  light under each side turns green while a loop plays.
- Unplug it while openDJ runs, plug it back in, and check it comes back.
