## ADDED Requirements

### Requirement: Pure Data patches
Where openDJ is built with Pure Data support, effect slots and the Instrument input SHALL run Pure Data patches (`.pd` files) from the Pd folder, each in its own plugin helper process with the same crash, hang and CPU limits as other plugins. A patch's knobs SHALL come from its text: receives marked `@hv_param` (name, minimum, maximum, default) and number boxes with a receive name. Turning a knob SHALL send its value to that receive name. The mix's tempo, beat and play state SHALL be sent to `opendj-bpm`, `opendj-beat` and `opendj-playing`. A patch with `[notein]` SHALL be offered as an instrument, one with `[adc~]` as an effect.

#### Scenario: A patch's knobs
- **WHEN** a patch with `[r cutoff @hv_param 20 20000 1000]` is loaded in an effect slot
- **THEN** the slot shows a knob named cutoff from 20 to 20000, starting at 1000, and turning it changes the sound

#### Scenario: A tempo-synced patch
- **WHEN** a patch reads `opendj-bpm` and the master deck plays at 124 BPM
- **THEN** the patch receives 124

#### Scenario: A patch that crashes
- **WHEN** a patch makes its helper crash
- **THEN** the slot plays dry and the decks keep playing
