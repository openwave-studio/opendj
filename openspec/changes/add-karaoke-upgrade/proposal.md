# Proposal: Karaoke Upgrade

Planned for: **v0.8 · From the booth to the studio** (a user request after reviewing community PR #773, 2026-11-15; issue #772)

## Why

The karaoke screen (`add-karaoke-screen`, v0.6) shows only the words on a dark background. On some screens the room wants to see the record too: the cover and what is playing, the way streaming apps show lyrics beside the artwork. On others, words only is still best. The sung-word color is fixed to lilac, which clashes with some venues' lighting and some covers.

## What Changes

- **Highlight color**: Settings › Appearance › Karaoke › Sung words picks the color the sung words and count-in dots fill with: Lilac (default), Ocean, Mint, Amber, Rose or White. It applies live to the window and the web page.
- **On-screen buttons**: Cover, Lyrics and Full screen buttons at the top right of the karaoke window and the `/karaoke` page. They appear while the mouse moves over the screen (or on a tap on the page) and fade out after about three seconds, so the audience normally sees none of them. Keys on the window: C for Cover, L for Lyrics, F for full screen, Esc leaves full screen. The window's choices are remembered and the page starts from them; a viewer's own picks on the page change only that page.
- **Cover shown**: the cover with the title and artist under it on the left, the lyrics on the right in a column that glides from line to line, sung words filling with the highlight color, count-in dots kept. The background is one flat color taken from the cover and darkened so white text keeps its contrast; with no cover it is the usual dark background.
- **Lyrics hidden**: only the cover, large in the middle, with title and artist. Cover and Lyrics can't both be off. Cover hidden is the current screen.
- **Cover for the page**: the page gets the cover as a JPEG at most 1000 pixels wide, for TV browsers on Wi-Fi.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `karaoke-screen`: adds the highlight color, the on-screen buttons and the cover layout.

## Out of Scope

- A blurred cover behind the lyrics (assessed in #773, not chosen).
- Video or visualizers behind the words.
- Colors picked per track.

## Difficulty

**Small to medium: one PR.** The artwork worker, highlight palette and rendered tests come from #773 (co-authored); the style menu becomes the two toggles and the blur becomes a flat color.

## Performance

Cover decode, resize, color and JPEG encode run on a background thread, only while the karaoke screen is open, and only the latest track's result is kept. The highlight color and toggles are cached, so playback never reads settings from disk. The window redraws only while words fill, lines glide or the buttons fade.
