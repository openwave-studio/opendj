## MODIFIED Requirements

### Requirement: Streaming is browse only
A streaming source SHALL never play streamed audio itself. A streamed track SHALL play when the collection holds the same track, or through an installed add-on that plays and takes links from the service's site, which loads it from the source cache.

#### Scenario: Matched track
- **WHEN** a Spotify playlist track is also in the collection
- **THEN** it can be loaded and plays the local file

#### Scenario: Through a playing add-on
- **WHEN** a TIDAL track the collection doesn't have is double-clicked and an add-on that plays takes tidal.com links
- **THEN** the add-on fetches it into the source cache and it loads onto a free deck

#### Scenario: No add-on
- **WHEN** no add-on takes the service's links
- **THEN** double-clicking opens the track's page on the service

## ADDED Requirements

### Requirement: TIDAL catalog search
In the TIDAL source, typing two or more letters in the search box SHALL list TIDAL's catalog tracks for them, up to 40 and best first, in place of the playlist once typing pauses; clearing the box SHALL show the playlist again. Found tracks SHALL be matched to the collection and offer the same actions as playlist tracks.

#### Scenario: Searching
- **WHEN** the DJ types "one more time" in the TIDAL source
- **THEN** TIDAL's tracks for it are listed, those in the collection playable

#### Scenario: Old sign-in
- **WHEN** the DJ signed in before search was added
- **THEN** the list says to sign in to TIDAL again
