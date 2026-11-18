# Design

## Where the protocol comes from

Veezuhz captured what Serato, rekordbox and VirtualDJ send the FLX10 and got
scrolling waveforms on Linux (May 2026); Loui1979 packaged it as a relay
daemon. Veezuhz's repository has no licence, so openDJ writes its own code
from the documented facts (byte positions, constants, rates) and credits
both. Loui1979's repository is public domain.

Facts used (all reports 128 bytes, no report ID, byte 0 the deck
0x10/0x20/0x30/0x40, byte 1 the kind):

- MIDI SysEx gate: a session start (`F0 00 20 7F …`), ENTER_HID
  (`F0 00 40 05 00 00 04 01 00 03 01 F7`), four per-deck inits, two globals,
  then a keepalive (`… 50 31 F7`) every 340 ms. Without it the screens ignore
  HID. The vendor control transfers some notes mention are for audio, not
  the screens, and hidapi can't send them anyway.
- `xx27` state, at 200 Hz for loaded decks only: time played as minutes,
  seconds and milliseconds (never above 999, or the screen jumps to the
  start), the track length at tempo, BPM with a 1/16 fraction, tempo in
  hundredths of a percent, the Camelot key, two counters that follow the
  time played, and constants.
- On load: `xx30` length, `xx39` HOT CUE labels (three reports), `xx35` entry
  count (three), `xx36` entries (19 a report, 150 a second, 16-bit
  height/red/green/blue), `xx2f` empty cue data. Then an `xx36` at the
  playhead every 125 ms when it moves, or the screen drops the waveform.

## Choices

- **Script, not Rust.** The protocol was found by trial on hardware and will
  need more. Testers can change the script without a build; settings sit at
  its top.
- **Waveform from the app.** `waves::set_source` takes a closure the app
  fills from the loaded tracks; `resample` keeps the loudest peak per slot.
  Colours put the strongest band at full brightness; height is the whole
  signal.
- **Uploads spread out.** Eight reports per 5 ms tick, with the state
  stream going on; no trickle for a deck while it loads.
- **The two mappings talk through free controls** `[DDJFLX10Screens],running`
  (set by the HID mapping) and `gate` (bumped by the MIDI mapping after the
  SysEx), so the screens only switch when both run and go back to MIDI when
  the HID side stops.

## Open questions for a tester

- Whether macOS and Windows let openDJ open interface 5 while the FLX10's
  audio and MIDI stay in use (Linux hidraw does).
- Whether 200 Hz state is needed, and tracks longer than 7 minutes (the entry
  count is 16 bits).
- Hot cue labels, cue data, album art (`xx33`) and the beat grid (`xx2f`)
  are left empty.
