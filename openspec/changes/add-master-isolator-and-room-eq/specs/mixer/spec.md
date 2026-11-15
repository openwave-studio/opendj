## ADDED Requirements

### Requirement: Master Isolator
The master SHALL have a three-band Isolator (`[Master],iso_low`, `iso_mid`, `iso_high`) over the whole mix: flat at the centre, the band silent fully left and up to +6 dB fully right, with no clicks while a knob turns. It SHALL act on the main, booth and headphone mix and on recordings, before the master limiter. While any band is off centre, the MASTER button in the top bar SHALL show an ISO tag. A flat Isolator SHALL cost nothing.

#### Scenario: Low out for a breakdown
- **WHEN** the DJ turns the Isolator's LOW fully left with two decks playing
- **THEN** the bass of both decks is gone from the main output, the booth and the recording, and the ISO tag lights

#### Scenario: Flat
- **WHEN** the DJ presses Flat in the MASTER panel
- **THEN** all three bands return to the centre and the ISO tag goes out

### Requirement: Room EQ
Settings › Audio SHALL have a Room EQ for the main output and one for the booth, each with an On switch and six bands: low cut, low shelf, two bells, high shelf and high cut. Each SHALL be drawn as a curve on a 20 Hz to 20 kHz axis with a dot per band: dragging a dot sets frequency and gain, scrolling on it sets its width or slope, and double-clicking resets it. Changing a band SHALL NOT click or zip.

#### Scenario: Reset a band
- **WHEN** the DJ double-clicks the dot of a bell at −3 dB
- **THEN** the bell returns to 0 dB and the curve is flat there

### Requirement: Room EQ placement
Room EQ SHALL sit before the master limiter on the main output and before the soft clip on the booth. It SHALL NOT reach the headphones, recordings, broadcasts or the loudness meter, which take the mix after the Isolator.

#### Scenario: Muddy room
- **WHEN** the DJ pulls the main curve's 250 Hz bell down 3 dB
- **THEN** the main output has less 250 Hz, while the recording and the headphones are unchanged

#### Scenario: Booth only
- **WHEN** the booth curve has its high shelf at −4 dB
- **THEN** only the booth output is darker

### Requirement: Room EQ presets
Room EQ SHALL offer the presets Flat, Muddy room, Harsh highs and Small speakers, and SHALL save the DJ's own presets by name. A preset that has been changed and not saved SHALL be marked. The MASTER panel SHALL show the main output's preset with a link to edit it.

#### Scenario: Own venue
- **WHEN** the DJ saves the main curve as "Main room" and later picks it again
- **THEN** the curve comes back exactly and the MASTER panel shows "Main room"

### Requirement: Booth EQ
While a booth output is set up, the MASTER panel SHALL show BOOTH EQ LOW and HIGH knobs that set the booth Room EQ's low and high shelves, so the knobs and the curve always agree.

#### Scenario: Knob and curve agree
- **WHEN** the DJ turns BOOTH EQ LOW down 3 dB
- **THEN** the booth curve's low shelf shows −3 dB and the booth Room EQ is on

## MODIFIED Requirements

### Requirement: MASTER panel
The centre mixer SHALL have no tabs: the channels and the crossfader only, with the middle of its top row free for the effects. A click on MASTER in the top bar SHALL open a panel with horizontal sliders for Master, Phones, Booth (only while a booth output is set up) and Cue / Mix, plus Ableton Link, the master Isolator with a Flat button, BOOTH EQ (only while a booth output is set up) and the main Room EQ preset with Edit…; a click outside closes it. Cue / Mix SHALL have Cue on the left, Mix on the right and a detent at the centre. Audio routing SHALL live only in Settings › Audio.

#### Scenario: Cue / Mix detent
- **WHEN** the Cue / Mix slider is dragged to within 3 % of the centre
- **THEN** it rests at the centre

#### Scenario: No booth output
- **WHEN** no output route has the booth role
- **THEN** the MASTER panel shows neither the Booth slider nor BOOTH EQ
