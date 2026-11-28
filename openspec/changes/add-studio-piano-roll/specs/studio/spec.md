## ADDED Requirements

### Requirement: Piano roll
openDJ Studio SHALL open a MIDI clip's notes in a piano roll in the bottom panel when the clip is double-clicked, with the clip's loop, a keyboard, a grid and its notes. Notes SHALL be drawn, selected, moved, resized, duplicated, copied, pasted, deleted and deactivated there, with undo and redo, and every change SHALL be heard the next time the clip plays the edited beat, without a click while it plays.

#### Scenario: Drawing a note
- **WHEN** the grid is 1/16 and the user draws on an empty row at beat 2.1
- **THEN** a 1/16 note appears at beat 2 on that pitch and plays the next time the loop passes beat 2

#### Scenario: Moving keeps the groove
- **WHEN** a note 10 ms after beat 3 is dragged right by about one 1/16 step with snap on
- **THEN** it lands 10 ms after the next 1/16 line; with Shift held it lands exactly on the line

### Requirement: Grid and snap
The piano roll SHALL offer Snap to Grid, an adaptive grid that follows the zoom (Widest, Wide, Medium, Narrow, Narrowest), a fixed grid from 8 bars to 1/32, Narrow and Widen Grid and a triplet grid, from a grid button at its top right and from its right-click menu, with Live's shortcuts.

#### Scenario: Triplets
- **WHEN** the grid is 1/16 and Triplet Grid is turned on
- **THEN** the grid lines and snapping fall on sixteenth-note triplets and the button reads 1/16T

### Requirement: Scale and folding
The piano roll SHALL highlight the set's scale (rows outside it darker, the root marked) and SHALL fold the rows to the scale or to the pitches the clip uses.

#### Scenario: Folding to the scale
- **WHEN** the set is in C minor and Fold is set to Scale
- **THEN** only the rows for C, D, E♭, F, G, A♭ and B♭ are shown, and moving a note up one row moves it one scale step

### Requirement: Ghost notes
The piano roll SHALL show, on request, the notes of the track's other clips (Track), and also the clips in the same scene on every other MIDI track (All), as outlines in their track's colour that cannot be selected.

#### Scenario: Writing over the chords
- **WHEN** Ghost notes is set to All while editing a lead clip
- **THEN** the chord track's notes show as outlines under the lead's notes

### Requirement: Note lanes
The piano roll SHALL show a velocity lane and a chance lane under the notes; dragging a bar SHALL set that note's value, and drawing across several bars SHALL set a ramp.

#### Scenario: A velocity ramp
- **WHEN** the user draws a line in the velocity lane from low on bar 1 to high on bar 2
- **THEN** the notes under the line take velocities rising along it

### Requirement: Note tools
The piano roll SHALL offer Quantize (with settings for grid, start or end, amount and swing), Legato, Transpose, Reverse, double and half length, Chop Notes on Grid, Join Notes, Duplicate and Invert Selection, from the clip column and from a right-click menu with Live's names and shortcuts.

#### Scenario: Half quantize
- **WHEN** a note 40 ms late is quantized to 1/16 at 50 %
- **THEN** it ends 20 ms late

### Requirement: Drum mode
On a track whose instrument is a Drum Rack, the piano roll's rows SHALL be the rack's pads by name, and folding SHALL show only the pads with notes.

#### Scenario: Pad names
- **WHEN** a clip on a Drum Rack track is opened
- **THEN** the rows read Kick, Snare, Closed hat and so on instead of note names
