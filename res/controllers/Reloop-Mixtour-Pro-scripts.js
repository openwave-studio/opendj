// Reloop Mixtour Pro, written for openDJ from the MIDI notes djay Pro's
// built-in mapping uses and the controller's layout.
//
// Four decks: SHIFT + LOAD switches a side between decks 1 and 3 (left) or
// 2 and 4 (right) in the controller itself, which then sends on that deck's
// MIDI channel.
// Pads: MODE + a pad picks the pad mode of the side the pads follow. The
// pads send the same notes in every mode, so this script keeps the mode:
//   1 HOT CUE      5 AUTO LOOP
//   2 BOUNCE LOOP  6 SAMPLER
//   3 PITCH CUE    7 SAVED LOOPS (beat jump here)
//   4 INSTANT FX   8 NEURAL MIX (stems)
// STEMS EQ turns HIGH, MID and LOW into the vocals, melody and drums
// volumes.

var ReloopMixtourPro = {};

ReloopMixtourPro.HOT_CUE = 0;
ReloopMixtourPro.BOUNCE_LOOP = 1;
ReloopMixtourPro.PITCH_CUE = 2;
ReloopMixtourPro.INSTANT_FX = 3;
ReloopMixtourPro.AUTO_LOOP = 4;
ReloopMixtourPro.SAMPLER = 5;
ReloopMixtourPro.BEAT_JUMP = 6;
ReloopMixtourPro.STEMS = 7;

// Pad colours for each mode, lit and dimmed (from djay Pro's mapping).
ReloopMixtourPro.colorOn = [112, 116, 67, 124, 76, 114, 79, 83];
ReloopMixtourPro.colorDim = [48, 52, 3, 60, 12, 50, 15, 19];

ReloopMixtourPro.rolls = ["0.0625", "0.125", "0.25", "0.5", "1", "2", "4", "8"];
ReloopMixtourPro.loops = ["0.25", "0.5", "1", "2", "4", "8", "16", "32"];
ReloopMixtourPro.jumps = ["1_backward", "2_backward", "4_backward", "8_backward",
    "1_forward", "2_forward", "4_forward", "8_forward"];

ReloopMixtourPro.init = function() {
    if (engine.getValue("[App]", "num_samplers") < 8) {
        engine.setValue("[App]", "num_samplers", 8);
    }
    ReloopMixtourPro.mode = {1: 0, 2: 0, 3: 0, 4: 0};
    ReloopMixtourPro.stemsEq = {1: false, 2: false, 3: false, 4: false};
    ReloopMixtourPro.held = {1: [], 2: [], 3: [], 4: []};
    ReloopMixtourPro.sent = {};
    ReloopMixtourPro.loopInSet = {};
    ReloopMixtourPro.meters = [];
    for (var d = 1; d <= 4; d++) {
        ReloopMixtourPro.meters.push(engine.makeConnection("[Channel" + d + "]", "vu_meter",
            ReloopMixtourPro._meter(d)));
        midi.sendShortMsg(0x93 + d, 0x24, 0x01);
    }
    ReloopMixtourPro.refresh();
    ReloopMixtourPro.timer = engine.beginTimer(100, ReloopMixtourPro.refresh);
};

ReloopMixtourPro.shutdown = function() {
    for (var d = 1; d <= 4; d++) {
        for (var i = 0; i < 8; i++) {
            midi.sendShortMsg(0x93 + d, 0x14 + i, 0x00);
        }
        midi.sendShortMsg(0x93 + d, 0x24, 0x00);
        midi.sendShortMsg(0xAF + d, 0x1F, 0x00);
    }
};

ReloopMixtourPro._deck = function(group) {
    return parseInt(group.match(/Channel(\d+)/)[1], 10);
};

ReloopMixtourPro._meter = function(deck) {
    return function(value) {
        midi.sendShortMsg(0xAF + deck, 0x1F, Math.round(value * 6));
    };
};

ReloopMixtourPro._ticks = function(value) {
    return value - 0x40;
};

// BROWSE: the list; with SHIFT the sidebar; with MODE through the track.
ReloopMixtourPro.browse = function(_channel, _control, value, _status, _group) {
    engine.setValue("[Library]", "MoveVertical", ReloopMixtourPro._ticks(value));
};

ReloopMixtourPro.browseShift = function(_channel, _control, value, _status, _group) {
    engine.setValue("[Playlist]", "SelectPlaylist", ReloopMixtourPro._ticks(value));
};

ReloopMixtourPro.seek = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "beatjump", ReloopMixtourPro._ticks(value));
};

// IN-OUT-EX: loop in, then loop out, then out of the loop.
ReloopMixtourPro.inOutEx = function(_channel, _control, value, _status, group) {
    if (!value) {
        return;
    }
    if (engine.getValue(group, "loop_enabled")) {
        engine.setValue(group, "reloop_toggle", 1);
    } else if (ReloopMixtourPro.loopInSet[group]) {
        engine.setValue(group, "loop_out", 1);
        engine.setValue(group, "loop_out", 0);
        ReloopMixtourPro.loopInSet[group] = false;
    } else {
        engine.setValue(group, "loop_in", 1);
        engine.setValue(group, "loop_in", 0);
        ReloopMixtourPro.loopInSet[group] = true;
    }
};

// SHIFT + IN-OUT-EX: a loop of the set length, or out of the loop.
ReloopMixtourPro.autoLoop = function(_channel, _control, value, _status, group) {
    if (!value) {
        return;
    }
    ReloopMixtourPro.loopInSet[group] = false;
    if (engine.getValue(group, "loop_enabled")) {
        engine.setValue(group, "reloop_toggle", 1);
    } else {
        engine.setValue(group, "beatloop_activate", 1);
    }
};

// HIGH, MID and LOW: the EQ, or with STEMS EQ the vocals, melody and drums.
ReloopMixtourPro.eqKnob = function(_channel, control, value, _status, group) {
    var deck = ReloopMixtourPro._deck(group);
    var v = value / 127;
    if (ReloopMixtourPro.stemsEq[deck]) {
        var stem = {0x17: 4, 0x18: 3, 0x19: 1}[control];
        engine.setParameter("[Channel" + deck + "_Stem" + stem + "]", "volume", v);
    } else {
        var param = {0x17: "parameter3", 0x18: "parameter2", 0x19: "parameter1"}[control];
        engine.setParameter("[EqualizerRack1_[Channel" + deck + "]_Effect1]", param, v);
    }
};

ReloopMixtourPro.stemsEqButton = function(_channel, _control, value, _status, group) {
    if (!value) {
        return;
    }
    var deck = ReloopMixtourPro._deck(group);
    ReloopMixtourPro.stemsEq[deck] = !ReloopMixtourPro.stemsEq[deck];
    midi.sendShortMsg(0x93 + deck, 0x24, ReloopMixtourPro.stemsEq[deck] ? 0x7F : 0x01);
};

// FX: DRY/WET sets every deck's effects; the paddles turn a deck's effects
// on (up, stays) or on while held (down); the arrows change the amount.
ReloopMixtourPro.dryWet = function(_channel, _control, value, _status, _group) {
    for (var d = 1; d <= 4; d++) {
        engine.setParameter("[EffectRack1_EffectUnit" + d + "]", "mix", value / 127);
    }
};

ReloopMixtourPro.paddle = function(_channel, _control, value, _status, group) {
    var deck = ReloopMixtourPro._deck(group);
    var unit = "[EffectRack1_EffectUnit" + deck + "]";
    if (value) {
        engine.setValue(unit, "enabled", 1);
    }
    engine.setValue(unit, "group_[Channel" + deck + "]_enable", value ? 1 : 0);
};

ReloopMixtourPro.backspin = function(_channel, _control, value, _status, group) {
    engine.spinback(ReloopMixtourPro._deck(group), value > 0);
};

ReloopMixtourPro._amountStep = function(step) {
    for (var d = 1; d <= 4; d++) {
        var unit = "[EffectRack1_EffectUnit" + d + "]";
        var v = engine.getParameter(unit, "super1") + step;
        engine.setParameter(unit, "super1", Math.max(0, Math.min(1, v)));
    }
};

ReloopMixtourPro.amountDown = function(_channel, _control, value) {
    if (value) {
        ReloopMixtourPro._amountStep(-1 / 16);
    }
};

ReloopMixtourPro.amountUp = function(_channel, _control, value) {
    if (value) {
        ReloopMixtourPro._amountStep(1 / 16);
    }
};

// MODE + pad: the pad mode.
ReloopMixtourPro.padMode = function(_channel, control, value, _status, group) {
    if (!value) {
        return;
    }
    ReloopMixtourPro.mode[ReloopMixtourPro._deck(group)] = control;
    ReloopMixtourPro.refresh();
};

ReloopMixtourPro._stemSolo = function(deck, stem) {
    var prefix = "[Channel" + deck + "_Stem";
    var alone = !engine.getValue(prefix + stem + "]", "mute");
    for (var i = 1; i <= 4; i++) {
        if (i !== stem && !engine.getValue(prefix + i + "]", "mute")) {
            alone = false;
        }
    }
    for (var j = 1; j <= 4; j++) {
        engine.setValue(prefix + j + "]", "mute", alone || j === stem ? 0 : 1);
    }
};

ReloopMixtourPro.pad = function(_channel, control, value, _status, group) {
    var deck = ReloopMixtourPro._deck(group);
    var shift = control >= 0x1C;
    var i = control - (shift ? 0x1C : 0x14);
    var n = i + 1;
    var on = value > 0;
    ReloopMixtourPro.held[deck][i] = on;
    var unit = "[EffectRack1_EffectUnit" + deck + "]";
    switch (ReloopMixtourPro.mode[deck]) {
    case ReloopMixtourPro.HOT_CUE:
        engine.setValue(group, "hotcue_" + n + (shift ? "_clear" : "_activate"), on ? 1 : 0);
        break;
    case ReloopMixtourPro.BOUNCE_LOOP:
        engine.setValue(group, "beatlooproll_" + ReloopMixtourPro.rolls[i] + "_activate", on ? 1 : 0);
        break;
    case ReloopMixtourPro.PITCH_CUE:
        if (shift) {
            // SHIFT + pad: the hot cue the pads play.
            if (on) {
                engine.setValue(group, "hotcue_focus", n);
            }
        } else {
            engine.setValue(group, "keyboard_" + n + "_activate", on ? 1 : 0);
        }
        break;
    case ReloopMixtourPro.INSTANT_FX:
        if (!on) {
            break;
        }
        if (i < 3) {
            var fx = "[EffectRack1_EffectUnit" + deck + "_Effect" + n + "]";
            engine.setValue(fx, "enabled", engine.getValue(fx, "enabled") ? 0 : 1);
        } else if (i === 3) {
            var key = "group_[Channel" + deck + "]_enable";
            engine.setValue(unit, "enabled", 1);
            engine.setValue(unit, key, engine.getValue(unit, key) ? 0 : 1);
        } else if (i < 7) {
            engine.setValue("[EffectRack1_EffectUnit" + deck + "_Effect" + (i - 3) + "]",
                "next_effect", 1);
        }
        break;
    case ReloopMixtourPro.AUTO_LOOP:
        if (on) {
            engine.setValue(group, "beatloop_" + ReloopMixtourPro.loops[i] + "_toggle", 1);
        }
        break;
    case ReloopMixtourPro.SAMPLER:
        if (on) {
            engine.setValue("[Sampler" + n + "]", shift ? "cue_gotoandstop" : "cue_gotoandplay", 1);
        }
        break;
    case ReloopMixtourPro.BEAT_JUMP:
        if (on) {
            engine.setValue(group, "beatjump_" + ReloopMixtourPro.jumps[i], 1);
        }
        break;
    case ReloopMixtourPro.STEMS:
        if (!on) {
            break;
        }
        if (i < 4) {
            var sg = "[Channel" + deck + "_Stem" + n + "]";
            engine.setValue(sg, "mute", engine.getValue(sg, "mute") ? 0 : 1);
        } else {
            ReloopMixtourPro._stemSolo(deck, i - 3);
        }
        break;
    }
    ReloopMixtourPro.refresh();
};

// Whether pad i is lit in the deck's mode.
ReloopMixtourPro._lit = function(deck, i) {
    var g = "[Channel" + deck + "]";
    var n = i + 1;
    switch (ReloopMixtourPro.mode[deck]) {
    case ReloopMixtourPro.HOT_CUE:
        return engine.getValue(g, "hotcue_" + n + "_enabled") > 0;
    case ReloopMixtourPro.BOUNCE_LOOP:
        return !!ReloopMixtourPro.held[deck][i];
    case ReloopMixtourPro.AUTO_LOOP:
        return engine.getValue(g, "beatloop_" + ReloopMixtourPro.loops[i] + "_enabled") > 0;
    case ReloopMixtourPro.SAMPLER:
        return engine.getValue("[Sampler" + n + "]", "play") > 0;
    case ReloopMixtourPro.INSTANT_FX:
        if (i < 3) {
            return engine.getValue("[EffectRack1_EffectUnit" + deck + "_Effect" + n + "]", "enabled") > 0;
        }
        if (i === 3) {
            return engine.getValue("[EffectRack1_EffectUnit" + deck + "]",
                "group_[Channel" + deck + "]_enable") > 0;
        }
        return !!ReloopMixtourPro.held[deck][i];
    case ReloopMixtourPro.STEMS: {
        var prefix = "[Channel" + deck + "_Stem";
        if (i < 4) {
            return !engine.getValue(prefix + n + "]", "mute");
        }
        for (var s = 1; s <= 4; s++) {
            if (!engine.getValue(prefix + s + "]", "mute") !== (s === i - 3)) {
                return false;
            }
        }
        return true;
    }
    default:
        return !!ReloopMixtourPro.held[deck][i];
    }
};

ReloopMixtourPro.refresh = function() {
    for (var d = 1; d <= 4; d++) {
        var mode = ReloopMixtourPro.mode[d];
        for (var i = 0; i < 8; i++) {
            var v = ReloopMixtourPro._lit(d, i) ?
                ReloopMixtourPro.colorOn[mode] : ReloopMixtourPro.colorDim[mode];
            var key = d + ":" + i;
            if (ReloopMixtourPro.sent[key] !== v) {
                ReloopMixtourPro.sent[key] = v;
                midi.sendShortMsg(0x93 + d, 0x14 + i, v);
            }
        }
    }
};
