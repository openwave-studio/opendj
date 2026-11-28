## ADDED Requirements

### Requirement: Tabs
A paired phone or tablet SHALL show tabs for Automix and Deck: along the bottom on a narrow screen; on a wide one (an iPad, or a phone on its side) in a top bar like the app's, with the page picker on the left, the logo in the middle and, on the Deck page, the deck picker and LOAD on the right. A tablet SHALL open on Deck and a phone on Automix the first time; after that the page SHALL open on the tab used last.

#### Scenario: iPad first opens the remote
- **WHEN** an iPad pairs for the first time
- **THEN** it shows the Deck page, with the page picker in a top bar

### Requirement: Deck page
The Deck page SHALL drive one deck, picked with 1 to 4 (deck 3 at first, then the one used last). Its header SHALL show the deck as the app does: cover, title and artist, bar and beat, time left, tempo, and the key it plays in with key shift, green when it mixes with the deck playing to the room. It SHALL have play, cue, sync, quantize, slip, reverse, nudge, hot cues, loops, beat jumps, the pitch fader with key lock, and the deck's trim, EQ, filter, headphone cue and volume. What the page changes SHALL show in openDJ at once, and changes made in openDJ or on a controller SHALL show on the page. A phone SHALL only set the deck controls on the page, not other controls.

#### Scenario: Third deck beside a controller
- **WHEN** the DJ plays deck 3 from an iPad while a two-channel controller plays decks 1 and 2
- **THEN** play, pads, loops, EQ and volume on the iPad drive deck 3, and deck 3's lights on the page follow the app

#### Scenario: Load onto a deck
- **WHEN** the DJ taps a track in LOAD's list for a deck that is stopped, or playing with its volume fader down
- **THEN** the track loads onto that deck

#### Scenario: Load onto a playing deck
- **WHEN** the DJ taps a track for a deck that plays to the room
- **THEN** the page asks for a second tap before the track is swapped

#### Scenario: Pick a track that mixes
- **WHEN** the DJ opens LOAD while another deck plays
- **THEN** keys that mix with it show in green, and quick filters find tracks that mix with it by key, tempo or both

### Requirement: Waveform over the network
The Deck page SHALL draw the deck's waveform in the colour scheme picked in the app (RGB or 3-Band, with its palette), beats and bars, hot cues and an active loop, scrolling with the music, and an overview of the whole track. It SHALL fetch the track's waveform once per track, from paired devices only, and move it between position updates on its own clock, steered by the app's time stamps rather than by when each update arrives, so it scrolls smoothly however the network jitters. Sending the waveform SHALL NOT slow the app down while a track is still being analysed.

#### Scenario: Track loads on the deck
- **WHEN** a track loads on the deck the page shows
- **THEN** its waveform appears within a second or two and scrolls with playback
