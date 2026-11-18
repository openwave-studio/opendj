# Proposal

Planned for: **v0.9 · In the club** (a user request, 2026-11-16)

## Why

Moving tracks and lists around the Collection works, but it is blind. A list dragged in the tree can only be dropped onto a folder or a section heading, always goes to the bottom, and leaves the folder closed, so it looks as if it vanished. Lists cannot be put in order at all. Folders do not open while dragging and the tree does not scroll, so nested lists are out of reach in one drag. Tracks cannot be reordered in a playlist, a drop always appends, playlists take the same track twice without a word, and the Delete key only works in the side panel. Moving tracks from one list to another means adding and then removing by hand. The Sidelist is a scratch pad that only takes drags and empties when the app quits.

Other DJ apps answer the "keep the list I am building in view" need with a split browser (rekordbox sub browser, djay Split Library) or by showing a playlist in a side view (VirtualDJ sideview, Traktor Preparation List, Serato Prepare). File managers solved "where will this land?" with an insertion line, spring-loaded folders, edge scrolling, a count badge and a modifier key for move. openDJ takes those patterns without adding panels.

## What Changes

- Tree: each row has three drop zones for a dragged list: before, into (folders and section headings) and after. A lilac line marks before and after, indented to the level it lands at; a lilac outline marks into. Lists keep a free order the DJ sets. A list dropped into a folder goes first in it. A closed folder opens after hovering 0.7 s with a drag, and the tree scrolls when a drag nears its top or bottom edge. After a move the list is selected and its folder open. A refused drop (a playlist under Crates) shows no line.
- List menu: Move to… with the folders on the list's side, plus Move Up and Move Down (⌥⌘↑ / ⌥⌘↓, Ctrl+Alt elsewhere, while the tree has the keys; elsewhere those keys keep changing the waveform height).
- Sidelist: a ▾ picker at its top shows the Scratch pad (today's Sidelist) or any of the DJ's playlists or crates; a list can also be dragged from the tree onto the Sidelist tab, or picked with Show in Sidelist on its menu. A list shown there takes drops at the insertion line, reorders by drag and removes with Delete, saving to the list at once. The Scratch pad is kept across restarts and can be saved as a playlist. A key adds the picked tracks to the end of whatever the Sidelist shows.
- Moves: dragging tracks between lists adds them, as today; holding ⌥ (Alt) moves them, taking them out of the list they came from. The drag shows "Add N tracks" or "Move N tracks". Stages keep moving.
- After any add, move or remove a note at the bottom of the track list says what happened, with Undo (⌘Z too). Tracks already in a playlist are skipped unless the DJ picks Add them again.
- The main track list: Delete removes the picked tracks from the open playlist or crate; tracks in a playlist sorted by # reorder by drag, with a drop line.

## Capabilities

### Modified Capabilities

- `track-browser`: Tree, Drag and drop, Side panel

## Out of Scope

- A second full browser (split view). Revisit only for streaming sources if the Sidelist falls short.
- Cut and paste of tracks between lists.
- The Sets section and set templates (dropped 2026-10-30).

## Impact

app ui/library.slint (tree drop zones, side panel picker, drag badge, note), browser.rs (list_move, move_list, side lists, drops), library sets.rs (positions, move before or after a sibling), library lib.rs (playlist reorder and remove at positions), settings (scratch pad), shortcuts (Sidelist add, list nudge). User guide: Browser.
