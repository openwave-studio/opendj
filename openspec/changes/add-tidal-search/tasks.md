# Tasks

## 1. Add-ons

- [x] 1.1 Add-on protocol carries `plays`; double-clicking a streamed row loads it through a playing add-on that takes its site's links (#836)
- [x] 1.2 Add-on protocol carries `rail`; sources with `in_rail` false stay out of the rail

## 2. TIDAL search

- [x] 2.1 `search.read` in the TIDAL sign-in scopes; catalog search for tracks in the worker
- [x] 2.2 Typing in the TIDAL source lists the catalog's tracks in place of the playlist after a short pause; clearing shows the playlist again
- [x] 2.3 Checked against a real TIDAL account on macOS

## 3. After the macOS check (issue #864)

- [x] 3.1 Search uses `GET /searchResults?filter[query]=…`, then the result's tracks by its id
- [x] 3.2 A dropdown under the search box lists artists, albums and tracks; an artist or album opens its tracks, a track loads onto a free deck, ↵ lists every track
- [x] 3.3 Add-ons can browse a service's catalog for the app's own source (`catalog`, `source.browse`); the TIDAL source signs in, lists and searches through one
- [x] 3.4 Streamed rows download through a playing add-on into the collection (Inbox), take the streamed track's place in its lists, and load onto a deck that shows their title, artist and progress; double-click, Download on the menu, or a drag onto a deck
- [ ] 3.5 Checked against a real TIDAL account on macOS
