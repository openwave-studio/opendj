// Pioneer-XDJ-AN-script.js
// ****************************************************************************
// * openDJ mapping script for the XDJ-AN in PC (MIDI) control mode.
// * Written from the XDJ-AN's MIDI messages as rekordbox maps them; the work is done by
// * Pioneer-all-in-one-script.js, shared with the other all-in-ones.
// ****************************************************************************

var PioneerXDJAN = PioneerAllInOne.create({
    decks: 2,
    padModes: [0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57],
    beatFxEffects: {0x32: 4, 0x36: 2, 0x37: 1},
    samplers: true,
});
