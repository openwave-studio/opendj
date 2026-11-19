## ADDED Requirements

### Requirement: Jog screen waveforms
openDJ SHALL give controller scripts a deck's waveform (`engine.getWaveform`) and timers down to 5 ms (`engine.beginFastTimer`), and SHALL draw the loaded tracks' waveforms on the DDJ-FLX10's jog screens over USB HID when its MIDI mapping and its jog screens HID mapping both run.

#### Scenario: Load a track on an FLX10
- **WHEN** a DDJ-FLX10 is connected and a track is loaded on deck 1
- **THEN** the deck 1 jog screen shows the track's waveform, time, BPM, tempo and key, and the waveform scrolls as it plays

#### Scenario: Screens mapping turned off
- **WHEN** the jog screens HID mapping is turned off in Settings › Controllers or can't open the device
- **THEN** the MIDI mapping stops switching the screens and shows BPM, time and the playhead from MIDI again

#### Scenario: Two names for one controller
- **WHEN** a controller is both a MIDI port running a MIDI mapping and an HID device of the same name
- **THEN** Settings › Controllers lists the HID side under its HID mapping's name, and each can be turned off on its own
