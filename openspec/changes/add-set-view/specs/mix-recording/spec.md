## ADDED Requirements

### Requirement: Change a transition
In the Mixed Set Editor the DJ SHALL be able to set a transition's length in bars and its curve, hear it with a few bars either side, and go back to how it was played.

#### Scenario: Longer bass swap
- **WHEN** the DJ sets a 4-bar Smooth transition to 16 bars of Bass swap and saves the edited mix
- **THEN** the new file mixes those two tracks over 16 bars with a bass swap, and the rest of the set is as recorded

### Requirement: Swap a track
In the Mixed Set Editor the DJ SHALL be able to replace a track of the set with another from the library, beatmatched to the set on the same bar for the same stretch, with both of its transitions made again in their recorded style and length.

#### Scenario: Swapped track
- **WHEN** the DJ swaps track 3 for another track and saves the edited mix
- **THEN** the new file plays the other track in its place in time with the tracks around it, and the tracklist and chapters name it


### Requirement: Frame-accurate control log
Control changes in the control log SHALL be stamped with the audio frame at which they took effect. Plugin parameter changes and instrument notes SHALL be logged.

#### Scenario: Exact time
- **WHEN** a hot cue is pressed during a recording
- **THEN** its log line's time matches the frame where the jump is heard in the recording

### Requirement: Render an edited set
After the DJ moves, stretches or edits transitions or lanes in the Mixed Set Editor, openDJ SHALL render the set through its engine to a new file beside the recording and SHALL NOT change the recording.

#### Scenario: Scratch kept or bypassed
- **WHEN** a set has a scratch on deck 2 and the DJ leaves it on Keep from recording
- **THEN** the render holds the recording's audio for that stretch
- **WHEN** the DJ switches it to Bypass
- **THEN** the render plays deck 2 straight through that stretch as if it had not been scratched

#### Scenario: Unedited render matches
- **WHEN** a recording with no edits is rendered
- **THEN** the render matches the recording within -60 dBFS, apart from third-party plugin effects
