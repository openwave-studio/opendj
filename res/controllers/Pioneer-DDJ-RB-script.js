// Pioneer-DDJ-RB-script.js
// ****************************************************************************
// * openDJ additions for the Pioneer DDJ-RB, which runs on the DDJ-400 script.
// * The RB numbers its pads like the DDJ-400 but sends deck 1's pads on MIDI
// * channel 8 and deck 2's on channel 9, with SHIFT adding 8 to the note
// * instead of moving to the next channel. Written from AlphaTheta's MIDI
// * message list.
// ****************************************************************************

var PioneerDDJRB = {};

// BEAT JUMP pads: back and forward by 1, 2, 4 and 8 beats.
PioneerDDJRB.beatjumpSizes = [-1, 1, -2, 2, -4, 4, -8, 8];

// FX1 (the DDJ-400 script's Beat FX unit) works on deck 1 and FX2 (effect
// unit 2) on deck 2, each side's own effect.
PioneerDDJRB.init = function() {
    engine.setValue("[EffectRack1_EffectUnit5]", "route", 1);
    engine.setValue("[EffectRack1_EffectUnit2]", "group_[Channel2]_enable", 1);
};

PioneerDDJRB.shutdown = function() {};

PioneerDDJRB.beatjumpPad = function(_channel, control, value, _status, group) {
    if (value) {
        engine.setValue(group, "beatjump", PioneerDDJRB.beatjumpSizes[control & 0x07]);
    }
};

// FX2 BEAT ◀ ▶: halve or double the effect's length, as FX1's do.
PioneerDDJRB.fx2Beat = function(_channel, control, value, _status, group) {
    if (value) {
        const key = control === 0x4A ? "beats_halve" : "beats_double";
        engine.setValue(group, key, 1);
        engine.setValue(group, key, 0);
    }
};

// Sampler pads blink while their sampler plays: samplers 1-8 on deck 1's
// pads, 9-16 on deck 2's, and the same pad under SHIFT (note + 8).
PioneerDDJ400.samplerPlayOutputCallbackFunction = function(value, group, _control) {
    if (value === 1) {
        const curPad = parseInt(group.match(script.samplerRegEx)[1]);
        PioneerDDJ400.startSamplerBlink(
            curPad > 8 ? 0x98 : 0x97,
            0x30 + ((curPad > 8 ? curPad - 8 : curPad) - 1),
            group);
    }
};

PioneerDDJ400.startSamplerBlink = function(status, control, group) {
    let val = 0x7F;
    PioneerDDJ400.stopSamplerBlink(status, control);
    PioneerDDJ400.timers[status][control] = engine.beginTimer(250, () => {
        val = 0x7F - val;
        midi.sendShortMsg(status, control, val);
        midi.sendShortMsg(status, control + 8, val);
        if (engine.getValue(group, "play") !== 1) {
            PioneerDDJ400.stopSamplerBlink(status, control);
            midi.sendShortMsg(status, control, 0x7F);
            midi.sendShortMsg(status, control + 8, 0x7F);
        }
    });
};
