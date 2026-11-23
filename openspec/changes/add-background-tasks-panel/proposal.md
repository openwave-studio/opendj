# Proposal

Planned for: **v1.0**. Issue #972 (first item), a user request (2026-11-22).

## Why

openDJ does heavy work behind the decks: analysis (tempo, grid, key, phrases), high-quality stems, Whisper word timing, and Collection Health's sound and cover scans. Today only analysis shows anything ("Analyzing 3 of 12" in the footer). HQ stems and lyrics give no sign of how far they are or when a requested track is ready to load, and nothing can be paused when the computer is needed for the set.

## What Changes

- **Footer ring**: a small progress ring takes the place of the footer's analysis text. It shows only while background work runs or is paused, fills with overall progress, turns amber when everything is paused, and shows a green check for a few seconds when the work is done. Player shows it in its bar's place.
- **Background panel**: a click on the ring pops a panel up over the browser, closed by a click outside or Esc. One row per kind of task (Analyze, HQ Stems, Lyrics, Sound check, Covers) with how many of how many, a progress bar, time left, the track it is on, and Pause or Resume and Stop where the task can stop. Analyze and HQ Stems list the next tracks, and "Do next" moves one to the front.
- **Pause all** in the panel's header pauses every task; pauses are kept across restarts until resumed.
- **While a deck plays**: Slow down (today: one core, rests between tracks), Pause, or Keep going; a switch pill at the panel's foot, kept in settings.
- **Pause points**: analysis, the scans and HQ stems pause between tracks; HQ stems also between the model's chunks, and Whisper inside its listening, so a pause frees the computer within seconds.
- **HQ Stems queue**: separations wait in one queue (one runs at a time, as now), report progress by chunk, and can be stopped; a stopped separation does not fall back to the CPU.
- **Stems column**: a track list column showing a track's HQ stems: "Ready", "64%", "Waiting", "Paused" or "Failed".

## Out of scope

- Downloads (store, TIDAL, yt-dlp): they have their own progress in the list rows.
- The issue's MP4 video panel and YouTube Music source, which are separate requests.
- Pausing inside one track's analysis (it pauses after the current track).

## Capabilities

### Modified Capabilities

- `app-layout`: the footer ring and the Background panel.
- `track-browser`: the Stems column.
