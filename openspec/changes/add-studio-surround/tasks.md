# Tasks

## 1. Engine

- [ ] 1.1 `surround`: layouts (Stereo, 4.0, 4.1, 5.0, 5.1) with speaker angles and HDMI channel order; pairwise constant-power gains with Width, Focus, Distance and Centre; unit tests for loudness and position
- [ ] 1.2 Surround bus beside Main: Surround Panner device last in a chain, Sub send to LFE, Unpanned (Front, Front and rear, Everywhere), Main fader and linked limiter on every speaker; saved in Studio sets
- [ ] 1.3 Orbit (synced to the clock or free), Smooth, gain ramps per block; follows Link and `process_following`
- [ ] 1.4 Speaker trims (level, delay, mute, solo) and the Test noise walk
- [ ] 1.5 Speaker render through the audio callback to each speaker's channel; Stereo fold-down

## 2. Panels

- [ ] 2.1 Surround Panner panel: room pad with speakers, gain rings and dots, knobs and Orbit block; Shift-drag and reset
- [ ] 2.2 Main's Surround panel: layout, monitor, unpanned, speaker map, channel picker, trims, Test; layout badge and per-speaker meter on Main's strip
- [ ] 2.3 Push 2 banks and MIDI mapping for both devices; Angle wraps on endless encoders

## 3. Headphones

- [ ] 3.1 Headphones render: virtual speakers through SADIE II HRIRs on `conv.rs`, resampled to the set's rate; credits and licence in the app's notices
- [ ] 3.2 Head tracking on macOS 14+: Core Motion bridge, Recenter, status, Motion permission in the bundle; HRIR crossfade as the head turns
- [ ] 3.3 Session deck plays the Headphones or Stereo render

## 4. Check

- [ ] 4.1 5.1 over HDMI to an AV receiver on macOS: Test walk lands on the right speakers, a 1-bar Orbit circles the room on the beat, no clicks while moving
- [ ] 4.2 AirPods Max: Headphones with head tracking, Recenter, the sound stays put while turning
