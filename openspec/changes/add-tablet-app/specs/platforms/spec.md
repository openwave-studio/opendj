## ADDED Requirements

### Requirement: Tablet build
openDJ SHALL have a small tablet app (`opendj-tablet`) on the same engine that builds for iPadOS from Xcode, for personal installs and not for the App Store. It SHALL play two decks with play, cue, sync, a crossfader and waveforms.

#### Scenario: Tracks from the Files app
- **WHEN** tracks are copied into Files › On My iPad › openDJ and Rescan is pressed
- **THEN** they are listed, and Load A puts the selected one on deck A

### Requirement: Tablet controllers
The tablet app SHALL work with MIDI controllers through their Mixxx mappings, including picking and loading tracks with a controller's browse knob and LOAD buttons, and SHALL send the headphones to a controller's second output pair when it has four output channels.

#### Scenario: Controller loads a track
- **WHEN** a DDJ-FLX4 is plugged in, its browse knob is turned two steps and deck B's LOAD is pressed
- **THEN** the track two rows down is selected and loads on deck B

#### Scenario: Headphones on the controller
- **WHEN** the app starts with a controller that gives four output channels
- **THEN** the master plays on channels 1-2 and the headphones on 3-4

### Requirement: Desktop-only parts left out of the tablet build
Parts that can't run on iPadOS (HID controllers, the plugin host, add-on programs, Whisper) SHALL be left out of the tablet build without changing the desktop app.

#### Scenario: Desktop unchanged
- **WHEN** the desktop app is built
- **THEN** it still has Whisper word timing and HID controllers
