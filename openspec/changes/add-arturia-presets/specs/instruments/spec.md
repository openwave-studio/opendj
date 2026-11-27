## ADDED Requirements

### Requirement: Maker preset libraries
When an Arturia instrument is listed, openDJ SHALL read Arturia's preset library, read only and off the main thread, and SHALL list each preset of that instrument's folder (for Analog Lab also those made for it) as the plugin's own preset, after the user's. A preset SHALL carry the library's type as a Sound type, its genres and styles as Genre and Style, and the styles and characteristics that match openDJ's Character tags; a preset liked in the library SHALL be starred once. Loading one SHALL give the plugin the preset file as its saved state (VST3: both state parts; Audio Unit: the class info's controller state). The library SHALL be read again only when it changed.

#### Scenario: Presets with tags
- **WHEN** Mini V4 is ticked in Settings › Plugins and Arturia's library lists 162 presets in its folder
- **THEN** the Jam column's Presets lists them under Mini V4, and a Genre menu lists their genres

#### Scenario: Loading one
- **WHEN** the user picks the Mini V4 preset "Kick"
- **THEN** the instrument deck plays Mini V4 with that sound, and the plugin's window shows "Kick"

#### Scenario: Saving keeps a copy
- **WHEN** the user tweaks an Arturia preset and presses SAVE
- **THEN** openDJ keeps the sound as the user's own preset, and Arturia's library is left as it was
