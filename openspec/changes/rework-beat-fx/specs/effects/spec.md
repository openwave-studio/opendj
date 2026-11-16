## MODIFIED Requirements

### Requirement: Beat FX
The FX bar SHALL show the Beat FX unit (unit 5) under the mixer, Pioneer style: one effect, a routing row (1, 2, 1&2 and MST; on 4-Deck 1, 2, 3, 4 and MST), ON, beat length and LEVEL. MST SHALL send the effect to the main mix and turn the deck routes off. The BEAT FX sections of Pioneer controllers SHALL drive it the same way: FX SELECT steps forward (SHIFT back), BEAT ◀ ▶ halve and double the length (SHIFT: AUTO and TAP), LEVEL/DEPTH sets its depth, ON/OFF toggles it (SHIFT: release), and CH SELECT MASTER selects MST.

#### Scenario: FLX4 Beat FX
- **WHEN** BEAT FX ON is pressed on a DDJ-FLX4
- **THEN** unit 5 turns on and the on-screen ON lights

#### Scenario: MASTER from a controller
- **WHEN** CH SELECT is set to MASTER on a DDJ-400
- **THEN** the Beat FX plays on the main mix, the decks' routes are off and MST lights on screen

## ADDED Requirements

### Requirement: Effect stepping order
`next_effect`, `prev_effect` and `effect_selector` SHALL step through the effects in the order the effect list shows them: the built-in effects by kind and then by name, then the ticked plugins in the Plugins tab's order, wrapping at either end. Built-in effects SHALL keep their `loaded_effect` numbers.

#### Scenario: Next after Echo
- **WHEN** a slot holding Echo gets `next_effect`
- **THEN** it loads Echo Out, the next effect in the list

#### Scenario: Wrap
- **WHEN** a slot holding the last effect in the list gets `next_effect`
- **THEN** it loads the first effect in the list

### Requirement: Effect changes shown
When a slot's effect changes from anywhere other than its own list (a controller, the ‹ › arrows, OSC), the effect's name SHALL glow for about 2 seconds. For the Beat FX, a strip under it SHALL show the previous, current and next effect for the same time, then fade out. Clicking the name SHALL still open the full list.

#### Scenario: FX SELECT on a controller
- **WHEN** FX SELECT is pressed on a DDJ-FLX4 while the Beat FX holds Echo
- **THEN** the Beat FX shows Echo Out glowing, with "Echo" before it and "Filter Echo" after it in the strip, and the strip fades after about 2 seconds

### Requirement: Shared Beat FX controls
The Beat FX unit SHALL have `beats_halve` and `beats_double` (halve or double the loaded effect's length within its limits) and `route` (0 off, 1 to 4 one deck, 5 decks 1 and 2, 6 the main mix), next to `next_effect`, `prev_effect`, `enabled`, `release`, `bpm_tap` and `bpm_auto`. Controller mappings for Beat FX sections SHALL use these controls rather than computing lengths or routes themselves.

#### Scenario: Double at the limit
- **WHEN** the Beat FX holds Echo at 16 beats and `beats_double` is pressed
- **THEN** the length stays at 16 beats

#### Scenario: Route to MST
- **WHEN** `route` is set to 6
- **THEN** the Beat FX plays on the main mix and every deck route is off

### Requirement: Beat limits per effect
Each effect SHALL have its own beat range: delay, echo, roll and gate-type effects 1/16 to 16 beats, and filter, flanger, phaser and other sweeps 1/16 to 64 beats. When an effect is loaded whose range doesn't hold the current length, the length SHALL move to the nearest value in range.

#### Scenario: Long flanger sweep
- **WHEN** the Beat FX holds Flanger at 32 beats and BEAT ▶ is pressed
- **THEN** the length becomes 64 beats

#### Scenario: Switching to a shorter range
- **WHEN** the Beat FX holds Flanger at 64 beats and Echo is loaded
- **THEN** the length becomes 16 beats

### Requirement: Beat FX depth curve
The Beat FX's LEVEL/DEPTH (its meta knob) SHALL follow a curve per effect, as on Pioneer mixers: for echo and delay effects the wet level SHALL reach full at the centre, with more feedback past it; for filter, flanger and phaser the effect SHALL be dry only fully left. Deck FX SHALL keep a linear D/W.

#### Scenario: Echo at the centre
- **WHEN** the Beat FX holds Echo and LEVEL/DEPTH is at the centre
- **THEN** the echo is at full wet level
