# Proposal

Planned for: **v0.9 · In the club** (2026-11-20, a user request)

## Why

The Traktor Kontrol S2 MK3, S3 and S4 MK3 run Mixxx's HID mappings as they
are, so they don't play the way Traktor DJs expect (stems on the pads, SHIFT +
CUE to the start, flux reverse, moving the grid with GRID held), and the
Controller Editor shows nothing for them. Several Native Instruments
controllers have no mapping at all (X1 MK3, Z1 MK2, D2, S5, S8), and the MX2
mapping is bundled but not on the supported list.

## What Changes

- The Controller Editor shows HID controllers: a picture drawn from Native
  Instruments' manual (top view, scaled to the spec sheet's size), and for
  each control what it does in each layer and where it is in the input
  report. HID mappings still can't be remapped. Layouts gain `does` and `hid`
  for this.
- S2 MK3, S3, S4 MK3 and MX2 behave as Traktor 4's own mappings do where
  openDJ has the controls: stem pads (mute on pads 1-4, hold 5-8 to put a
  stem on the encoders), SHIFT + CUE back to the start, REV as flux reverse,
  hold GRID and turn the wheel to move the grid, S3 SHIFT + SYNC tempo leader
  and SHIFT + KEYLOCK tempo reset, S4 MK3 SHIFT + MASTER tempo reset, 8 hot
  cues and Traktor's deck colours.
- MX2 joins the supported list as Beta.
- New HID mappings, written for openDJ from published report layouts (not
  copied code): X1 MK3 and Z1 MK2, then D2, S5 and S8 (their screens stay
  dark). All Beta until someone tries them on the hardware.

## Capabilities

### Modified Capabilities

- `controllers`: HID controllers get pictures and Traktor-style behaviour;
  more Native Instruments controllers.

## Out of Scope

- Pioneer CDJ-3000, XDJ-700 and XDJ-1000MK2 in HID mode: they only talk after
  a challenge-response handshake with a secret each licensed vendor gets from
  AlphaTheta. Using another vendor's secret would get round that licensing,
  so these stay out (their MIDI modes work as before).
- The MX2's PATTERNS mode (no pattern player on a deck yet) and 1/32-beat
  loops (openDJ's smallest is 1/16).
- Native Instruments' own `.tsi` mapping files: the site offers none for its
  own controllers, and openDJ can't read Traktor's QML mappings.
