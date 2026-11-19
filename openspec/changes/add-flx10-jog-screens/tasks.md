# Tasks

## 1. Script API
- [x] 1.1 `engine.getWaveform(group, perSecond)` from the loaded tracks' waveforms
- [x] 1.2 `engine.beginFastTimer` down to 5 ms; HID FIFO holds thousands of reports
- [x] 1.3 An HID device that is also a mapped MIDI port takes its mapping's name

## 2. Mappings
- [x] 2.1 "Pioneer DDJ-FLX10 jog screens" HID mapping: state, load, waveform, trickle
- [x] 2.2 FLX10 MIDI mapping sends the SysEx gate and keepalive while it runs
- [x] 2.3 Linux udev rule, guide page with test steps, supported list note

## 3. Hardware
- [ ] 3.1 A tester tries it on Linux, macOS and Windows
- [ ] 3.2 Hot cue labels, beat grid and album art on the screens
