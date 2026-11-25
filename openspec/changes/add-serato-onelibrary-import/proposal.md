# Proposal

Planned for: **v1.0** (a user request in issue #602, 2026-11-14; Serato part narrowed to Serato DJ 4 and 5 at a user request, 2026-11-25)

## Why

openDJ imports rekordbox, Traktor, VirtualDJ, djay Pro, Mixxx and the Music app, but not the last two places a DJ's prep commonly lives:

- **Serato DJ**, which keeps cues, loops, beat grids and colors in the audio files' own tags, and since Serato DJ Pro 4.0 its tracks, crates and history in a SQLite library (`master.sqlite`), with the older `_Serato_` files still written next to it for other apps.
- **OneLibrary** (`exportLibrary.db`), the library AlphaTheta's newer players read from a USB stick. rekordbox, djay, Traktor and VirtualDJ can all write one (Serato does not), so a stick prepared in any of them could come into openDJ the same way. OneLibrary import was dropped on 2026-10-19 while rekordbox import was built; openDJ now writes OneLibrary for its own USB export (`crates/export/src/onelib.rs`, rbox-onelib), so reading it back costs little.

With both, a DJ can move from any major app, or from a prepared stick, without losing their prep.

## What Changes

- **Serato importer** for Serato DJ Pro 4 and 5 (read-only, like the others; see design.md):
  - Tracks, crates and subcrates, smart crates and History sessions from Serato's library, `~/Library/Application Support/Serato/Library/master.sqlite`, copied before it is opened, as the djay import does. When that can't be read, tracks and crates come from the `database V2` and `Subcrates/*.crate` files Serato 4 and 5 still write on quit, in `~/Music/_Serato_` and each external drive's `_Serato_` folder (no History there).
  - Hot cues with colors and names, saved loops (Serato 5's Cue Loops included), beat grids (Lucid Beatgrids' tempo changes included) and track color from the files' `Serato Markers2` and `Serato BeatGrid` tags: ID3 GEOB frames (MP3, AIFF, WAV), MP4 freeform atoms (M4A) and FLAC/Ogg Vorbis comments, read with lofty. Imported grids are locked, like rekordbox's.
  - Smart crates become smart lists (off by default, as elsewhere) where their rules map onto openDJ's search, otherwise plain lists of the tracks they held.
  - Streamed tracks (Serato 5's Open Streaming) stay as Wanted placeholders.
  - Our own reader in `crates/plugins/src/serato/`, following Mixxx's Serato code (GPL-2.0-or-later) and checked against serato-tools (MIT). triseratops is AGPL-3.0, so it is not used.
- **OneLibrary importer**: pick a stick or a folder holding `PIONEER/rekordbox/exportLibrary.db`. Tracks, hot and memory cues with colors, saved loops, beat grids and phrases (from the stick's ANLZ files, read with rbox-anlz as the rekordbox import does), rating, color, play count, playlists in their folders and history. An older stick with only `export.pdb` is read the same way through rbox-devlib.
- Both go in Settings › Collection › Import with the same wizard as the others: pick lists, re-read at start, removable, and matched across sources so a track in two apps stays one track.
- The roadmap's "OneLibrary import (dropped)" line is removed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `library-import`: Serato and OneLibrary importers

## Out of Scope

- Serato DJ before 4.0: its `History/Sessions` files, `Serato Markers_` tags without a `Serato Markers2` tag, and libraries never opened in Serato 4 or 5.
- Serato DJ Lite, Serato Studio and Serato DJ Intro libraries, unless they turn out to be the same files.
- Writing back to Serato (tags or crates) or writing a OneLibrary stick from the import side. openDJ's USB export already writes OneLibrary.
- Serato's stems, Flip and video data.
- Serato DJ's Apple Music and streaming entries beyond keeping them as Wanted placeholders, as other imports do.

## Impact

crates/plugins (new `serato` and `onelibrary` importers; rusqlite (already a dependency) and lofty (used by the engine) for Serato; rbox-onelib, rbox-devlib and rbox-anlz are already dependencies), Settings › Collection › Import rows, fixture libraries and tagged files for tests, the Coming soon Serato row on the Import tab (`COMING_IMPORTS` in crates/app/src/prefs.rs) replaced by the real one, the user guide's import page.
