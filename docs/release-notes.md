openDJ 0.9.6 is another test release on the way to 1.0. Instrument decks get a new large waveform, a smoother spectrum, knob names that fit, a preset picker and Arturia's own presets, and they can play a hardware synth such as a MicroFreak or Moog Messenger. On Apple Silicon Macs, instant and HQ stems run on the Neural Engine. A command line and an MCP server let you search, tag and sort your library, analyse tracks, make stems and plan a set from a terminal or an AI assistant, never while a deck plays. You can scratch with the on-screen jog, the phone remote gets a Deck tab, rekordbox grids import with bar 1 in place, and openDJ Studio gets an editable piano roll, MIDI recording, undo and more of Push 2.

## What's new

**Command line and MCP server**

- **`opendj <command>`** works on your library from a terminal: `search`, `track`, `lists`, `list create`, `list add`, `rate` and `tag add` / `tag remove`, with `--where <search>` in place of track ids, `--dry-run` and `--json`. The open app shows the changes within a second.
- **Prep from the command line**: `opendj analyze`, `opendj stems` (HQ stems) and `opendj health` (missing files, unanalysed tracks, no genre or rating, duplicates).
- **Set planning**: `opendj plan <list>` orders a list by flow, key or tempo and can save it as a new list, and `opendj next <track>` suggests what to play next by key and tempo.
- **`opendj mcp`** runs the same tools as an MCP server over stdio for AI assistants. It has no deck controls and no deletes, and no network port is opened.
- **Never while you play**: analysis and stems from the command line or an assistant wait while any deck plays, and only one stem separation runs at a time across the app and the command line.
- The user guide has a new **Command line** page with setup steps.

**Decks**

- **Scratch with the on-screen jog**: drag its top, or move two fingers anywhere over it on a trackpad (up or left forward, down or right back, circles follow your fingers). The rim still bends the speed.
- **Ableton Link**: the Link session's bar stays with the master deck after a beat jump or a hot cue a beat away, so OSC and the beat counter stay in step.
- **Mixer**: the EQ / STEMS / SCULPT knob mode button opens a working drop-down again, now with an icon per mode and a check on the current one.

**openDJ Studio (preview)**

- **Piano roll editing**: draw, select, move and resize notes, Velocity and Chance lanes, hear notes as you edit, a right-click menu with Quantize, Chop, Join and Fold to Scale, Live's shortcuts, and a drum mode with pad names. Edits while a clip plays swap in without a click.
- **Record MIDI into slots**: Record on an empty slot starts a clip on the next launch, Record on a note clip overdubs, and Fixed Length (1 to 128 bars) stops by itself.
- **Undo**: 64 steps of clip, mute and solo edits (Ctrl/⌘+Z, Shift for redo).
- **Mute, solo and a metronome** on every track.
- **Racks**: Live's macro mappings and ranges come in with a Live set, and plug-ins in a rack keep running when you edit it.
- **Push 2**: Device mode as in Live, plus Mute, Solo, Metronome, Tap Tempo, New, Duplicate, Double Loop, Record, Fixed Length and Undo.
- Studio still isn't in the downloads. Build it with `cargo run --profile fast --bin opendj-studio`.

**Instruments and controllers**

- **Knob names**: instrument deck knobs show long names in full on up to two lines, and a knob's value pops up beside it while you hover or turn it.
- **Hardware synths**: play an Arturia MicroFreak or Moog Messenger from an instrument deck, with named knobs, Note FX, the sequencer and clock sync. **SETUP** sets the MIDI port, audio input, gain with a limiter, and latency. Pick presets by bank or by the synth's own categories; the MicroFreak's preset names are read from the synth. A synth keeps its own patch when it loads, comes back with its last preset and moved knobs, and a keyboard's preset buttons step its presets.
- **Instrument deck large waveform**: a lilac wave with a live scope past the play line, in lilac or the instrument's own color, plus **Knobs** and **Notes** views (2 to 4 steady octaves). The small OSC scope is now a single clean line.
- **Instrument deck scope**: a smoother, flicker-free spectrum with Smooth glow, Bars and Mirror styles (right-click to pick), optional Hz labels and grid lines, and an OSC wave that holds still and fills the box.
- **Preset picker**: click an instrument's preset name, on the deck or in the plugin panel, to open a picker with search, All / Starred / Mine and sound-type groups. ‹ › step within the group you picked.
- **Arturia presets**: Arturia instruments list their own factory and user presets in the Jam column, on the deck and on controller preset buttons, tagged by sound type, genre, style and character, with your Arturia likes starred.
- **Komplete Kontrol M32**: with the keyboard's knobs over HID, they glide between steps instead of jumping, and the screen updates faster.

**Stems**

- On Macs with Apple Silicon, **instant stems** run on the Neural Engine, about 6× faster (a 6-minute track separates in about 4 seconds), with the CPU as a fallback.
- **HQ stems** also run on the Neural Engine on Macs, leaving the GPU free for the waveforms, with the GPU and then the CPU as fallbacks. The first run prepares the model once (about 20 seconds).

**Phone remote**

- A new **Deck** tab turns an iPad or phone into any deck (deck 3 at first) with a scrolling waveform, hot cue, loop and beat jump pads, pitch and the channel strip, over Wi-Fi with nothing to map.

**Fixes and checks**

- **rekordbox and MP3s**: beat grids, cues and loops on MP3s no longer land 25 or 51 ms off on USB and XML export and on rekordbox imports. openDJ now shifts them by each file's gapless trim, and by one more frame when the file starts with an Info header (thanks @M-Igashi).
- **rekordbox import**: bar 1 lands where rekordbox has it when a grid starts mid-bar, a track left on a deck no longer saves its old grid over an import, and Reset back to analysis clears the edited mark.
- The developer docs have a new page on writing a controller mapping.
- The 10-second frame log line also counts page faults and audio dropouts, to find what causes a dropout.
- The website and README link the openDJ Discord.

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
- Controllers marked Beta, including the Reloop, DDJ-1000 and Komplete Kontrol M32 mappings, haven't all been tried on the hardware yet. The S5 and S8 mappings are partial, and the FLX10 jog screens need a tester.
- Plugins: their latency isn't compensated, and one instrument plugin plays at a time. On Linux, plugin windows need X11 (XWayland on Wayland).
- Pure Data patches are experimental.
- The phone remote works on your local network only.
- Broadcasting: Icecast and Shoutcast only for now. Twitch, YouTube and Mixcloud Live streaming come later.
- USB export for Pioneer and Engine DJ players is experimental.
- Room EQ has no Isolator or booth EQ knobs yet.
- One Remix deck at a time.
- The Mixed Set Editor changes a transition's length and curve and swaps tracks. Moving transitions and drawing curves come later.
