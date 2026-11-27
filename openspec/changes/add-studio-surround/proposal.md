# Proposal

Planned for: **v0.9.6**. Issue #1172, a user request (2026-11-26) after an assessment of Envelop for Live (E4L) and open-source spatial audio libraries.

## Why

Producers with a home surround system (quad, 4.1 or 5.1 speakers on an AV receiver, fed from a laptop over HDMI) want to place and move sounds around the room while they make and play a set, and to hear the same mix on headphones. Today openDJ Studio is stereo: every track ends in a pan knob, and the only way to reach other speakers is Audio To an Ext. Out pair, with no panning between speakers. In Live this takes Max for Live devices (E4L, Surround Panner) that cannot run outside Max.

E4L is GPL-2 and its design is proven, but its devices are Max patches built on Max-only objects, so it is rebuilt rather than ported. For four to six speakers, panning directly between neighbouring speakers is sharper than E4L's encode-to-Ambisonics-then-decode route; an Ambisonics mode compatible with E4L can follow later.

## What Changes

- **Surround Panner**: a studio device that goes last in a track's chain and places the track in the room: a top-down position pad with the speakers and their live levels, Angle, Distance, Width, Focus, Centre and Sub, an Orbit that moves the sound in time with the set, and Smooth for gliding moves. A track with a Surround Panner leaves the stereo Main and joins the surround mix.
- **Surround output on Main**: Main gets a Surround panel with the speaker layout (Stereo, Quad 4.0, 4.1, 5.0, 5.1), the sound card channel for each speaker (HDMI order L R C LFE Ls Rs by default), what tracks without a panner do (front only, front and rear, or everywhere), per-speaker level, delay, mute and solo, and a test sound that walks around the speakers.
- **Monitor: Speakers, Headphones or Stereo**: Headphones plays the surround mix through virtual speakers on headphones (HRTF convolution, free SADIE II data); Stereo is the fold-down check. On macOS 14+ with AirPods that report head motion (AirPods Pro, AirPods Max), Head tracking keeps the virtual speakers still as the head turns, with a Recenter button.
- **Everything controllable**: every Surround Panner and Surround setting is a device parameter: MIDI-mappable, on Push 2's encoders as a bank, saved in the set, and ready for automation when the Arrangement lands. Angle suits an endless encoder (Push, Midi Fighter Twister).
- **Session deck**: openDJ's Session deck (deck 5) plays a surround set through Headphones (binaural, which is stereo) or as the stereo fold-down; it never sends speaker channels.

## Out of scope

- Speaker surround output from openDJ itself (decks or the Session deck on 4-6 speakers): openDJ's master is stereo; a later change.
- E4L-compatible 3rd-order Ambisonics bus, ambiX files, B-format reverb, ADM-OSC object positions for lights: later steps in this change's design, not in its tasks.
- Height speakers (7.1.4, Atmos) and Dolby Atmos/ADM export.
- Reading E4L or Live's Surround Panner settings from a Live set.
- Apple's personalised spatial audio profile: the HRTF is generic.
- Bass management: the receiver does it (speakers set to Small); Studio only feeds LFE from Sub sends.

## Capabilities

### New Capabilities

- `studio`: surround panning, surround output and headphone monitoring in openDJ Studio.

## Impact

- `opendj-studio-engine`: new `surround` module (layouts, pairwise panner, orbit, headphone renderer on `conv.rs`, head rotation); the engine gains a surround bus beside Main; per-speaker limiting.
- `crates/studio`: the panner's panel and pad, Main's Surround panel, the audio callback writing speaker channels, a macOS head-motion bridge (Core Motion, `NSMotionUsageDescription` in the app bundle).
- Bundled HRTF data (SADIE II, Apache-2.0), a few hundred KB at the set's sample rate.
- openDJ: the Session deck uses the Headphones or Stereo render.
