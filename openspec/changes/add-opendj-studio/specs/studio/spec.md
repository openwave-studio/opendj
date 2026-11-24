## ADDED Requirements

### Requirement: Session grid
openDJ Studio SHALL show tracks as columns and scenes as rows of clip slots. Pressing a clip SHALL launch it at the next launch point of the clock (by default the next bar); a playing clip on the same track SHALL stop at that point. Pressing a scene SHALL launch every clip in its row at the next launch point and stop tracks whose slot in that row is empty. A track's stop button SHALL stop it at the next launch point. A clip waiting to launch SHALL show as queued until it plays.

#### Scenario: Launching on the bar
- **WHEN** the transport is at beat 5.5 of a 4/4 bar grid and a clip is pressed
- **THEN** the clip starts exactly at beat 8, to the sample

#### Scenario: Launching a scene
- **WHEN** scene 3 is pressed while scene 1 plays and track 4 has no clip in scene 3
- **THEN** at the next bar tracks 1-3 switch to scene 3's clips and track 4 stops

### Requirement: Looping MIDI clips
A MIDI clip SHALL loop over its length in beats from the beat it launched, playing its notes on its track's instrument; notes that are sounding when a clip stops SHALL be released.

#### Scenario: A one-bar loop
- **WHEN** a one-bar clip with a note on beat 0 has played for three bars
- **THEN** the note has sounded three times, once at the start of each bar

### Requirement: Ableton Link
openDJ Studio SHALL be able to join an Ableton Link session, take its tempo and beat phase, and change the session's tempo when the tempo is changed in the studio, so its launch points line up with the bars of openDJ and other Link apps.

#### Scenario: Playing with openDJ
- **WHEN** openDJ and openDJ Studio are both in a Link session and the studio's clip is launched
- **THEN** it starts on a bar of the session, in time with openDJ's master deck

### Requirement: Ableton Push 2 control
openDJ Studio SHALL use an Ableton Push 2 without Ableton Live running: it SHALL switch Push to User mode on its User port, show the Session grid on the pads in each track's colour (queued clips blinking and playing clips pulsing in time with the clock), launch clips and scenes from the pads and scene buttons, stop tracks from the buttons over the screen, select tracks from the buttons under it, turn the selected instrument's knobs or the mixer from the eight encoders, and show the knobs' names and values on Push's screen. When the studio closes it SHALL turn Push's lights off and hand Push back to Live mode.

#### Scenario: Launching from Push
- **WHEN** Push 2 is plugged in and the top-left pad is pressed
- **THEN** track 1's clip in scene 1 launches at the next launch point, as if pressed in the window

#### Scenario: Turning a knob on Push
- **WHEN** the Bass track is selected and the first encoder is turned right
- **THEN** the bass synth's cutoff rises, and the window and Push's screen show the new value

### Requirement: Stage sequencer on Push 2
openDJ Studio SHALL offer a Steps pad mode on Push 2 that edits the selected slot as a stage clip of eight stages, one per pad column. The scene buttons SHALL pick the layer the pads set: pitch (in key, or a drum sound), octave, velocity, gate (pulses, stage length and ties), chance (including skipping the stage) and lock amount. Holding stage pads and turning an encoder SHALL lock that knob to a value on those stages only; the knob SHALL return to the track's value on stages without a lock. An empty slot SHALL become a stage clip when edited; a note clip SHALL NOT be changed.

#### Scenario: Locking a knob on a stage
- **WHEN** a stage clip plays and stage 6's pad is held while the first encoder turns right
- **THEN** the cutoff opens on stage 6 only, Push's screen shows the locked value in white, and the other stages keep the track's cutoff

### Requirement: Opening Ableton Live sets
openDJ Studio SHALL open an Ableton Live set (`.als`, gzipped or plain XML) given on the command line. It SHALL bring in every audio and MIDI track (up to 16) with its name, colour, volume and pan, the first eight scenes of Session clips, and the return tracks' names. MIDI clips SHALL keep their notes, loop and note chance. Audio clips SHALL play their recording following its warp markers at the studio's tempo, keeping its key; a recording that can't be found SHALL leave the clip silent and be counted in the window's status line. Live devices and plug-ins SHALL be listed on each track; MIDI tracks SHALL play a built-in stand-in sound until plug-ins are hosted. Parts not brought in (the Arrangement, group tracks, extra scenes) SHALL be named in the status line and printed.

#### Scenario: A recording made at another tempo
- **WHEN** a set at 99.45 BPM holds an audio clip recorded at 120 BPM with two warp markers, and the clip is launched
- **THEN** the recording plays in time with the studio's clock at 99.45 BPM, in its own key

#### Scenario: More than eight tracks on Push
- **WHEN** a 12-track set is open and the ninth track is selected
- **THEN** Push's pads, track buttons, encoders and screen show tracks 9 to 12

### Requirement: Scales on Push 2
openDJ Studio SHALL keep one key for the set, a scale and a root, that Push's Note view plays and stage clips are written in. Push's Scale button SHALL open a Scales mode where the buttons over and under the screen pick the root (around the circle of fifths) and step through the scales, and the encoders pick scale, root, pad layout (fourths up or right, thirds, octaves, sequential) and in-key or chromatic pads. Changing the key SHALL rewrite stage clips in it. Holding Session, Note or Scale SHALL show that view or mode only until the button is let go.

#### Scenario: Playing in another key
- **WHEN** Scale is pressed, then C and Major are picked and Scale is pressed again
- **THEN** the Note view's bottom-left pad plays C, its pads light the scale's notes with C in the track's colour, and stage clips play in C major

### Requirement: Drum sequencer on Push 2
On a drum track, Push 2's Note view SHALL be a step sequencer for the selected slot's note clip: the top four pad rows SHALL show 32 steps of the selected sound, the bottom-left 4x4 pads SHALL play and select sounds, and the bottom-right 4x4 pads SHALL show the clip's pages of 32 steps, selecting one on a tap and setting the clip's length when one is held and another tapped. The scene buttons SHALL set the step length from 1/4 to 1/32 triplets. Tapping a step SHALL add or remove a note; holding a step and tapping a later one SHALL make one note across them; while steps are held the encoders SHALL change their notes' velocity, length and chance. An empty slot SHALL become a one-bar clip when a step is added.

#### Scenario: A long note
- **WHEN** step 1 is held and step 3 tapped in 1/16 steps
- **THEN** the clip holds one note of the selected sound from beat 0 lasting three sixteenths

### Requirement: Plug-ins on tracks
Each track SHALL have a chain of plug-in slots run in `opendj-plugin-host` helper processes, in the set's device order: a plug-in instrument SHALL play the track's notes in place of the built-in sound, and plug-in effects SHALL process the track's sound in turn. A Live set's Audio Unit and VST3 devices SHALL start with the sound saved in the set, found by their id (or, failing that, by name) where plug-ins are installed; a device that is off in the set SHALL start off. A plug-in that isn't installed, doesn't start or stops SHALL leave its slot silent or dry and SHALL be named in the device row. Each plug-in device SHALL switch on and off and open its own window from the device row. Live's Wavetable SHALL be replaced by Vital, from Vital's first sound.

#### Scenario: A set's synth with its sound
- **WHEN** a set whose MIDI track holds Prophet-5 V is opened on a Mac with Prophet-5 V installed
- **THEN** the track's clips play through Prophet-5 V with the patch the set saved, and its device box opens Prophet-5 V's window

### Requirement: Track inputs and outputs
Each track SHALL have an I/O section as Live's. An audio track SHALL take one sound card input channel or a stereo pair (Audio From), and SHALL play it when Monitor is In (its clips then quiet), or Auto while the track is armed and plays no clip; Off SHALL never play it. A MIDI track SHALL take notes from every MIDI input or one, on every channel or one (MIDI From), when Monitor is In, or Auto while it is armed (or, with no track armed, selected); Push SHALL stay out of MIDI From. A track's sound SHALL go to Main, a pair of sound card outputs (Ext. Out) or only its sends (Audio To). A MIDI track with a MIDI To port SHALL send its notes there on its channel in place of its own sound. Arming a track SHALL disarm the others. A Live set's routings, monitoring and arm SHALL open as saved.

#### Scenario: A microphone through a track's effects
- **WHEN** a set whose audio track takes input 1 with Monitor In is opened
- **THEN** what comes into input 1 plays through that track's chain and fader to Main

#### Scenario: A keyboard on its own track
- **WHEN** a MIDI track's MIDI From names a keyboard and Monitor is In
- **THEN** that keyboard plays the track, and other tracks set to other inputs don't hear it

### Requirement: Audio settings
The studio SHALL have a Settings panel with an Audio page choosing the output device, the input device (or none), the sample rate and the buffer size. A new device or buffer SHALL take effect at once without closing the set; a new sample rate SHALL take effect when the studio next opens. The choices SHALL be kept between runs, and until the studio has saved any it SHALL start from openDJ's audio settings.

#### Scenario: Moving to an audio interface
- **WHEN** the output device is changed to an audio interface while a set plays
- **THEN** the set keeps playing, now on the interface, and the tracks' Ext. Out pairs list the interface's outputs

### Requirement: Drum Racks and Live's devices
A Live set's Drum Rack SHALL play each pad's own sample, found beside the set, where the set saved it, or in an installed Live's Core Library, with the pad's note, volume, pan, tuning, velocity response and choke group; a rack whose samples aren't found SHALL fall back to the built-in kit. Live's Utility, EQ Eight, Compressor, Delay, Echo, Reverb, Gate and Overdrive SHALL be played by the studio's own devices with the settings the set saved, Hybrid Reverb with the set's own room (its impulse response, found in the installed Live) blended with an algorithmic reverb, and Roar by a close approximation, in chain order with the track's plug-ins, and SHALL switch on and off from the device row.

#### Scenario: The 808 kit
- **WHEN** a set with Live's 808 Core Kit is opened on a Mac with Live installed
- **THEN** the track plays Live's 808 samples, and the open hi-hat is cut off by the closed one

#### Scenario: Editing a device
- **WHEN** a built-in device's knob is turned, a switch clicked, or an EQ Eight band dragged on its curve
- **THEN** the device changes while it plays, without cutting a reverb's tail or a delay's repeats

#### Scenario: A synced delay
- **WHEN** a track's Delay is synced to 3/16 and the set plays at 120 BPM
- **THEN** its repeats come 375 ms apart, and follow the tempo when it changes
