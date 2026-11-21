## ADDED Requirements

### Requirement: Add-ons that play
An add-on source SHALL be able to say that its files play from the source cache, as the built-in music servers do, and that it has no place in the rail. One kept out of the rail SHALL still run, and play the streamed rows whose site it takes links from.

#### Scenario: Behind the TIDAL source
- **WHEN** an add-on that plays tidal.com links says it stays out of the rail
- **THEN** the rail shows only the TIDAL source, and its tracks play through the add-on

### Requirement: Add-ons that browse a service's catalog
An add-on source SHALL be able to say it browses a streaming service's catalog for the app's own source of that service. The app's source SHALL then sign in, list playlists, list tracks and search through the add-on instead of the service's public API.

#### Scenario: One TIDAL sign-in
- **WHEN** an add-on says it browses TIDAL's catalog
- **THEN** the TIDAL source signs in through it, and lists, searches and plays with that one sign-in

### Requirement: Streamed tracks fetched onto decks
A streamed track fetched through an add-on that plays SHALL download into the collection's Inbox, take the streamed track's place in every list it stood in, and load onto the deck it was meant for. That deck SHALL show the track's title and artist and the download's progress until it loads.

#### Scenario: Dragged onto a deck
- **WHEN** the DJ drags a greyed TIDAL row onto deck 2
- **THEN** deck 2 shows its title, artist and "Downloading" with a progress bar, and loads the file once it is in
