# Proposal

Planned for: **v0.9 · In the club** (2026-11-19, a user request after a gap analysis of hardware remix effect units against openDJ's FX; issue #886)

## Why

Hardware remix stations sit on a mixer's send/return and turn a few big gestures into whole build-ups, drops and fills: a knob that cuts the bass and adds an echo to it, a lever that throws the highs into a delay while held, one knob that builds tension, a button that releases everything with a tail, and four drum pads that roll on the beat. openDJ already has most of the sounds (Echo, Reverb, Filter Echo, Spiral, Noise, Gate, Roll, Rev Roll, Brake, Echo Out, Drive), but each lives in its own slot with its own controls, always on the whole signal, and nothing releases them together. The gaps are:

- **No effect on one band.** Every effect processes the full signal. Putting an echo on the highs only, or a ducker on the mids, isn't possible.
- **No one-gesture build-up.** A build needs three or four effects turned up together by hand.
- **No release.** Every effect is turned off on its own; there is no "let go of everything, keep the tail".
- **No drum rolls on the beat.** The sampler plays one-shots; the Groove Box plays patterns; nothing repeats a single hit at 1/16 while a pad is held.
- **No surface made for it.** A MIDI Fighter Twister or an iPad could play all of this, but there is nothing to map to.

## What Changes

1. **Band FX on every effect slot.** Each slot of every unit (deck FX, Beat FX, Pad FX, Mic/Aux) gets a `band` control and L · M · H chips in the FX bar. With none lit the effect works as today. With some lit, the slot splits its input with the same Linkwitz-Riley crossovers as the channel isolator (246 Hz, 2.48 kHz), runs only the chosen bands through the effect and adds the others back dry, so a plugin in the slot hears only its band too.
2. **Release.** `[RemixFX],release` (also on every unit as `release_all` through `[Master],fx_release`) turns off every effect on the channels the Remix FX is routed to (Remix FX, Beat FX, the deck units, Pad FX, sample rolls) at once. Delays and reverbs keep ringing out (Echo mode), or the music slows to a stop (Brake), spins back (Spin) or cuts until the button is let go (Cut). A new **Backspin** effect joins the list.
3. **The Remix FX unit (`[RemixFX]`).** One shared unit, routed like Beat FX (`route`: 1-4 a deck, 5 decks 1&2, 6 main), with:
   - **Three band lanes, LOW · MID · HIGH.** An **ISOLATE** knob per band with a centre detent: left lowers the band to silence, right adds the Isolate effect on that band. A **LEVER** per band: the Lever effect on that band while held, or latched on. One Isolate effect and one Lever effect are chosen for all three bands.
   - Isolate effects: Tape Echo, Reverb, Drive, Filter, Ducker, Rhythm. Lever effects: Echo, Reverb, Repeat, Reverse, Solo, Stretch.
   - **SCENE knob**: left of centre is Break Down, right is Build Up. Each side runs one scene, a small preset of existing effects whose amounts follow curves as the knob turns (Build Up: Rise Echo, Echo, Noise, Spiral Up, Reverb Up; Break Down: HPF Echo, LPF Echo, Crush Echo, Spiral Down, Reverb Down). TIME and COLOR knobs.
   - **RELEASE** with its mode.
   - **Four sample pads**: hits are quantized to the routed deck's grid; hold a pad and pick a rate (1/2, 1/4, 1/8, 1/16, 1/32, 1/6, 1/12, 1/24) to roll it; two rates held at once give a groove roll; PITCH per pad; **OVERDUB** records hits into a 1-bar loop that keeps playing; one **sample effect** (Echo, Space, Filter, Pitch, Decay, Swing) on the pads only. The default bank uses the Groove Box's Boom kit (synthesised, nothing to license); any sample pack bank can be picked.
   - Tempo and beat come from the routed deck's grid, or the sync leader on MST.
4. **New effects**, usable in any slot as well: Ducker, Repeat, Solo, Rhythm, Tape Echo, Rise Echo, Crush Echo, Spiral Down, Backspin, and later Stretch.
5. **FX bar REMIX tab.** The FX bar gets DECK FX · REMIX tabs. REMIX shows routing, SCENE, the three band lanes, the sample pads with rates, and RELEASE (mockups in the plan PR).
6. **MIDI Fighter Twister Remix mapping.** A bundled mapping with a Remix bank: row 1 ISOLATE LOW/MID/HIGH + SCENE, row 2 LEVER LOW/MID/HIGH (hold the push; double-press latches) + RELEASE, row 3 the four pads (push = hit, hold and turn = roll rate), row 4 Lever effect, Isolate effect, sample effect and Remix level. Side buttons: SHIFT (TIME per band, COLOR, pitch per pad), OVERDUB, UNDO, ROUTE, bank ‹ ›. Rings show band colours and values; the existing 4-deck controls move to the next banks.
7. **Remix page in the browser remote.** The phone remote page (REMOTE in the top bar) gets a Remix tab laid out for iPads in landscape and usable on phones: three multi-touch levers that spring back (drag below the notch to latch), ISOLATE sliders with a centre snap, a SCENE strip, a roll pad (left to right = rate, up = pitch) beside the four pads, RELEASE. Every `[RemixFX]` control also has its OSC address, so TouchOSC layouts work too.

## Capabilities

### Modified Capabilities

- `effects`: band selection on every slot, release of every effect, the Remix FX unit, scenes, sample pads with rolls and overdub, new effects
- `controllers`: the MIDI Fighter Twister Remix mapping
- `phone-remote`: the Remix tab

## Out of Scope

- Copying any hardware unit's look, effect names or presets one-to-one; in-app text names no other product.
- Mapping hardware remix units themselves (one has no MIDI; the older one is discontinued and untested here).
- An external hardware FX slot (send/return through an audio interface): add-fx-pre-post-and-headphone.
- Saving and sharing user scenes: a later step once FX presets exist.
- The tablet app (`crates/tablet`) showing the Remix page: later, from the same layout.

## Impact

`crates/engine`: effects.rs (`band` per slot, BandSplit, release, new effects), new `remix.rs` (unit, scenes, pads, rolls, overdub), controls.rs (`[RemixFX]`), sampler (pad voices); `crates/app`: ui/fxbar.slint (chips, REMIX tab), remix_fx.rs; `crates/remote` page.html (Remix tab) and its WebSocket controls; `crates/osc` (addresses come from the controls); `res/controllers` Twister Remix mapping; supported-controls.txt; the FX and remote pages of the user guide.

## Difficulty

Large: about eight PRs (band FX; release and Backspin; Remix unit with lanes; new effects; scenes; sample pads; FX bar tab; Twister mapping; remote page). Band FX and Release are useful on their own and land first.

## Performance

A band split is two LR4 crossovers plus an all-pass (the isolator's cost), only on slots with a band chosen. The Remix unit is idle (no processing) while nothing in it is on, like an empty slot. Pad rolls reuse sampler voices; no allocation on the audio thread; new delay buffers come from the existing pool.
