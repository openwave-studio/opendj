## MODIFIED Requirements

### Requirement: Side panel
The side panel SHALL have icon tabs Automix, Sidelist, Linked and Info, with a lilac underline on the open one.
Info's preview bar SHALL show the track's waveform (the same overview as the Waveform column), gray with the part played in lilac and a white playhead, and click or drag to seek; a track not yet analysed SHALL keep the plain line.
On a window under 1440 wide the side panel SHALL slide in over the track list instead of taking width from it. Its edge SHALL drag from 280 to 480 px, kept per layout. A track dragged to the window's right edge SHALL slide it open.

#### Scenario: Open Info
- **WHEN** the Info tab icon is clicked
- **THEN** the selected track's details show

#### Scenario: Narrow window
- **WHEN** the window is 1280 wide and the footer's side panel button is pressed
- **THEN** the side panel slides in over the track list, which keeps its columns

## ADDED Requirements

### Requirement: Lists panel
Clicking the open source's rail icon SHALL fold the lists panel into the rail and unfold it again. Folded, a click on a rail icon SHALL slide the panel out over the track list, and it SHALL slide back after a list is picked, on Escape or on a click in the track list. On a window under 1440 wide it SHALL start folded, and come back beside the track list when the window grows unless the DJ folded it. Its edge SHALL drag from 200 to 360 px, kept per layout.

#### Scenario: Fold the lists
- **WHEN** the Collection rail icon is clicked while the Collection is open
- **THEN** the lists panel folds and the track list takes its width

#### Scenario: Pick a list while folded
- **WHEN** the panel is folded, the Collection icon is clicked and "Warm-up" is picked
- **THEN** the panel slides out, Warm-up opens, and the panel slides back
