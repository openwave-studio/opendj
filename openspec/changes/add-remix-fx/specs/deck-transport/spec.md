## ADDED Requirements

### Requirement: Remix deck
The deck ☰ menu SHALL offer Remix beside the other deck kinds. Only one deck SHALL hold Remix at a time: choosing Remix on another deck SHALL move it there, keeping its settings. The Remix deck SHALL listen to one source, assignable at any time to another deck, the mic or an aux input, from the deck or a controller. In Send mode the source SHALL keep playing on its own channel and the Remix deck SHALL add only its effects and pads; in Insert mode the source SHALL be heard only through the Remix deck. The Remix deck's channel fader, EQ, crossfader side and deck FX SHALL work on its output like any deck's, and its waveform strip SHALL show what it plays. In 2-Deck it SHALL show the source and mode and two tabs: WHEEL, with the SCENE wheel at the centre, the three ISOLATE knobs each with its LEVER beside it, the Isolate and Lever effect pickers, and a RELEASE lever with its mode; and PADS, with the four sample pads, OVERDUB, UNDO, pad level and pitch, the sample effect and a roll-rate strip. A BIG button SHALL spread the Remix deck over the library area in the same wheel layout as the remote's Remix tab, with every control at once and the decks' waveform strips kept above it; BIG again or Esc SHALL put it back. In 4-Deck it SHALL show a slim header with the source, the SCENE amount, RELEASE and BIG.

#### Scenario: Only one Remix deck
- **WHEN** deck 2 holds Remix and the DJ picks Remix on deck 4
- **THEN** deck 4 becomes the Remix deck with the same settings and deck 2 is an empty track deck again

#### Scenario: Send from deck 1
- **WHEN** deck 2 is a Remix deck listening to deck 1 in Send mode and LEVER HIGH is held with Echo
- **THEN** deck 1 plays as before and the echoed highs come out of channel 2, at channel 2's fader level

#### Scenario: Insert
- **WHEN** the same Remix deck is switched to Insert and ISOLATE LOW is turned fully left
- **THEN** deck 1's own channel goes quiet and deck 1 is heard through channel 2 without its lows

#### Scenario: BIG mode
- **WHEN** the DJ presses BIG on the Remix deck
- **THEN** the library area shows the full wheel layout with pads and roll rates, and the waveform strips stay visible above it
