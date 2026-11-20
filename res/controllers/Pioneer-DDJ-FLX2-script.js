// Pioneer-DDJ-FLX2-script.js
// ****************************************************************************
// * openDJ mapping script for the Pioneer DDJ-FLX2: what it changes in
// * openDJ's DDJ-FLX4 script, which drives the rest. The DDJ-FLX2 numbers its
// * decks, jogs, pads and mixer as the DDJ-FLX4 does; only its four pad mode
// * buttons sit on other notes. Written from AlphaTheta's "DDJ-FLX2 List of
// * MIDI Messages", cross-checked with rekordbox's DDJ-FLX2 mapping file.
// ****************************************************************************
//
//  Pad modes: HOT CUE, PAD FX, BEAT LOOP (SHIFT + the third button) and
//  SAMPLER. The DDJ-FLX2 has no browse knob, loop buttons, TRIM or Beat FX.
//
//  Not mapped yet: SMART CFX (the CFX knobs work as filters), SMART FADER.

var PioneerDDJFLX2 = {};

// The DDJ-FLX2's pad mode buttons (deck channel notes) and the DDJ-FLX4's
// buttons for the same modes, which the DDJ-FLX4 script knows.
PioneerDDJFLX2.modeButtons = {
    0x00: 0x1B, // HOT CUE
    0x05: 0x1E, // PAD FX
    0x01: 0x6D, // BEAT LOOP (SHIFT + BEAT JUMP on the DDJ-FLX4)
    0x03: 0x22, // SAMPLER
};

// The DDJ-FLX4 script lights the mode buttons from these: move them to the
// DDJ-FLX2's notes. Modes the DDJ-FLX2 lacks light nothing (note 0x7F,
// unused on the deck channels).
(function() {
    const notes = {
        hotcueMode: 0x00,
        padFX1Mode: 0x05,
        beatLoopMode: 0x01,
        samplerMode: 0x03,
        keyboardMode: 0x7F,
        padFX2Mode: 0x7F,
        beatJumpMode: 0x7F,
        keyShiftMode: 0x7F,
    };
    ["deck1", "deck2"].forEach(function(deck) {
        Object.keys(notes).forEach(function(mode) {
            PioneerDDJFLX4.lights[deck][mode].data1 = notes[mode];
        });
    });
})();

PioneerDDJFLX2.padModeKeyPressed = function(channel, control, value, status, group) {
    const flx4 = PioneerDDJFLX2.modeButtons[control];
    if (flx4 !== undefined) {
        PioneerDDJFLX4.padModeKeyPressed(channel, flx4, value, status, group);
    }
};

PioneerDDJFLX2.init = function() {};

PioneerDDJFLX2.shutdown = function() {};
