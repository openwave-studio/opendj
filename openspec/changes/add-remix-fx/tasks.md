# Tasks

## 1. Band FX

- [ ] 1.1 engine: `BandSplit` shared with the isolator; `band` mask on every slot; dry bands added back; plugin slots get the band-limited buffer; tests (flat sum with the effect bypassed, only the chosen band changes)
- [ ] 1.2 app: L · M · H chips on every slot in the FX bar (deck FX, Beat FX, Pad FX, Mic/Aux); screenshot

## 2. Release

- [ ] 2.1 engine: `[RemixN],release`, `release_mode` (Echo, Brake, Spin, Cut), `release_time`, `[Master],fx_release`; Backspin effect; tests (tails keep ringing, no clicks)
- [ ] 2.2 app: RELEASE on the Remix deck and in the FX bar; Shift + Beat FX ON releases on Pioneer mappings where the button is free

## 3. Remix deck

- [ ] 3.1 engine `remix.rs`: Remix deck kind (one at a time; moving keeps settings), `[RemixN]` with `source` and `mode` (Send, Insert), isolate and lever lanes, `isolate_effect`, `lever_effect`; idle Send adds nothing, idle Insert is bit-identical; tests
- [ ] 3.2 new effects: Ducker, Repeat, Solo, Rhythm, Tape Echo (in any slot too); bench numbers
- [ ] 3.3 scenes: `scene`, `scene_up`, `scene_down`, `scene_time`, `scene_color`; Rise Echo, Crush Echo, Spiral Down; five scenes each side; tests
- [ ] 3.4 Stretch lever effect

## 4. Sample pads

- [ ] 4.1 engine: `pad1..4`, quantized hits, `roll_rate`, `roll_rate2` groove roll, pitch per pad, `pad_bank` (Groove Box kit or sample pack bank); tests
- [ ] 4.2 overdub 1-bar loop with undo and clear; `sample_fx`, `sample_fx_amount`

## 5. On screen

- [ ] 5.1 Remix in the deck ☰ menu; Remix deck layout in 2-Deck (WHEEL tab: SCENE wheel, ISOLATE knobs with LEVERs, RELEASE; PADS tab: pads, OVERDUB, roll rates), BIG mode over the library in the wheel layout, slim header in 4-Deck; live waveform; screenshots

## 6. Controllers and remote

- [ ] 6.1 MIDI Fighter Twister Remix mapping (bank 1 the first Remix deck, bank 2 decks; ring colours and values; SHIFT layer); mapping tests; supported-controls.txt
- [ ] 6.2 Remote page Remix tab in the wheel layout: isolate knobs into the SCENE wheel, multi-touch levers with latch, release lever, pads and roll-rate strip, BPM/TAP/QUANTIZE; landscape only (rotate prompt when upright), scaled for phones; a latency readout (ms) with a warning when the connection is jittery; OSC addresses listed in the guide

## 7. Docs

- [ ] 7.1 The FX page (Band FX, Release, Remix FX) and the remote page (Remix tab, TouchOSC table); Twister setup (centre detents)
