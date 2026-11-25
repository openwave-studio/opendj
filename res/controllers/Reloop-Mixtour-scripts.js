// Reloop Mixtour, written for openDJ from the MIDI notes djay Pro's built-in
// mapping uses and the controller's layout.
//
// The T and C buttons switch the four transport buttons between transport
// (LOOP, SYNC, CUE, PLAY) and hot cues 1-4 in the controller itself: each
// mode sends its own notes.
// FX: the FX button next to FILTER turns the deck's effect unit on; while
// it's on, FILTER also sets the effects' amount.

var ReloopMixtour = {};

ReloopMixtour.meterMax = 77;

ReloopMixtour.init = function() {
    ReloopMixtour.fx = {1: false, 2: false};
    var deckMeter = function(ch) {
        return function(value) {
            var v = Math.round(value * ReloopMixtour.meterMax);
            midi.sendShortMsg(0x90 + ch, 0x11, v);
            midi.sendShortMsg(0x90 + ch, 0x50, v);
        };
    };
    var mainMeter = function(ch) {
        return function(value) {
            var v = Math.round(value * ReloopMixtour.meterMax);
            midi.sendShortMsg(0x90 + ch, 0x12, v);
            midi.sendShortMsg(0x90 + ch, 0x51, v);
        };
    };
    ReloopMixtour.meters = [
        engine.makeConnection("[Channel1]", "vu_meter", deckMeter(0)),
        engine.makeConnection("[Channel2]", "vu_meter", deckMeter(1)),
        engine.makeConnection("[Master]", "vu_meter_left", mainMeter(0)),
        engine.makeConnection("[Master]", "vu_meter_right", mainMeter(1)),
    ];
    for (var d = 1; d <= 2; d++) {
        midi.sendShortMsg(0x90 + d - 1, 0x01, 0x00);
    }
};

ReloopMixtour.shutdown = function() {
    for (var ch = 0; ch < 2; ch++) {
        midi.sendShortMsg(0x90 + ch, 0x01, 0x00);
        [0x11, 0x12, 0x50, 0x51].forEach(function(no) {
            midi.sendShortMsg(0x90 + ch, no, 0x00);
        });
    }
};

ReloopMixtour._deck = function(group) {
    return parseInt(group.match(/Channel(\d+)/)[1], 10);
};

// FX: the deck's effect unit on or off, and FILTER sets its amount too.
ReloopMixtour.fxMode = function(_channel, _control, value, _status, group) {
    if (!value) {
        return;
    }
    var deck = ReloopMixtour._deck(group);
    var on = !ReloopMixtour.fx[deck];
    ReloopMixtour.fx[deck] = on;
    var unit = "[EffectRack1_EffectUnit" + deck + "]";
    engine.setValue(unit, "group_[Channel" + deck + "]_enable", on ? 1 : 0);
    if (on) {
        engine.setValue(unit, "enabled", 1);
    }
    midi.sendShortMsg(0x90 + deck - 1, 0x01, on ? 0x7F : 0x00);
};

ReloopMixtour.filter = function(_channel, _control, value, _status, group) {
    var deck = ReloopMixtour._deck(group);
    var v = value / 127;
    engine.setParameter("[QuickEffectRack1_[Channel" + deck + "]]", "super1", v);
    if (ReloopMixtour.fx[deck]) {
        // Centre: no effect; either way from the centre: more of it.
        engine.setParameter("[EffectRack1_EffectUnit" + deck + "]", "super1",
            Math.abs(v - 0.5) * 2);
    }
};

// LOOP: a loop of the set length, or out of the loop.
ReloopMixtour.loop = function(_channel, _control, value, _status, group) {
    if (!value) {
        return;
    }
    if (engine.getValue(group, "loop_enabled")) {
        engine.setValue(group, "reloop_toggle", 1);
    } else {
        engine.setValue(group, "beatloop_activate", 1);
    }
};

// BROWSE turned with T held: move through the track a beat at a time.
ReloopMixtour.seek = function(_channel, _control, value, _status, group) {
    var ticks = value < 0x40 ? value : value - 0x80;
    engine.setValue(group, "beatjump", ticks);
};
