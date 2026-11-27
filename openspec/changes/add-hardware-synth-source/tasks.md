# Tasks

## 1. Engine

- [x] 1.1 Instrument slot plays a hardware synth: notes and macro CCs to a MIDI port and channel at their time (MIDI thread), sound from the `Instrument` input route, scope and FX as for a plugin
- [x] 1.2 Latency look-ahead: sequencer and arp notes sent `hardware_latency` early; all notes off when the synth is unloaded

## 2. Synth profiles

- [x] 2.1 Profile format in `opendj-studio-engine` (port match, channel, named/grouped CCs, 8 macros, banks, factory names); MicroFreak, MiniFreak and Moog Messenger profiles
- [ ] 2.2 Macros and panel CONTROLS from the profile; generic profile with CC learn

## 3. Loading and setup

- [ ] 3.1 "Hardware synths" in the Jam tree; setup sheet (port, channel, input, latency); Aux strip on the same input marked in use
- [ ] 3.2 Measure latency from a test note's onset

## 4. Presets, clock and transport

- [ ] 4.1 Program change and banks on the preset bar with factory names; deck presets keep the program
- [x] 4.2 MIDI clock and Start/Stop to the synth following its deck, early by the latency

## 5. Studio

- [ ] 5.1 Studio's External Instrument uses the shared profiles (named knobs, presets)
