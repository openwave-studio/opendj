## ADDED Requirements

### Requirement: Plugin panel
While an instrument plugin plays on a deck, the Jam source SHALL be able to show it in a panel in the track list's and side panel's place, keeping the rail and the Jam tree. The panel SHALL show the plugin's own display, when it has one, at the display's own width over height and as large as the panel allows, passing clicks and drags to it; otherwise it SHALL show the plugin's own editor over the panel's area (a borderless window that follows the panel), or, when the user picks CONTROLS or the plugin has none, its parameters as sliders grouped by the plugin's pages. Its bar SHALL name the plugin, its format and its deck, open the plugin's own window, and close the panel back to the list. Plugin effects SHALL keep opening in their own window.

#### Scenario: A Pd display at its shape
- **WHEN** a deck plays a Pd patch whose Lua display is 1000 by 445 and the user opens the plugin panel
- **THEN** the display fills the panel's height at 2.25 to 1 with no bars beside it, and dragging on it moves the patch's controls

#### Scenario: A plugin's own editor in the panel
- **WHEN** a deck plays a VST3 synth and the user opens the plugin panel
- **THEN** the synth's own editor shows over the panel's area with no window frame, and moves with openDJ's window

#### Scenario: Sliders instead
- **WHEN** the user picks CONTROLS in the panel, or the plugin has no editor
- **THEN** the panel lists the plugin's parameters as sliders by page, and the expand button opens the plugin's own window

#### Scenario: From the deck
- **WHEN** the user clicks DISPLAY on an instrument deck
- **THEN** the browser shows the Jam source with that instrument's panel, and the deck keeps its knobs
