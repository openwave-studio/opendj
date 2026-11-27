## ADDED Requirements

### Requirement: Surround layouts
openDJ Studio SHALL offer the Main outputs Stereo, Quad (4.0), 4.1, 5.0 and 5.1, chosen on Main's Surround panel and saved in the set. Each speaker SHALL be sent to a sound card channel, by default in HDMI order (L, R, C, LFE, Ls, Rs on channels 1-6; quad uses L, R, Ls, Rs on 1, 2, 5, 6), and each speaker's channel SHALL be changeable. A layout needing more channels than the output device has SHALL say so and play nothing on the missing speakers.

#### Scenario: 5.1 over HDMI
- **WHEN** the output is an 8-channel HDMI device and the layout is 5.1
- **THEN** front left and right play on channels 1 and 2, centre on 3, LFE on 4, and the rear left and right on 5 and 6

#### Scenario: Back to stereo
- **WHEN** the layout is set to Stereo in a set with Surround Panners
- **THEN** every panned track folds down to the stereo Main at the same loudness and the panners show "Stereo"

### Requirement: Speaker setup and test
Main's Surround panel SHALL show the layout from above with each speaker's level meter, and SHALL let each speaker be muted, soloed, trimmed (-12 to +6 dB) and delayed (0 to 30 ms). A Test button SHALL play pink noise on one speaker at a time, walking around the room, naming the speaker it plays on.

#### Scenario: Finding a swapped cable
- **WHEN** Test is pressed on 5.1
- **THEN** the noise plays from front left, centre, front right, rear right, rear left and the sub in turn, each highlighted on the panel, so a speaker that sounds from the wrong place is found

### Requirement: Tracks without a panner
Main's Surround panel SHALL set what tracks without a Surround Panner do: Front (front left and right only, as today), Front and rear (also in the rear speakers, lower by a Rear level), or Everywhere (all speakers, for a wash).

#### Scenario: A stereo bed
- **WHEN** Unpanned is Front and rear with Rear level -6 dB
- **THEN** a track with no panner plays in front left and right and 6 dB lower in rear left and right

### Requirement: Surround Panner
openDJ Studio SHALL offer a Surround Panner device that goes last in a track's chain. Its panel SHALL show the room from above with the speakers, the sound's position as a dot (two dots, left and right, when Width is above 0) and each speaker's live gain, and dragging the dot SHALL move the sound. Its settings SHALL be:
- Angle (-180° to 180°, 0 front, clockwise positive) and Distance (0 centre to 100 % the speaker circle), the dot's position
- Width (0° to 180°): how far apart the left and right sides are, around the Angle; a Mono switch sums them
- Focus (0 to 100 %): 100 % plays only the nearest one or two speakers; lower values spread the sound into the others, at 0 % evenly into all
- Centre (0 to 100 %, 5.0 and 5.1): how much a front sound uses the centre speaker instead of a phantom centre between left and right
- Sub (off, -40 to 0 dB): a send to the LFE speaker
- Smooth (0 to 2000 ms): moves glide to new positions instead of jumping, so stepped automation and controller moves don't zip

The panner SHALL keep loudness steady as the sound moves (constant power), so a sound passing between speakers doesn't dip or bulge.

#### Scenario: Rear left
- **WHEN** Angle is -110°, Distance 100 %, Focus 100 % on 5.1
- **THEN** the track plays only from the rear left speaker

#### Scenario: Pulled to the middle
- **WHEN** Distance is 0 %
- **THEN** the track plays evenly from every main speaker (not the sub) at the same total loudness as at the edge

### Requirement: Orbit
The Surround Panner SHALL have an Orbit that moves the Angle on its own: On, Rate (synced 1/16 to 32 bars, with triplets and dotted, or free 0.01 to 20 Hz), Shape (Circle, Back and forth, Random), Direction (clockwise, anticlockwise), Range (0° to 360° of travel) and Phase. A synced Orbit SHALL follow the set's clock, so the same beat is always at the same place, also when the set follows Link or openDJ's master.

#### Scenario: A sound circling every bar
- **WHEN** Orbit is on, Rate 1 bar, Shape Circle, Range 360°
- **THEN** the sound goes all the way round the room once per bar, at the front on every downbeat

### Requirement: Surround controls
Every Surround Panner setting and Main's Surround settings (layout aside) SHALL be device parameters: shown as knobs and switches, mappable to MIDI controllers, on Push 2's encoders as the device's bank (Angle, Distance, Width, Focus, Orbit Rate, Orbit Range, Centre, Sub), and saved in the set. Angle SHALL wrap around on endless encoders instead of stopping at ±180°.

#### Scenario: Turning a sound around with Push
- **WHEN** a Surround Panner is selected and Push's first encoder is turned clockwise past 180°
- **THEN** the sound keeps moving clockwise through the rear to the left without a jump

### Requirement: Headphone monitoring
Main's Surround panel SHALL have a Monitor choice: Speakers, Headphones or Stereo. Headphones SHALL play the surround mix through virtual speakers at the layout's angles using HRTF data, out of the stereo outputs. Stereo SHALL play the standard fold-down. Speakers SHALL be the default whenever the layout is not Stereo.

#### Scenario: Checking the mix away from the room
- **WHEN** Monitor is Headphones on 5.1 and a track's Angle is 110°
- **THEN** the track is heard behind and to the right on headphones, out of outputs 1 and 2 only

### Requirement: Head tracking
On macOS 14 or later, with headphones that report head motion (AirPods Pro, AirPods Max), Headphones monitoring SHALL offer Head tracking: turning the head turns the virtual speakers the other way, so the room stays put. A Recenter button SHALL make the current direction the front. The panel SHALL show whether tracking is live, and Studio SHALL ask for Motion permission the first time it is turned on. The panel SHALL remind the user to turn off the system's own Spatial Audio for Studio.

#### Scenario: Turning to the left
- **WHEN** Head tracking is on and the head turns 90° left
- **THEN** the front speakers' sound is heard from the right, as if the room stayed still

#### Scenario: Headphones without motion
- **WHEN** the headphones report no head motion
- **THEN** Head tracking shows "Not available with these headphones" and Headphones monitoring plays without it

### Requirement: Surround in the Session deck
openDJ's Session deck SHALL play a surround set as the Headphones render or the Stereo fold-down, as the set's Monitor says (Speakers plays as Stereo), never sending speaker channels from openDJ.

#### Scenario: A surround set in openDJ
- **WHEN** a 5.1 set with Monitor on Speakers plays on the Session deck
- **THEN** it plays through the Session channel as the stereo fold-down
