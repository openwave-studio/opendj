# Proposal

Planned for: **v1.0** (issue #1060)

## Why

The Jam source lists instruments in a 270 px tree of 34 px rows (Recently played, All instruments, My presets, Groove Box, Looper, Remix, Plugins by maker, Inputs) with a wide instrument list beside it. Once the plugin panel (`add-plugin-panel`) shows a plugin in the list's place, the tree is the only way to pick what plays, and it neither fits beside a plugin nor scales: a DJ who also produces can have hundreds of plugins and thousands of presets, and the tree only groups plugins by maker. Finding "a warm pad" means opening plugins one by one.

## What Changes

- **One narrow column** (250 px) replaces the tree. From the top:
  - search at the top, then **Instruments** and **Presets** buttons: Presets lists every preset of every instrument (the user's own and the plugins' factory ones), Instruments one row per instrument (Groove Box, Looper, Remix, plugins, Pd patches, inputs);
  - **search** over the side shown, and a **star** button that shows starred items only;
  - **filter menus** that fold open as chip groups, in this order: Sound type, Genre, Style, Character, Instrument, Maker. A closed menu shows the chips picked in it on its header and nothing when none is; chips narrow each other and show only values that still match; "Clear all" by the count;
  - the **list**: 24 px rows with a plain colour square for the instrument's kind (synth, drums, keys and samplers, tools, Pd, inputs), the name, the instrument's name dimmed, the deck badge and the star. No icons.
- **Loading**: click loads the row on the deck the plugin panel shows (else the selected deck); double-click on a free deck; drag onto a deck as today.
- **Tags**: Sound type and Character start as today's preset tags (`presets::TYPES`, `presets::CHARACTERS`); later steps add sound types with sub-types, genres, styles and a longer character list, and read the tags plugins ship with their presets.
- **Formats**: no Format filter. A preferred plugin format moves to Settings › Plugins; a plugin installed in two formats is one row, with a small format picker shown on the selected row only.

## Out of scope

- Previewing a preset before loading it.
- Colour favourites and playlists of presets (the star is enough for now).
- Art tiles for instruments (assessed in the mockups, left out).

## Right of the column

The wide table goes. The loaded plugin's panel shows right of the column; until one is open, a short hint shows there.

## Capabilities

### Modified Capabilities

- `instruments`: the Jam source is one column with search, star, Instruments and Presets buttons and chip filters.

## Impact

- UI: `crates/app/ui/instruments.slint` (`InstrumentTree` replaced by the column; `InstrumentList` replaced by a hint until the plugin panel is open).
- App: `crates/app/src/instruments.rs` (`nodes()` gives way to facets: values and counts per menu, picks, starred items); `presets.rs` (stars, more tag groups); settings for the stars and the preferred format.
