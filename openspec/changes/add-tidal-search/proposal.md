# Proposal

Planned for: **v0.8**. Issue #838, a user request (2026-11-17).

## Why

The TIDAL source only lists the DJ's own playlists and favourites, so finding a track on TIDAL means leaving openDJ. TIDAL's API has catalog search, and the DJ's own TIDAL app can read it once it asks for the `search.read` scope. Add-ons can now play streamed rows through the source cache (#836), and an add-on that does only that should not take a second place in the rail beside the service it works for.

## What Changes

- **TIDAL search**: in the TIDAL source, typing two or more letters in the search box lists TIDAL's catalog tracks for them (up to 40, best first) in place of the playlist, once typing pauses. Clearing the box shows the playlist again. Found tracks look and act like playlist tracks: greyed when the collection doesn't have them, matched by ISRC, Add to Wanted, Download Track, and double-click.
- **Sign-in scope**: the TIDAL sign-in asks for `search.read` too. A DJ signed in before signs in again to search; the error says so.
- **Add-ons out of the rail**: an add-on source can say it has no place in the rail (`in_rail`, `rail` in the protocol). It still runs, and plays the rows of the service whose links it takes.
- **Spec**: a streaming source still never plays streamed audio itself; an add-on that plays may load its rows, as #836 built.

## Out of scope

- Searching albums, artists or playlists on TIDAL.
- Search for the other streaming services.

## Capabilities

### Modified Capabilities

- `library-sources`: TIDAL catalog search; streamed rows may play through a playing add-on.
- `add-ons`: add-ons that play, and add-ons kept out of the rail.
