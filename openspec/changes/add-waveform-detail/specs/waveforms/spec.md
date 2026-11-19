## ADDED Requirements

### Requirement: Real wave zoomed right in
When a deck's scrolling waveform is zoomed in so that a screen pixel covers about four milliseconds of audio or less, and the deck's track is decoded, the waveform SHALL fade over from the summary to the track's own samples: each pixel column drawn from its lowest to its highest sample, above and below the centre line as the audio swings, in the look's colours, fully shown from about one millisecond a pixel. A half waveform SHALL draw the larger swing up from the bottom edge. The 3-Band look, live decks and overviews SHALL keep the summary. Reading the samples SHALL NOT hold up a frame: columns not read yet SHALL draw from the summary until they are.

#### Scenario: A kick up close
- **WHEN** a track with a kick drum is zoomed in to under a second across
- **THEN** each kick shows its single cycles swinging above and below the centre line, with the gaps between hits open

#### Scenario: Zooming out
- **WHEN** the same deck is zoomed out to ten seconds across
- **THEN** the waveform is the summary, mirrored about the centre line, as before

#### Scenario: Track still decoding
- **WHEN** a track has just been loaded and is still decoding, zoomed right in
- **THEN** the waveform draws the summary, and the real wave fades in once the track is decoded
