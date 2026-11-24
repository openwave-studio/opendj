## ADDED Requirements

### Requirement: Pitch Color FX
The Color FX SHALL include Pitch. With Pitch chosen, a deck's COLOR (filter) knob SHALL shift the deck's sound down when turned left and up when turned right, up to 12 semitones at either end, keeping its tempo, and SHALL leave the sound exactly as it is in the middle. Changes in the knob SHALL glide without clicks, and the shifted sound SHALL keep about the level of the dry sound.

#### Scenario: An octave down
- **WHEN** Pitch is the deck's Color FX and its COLOR knob is turned all the way left
- **THEN** the deck plays an octave lower at the same tempo

#### Scenario: Back in the middle
- **WHEN** the COLOR knob comes back to the middle
- **THEN** within a quarter of a second the deck plays exactly as without the effect
