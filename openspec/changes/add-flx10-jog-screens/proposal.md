# Waveforms on the DDJ-FLX10's jog screens

## Why

The DDJ-FLX10's jog screens show BPM, time and a playhead from MIDI, but the
waveform that rekordbox, Serato and VirtualDJ draw there needs the
controller's USB HID interface. FLX10 owners ask for it (issue #850). Mixxx
users have worked out the protocol (Veezuhz, Loui1979), but Mixxx scripts
can't read a track's waveform, so they needed a separate program reading
Mixxx's analysis files. openDJ has the waveform in memory and can hand it to
the mapping.

## What Changes

- Controller scripts get `engine.getWaveform(group, perSecond)`: the deck's
  waveform as low, mid and high band peaks and the whole signal's, at any rate.
- Scripts get `engine.beginFastTimer`, down to 5 ms (Mixxx's floor is 20 ms),
  for screens that want a steady stream; HID reports sent in order
  (`useNonSkippingFIFO`) can queue by the thousand.
- A new HID mapping, "Pioneer DDJ-FLX10 jog screens", for the FLX10's
  interface 5: deck state 200 times a second (time, length, BPM, tempo,
  key), and on each load the track's length, HOT CUE labels, entry count and
  waveform, then entries at the playhead now and then.
- The FLX10 MIDI mapping switches the screens to HID with SysEx while that
  mapping runs, keeps them there, and stops its MIDI jog display meanwhile.
- An HID device that is also a MIDI port running a MIDI mapping takes its HID
  mapping's name in Settings › Controllers, so each side can be turned off.
- Linux udev rule for the FLX10; guide page with test steps.

## Impact

- `crates/controllers` (waves, prelude, host timers, HID FIFO, manager port
  names), `crates/app` (waveform source), the FLX10 mappings, docs.
- Needs an FLX10 tester: openDJ's maintainers don't have one.
