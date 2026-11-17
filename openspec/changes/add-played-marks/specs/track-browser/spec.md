## MODIFIED Requirements

### Requirement: Row look
Rows SHALL alternate #161819 and #101112; keys compatible with the playing deck SHALL be lilac; tracks loaded on a deck SHALL carry a badge with the deck number. Tracks played in the current History session SHALL show dimmed with a lilac check before the title; hovering the check SHALL show the time it was played.

#### Scenario: Deck badge
- **WHEN** a track is on deck 3
- **THEN** its row shows a 3 badge

#### Scenario: Played tonight
- **WHEN** a track was played at 23:41 in this session
- **THEN** its row shows dimmed with a lilac check, and hovering the check shows "Played 23:41"

## ADDED Requirements

### Requirement: Played note on a deck
Loading a track played in the current History session SHALL show a "Played" pill with its time beside the deck's title for 3 seconds.

#### Scenario: Loading a played track
- **WHEN** the DJ loads a track played at 23:41 tonight
- **THEN** the deck shows "Played 23:41" for 3 seconds

### Requirement: Mark as Not Played
The track menu SHALL offer Mark as Not Played on tracks played in the current session; it SHALL take them out of the session's history and clear their marks, keeping their play counts.

#### Scenario: A track only tested
- **WHEN** the DJ picks Mark as Not Played on a marked track
- **THEN** its mark goes and it leaves tonight's session
