## ADDED Requirements

### Requirement: MIDI Fighter Twister Remix mapping
openDJ SHALL bundle a MIDI Fighter Twister mapping whose first bank plays the first deck with Remix loaded: row 1 ISOLATE LOW, MID, HIGH and SCENE; row 2 LEVER LOW, MID, HIGH (hold the push; a double press latches) and RELEASE (push; turn picks the mode); row 3 the four sample pads (push plays; hold and turn sets the roll rate); row 4 the Lever effect, Isolate effect, sample effect and the Remix deck's fader. Side buttons SHALL be SHIFT (TIME per band, COLOR, pitch per pad, release time), OVERDUB, UNDO, SOURCE (steps the source) and bank back and forward; the next bank SHALL keep the deck controls. Rings SHALL show each control's value in its band colour (red low, green mid, blue high, lilac for the rest).

#### Scenario: Cut the lows from the Twister
- **WHEN** the top-left encoder turns fully left
- **THEN** the source's lows are silent and its ring shows the cut

#### Scenario: Roll from the Twister
- **WHEN** the KICK encoder is held down and turned
- **THEN** the kick rolls at the rate the ring shows until the encoder is let go
