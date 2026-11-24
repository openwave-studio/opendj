## Purpose

Playing and shaping the instrument deck from a keyboard controller: its knobs, transport, preset and browse controls and its screen, the same on every keyboard.

## ADDED Requirements

### Requirement: Keys play over the keyboard's MIDI port
A keyboard whose buttons, knobs and screen run an HID mapping marked `keys="midi"` SHALL keep its USB MIDI port playing the instrument deck (notes, octave shifts, pitch bend, modulation, pedal) as any keyboard does. Its HID side SHALL take the mapping's name so the two never share one.

#### Scenario: Plug in an M32
- **WHEN** a Komplete Kontrol M32 is plugged in and deck 2 plays an instrument
- **THEN** its keys play deck 2's instrument and its HID mapping runs the knobs and screen

### Requirement: Knobs turn the instrument's macros
Knobs 1-8 SHALL turn the instrument's knobs on the page the deck shows (`[Instrument1],knob_page`, kept in step with the screen both ways), as endless encoders, more finely with SHIFT. Page buttons SHALL turn the page within the instrument's knobs.

#### Scenario: Second page
- **WHEN** the page shown is 2 and knob 1 is turned up
- **THEN** the instrument's knob 9 goes up and the deck shows it move

### Requirement: Transport runs the instrument deck
PLAY SHALL start the instrument deck's step sequencer and STOP SHALL stop it. REC SHALL start or stop recording the set; with SHIFT, recording into the sequencer. METRO SHALL turn the instrument deck's metronome on or off and TEMPO SHALL tap its tempo.

#### Scenario: Start the sequencer
- **WHEN** PLAY is pressed on the keyboard
- **THEN** the instrument deck's sequencer plays, and PLAY lights

### Requirement: Presets and instruments from the keyboard
PRESET up and down SHALL load the previous and next preset of the instrument playing, in name order, round the ends. The browse encoder SHALL list the instruments, then the presets of the instrument opened; pressing it SHALL open the instrument picked (playing it on the instrument deck, or the first free deck) or load the preset picked, and SHIFT + press SHALL go back a level.

#### Scenario: Browse to a preset
- **WHEN** the encoder is turned to the second instrument and pressed, then turned to its third preset and pressed
- **THEN** the instrument deck plays the second instrument with its third preset

### Requirement: Arpeggiator and scale buttons
ARP SHALL turn openDJ's arpeggiator on or off (SHIFT: hold) and SCALE SHALL turn the scale between Off and Auto.

#### Scenario: Arpeggiate
- **WHEN** ARP is pressed and a chord is held
- **THEN** the instrument plays the chord arpeggiated, and ARP lights

### Requirement: Screens show what plays
A keyboard with a screen SHALL show the instrument and its preset; while a knob turns, its name and value; while browsing, the list around the item picked. Scripts SHALL read these words with `engine.getText(group, key)`.

#### Scenario: Turn a knob
- **WHEN** knob 3 is turned
- **THEN** the screen shows the knob's name and value for a moment, then the instrument and preset again
