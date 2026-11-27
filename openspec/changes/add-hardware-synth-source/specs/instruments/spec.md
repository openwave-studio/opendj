## ADDED Requirements

### Requirement: Hardware synth on the instrument deck
The instrument deck SHALL be able to play a hardware synth instead of a plugin. Every note the deck plays (keys, pads, Note FX, arpeggiator, step sequencer) SHALL go to the synth's MIDI output and channel, and the macro knobs SHALL send the synth's control changes. The synth's sound SHALL come in on a chosen audio input and play as the instrument's sound, through the deck's scope, effects and mixer channel.

#### Scenario: Play the synth from the deck
- **WHEN** a hardware synth is loaded on the instrument deck and the user plays a key on the deck's keyboard
- **THEN** the synth receives the note on its channel and its sound is heard through the instrument's channel

#### Scenario: Turn a macro
- **WHEN** the user turns a macro knob mapped to the synth's filter cutoff
- **THEN** the synth receives that control change with the knob's value

### Requirement: Hardware synth latency
The deck SHALL take a hardware latency of 0 to 200 ms for a hardware synth, and SHALL send sequenced and arpeggiated notes that much early, so the synth's sound lands on the beat.

#### Scenario: Sequencer on the beat
- **WHEN** the latency is 12 ms and the step sequencer plays a step on a downbeat
- **THEN** the note goes out 12 ms before the downbeat is heard
