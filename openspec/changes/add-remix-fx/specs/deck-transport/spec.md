## ADDED Requirements

### Requirement: Remix deck
The deck ☰ menu SHALL offer Remix beside the other deck kinds. Only one deck SHALL hold Remix at a time: choosing Remix on another deck SHALL move it there, keeping its settings. The Remix deck SHALL listen to one source, assignable at any time to another deck, the mic or an aux input, from the deck or a controller. In Send mode the source SHALL keep playing on its own channel and the Remix deck SHALL add only its effects and pads; in Insert mode the source SHALL be heard only through the Remix deck. The Remix deck's channel fader, EQ, crossfader side and deck FX SHALL work on its output like any deck's, and its waveform strip SHALL show what it plays. In 2-Deck it SHALL show the source and mode, the three band lanes, the SCENE wheel, the pads with roll rates and OVERDUB, and RELEASE; in 4-Deck a slim header with the source, SCENE and RELEASE.

#### Scenario: Only one Remix deck
- **WHEN** deck 2 holds Remix and the DJ picks Remix on deck 4
- **THEN** deck 4 becomes the Remix deck with the same settings and deck 2 is an empty track deck again

#### Scenario: Send from deck 1
- **WHEN** deck 2 is a Remix deck listening to deck 1 in Send mode and LEVER HIGH is held with Echo
- **THEN** deck 1 plays as before and the echoed highs come out of channel 2, at channel 2's fader level

#### Scenario: Insert
- **WHEN** the same Remix deck is switched to Insert and ISOLATE LOW is turned fully left
- **THEN** deck 1's own channel goes quiet and deck 1 is heard through channel 2 without its lows
