## ADDED Requirements

### Requirement: Set Builder source
The rail SHALL have a Set Builder source whose tree lists the DJ's sets with + New set. + New set SHALL open a wizard in the browser panel of at most three pages: name, duration, crates and shape (with Build now); segments with Trip Stage and an intensity star range, and BPM (Full, Semi, Off), Harmonic (Off, fuzz 0, 1 or 2 semitones) and Automix mixing; and a review with Swap, Lock, Remove and Rebuild.

#### Scenario: Quick set from page 1
- **WHEN** the DJ names a set, sets 2 h, ticks two crates, picks Journey and presses Build now
- **THEN** the review page shows about two hours of tracks from those crates under Journey's segment headings

#### Scenario: Stars as intensity
- **WHEN** a segment asks for 4 to 5 stars and enough rated tracks match
- **THEN** no unrated or 1 to 3 star track is placed in it

#### Scenario: Harmonic fuzz
- **WHEN** Harmonic is on with fuzz 0
- **THEN** every step between two tracks with keys mixes on the Camelot wheel
- **WHEN** fuzz is 1
- **THEN** a step may also be one semitone from a key that mixes

#### Scenario: A filter runs out
- **WHEN** a segment's crates hold too few tracks for its minutes with its stars and Trip Stage
- **THEN** the review says which filter ran out instead of leaving the segment short without a word

### Requirement: Send a set
A set SHALL be sent to the Collection as a list or crate, to Automix, to the Mixed Set Editor as a planned set, or to Spotify as a private playlist, in its order. The set SHALL stay in Set Builder after it is sent and SHALL show where it was sent.

#### Scenario: Save and rebuild
- **WHEN** a set is sent to the Collection, then rebuilt with one track locked
- **THEN** the saved list is unchanged and the set keeps the locked track in its place
