// Pioneer-XDJ-RX-script.js
// ****************************************************************************
// * openDJ mapping script for the XDJ-RX in PC (MIDI) control mode.
// * Written from AlphaTheta's XDJ-RX list of MIDI messages and rekordbox's XDJ-RX mapping; the work is done by
// * Pioneer-all-in-one-script.js, shared with the other all-in-ones.
// ****************************************************************************

var PioneerXDJRX = PioneerAllInOne.create({
    decks: 2,
    padModes: [0x22, 0x23, 0x24],
    colorFx: {0x55: 4, 0x56: 5, 0x57: 1},
    colorFxStatus: 0xB4,
    beatFxEffects: {0x2A: 1, 0x2B: 12, 0x2E: 8, 0x32: 4, 0x35: 7, 0x36: 2, 0x37: 1},
});
