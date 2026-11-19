# Tasks

## 1. Band FX

- [x] 1.1 engine: `band` mask on every slot (0 whole signal, 1 low, 2 mid, 4 high); the chosen bands split at the isolator's crossovers and the rest (input minus the chosen bands) added back, so the bands sum exactly; plugin slots get the band-limited buffer; tests (flat sum with the effect bypassed, only the chosen band changes)
- [x] 1.2 app: a third FX bar row with a Band value pill and titled list on every 2-Deck slot and the Beat FX, an icon-only pill on 4-Deck rows; band colours as the waveform's low, mid and high; screenshot

## 2. Release

- [x] 2.1 engine: `[Master],fx_release` (all units at once) and `fx_release_mode` (Echo, Brake, Spin, Cut) with the main mix's brake, spin and cut; tests
- [x] 2.2 app: Release value pill and hold-to-release button in the Beat FX row
- [ ] 2.3 `[RemixN],release`, `release_mode`, `release_time` on the Remix deck (with step 3); Backspin as a slot effect (needs a slot that keeps history while off)
- [ ] 2.4 Shift + Beat FX ON releases on Pioneer mappings where the button is free

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
