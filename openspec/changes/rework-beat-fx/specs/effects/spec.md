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
