# Proposal

Planned for: **v0.9.6**. Issue #1166, a user request (2026-11-26) after a design write-up.

## Why

DJs and live players with hardware synths (first: Arturia MicroFreak and Moog Messenger, both USB MIDI with line outs) want to play them from openDJ like an instrument plugin: macro knobs, Note FX, the melodic step sequencer, scope and spectrum, presets, the FX rack. Today a hardware synth can only be heard on an Aux strip, with no notes, knobs or timing from openDJ. openDJ Studio already has an External Instrument (MIDI to a port, audio back on an input, latency); the instrument deck has none.

## What Changes

- **Hardware synth source**: the instrument slot (`[Instrument1]`) holds either a plugin or a hardware synth, one at a time.
- **Notes and knobs out**: everything the deck plays (keys, pads, Scale/Chord/Strum, Arp, HOLD, step sequencer) goes to the synth's MIDI port and channel; the macro knobs send CCs. A MIDI thread sends each message at the moment it is due (sleep then spin, as MIDI clock out does).
- **Sound back**: the synth's line out comes in on an audio interface input (`Instrument` input role, mono or a pair) and plays as the instrument's sound, so scope, spectrum, PRE/POST, FX rack, level and mixer channel work as for a plugin.
- **Latency**: `[Instrument1],hardware_latency` (0-200 ms). Sequenced and arpeggiated notes are worked out that much ahead and sent early so the sound lands on the beat; played keys go out at once. A Measure button times a test note's onset on the input.
- **Synth profiles**: a file per model with the MIDI port to match, channel, named and grouped CCs, 8 default macros, preset banks and factory preset names; shared with openDJ Studio's External Instrument in `opendj-studio-engine`. MicroFreak, MiniFreak and Moog Messenger ship first; any other synth starts from a generic profile and learns CCs.
- **Presets**: the preset bar sends program change (and bank select) with the profile's factory names; deck presets keep the program, macros, Note FX and sequencer.
- **Clock and transport**: MIDI clock and Start/Stop go to the synth's port following the deck it plays on (Start on the next bar), so its own sequencer and arpeggiator run with the mix, early by the same latency.
- **Load and set up**: "Hardware synths" in the Jam tree beside plugins; a setup sheet for port, channel, input and latency.

## Out of scope

- CV/gate jacks (USB carries MIDI only); real CV would need a DC-coupled interface output.
- Two-way knobs (the synth's own knob moves turning openDJ's knob): later.
- A hardware synth and a plugin playing at once on two decks: needs `add-instrument-per-deck`.

## Capabilities

### Modified Capabilities

- `instruments`: the instrument deck can play a hardware synth over MIDI with its sound from an audio input.

## Impact

- Engine: `hardware_synth.rs` (MIDI out thread, timed queue from the audio thread), the effect rack's instrument slot plays plugin or hardware, `InputRole::Instrument` input route, latency look-ahead for the sequencer and arp.
- Studio engine: synth profile format and the bundled profiles.
- App: Jam tree entries, setup sheet, macros and panel controls from the profile, presets.
