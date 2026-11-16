# Tasks

## 1. Effect order

- [x] 1.1 `next_effect`, `prev_effect` and `effect_selector` step in the list's order with wraparound (built-in IDs unchanged); plugins numbered in the Plugins tab's order; a test that the engine order and the menu order agree
- [x] 1.2 Beat FX unit controls `beats_halve`, `beats_double` and `route` (0 off, 1-4 a deck, 5 decks 1&2, 6 MST); tests
- [x] 1.3 Beat limits per effect: `beats` range up to 64; flanger and phaser 1/2 to 64, gate 1/16 to 16; halving and doubling stop at the limits; a newly loaded effect keeps the length as near as it allows; tests
- [x] 1.4 LEVEL/DEPTH curve per effect on the Beat FX meta knob (echo-type full wet at the centre, more feedback past it; filter-type dry only fully left); tests
- [ ] 1.5 Echo, filter echo and roll up to 16 beats: a longer delay memory for the Beat FX (today 2 s, which is 4 beats at 120 BPM)

## 2. On screen

- [x] 2.1 An effect change that doesn't come from the slot's own list makes the name glow and shows the previous / current / next strip for about 2 s (Beat FX); deck FX names glow; screenshot
- [x] 2.2 MST in the Beat FX routing row (1 · 2 · 1&2 · MST; 4-Deck 1 · 2 · 3 · 4 · MST); screenshot

## 3. Controllers

- [x] 3.1 DDJ-400 Beat FX on unit 5 (community PR #769, DDJ-400 part only)
- [x] 3.2 DDJ-FLX4, DDJ-FLX6, DDJ-FLX10, DDJ-800, DDJ-400, XDJ-RX3 and XDJ-AZ scripts use the shared controls; CH SELECT MASTER selects MST on every one; mapping tests

## 4. Docs

- [x] 4.1 The FX page: list order, the strip, MST; the DDJ-400 section
