# Tasks

## 1. Scratch

- [x] 1.1 Add a latency probe and tune: a quick twin of the scratch filter takes over when the hand changes speed fast (50 % in 26 → 11 ms, 8 Hz baby scratch lag 26 → 10 ms, steady wobble unchanged)
- [x] 1.2 Add cut-in setting with a curve test (0.5 / 1 / 2 / 5 %, default 2 %)
- [x] 1.3 Update the matching user guide page in website/book/src/guide
- [x] 1.4 Ask Windows for 1 ms timers, so the 1 kHz scratch filter runs on time there
- [ ] 1.5 Feel test on the DDJ-FLX4 (hardware)
- [ ] 1.6 Tune platter inertia and back-spin per controller (brake/spinback ramps)
- [x] 1.7 DDJ-400: tune rim pitch bend, follow per-deck Vinyl mode, and return Slip after wheel movement ends; verify on hardware
- [x] 1.8 `jog` bends the speed as in Mixxx (a tenth of each unit per 10 ms block, averaged; 18 times while paused), so every Mixxx mapping's rim is a pitch bend; openDJ's own SHIFT + jog search and the on-screen scroll nudge move to `jog_search`
- [x] 1.9 Slip mode: a let-go jog is followed until it is still for 80 ms, then slip returns, for every controller (from the DDJ-400 script)
- [x] 1.10 Controller Editor › Options: Pitch bend (Fine / Normal / Strong) and Scratch (Loose / Normal / Tight) per controller

## Workflow follow-up

- Archive the change once its PR is merged, so the main specs take its requirements.
