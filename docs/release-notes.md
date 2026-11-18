openDJ 0.8.0 takes a set from the booth to the studio. The Mixed Set Editor opens a recorded set as a timeline: check the mix, fix it, and export it with chapters. Collection Health is rebuilt around one row per check, with duplicates found by sound and new Tags and Lists pages. Beat FX work the same on every controller, the instrument deck is redesigned with a step sequencer, a Room EQ tunes the main and booth outputs to the venue, and the karaoke screen can show the cover beside the lyrics.

## What's new

**Mixed Set Editor**

- Right-click a recording in Mixed Sets, or a History session, and choose **Open in Mixed Set Editor**. It shows the set as track blocks over the recording's waveform. **Show decks** adds a row per deck with loops and scratches, and **Show moves** adds the faders, EQs, filters and effects.
- Play the recording from any point, zoom, and step through a **Transitions** list with each mix's length, style and whether the beats stayed in time.
- **Check my mix** finds clipping, silences, level jumps and drifting beats. Click one to hear it from 2 bars before.
- **Fixes**: trim the start and end, change a track's level, cut a part out, and set the loudness for a platform, with Undo. **Save edited mix** writes a new file beside the recording.
- **Export** a Mixcloud tracklist, YouTube chapters or a cue sheet, or send the edited mix to Mixcloud.
- Save the transitions you liked as **pair mixes**, so Automix mixes those two tracks the same way next time.

**Collection Health**

- One row per check in the sidebar, grouped under Files, Location, Duplicates, Metadata, Prep and Organization, each with a dot for its state and a green tick when it finds nothing.
- The Overview shows the collection by stage on one bar, with Wanted beside it, and explains the score: which checks hold it back and by how much.
- Check pages list their fixes as numbered steps.
- **Duplicates** come as one row per group, marked Sure, Likely or Versions. Open a group to compare the copies. Copies are also found **by sound**, even with different or missing tags.
- **Tags**: a card for Genre and each tag group, with names spelled like another and names used once. **Roll up** merges spelling twins, and any tag can be merged into another from Settings › Tags.
- **Lists**: removed tracks still in lists, tracks listed twice, empty lists and lists named like another. **Tidy the sure ones** fixes the safe ones.
- **Quarantine** lists every track taken out of the collection, with **Bring back**, and moves files left behind in your music folders into quarantine.
- **Missing files**: **Look in a folder…** relinks files from another drive or a backup, and **Move to Wanted** puts tracks you no longer have on the Wanted list.
- Old analyses of files that are gone can be cleared from Settings › Collection › Storage.

**Beat FX**

- Beat FX work the same on every controller: the effect buttons step through effects in the order the list shows, and halve and double use each effect's own limits.
- When an effect changes from a controller, its name glows and a strip shows the effects either side.
- **MST** in the Beat FX routing row sends the effect to the main mix, and the MASTER position on Pioneer controllers now selects it.
- Echoes and rolls go up to 16 beats.

**Instruments**

- A redesigned instrument deck: a slim title with a ☰ deck menu, a heart to save presets and a mark when one has changed, an in and out box with the scope, trim and pan, larger knobs in the instrument's own color, Scale, Chord and Arp pills, and a shaded keyboard in three sizes.
- **HOLD** for any instrument, and knob pages named after the plugin's own.
- A **step sequencer** when the keyboard is hidden: pitch, octave, velocity, gate and chance lanes with their own lengths, four pattern slots, **REC** from keys and pads, **Dice** for a new riff with undo, and Auto-Regen. Patterns are saved in the preset, and the 4-Deck strip has ▶ and the slot picker.

**Sound**

- **Room EQ** in Settings › Audio: six bands on the main and booth outputs with presets for common rooms, a live spectrum with Before and After, Freeze, click a peak to cut it, Bypass and A/B. Recordings are taken before it. The preset shows in the MASTER panel.

**Karaoke**

- Show the **cover** beside the lyrics, and go full screen, from buttons on the karaoke screen and the /karaoke page.
- Pick the sung words' color, Few or Many lines, and Word by word or Whole line light-up.
- In Player, a microphone button shows the lyrics beside the album art in the main window.
- Lines no longer step back during long gaps.

**Library and DJing**

- **Played marks**: tracks played in this History session are dimmed with a lilac check. A play counts only when it is heard in the main mix, not in the headphones. Start a new session from History.
- **TIDAL**: search TIDAL's catalog from the TIDAL source, with a dropdown of artists, albums and tracks as you type. With a TIDAL add-on that plays, double-click or drag a TIDAL row onto a deck: it downloads into the Inbox, loads, and takes the row's place in your lists.
- **Jog wheels** bend the tempo as they do in Mixxx mappings, slip mode lets a back-spin finish before it returns, and Controller Editor › Options sets each controller's pitch bend and scratch feel. Touching or letting go of a platter no longer clicks, and on Pioneer DDJ controllers the rim bends gently, moves a paused track one bar a turn, and never scratches.
- **Tap tempo** fits steady taps and rounds to a whole BPM.
- The MASTER pill clock shows the time of day.
- The RGB waveform draws mids green instead of amber, with purer red and blue, so bass with hi-hats reads purple and bass with mids orange.
- A finished download shows the quality the source actually sent.
- Phrase cuts wait for the kick, and phrase tooltips count bars from bar 1.
- The Info tab's File, Track and Tags tabs are tidier.
- VirtualDJ import no longer makes duplicate tag groups or empty folder lists.

**Controllers**

- **Pioneer OPUS-QUAD** (Beta).
- **DDJ-FLX10**: jogs scaled for its big platters, pad colors that match the screen, ACTIVE PART lights for stems, and tempo, key and key shift on the jog displays.
- The cue marker on the jog displays of the FLX10, OPUS-QUAD, XDJ-AZ and XDJ-RX3 points the right way.
- **DDJ-400**: Beat FX controls, rim sensitivity, Slip release and the SHIFT pad modes (Keyboard, Pad FX 1 and 2, Key Shift) work.

**Fixes**

- Picking a LENGTH in Automix during an album takes effect at once, and dragging the Player bar no longer leaves a deck stuck.
- A hot cue or saved loop pressed on a paused deck plays from it, as on Pioneer gear.
- Plugins always get a tempo, 120 BPM while no deck gives one, so synths such as Vital no longer crash on presets with tempo-synced parts.
- Zooming far into the Mixed Set Editor with Show decks on no longer crashes.
- The plugin host offers CLAP plugins the host features they expect (parameters, state, latency, log and note ports).
- Instrument rack knobs turn with click and drag again.
- Control-click a track list heading, or right-click the empty header, to open the column menu.
- The Prep Checklist scrolls when the side panel is short.
- Windows draws with DirectX 12 first, falls back to another graphics driver after a start that never draws, and writes a crash report when it fails. Plugin helpers start without a console window.

## Install

**macOS** (Apple Silicon and Intel, macOS 11 or newer): if you have 0.3.0 or newer, openDJ offers this update itself when it starts. Otherwise download `openDJ-…-macos.zip`, unzip it and move openDJ to Applications.

**Windows** (preview; 64-bit Windows 10 and 11):

- `openDJ-…-windows-x64-setup.exe`: installs for your user, no administrator needed, and updates itself from now on.
- `opendj-…-windows-x64.zip`: portable. Unzip anywhere and run `openDJ.exe`.

The Windows build isn't signed yet, so on first run SmartScreen says "Windows protected your PC": click More info, then Run anyway. ASIO drivers show next to Windows Audio devices in Settings › Audio. Windows on ARM runs it through emulation.

**Linux** (x86_64, glibc 2.35 or newer, e.g. Ubuntu 22.04+, Fedora 36+, Debian 12+):

- `opendj_…_amd64.deb` (Ubuntu, Debian, Mint, Pop!_OS) or `opendj-…x86_64.rpm` (Fedora, openSUSE): open it to install with your software centre, or run `sudo apt install ./opendj_…_amd64.deb` / `sudo dnf install ./opendj-…x86_64.rpm`. openDJ then shows up with your other apps, Traktor Kontrol controllers work without extra setup, and when a new version is out openDJ offers the new package.
- `openDJ-…-x86_64.AppImage`: make it executable (`chmod +x`) and run it. AppImages from 0.3.0 on update themselves.
- `opendj-…-linux-x86_64.tar.gz`: unpack anywhere and run `bin/opendj`.

openDJ needs ALSA and a Vulkan driver, which most desktops already have.

Add your music folders in Settings › Collection, or let the guided tour do it the first time openDJ starts. MIDI controllers that Mixxx supports load their mapping when plugged in.

## Known limits

- Windows is a preview: unsigned, and less tested than macOS and Linux.
- Streaming services are browse only, except rows a playing add-on loads. Buying from stores opens the store's page; openDJ doesn't check out for you.
- Automix uses decks 1 and 2 only.
- Controllers marked Beta, including the Xone K2 and Maschine Mikro MK3 mappings, the Traktor Kontrol controllers, the XDJ-RX3 and Numark V7 mappings and the new Pioneer mappings, haven't all been tried on the hardware yet.
- Plugins: their latency isn't compensated. On Linux, plugin windows need X11 (XWayland on Wayland).
- The phone remote works on your local network only.
- Broadcasting: Icecast and Shoutcast only for now. Twitch, YouTube and Mixcloud Live streaming come later.
- USB export for Pioneer and Engine DJ players is experimental.
- Room EQ has no Isolator or booth EQ knobs yet; they come in 0.9.
- One instrument plugin plays at a time.
