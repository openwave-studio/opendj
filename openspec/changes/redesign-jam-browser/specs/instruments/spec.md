## ADDED Requirements

### Requirement: Jam browser column
The Jam source SHALL list instruments and presets in one narrow column with, from the top, a search field and a starred-only button, Instruments and Presets buttons, filter menus and a list. Presets SHALL list every preset of every instrument; Instruments SHALL list one row per instrument.

#### Scenario: Finding a warm pad
- **WHEN** Presets is shown, Pad is picked under Sound type and Warm under Character
- **THEN** the list shows only presets tagged Pad and Warm, from every instrument, with the count above it

### Requirement: Filter menus
The filter menus SHALL be, in order, Sound type, Genre, Style, Character, Instrument and Maker, each folding open as a group of chips. A closed menu SHALL show the chips picked in it on its header and nothing else. Each menu SHALL offer only values that still match the other picks and the search.

#### Scenario: Closed menu with picks
- **WHEN** House and Ambient are picked under Genre and the menu is closed
- **THEN** its header shows House and Ambient as chips

#### Scenario: Closed menu with no picks
- **WHEN** nothing is picked under Style and the menu is closed
- **THEN** its header shows only its name

### Requirement: Loading from the column
Clicking a row SHALL load it on the deck the plugin panel shows, or the selected deck when no panel shows; double-clicking SHALL load it on a free deck; dragging it onto a deck SHALL load it there.

#### Scenario: Swapping the panel's preset
- **WHEN** the plugin panel shows deck A's instrument and a preset row is clicked
- **THEN** deck A plays that preset and the panel shows it
