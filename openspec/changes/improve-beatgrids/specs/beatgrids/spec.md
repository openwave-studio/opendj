## ADDED Requirements

### Requirement: Edit on the waveform
A grid edit mode SHALL let the DJ drag a beat marker to move the grid, tap to set the tempo, click to set the downbeat, halve or double the tempo, add or remove a tempo change, and undo.

#### Scenario: Set downbeat
- **WHEN** the DJ clicks a beat in edit mode and chooses Downbeat
- **THEN** bar 1 starts there

### Requirement: Saved grids
An edited grid SHALL be saved with the track and marked as edited; the DJ SHALL be able to lock it, and to go back to the analysed grid.

#### Scenario: Back to analysed
- **WHEN** the DJ chooses Use Analysed Grid on an edited track
- **THEN** the analysed grid comes back and the track is no longer marked as edited

### Requirement: Grid lock
A locked grid SHALL never be changed by analysis or an import.

#### Scenario: Locked
- **WHEN** a locked track is re-analysed
- **THEN** its grid stays

### Requirement: Unsure warning
Tracks whose beats drift from the grid SHALL be marked unsure on the deck and searchable as `is:gridunsure`.

#### Scenario: Drifting
- **WHEN** a track drifts over 10 ms from its grid
- **THEN** it is marked unsure

### Requirement: Grid modes
Each track SHALL have a grid mode: Auto (one tempo unless the beats drift for a while), Fixed (one tempo for the whole track) or Flexible (the grid follows the beats bar by bar). Tracks without one SHALL use the mode set in Settings, Auto by default. Changing the mode SHALL re-fit the grid from the beats analysis already found, without analysing the track again, and SHALL leave a locked grid alone.

#### Scenario: Force one tempo
- **WHEN** the DJ sets a track whose grid has tempo sections to Fixed
- **THEN** its grid has one tempo and the track's mode is saved as Fixed

#### Scenario: Follow a drummer
- **WHEN** a live track drifts a few BPM over 16 bars and its mode is Flexible
- **THEN** its grid gets new tempo sections there, while Auto keeps one tempo

#### Scenario: Locked
- **WHEN** the DJ changes the mode of a track whose grid is locked
- **THEN** the grid stays as it is

### Requirement: Tempo labels
The big waveforms SHALL label the tempo, in whole BPM, where it changes, only when the whole number changes and at least 16 bars after the last label; edit mode SHALL show every tempo change. A label within 2 beats of a phrase flag SHALL join that flag. The DJ SHALL be able to hide the labels.

#### Scenario: Small wobble
- **WHEN** a Flexible grid moves between 120.2 and 120.4 BPM
- **THEN** no label is added
