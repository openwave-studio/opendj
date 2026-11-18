# openDJ for tablets (iPad first)

A very small openDJ on the same engine as the desktop app: two decks, the
tracks in one folder, play, cue, sync, a crossfader, waveforms, and MIDI
controllers through their Mixxx mappings (made with the DDJ-FLX4 in mind:
its browse knob and LOAD buttons pick and load tracks, and with the FLX4
plugged in the headphones get the cue on its second output pair).

Left out on purpose: the library database, crates and playlists, effects
UI, stems, recording, add-ons, the plugin host and HID controllers.

This build is for personal use, installed from Xcode. It is not for the
App Store: openDJ is GPL and contains Mixxx code, and the App Store's terms
conflict with the GPL.

## Try it on the desktop

```sh
OPENDJ_MUSIC=~/Music/some-folder cargo run -p opendj-tablet
```

## Put it on an iPad

On a Mac with Xcode, an Apple ID (a free one installs for 7 days, a paid
developer account for a year) and the iPad plugged in:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
brew install xcodegen
cd crates/tablet/ios
xcodegen
open openDJ.xcodeproj
```

In Xcode: pick the **openDJ** target › **Signing & Capabilities** › your
**Team**, choose the iPad as the run destination, and press **Run**. The
first build takes a while (Skia and the engine). On the iPad, allow the
developer in **Settings › General › VPN & Device Management** and turn on
**Settings › Privacy & Security › Developer Mode** if asked.

## Music

Copy tracks into **Files › On My iPad › openDJ**, then press **Rescan**.
Subfolders are read too.

## Controller

Plug the DDJ-FLX4 into the iPad's USB-C port (through a hub with power if
the iPad can't power it). It is class compliant: the iPad uses it as its
sound card, and openDJ finds its MIDI within a few seconds. Plug it in
before opening openDJ: the headphone output is set up at start.

## Known gaps

- Folders outside the app (iCloud Drive, USB drives) need a folder picker
  with security-scoped bookmarks: next step.
- No layout for iPhone yet; it runs there, cramped.
- Android would reuse this crate; it needs Android MIDI, an APK build and
  the Storage Access Framework.
