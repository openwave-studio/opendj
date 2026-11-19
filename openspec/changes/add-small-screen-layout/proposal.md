# Proposal

Planned for: **v0.9 · In the club** (a user request, issue #906, 2026-11-19)

## Why

openDJ should work on laptops from the last five years. The tightest common screens leave 1440 × 875 (a 13" MacBook Air) or 1280 × 720 (a Full HD Windows laptop at 125 to 150%) for the window. PR #907 lowered the minimum to 1280 × 720 and made the waveforms, pads and FX bar give way on short windows, but the browser's panels have fixed widths: the source rail (52 px), the lists panel (270 px) and the side panel (340 px). At 1280 wide the track list gets 618 px and shows only Title and Artist. Other DJ apps answer small screens with a library maximize button or a splitter, and users of one ask for resizable sections. A Mixer deck with more strips than fit scrolls sideways with nothing on screen saying so.

## What Changes

- Lists panel: clicking the open source's rail icon again folds the panel into the rail. Folded, a click on the rail icon slides it out over the track list; it slides back after a list is picked, on Escape or on a click in the track list. Below 1440 wide it starts folded. Its edge drags from 200 to 360 px.
- Side panel: below 1440 wide it slides in over the track list instead of taking width from it, opened from the footer button as today. Its edge drags from 280 to 480 px. A track dragged to the window's right edge slides it open so the drop lands on Automix or the Sidelist.
- Splitter: a line between the decks and the library drags the library taller or shorter, stepping the waveforms down and then folding the FX bar; a double-click expands or restores the library.
- Mixer deck: an edge fade and a ‹ › pair show that more strips are off to the side.
- UI scale: Settings › Interface gets a scale from 85% to 125%; on first start, a screen whose usable area is under 1280 × 720 gets 90%.
- Widths and folded states are kept per layout; an automatic fold comes back when the window grows, a fold the DJ made stays.

## Capabilities

### Modified Capabilities

- `track-browser`: Side panel, Lists panel
- `app-layout`: Compact view, Window size

## Out of Scope

- A separate small-screen layout: the existing layouts give way instead.
- Docking panels to other edges or into their own windows.
