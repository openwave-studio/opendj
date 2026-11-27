# Tasks

## 1. Serato (DJ Pro 4 and 5)

- [ ] 1.1 Map `master.sqlite` from a copy of a real Serato 5 library (tracks, crates, smart crates, History tables) and record the layout in design.md; cut a small fixture from it
- [ ] 1.2 Tag reader: `Serato Markers2`, `Serato BeatGrid` and `Serato Autotags` from ID3 GEOB, MP4 freeform and Vorbis comments: hot cues, loops, colors, track color, grid, gain; byte fixtures; MP3/M4A timing checked against Serato on real files
- [ ] 1.3 Serato 5 specifics on real files: Cue Loops, Lucid Beatgrids with tempo changes, Serato's pad color palette
- [ ] 1.4 Read tracks, crates and subcrates, smart crates and History from `master.sqlite` (copied first), on the music drive and external drives; streamed tracks as placeholders
- [ ] 1.5 Fallback to `database V2` and `Subcrates/*.crate` when `master.sqlite` can't be read, with the preview line saying History is left out
- [ ] 1.6 Settings › Collection › Import row and wizard in place of the Coming soon row; "open in Serato 4 or newer first" hint; screenshot

## 2. OneLibrary

- [ ] 2.1 Read `exportLibrary.db` with rbox-onelib: tracks, cues, playlists, history, with a fixture made by openDJ's own USB export
- [ ] 2.2 Grids and phrases from the stick's ANLZ files; `export.pdb` fallback through rbox-devlib
- [ ] 2.3 Pick a stick or folder in the wizard; Settings row; screenshot

## 3. Wrap up

- [ ] 3.1 Cross-source matching tests with a track in Serato and another app
- [ ] 3.2 Update the import page in website/book/src/guide (music.md)

## Workflow follow-up

- Archive the change once its PR is merged, so the main specs take its requirements.
