## ADDED Requirements

### Requirement: Instrument step sequencer
With its keyboard hidden, a 2-Deck instrument deck SHALL show a step sequencer of 16 steps, one note per step, with lanes for pitch, octave, velocity, gate and chance, each with its own length, and accent and slide per step. Pitches SHALL snap to the deck's scale. The sequencer SHALL start on the next bar of the mix's beat, and its notes SHALL go through the deck's Scale, Chord, Arp and HOLD as played keys do. A deck SHALL keep four patterns (A to D), saved with the instrument's preset.

#### Scenario: Play a riff in time
- **WHEN** the deck's keyboard is hidden, a pattern has notes and the user presses ▶ while the master deck plays
- **THEN** the pattern starts on the next bar and plays the instrument on the beat

#### Scenario: Notes stay in key
- **WHEN** SCALE is on in A minor and the user drags a step's pitch bar
- **THEN** the step only takes notes of A minor

#### Scenario: Lanes of different lengths
- **WHEN** the pitch lane is 16 steps and the accent lane 12
- **THEN** the accents shift against the notes each bar and come back in line after three bars
