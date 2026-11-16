# Proposal

Planned for: **v0.8**, after `rework-instruments` (v0.6). Issue #802, a user request (2026-11-16).

## Why

The 2-Deck instrument deck grew one feature at a time and reads as a pile of parts. The header spends 108 px on four facts (name, format, MIDI from, preset) plus a large art box. TRIM and PAN sit between a tiny floating scope and the plugin's own knobs, as if they were plugin knobs. OPEN and KEYS ON PADS look alike but do unrelated things. Scale, Chord and Arp are three dropdowns that read "Off ▾" until opened. The keyboard is squeezed into what is left of the bottom row. Hint text ("No preset: SAVE keeps this sound…", "Playing live through…") repeats what the controls already say. The Mixer and Groove Box decks already have a slim title bar; the instrument deck should match them.

## What Changes

- **Title bar** (30 px) on instrument decks, in the Mixer's and Groove Box's place: a lilac ☰ first, the plugin name in white, its format chip, the preset as ‹ name › with a heart, MIDI from, OPEN (plugin window) and ⏏. The heart is filled while the deck plays a preset unchanged. Once a knob moves, the name gets an asterisk ("Squelch Bass*") and the heart empties. A click on the heart saves over the user's own preset; for a plugin's own preset, or a sound with no preset, it asks a name and saves a copy. Save as… always makes a copy. No art on the deck; the Instruments browser keeps it.
- **☰ deck menu**, the same lilac ☰ starting the Mixer and Groove Box titles too: New preset, Save preset, Save preset as…, Rename…, Tags…, Keys on pads, Keyboard (Full, Slim, Hidden), Open plugin window, Unload deck. The Mixer and Groove Box menus start with Unload deck and grow later. The SAVE ▾ button and the KEYS ON PADS button go.
- **Body**: an in/out box (scope drawn filled, level meter, TRIM, PAN); the plugin's knobs, 8 to a page, about 10 % larger and centred between page tabs (a power mark and the page's name, the picked page outlined) and the Note FX row; Note FX as slim pills (SCALE, CHORD, ARP, HOLD) that keep their state in words, with the octave as a ‹ n › stepper on the left; a full-width keyboard.
- **Instrument color**: each instrument gets a color, shown as a soft diffused backdrop behind the knobs and on its chip and knob arcs: the color of its art when it has art, else one picked from its name out of the Mixer strips' 16 colors; ☰ › Color changes it.
- **Keybed**: shaded keys in the app's neutrals, played keys lit lilac (no blue), scale notes marked subtly; pitch bend and mod wheel strips on the keyboard's left, sending bend and CC 1 to the plugin.
- **Keyboard sizes**: Full, Slim (a 14 px strip that still lights played notes; the room shows the next knob page under the first, 16 knobs) and Hidden. Kept as a setting.
- **Smaller Scale, Chord and Arp editors**: segmented choices, the scale's or chord's notes shown on a one-octave keyboard, the arp's order as icons, Rate as a ‹ 1/16 › stepper, octaves 1-4 and a gate bar.
- **HOLD for any instrument**: latches the notes played until they are played again or HOLD is switched off; with ARP on it is the arpeggiator's hold (one HOLD, not two).
- **Page names from the plugin**: CLAP remote-control page names, VST3 unit names; numbered pages otherwise.
- Hint text on live decks goes.

## Out of scope

- One instrument per deck (`add-instrument-per-deck`).
- More Mixer and Groove Box ☰ entries beyond Unload deck.
- Light-mode-specific styling beyond using the theme's tokens.

## Capabilities

### Modified Capabilities

- `instruments`: the instrument deck's layout, deck menu, preset heart, keyboard sizes, Note FX controls and HOLD.
