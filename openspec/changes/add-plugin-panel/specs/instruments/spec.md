## ADDED Requirements

### Requirement: Plugin panel
While an instrument plugin plays on a deck, the Jam source SHALL be able to show it in a panel in the track list's and side panel's place, keeping the rail and the Jam tree. The panel SHALL show the plugin's own display, when it has one, at the display's own width over height and as large as the panel allows, passing clicks and drags to it; otherwise it SHALL show every parameter as a slider, grouped by the plugin's pages. Its bar SHALL name the plugin, its format and its deck, open the plugin's own window, and close the panel back to the list. Plugin effects SHALL keep opening in their own window.

#### Scenario: A Pd display at its shape
- **WHEN** a deck plays a Pd patch whose Lua display is 1000 by 445 and the user opens the plugin panel
- **THEN** the display fills the panel's height at 2.25 to 1 with no bars beside it, and dragging on it moves the patch's controls

#### Scenario: A plugin with no display
- **WHEN** a deck plays a VST3 synth and the user opens the plugin panel
- **THEN** the panel lists the synth's parameters as sliders by page, and the expand button opens the synth's own window

#### Scenario: From the deck
- **WHEN** the user clicks DISPLAY on an instrument deck
- **THEN** the browser shows the Jam source with that instrument's panel, and the deck keeps its knobs
