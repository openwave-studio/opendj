// Pioneer-XDJ-R1-script.js
// ****************************************************************************
// * openDJ mapping script for the XDJ-R1 in PC (MIDI) control mode.
// * Written from the XDJ-R1's MIDI messages as rekordbox maps them; the work is done by
// * Pioneer-all-in-one-script.js, shared with the other all-in-ones.
// ****************************************************************************

var PioneerXDJR1 = PioneerAllInOne.create({
    decks: 2,
    colorFx: {0x4E: 1, 0x50: 5, 0x51: 4},
    colorFxStatus: 0x94,
    deckFx: true,
    deckFxEffects: {0x46: 7, 0x47: 4, 0x48: 1, 0x49: 8, 0x4A: 7, 0x4B: 4, 0x4C: 1, 0x4D: 8},
});
