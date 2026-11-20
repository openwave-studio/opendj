## Getting started

The DJM-250MK2 mixes the sound itself, so openDJ plays each deck out of its own
USB channel and lets the DJM-250MK2 do the mixing:

1. Connect the DJM-250MK2's USB port to the computer. On Windows, install
   AlphaTheta's DJM-250MK2 driver first; macOS needs none.
2. In **Settings › Audio**, choose the DJM-250MK2 as the sound card, set
   **Mixing** to **External mixer**, and send Deck 1, Deck 2 to the DJM-250MK2's USB channels.
3. On the DJM-250MK2, turn each channel's **INPUT SELECT** to **USB**.
4. openDJ picks this mapping by itself.

The DJM-250MK2 sends MIDI for its channel faders, crossfader, and for **INPUT SELECT**. openDJ uses them to know which decks the room
hears: a deck is on air while its channel's INPUT SELECT is on this
computer's USB port and its fader is up, and openDJ's own channel
volume follows. The EQs, filters, effects and headphone cues stay on the
mixer.

If you'd rather mix in openDJ and use the mixer only as a sound card, leave
**Mixing** on openDJ: the faders then turn openDJ's decks down as well as
the mixer's channels.

## When you try it

- Check the DJM-250MK2 sends MIDI at all: move a channel fader with **Settings ›
  Controllers** open and watch for activity. Some DJM mixers only send MIDI
  with their **MIDI** setting on; tell us if you had to change it.
- Move each fader and check openDJ's deck volume follows, then the crossfader and the crossfader assign switches.
- Turn a channel's INPUT SELECT away from USB and back: its deck should go
  quiet in openDJ and come back.
- Tell us whether the mixer sends where its controls sit when it connects,
  or only when you move them.
