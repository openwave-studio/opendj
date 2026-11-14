# Proposal

Planned for: **v0.8 · From the booth to the studio** (a user request in issue #602, 2026-11-14)

## Why

openDJ imports rekordbox, Traktor, VirtualDJ, djay Pro, Mixxx and the Music app, but not the last two places a DJ's prep commonly lives:

- **Serato DJ**, which keeps cues, loops, beat grids, colors and crates in the audio files' own tags and a `_Serato_` folder, not in a database openDJ can already read.
- **OneLibrary** (`exportLibrary.db`), the library AlphaTheta's newer players read from a USB stick. rekordbox, djay, Traktor and VirtualDJ can all write one, so a stick prepared in any of them could come into openDJ the same way. OneLibrary import was dropped on 2026-10-19 while rekordbox import was built; openDJ now writes OneLibrary for its own USB export (`crates/export/src/onelib.rs`, rbox-onelib), so reading it back costs little.

With both, a DJ can move from any major app, or from a prepared stick, without losing their prep.

## What Changes

- **Serato importer** (read-only, like the others):
  - Tracks from `_Serato_/database V2` on the music drive and any external drives that have a `_Serato_` folder.
  - Hot cues with colors and names, saved loops, beat grid markers and track color from the `Serato Markers2` and `Serato BeatGrid` tags (ID3 GEOB, MP4 and FLAC/Ogg Vorbis comments), with `Serato Markers_` as fallback for older files. Tag layouts follow the reverse-engineered notes at Holzhaus/serato-tags.
  - Crates and subcrates from `_Serato_/Subcrates/*.crate` (`%%` nests them), and smart crates as smart lists where their rules map onto openDJ's filters, otherwise left out with a note.
  - History sessions from `_Serato_/History` as History days.
- **OneLibrary importer**: pick a stick or a folder holding `PIONEER/rekordbox/exportLibrary.db`. Tracks, hot and memory cues with colors, saved loops, beat grids and phrases (from the stick's ANLZ files, read with rbox-anlz as the rekordbox import does), rating, color, play count, playlists in their folders and history. An older stick with only `export.pdb` is read the same way through rbox-devlib.
- Both go in Settings › Collection › Import with the same wizard as the others: pick lists, re-read at start, removable, and matched across sources so a track in two apps stays one track.
- The roadmap's "OneLibrary import (dropped)" line is removed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `library-import`: Serato and OneLibrary importers

## Out of Scope

- Writing back to Serato (tags or crates) or writing a OneLibrary stick from the import side. openDJ's USB export already writes OneLibrary.
- Serato's stems, Flip and video data.
- Serato DJ's Apple Music and streaming entries beyond keeping them as Wanted placeholders, as other imports do.

## Impact

crates/plugins (new `serato` and `onelibrary` importers; rbox-onelib, rbox-devlib and rbox-anlz are already dependencies), Settings › Collection › Import rows, fixture libraries for tests, the user guide's import page.
