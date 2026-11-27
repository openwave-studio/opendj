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

### Requirement: Hardware synth presets by bank or category
The preset picker SHALL group a hardware synth's presets by its named banks, or by its own categories when its profile has them, with a search by name; the deck's ‹ › SHALL step through the picked group only. When the profile says how, openDJ SHALL read the preset names and categories from the synth itself, keep them, and read them again on request.

#### Scenario: Pick a pad on the MicroFreak
- **WHEN** the MicroFreak loads for the first time
- **THEN** openDJ reads its 512 preset names and categories in the background
- **AND** picking Pad in the picker lists only its pads, factory and the user's own, and ‹ › step through them

#### Scenario: Pick a preset in a named bank
- **WHEN** the user picks the Messenger's Classic Bass bank and its third preset
- **THEN** the synth gets Bank Select and Program Change for it and the deck bar reads "Classic Bass › 3"
