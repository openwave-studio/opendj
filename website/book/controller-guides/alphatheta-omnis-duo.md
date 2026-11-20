## Getting started

The OMNIS-DUO is an all-in-one system, so openDJ drives it in its **PC
control** (MIDI) mode:

1. Connect the OMNIS-DUO to the computer with a USB cable and switch it to PC control the way you would for rekordbox.
2. In **Settings › Audio**, choose the OMNIS-DUO as the sound card.
3. openDJ picks this mapping by itself.

Use the **4-Deck** layout to see all four decks on screen.

This mapping is openDJ's own, written from AlphaTheta's list of the OMNIS-DUO's MIDI messages and the messages rekordbox maps on it. It shares its
script with openDJ's other Pioneer all-in-one mappings, so they work the same
way where they share controls. The picture in the Controller Editor is a
sketch from the unit's size, not measured.

- **PLAY/PAUSE**, **CUE** (SHIFT: back to the start), the jog wheel
  (**VINYL** swaps scratching and bending; SHIFT and turn searches) and the
  **TEMPO** slider work on each deck. **MASTER TEMPO** is key lock, **BEAT
  SYNC** syncs (held or with SHIFT: this deck leads) and **QUANTIZE** sets
  quantize for every deck.
- **LOOP IN**, **LOOP OUT** and **RELOOP/EXIT** set and leave loops; holding
  **LOOP IN** lets the jog move the loop start. **CALL ◀ ▶** halve and double
  the loop playing.
- **TRACK SEARCH** jumps 32 beats and **SEARCH** by the deck's beat jump size.
- **Pads** play hot cues A to H; SHIFT clears them.
- **BEAT LOOP**: turn it to halve or double the loop size, press it for a loop of that size or to leave the loop.
- **PARAMETER ◀ ▶** halve and double the loop playing.
- **DECK SELECT** picks deck 1 or 3, 2 or 4 on each side; each deck sends on its own MIDI channel.
- The **mixer** (TRIM, EQ, COLOR, CUE, channel faders, crossfader) runs openDJ's mixer.
- **BEAT FX**: **BEAT ◀ ▶** halve and double the length of openDJ's Beat FX, **LEVEL/DEPTH** sets the amount and **ON/OFF** turns it on.
- The **ROTARY SELECTOR** moves through the list (SHIFT: zooms the
  waveforms), press opens or loads. **BACK** moves to the sidebar and **TAG
  TRACK** adds the track to Automix. **LOAD** loads the selected track.

Not mapped yet: AUTO CUE, the crossfader curve and the Bluetooth buttons.

## When you try it

- Load a track on each deck and check PLAY, CUE, the hot cues and the jog
  wheel follow.
- Move every knob and fader, and turn on each effect.
- When it starts, openDJ switches the OMNIS-DUO's mixer to MIDI control, so openDJ mixes; it switches it back when you quit. Check the mixer doesn't also mix by itself.
- rekordbox maps a MASTER button that AlphaTheta's list doesn't have; tell us if your unit has it.
