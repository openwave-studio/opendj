## ADDED Requirements

### Requirement: Remix tab
The remote page SHALL have a Remix tab that plays the Remix deck. On a tablet in landscape it SHALL show the source and Send/Insert, a SCENE strip that stays where it is left (double tap = centre), three LEVER strips that spring back when let go and stay on when dragged below their notch, an ISOLATE slider per band that snaps to centre, a roll pad (left to right = roll rate, up = pitch) beside the four sample pads, OVERDUB, the sample effect, and RELEASE with its mode. Several fingers SHALL work at once. On a phone the same controls SHALL stack. Hits and rolls SHALL land on the beat grid in openDJ, so network delay only moves them to the next grid step.

#### Scenario: Two hands
- **WHEN** one finger holds LEVER HIGH while another holds the kick pad and slides on the roll pad
- **THEN** the highs echo and the kick rolls at the rate under the finger, both on the beat

#### Scenario: Latch a lever
- **WHEN** a LEVER strip is dragged below its notch and let go
- **THEN** it stays on until tapped again
