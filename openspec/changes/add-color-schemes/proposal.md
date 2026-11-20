# Proposal

Planned for: **v1.0 · Ready for any DJ switching over** (a user request, issue #921, 2026-11-20)

## Why

DJs coming from VirtualDJ and Mixxx expect to change how the app looks, and openDJ can't do that yet beyond Dark and Light. Full skin files that rearrange the window are a large job: the default skin is compiled in, and Rust still talks to the main window through about 94 window properties and callbacks that a skin loaded at runtime would have to provide. Colours are the cheapest level to open up. Every colour already goes through the `Theme` global since Light mode, and the Pigments-style controls (#908, #920) are becoming one shared kit that a scheme can restyle in one place.

## What Changes

- `Theme` colours become values Rust sets from one token table holding the Dark and Light values, instead of `out` properties computed from `Theme.light`. Dark and Light look exactly as before.
- The colour literals still written in `.slint` files move onto tokens, except the few kept on purpose (the karaoke screen, the mini player's artwork overlay, brand art and fader caps).
- The shared controls (power button, stepper and value pills, titled menus, arc knobs, section titles) read a few style values from `Theme`: corner radius, power ring width, knob arc width, pill fill. A scheme can change them.
- Color schemes are small files in the openDJ data folder (`Color Schemes/*.toml`). Each starts from Dark or Light and overrides any token by name; tokens it leaves out keep the base value, and unknown names are listed as a warning, not an error.
- Settings › Layouts gets a Color scheme choice under Appearance: Dark, Light, Match system and each scheme found. **Make a copy** writes the current scheme to a new file with every token in it and shows it in the file manager. Schemes are read again when the window regains focus, so an edit shows without a restart.
- If text in a scheme falls under 4.5:1 contrast on its background, the Color scheme row says which tokens fail.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-layout`: Color schemes from files, on top of Dark, Light and Match system

## Out of Scope

- Skin files that move, hide or replace parts of the window (`slint-interpreter`), which come after 1.0; showing and hiding deck sections stays in add-layout-customization.
- Images, icons and fonts in a scheme.
- Waveform colours, which keep their own setting.
- A choice between the old boxed controls and the Pigments-style kit.

## Difficulty

**Medium: four PRs.** The token table and the Rust setter are mechanical and checked by screenshots (Dark and Light must not change by a pixel). Moving the last literals follows the light-mode sweep. The loader, Settings row and contrast check are small.

## Performance

Nothing per frame. Picking a scheme sets the tokens once; reading the folder on focus costs a directory listing.
