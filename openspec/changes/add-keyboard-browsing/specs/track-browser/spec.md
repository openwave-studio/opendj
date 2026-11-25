## ADDED Requirements

### Requirement: Keyboard browsing
The browser SHALL have three panes that take the keys in turn: the source rail, the folder tree and the track list (in Jam, its tree and its instruments). The pane with the keys SHALL show the bright selection and the others a dim one. ↑ and ↓ SHALL move in the pane with the keys. Tab and Shift+Tab SHALL move the keys to the next or previous pane that shows. In the tree, → SHALL open a closed folder, step into an open one, and on a row without children give the track list the keys; ← SHALL close an open folder, step to the parent, and from a top-level row give the rail the keys. In the track list ← SHALL give the tree the keys. Shift+← and Shift+→ SHALL load the picked track onto deck 1 and deck 2. Esc SHALL hand the keys back to the decks, where ← and → bend the deck started last; W/E and U/I SHALL bend deck 1 and deck 2 while held. In the Prep layout Tab and ← → SHALL keep their Prep jobs. A controller's browse turn SHALL move in the pane with the keys, its press SHALL step right as → does (from the track list, back to the tree) and its Shift+press SHALL step left as ← does, except in the tree, where it SHALL close an open folder and otherwise give the rail the keys at once.

#### Scenario: Into a playlist without the mouse
- **WHEN** the tree has the keys on the closed folder "Gigs" and the DJ presses →, →, → and then ↓
- **THEN** "Gigs" opens, its first playlist is selected, the track list gets the keys, and its second track is selected

#### Scenario: Back to the rail
- **WHEN** the track list has the keys inside a top-level playlist and the DJ presses ← twice
- **THEN** the tree gets the keys and then the rail, and ↓ picks the next source in the rail

#### Scenario: Load from the keyboard
- **WHEN** a track is picked in the list and the DJ presses Shift+→
- **THEN** it loads onto deck 2

#### Scenario: Controller with only a press
- **WHEN** a mapping has BROWSE press but no Shift+press, and the track list has the keys
- **THEN** a press gives the tree the keys
