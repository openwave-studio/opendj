# Proposal

Planned for: **v2.0** (a second app; the test app ships as an experiment before then). Issue #1009, a user request (2026-11-23).

## Why

DJs who also produce or play live want a clip-launching DAW that keeps time with their DJ set: launch loops, beats and parts in time with the mix, then turn the best of it into tracks. The open-source DAWs reviewed (Ardour, Zrythm, LMMS, and the young Rust DAWs synkopa, soundcraft, transit, maolan and Suhina) either have no Session-style clip launcher, have no Ableton Link, are AGPL-only, or are large, fast-moving codebases in other UI toolkits. openDJ already has most of a live engine: Link and MIDI clock, time-stretching, an out-of-process plugin host for CLAP, VST3, AU and Pure Data, Groove Box kits, a looper, stems, OSC and controller support.

openDJ Studio is a separate app in this workspace that reuses those crates.

## What Changes

- **New app `opendj-studio`** (`crates/studio`): a Session grid of tracks and scenes. Clips launch on the next bar (or beat) of the shared clock; a scene launches its whole row; a track's stop button stops it on the next bar.
- **Shared clock**: the studio keeps its own tempo or joins the Ableton Link session, so it plays in time with openDJ and other Link apps.
- **MIDI clips first**: looping note clips play built-in sounds (openDJ's Groove Box kits and a small synth), then plugin instruments through `opendj-plugin-host`.
- **Audio clips next**: clips warped to the tempo from openDJ's library, with its beatgrids and stems.
- **Later**: piano roll and recording into slots, save and open sets, a shared transport crate split out of `opendj-engine`, an Arrangement timeline, automation, export.
- **Shared engine**: the studio's engine is its own crate with no sound card or UI (`opendj-studio-engine`). The Studio app is for production (arrangement, editing, deep device editing, saving). openDJ plays a set on a Session deck for performance: the clip grid, mixer, devices, MIDI effects, controllers and recording into clips, but no arrangement or clip editing. openDJ never uses the Studio app crate.

## Out of scope

- Forking another DAW; code from permissively licensed ones may be borrowed with notices.
- The full Studio app inside openDJ: openDJ gets the Session deck only.

## Capabilities

### New Capabilities

- `studio`: the openDJ Studio app.

## Impact

- New crate `crates/studio` (binary `opendj-studio`), depending on `opendj-engine` (kits, samples), Slint, cpal, rtrb and rusty_link.
- `opendj-engine`: `Sample::frame` becomes public for the studio's drum tracks.
