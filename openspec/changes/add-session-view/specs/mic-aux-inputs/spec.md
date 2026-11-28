## ADDED Requirements

### Requirement: Session channel
openDJ SHALL play an openDJ Studio Session on a channel of its own beside the microphone and auxiliary inputs, with a fader, EQ, effect units and cue, labelled Session, and in recordings of the main mix. The Session SHALL render on a real-time thread of its own ahead of the output, so that a late Session block is heard as silence on the Session channel only and the decks never wait for it. The Session SHALL follow openDJ's leading tempo and beat, and keep its own tempo while nothing leads.

#### Scenario: Launches on the decks' bars
- **WHEN** deck 1 plays a track at 124 BPM as the master and the user launches a clip in the Session with 1-bar quantize
- **THEN** the clip starts on deck 1's next downbeat, at 124 BPM

#### Scenario: A heavy set never glitches the decks
- **WHEN** the Session's set takes longer than a block to render
- **THEN** the Session channel is silent for that block, the decks play on without a gap, and the Session's load shows it

#### Scenario: Leaving the view
- **WHEN** clips play in the Session and the user switches to the 2-deck library view
- **THEN** the clips keep playing and the Session bar shows the playing scene with a way back to the view
