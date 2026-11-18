# Proposal

Planned for: **v0.9 · In the club**

## Why

A DJ with an iPad and a class-compliant controller such as the DDJ-FLX4 has everything needed to play a set except software that runs there. The research in 2026-11 (iPad and iPhone review; Pi, Engine hardware and tablets review) found the engine, decoding, analysis and Mixxx controller scripts port to iPadOS with little change, and that the hard parts are a touch UI and the App Store's terms, which conflict with the GPL. A very small app installed from Xcode for personal use sidesteps the licence question and tests the port before any larger tablet work. Android tablets can reuse the same crate later.

## What Changes

- **A new crate, `crates/tablet` (`opendj-tablet`)**: its own small Slint window on openDJ's engine. Two decks, each with title, artist, BPM, remaining time, a whole-track overview (tap to jump) and CUE, PLAY and SYNC; both decks' waveforms stacked full width with beat ticks, scrolling around a centre playhead; a crossfader; and the tracks of one folder in a list with Load A and Load B. Touch targets are at least 44 px.
- **Controllers**: Mixxx MIDI mappings bundled in the app; the browse knob and LOAD buttons pick and load tracks from the list. With a controller that has its own sound card (four output channels, as on the FLX4) the master plays on 1-2 and the headphones on 3-4.
- **Music**: the app's Documents folder, which the Files app shows as On My iPad › openDJ; Rescan reads it again.
- **Audio session**: Playback category (plays with the screen locked and the ringer off), about 5 ms buffers, every output channel the route has.
- **Waveforms drawn on the CPU** into images, since Slint's wgpu interop is not known to work with the Skia renderer Slint uses on iOS.
- **Xcode project** made with XcodeGen (`crates/tablet/ios/project.yml`) and a build script that builds the Rust binary for the device or simulator.
- **Shared crates build for iPadOS**: Whisper word timing becomes an engine feature (`word-timing`, on by default) the tablet leaves out; HID controllers are off on iPadOS (no HID access for apps); QuickJS makes its bindings at build time there; stems use plain wgpu (cubecl's Metal path is macOS only); thread priority, MIDI device watching and the settings and cache folders follow macOS.
- It also runs on the desktop (`cargo run -p opendj-tablet`) to try changes without an iPad.

Not in this change: the library database, lists, effects UI, stems on screen, recording, add-ons, the plugin host, folders outside the app (iCloud Drive, USB drives), an iPhone layout, App Store or TestFlight distribution, and Android.

## Impact

- New crate `crates/tablet`; `crates/engine` (optional Whisper, Apple thread priority and folders), `crates/controllers` (HID off and QuickJS bindgen on iOS, run loop for MIDI devices), `crates/separation` (no Vulkan on iOS).
- The desktop app is unchanged: it keeps Whisper through the engine's default features.
