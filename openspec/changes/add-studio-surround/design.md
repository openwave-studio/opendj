# Surround: design notes

Research: `/mnt/project-files/notes/e4l-spatial-audio-assessment.md` (project notes): E4L's devices and licence, the open-source spatial libraries compared, and the panner UI study (E4L, Live's Surround Panner, Logic).

## Approach

- **Pairwise panning, not Ambisonics, for 4-6 speakers.** With so few speakers, an Ambisonics decode spreads every source over most speakers, so it sounds blurred and is only right in one seat. Constant-power panning between the two speakers around the source's angle (2D VBAP) keeps sources sharp. Focus blends that with an even spread (Live's Focus, Logic's Diversity). Ambisonics comes later, for E4L compatibility, bigger rigs and recorded B-format.
- **Tracks stay stereo.** Racks, plug-ins and clips stay as they are. The Surround Panner is the last device; it takes the chain's stereo and writes into a surround bus beside Main instead of into Main (E4L's own pattern, with its hidden routing made explicit).
- **No new C dependencies.** The panner, orbit and headphone render are our own Rust (a few hundred lines each). libspatialaudio (LGPL-2.1+) stays plan B if objects, ADM or SOFA files are wanted quickly; SAF/SPARTA/IEM are references and test oracles.

## Engine (`opendj-studio-engine::surround`)

- `Layout { speakers: [Speaker; N] }`, `Speaker { name, angle, lfe, channel }`. Presets: Quad ±45°/±135°; 5.0/5.1 ITU 0°, ±30°, ±110°; LFE has no angle.
- `Panner` settings: angle, distance, width, mono, focus, centre, sub, smooth, orbit {on, sync, rate, shape, direction, range, phase}. Each block it works out per-speaker gains for the left and right sides (angle ± width/2) and ramps from the last block's gains over the block, so moves never click. Smooth is a one-pole glide on angle (shortest way round) and distance.
  - Gains: the two speakers around the angle by tangent law, power-normalised; Focus mixes with an equal-power spread over all main speakers (`g = sqrt(f·pair² + (1-f)/N)`), renormalised; Distance below 100 % mixes towards the even spread too (centre = everywhere). Centre scales how much of the C speaker's share goes to C vs L/R. Sub is a separate send to LFE (no filter; the receiver's crossover handles it).
  - Orbit: an LFO on angle, phase from the engine's beat when synced (`beat / rate_beats + phase`), so it follows the studio clock, Link and `process_following` the same way.
- **Surround bus**: `Engine` gets `surround: Vec<f32>` (`MAX_BLOCK × 6`, allocated up front like `ext`). Panned tracks add into it after their fader; unpanned tracks go to Main and, per the Unpanned choice, also into the rear or all speakers. Main's fader and soft clipper apply to every speaker channel; the limiter links its gain across channels so the image doesn't shift.
- **Speaker trims**: level, delay (a small ring per speaker, 30 ms max), mute, solo; test noise as a source in the bus.
- **Render**:
  - Speakers: the bus goes to each speaker's sound card channel (`audio.rs` already opens every channel of the device and adds `ext`; it adds the surround channels the same way). Main L/R and the bus's L/R are one signal.
  - Stereo: ITU fold-down (C and rears at -3 dB into L/R, LFE dropped).
  - Headphones: each main speaker convolved with the HRIR pair for its angle (`conv.rs` gives mono-in, stereo-out partitioned convolution) and summed; 5 convolvers at 256-sample blocks, about 1-2 % of a core. Head tracking subtracts the head's yaw from every speaker angle and crossfades between the two nearest measured HRIRs (SADIE II has a dense horizontal ring), recomputed at most every 10 ms with a short crossfade so turning doesn't zip.
- `process` and `process_following` keep returning stereo; a new `render_speakers(out: &mut [f32], channels)` gives the interleaved speaker channels. The Session deck calls the stereo path, which is the Headphones or Stereo render.

## Head tracking (macOS)

- `crates/studio/src/head.rs`: `CMHeadphoneMotionManager` through `objc2-core-motion` on a background queue; yaw, pitch and roll in radians into an atomic the audio thread reads (pitch/roll unused until height speakers). Recenter stores the current yaw as zero.
- Needs `NSMotionUsageDescription` in the app's Info.plist; a plain `cargo run` binary has no bundle, so tracking works in the packaged and nightly builds (and a `scripts/` bundle helper for testing).
- Off and greyed elsewhere (Linux, Windows); OSC head input (`/studio/head yaw pitch roll`) can come later for other trackers.
- The system's own Spatial Audio would rotate the sound a second time; the panel says to set it to Off for Studio (Control Centre › Sound).

## UI

- **Surround Panner panel** (device row): left, a square room pad (speakers as dots with gain rings, the source as one or two lilac dots, orbit path dashed); right, knobs Angle, Distance, Width, Focus, Centre, Sub, Smooth, and an Orbit block (On, Sync/Free, Rate, Shape, Direction, Range, Phase). Shift-drag locks Distance (moves angle only); double-click the dot resets to front.
- **Main's Surround panel** (shown when Main is selected, where a track's devices show): Layout, Monitor (Speakers / Headphones / Stereo), Unpanned (Front / Front and rear / Everywhere) with Rear level, speaker map from above with per-speaker meter, channel picker, trim, delay, mute, solo; Test; Head tracking, Recenter, status.
- Main's strip shows the layout as a badge ("5.1", "HP") and its meter grows to one bar per speaker.
- Colours and knobs from Studio's theme; Live's look for device panels.

## Control

- Both devices expose `Param`s like the other studio devices, so MIDI mapping, Push banks and saving come for free. Angle is `bipolar` with wrap-around on relative encoders.
- Push 2 bank 1: Angle, Distance, Width, Focus, Orbit Rate, Orbit Range, Centre, Sub; bank 2: Orbit On, Shape, Direction, Phase, Smooth, Mono.
- Automation lands with the Arrangement (`add-opendj-studio`); Smooth exists partly for it.

## Later (not in tasks)

1. Ambisonics mode (3rd order, ambiX ACN/SN3D) beside pairwise: E4L-compatible panning, rotation, ambiX file playback and 16-channel export; decoders generated from the BSD-3 Ambisonic Decoder Toolbox Faust files.
2. Surround effects: Multi-Delay and Brownian Delay around the room, a four-channel decorrelated reverb, a surround meter/scope.
3. ADM-OSC object positions out (lights and visuals), and OSC head-tracking in.
4. openDJ speaker surround: a multichannel master, deck positions and an orbit beat FX.
