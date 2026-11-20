# Proposal

Planned for: **v0.9 · In the club** (a user request, issue #923, 2026-11-20)

## Why

The FX bar now uses Pigments-style controls (#908, #920): round power buttons, dark value pills with titled lists, ‹ › inside the name pill, arc knobs. The rest of the window still uses the older boxed buttons, tabs that get cut off ("BEAT JUMF"), unlabeled ▾ menus and plain grey knobs, so the app has two looks. Using one shared kit everywhere makes the window easier to read, gives every on/off and every choice the same shape, and lets a color scheme (add-color-schemes, #921) restyle all of it from `Theme`.

## What Changes

The kit (in `pills.slint` and `controls.slint`, colours and sizes from `Theme`):

- **Power button:** round, lilac ring when on. Only for turning a whole unit on: an FX slot, an instrument deck, Automix.
- **Switch pill:** pressed to turn something on (lilac text) or off (grey), or to step through a few choices (small dots show how many). Replaces power-button-plus-label pairs and ON boxes.
- **Stepper pill:** `‹ value ›`, no ▾. ‹ › step; pressing the value opens its titled list.
- **Value pill:** `value ▾` for long lists, opening a titled list.
- **Unit pill:** a small pill beside a knob's or stepper's label that switches its unit or mode (Rate sync, Filter LP), used where it fits.
- **Arc knob:** a lilac arc for the value; EQ arcs grow from the centre in the waveform band colours.
- **Titled menus:** a bold title naming what is chosen, grey group headings, the current choice tinted lilac.
- **Section titles** in Roboto Condensed (bundled, OFL); values and labels stay in Inter.

The areas, in this order:

1. **Effect menu and FX bar:** the name pill drops its ▾; the effect menu gets the slot as its title, None, two columns of kinds with a white icon per kind (lilac for the current one), openDJ FX / Plugins as pill tabs, and plugins in a second column by maker (with kinds listed too).
2. **Instrument decks:** power, page pills (Sequencer / Synth / Mixer), Kit and Pattern steppers, Scene, Playback and Generation sections, tempo as an arc knob, Fills as a switch pill. Groove Box first, then the Looper and synth decks on the same frame.
3. **Deck controls:** Sync, Key lock and Master as switch pills; Vinyl and Slip as two switch pills by the jog; the pad mode as one stepper (no tabs); square pads; a Loop section with a length stepper, In, Out, Reloop and Jump and no on/off button; Key as a stepper; the tempo range as a value pill. Play and CUE stay boxed.
4. **Pad pages:** each page's own setting on the right of the stepper row (for example Hold/Latch for Pad FX, Mute/Solo for Stems, Roll for Beat Loop), the main word large on each pad with its detail under it.
5. **Mixer:** centre-out EQ arcs in band colours with the value in the label while off centre; the mode pill reads EQ, Sculpt or Stems and steps on press; cue as a round button with openDJ's headphone icon, red ring when on; FX 1 / 2 assign as switch pills.
6. **Menus everywhere** use the titled menu.
7. **Automix panel:** the power button starts and stops Automix (replacing AUTOMIX and Start); Length as a stepper, Style as a switch pill, Repeat and Skip played as switch pills. The track list does not change.
8. **Settings:** steppers for numbers, value pills for named choices, switches kept for saved on/off settings, in lilac.
9. **Top bar,** last and only after real screenshots are reviewed: Rec as a pill showing the running time while recording; Mic as one switch pill with its settings on right-click.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-layout`: one shared set of controls across the window

## Out of Scope

- The track list, columns, search and sidebar.
- Colour per effect kind (icons stay white with lilac).
- Copying an effect between slots.
- Light mode values for the new pieces beyond what `Theme` already gives.

## Difficulty

**Large: about nine PRs, one per area.** Each is mostly UI: the controls already exist in the engine. Each PR checks Dark and Light by screenshot and keeps controller mappings and Map mode marks working.

## Performance

No change per frame. Arc knobs and pills are simple paths and rectangles, as in the FX bar today.
