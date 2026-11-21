// Hercules DJControl Mix Ultra, written for openDJ from the user manual and
// the MIDI notes djay Pro's built-in mapping uses.
//
// Jog wheels: the top scratches (playing or paused) and the ring bends the
// tempo; with SHIFT either one moves quickly through the track.
// NEURAL MIX: while it's on, HIGH, MID and LOW set the volume of the vocals,
// melody and drums stems instead of the EQ.

var DJCMixUltra = {};

DJCMixUltra.scratchScale = 1.0;
DJCMixUltra.seekScale = 4.0;
DJCMixUltra.bendScale = 1.0;

DJCMixUltra.init = function() {
    if (engine.getValue("[App]", "num_samplers") < 8) {
        engine.setValue("[App]", "num_samplers", 8);
    }
    DJCMixUltra.neural = false;
    DJCMixUltra.touch = {1: false, 2: false};
    DJCMixUltra.seek = {1: false, 2: false};
    midi.sendShortMsg(0x90, 0x01, 0x00);
};

DJCMixUltra.shutdown = function() {
    midi.sendShortMsg(0x90, 0x01, 0x00);
};

DJCMixUltra._deck = function(group) {
    return parseInt(group.substring(8, 9), 10);
};

DJCMixUltra._scratchEnable = function(deck) {
    var alpha = 1.0 / 8;
    engine.scratchEnable(deck, 248, 33 + 1 / 3, alpha, alpha / 32);
};

DJCMixUltra._ticks = function(value) {
    // 0x01 clockwise, 0x7F counter-clockwise (7-bit signed).
    return value < 0x40 ? value : value - 0x80;
};

DJCMixUltra.wheelTouch = function(_channel, _control, value, _status, group) {
    var deck = DJCMixUltra._deck(group);
    DJCMixUltra.touch[deck] = value > 0;
    DJCMixUltra.seek[deck] = false;
    if (value > 0) {
        DJCMixUltra._scratchEnable(deck);
    } else {
        engine.scratchDisable(deck);
    }
};

DJCMixUltra.wheelTouchShift = function(_channel, _control, value, _status, group) {
    var deck = DJCMixUltra._deck(group);
    DJCMixUltra.touch[deck] = value > 0;
    DJCMixUltra.seek[deck] = value > 0;
    if (value > 0) {
        DJCMixUltra._scratchEnable(deck);
    } else {
        engine.scratchDisable(deck);
    }
};

DJCMixUltra.scratchWheel = function(_channel, _control, value, _status, group) {
    var deck = DJCMixUltra._deck(group);
    var ticks = DJCMixUltra._ticks(value);
    if (DJCMixUltra.touch[deck] && engine.isScratching(deck)) {
        var scale = DJCMixUltra.seek[deck] ? DJCMixUltra.seekScale : DJCMixUltra.scratchScale;
        engine.scratchTick(deck, ticks * scale);
    } else {
        engine.setValue(group, "jog", ticks * DJCMixUltra.bendScale);
    }
};

DJCMixUltra.bendWheel = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "jog", DJCMixUltra._ticks(value) * DJCMixUltra.bendScale);
};

// SHIFT + jog wheel: move quickly through the track.
DJCMixUltra.scratchWheelShift = function(_channel, _control, value, _status, group) {
    var deck = DJCMixUltra._deck(group);
    var ticks = DJCMixUltra._ticks(value);
    if (engine.isScratching(deck)) {
        engine.scratchTick(deck, ticks * DJCMixUltra.seekScale);
    } else {
        engine.setValue(group, "jog", ticks * DJCMixUltra.seekScale * 2);
    }
};

DJCMixUltra.neuralMix = function(_channel, _control, value, _status, _group) {
    if (!value) {
        return;
    }
    DJCMixUltra.neural = !DJCMixUltra.neural;
    midi.sendShortMsg(0x90, 0x01, DJCMixUltra.neural ? 0x7F : 0x00);
};

// LOW, MID and HIGH: the EQ, or with NEURAL MIX the drums, melody and vocals
// stems' volumes.
DJCMixUltra.eqKnob = function(_channel, control, value, _status, group) {
    var deck = group.match(/\[Channel(\d+)\]/)[1];
    var v = value / 127;
    if (DJCMixUltra.neural) {
        var stem = {2: 1, 3: 3, 4: 4}[control];
        engine.setParameter("[Channel" + deck + "_Stem" + stem + "]", "volume", v);
    } else {
        var param = {2: "parameter1", 3: "parameter2", 4: "parameter3"}[control];
        engine.setParameter(group, param, v);
    }
};

// NEURAL MIX top pads: play that stem alone, or all four again if it already
// plays alone.
DJCMixUltra.stemSolo = function(_channel, _control, value, _status, group) {
    if (!value) {
        return;
    }
    var m = group.match(/^\[Channel(\d+)_Stem(\d)\]$/);
    var prefix = "[Channel" + m[1] + "_Stem";
    var stem = parseInt(m[2], 10);
    var alone = !engine.getValue(group, "mute");
    for (var i = 1; i <= 4; i++) {
        if (i !== stem && !engine.getValue(prefix + i + "]", "mute")) {
            alone = false;
        }
    }
    for (var j = 1; j <= 4; j++) {
        engine.setValue(prefix + j + "]", "mute", alone || j === stem ? 0 : 1);
    }
};
