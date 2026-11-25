# Proposal

Planned for: **v0.9.5** (issue #1096)

## Why

Club players show a phase meter: how far a deck's beats are ahead of or behind the master deck's, and whether it sits on the wrong beat of the bar. Controllers in the Hercules Inpulse line add a beatmatch guide with tempo arrows that say which way to move the tempo fader. openDJ shows neither, so beatmatching by ear without SYNC, or catching a deck dropped a beat off, has nothing on screen to lean on. The beat counter sits beside the title in the deck header, where it competes with the title for room.

## What Changes

- **A 14 px strip under each deck's overview** in the 2-Deck layout, right under the phrase colours. The title row gets shorter by about as much (the key box slims), so the deck keeps its height and the cover stays the same square.
- **Beat counter moves into the strip**, on its inner side next to the mixer: beat dots, phrase.bar.beat and the countdown to the next phrase (mirrored on the left deck, so the dots sit closest to the mixer). The beat counter setting still applies: Beats shows the dots only, Off leaves that corner empty.
- **EARLY / LATE line** on the strip's outer side: a line half a beat each way with a marker at this deck's offset from the master deck, and the offset in ms. Whole beats off (the wrong beat of the bar) read "+1 BEAT" or "−1 BEAT" instead. Green within 10 ms, yellow within 35 ms, orange beyond or when whole beats off.
- **Tempo arrows** beside it: ▲ or ▼ points the way to move this deck's tempo fader until its BPM matches the master's (following the fader direction setting); both dark within 0.05 BPM.
- **What a deck compares with**: the deck with the MASTER badge, else the other playing deck. Half and double tempo count as matched. The master deck's line is drawn muted with a MASTER label; with nothing to compare with, it is muted and blank.
- The strip always shows, at a fixed height, so the deck never changes height.

## Out of scope

- The 4-Deck strips, Prep and Player headers (2-Deck only for now).
- Driving controller LEDs (beat align and tempo arrows on Hercules controllers, jog displays) from the same values: a later step.

## Capabilities

### Modified Capabilities

- `sync`: a phase meter and tempo guide per deck.

## Impact

- App: `crates/app/src/beat_counter.rs` (`phase()`: offset, whole beats and tempo direction against a reference deck); `crates/app/src/main.rs` (fills it per frame from the decks' play positions and tempo).
- UI: `crates/app/ui/deck.slint` (`BeatCount` gains the phase fields; `BeatStrip`; `Header` and `Deck` take `beat-strip`); `crates/app/ui/main.slint` (2-Deck decks turn it on); `crates/app/ui/theme.slint` (phase colours).
