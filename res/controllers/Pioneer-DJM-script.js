// Pioneer-DJM-script.js
// ****************************************************************************
// * openDJ mapping script for Pioneer DJ / AlphaTheta mixers: the DJM club
// * mixers (DJM-250MK2, 450, 750MK2, 900NXS2, A9, V5, V10, TOUR1), the
// * euphonia rotary mixer, the DJM-S7, S9, S11 and T1 battle mixers, and the
// * PLX-CRSS12 turntable's pads. Written from the mixers' MIDI messages.
// ****************************************************************************
//
//  These mixers mix the sound themselves. openDJ plays each deck out of
//  its own USB channel (Settings › Audio › Mixing › External mixer), and the
//  mixer's MIDI tells openDJ which decks are on air: a deck counts as
//  playing to the room when its channel's INPUT SELECT is on this
//  computer's USB port and its fader is up. openDJ's channel volume follows
//  that, so the track list, played marks and the meters know what the room
//  hears. With openDJ's own mixer instead of External mixer, the faders
//  turn openDJ's decks down as well as the mixer's channels.
//
//  The club mixers send their channel faders (euphonia: its rotary volume
//  knobs), crossfader, crossfader assign and INPUT SELECT; the rest of their
//  knobs stay on the mixer. The battle mixers (DJM-S7, S9, S11) and the
//  DJM-T1 also run decks: browser, loops, performance pads and effects.

var PioneerDJM = {};

// "A" or "B": the mixer's USB port this computer is on. The mixer says so
// when it connects (USB A / USB B notes); A until it does.
PioneerDJM.port = "A";

// Each channel's INPUT SELECT ("A", "B" or "other") and fader (0 to 1).
// Until the mixer sends its INPUT SELECT, a channel counts as on USB.
PioneerDJM.source = {};
PioneerDJM.level = {};

// 14-bit controls: the last MSB of each.
PioneerDJM.msb = {};

// The lit pad mode button on each deck channel.
PioneerDJM.mode = {};

PioneerDJM.init = function() {};

PioneerDJM.shutdown = function() {};

PioneerDJM.onAir = function(group) {
    var source = PioneerDJM.source[group];
    return source === undefined || source === PioneerDJM.port;
};

PioneerDJM.apply = function(group) {
    var level = PioneerDJM.level[group];
    if (level === undefined) {
        return;
    }
    engine.setParameter(group, "volume", PioneerDJM.onAir(group) ? level : 0);
};

PioneerDJM.applyAll = function() {
    for (var i = 1; i <= 4; i++) {
        PioneerDJM.apply("[Channel" + i + "]");
    }
};

//
// INPUT SELECT and the USB port
//

PioneerDJM.inputUsbA = function(_channel, _control, value, _status, group) {
    if (value) {
        PioneerDJM.source[group] = "A";
        PioneerDJM.apply(group);
    }
};

PioneerDJM.inputUsbB = function(_channel, _control, value, _status, group) {
    if (value) {
        PioneerDJM.source[group] = "B";
        PioneerDJM.apply(group);
    }
};

PioneerDJM.inputOther = function(_channel, _control, value, _status, group) {
    if (value) {
        PioneerDJM.source[group] = "other";
        PioneerDJM.apply(group);
    }
};

PioneerDJM.usbPortA = function(_channel, _control, value) {
    if (value) {
        PioneerDJM.port = "A";
        PioneerDJM.applyAll();
    }
};

PioneerDJM.usbPortB = function(_channel, _control, value) {
    if (value) {
        PioneerDJM.port = "B";
        PioneerDJM.applyAll();
    }
};

//
// Faders
//

PioneerDJM.fader = function(_channel, _control, value, _status, group) {
    PioneerDJM.level[group] = value / 0x7F;
    PioneerDJM.apply(group);
};

// 14-bit faders send the MSB then the LSB 0x20 above it. The MSB alone
// moves the fader, so the mixer works with its 7-bit setting too.
PioneerDJM.fourteen = function(status, control, value, lsb) {
    var key = status + ":" + (lsb ? control - 0x20 : control);
    if (!lsb) {
        PioneerDJM.msb[key] = value;
        return (value << 7) / 0x3FFF;
    }
    var msb = PioneerDJM.msb[key] || 0;
    return ((msb << 7) + value) / 0x3FFF;
};

PioneerDJM.faderMSB = function(_channel, control, value, status, group) {
    PioneerDJM.level[group] = PioneerDJM.fourteen(status, control, value, false);
    PioneerDJM.apply(group);
};

PioneerDJM.faderLSB = function(_channel, control, value, status, group) {
    PioneerDJM.level[group] = PioneerDJM.fourteen(status, control, value, true);
    PioneerDJM.apply(group);
};

PioneerDJM.crossfader = function(_channel, _control, value) {
    engine.setParameter("[Master]", "crossfader", value / 0x7F);
};

PioneerDJM.crossfaderMSB = function(_channel, control, value, status) {
    engine.setParameter("[Master]", "crossfader", PioneerDJM.fourteen(status, control, value, false));
};

PioneerDJM.crossfaderLSB = function(_channel, control, value, status) {
    engine.setParameter("[Master]", "crossfader", PioneerDJM.fourteen(status, control, value, true));
};

// A (0x00), THRU (0x40) or B (0x7F).
PioneerDJM.crossfaderAssign = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "orientation", value < 0x20 ? 0 : (value > 0x60 ? 2 : 1));
};

//
// Browser and decks
//

// Rotaries send 0x01 up for each click clockwise and 0x7F down
// counterclockwise.
PioneerDJM.delta = function(value) {
    return value < 0x40 ? value : value - 0x80;
};

PioneerDJM.zoom = function(_channel, _control, value) {
    var key = PioneerDJM.delta(value) > 0 ? "waveform_zoom_up" : "waveform_zoom_down";
    for (var i = 1; i <= 4; i++) {
        engine.setValue("[Channel" + i + "]", key, 1);
    }
};

// Halves or doubles the loop size, and the loop playing.
PioneerDJM.loopSize = function(_channel, _control, value, _status, group) {
    var up = PioneerDJM.delta(value) > 0;
    if (engine.getValue(group, "loop_enabled")) {
        engine.setValue(group, up ? "loop_double" : "loop_halve", 1);
        return;
    }
    var size = engine.getValue(group, "beatloop_size");
    engine.setValue(group, "beatloop_size", up ? Math.min(size * 2, 512) : Math.max(size / 2, 1 / 32));
};

PioneerDJM.loopMove = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "loop_move", PioneerDJM.delta(value) > 0 ? 1 : -1);
};

PioneerDJM.quantize = function(_channel, _control, value) {
    if (!value) {
        return;
    }
    var on = engine.getValue("[Channel1]", "quantize") ? 0 : 1;
    for (var i = 1; i <= 4; i++) {
        engine.setValue("[Channel" + i + "]", "quantize", on);
    }
};

// Lights the pad mode button pressed and puts out the deck's last one.
PioneerDJM.padMode = function(_channel, control, value, status) {
    if (!value) {
        return;
    }
    var last = PioneerDJM.mode[status];
    if (last !== undefined && last !== control) {
        midi.sendShortMsg(status, last, 0x00);
    }
    midi.sendShortMsg(status, control, 0x7F);
    PioneerDJM.mode[status] = control;
};

//
// Effects: effect units 1 and 2 stand for the mixer's FX1 and FX2.
//

PioneerDJM.unitOf = function(group) {
    return group.replace(/_Effect\d\]$/, "]");
};

PioneerDJM.fxDepth = function(_channel, _control, value, _status, group) {
    engine.setParameter(group, "meta", value / 0x7F);
};

PioneerDJM.fxDepthMSB = function(_channel, control, value, status, group) {
    engine.setParameter(group, "meta", PioneerDJM.fourteen(status, control, value, false));
};

PioneerDJM.fxDepthLSB = function(_channel, control, value, status, group) {
    engine.setParameter(group, "meta", PioneerDJM.fourteen(status, control, value, true));
};

PioneerDJM.toggleEnabled = function(_channel, _control, value, _status, group) {
    if (value) {
        script.toggleControl(group, "enabled");
    }
};

// A channel's FX lever: the effect plays on that channel while it's held.
PioneerDJM.fxLever = function(channel, value, group) {
    if (value) {
        engine.setValue(PioneerDJM.unitOf(group), "group_[Channel" + channel + "]_enable", 1);
    }
    engine.setValue(group, "enabled", value ? 1 : 0);
};

PioneerDJM.fxLeverCh1 = function(_channel, _control, value, _status, group) {
    PioneerDJM.fxLever(1, value, group);
};

PioneerDJM.fxLeverCh2 = function(_channel, _control, value, _status, group) {
    PioneerDJM.fxLever(2, value, group);
};

PioneerDJM.fxSelect = function(_channel, _control, value, _status, group) {
    engine.setValue(group, PioneerDJM.delta(value) > 0 ? "next_effect" : "prev_effect", 1);
};

PioneerDJM.fxAssign = function(channel, value, group) {
    if (value) {
        script.toggleControl(group, "group_[Channel" + channel + "]_enable");
    }
};

PioneerDJM.fxAssign1 = function(_channel, _control, value, _status, group) {
    PioneerDJM.fxAssign(1, value, group);
};

PioneerDJM.fxAssign2 = function(_channel, _control, value, _status, group) {
    PioneerDJM.fxAssign(2, value, group);
};

PioneerDJM.samplerVolume = function(level) {
    var count = engine.getValue("[Master]", "num_samplers") || 8;
    for (var i = 1; i <= count; i++) {
        engine.setParameter("[Sampler" + i + "]", "volume", level);
    }
};

PioneerDJM.samplerVolumeMSB = function(_channel, control, value, status) {
    PioneerDJM.samplerVolume(PioneerDJM.fourteen(status, control, value, false));
};

PioneerDJM.samplerVolumeLSB = function(_channel, control, value, status) {
    PioneerDJM.samplerVolume(PioneerDJM.fourteen(status, control, value, true));
};
