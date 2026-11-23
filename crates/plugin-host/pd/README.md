# Pure Data patches

Example patches for openDJ's experimental Pure Data support (OpenSpec change `add-pd-rack`). The tests in `crates/plugin-host/tests/pd.rs` run them.

- `gain.pd`: one knob, `gain` (0 to 2).
- `beat-echo.pd`: an echo locked to the mix's tempo, with `beats`, `feedback` and `mix` knobs.
- `libs-check.pd`: uses cyclone and ELSE objects, for the tests.

## How a patch talks to openDJ

- Audio in and out: `[adc~ 1 2]` and `[dac~ 1 2]`.
- Knobs: `[r cutoff @hv_param 20 20000 1000]` (name, min, max, default, as in hvcc), or a number box with a receive name.
- Tempo: `[r opendj-bpm]`, `[r opendj-beat]`, `[r opendj-playing]`.
- Notes: `[notein]`, `[ctlin]`, `[bendin]`. A patch with `[notein]` is an instrument.

## Built-in libraries

cyclone (Max-style objects) and ELSE are built in, as in plugdata: write `[lores~]` or `[cyclone/lores~]`, `[knob]` or `[else/knob]`. Where both libraries have an object of the same name, the plain name is ELSE's. What's left out is listed in `third_party/pd-libs/OPENDJ.md`.

## Trying it

```sh
cargo build --profile fast -p opendj-plugin-host --features pd
cargo run --profile fast -p opendj --features pd --bin opendj
```

Put patches in openDJ's music folder › `Pd` (or point `OPENDJ_PD_DIR` at a folder), then tick them in Settings › Plugins under the Pd chip.
