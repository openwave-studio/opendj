# Design

## Signal path

The Remix deck is a deck kind like the Groove Box. Its input is a copy of its source taken after the source's EQ and before its fader (Send), or the source itself with the source's own channel muted (Insert). Its output goes through its own channel (EQ, fader, crossfader, deck FX). Inside:

```
in ─► BandSplit (LR4 246 Hz / 2.48 kHz + all-pass)
        ├─ low  ─► isolate gain/effect ─► lever effect ─┐
        ├─ mid  ─► isolate gain/effect ─► lever effect ─┼─► sum ─► scene chain ─► release stage ─┐
        └─ high ─► isolate gain/effect ─► lever effect ─┘                                       ├─► out
                         sample pads ─► sample effect ──────────────────────────────────────────┘
```

- Bands with nothing on skip their effects. In Send mode the deck plays only what each lane adds (lane output minus its band) and pads, so idle it adds exactly nothing. Turning a band down (ISOLATE left, Solo) does nothing in Send: the source's channel still plays the band, and the band played upside down would not cancel it because the crossovers shift its phase. In Insert mode the idle deck passes the source through bit-identical (tested), and fades over 5 ms to the bands' sum when anything comes on.
- Source: `[RemixN],source` 1-4 a deck (not itself), 5 mic, 6-13 aux 1-8; `[RemixN],mode` 0 Send, 1 Insert. Tempo from the source deck's grid, else the sync leader.
- The split is the channel `Isolator`'s `split()`: low = LP(246), mid = LP(2484)·HP(246), high = HP(2484), low through the upper crossover's all-pass so the sum is flat. In the mixer the Remix deck runs after every other deck in the frame loop, so its source's frame (after EQ, filter and sculpt) is ready; in Insert it then zeroes the source's frame.

## Band FX on slots

`[EffectRack1_EffectUnitN_EffectM],band` is a bit mask: 0 all (today), 1 low, 2 mid, 4 high, any sum. When non-zero the slot runs its effect on the sum of the chosen bands and adds the rest dry. Wet/dry of the effect applies to its bands only. Plugin slots: the helper process gets the band-limited buffer; the dry bands are added back after the round trip and delayed by the plugin's reported latency (zero today).

## Isolate knob

`isolate_low|mid|high` in -1..1, 0 = centre. Below 0 the band's gain follows the channel EQ's kill curve (−∞ at −1). Above 0 the Isolate effect runs on that band with depth = value; the band stays at unity. A dead zone of ±0.02 keeps a detented knob exactly off.

Isolate effects (one choice, `isolate_effect`): Tape Echo (echo with gentle saturation and wow in the feedback), Reverb, Drive (oversampled, the existing Drive), Filter (a resonant band-pass sweep inside the band, from its low edge to its high edge as the knob turns), Ducker (a beat-synced volume envelope: dips on each beat over 3 ms and recovers over 1/2 beat, 1/4 to 1 beat in a slot), Rhythm (cuts the band into 1/8-beat slices, 16 to a cycle, and plays each from the same place or up to 4 slices back by a fixed pattern, four patterns; each jump crossfades over 3 ms, and reaching back only keeps it inside the 2 s delay memory). The same three, and Solo (plays only the slot's Band choice), are slot effects too: `loaded_effect` 19-22, so plugins now start at 23.

## Levers

`lever_low|mid|high` in 0..1 (depth); `lever_low|mid|high_on` 0/1. Mappings and the remote implement hold vs latch: press = on, release = off unless it was a double-press (latched; next press releases). Lever effects (`lever_effect`): Echo, Reverb, Repeat (repeats the last 1/2 beat, Roll's buffer), Reverse (plays each beat backwards, Rev Roll), Solo (mutes the other two bands while on), Stretch (slows the band to half speed over a bar and back; signalsmith-stretch, last).

## Scenes

`scene` in -1..1 (±0.02 dead zone); `scene_up` and `scene_down` pick one of five built-in scenes each; `scene_time` (echo length 1/4, 1/2 or 1 beat) and `scene_color` (filters an octave down to an octave up). A scene is data in `effects/scenes.rs`: two effects in series, each with a function from (amount 0..1, time, color) to its parameters and wet amount, e.g. Rise Echo = a band-pass from 400 Hz to 6 kHz into an echo with feedback 0.3 → 0.8 and send 0 → 0.7.

- Build Up: Rise Echo (band-pass + Echo), Crush Echo (Bitcrusher + Echo), Spiral Up (high-pass + Spiral), Noise Riser (high-pass + Noise), Roll Up (high-pass + Roll from 1/2 to 1/16 beat).
- Break Down: Dub Down (low-pass closing + Tape Echo), Wash Out (high-pass + Hall), Filter Down (Ladder closing + Gate), Gate Down (1/8-beat Gate + low-pass), Crush Down (downsampling + low-pass).
- The scene is picked when SCENE leaves centre and kept while it sounds: choosing another scene, or turning the knob to the other side, only takes effect once the first has rung out. The two instances are the Remix deck's own (with their delay memory), not the rack's pool, so a scene never fails to start.
- After the lanes, in both modes. In Send only what the scene's echoes and reverbs add is played: its filters, gates and crushers colour what feeds the echo but can't take anything out of the source's channel.

## Release

`release` (button): every effect on the Remix deck and its source stops taking input on the next sample. Mode `release_mode`: 0 Echo (tails keep ringing; wet-only for up to 8 s), 1 Brake (deck slows to stop over `release_time` beats via the Brake effect on the main path), 2 Spin (Backspin effect for one beat, then the music resumes), 3 Cut (music muted while held, effects stay, all off on release). `[Master],fx_release` does the same on every channel.

## Sample pads

`pad1..4` (press/release), `pad_bank` (sample pack bank or a Groove Box kit), `roll_rate` (0 off, 1-8 the rates), `roll_rate2` (second held rate for groove rolls: hits on both grids, accents where they meet), `pad1..4_pitch` (±12 semitones), `overdub` (toggle; a 1-bar loop recorded on the grid, layered, `overdub_undo` removes the last pass, `overdub_clear`), `sample_fx` (choice) and `sample_fx_amount`. Hits play on the Remix deck's own eight voices, mixed into its output after the band split, so they follow its fader. On screen the X-PAD sits under the ISOLATE arrow on the WHEEL screen: the roll rates in a strip to slide along, over the four pads.

## Controllers and remote

- Twister mapping: own JS mapping, bank 1 = the first deck with Remix loaded, bank 2 = 4-deck as today. Encoders absolute CC on channel 1; pushes on channel 2; ring colour (channel 2 values) and ring value fed back from the controls at 30 Hz. Centre detent for ISOLATE and SCENE is a Twister utility setting; the guide says how.
- Remote: a Remix tab in `crates/remote/page.html`; controls go over the existing WebSocket as `[RemixN],key` sets for the Remix deck; multi-touch with pointer events (one pointer per lever, pads and roll pad at once); actions are quantized in the engine, so Wi-Fi latency only moves a hit to the next grid point. Layout switches between tablet (≥ 900 px wide) and phone (stacked: levers + release, then pads).
- OSC: `/remixN/<key>` come for free from the control names.
