## Getting started

The XDJ-RX is an all-in-one system, so openDJ drives it in its **PC
control** (MIDI) mode:

1. Connect the XDJ-RX to the computer with a USB cable and switch it to PC control the way you would for rekordbox (press **BROWSE**, pick the computer under **MIDI**).
2. In **Settings › Audio**, choose the XDJ-RX as the sound card.
3. openDJ picks this mapping by itself.

This mapping is openDJ's own, written from AlphaTheta's list of the XDJ-RX's MIDI messages (ver. 1.00) and the messages rekordbox maps on it. It shares its
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
- **Pads**: HOT CUE plays hot cues A to H (SHIFT clears them), AUTO LOOP sets beat loops of 1, 2, 4 and 8 beats, PAD FX plays loop rolls.
- The **mixer** (TRIM, EQ, COLOR, CUE, channel faders, crossfader) runs openDJ's mixer.
- **BEAT FX**: each effect button loads the nearest openDJ effect into openDJ's Beat FX, **BEAT ◀ ▶** halve and double its length, **TAP** taps its tempo, **LEVEL/DEPTH** sets the amount and **ON/OFF** turns it on.
- **SOUND COLOR FX**: NOISE, CRUSH and FILTER pick what the COLOR knobs play.
- The **ROTARY SELECTOR** moves through the list (SHIFT: zooms the
  waveforms), press opens or loads. **BACK** moves to the sidebar and **TAG
  TRACK** adds the track to Automix. **LOAD** loads the selected track.

Not mapped yet: the SLICER pads, MEMORY, DELETE, TIME MODE, GATE/COMP, PITCH, VINYL SPEED ADJUST, the input selectors and the jog ring lights.

## When you try it

- Load a track on each deck and check PLAY, CUE, the hot cues and the jog
  wheel follow.
- Move every knob and fader, and turn on each effect.
- The XDJ-RX has no MIDI output on its first firmware, so its lights may not follow openDJ. Tell us your firmware version.
- Its first MIDI list sends the TEMPO slider in 7 bits; later firmware adds a finer second message. Both work; check the tempo moves smoothly.
