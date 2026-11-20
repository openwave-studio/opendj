// Pioneer-XDJ-XZ-script.js
// ****************************************************************************
// * openDJ mapping script for the XDJ-XZ in PC (MIDI) control mode.
// * Written from AlphaTheta's XDJ-XZ MIDI message list and rekordbox's XDJ-XZ mapping; the work is done by
// * Pioneer-all-in-one-script.js, shared with the other all-in-ones.
// ****************************************************************************

var PioneerXDJXZ = PioneerAllInOne.create({
    decks: 4,
    jogDisplay: true,
    loadLights: true,
    padModes: [0x22, 0x23, 0x24, 0x25],
    samplers: true,
});
