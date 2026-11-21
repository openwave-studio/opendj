# openDJ for tablets (iPad first)

A very small openDJ on the same engine as the desktop app, in its look
(the same theme, fonts, logo and waveform colours): two decks, the tracks
in one folder, play, cue, sync, headphone cue, a crossfader and waveforms.

Controllers, for now: the **Pioneer DDJ-FLX4** (Mixxx's mapping) and the
**Allen & Heath Xone:K2** (openDJ's two-deck mapping). Their browse knobs
and LOAD buttons pick and load tracks. Add another by listing its files in
`controllers.txt`.

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

After pulling changes to `ios/project.yml` (or `ios/Assets.xcassets`), run
`xcodegen` again in `crates/tablet/ios`, then **Run**.

## Music

Tap **Folder** and pick the folder your music is in: in iCloud Drive, On
My iPad, or a USB drive. openDJ remembers it. Subfolders are read too.
Tracks in iCloud Drive that aren't on the iPad yet show **iCloud** as their
length and download in the background; they load once they're here. Pick
a smaller folder if the iPad is short of space.

Or copy tracks into **Files › On My iPad › openDJ** (from a Mac: Finder ›
the iPad › Files › openDJ), then press **Rescan**.

## Controllers and audio

Plug the DDJ-FLX4 into the iPad's USB-C port (through a hub with power if
the iPad can't power it). It is class compliant: the iPad uses it as its
sound card, and openDJ finds its MIDI within a few seconds. With its four
output channels the master plays on 1-2 (the FLX4's master out) and the
headphones on 3-4; openDJ follows when it is plugged in or out, and the
AUDIO pill at the top shows the routing. Without it, only the master plays
(on the iPad's speakers or whatever is connected).

The Xone:K2 is MIDI only: use it with the FLX4 or another sound card
through a USB hub. Set it to its factory settings (Latching Layers off,
MIDI channel 15). On the desktop, `OPENDJ_CHANNELS=4` tries the
four-channel routing.

## Known gaps

- No layout for iPhone yet; it runs there, cramped.
- Android would reuse this crate; it needs Android MIDI, an APK build and
  the Storage Access Framework.
