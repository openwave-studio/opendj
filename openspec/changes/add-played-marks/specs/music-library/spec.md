## MODIFIED Requirements

### Requirement: Play history
A track heard in the master for 30 seconds SHALL count as played: its play count goes up, its last-played date is set, and it is added to this session's history (named by date and time) in the History tab. Heard in the master means playing with its channel fader up and the crossfader open on its side; with an external mixer, playing is enough. A deck heard only in the headphones SHALL NOT count. A new session SHALL start after 60 minutes with nothing played, at 6:00, or when the DJ starts one with the + over History's Sessions list. When the app starts within a session's time, it SHALL carry on in that session. Empty sessions SHALL be dropped.

#### Scenario: History session
- **WHEN** the first track of the evening has been heard in the master for 30 seconds
- **THEN** a history session named with the date and time appears with that track

#### Scenario: Pre-listening doesn't count
- **WHEN** a track plays for a minute on a deck with its channel fader down
- **THEN** it is not added to the history and its play count stays

#### Scenario: Restart in the middle of a set
- **WHEN** the app restarts ten minutes after the last track was played
- **THEN** the next track played goes into the same session

#### Scenario: Start a new session by hand
- **WHEN** the DJ clicks Start new session over the Sessions list
- **THEN** the played marks clear and the next track played starts a new session
