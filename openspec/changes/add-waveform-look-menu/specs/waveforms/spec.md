## ADDED Requirements

### Requirement: Waveform look from the waveforms button
The waveforms button's drop-down SHALL hold, in order, the waveform height as a slider, Type (Full or Half), Color (RGB, 3-Band or Stems, with 3-Band's palette under it), Show vocals (hidden while Stems is picked), Needle (Center or Left) and Show lyrics, with Full, RGB, Center, vocals off and lyrics on the defaults. Each choice SHALL show on the decks at once and be saved, and the drop-down SHALL stay open after a choice. Settings › Appearance SHALL NOT list Type, Color, Palette, Show vocals or the lyrics switch. A saved choice of the dropped Blue scheme SHALL come back as RGB.

#### Scenario: Live height
- **WHEN** the DJ drags the height slider from 3 towards 7 and lets go nearer 7
- **THEN** the waveforms grow with the slider while it moves and settle on height 7

#### Scenario: Old Blue setting
- **WHEN** openDJ starts with the Blue scheme saved
- **THEN** the waveforms are RGB and the drop-down shows RGB

### Requirement: Half large waveforms
With Type set to Half, the large scrolling waveforms and the overviews SHALL be drawn from the bottom edge up rather than mirrored about the middle. A live deck showing lanes of notes, steps or knobs SHALL stay mirrored.

#### Scenario: Half
- **WHEN** the DJ picks Half
- **THEN** both the large waveforms and the overviews grow from their bottom edge

### Requirement: Needle position
With Needle set to Left, the large scrolling waveforms' play line SHALL sit a quarter of the way in from the left, and the beat grid, cues, loops, lyrics, knob labels and live decks' lanes SHALL be drawn relative to it. With Center, they SHALL look as before.

#### Scenario: Left needle
- **WHEN** the DJ picks Left
- **THEN** the play line moves to a quarter of the width and three quarters of the waveform shows what is coming

### Requirement: Size follows gain on the large waveforms only
Size follows gain SHALL be on by default and SHALL scale only the large scrolling waveforms with the deck's gain knob; the overviews SHALL keep their height.

#### Scenario: Gain up
- **WHEN** the DJ turns a deck's gain knob up with Size follows gain on
- **THEN** that deck's large waveform grows and its overview stays the same height
