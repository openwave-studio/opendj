## ADDED Requirements

### Requirement: Jog latency
A controller's jog, scratched through `engine.scratchEnable` and `engine.scratchTick`, SHALL follow the hand closely without making a steady hand wobble. The mapping's own alpha-beta filter SHALL be paired with a quicker one (3 times alpha, 9 times beta) that takes over while the two disagree by more than the quick filter's own wobble for that wheel's resolution. A latency probe test SHALL measure the result on a DDJ-FLX4-like wheel (720 ticks a turn, a MIDI message every 2 ms).

#### Scenario: Grabbing the record
- **WHEN** the hand starts the record at normal speed from a stop
- **THEN** the deck reaches half that speed within 16 ms and 90 % within 40 ms, before the audio buffer

#### Scenario: Baby scratch
- **WHEN** the hand scratches back and forth 8 times a second at up to 3 times normal speed
- **THEN** the deck lags the hand by at most 14 ms and reaches over 2.5 times normal speed

#### Scenario: Steady hand
- **WHEN** the hand holds normal speed
- **THEN** the speed wobbles by under 1 %, as with the mapping's filter alone

### Requirement: Millisecond timers on Windows
On Windows the app SHALL ask for 1 ms timer resolution at start, so the controller threads' 1 kHz scratch filter and timers run on time.

#### Scenario: Windows jog
- **WHEN** a jog is scratched on Windows
- **THEN** the deck's speed is updated every millisecond, not every 15.6 ms

### Requirement: Jog bends like Mixxx
A controller's `jog` SHALL bend the deck's speed as Mixxx's does: each unit adds a tenth of itself to the speed for one 10 ms block, averaged over about a quarter second, so a playing deck moves 1 ms further per unit and a large jog plays fast, backwards too. A paused deck SHALL move 18 times as far per unit. Fast searches that move the position directly SHALL use `jog_search` (10 ms of track a unit); a deck searching faster than 8 times normal speed plays silently.

#### Scenario: A Mixxx mapping's rim
- **WHEN** a mapping sends 0.8 `jog` a tick while the rim turns a quarter turn a second (720 ticks a turn)
- **THEN** the deck plays about 14 % faster and springs back once the rim stops, without the playhead jumping

#### Scenario: SHIFT + jog search
- **WHEN** openDJ's own Pioneer mappings turn the jog with SHIFT held
- **THEN** `jog_search` moves the position as before and the deck is silent while it searches

### Requirement: Slip back-spin
In slip mode, letting go of a scratched jog SHALL keep the deck following the wheel while it still turns, and let go once it has been still for 80 ms, so slip returns after a back-spin.

#### Scenario: Back-spin in slip mode
- **WHEN** the hand flings the record back and lets go with SLIP on
- **THEN** the deck plays the spin until the wheel stops, then carries on from the slip line

### Requirement: Jog feel per controller
The Controller Editor's Options SHALL offer, for each connected controller, Pitch bend (Fine, Normal, Strong: half, as mapped, double the `jog` and `jog_bend` steps) and Scratch (Loose, Normal, Tight: the mapping's scratch alpha times 0.5, 1 or 2 and beta times 0.25, 1 or 4). Normal SHALL leave the mapping as it is. The choices SHALL be kept per controller.

#### Scenario: Tighter platter
- **WHEN** Scratch is Tight and the hand starts the record
- **THEN** the deck reaches the hand's speed sooner than with Normal
