## MODIFIED Requirements

### Requirement: Tree
The tree SHALL start with the stages (when set up), then All Tracks with Collection Health beneath, then the DJ's playlists, folders and smart lists, imported folders, and History, each with its count.
The DJ's lists and folders SHALL keep a free order. A list dragged in the tree SHALL show where it lands: a lilac line, indented to the level it lands at, for before or after a row, and a lilac outline for into a folder or section heading. A list dropped into a folder SHALL go first in it. A closed folder SHALL open after a drag hovers over it for 0.7 s, and the tree SHALL scroll while a drag is near its top or bottom edge. After a move the list SHALL be selected with its folder open. The list menu SHALL offer Move to… and Move Up / Move Down (⌥⌘↑ / ⌥⌘↓).

#### Scenario: Stages first
- **WHEN** triage and collection folders are set
- **THEN** the tree starts with Inbox, Triage and Collection

#### Scenario: Put a list between two others
- **WHEN** a playlist is dragged to the lower edge of "Peak hour" inside folder "Gigs" and dropped
- **THEN** it sits right after "Peak hour" in "Gigs", stays selected, and keeps that place after a restart

#### Scenario: Reach a closed folder
- **WHEN** a list is dragged over the closed folder "Radio" and held there
- **THEN** "Radio" opens so the list can be dropped beside one of its lists

### Requirement: Drag and drop
Rows SHALL drag onto decks, lists, stages, the Automix and Sidelist tabs; a deck's cover SHALL drag out onto lists.
Tracks dropped on a list SHALL be added and stay in the list they came from; with ⌥ (Alt) held they SHALL be moved, leaving that list. The drag SHALL say "Add N tracks" or "Move N tracks". After the drop a note SHALL say what happened, with Undo. Tracks already in a playlist SHALL be skipped unless the DJ picks Add them again.

#### Scenario: Drag to deck
- **WHEN** a row is dropped on deck 2
- **THEN** it loads on deck 2

#### Scenario: Move between lists
- **WHEN** five tracks of "Warm-up" are dragged onto "Peak hour" with ⌥ held
- **THEN** "Peak hour" ends with them, "Warm-up" no longer has them, and the note offers Undo

### Requirement: Side panel
The side panel SHALL have icon tabs Automix, Sidelist, Linked and Info, with a lilac underline on the open one.
Info's preview bar SHALL show the track's waveform (the same overview as the Waveform column), gray with the part played in lilac and a white playhead, and click or drag to seek; a track not yet analysed SHALL keep the plain line.
The Sidelist SHALL show the Scratch pad or any of the DJ's playlists or crates, picked from a menu at its top, by dropping a list on its tab, or with Show in Sidelist. A list shown there SHALL take drops at an insertion line, reorder by drag and remove with Delete, saving at once. The Scratch pad SHALL be kept across restarts and can be saved as a playlist.

#### Scenario: Open Info
- **WHEN** the Info tab icon is clicked
- **THEN** the selected track's details show

#### Scenario: Build a list while browsing
- **WHEN** "Saturday warm-up" is shown in the Sidelist and two tracks from a crate are dropped between its second and third rows
- **THEN** the playlist holds them there and the crate is unchanged
