## ADDED Requirements

### Requirement: Tabs
A paired phone or tablet SHALL show tabs for Automix and Deck: along the bottom on a narrow screen, in a top bar on a wide one (an iPad, or a phone on its side). A tablet SHALL open on Deck and a phone on Automix the first time; after that the page SHALL open on the tab used last.

#### Scenario: iPad first opens the remote
- **WHEN** an iPad pairs for the first time
- **THEN** it shows the Deck tab, with the tabs in a top bar

### Requirement: Deck page
The Deck page SHALL drive one deck, picked with 1 to 4 (deck 3 at first, then the one used last), with play, cue, sync, quantize, slip, reverse, nudge, hot cues, loops, beat jumps, the pitch fader with key lock, and the deck's trim, EQ, filter, headphone cue and volume. What the page changes SHALL show in openDJ at once, and changes made in openDJ or on a controller SHALL show on the page. A phone SHALL only set the deck controls on the page, not other controls.

#### Scenario: Third deck beside a controller
- **WHEN** the DJ plays deck 3 from an iPad while a two-channel controller plays decks 1 and 2
- **THEN** play, pads, loops, EQ and volume on the iPad drive deck 3, and deck 3's lights on the page follow the app

#### Scenario: Load onto a playing deck
- **WHEN** the DJ taps Load on a track for a deck that is playing
- **THEN** the page asks again (Load anyway) before the track is swapped

### Requirement: Waveform over the network
The Deck page SHALL draw the deck's waveform (low, mid and high), beats and bars, hot cues and an active loop, scrolling with the music, and an overview of the whole track. It SHALL fetch the track's waveform once per track, from paired devices only, and move it between position updates on its own clock, so it scrolls smoothly however the network jitters.

#### Scenario: Track loads on the deck
- **WHEN** a track loads on the deck the page shows
- **THEN** its waveform appears within a second or two and scrolls with playback
