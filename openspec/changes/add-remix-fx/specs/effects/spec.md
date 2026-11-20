## ADDED Requirements

### Requirement: Band FX
Every effect slot SHALL have a band choice, any of LOW, MID and HIGH, shown as a Band value pill in a third row of the FX bar (an icon-only pill on 4-Deck) in the waveform's band colours. With no band chosen the slot SHALL process the whole signal as before. With bands chosen, the slot SHALL split its input at the channel isolator's crossover frequencies, run only the chosen bands through its effect, built-in or plugin, and add the other bands back unchanged. With the effect bypassed, the split SHALL sum back flat.

#### Scenario: Echo on the highs
- **WHEN** a deck's first slot holds Echo with Band set to High and the effect is on
- **THEN** only the hi-hats and highs echo, and the kick and bass play dry

#### Scenario: No band chosen
- **WHEN** Band is All
- **THEN** the effect sounds exactly as it did before band FX existed

### Requirement: Release
A release control SHALL turn off, in one step, every effect on a Remix deck and its source: the Remix deck's lanes, scene and sample rolls, and the Beat FX, deck FX and Pad FX on both channels. Its mode SHALL be one of Echo (delay and reverb tails keep ringing out), Brake (the music slows to a stop), Spin (the music spins back for a beat, then carries on) and Cut (the music is silent while the control is held, then everything is off). `[Master],fx_release` SHALL do the same for every channel. Backspin SHALL be available as an effect in any slot.

#### Scenario: Release with echo
- **WHEN** Echo on the highs and a Build Up scene are on and RELEASE is pressed in Echo mode
- **THEN** both effects stop taking new sound, the echoes ring out and the track carries on dry

### Requirement: Remix effects on the Remix deck
A Remix deck SHALL hold three band lanes (LOW, MID, HIGH), each with an ISOLATE knob and a LEVER, one Isolate effect and one Lever effect shared by the lanes, a SCENE knob, RELEASE, four sample pads and a sample effect, all following its source deck's beat grid (the sync leader when the source is an input). With nothing on, a Remix deck in Send mode SHALL add nothing to the mix, and one in Insert mode SHALL pass its source through unchanged.

#### Scenario: Idle Remix deck
- **WHEN** every lever is off, every ISOLATE knob and SCENE are at centre and no pad plays
- **THEN** the mix sounds exactly as without the Remix deck

### Requirement: Isolate knobs
Each band's ISOLATE knob SHALL have a centre where it does nothing. Turned left in Insert mode, it SHALL lower that band down to silence at the end (in Send mode the source's own channel keeps playing the band, so turning left does nothing). Turned right, it SHALL add the Isolate effect (Tape Echo, Reverb, Drive, Filter, Ducker or Rhythm) to that band, more as it turns.

#### Scenario: Cut the bass
- **WHEN** the Remix deck is in Insert mode and ISOLATE LOW is turned fully left
- **THEN** the source's lows are silent and the mids and highs are unchanged

#### Scenario: Duck the mids
- **WHEN** the Isolate effect is Ducker and ISOLATE MID is turned right
- **THEN** the mids dip on every beat and recover before the next one

### Requirement: Levers
Each band's LEVER SHALL put the Lever effect (Echo, Reverb, Repeat, Reverse, Solo or Stretch) on that band. It SHALL be on while held, or stay on when latched, and its depth SHALL be adjustable.

#### Scenario: Throw the highs
- **WHEN** LEVER HIGH is held with Echo chosen
- **THEN** the highs echo on the beat until it is let go, and the echo tail rings out

#### Scenario: Solo
- **WHEN** LEVER MID is latched with Solo chosen in Insert mode
- **THEN** only the mids play until the lever is pressed again

### Requirement: Scenes
The SCENE knob SHALL do nothing at centre, run the chosen Build Up scene as it turns right and the chosen Break Down scene as it turns left, each scene moving several effect amounts together on curves. Five built-in scenes SHALL be offered on each side. TIME and COLOR SHALL change the scene's delay time and filter colour.

#### Scenario: Build up
- **WHEN** SCENE turns slowly from centre to fully right with Rise Echo chosen
- **THEN** a band-pass echo rises in pitch, feedback and level together, on the beat

### Requirement: Sample pads
A Remix deck SHALL have four sample pads, played out of its own channel, by default the Groove Box's Boom kit kick, snare, clap and hi-hat, with any sample pack bank available instead. A pad press SHALL play on the next grid step. Holding a pad with a roll rate (1/2, 1/4, 1/8, 1/16, 1/32, 1/6, 1/12, 1/24) SHALL repeat it on that grid while held; two rates at once SHALL combine into a groove roll. Each pad SHALL have its own pitch. OVERDUB SHALL record pad hits into a one-bar loop that keeps playing, with undo and clear. One sample effect (Echo, Space, Filter, Pitch, Decay or Swing) SHALL apply to the pads only.

#### Scenario: Kick roll
- **WHEN** the kick pad is held with 1/16 chosen
- **THEN** the kick repeats every sixteenth on the deck's grid until the pad is let go

#### Scenario: Overdub
- **WHEN** OVERDUB is on and the clap is hit on beats 2 and 4
- **THEN** the clap keeps playing on 2 and 4 every bar after OVERDUB is turned off, until cleared
