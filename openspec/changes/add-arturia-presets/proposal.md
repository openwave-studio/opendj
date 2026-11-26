# Proposal

Planned for: **v0.9.5**. Issue #1161, a user request (2026-11-26).

## Why

Arturia's instruments ship thousands of presets (about 18,500 with V Collection), tagged by sound type, genre, style and character. openDJ lists a plugin's own presets only from the standard preset files and lists (`.vstpreset`, `.aupreset`, AU factory presets, CLAP preset discovery); Arturia uses none of them, so its instruments showed no presets at all and could only be browsed in their own window.

## What Changes

- **The library is read.** When an Arturia instrument is ticked in Settings › Plugins, openDJ reads Arturia's preset library (`db.db3` beside the instrument folders) in the background, read only, and again only when it changed.
- **Listed as the plugin's own.** Each Arturia instrument lists the presets in its own folder (Analog Lab also those made for it), after the user's, read only as other plugin presets: SAVE keeps a copy.
- **Tags come along.** Arturia's types become openDJ's Sound types, its Genres and Styles fill new Genre and Style menus in the Jam column (shown only when some preset has one), and styles and characteristics that match openDJ's Character tags count as those. Presets liked in Arturia's browser are starred once in the Jam column; unstarring sticks.
- **Loaded as the plugin's sound.** A preset file is the instrument's own saved state, so it loads as is: as both VST3 state parts, or as the Audio Unit's controller state in its class info. No parameter mapping.
- **Everywhere presets are.** The Jam column, the deck's ‹ › and preset menu, and controller preset buttons (the M32 screen shows the name).
- **Later:** keep the plugin's MIDI CC assignments across a load; a preset picker in the plugin panel header for every plugin; Arturia's effects once FX-slot plugins load presets.

## Out of scope

- Writing to Arturia's library (likes, new presets): openDJ's own presets stay in its folder.
- Other makers' libraries (NKS and the like): their own change if wanted.
- Linux: Arturia has no Linux instruments.

## Capabilities

### Modified Capabilities

- `instruments`: a maker's preset library listed and loaded as the plugin's own presets.
