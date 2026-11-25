# Proposal

Planned for: **v1.0**. Issue #1052, a user request (2026-11-24).

## Why

An instrument's own controls do not fit on a deck. A Pd patch's Lua display (Lilac Dual draws at 1000 x 445) shrinks to a strip in the deck's DISPLAY tab with bars on both sides, and a synth with dozens of parameters shows eight at a time. The browser area below the decks is the largest free space on screen while playing an instrument: at 2000 px wide it is 535 px tall, and the Jam source's list and the side panel take 1,615 px of its width.

## What Changes

- **Plugin panel.** While an instrument plays on a deck, the Jam source can show it in a panel that takes the track list's and the side panel's place; the rail and the Jam tree stay. The panel has a slim bar (the plugin's name, its format, the deck, an expand button for the plugin's own window, ✕ back to the list).
- **At the plugin's shape.** A plugin's own display (a Pd patch's Lua displays) is drawn at its own width over height, as large as the panel allows, with no letterbox bars; clicks and drags go to the patch.
- **Generic controls.** Plugins whose editor cannot be embedded (CLAP, VST3, AU windows) show openDJ's controls instead: every parameter as a slider, grouped by the plugin's pages.
- **From the deck.** The instrument deck's DISPLAY tab opens the panel (the deck keeps its knobs).
- **From the tree.** Clicking an instrument in the Jam tree or list loads it on its deck and shows it in the panel.
- **Several at once.** One tab per deck's instrument once `add-instrument-per-deck` gives each deck its own.
- **FX rack beside it.** The deck's FX units to the right of the plugin, as on the FX bar.
- Plugin effects keep opening in their own window.

## Out of scope

- Embedding CLAP, VST3 or AU editor windows.
- A redesign of the Jam tree.
- The instrument deck's own simplification (another change).

## Capabilities

### Modified Capabilities

- `instruments`: a plugin panel in the browser area for the instrument on a deck.
