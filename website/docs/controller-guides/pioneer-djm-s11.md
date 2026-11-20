## Getting started

1. Connect the DJM-S11's USB port to the computer. On Windows, install
   AlphaTheta's DJM-S11 driver first; macOS needs none.
2. In **Settings › Audio**, choose the DJM-S11 as the sound card, set
   **Mixing** to **External mixer**, and send Deck 1 and Deck 2 to its USB
   channels.
3. openDJ picks this mapping by itself.

This mapping is openDJ's own, written from the DJM-S11's MIDI messages. The
DJM-S11 mixes the sound itself; openDJ follows its channel faders and crossfader
so it knows which deck the room hears.

- **BROWSE** moves through the list and opens folders; **BACK** moves between
  the list and the sidebar. **LOAD** loads the selected track; SHIFT and
  LOAD adds it to Automix.
- **Pads**: **HOT CUE** plays hot cues 1 to 8 (SHIFT and a pad clears it),
  **ROLL** plays rolls from 1/16 to 8 beats and **SAMPLER** plays samplers 1
  to 8 (SHIFT stops them).
- **AUTO LOOP** sets a 4-beat loop; SHIFT goes back into the last loop.
  **1/2X** and **2X** halve and double it; with SHIFT they set the loop in
  and out.
- The channel **FX** levers play openDJ's effect units 1 and 2 on that
  channel while held; the **FX** knobs set each effect's amount.
- **SYNC** (SHIFT: master), **MASTER TEMPO** and the pitch bend buttons work
  the decks. The touch screen keeps its own screen.

Not mapped yet: SAVED LOOP, PITCH PLAY, SLICER LOOP, SAVED FLIP, SCRATCH
BANK, the pad PAGE and PARAMETER buttons, BEAT ◀ ▶, TAP and the second row
of pads.

## When you try it

- Check the pads light in each mode, and that the hot cue lights follow.
- Move both channel faders and the crossfader: openDJ's volumes should
  follow without jumps.
- Hold each FX lever.
