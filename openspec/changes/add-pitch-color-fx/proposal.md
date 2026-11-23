# Proposal

Planned for: **v1.0 · Ready for any DJ switching over** (a user request from a DDJ-FLX10 bug report, issue #1029, 2026-11-23)

## Why

Pioneer mixers and controllers have a PITCH Sound Color FX: the channel's COLOR knob shifts the sound down to the left and up to the right, up to an octave, and does nothing in the middle. openDJ's Color FX are Filter, Space, Dub Echo, Noise and Crush, so the PITCH button on the DDJ-FLX10, XDJ-R1, XDJ-RR, DDJ-RX, DDJ-RZ and DDJ-RZX does nothing and the knobs keep playing the filter.

## What Changes

1. **Pitch Color FX.** A sixth Color FX, `loaded_chain_preset` 6, named "Pitch" in the menu beside each filter knob. The knob shifts the deck's sound by up to 12 semitones down (left) or up (right), linearly in semitones, keeping its tempo; it is exactly dry in the middle. The shifter is a low-latency two-head delay-line shifter (50 ms window) whose heads start where the sound lines up with the other head, so pure tones don't cancel where the heads cross; the shift glides over about 10 ms and the effect fades in over the first 5% of the knob and out within a quarter of a second when the knob is back in the middle.
2. **Pioneer PITCH buttons.** The SOUND COLOR FX PITCH button picks it on every deck in the DDJ-FLX10 (`96 03`, `96 0B` with SHIFT), XDJ-R1 (`94 4F`) and XDJ-RR (`B4 56`) mappings, and pressing the lit one goes back to the filter, as the other Color FX buttons do.
3. **Sound Color FX buttons on the DDJ-RX, RZ and RZX.** These mappings had no Sound Color FX buttons at all. They get them, PITCH included, from rekordbox's mappings: RX NOISE, CRUSH, PITCH, FILTER; RZ SPACE, PITCH, FILTER (JET has no Color FX); RZX SPACE, DUB ECHO, CRUSH, SWEEP (the filter), PITCH, FILTER; each with its SHIFT message too, and lit while on.
4. The FX guide and the controller guides list Pitch.

## Capabilities

### Modified Capabilities

- `effects`: the Pitch Color FX
- `controllers`: Sound Color FX PITCH on Pioneer mappings, and Sound Color FX buttons on the DDJ-RX, RZ and RZX

## Out of Scope

- The DDJ-1000 and DDJ-800 (not mapped yet, or their COLOR FX buttons aren't), the DDJ-SZ (rekordbox leaves its Sound Color FX buttons to the hardware) and the DJM mixers, which don't map Sound Color FX buttons yet.
- A separate parameter knob for Pitch (Pioneer's PARAMETER knob); the COLOR knob alone sets the shift.
