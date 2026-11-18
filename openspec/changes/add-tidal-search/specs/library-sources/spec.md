## MODIFIED Requirements

### Requirement: Streaming is browse only
A streaming source SHALL never play streamed audio itself. A streamed track SHALL play when the collection holds the same track, or through an installed add-on that plays and takes links from the service's site, which downloads it into the collection's Inbox and loads it.

#### Scenario: Matched track
- **WHEN** a Spotify playlist track is also in the collection
- **THEN** it can be loaded and plays the local file

#### Scenario: Through a playing add-on
- **WHEN** a TIDAL track the collection doesn't have is double-clicked and an add-on that plays takes tidal.com links
- **THEN** the add-on downloads it into the Inbox and it loads onto a free deck

#### Scenario: No add-on
- **WHEN** no add-on takes the service's links
- **THEN** double-clicking opens the track's page on the service

## ADDED Requirements

### Requirement: TIDAL catalog search
In the TIDAL source, typing two or more letters in the search box SHALL open a dropdown under it, once typing pauses, listing TIDAL's artists, albums and tracks for them with their pictures. An artist or album SHALL open its tracks in the list, a track SHALL load onto a free deck, and Return SHALL list every track found in place of the playlist. Clearing the box SHALL show the playlist again. Listed tracks SHALL be matched to the collection and offer the same actions as playlist tracks.

#### Scenario: Searching
- **WHEN** the DJ types "radiohead" in the TIDAL source
- **THEN** the dropdown shows Radiohead under Artists, their albums under Albums and their tracks under Tracks

#### Scenario: Opening an album
- **WHEN** the DJ clicks an album in the dropdown
- **THEN** the list shows the album's tracks, those in the collection playable

#### Scenario: Old sign-in
- **WHEN** the DJ signed in before search was added
- **THEN** the list says to sign in to TIDAL again
