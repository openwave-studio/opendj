# Tasks

## 1. Komplete Kontrol M32

- [x] 1.1 Engine and app: `[Instrument1],knob_page` in step with the deck, `preset_next` / `preset_prev` / `preset_load` / `instrument_load`, words for screens (`engine.getText`); `keys="midi"` HID mappings leave the MIDI port playing; tests
- [x] 1.2 Shared `Keyboard-Instrument.js` and the M32 HID mapping (knobs, transport, presets, ARP/SCALE, browser, lights, screen); support.json Beta, guide page, picture; tests with a stand-in device
- [ ] 1.3 Try it on the hardware (knob speed, encoder direction, lights, screen, NI Hardware Agent on macOS)
- [x] 1.4 Register with NI's host agent on macOS when it runs, so the M32's knobs come over HID (no "Template 1"); MIDI-mode knobs as the fallback
- [ ] 1.5 The same agent registration on Windows (named pipes)

## 2. More keyboards

- [ ] 2.1 Komplete Kontrol A-series and S-series MK2 (same HID family)
- [ ] 2.2 Arturia KeyLab Essential 61 next (a tester has one), then KeyLab mk3 and Novation Launchkey, over MIDI (relative knob CCs, their screens' SysEx)
- [ ] 2.4 Pad controllers that play instruments as well as the Groove Box (Launchpad X, Circuit Tracks, Maschine Mikro MK3): a mode switch between the Groove Box pads (add-groove-pads) and playing the instrument deck, each with its own docs page
- [ ] 2.3 Keyboard functions in the mapper's function list, so any MIDI keyboard can be mapped to them

## Workflow follow-up

- Archive the change once its last step is merged, so the main specs take its requirements.
