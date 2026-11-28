openDJ 0.9.5 is a test release on the way to 1.0. The top bar and mixer are tidier, with one pill style, a compact WAVE panel and a Deck options pill, and a phase meter under each 2-Deck overview helps beatmatch by ear. The instrument deck is simpler, a plugin panel in the browser shows the instrument playing, and the Jam source is one column with search, stars and filters. A first Session preview opens an Ableton Live set and launches its clips in time with the decks. Pure Data patches run as effects and instruments (experimental), HQ stems start at the playhead and export as NI STEM files, waveforms scroll more smoothly, and Windows gets a round of audio fixes. Reloop, the DDJ-1000 and the Komplete Kontrol M32 join the controllers.

## What's new

**Decks**

- **Pads**: the pad pages are four tabs again, one over each column of pads. Shift-click a tab for its partner page, and right-click it to pick what it shows. The page's setting (Quantize, Bank, Match, Roll) sits under the pads.
- **2-Deck**: the loop controls sit under the pads (IN, OUT, then the length), and Play and CUE stay under the jog.
- **Grid editor bar** under both decks in 2-Deck: grid type, BPM with Tap, ½ and ×2, downbeat, anchors and grid nudges, plus a new **Clear** for the grid. Prep uses the same bar. The Cues and Checklist tabs are gone.
- Under each deck's key, a **Master** pill and a ⚡ pill replace the B, K, S and P letters. The ⚡ pill opens **Deck options**: Match BPM, Match Key, Sync on Play and Key Lock on load.
- **Phase meter** (2-Deck): a strip under each overview shows a big beat number, the countdown to the next phrase and the current phrase's name, an EARLY / LATE meter against the master deck (the marker's colour shows how close; +1 BEAT when a beat off) and tempo arrows that point the way to move the fader.
- **2-Deck mixer**: the level meters sit beside each fader and span its travel, with a peak hold. EQ / STEMS / SCULPT is a drop-down pill, and knob names are condensed capitals that show the value for a moment while you turn.
- **FX bar** in two rows (72 px): labels move to tooltips, Band is three bars under the power button, and Beat FX sits in a lilac box with a BEAT FX tab, 1+2 routing and one **Release** button (hold to release, right-click for the mode).
- **Play** starts and pauses on the press of the button, not when you let go, so a track starts right on the beat.
- **Pitch Color FX**: pick **Pitch** from the filter knob's menu, and the COLOR knob shifts the deck up to an octave down or up while keeping the tempo. PITCH buttons on the DDJ-FLX10, DDJ-1000, XDJ-R1, XDJ-RR, DDJ-RX/RZ/RZX/SZ and DDJ-800 pick it.
- A hot cue or cue jump out of a playing loop now ends the loop.
- The top bar shows the new 1.0 logo.

**Top bar and WAVE menu**

- Every top bar button is the same 32 px pill. The clock has its own pill, and **Master** is one pill with the knob, two wider meters and a ▾ that opens the Master panel.
- **Remote** moves to **File › Remote…**. Its pill shows only while the phone remote is on, and turns green when a phone connects.
- A compact **WAVE** panel behind the waveform icon: height and zoom sliders (zoom with ten ticks), one row each for size, style, color and needle, and Phrases and Lyrics switches. BPM labels always show.
- **RGB** waveforms come in Red-Amber or Red-Green-Blue.
- On macOS, dragging the master knob no longer moves the window.

**Instruments and Jam**

- **Instrument deck**: a thin colored frame, the preset on the centre line, and a **PLAY | SEQ** switch between eight bigger knobs and the step sequencer, with a ‹ page › turner under it. A scope sits over the knobs: **OSC | FREQ** switches wave and spectrum, **PRE | POST** the instrument alone or after its effects. Dice, clear and undo have drawn icons, the scope runs full width, and the deck fits a 1280 × 720 window.
- **Note FX tiles** for Scale, Chord and Arp, with a round HOLD latch. New scales (Dorian, Phrygian, Mixolydian, Harmonic minor, Blues), chord **Strum** and arpeggiator **Swing**.
- **Plugin panel**: **OPEN** on the instrument deck shows the instrument in the browser area, at its own shape for plugins that draw a display, or as sliders for the others. The panel's header opens the plugin's own window.
- **Jam** is one column: a **Presets | Instruments** switch, search, a ☆ for starred items, and filter chips for sound type, character, instrument and maker.
- **Pure Data** (experimental): Pd patches run as effects and instruments in the plugin helper, with the cyclone, ELSE and pd-lua libraries built in. A patch's knobs, sliders and radios become openDJ knobs, Lua displays show on the instrument deck or in a VIEW panel on an FX slot, and a slot keeps its knobs and Lua state with its sound.
- **Komplete Kontrol M32** (Beta) drives the instrument deck: knobs, transport, presets, ARP and SCALE, and its screen. In the keyboard's MIDI mode its knobs still turn the instrument's knobs, and its screen shows one line in big letters.
- An instrument's notes are no longer sent twice after a late block, which could leave a note hanging.

**openDJ Studio and Session (preview)**

- First steps of openDJ Studio, a clip-launching music app built on openDJ's engine: a Session grid of looping MIDI clips launched on the bar, in time over Ableton Link. It isn't in the downloads yet. Build it with `cargo run --profile fast --bin opendj-studio`.
- In Studio, drag devices from the browser onto a track, reorder a track's chain in the device bar, or move a device to another track (Option/Alt copies it).
- Studio gains Limiter, Auto Filter and Saturator devices, MIDI effects, Simpler, racks (with a Drum Rack, macros, and MIDI effects and plug-ins inside rack chains), plug-ins in the browser, saving and opening sets, and note edits and a clip loop start in the piano roll.
- **Session** (preview), in openDJ itself: **File › Open Live Set…** opens an Ableton Live set, and its clips and scenes launch from the Jam source in time with your decks, with a mixer strip per track (pan, fader, mute, solo) and a Session channel of their own in the mixer. Plug-in tracks stay silent for now.

**Stems**

- **HQ stems from the play position**: on a loaded track the stems around the playhead are ready first, then the rest.
- **Prepare HQ Stems for All** in the sidebar menu of a crate, playlist, set or folder. The STEMS pad shows `HQ…` while waiting and `HQ 42%` while preparing.
- The HQ cache keeps three files per track instead of four, so the default 10 GB holds about a third more tracks, and the HQ model stays loaded between tracks.
- **File › Export › Stem Files…** writes NI STEM files (`.stem.mp4`) for tracks with HQ stems.
- Fixed: HQ and instant stems on tracks not at 44.1 kHz drifted out of line with the track. Older stems that drifted are made again.

**Library and analysis**

- **Background panel**: a ring at the bottom right fills while analysis, HQ stems, lyrics, sound checks or covers run. Click it to pause, stop or pick what goes next, and choose what happens while a deck plays: slow down, pause or keep going.
- Analysis runs several tracks side by side while no deck plays, and while a deck plays it saves finished tracks a few at a time, so the waveforms don't stutter.
- **Lyrics** are looked up when tracks are analysed, for files that have none.
- **BROWSE knob**: Shift+press goes straight to the rail, and turning through the rail no longer stalls.
- On narrow windows, only one of the lists panel and side panel opens over the track list at a time.
- **rekordbox import** finds beat grids for a master.db kept outside rekordbox's folder or moved from Windows, says how many tracks get grids, and no longer crashes on unanalysed tracks.
- **USB export** (experimental): players show the right DATE ADDED, quiet preview columns no longer drop to nothing, and a fresh HFS+ stick gets its library where players look. The guide covers FAT32, exFAT and GPT limits.

**Waveforms**

- The scrolling waveforms and the jog marker follow the display's refresh, so they no longer shake sideways when frames are missed.
- The overview waveform is drawn at true loudness: quiet parts sit low and 3-Band no longer fills the height.
- On computers with two graphics cards, openDJ asks for the faster one.
- The **Stems** waveform look no longer draws the stems over each other: **Stacked** cuts the track's waveform into a layer per stem, and **3 Lanes** splits drums, music and vocals. New Ember and Lilac stem colors.

**Controllers**

- **Reloop** (Beta): Mixtour, Mixtour Pro, Ready, Buddy, Mixon 4, Mixon 8 Pro, Beatpad 2, Beatmix, Elite, RP-8000 and RP-8000 MK2, with pictures and guides, and pictures for the other Reloop mappings.
- **Pioneer DJ DDJ-1000** (Beta), with its jog displays.
- **Native Instruments Traktor Kontrol S2 MK1** (Beta), with a picture.
- **DDJ-FLX10**: BROWSE press loads, loop halve and double, DECK 3/4, long-press beat jump search, and the jog screens no longer flicker and show cover art. CUE on a paused deck is quantized.
- The controller pages in the user guide have a numbered picture of each controller.

**Windows and audio**

- An ASIO driver plays as the master output (it was silent).
- Audio devices are scanned once at start-up, so Settings and device menus no longer lag or cut the sound. **Refresh devices** in Settings › Audio scans again.
- A missing master device falls back to the system default, and openDJ moves back when it returns. Looking for a lost device no longer freezes the window.
- Quitting lets go of the audio interface, MIDI clock and controllers in order, so the sound stops at once, controller lights go off, clocked gear gets Stop, and a stuck driver can no longer leave openDJ running without a window.
- Key-locked decks play on without a skip through loop controls and Key Lock toggles.
- Starting openDJ while it already runs brings the open window forward instead of opening a second copy that fights it for the audio interface and controllers.

**Fixes**

- Collection Health no longer hangs on a large imported library.
- The plugin helper no longer crashes on exit on macOS, CLAP plugins with extra ports (Cardinal, Vital) play, and synths play before their window is opened.
- Settings are kept in memory instead of being read from disk many times a second.
- A track list scrolled sideways no longer stays stuck with its first columns cut off when the window or panels resize.

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
- Controllers marked Beta, including the new Reloop, DDJ-1000 and Komplete Kontrol M32 mappings, haven't all been tried on the hardware yet. The S5 and S8 mappings are partial, and the FLX10 jog screens need a tester.
- Plugins: their latency isn't compensated, and one instrument plugin plays at a time. On Linux, plugin windows need X11 (XWayland on Wayland).
- Pure Data patches are experimental.
- The phone remote works on your local network only.
- Broadcasting: Icecast and Shoutcast only for now. Twitch, YouTube and Mixcloud Live streaming come later.
- USB export for Pioneer and Engine DJ players is experimental.
- Room EQ has no Isolator or booth EQ knobs yet.
- One Remix deck at a time.
- The Mixed Set Editor changes a transition's length and curve and swaps tracks. Moving transitions and drawing curves come later.
