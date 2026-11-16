# Tasks

## 1. Engine

- [x] 1.1 Low and high shelf in `dsp::Biquad`; a `RoomEq` with six bands, gliding changes and skipping of flat bands; tests (flat is transparent, each band matches its curve, no zipper on a sudden change)
- [x] 1.2 `[RoomEQ_Main]`/`[RoomEQ_Booth]` controls; Room EQ in the master chain as in design.md; the recording, broadcast and loudness tap before Room EQ with its own limiter; tests (recording unchanged by Room EQ, Room EQ not in the recording or headphones, booth curve on the booth only, no seam as it comes and goes); benchmark
- [ ] 1.3 An Isolator with boost and ramped gains; tests (flat is transparent, a kill removes its band)
- [ ] 1.4 `[Master],iso_low/mid/high` controls and the Isolator in the master chain; tests

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
