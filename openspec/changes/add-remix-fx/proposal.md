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
2. **Release.** `[RemixN],release` turns off every effect on the Remix deck and its source (Remix lanes and scene, Beat FX and deck FX on both, Pad FX, sample rolls) at once; `[Master],fx_release` does it for every channel. Delays and reverbs keep ringing out (Echo mode), or the music slows to a stop (Brake), spins back (Spin) or cuts until the button is let go (Cut). A new **Backspin** effect joins the list.
3. **The Remix deck.** One Remix deck at a time (picking Remix on another deck moves it there). Remix loads onto a deck from the deck's ☰ menu, beside Track, Groove Box, Instrument and Mixer, the way hardware remix stations sit on a mixer's send/return. The deck **listens to** one source, reassignable at any time (another deck, the mic or an aux input), and plays the result on its own channel: the channel fader is the return level, and its EQ, crossfader side, deck FX and live waveform work as on any deck. **Send** mode leaves the source playing dry and adds Remix on top; **Insert** mode takes the source off its own channel so it is heard only through Remix. Controls live in `[RemixN]` (N = the deck). Tempo and beat come from the source deck's grid, or the sync leader for an input. The deck holds:
   - **Three band lanes, LOW · MID · HIGH.** An **ISOLATE** knob per band with a centre detent: left lowers the band to silence, right adds the Isolate effect on that band. A **LEVER** per band: the Lever effect on that band while held, or latched on. One Isolate effect and one Lever effect are chosen for all three bands.
   - Isolate effects: Tape Echo, Reverb, Drive, Filter, Ducker, Rhythm. Lever effects: Echo, Reverb, Repeat, Reverse, Solo, Stretch.
   - **SCENE knob**: left of centre is Break Down, right is Build Up. Each side runs one scene, a small preset of existing effects whose amounts follow curves as the knob turns (Build Up: Rise Echo, Echo, Noise, Spiral Up, Reverb Up; Break Down: HPF Echo, LPF Echo, Crush Echo, Spiral Down, Reverb Down). TIME and COLOR knobs.
   - **RELEASE** with its mode.
   - **Four sample pads**, played out of the Remix deck's own channel: hits are quantized to the grid; hold a pad and pick a rate (1/2, 1/4, 1/8, 1/16, 1/32, 1/6, 1/12, 1/24) to roll it; two rates held at once give a groove roll; PITCH per pad; **OVERDUB** records hits into a 1-bar loop that keeps playing; one **sample effect** (Echo, Space, Filter, Pitch, Decay, Swing) on the pads only. The default bank uses the Groove Box's Boom kit (synthesised, nothing to license); any sample pack bank can be picked.
4. **New effects**, usable in any slot as well: Ducker, Repeat, Solo, Rhythm, Tape Echo, Rise Echo, Crush Echo, Spiral Down, Backspin, and later Stretch.
5. **The Remix deck on screen.** In 2-Deck the deck area shows the source chooser and Send/Insert, the three band lanes (ISOLATE knob and LEVER each), the SCENE wheel, the pads with rates and OVERDUB, and RELEASE; the waveform strip shows the deck's live output. 4-Deck shows a slim header with the source, SCENE and RELEASE. A layout echoing classic remix stations (isolate knobs pointing into a large scene wheel) in openDJ's own colours and shapes.
6. **MIDI Fighter Twister Remix mapping.** A bundled mapping whose Remix bank plays the first deck with Remix loaded: row 1 ISOLATE LOW/MID/HIGH + SCENE, row 2 LEVER LOW/MID/HIGH (hold the push; double-press latches) + RELEASE, row 3 the four pads (push = hit, hold and turn = roll rate), row 4 Lever effect, Isolate effect, sample effect and the Remix deck's fader. Side buttons: SHIFT (TIME per band, COLOR, pitch per pad), OVERDUB, UNDO, SOURCE, bank ‹ ›. Rings show band colours and values; the existing 4-deck controls move to the next banks.
7. **Remix page in the browser remote.** The phone remote page (REMOTE in the top bar) gets a Remix tab for the Remix deck in the **wheel layout**, landscape only (phones scale it down): the three ISOLATE knobs in a panel pointing into a large SCENE wheel (Build Up scenes on its upper half, Break Down on the lower), BPM, TAP and QUANTIZE on top, a release lever at the right edge, pads and a roll-rate strip along the bottom, multi-touch throughout; levers under the isolate knobs: press and hold = on while held, drag down = on and held, tap to let go. Every `[RemixN]` control also has its OSC address, so TouchOSC layouts work too.

## Capabilities

### Modified Capabilities

- `effects`: band selection on every slot, release of every effect, the Remix deck's effects, scenes, sample pads with rolls and overdub, new effects
- `deck-transport`: Remix as a deck kind with a source and Send/Insert
- `controllers`: the MIDI Fighter Twister Remix mapping
- `phone-remote`: the Remix tab

## Out of Scope

- A shared Remix unit in the FX bar: the Remix deck replaces it (a user decision, 2026-11-19).
- Copying any hardware unit's artwork, effect names or presets one-to-one; in-app text names no other product.
- Mapping hardware remix units themselves (one has no MIDI; the older one is discontinued and untested here).
- An external hardware FX slot (send/return through an audio interface): add-fx-pre-post-and-headphone.
- Saving and sharing user scenes: a later step once FX presets exist.
- The tablet app (`crates/tablet`) running the Remix deck on the iPad itself: later, from the same wheel layout. The browser remote comes first (a user decision, 2026-11-19).

## Impact

`crates/engine`: effects.rs (`band` per slot, BandSplit, release, new effects), new `remix.rs` (deck kind, source send/insert, scenes, pads, rolls, overdub), controls.rs (`[RemixN]`), sampler (pad voices); `crates/app`: ui/fxbar.slint (chips), ui/remix_deck.slint, remix_deck.rs, the deck ☰ menu; `crates/remote` page.html (Remix tab) and its WebSocket controls; `crates/osc` (addresses come from the controls); `res/controllers` Twister Remix mapping; supported-controls.txt; the FX and remote pages of the user guide.

## Difficulty

Large: about eight PRs (band FX; release and Backspin; Remix unit with lanes; new effects; scenes; sample pads; FX bar tab; Twister mapping; remote page). Band FX and Release are useful on their own and land first.

## Performance

A band split is two LR4 crossovers plus an all-pass (the isolator's cost), only on slots with a band chosen. A Remix deck with nothing on copies its source through (Send mode adds silence), so it costs one band split only once a band control moves. Pad rolls reuse sampler voices; no allocation on the audio thread; new delay buffers come from the existing pool.
