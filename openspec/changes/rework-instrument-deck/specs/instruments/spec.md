## MODIFIED Requirements

### Requirement: Instrument deck
A deck playing an instrument plugin SHALL show a slim title bar (a ☰ deck menu, the plugin's name, its format, the preset with ‹ › and a save heart, MIDI from, a button to open the plugin's window, and unload) and under it an in/out box (scope, level meter, TRIM, PAN), the plugin's knobs 8 to a page under named page tabs, the Note FX controls (Scale, Chord, Arp, Hold) and a full-width on-screen keyboard that lights the notes being played. The keyboard SHALL come in three sizes (Full, Slim, Hidden); with Slim or Hidden the deck SHALL show a second page of knobs. The deck SHALL show no artwork and no hint text. In the 4-Deck layout it SHALL show 4 knobs and no keyboard. Instrument and input decks SHALL default to Thru on the crossfader.

#### Scenario: Save the sound with the heart
- **WHEN** a deck plays Acid V with an unsaved sound and the user clicks the empty heart and names the sound "Squelch Bass"
- **THEN** a preset "Squelch Bass" is saved, the title shows its name and the heart is filled

#### Scenario: A knob turn marks the preset changed
- **WHEN** a deck plays the preset "Squelch Bass" and the user turns a knob
- **THEN** the title reads "Squelch Bass*" and the heart shows empty until the sound is saved again

#### Scenario: A plugin's own preset is copied, not overwritten
- **WHEN** a deck plays one of the plugin's own presets, a knob is turned and the heart is clicked
- **THEN** openDJ asks a name and saves the sound as a new preset of the user's, leaving the plugin's preset as it was

#### Scenario: Slim keyboard shows more knobs
- **WHEN** the user picks ☰ › Keyboard › Slim on a deck playing a plugin with 16 or more knobs
- **THEN** the keyboard shrinks to a strip that still lights played notes and the deck shows two pages of knobs

## ADDED Requirements

### Requirement: Deck menu
Instrument, Mixer and Groove Box decks SHALL start their title bar with the same ☰ menu. On an instrument deck it SHALL offer New preset, Save preset, Save preset as, Rename, Tags, Keys on pads, Keyboard size, Open plugin window and Unload deck; on Mixer and Groove Box decks at least Unload deck.

#### Scenario: Keys on pads from the menu
- **WHEN** the user picks ☰ › Keys on pads on an instrument deck
- **THEN** the deck's pad controller plays the instrument's notes, as the former KEYS ON PADS button did

### Requirement: Hold
The instrument deck SHALL offer HOLD: while on, notes played keep sounding after their keys are let go, until played again or HOLD is switched off. With the arpeggiator on, HOLD SHALL be the arpeggiator's hold.

#### Scenario: Hold a pad
- **WHEN** HOLD is on, the arpeggiator is off and the user plays and releases a chord
- **THEN** the chord keeps sounding until HOLD is switched off
