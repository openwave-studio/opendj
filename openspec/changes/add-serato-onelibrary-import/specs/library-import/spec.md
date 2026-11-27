## ADDED Requirements

### Requirement: Serato import
Settings › Collection › Import SHALL offer Import from Serato DJ for libraries from Serato DJ Pro 4 and 5, read-only, with the shared wizard, kept up to date at start, and removable. It SHALL never write Serato's library, its `_Serato_` folders or the audio files' tags.

#### Scenario: Serato 5 library
- **WHEN** a Serato DJ Pro 5 library is imported
- **THEN** tracks keep their hot cues with colors and names, saved loops, beat grid and track color from the files' Serato tags, Serato crates appear as lists nested as their subcrates are, and Serato's History sessions appear as History days

#### Scenario: Serato open while importing
- **WHEN** Serato DJ is running during the import
- **THEN** openDJ reads a copy of Serato's library and Serato is not disturbed

#### Scenario: Library on an external drive
- **WHEN** an external drive has its own `_Serato_` folder
- **THEN** its tracks and crates come across with the rest

#### Scenario: Library openDJ can't open
- **WHEN** `master.sqlite` is missing or its layout isn't recognised
- **THEN** tracks and crates come from the `database V2` and crate files Serato writes next to it, and the preview line says History is not included

#### Scenario: Cue loops and tempo changes
- **WHEN** a track has a Serato 5 Cue Loop or a Lucid Beatgrid with tempo changes
- **THEN** the cue loop becomes a saved loop on the same pad, and the grid keeps each tempo change, locked against analysis

#### Scenario: Smart crates
- **WHEN** the DJ turns on Serato's smart crates in the wizard
- **THEN** a smart crate whose rules openDJ's search can express becomes a smart list, and any other comes in as a list of the tracks it last held

#### Scenario: Older Serato
- **WHEN** the library was never opened in Serato DJ Pro 4 or 5
- **THEN** the wizard says to open it once in Serato 4 or newer and import again

### Requirement: OneLibrary import
Settings › Collection › Import SHALL offer Import from a OneLibrary stick or folder, read-only, with the shared wizard.

#### Scenario: Stick from another app
- **WHEN** the DJ picks a USB stick with `exportLibrary.db` written by rekordbox, djay, Traktor or VirtualDJ
- **THEN** its tracks keep their hot and memory cues, loops, beat grids, phrases, rating and color, and its playlists and history appear as lists

#### Scenario: Older stick
- **WHEN** the stick has only `export.pdb`
- **THEN** the same tracks, cues, grids and playlists come across
