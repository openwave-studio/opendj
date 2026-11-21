// Pioneer-XDJ-RR-script.js
// ****************************************************************************
// * openDJ mapping script for the XDJ-RR in PC (MIDI) control mode.
// * Written from the XDJ-RR's MIDI messages as rekordbox maps them; the work is done by
// * Pioneer-all-in-one-script.js, shared with the other all-in-ones.
// ****************************************************************************

var PioneerXDJRR = PioneerAllInOne.create({
    decks: 2,
    padModes: [0x22, 0x23, 0x24, 0x25],
    colorFx: {0x55: 4, 0x57: 1, 0x6A: 3},
    colorFxStatus: 0xB4,
    beatFxEffects: {0x32: 4, 0x36: 2, 0x37: 1},
});
