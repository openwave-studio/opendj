# Tasks

## 1. Engine

- [ ] 1.1 Low and high shelf in `dsp::Biquad`; a `RoomEq` with six bands, coefficient crossfades and skipping of flat bands; an Isolator with boost and ramped gains; tests (flat is transparent, a kill removes its band, no zipper on a sweep)
- [ ] 1.2 `[Master],iso_low/mid/high` and `[RoomEQ_Main]`/`[RoomEQ_Booth]` controls; the master chain as in design.md; the recording, broadcast and loudness tap before Room EQ with its own limiter; tests (recording unchanged with Room EQ off, Room EQ not in the recording or headphones, booth curve on the booth only); benchmark

## 2. MASTER panel

- [ ] 2.1 ISOLATOR knobs with Flat, the ISO tag on the MASTER button, BOOTH EQ LOW/HIGH while a booth output is set up, the Room EQ preset name with Edit…; screenshot
- [ ] 2.2 `[Master],iso_*` in the controller catalog and Map mode

## 3. Room EQ

- [ ] 3.1 Settings › Audio › Room EQ: Main and Booth tabs, On switch, the curve widget with draggable dots, scroll for width, double-click reset, band cards; saved in settings; screenshot
- [ ] 3.2 Presets: Flat, Muddy room, Harsh highs, Small speakers, own presets by name, modified mark
- [ ] 3.3 Spectrum behind the curve while the page is open

## 4. Docs

- [ ] 4.1 Guide page in website/book/src/guide: the Isolator, Room EQ per venue, and why recordings don't include it

## Workflow follow-up

- Archive the change once its last PR is merged, so the main specs take its requirements.
