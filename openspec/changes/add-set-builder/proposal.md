# Proposal

Planned for: **v0.9** (2026-11-17, a user request; issue #851)

## Why

Building a set for a gig, a long Automix night or a mix to publish is done by hand today: pick crates, scroll, sort by BPM, check keys, add up minutes. A ceremony playlist builder seen in a review builds long timed playlists from the user's own playlists as a "recipe" of segments with minutes each. The same idea fits DJ sets: describe the shape of the set and how long it runs, pick the crates, and let openDJ fill it with tracks that fit the intensity, tempo and key, then send the result where it is needed.

openDJ already has the parts: crates and tag groups (Trip Stage), star ratings, BPM and key analysis, Automix's Harmonic path and Smooth flow ordering, its styles and durations, the Mixed Set Editor timeline with the add-set-view render path, and Export to Spotify Playlist.

## What Changes

- **Set Builder source in the rail**, beside Albums, Recordings and Jam. Its tree lists the DJ's sets (drafts, newest first, folders allowed) with + New set. A set stays in Set Builder until it is sent somewhere, and remembers where it was sent.
- **A wizard in the browser panel**, three pages with Back and Next:
  1. **What and how long:** Name your set (blank names it from shape, length and day), Duration (a length or an end time), Crates (none ticked means the whole Collection), and a shape: Warm-up, Peak hour, Journey, Sunset, Wave or Flat. Build now is on this page and uses the shape's defaults for the rest.
  2. **Feel:** the shape's segments as blocks in a row, edges dragged to change minutes. Each segment has Trip Stage chips and an intensity star range. Set-wide, each with a per-segment override where it makes sense: BPM (Full: each track close to the last and following the segment's range or ramp; Semi: the range is a guide; Off), Harmonic (Off, or fuzz 0, 1 or 2 semitones), and Mixing (Automix style Smooth, Bass swap, Filter or Cut, and duration 0 s, 12 s, 15 s, 30 s, 16 bars, 32 bars or Intro over outro).
  3. **Review:** the built tracks under segment headings, each with why it was picked (crate, stage, stars, BPM step, key step), Swap (three alternatives for the same slot), Lock, Remove and Rebuild (keeps locked tracks), and planned against built time.
- **Intensity is the star rating:** 1 lowest, 5 highest. Unrated (0) tracks may fill any segment, only after rated tracks that match; the review says how many unrated tracks were used.
- **Picking** scores each candidate on the segment's filters and on the step from the track before (Camelot distance with the chosen fuzz, the same rule as the browser's ±1/±2 key marks; BPM step), and orders with a small beam search instead of a greedy pick. One recording appears once per set. A segment that cannot be filled says which filter ran out.
- **Send to…**
  - **Collection:** a list (or crate) with the set's tracks in order.
  - **Automix:** replaces or appends to the Automix list with the segments as headings; each transition uses its segment's style and duration. Refill from here is a button that rebuilds the rest of the list in the same shape to the same end time. The list never changes by itself.
  - **Set Editor:** opens the set in the Mixed Set Editor as a planned set with no recording; transitions start from pair mixes, then track Automix points, then phrases, in the segment's style. Editing and rendering use the add-set-view tools.
  - **Spotify:** a private playlist in this order (Export to Spotify Playlist).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `library-sources`: the Set Builder source and its wizard
- `automix`: play a set from Set Builder with per-segment mixing, Refill from here

## Out of Scope

- Rules from play history (unplayed first, not played recently): decided against.
- Automix re-planning by itself when the DJ takes over: decided against.
- A stored automatic energy number; stars carry intensity.
- Drawing a free energy curve; shapes and segment blocks come first.

## Impact

library (sets and recipes tables), app (rail source, wizard, picker), automix (segment headings, per-segment mixing, refill), Mixed Set Editor (planned sets with no recording; depends on add-set-view's render path).
