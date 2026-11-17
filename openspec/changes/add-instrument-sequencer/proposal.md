# Proposal

Planned for: **v0.8** (moved from v1.x 2026-11-17 to build it now). Follows `rework-instrument-deck` (v0.8). Issue #819, a user request (2026-11-16).

## Why

With ☰ › Keyboard › Hidden, the instrument deck's lower half only holds a second page of knobs. A DJ playing a synth in a set wants a riff that runs on its own, in time and in key, while their hands are on the mixer. Step sequencers in synths (Pigments' sequencer, Acid V's 303-style steps) do this with a row of steps and a few lanes, which fits the room the keyboard leaves.

## What Changes

- **Step sequencer** in the instrument deck's keyboard space when the keyboard is hidden (2-Deck). One note per step, 16 steps (32 later on a second page).
- **Lanes**, modelled on Pigments' sequencer: **ON/OFF** (which steps play), **CHANCE** (probability), **PITCH** (bars dragged up and down; notes snap to the deck's scale, C minor when SCALE is off), **VEL**, **OCT** (−1, 0, +1), **GATE** (step length, up to a tie into the next step) and **SLIDE** (into the next note, sent as legato with portamento where the plugin offers it). Accent is velocity at the top of its range, as on Acid V.
- **Per-lane controls**, as in Pigments: **RANDOM** (fill that lane only), **DIV** (the lane's own length, for polymetric riffs; off = the pattern's length) and **RESET** (clear the lane). PITCH and VEL also take an offset (semitones, percent) for the whole lane.
- **Layout in the deck**: the room under the knobs (about 712 × 110 px) can't stack all seven lanes like Pigments, so PITCH shows tall with ON/OFF folded into it (an empty step is off), and the other lanes show one at a time under it, picked by tabs (CHANCE, VEL, OCT, GATE, SLIDE). The per-lane RANDOM, DIV and RESET sit at the lane's left.
- **Playback**, Pigments' left panel in a compact strip: ▶ (starts on the next bar of the mix's beat, as the Groove Box), **Mode** (forward, backward, ping-pong, random), **Rate** (synced to the beat: 1/8, 1/16, 1/32, triplets), **Swing**, and **● REC**, which records notes played on keys or pads into the steps while it runs.
- **Generation**: **Dice** writes a random riff in key, with undo (✕) and keep (✓), and a generation style (for example "contrasting minor", "steady bass", "arp-like"). **Auto-Regen** rolls a new riff every N bars.
- **Pattern slots A B C D**, as on the Groove Box, and **LEN** 1 to 16 for the whole pattern.
- The steps' notes go in where keys do, so **Scale**, **Chord**, **Arp** and **HOLD** apply to them.
- Patterns are saved with the instrument's sound (in its preset).
- 4-Deck shows ▶ and the slot in the strip, no steps.

## Out of scope

- A scale-grid or piano-roll view (drawn as alternatives S1 and S3 in the mockups).
- Sequencing plugin parameters (Pigments' modulation lanes); maybe later as a lane.
- Sharing the pattern engine with the Groove Box's melodic row (`add-groove-box-2`); this change's engine should be written so it can be reused.

## Capabilities

### Modified Capabilities

- `instruments`: the instrument deck gets a step sequencer when its keyboard is hidden.

## Impact

- Engine: a step clock on the master deck's beat grid (as `groove.rs`) feeding the instrument's note input before scale, chord and arp; per-lane lengths; accent as velocity, slide as overlapping notes.
- App: sequencer state per instrument deck, saved in the preset; `[Instrument1],seq_*` controls for ▶, slot, REC, so controllers and OSC can drive it.
- UI: `InstrumentRack` shows the sequencer in place of the keyboard when hidden.
