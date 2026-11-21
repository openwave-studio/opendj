# Proposal

Planned for: **v0.9 · In the club** (issue #763, a user request, 2026-11-19)

## Why

The browser can only be driven from the keyboard in part: ↑ ↓ move through the tracks, but there is no way to reach the folder tree or the source rail, open a folder or go back up without the mouse. A controller's BROWSE press flips between the tree and the list without opening folders. Outside contribution PR #800 built a shared navigation for keyboard and controllers; this change takes it over and fits its keys to what other DJ apps do.

A comparison of the default keyboard maps of Mixxx, rekordbox, Serato, Traktor, VirtualDJ and djay Pro found: Shift+← / Shift+→ load the picked track onto deck 1 / 2 in Mixxx, rekordbox, Serato and Traktor; Tab moves between browser panes in Serato and VirtualDJ; Traktor opens and closes tree folders with ← →; and every one of them keeps pitch bend on letter keys (rekordbox W/E and U/I), while openDJ had it on the plain arrows.

## What Changes

- The browser has three panes that take the keys: the source rail, the folder tree (Jam's tree in Jam) and the track list (Jam's instruments). The pane with the keys has the bright selection; the others dim.
- ↑ ↓ move in the pane with the keys. With no pane holding the keys, ↑ ↓ move through the tracks and give the list the keys.
- Tab / Shift+Tab move between the panes, skipping one that doesn't show. From the decks, Tab goes to the tracks and Shift+Tab to the tree.
- Tree: → opens a closed folder, then steps into its first child; on a leaf (playlist, crate, folder without children) it goes to the tracks. ← closes an open folder, then steps to its parent, and from a top-level row to the rail.
- Track list: ← goes to the tree. Rail: ↑ ↓ pick the next or previous source in the rail's order, → goes to the tree.
- Shift+← / Shift+→ load the picked track onto deck 1 / 2 (default shortcuts `deck 1 load` / `deck 2 load`, replacing the bigger pitch bend that was on them).
- Pitch bend: W / E bend deck 1 slower / faster while held, U / I deck 2, with Shift for the bigger bend. ← → keep bending the deck started last while no browser pane has the keys; Esc in the browser hands the keys back to the decks.
- In the Prep layout Tab and ← → keep their Prep jobs (phrases and beat jumps); ↑ ↓ still browse.
- Controllers: the BROWSE turn moves in the pane with the keys (signed encoder steps kept), `MoveFocusForward` steps right as → does, `MoveFocusBackward` steps left as ← does, `GoToItem` on a folder steps right. From the track list `MoveFocusForward` goes back to the tree, so mappings with only a press still reach both. DDJ-400 and DDJ-FLX4 mapping descriptions updated.

## Capabilities

### Modified Capabilities

- `track-browser`: Keyboard browsing (new requirement)

## Out of Scope

- A Settings page to rebind the browsing keys (the bend letters and load keys follow the DJ's own shortcuts; the arrows and Tab stay fixed).
- Moving the selection by page (Page Up / Page Down) and type-to-find in the tree.

## Impact

app tree_navigation.rs (new), browser/navigation.rs (new), browser.rs (shared dispatcher, controller [Library] controls), browser/rail_order.rs (rail order for ↑ ↓), ui/browser_focus.slint (new BrowserFocus global), ui/library.slint and ui/instruments.slint (pane focus scopes, tree reveal), ui/main.slint (bend keys, Tab, Esc), shortcuts.rs (load defaults). Mappings: DDJ-400, DDJ-FLX4. User guide: Your collection, Keyboard shortcuts, Decks.
