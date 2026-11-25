## ADDED Requirements

### Requirement: Phase meter
In the 2-Deck layout each deck SHALL show, under its overview, a line spanning half a beat early to half a beat late with a marker at the deck's beat offset from its reference deck and the offset in milliseconds. The reference deck SHALL be the sync leader, else another playing deck. Tracks at half or double the reference tempo SHALL be compared beat for beat at the matching multiple. When the deck sits whole beats off the reference's bar, the readout SHALL say how many beats instead of milliseconds. The reference deck's own line SHALL be muted and labelled MASTER.

#### Scenario: A deck drifting late
- **WHEN** deck 1 plays 18 ms behind the master deck's beats, on the same beat of the bar
- **THEN** deck 1's marker sits right of centre, toward LATE, and the readout says +18 ms

#### Scenario: A deck dropped a beat off
- **WHEN** deck 2's downbeats land on the master deck's second beat
- **THEN** deck 2's readout says +1 BEAT

#### Scenario: The master deck
- **WHEN** deck 1 is the sync leader
- **THEN** deck 1's line is muted and labelled MASTER

### Requirement: Tempo guide
Beside the phase meter each deck SHALL show an up and a down arrow; the arrow pointing the way the deck's tempo fader must move to match the reference deck's tempo SHALL light, following the fader direction setting, and neither SHALL light within 0.05 BPM.

#### Scenario: A slower deck with faders that speed up going down
- **WHEN** deck 1 runs at 139.6 BPM against a 140.1 BPM master and the tempo faders speed up going down
- **THEN** deck 1's down arrow lights

### Requirement: Beat counter in the strip
In the 2-Deck layout the beat counter SHALL show in the strip under the overview, on the side next to the mixer, and not beside the title.

#### Scenario: Phrases off
- **WHEN** the beat counter is set to Beats
- **THEN** the strip shows the beat dots without the phrase count or countdown
