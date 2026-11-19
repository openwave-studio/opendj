# The real wave, zoomed right in

Planned for: **v0.9**

## Why

The scrolling waveform is drawn from a summary of the track: band peaks and
levels a thousand times a second. That is the right picture from a few
seconds across to the whole track, but zoomed right in, where a screen pixel
covers a millisecond or less, the summary has no more to give. Each pixel
still shows a whole millisecond's peak, mirrored about the centre line, so a
kick reads as a solid block where other DJ apps show the drum's single
cycles and the gaps between them. Zoomed in is where a DJ sets a grid or a
cue on the exact transient, so this is where detail matters most.

The fix for the block's height (true loudness when zoomed in, issue #889)
opens the gaps up; this change draws the audio itself.

## What Changes

- **Real wave up close.** Once a screen pixel covers about four milliseconds
  or less, a deck's scrolling waveform fades over from the summary to the
  track's own samples: each pixel column from its lowest to its highest
  sample, swinging above and below the centre line as the audio does, in
  the look's colours (RGB, Blue, Mono and Stems without stems; 3-Band keeps
  its stacked bands). A half waveform draws the larger swing up from the
  bottom edge. Heights follow the zoomed-in summary's curve, so the fade
  doesn't jump.
- **Fast.** Only the columns on screen are read, from the decoded audio
  already in memory, on the CPU each frame. The picture moves a whole pixel
  at a time, so a column keeps its samples as it scrolls and most frames
  read only the few columns that scrolled in. A zoom step or a jump reads
  them all again, at most half a million frames a view a frame; columns
  still waiting draw from the summary until their turn, a frame later.
- **When it shows.** Only on a deck with a loaded track, once the track is
  decoded and its loudest sample is known (found on a thread of its own, in
  milliseconds); until then, and on live decks and overviews, the summary.

## Out of scope

- Drawing each band's real wave separately.
- Sample-level editing (drawing on the wave).
- The overview and the mini waveforms in lists.

## Impact

- `crates/waveform`: `Samples` trait, `WaveformView::set_samples`, the
  column cache (`detail.rs`), a detail texture per scrolling view; shader
  draws the signed shape and cross-fades it in.
- `crates/app`: each deck's decoded audio handed to its scrolling view.

## Capabilities

### Modified Capabilities

- `waveforms`: the scrolling waveform zoomed right in.
