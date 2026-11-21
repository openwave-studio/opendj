## ADDED Requirements

### Requirement: HID controller pictures
The Controller Editor SHALL show a picture for each HID controller on the supported list, drawn from the maker's manual, and list what each control does in each layer and where it is in the input report. HID mappings SHALL NOT be remappable in the editor.

#### Scenario: Open the S2 MK3 in the Controller Editor
- **WHEN** the Traktor Kontrol S2 MK3 mapping is opened in the Controller Editor
- **THEN** its picture shows, the CUE button lists Cue and, in the SHIFT layer, Back to the start, and the chip reads USB HID with no MIDI map button

### Requirement: Traktor-style Kontrol mappings
openDJ's Traktor Kontrol mappings SHALL play as Traktor's own mappings do where openDJ has the controls: stem pads on a track with stems, SHIFT + CUE back to the start, REV as flux reverse and GRID held with the jog wheel moving the beat grid.

#### Scenario: Stems on the S2 MK3
- **WHEN** a track with stems is on Deck 1 and SAMPLES then pad 1 are pressed
- **THEN** stem 1 of Deck 1 is muted, and pressing pad 1 again unmutes it
