## ADDED Requirements

### Requirement: Play a set from Set Builder
A set sent to Automix SHALL fill the Automix list with its segments as headings, and each transition SHALL use its segment's style and duration. Refill from here SHALL rebuild the rest of the list in the set's shape to its end time only when pressed; the list SHALL NOT change by itself.

#### Scenario: Per-segment mixing
- **WHEN** a set's opening segment uses 0 s and its peak segment uses Bass swap over 16 bars
- **THEN** Automix joins the opening tracks gaplessly and mixes the peak tracks with a 16-bar bass swap

#### Scenario: Taking over
- **WHEN** the DJ plays three tracks by hand and hands back to Automix
- **THEN** the rest of the list is as it was until the DJ presses Refill from here
