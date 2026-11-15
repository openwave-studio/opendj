# Proposal

Planned for: **v0.8 · From the booth to the studio** (2026-11-15, a user request after reviewing community PR #769 for the DDJ-400 and comparing Beat FX on controllers with rekordbox)

## Why

Every Pioneer-style controller drives the same Beat FX unit (`[EffectRack1_EffectUnit5]`), but each mapping does it in its own way, and the screen doesn't keep up:

- **Stepping skips around the list.** `next_effect`, `prev_effect` and `effect_selector` walk the effects in their internal ID order (Echo, Reverb, Filter, Flanger…), while the list on screen is grouped by kind and sorted A to Z. FX SELECT on a DDJ-FLX4, DDJ-FLX6 or DDJ-400, and the ‹ › arrows in the FX bar, jump around the list. In rekordbox, FX SELECT walks the list in the order it is shown.
- **Hardware changes are easy to miss.** When FX SELECT changes the effect, only the name changes. A DJ looking at the controller can't see what comes before or after it.
- **MASTER is handled in different ways.** CH SELECT on MASTER turns on every deck on the DDJ-FLX10 and sends to `[Master]` on the DDJ-800. The Beat FX routing row only has 1, 2 and 1&2, so MASTER lights nothing on screen.

## What Changes

1. **One effect order.** `next_effect`, `prev_effect` and `effect_selector` step through the effects in the list's order (built-in effects by kind, then A to Z, then the ticked plugins in the same order as the Plugins tab), and wrap at either end. Built-in `loaded_effect` IDs stay the same, so mappings that pick an effect by number keep working. Plugin IDs follow the Plugins tab's order. Effect choices aren't saved across restarts, so nothing saved changes meaning. This applies to every controller and to the on-screen arrows.
2. **Show the change on screen (option B).** When an effect changes from anything other than the slot's own list (a controller, the ‹ › arrows, OSC), its name glows in the accent color and a slim strip under the Beat FX shows the previous, current and next effect for about 2 seconds, then fades. Clicking the name still opens the full list. No mapping has to ask for it.
3. **MST in the routing row.** The Beat FX routing row becomes 1 · 2 · 1&2 · MST (4-Deck: 1 · 2 · 3 · 4 · MST). MST sends the effect to the main mix (`group_[Master]_enable`) and turns off the deck routes.
4. **The same Beat FX on every Pioneer mapping.** The DDJ-FLX4, DDJ-FLX6, DDJ-FLX10, DDJ-800, DDJ-400, XDJ-RX3 and XDJ-AZ scripts send CH SELECT MASTER to `[Master]`, and keep the same controls: FX SELECT (SHIFT: back), BEAT ◀ ▶ halve and double (SHIFT: AUTO, TAP), LEVEL/DEPTH as depth, ON/OFF (SHIFT: release).

The DDJ-400 mapping itself comes from community PR #769, cut down to that controller.

## Capabilities

### Modified Capabilities

- `effects`: effects step in list order, effect changes from outside the list show on screen, and the Beat FX routing row has MST

## Out of Scope

- Beat lengths over 4 beats (Pioneer goes to 16 on echo-type effects), and new effects such as Ping Pong or Helix: issue #713.
- Opening the full effect list from a controller.
- Blinking ON/OFF lights while the effect is on.

## Impact

`crates/engine` effects.rs (stepping order); `crates/app` audio_plugins.rs (plugin order), ui/fxbar.slint (glow and strip, MST button), ui/deck.slint (deck FX names glow too); `res/controllers` Pioneer scripts (MASTER); tests in `crates/controllers/tests/mappings.rs`; the FX page of the user guide.

## Difficulty

Small to medium: about two PRs.
