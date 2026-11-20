## Getting started

1. Connect the DJM-S9's USB port to the computer. On Windows, install
   AlphaTheta's DJM-S9 driver first; macOS needs none.
2. In **Settings › Audio**, choose the DJM-S9 as the sound card, set
   **Mixing** to **External mixer**, and send Deck 1 and Deck 2 to its USB
   channels. Turn the channels' input switches to USB.
3. openDJ picks this mapping by itself.

This mapping is openDJ's own, written from AlphaTheta's list of the DJM-S9's
MIDI messages. The DJM-S9 mixes the sound itself; openDJ follows its channel
faders and crossfader so it knows which deck the room hears.

- **BROWSE** moves through the list and opens folders; turning it with SHIFT
  zooms the waveforms. **BACK** moves between the list and the sidebar. **LOAD** loads
  the selected track; SHIFT and LOAD adds it to Automix.
- **Pads**: **HOT CUE** plays hot cues 1 to 8 (SHIFT and a pad clears it),
  **ROLL** plays rolls from 1/16 to 8 beats, **SAMPLER** plays samplers 1 to 8
  (SHIFT stops them). SHIFT and **HOT CUE** gives **BEAT JUMP**: 1, 2, 4 and
  8 beats back and forward. SHIFT and **SAMPLER** (velocity sampler) plays
  the samplers too.
- **4 BEAT** sets a 4-beat loop; SHIFT goes back into the last loop.
  **1/2X** and **2X** halve and double it; with SHIFT they set the loop in
  and out.
- The **FX** levers hold openDJ's effect units: the CH1 lever plays the
  effect picked on FX1 on deck 1, the CH2 lever FX2 on deck 2.
  **LEVEL/DEPTH** sets the amount. Hold an FX button and turn BROWSE to
  pick an effect.
- **SAMPLER VOLUME** sets the samplers' volume.

Not mapped yet: SLICER and SLICER LOOP, PAD FX 2, the PARAMETER buttons,
BEAT ◀ ▶, TAP, the fader start notes and the panel buttons.

## When you try it

- Check the pads light in each mode, and that the hot cue lights follow.
- Move both channel faders and the crossfader: openDJ's volumes should
  follow without jumps (they're 14-bit).
- Hold each FX lever with each FX button picked.
