## ADDED Requirements

### Requirement: Background tasks
The footer SHALL show a small progress ring while background work (analysis, HQ stems, word timing, Collection Health's sound and cover scans) runs or is paused, filled with the overall progress, amber when every task is paused, and a check for a few seconds after the last task finishes. A click on it SHALL open a panel over the browser with one row per kind of task: how many of how many, a progress bar, time left, the track it is on, Pause or Resume and, where the task can stop, Stop. Analysis and HQ stems SHALL list their next tracks with Do next, which moves a track to the front. The panel's header SHALL offer Pause all, and pauses SHALL be kept until resumed, across restarts. The panel's foot SHALL offer what happens while a deck plays: Slow down, Pause or Keep going.

#### Scenario: Stems for a request
- **WHEN** the DJ prepares HQ stems for three tracks
- **THEN** the footer ring appears, and its panel shows "HQ Stems 1 of 3" with a bar, time left and the track being separated

#### Scenario: Freeing the computer
- **WHEN** the DJ presses Pause all
- **THEN** every task stops at its next pause point within seconds, the ring turns amber, and the tasks stay paused after a restart until resumed

#### Scenario: A track needed now
- **WHEN** the DJ presses Do next on a waiting track
- **THEN** it is the next one worked on

#### Scenario: Pause while playing
- **WHEN** While a deck plays is set to Pause and a deck starts playing
- **THEN** background work waits until the decks stop
