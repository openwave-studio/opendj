## ADDED Requirements

### Requirement: Window size
The window SHALL be at least 1280 × 720. On a short window the waveforms SHALL step down to keep about five library rows; below that 4-Deck's pads and then the FX bar SHALL fold, and pressing PADS or FX SHALL show them anyway. Settings › Appearance SHALL offer an interface scale from 85% to 125%, using the largest step that fits when a larger one would not, and a first start on a screen with less than 1280 × 720 usable SHALL pick 90%.

#### Scenario: 13-inch laptop
- **WHEN** 4-Deck runs in a 1440 × 875 window
- **THEN** the library shows at least five rows

#### Scenario: Small Windows laptop
- **WHEN** openDJ first starts on a 1920 × 1080 screen at 150%
- **THEN** the scale is 90% and the window fits with nothing clipped

### Requirement: List text
Settings › Appearance SHALL offer List text (Small, Normal, Large, Extra large) for the track list, the lists panel and the side panel's lists, with rows growing to match.

#### Scenario: Larger list text
- **WHEN** List text is set to Extra large
- **THEN** the track list's text and rows are about a third larger and nothing in a row is clipped
