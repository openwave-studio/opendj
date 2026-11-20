// Pioneer-XDJ-AERO-script.js
// ****************************************************************************
// * openDJ mapping script for the XDJ-AERO in PC (MIDI) control mode.
// * Written from the XDJ-AERO's MIDI messages as rekordbox maps them; the work is done by
// * Pioneer-all-in-one-script.js, shared with the other all-in-ones.
// ****************************************************************************

var PioneerXDJAERO = PioneerAllInOne.create({
    decks: 2,
    deckFx: true,
    deckFxEffects: {0x46: 7, 0x47: 4, 0x48: 1, 0x49: 8, 0x4A: 7, 0x4B: 4, 0x4C: 1, 0x4D: 8},
});
