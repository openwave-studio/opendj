# Design

Research note: `/mnt/project-files/notes/serato-import-research.md` (project files, not in the repo). This design covers the Serato importer; the OneLibrary importer follows the rekordbox import and needs no separate design.

## Scope: Serato DJ Pro 4 and 5 only

Serato DJ Pro 4.0 moved its library into SQLite. Serato 5.0 (2026-10-20) kept that and added Lucid Beatgrids, Cue Loops and Open Streaming. Libraries from Serato 3 and older are out: a DJ still on 3.x opens their library once in Serato 4 or 5, which converts it (and its History) into `master.sqlite`.

## Where the data is

| Data | Where (Serato 4 and 5) | Read from |
|---|---|---|
| Tracks, paths, play counts, added dates | `~/Library/Application Support/Serato/Library/master.sqlite` (with `root.sqlite`) | `master.sqlite`; else `_Serato_/database V2` |
| Crates, subcrates, crate order | `master.sqlite`; also exported to `_Serato_/Subcrates/*.crate` on quit (`%%` nests) | `master.sqlite`; else the `.crate` files |
| Smart crates and their rules | `master.sqlite` (to confirm); `_Serato_/SmartCrates/*.scrate` | same order |
| History sessions | `master.sqlite` only (reported tables `history_session`, `history_entry`); the old `History/Sessions` folder stops updating at the 4.0 upgrade | `master.sqlite` |
| Hot cues, saved loops, cue colors and names, track color, BPM lock | each file's `Serato Markers2` tag | file tags |
| Beat grid | each file's `Serato BeatGrid` tag | file tags |
| Auto gain, BPM | `Serato Autotags` | file tags (gain only; BPM comes from the grid) |

External drives keep their own `_Serato_` folder at the drive root for the tracks on that drive; tracks there appear in `master.sqlite` too when the drive was connected (to confirm). Windows paths (`%USERPROFILE%\Music\_Serato_`, Serato's app-data folder) are to be confirmed on a Windows install before that path is turned on.

The table layout of `master.sqlite` isn't published. Task 1.1 maps it from a copy of a real Serato 5 library and records it here before the reader is written. Where `master.sqlite` can't be opened or its layout isn't recognised (a newer Serato), the importer falls back to `database V2` and the `.crate` files, which every Serato 4 and 5 still writes, says so on the preview line, and skips History.

`master.sqlite` is copied (with any `-wal` and `-shm` files) to a temporary folder and opened read-only, as the djay import does, so a running Serato is never disturbed.

## Tag containers

| Format | Where Serato writes | Payload |
|---|---|---|
| MP3, AIFF, WAV | ID3v2 GEOB frame, description `Serato Markers2` / `Serato BeatGrid` / `Serato Autotags` | raw |
| M4A / MP4 | freeform atom `----:com.serato.dj:markersv2`, `beatgrid`, `autgain` | base64 of a mime + name header and the raw payload |
| FLAC, Ogg Vorbis | Vorbis comment `SERATO_MARKERS_V2`, `SERATO_BEATGRID`, `SERATO_AUTOGAIN` | base64, as MP4 |

lofty reads all of these; the Serato reader gets bytes and parses them itself, so its parsing is testable on byte fixtures without audio files.

## Formats

Layouts as documented by Mixxx, serato-tools and Holzhaus/serato-tags; each one is confirmed against fixtures cut from real Serato 5 files before it is relied on.

- **Markers2**: `01 01` header, then base64 text (line breaks allowed, final padding sometimes wrong, so decode leniently) holding a sequence of entries: a zero-terminated type name, a big-endian u32 length and the body. `CUE` (index, position in ms, RGB color, UTF-8 name), `LOOP` (index, start and end in ms, locked flag, name), `COLOR` (track color), `BPMLOCK`. Unknown types (`FLIP`, new Serato 5 entries) are skipped by length, never an error.
- **BeatGrid**: version header, marker count, then non-terminal markers (position in seconds as f32, beats to the next marker) and one terminal marker (position, BPM as f32). A Lucid grid with tempo changes is expected to be several non-terminal markers; to confirm with a Serato 5 file that has a tempo change (task 1.3).
- **Cue Loops** (Serato 5): expected to be a `CUE` and a `LOOP` at the same index; to confirm (task 1.3).

## Mapping onto openDJ

- Serato cue index n → `Hotcue { number: n, seconds, name, color }`. Serato's pad colors are stored as RGB, but Serato shows them through its own palette; map the eight-plus stored values to the colors Serato shows (table to fill from a real file, as the djay import's colors were).
- `LOOP` → a `Hotcue` with `end`. A Serato loop in a pad slot already used by a cue goes to the first free pad above 8, as other imports do; a Cue Loop whose cue and loop share an index becomes one saved loop starting at the loop start (openDJ's cue = loop start rule).
- `BeatGrid` → grid markers, saved locked with `grid_from = "serato"`, never replaced by analysis; re-import updates them unless the DJ edited the grid in openDJ (the rekordbox rule).
- `COLOR` → track color; `BPMLOCK` → grid locked; Autotags gain → track gain.
- Crates → `ImportedListKind::List` in a Serato DJ folder, nested as subcrates; History sessions → `ImportedListKind::History` with each entry's play time; smart crates → `ImportedListKind::Smart` where every rule maps (title/artist/genre/BPM/key/year/comment/added date contains/is/between), else a plain list of the tracks Serato last matched.
- Streamed entries (Serato 5 Open Streaming: TIDAL, Beatport, Beatsource, SoundCloud, Apple Music) → placeholders via `StreamedTrack`.

## Decisions

- **Own reader, no triseratops.** triseratops (Rust) is AGPL-3.0-or-later, released last in 2023 and marked unstable. Mixxx's `src/track/serato` and `src/library/serato` are GPL-2.0-or-later and can be followed directly, with credit; serato-tools (MIT, active) is the cross-check for field layouts and smart crate rules.
- **MP3 and M4A timing.** Serato's positions can sit a few ms off openDJ's decoder for some MP3s (encoder delay handling differs) and M4As. Mixxx applies a per-file offset for this. Task 1.2 measures it on real files against what Serato shows and ports Mixxx's rule only if openDJ's Symphonia decode drifts.
- **Tags read at import only.** The tag reader runs when the Serato importer reads a track, not on every folder scan; reading Serato tags on scan is a possible follow-up.
- **No writes.** Neither `master.sqlite`, the `_Serato_` files nor the audio tags are ever written.
