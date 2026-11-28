# Proposal

Planned for: **v0.9.5 · Mixing by ear** (a user request, issue #1155, 2026-11-26)

## Why

A DJ with a two-channel controller (a DDJ-FLX4, say) and an iPad wants a third deck without buying hardware. Others do this today with a MIDI bridge and a web page that imitates a controller. openDJ already serves a phone page with a live connection, so the page can drive the decks itself: nothing to map, and the page always matches the app's version.

## What Changes

- The phone page gets tabs: **Automix** and **Deck** (FX later), along the bottom on a phone and in a top bar on wide screens.
- A **Deck** page drives any deck, picked with 1 to 4 (deck 3 at first): play, cue, sync, quantize, slip, reverse, nudge, 8 hot cue pads with Loop and Beat Jump pad modes, loop in/out/size, reloop, beat jump, the pitch fader with key lock, and the deck's channel strip (trim, EQ, filter, headphone cue, volume, level meter). LOAD searches the collection and loads onto the deck.
- The deck's waveform is drawn on the page: the track's analysed waveform (low, mid and high peaks, 1000 a second) is fetched once from `/wave/<id>`, the play position streams twenty times a second, and the page moves the waveform on its own clock in between. Beats, bars, hot cues and the loop are drawn on it; an overview shows the whole track.
- The layout follows the screen: an iPad on its side (the main target, in a case), upright, a phone upright and on its side.
- WebSocket API: `deck.set` (an allow-list of deck controls, EQ and filter), `deck.load`; topics `decks`, `deck_pos`, `track1` to `track4`.

## Capabilities

### Modified Capabilities

- `phone-remote`: Deck page, tabs, waveforms over the network.

## Out of Scope

- The FX page (X-Y pads), a Player look with a platter, the library panel with playlists and filters, and the waveform on WebGPU: later steps of this change.
- Scratching or jog wheels from the page (Wi-Fi adds 10 to 30 ms).
- Pairing roles: any paired phone can use every page for now.

## Impact

`crates/remote` (waveform route, higher message rate, page), app `remote_deck.rs` and `Browser::load_id_onto`, the user guide's Phone remote page.
