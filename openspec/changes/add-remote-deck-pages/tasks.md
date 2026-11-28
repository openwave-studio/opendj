# Tasks

## 1. The base: a deck on the page

- [x] 1.1 App: `deck.set` with an allow-list of deck controls, EQ and filter; `deck.load` (asks again for a playing deck); `decks`, `deck_pos` and `track1`-`track4` topics
- [x] 1.2 Remote: `/wave/<id>` serves the decks' waveforms (low, mid, high peaks at full resolution) to paired phones only; message rate raised for faders and knobs
- [x] 1.3 Page: Automix and Deck tabs (bottom on phones, top on wide screens), deck switcher starting on deck 3
- [x] 1.4 Page: waveform and overview drawn on a canvas from the fetched waveform and the streamed position, with beats, bars, hot cues and the loop
- [x] 1.5 Page: transport, pads (Hot Cue, Loop, Beat Jump), loop and jump steppers, nudge, pitch fader, channel strip; layouts for iPad landscape and portrait, phone portrait and landscape
- [x] 1.6 Update the user guide (Use an iPad as a deck)
- [ ] 1.7 Test on an iPad beside a controller (latency, touch, layout in a case)

## 2. Looks and library

- [ ] 2.1 Player look: screen, 8 hot cues, platter (touch to nudge and search), tempo fader; a look switch on the Deck page
- [ ] 2.2 Library panel on iPad: playlists, search with BPM and key filters, LOAD on each row, played tracks dimmed (Pads / Library switch on its side, always shown upright)

## 3. FX page

- [ ] 3.1 FX tab with two X-Y pads (deck FX slot and Beat FX): touch turns on, X beats, Y dry/wet, HOLD latches; effect pickers and targets as on the FX bar
- [ ] 3.2 Filter sweep as a pad effect; one pad on a phone upright

## 4. Waveform on the GPU

- [ ] 4.1 Draw the waveform with WebGPU (the app's WGSL shader), WebGL2 where WebGPU is missing, canvas as the last fallback
