# Design

## Signal chain

```
decks + master FX + mic ─▶ Isolator ─┬─▶ recording / broadcast / loudness meter (own limiter)
                                     ├─▶ Room EQ (main) ─▶ MASTER level ─▶ limiter ─▶ main out
                                     ├─▶ Room EQ (booth) ─▶ BOOTH level ─▶ soft clip ─▶ booth out
                                     └─▶ headphones' Mix side (no Room EQ)
```

- The Isolator is part of the performance, so everything after it hears it: main, booth, headphones and recordings.
- Room EQ corrects a room, so only the outputs into that room get it. The main curve goes before the limiter, so a boost can't clip; the booth keeps its soft clip.
- The recording tap moves from after the main limiter to before Room EQ, with its own limiter at the same ceiling as today (`LIVE_CEILING` while recording or on air). With Room EQ off, a recording is the same as today's, which a test checks.

## Isolator

`dsp::Isolator` already splits into three Linkwitz-Riley bands that sum flat. The master one uses the same crossovers as the decks' isolator, with each band's gain from silent fully left, unity at the centre, to +6 dB fully right, and gains ramped per buffer so a turn doesn't click. Fully flat skips it.

## Room EQ

Six bands, each with an on flag: low cut and high cut (12 or 24 dB/octave), low and high shelf (gain ±12 dB, fixed slope), two bells (gain ±12 dB, Q 0.3 to 8). The RBJ `Biquad` in `dsp.rs` gains `set_low_shelf` and `set_high_shelf`. Coefficients are recomputed when a band changes and crossfaded over 20 ms, so dragging a dot doesn't zip. A band at 0 dB, or switched off, is skipped.

Settings keys: `room_eq` holds `main` and `booth`, each `{ on, preset, bands: [...] }`, plus `room_eq_presets` for the DJ's own presets. The engine reads them as `[RoomEQ_Main]` and `[RoomEQ_Booth]` controls (`b1_freq`, `b1_gain`, `b1_q`, `b1_on`… `b6_*`, `enabled`), so OSC reaches them like any control.

BOOTH EQ LOW and HIGH in the MASTER panel are `[RoomEQ_Booth],b2_gain` and `b5_gain` (the shelves). Turning one switches the booth curve on.

## Curve widget

A Slint component drawing the summed response (the product of the bands' magnitude responses, computed in Rust at 256 log-spaced points when a band changes) on a 20 Hz to 20 kHz log axis and ±12 dB. Dots are draggable; scroll on a dot changes Q (bells) or slope (cuts); double-click resets the band. The spectrum is an FFT of the main output taken on the UI side from the existing master copy, drawn at 30 fps only while the page is open.

## Presets

Built in: Flat; Muddy room (low shelf −3 dB at 120 Hz, bell −3 dB at 250 Hz); Harsh highs (bell −3 dB at 3 kHz, high shelf −2 dB at 10 kHz); Small speakers (low cut 60 Hz, bell +2 dB at 120 Hz). Own presets are saved by name. Editing a preset marks it modified (an asterisk) until saved or reverted.
