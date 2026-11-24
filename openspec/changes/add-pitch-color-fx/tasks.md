# Tasks

## 1. Engine

- [x] 1.1 `PitchShifter` in `effects.rs`: two read heads with raised-cosine fades half a window apart, each starting where the sound best matches the other head; cubic reads; 10 ms glide; tests (an octave and a half octave each way within 2%, level within 10%, back to exactly dry in the middle without a jump); bench 54 ns per frame at 48 kHz (Space 72, Dub Echo 55)
- [x] 1.2 Pitch as Color FX 6: `COLOR_FX`, the deck menu, stepping with next/prev_chain_preset

## 2. Controllers

- [x] 2.1 SOUND COLOR FX PITCH on the DDJ-FLX10, XDJ-R1 and XDJ-RR; PITCH on the XDJ-R1 and XDJ-RR Controller Editor pictures; mapping tests
- [x] 2.2 Sound Color FX buttons on the DDJ-RX, RZ and RZX (shared `Pioneer-DDJ-RX-scripts.js` handlers per Color FX, lights); their Controller Editor pictures show the buttons' press messages and the RZ's real labels; mapping tests
- [x] 2.3 FX guide and controller guides
- [x] 2.4 Every bundled controller checked: Sound Color FX buttons on the DDJ-SZ (ECHO, PITCH, FILTER, from Pioneer's MIDI list) and the DDJ-800 (D-ECHO, PITCH, NOISE, FILTER, lit); mapping tests

## 3. Check

- [ ] 3.1 Listen on the Mac: a sweep from an octave down to an octave up on a vocal and on a drum loop
- [ ] 3.2 Press PITCH on a DDJ-FLX10
