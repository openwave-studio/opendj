## MODIFIED Requirements

### Requirement: Compact view
The button at the bottom left SHALL hide the decks in any layout.
A splitter between the decks and the library SHALL drag the library taller or shorter, stepping the waveforms down and then folding the FX bar; a double-click SHALL expand or restore the library.

#### Scenario: Compact
- **WHEN** the compact view button is pressed
- **THEN** the decks hide and the library fills the window

#### Scenario: Drag the library taller
- **WHEN** the splitter is dragged up by 100 px in 2-Deck
- **THEN** the waveforms step down and the library shows three more rows

## ADDED Requirements

### Requirement: Window size
The window SHALL be at least 1280 × 720. On a short window the waveforms SHALL step down to keep about five library rows; below that 4-Deck's pads and then the FX bar SHALL fold, and pressing PADS or FX SHALL show them anyway. Settings › Interface SHALL offer a scale from 85% to 125%, and a first start on a screen with less than 1280 × 720 usable SHALL pick 90%.

#### Scenario: 13-inch laptop
- **WHEN** 4-Deck runs in a 1440 × 875 window
- **THEN** the library shows at least five rows

#### Scenario: Small Windows laptop
- **WHEN** openDJ first starts on a 1920 × 1080 screen at 150%
- **THEN** the scale is 90% and the window fits with nothing clipped
