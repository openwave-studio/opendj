# Tasks

## 1. Tablet app

- [x] 1.1 `crates/tablet`: two decks, the music folder, play, cue, sync, seek, crossfader, stacked waveforms with beat ticks, overviews
- [x] 1.2 Controllers through Mixxx mappings: browse knob and LOAD buttons pick and load tracks; headphones on a four-channel controller
- [x] 1.3 Audio session: Playback category, short buffers, every output channel
- [x] 1.4 Shared crates build for iPadOS: Whisper optional, no HID, QuickJS bindgen, plain wgpu stems, Apple thread priority, folders and MIDI device watching
- [x] 1.5 Xcode project (XcodeGen) and Rust build script; README with install steps
- [x] 1.6 The desktop look: shared theme and fonts, logo, deck headers with cover or initials, the desktop's RGB waveform colours and bar lines, library headings and rows
- [x] 1.7 Audio routing: master 1-2, headphones 3-4 on a four-channel controller, re-routed when it is plugged in or out; headphone cue buttons; an AUDIO pill
- [x] 1.8 Controllers limited to the DDJ-FLX4 and the Xone:K2 two-deck mapping (`controllers.txt`), bundled by the Xcode build; a CONTROLLER pill

## 2. On the iPad

- [ ] 2.1 Build with Xcode on a Mac and install on an iPad
- [ ] 2.2 Play a set with the DDJ-FLX4: sound, headphone cue, browse and load, jogs and pitch

## 3. Next steps

- [ ] 3.1 Pick a folder anywhere (iCloud Drive, USB drives) with security-scoped bookmarks; download iCloud files before loading
- [ ] 3.2 iPhone layout
- [ ] 3.3 Android tablet build: entry point, APK, Android MIDI, Storage Access Framework
