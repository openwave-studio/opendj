## ADDED Requirements

### Requirement: Serato import
Settings › Collection › Import SHALL offer Import from Serato DJ, read-only, with the shared wizard, kept up to date at start, and removable.

#### Scenario: Serato library
- **WHEN** a Serato DJ library is imported
- **THEN** tracks keep their hot cues with colors and names, saved loops, beat grid and track color from the files' Serato tags, and Serato crates appear as lists nested as their subcrates are

#### Scenario: Library on an external drive
- **WHEN** an external drive has its own `_Serato_` folder
- **THEN** its tracks and crates come across with the rest

#### Scenario: Older files
- **WHEN** a file only has the older `Serato Markers_` tag
- **THEN** its hot cues and loops still come across

### Requirement: OneLibrary import
Settings › Collection › Import SHALL offer Import from a OneLibrary stick or folder, read-only, with the shared wizard.

#### Scenario: Stick from another app
- **WHEN** the DJ picks a USB stick with `exportLibrary.db` written by rekordbox, djay, Traktor or VirtualDJ
- **THEN** its tracks keep their hot and memory cues, loops, beat grids, phrases, rating and color, and its playlists and history appear as lists

#### Scenario: Older stick
- **WHEN** the stick has only `export.pdb`
- **THEN** the same tracks, cues, grids and playlists come across
