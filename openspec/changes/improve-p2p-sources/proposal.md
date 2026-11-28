# Proposal

Planned for: **v0.9.5** (issue #1149)

## Why

Soulseek results today rank by format, then a free upload slot, then queue length, then the speed a user advertises. A user with a short queue and a slow line can still hold a download for an hour, many users advertise speeds they never keep up, and a user whose last three downloads failed ranks like anyone else. The Downloads list says "12th in line", which says little about how long that is. Karaoke, sped-up and cover copies sit among the real songs. When slskd loses its Soulseek sign-in, every waiting download fails at once and has to be retried by hand, often from another user. Nothing checks that a FLAC is really lossless. Music servers that can find songs that sound alike (Navidrome's sonic similarity) have no way in.

## What Changes

- **Readiness ranking** (Soulseek add-on): each user's estimated wait (their queue × about 40 MB ÷ their speed; none with a free slot), using the speed measured on recent downloads from them over what they advertise, plus ten minutes for each recent failed download from them. Within the same quality, results sort by wait (whole minutes), then speed. Kept in memory for an hour.
- **Ready column** on Soulseek results ("Now · 2.1 MB/s", "~4 min wait", "~1 h+"), green, amber or grey; folders show it too, with songs found against the album's count. Downloads say "~4 min wait" instead of the place in line once a speed is known.
- **Unwanted copies**: karaoke, nightcore, sped up, slowed, 8D, cover, tribute, lullaby and parody copies (and "in the style of", "made famous", "originally performed", "piano version") rank after other versions and fold into a closed "Other versions" group, unless the search names them. Remixes, edits and extended mixes stay in the list, after the original.
- **Sign-out pause**: while slskd is signed out, a download whose transfers fail is held as waiting ("Paused · Soulseek signed out"), not failed, and not counted against the other user; once slskd is signed in again it is asked for again from the same user. Downloads shows a banner while any are held.
- **Fake-lossless check**: a finished FLAC, WAV, AIFF or ALAC download, and any lossless track on Analyze, gets its spectrum's cutoff measured; below 19.5 kHz (at 44.1 kHz or more) it is flagged: an amber "Not real lossless" chip on Downloads, a Lossy tag, and a Health list.
- **Find and source order**: the `add-download-search` change, moved into this release.
- **Music servers**: a song's menu on a Subsonic/Navidrome source gains "Similar artists' songs" (`getSimilarSongs2`).

## Out of scope

- Torrent and Usenet sources, and web services that rip DRM-protected stores.
- Keeping peer observations across restarts.
- Audio-similarity "Sounds like this" and "Path to another song" from Navidrome's sonic-similarity plugin: dropped after review (needs a server plugin few have, and goes by overall sound, not tempo or key).

## Capabilities

### Modified Capabilities

- `library-sources`: readiness ranking and column, unwanted copies, sign-out pause, similar artists' songs from music servers.
- `library-health`: a fake-lossless list.

## Impact

- Soulseek add-on (separate repository): `src/peers.rs` (observations, wait, rank), `src/slskd.rs` (ranking, unwanted copies), `src/lib.rs` (sign-out pause, observations).
- Engine: `crates/engine/src/bandwidth.rs` (spectrum cutoff).
- Plugins API: `Found` gains a readiness line for the Ready column (empty from sources that have none).
- App: `crates/app/ui/sources.slint`, `crates/app/src/sources.rs` (Ready column, Other versions group, sign-out banner, fake-lossless chip), Health, `crates/plugins/src/subsonic.rs` (`getSimilarSongs2`).
