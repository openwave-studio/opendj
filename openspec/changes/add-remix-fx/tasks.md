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

- [x] 3.1 engine `effects/remix.rs`: Remix deck kind (one at a time; moving keeps settings; a track, input, Groove Box or Looper takes the deck back), `[RemixN]` with `source` (1-4 decks, 5 mic, 6-13 aux) and `mode` (Send, Insert), isolate and lever lanes (`isolate_low|mid|high`, `lever_low|mid|high` depth, `lever_low|mid|high_on`), `isolate_effect` (Echo, Reverb, Drive, Filter so far), `lever_effect` (Echo, Reverb, Repeat, Reverse, Solo); idle Send adds nothing, idle Insert is bit-identical; turning bands down works in Insert only; tests
- [x] 3.2 new effects: Tape Echo, Ducker and Rhythm as Isolate effects (Tape Echo replacing plain Echo), and Tape Echo, Ducker, Rhythm and Solo in any slot too (Repeat in a slot is Roll); bench numbers (ns per frame at 48 kHz: Tape Echo 70, Ducker 8, Rhythm 33, Solo 7)
- [x] 3.3 scenes (`effects/scenes.rs`): `scene`, `scene_up`, `scene_down`, `scene_time`, `scene_color`; Build Up: Rise Echo, Crush Echo, Spiral Up, Noise Riser, Roll Up; Break Down: Dub Down, Wash Out, Filter Down, Gate Down, Crush Down; the scene is picked when SCENE leaves centre and kept until it rings out; in Send only its echoes and reverbs add; tests
- [ ] 3.4 Stretch lever effect

## 4. Sample pads

- [x] 4.1 engine (`effects/remix_pads.rs`): `pad1..4` (kick, snare, clap, hat), hits on the next 1/4 beat (at once just after one, or without a moving grid), `roll_rate` (1 to 1/16 beat), `roll_rate2` groove roll (both grids, louder where they meet), `pad1..4_pitch`, `pad_bank` (a Groove Box kit); the deck's own eight voices; tests
- [ ] 4.2 overdub 1-bar loop with undo and clear; `sample_fx`, `sample_fx_amount`; sample pack banks on `pad_bank`

## 5. On screen

- [x] 5.1 Remix in the Jam list (dropped on a deck like the Looper; ⏏ turns the deck back into a track deck); the Remix deck in 2-Deck: slim title (source pill, SEND/INSERT), WHEEL screen (Isolate and Lever effect pills, ISOLATE knobs with LEVERs that latch on a tap and spring back when held, SCENE wheel with the scenes round its ring, TIME and COLOR); screenshot
- [ ] 5.2 RELEASE on the WHEEL screen (with 2.3); PADS tab: pads, OVERDUB, roll rates (with step 4); BIG mode over the library in the wheel layout; slim header in 4-Deck and 2-Deck Basic; live waveform; screenshots
- [x] 5.3 the WHEEL screen on a grid: Isolate and Lever pickers and TIME over the ISOLATE arrow (its tip reaching the SCENE wheel, centred on it) and COLOR, over the X-PAD (roll rates to slide along over four pads) and KIT; screenshot

## 6. Controllers and remote

- [ ] 6.1 MIDI Fighter Twister Remix mapping (bank 1 the first Remix deck, bank 2 decks; ring colours and values; SHIFT layer); mapping tests; supported-controls.txt
- [ ] 6.2 Remote page Remix tab in the wheel layout: isolate knobs into the SCENE wheel, multi-touch levers with latch, release lever, pads and roll-rate strip, BPM/TAP/QUANTIZE; landscape only (rotate prompt when upright), scaled for phones; a latency readout (ms) with a warning when the connection is jittery; OSC addresses listed in the guide

## 7. Docs

- [ ] 7.1 The FX page (Band FX, Release, Remix FX) and the remote page (Remix tab, TouchOSC table); Twister setup (centre detents)
