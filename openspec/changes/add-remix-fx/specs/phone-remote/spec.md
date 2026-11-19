## ADDED Requirements

### Requirement: Remix tab
The remote page SHALL have a Remix tab that plays the Remix deck. On a tablet in landscape it SHALL use the wheel layout: the three ISOLATE knobs in a panel pointing into a large SCENE wheel whose upper half lists the Build Up scenes and lower half the Break Down scenes around a centre knob that stays where it is left (double tap = centre), the source and Send/Insert, BPM with TAP and QUANTIZE along the top, a release lever at the right edge, a LEVER under each ISOLATE knob (press and hold = on while held, like pushing a lever up; drag it down = on and held, like pushing a lever down; tap a held lever to let it go; depth from the lever's depth control), a roll pad (left to right = roll rate, up = pitch) beside the four sample pads, OVERDUB, the sample effect, and RELEASE with its mode. Several fingers SHALL work at once. The Remix tab SHALL be designed for an 11-inch iPad in landscape (1194 × 834 points) and be landscape only: held upright, it SHALL ask to turn the device sideways. On a phone in landscape the same layout SHALL scale down. The layout SHALL use openDJ's own colours, shapes and names. Hits and rolls SHALL land on the beat grid in openDJ, so network delay only moves them to the next grid step. The page SHALL show the round-trip delay in milliseconds and warn when it jumps around.

#### Scenario: Two hands
- **WHEN** one finger holds LEVER HIGH while another holds the kick pad and slides on the roll pad
- **THEN** the highs echo and the kick rolls at the rate under the finger, both on the beat

#### Scenario: Latch a lever
- **WHEN** a LEVER is dragged down and let go
- **THEN** it stays on until tapped again

#### Scenario: Press and hold a lever
- **WHEN** a LEVER is pressed and held without dragging
- **THEN** its effect is on until the finger lifts
