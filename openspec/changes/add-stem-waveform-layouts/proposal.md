# Proposal

Planned for: **v0.9.5** (issue #1132)

## Why

The Stems waveform look draws all five stems (kick, hi-hats, bass, instruments, vocals) full size on top of each other, the loudest at the back. On real music every stem is loud most of the time, so five colours fight over the same pixels and the picture reads as noise rather than showing where the vocals come in or the drums drop out. Other DJ apps either keep one waveform and recolour it, or split it into a small waveform per group of stems (djay Pro's two, three or four stem waveforms).

## What Changes

- **Stacked** (the new default for Stems): the track's own waveform shape, cut into a layer per stem that sit one on the other from the centre line out: kick, bass, instruments, vocals, then hi-hats on the outside (as the highs are in 3-Band). Each layer is as thick as its stem's share of the stems' levels, so no stem hides another and the outline matches the RGB waveform.
- **3 Lanes**: the large waveforms split into three thin waveforms, top to bottom: drums (kick and hi-hats), music (bass and instruments) and vocals, each its stems stacked as above. Overviews stay Stacked: too short for lanes.
- **Layout row** in the Wave panel under Style, only with Stems, in place of the Color row the other styles have: Stacked / 3 Lanes, kept under `waveform_stem_layout`.
- The overlapping look is removed.
- A stem's place comes from its name (kick, drums, hat, bass, vocal...), so NI STEM files with their own names land in the right lane; unknown names count as instruments.

## Out of scope

- Greying out muted stems (the waveform following the mixer): a later step.
- Lane labels drawn on the waveform.

## Capabilities

### Modified Capabilities

- `waveforms`: Stems look layouts.

## Impact

- Waveform: `crates/waveform/src/lib.rs` (`StemLayout`, `StemRole`, `Extras::stem_roles`, `WaveformView::set_stem_layout`, per-stem lane and layer uniforms); `crates/waveform/src/waveform.wgsl` (`stem_layers`, `stem_lanes` replace the overlay).
- App: `crates/app/src/wave_look.rs` (layout setting), `crates/app/src/stem_levels.rs` (roles from stem names), `crates/app/src/main.rs`; UI `crates/app/ui/wave_height.slint`, `crates/app/ui/wave_look.slint`.
