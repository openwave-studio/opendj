# Proposal

Planned for: **v1.0 · Ready for any DJ switching over** (issue #1063, a user request, 2026-11-24)

## Why

Keyboards with knobs, transport buttons and a screen (the Native Instruments Komplete Kontrol M32 first; Arturia KeyLab, Novation Launchkey and the like later) only play notes in openDJ today: their keys reach the instrument deck, but the knobs, transport and browse controls do nothing. Each keyboard differs only in how it reports its controls and draws its screen, so what the controls do belongs to openDJ, the same on every keyboard, and each mapping stays thin.

The M32's buttons, knobs, lights and screen speak USB HID, decoded from NI's firmware by the MIT-licensed komplete-keylab-32 project. Driving them over HID needs no NI software and reaches every control (SCALE, ARP and PRESET included), where NI's host-integration "DAW" protocol only passes transport and track buttons through NI's agent.

## What Changes

- **Shared keyboard behaviour** (`res/controllers/Keyboard-Instrument.js`), for any keyboard mapping:
  - 8 knobs turn the instrument's knobs on the page the deck shows, as endless encoders; SHIFT turns them finely; page buttons turn the page.
  - PLAY / STOP run the instrument deck's step sequencer; REC records the set (SHIFT: records into the sequencer); METRO is the instrument deck's metronome, TEMPO taps its tempo.
  - PRESET up / down step through the instrument's presets.
  - ARP and SCALE turn openDJ's arpeggiator and scale (Auto) on and off.
  - The browse encoder browses instruments, then the picked instrument's presets: press opens or loads, SHIFT + press goes back.
  - Four lines of text for a screen: instrument and preset; the knob turned, its name and value; the browser's list.
- **Engine and app**: `[Instrument1],knob_page` (kept in step with the deck's knob page), `preset_next`, `preset_prev`, `preset_load` and `instrument_load` (the app acts and sets them back to 0), and words for screens (`engine.getText`: instrument, preset, knob names, instrument and preset lists).
- **HID keyboards whose keys are MIDI**: a `<product keys="midi">` HID mapping names its HID side after the mapping, so the keyboard's own USB MIDI port keeps playing instruments (keys, octave buttons, pitch and mod strips, pedal) beside the HID mapping.
- **Native Instruments Komplete Kontrol M32** HID mapping (`17cc:1860`): all of the above, its button lights and its 128 x 32 screen.

## Capabilities

### New Capabilities

- `keyboard-controllers`: play and shape the instrument deck from a keyboard's knobs, transport, browse encoder and screen

### Modified Capabilities

None.

## Out of Scope (later steps)

- Other keyboards (Arturia KeyLab mk3 and Essential, Novation Launchkey, Komplete Kontrol A-series and S-series MK2/MK3), each a thin mapping on the shared behaviour.
- The M32's knob touch and touch-strip lights, and the instruments' own preset browsers inside their plugins.
- The metronome's tempo for an instrument deck that doesn't follow the mix (it uses the deck's own tempo for now).
- openDJ Studio (`crates/studio`): the same shared keyboard behaviour suits its MIDI clips and instruments once it has a control surface layer.
