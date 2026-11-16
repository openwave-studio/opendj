// Pioneer-OPUS-QUAD-script.js
// ****************************************************************************
// * openDJ mapping script for the Pioneer OPUS-QUAD in PC (MIDI) control
// * mode. Written from AlphaTheta's "OPUS-QUAD MIDI Message List", from
// * openDJ's XDJ-AZ script (itself from Mixxx's DDJ-400 and DDJ-FLX4
// * mappings: Warker, nschloe, dj3730, jusko, Robert904).
// ****************************************************************************
//
//  The OPUS-QUAD sends each deck on its own MIDI channel (decks 1 to 4 on
//  channels 1 to 4, DECK SELECT picks which), the browser on channel 1, the
//  mixer and BEAT FX on channel 5, and takes the LOAD lights and jog ring
//  lights on channel 12. Its SHIFT button sends a note of its own, and
//  every control sends a message of its own with SHIFT held.
//
//  The pads only play hot cues: A to H, SHIFT clears them.
//
//  The OPUS-QUAD has no MIDI message to hand its mixer to the computer; its
//  mixer section sends MIDI, which openDJ's mixer follows.
//
//  The jog ring shows the playhead going round and the cue point.
//
//  Not mapped yet: VINYL SPEED ADJUST, BEAT FX TIME, PARAMETER, SMOOTH
//  ECHO, ZONE LEVEL, INPUT SELECT, the needle press (2/4 deck waveform),
//  SYNC long press, hot cue colours. The touch screen keeps showing the
//  OPUS-QUAD's own PC-mode screen.

var PioneerOPUSQUAD = {};

PioneerOPUSQUAD.decks = ["[Channel1]", "[Channel2]", "[Channel3]", "[Channel4]"];

PioneerOPUSQUAD.deckIndex = function(group) {
    return parseInt(group.match(script.channelRegEx)[1]) - 1;
};

// The status of a deck's notes.
PioneerOPUSQUAD.deckStatus = function(group) {
    return 0x90 + PioneerOPUSQUAD.deckIndex(group);
};

PioneerOPUSQUAD.timers = {};

// Jog wheel constants
PioneerOPUSQUAD.vinylMode = [true, true, true, true];
PioneerOPUSQUAD.alpha = 1.0/8;
PioneerOPUSQUAD.beta = PioneerOPUSQUAD.alpha/32;
PioneerOPUSQUAD.fastSeekScale = 75;

PioneerOPUSQUAD.tempoRanges = [0.06, 0.10, 0.16, 0.25];

PioneerOPUSQUAD.loopAdjustIn = [false, false, false, false];
PioneerOPUSQUAD.loopAdjustMultiply = 50;

PioneerOPUSQUAD.quickJumpSize = 32;

PioneerOPUSQUAD.highResMSB = {
    "[Channel1]": {}, "[Channel2]": {}, "[Channel3]": {}, "[Channel4]": {},
};

// The jog ring has 72 lights.
PioneerOPUSQUAD.ringSteps = 72;

//
// Init
//

PioneerOPUSQUAD.init = function() {
    PioneerOPUSQUAD.decks.forEach(function(group) {
        engine.softTakeover(group, "rate", true);

        engine.makeConnection(group, "loop_enabled", PioneerOPUSQUAD.loopToggle);
        PioneerOPUSQUAD.setLoopButtonLights(PioneerOPUSQUAD.deckStatus(group), 0x7F);
        midi.sendShortMsg(PioneerOPUSQUAD.deckStatus(group), 0x12, 0x7F);

        engine.makeConnection(group, "track_loaded", PioneerOPUSQUAD.trackLoadedLED);
        engine.makeConnection(group, "sync_leader", PioneerOPUSQUAD.syncLeaderLight);
        engine.makeConnection("[QuickEffectRack1_" + group + "]", "loaded_chain_preset", PioneerOPUSQUAD.colorFxLights);
    });
    engine.softTakeover(PioneerOPUSQUAD.beatFxSlot, "meta", true);

    engine.makeConnection(PioneerOPUSQUAD.beatFxSlot, "enabled", PioneerOPUSQUAD.beatFxLight);
    PioneerOPUSQUAD.colorFxLights();

    PioneerOPUSQUAD.jogRings = [[-1, -1], [-1, -1], [-1, -1], [-1, -1]];
    PioneerOPUSQUAD.jogRingTimer = engine.beginTimer(40, PioneerOPUSQUAD.jogRingUpdate);
};

//
// Lights
//

// The LOAD button flashes when a track loads (channel 12, notes 4 to 7).
PioneerOPUSQUAD.trackLoadedLED = function(value, group) {
    midi.sendShortMsg(0x9B, 4 + PioneerOPUSQUAD.deckIndex(group), value > 0 ? 0x7F : 0x00);
};

// The jog ring's playhead light (CC 0 to 3 on channel 12, 1 to 72) goes
// round once per turn of a record at 33 1/3 rpm; its cue marker (CC 4 to 7)
// shows where the cue point sits on that turn. 0 turns either off.
PioneerOPUSQUAD.jogRingUpdate = function() {
    const step = function(seconds) {
        const turns = Math.max(0, seconds) * (100 / 3) / 60;
        return 1 + Math.floor((turns - Math.floor(turns)) * PioneerOPUSQUAD.ringSteps) % PioneerOPUSQUAD.ringSteps;
    };
    PioneerOPUSQUAD.decks.forEach(function(group, i) {
        let bar = 0;
        let cue = 0;
        if (engine.getValue(group, "track_loaded")) {
            const duration = engine.getValue(group, "duration");
            bar = step(engine.getValue(group, "playposition") * duration);
            const samples = engine.getValue(group, "track_samples");
            const rate = engine.getValue(group, "track_samplerate");
            const point = engine.getValue(group, "cue_point");
            if (samples > 0 && rate > 0 && point >= 0) {
                cue = step(point / rate / 2);
            }
        }
        const last = PioneerOPUSQUAD.jogRings[i];
        if (bar !== last[0]) {
            last[0] = bar;
            midi.sendShortMsg(0xBB, i, bar);
        }
        if (cue !== last[1]) {
            last[1] = cue;
            midi.sendShortMsg(0xBB, 4 + i, cue);
        }
    });
};

//
// Browser
//

PioneerOPUSQUAD.waveformZoom = function(_channel, _control, value) {
    const up = value < 0x40;
    PioneerOPUSQUAD.decks.forEach(function(group) {
        script.triggerControl(group, up ? "waveform_zoom_up" : "waveform_zoom_down", 100);
    });
};

//
// Deck
//

PioneerOPUSQUAD.syncPressed = function(_channel, _control, value, _status, group) {
    if (value > 0) {
        script.toggleControl(group, "sync_enabled");
    }
};

PioneerOPUSQUAD.masterPressed = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "sync_leader", engine.getValue(group, "sync_leader") === 2 ? 0 : 2);
    }
};

PioneerOPUSQUAD.syncLeaderLight = function(value, group) {
    midi.sendShortMsg(PioneerOPUSQUAD.deckStatus(group), 0x1E, value > 0 ? 0x7F : 0x00);
};

PioneerOPUSQUAD.cycleTempoRange = function(_channel, _control, value, _status, group) {
    if (value === 0) { return; }
    const curr = engine.getValue(group, "rateRange");
    let idx = 0;
    for (let i = 0; i < PioneerOPUSQUAD.tempoRanges.length; i++) {
        if (curr === PioneerOPUSQUAD.tempoRanges[i]) {
            idx = (i + 1) % PioneerOPUSQUAD.tempoRanges.length;
            break;
        }
    }
    engine.setValue(group, "rateRange", PioneerOPUSQUAD.tempoRanges[idx]);
};

PioneerOPUSQUAD.toggleButton = function(key) {
    return function(_channel, _control, value, _status, group) {
        if (value) {
            script.toggleControl(group, key);
        }
    };
};
PioneerOPUSQUAD.togglePlay = PioneerOPUSQUAD.toggleButton("play");
PioneerOPUSQUAD.toggleKeylock = PioneerOPUSQUAD.toggleButton("keylock");
PioneerOPUSQUAD.toggleQuantize = PioneerOPUSQUAD.toggleButton("quantize");
PioneerOPUSQUAD.toggleReverse = PioneerOPUSQUAD.toggleButton("reverse");

// JOG MODE: VINYL (scratch on touch) or CDJ (the top bends too).
PioneerOPUSQUAD.jogModePressed = function(channel, _control, value, status) {
    if (value) {
        PioneerOPUSQUAD.vinylMode[channel] = !PioneerOPUSQUAD.vinylMode[channel];
        midi.sendShortMsg(status, 0x12, PioneerOPUSQUAD.vinylMode[channel] ? 0x7F : 0x00);
    }
};

PioneerOPUSQUAD.quickJumpForward = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "beatjump", PioneerOPUSQUAD.quickJumpSize);
    }
};

PioneerOPUSQUAD.quickJumpBack = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "beatjump", -PioneerOPUSQUAD.quickJumpSize);
    }
};

// BEAT JUMP ◀ / ▶ jump by the deck's beat jump size; with SHIFT they halve
// or double it.
PioneerOPUSQUAD.beatJump = function(direction) {
    return function(_channel, _control, value, _status, group) {
        if (value) {
            engine.setValue(group, "beatjump", direction * engine.getValue(group, "beatjump_size"));
        }
    };
};
PioneerOPUSQUAD.beatJumpForward = PioneerOPUSQUAD.beatJump(1);
PioneerOPUSQUAD.beatJumpBack = PioneerOPUSQUAD.beatJump(-1);
PioneerOPUSQUAD.beatJumpSize = function(factor) {
    return function(_channel, _control, value, _status, group) {
        if (value) {
            const size = engine.getValue(group, "beatjump_size") * factor;
            engine.setValue(group, "beatjump_size", Math.min(64, Math.max(0.03125, size)));
        }
    };
};
PioneerOPUSQUAD.beatJumpSizeUp = PioneerOPUSQUAD.beatJumpSize(2);
PioneerOPUSQUAD.beatJumpSizeDown = PioneerOPUSQUAD.beatJumpSize(0.5);

// CROSSFADER ASSIGN (one CC a channel, 0x41 to 0x44): A, THRU or B.
PioneerOPUSQUAD.crossfaderAssign = function(_channel, control, value) {
    const group = PioneerOPUSQUAD.decks[control - 0x41];
    engine.setValue(group, "orientation", value < 0x20 ? 0 : (value > 0x60 ? 2 : 1));
};

// NEEDLE POSITION: touching the waveform on the screen jumps there.
PioneerOPUSQUAD.needleSearch = function(_channel, _control, value, _status, group) {
    if (value > 0 && !engine.getValue(group, "play")) {
        engine.setValue(group, "playposition", (value - 1) / 126);
    }
};

PioneerOPUSQUAD.tempoSliderMSB = function(_channel, _control, value, _status, group) {
    PioneerOPUSQUAD.highResMSB[group].tempoSlider = value;
};

// "-" side 0, "+" side 16383.
PioneerOPUSQUAD.tempoSliderLSB = function(_channel, _control, value, _status, group) {
    const fullValue = (PioneerOPUSQUAD.highResMSB[group].tempoSlider << 7) + value;
    engine.setValue(group, "rate", 1 - (fullValue / 0x2000));
};

//
// Jog wheels
//

PioneerOPUSQUAD.jogTurn = function(channel, _control, value, _status, group) {
    const deckNum = channel + 1;
    const newVal = value - 64;

    if (engine.getValue(group, "loop_enabled") > 0 && PioneerOPUSQUAD.loopAdjustIn[channel]) {
        engine.setValue(group, "loop_start_position",
            newVal * PioneerOPUSQUAD.loopAdjustMultiply + engine.getValue(group, "loop_start_position"));
        return;
    }

    if (engine.isScratching(deckNum)) {
        engine.scratchTick(deckNum, newVal);
    } else {
        engine.setValue(group, "jog_bend", newVal);
    }
};

PioneerOPUSQUAD.jogSearch = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "jog", (value - 64) * PioneerOPUSQUAD.fastSeekScale);
};

PioneerOPUSQUAD.jogTouch = function(channel, _control, value) {
    const deckNum = channel + 1;
    if (PioneerOPUSQUAD.loopAdjustIn[channel]) {
        return;
    }
    if (value !== 0 && PioneerOPUSQUAD.vinylMode[channel]) {
        engine.scratchEnable(deckNum, 720, 33+1/3, PioneerOPUSQUAD.alpha, PioneerOPUSQUAD.beta);
    } else {
        engine.scratchDisable(deckNum);
    }
};

//
// Loops: IN (long press: adjust), OUT, the BEAT LOOP knob
//

PioneerOPUSQUAD.toggleLoopAdjustIn = function(channel, _control, value, _status, group) {
    if (value === 0 || !engine.getValue(group, "loop_enabled")) {
        return;
    }
    PioneerOPUSQUAD.loopAdjustIn[channel] = !PioneerOPUSQUAD.loopAdjustIn[channel];
};

PioneerOPUSQUAD.setLoopButtonLights = function(status, value) {
    midi.sendShortMsg(status, 0x06, value);
    midi.sendShortMsg(status, 0x07, value);
};

// LOOP IN and OUT blink while a loop plays; LOOP OUT stays dark while the
// jog moves the loop start.
PioneerOPUSQUAD.loopToggle = function(value, group, control) {
    const status = PioneerOPUSQUAD.deckStatus(group),
        channel = PioneerOPUSQUAD.deckIndex(group);
    PioneerOPUSQUAD.timers[group] = PioneerOPUSQUAD.timers[group] || {};
    if (PioneerOPUSQUAD.timers[group][control] !== undefined) {
        engine.stopTimer(PioneerOPUSQUAD.timers[group][control]);
        PioneerOPUSQUAD.timers[group][control] = undefined;
    }
    if (value) {
        let blink = 0x7F;
        PioneerOPUSQUAD.timers[group][control] = engine.beginTimer(500, () => {
            blink = 0x7F - blink;
            midi.sendShortMsg(status, 0x06, blink);
            midi.sendShortMsg(status, 0x07, PioneerOPUSQUAD.loopAdjustIn[channel] ? 0x00 : blink);
        });
    } else {
        PioneerOPUSQUAD.setLoopButtonLights(status, 0x7F);
        PioneerOPUSQUAD.loopAdjustIn[channel] = false;
    }
};

// BEAT LOOP turned: halve (left) or double (right) the loop size, and the
// loop playing. Sends ±1 to ±30 (98 to 127 to the left).
PioneerOPUSQUAD.beatLoopTurn = function(_channel, _control, value, _status, group) {
    const steps = value >= 0x40 ? value - 0x80 : value;
    if (steps === 0) { return; }
    const factor = steps > 0 ? 2 : 0.5;
    const size = engine.getValue(group, "beatloop_size") * factor;
    if (size >= 0.03125 && size <= 512) {
        engine.setValue(group, "beatloop_size", size);
    }
    if (engine.getValue(group, "loop_enabled")) {
        script.triggerControl(group, steps > 0 ? "loop_double" : "loop_halve");
    }
};

// BEAT LOOP pressed: a loop of the loop size, or out of the loop playing.
PioneerOPUSQUAD.beatLoopPressed = function(_channel, _control, value, _status, group) {
    if (!value) { return; }
    if (engine.getValue(group, "loop_enabled")) {
        engine.setValue(group, "reloop_toggle", 1);
        engine.setValue(group, "reloop_toggle", 0);
    } else {
        engine.setValue(group, "beatloop_activate", 1);
        engine.setValue(group, "beatloop_activate", 0);
    }
};

// SHIFT + BEAT LOOP: back into the last loop, or out of it.
PioneerOPUSQUAD.beatLoopShiftPressed = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "reloop_toggle", 1);
        engine.setValue(group, "reloop_toggle", 0);
    }
};

//
// Mixer
//

PioneerOPUSQUAD.pflPressed = function(_channel, control, value) {
    if (value) {
        script.toggleControl(PioneerOPUSQUAD.decks[control - 0x46], "pfl");
    }
};

// SOUND COLOR FX buttons: each picks the Color FX the COLOR knobs play;
// pressing the lit one goes back to the filter. Numbers as in the engine's
// COLOR_FX (1 Filter, 2 Space, 3 Dub Echo, 4 Noise, 5 Crush); SWEEP plays
// the filter.
PioneerOPUSQUAD.colorFxButtons = {0x69: 2, 0x6B: 3, 0x6A: 1, 0x55: 4, 0x56: 5, 0x57: 1};

PioneerOPUSQUAD.colorFxPressed = function(_channel, control, value) {
    if (!value) { return; }
    const preset = PioneerOPUSQUAD.colorFxButtons[control];
    const current = engine.getValue("[QuickEffectRack1_[Channel1]]", "loaded_chain_preset");
    const next = current === preset && control !== 0x57 ? 1 : preset;
    PioneerOPUSQUAD.decks.forEach(function(group) {
        engine.setValue("[QuickEffectRack1_" + group + "]", "loaded_chain_preset", next);
    });
};

PioneerOPUSQUAD.colorFxLights = function() {
    const current = engine.getValue("[QuickEffectRack1_[Channel1]]", "loaded_chain_preset");
    Object.keys(PioneerOPUSQUAD.colorFxButtons).forEach(function(control) {
        const preset = PioneerOPUSQUAD.colorFxButtons[control];
        const lit = preset === current && !(preset === 1 && parseInt(control) === 0x6A);
        midi.sendShortMsg(0xB4, parseInt(control), lit ? 0x7F : 0x00);
    });
};

//
// BEAT FX: openDJ's Beat FX unit
//

PioneerOPUSQUAD.beatFxUnit = "[EffectRack1_EffectUnit5]";
PioneerOPUSQUAD.beatFxSlot = "[EffectRack1_EffectUnit5_Effect1]";

// EFFECT SELECT, by the CC each position sends, to the engine's effects
// (loaded_effect: 1 Echo, 2 Reverb, 3 Filter, 4 Flanger, 6 Phaser, 7 Gate,
// 8 Roll, 10 Brake, 12 Spiral). PITCH has no openDJ effect and keeps the
// one playing.
PioneerOPUSQUAD.beatFxEffects = {
    0x2A: 1, // DELAY
    0x37: 1, // ECHO
    0x33: 1, // PING PONG
    0x2B: 12, // SPIRAL
    0x3E: 12, // HELIX
    0x36: 2, // REVERB
    0x32: 4, // FLANGER
    0x39: 6, // PHASER
    0x3B: 3, // FILTER
    0x35: 7, // TRANS
    0x2E: 8, // ROLL
    0x3A: 8, // SPIRAL ROLL
    0x3D: 10, // VINYL BRAKE
};

PioneerOPUSQUAD.beatFxSelect = function(_channel, control, value) {
    const effect = PioneerOPUSQUAD.beatFxEffects[control];
    if (value === 0x7F && effect !== undefined) {
        engine.setValue(PioneerOPUSQUAD.beatFxSlot, "loaded_effect", effect);
    }
};

PioneerOPUSQUAD.beatFxLevelDepthRotate = function(_channel, _control, value) {
    engine.setParameter(PioneerOPUSQUAD.beatFxSlot, "meta", value / 0x7F);
};

// Halves (-1) or doubles (+1) the effect's length, within the effect's
// own limits; effects without a length stay as they are.
PioneerOPUSQUAD.changeBeatFxBeats = function(direction) {
    const key = direction > 0 ? "beats_double" : "beats_halve";
    engine.setValue(PioneerOPUSQUAD.beatFxSlot, key, 1);
    engine.setValue(PioneerOPUSQUAD.beatFxSlot, key, 0);
};

PioneerOPUSQUAD.beatFxLeftPressed = function(_channel, _control, value) {
    if (value) { PioneerOPUSQUAD.changeBeatFxBeats(-1); }
};

PioneerOPUSQUAD.beatFxRightPressed = function(_channel, _control, value) {
    if (value) { PioneerOPUSQUAD.changeBeatFxBeats(1); }
};

PioneerOPUSQUAD.beatFxTapPressed = function(_channel, _control, value) {
    if (value) {
        engine.setValue(PioneerOPUSQUAD.beatFxUnit, "bpm_tap", 1);
        engine.setValue(PioneerOPUSQUAD.beatFxUnit, "bpm_tap", 0);
    }
};

PioneerOPUSQUAD.beatFxOnOffPressed = function(_channel, _control, value) {
    if (value) {
        engine.setValue(PioneerOPUSQUAD.beatFxSlot, "enabled", !engine.getValue(PioneerOPUSQUAD.beatFxSlot, "enabled"));
    }
};

PioneerOPUSQUAD.beatFxLight = function(value) {
    midi.sendShortMsg(0xB4, 0x72, value ? 0x7F : 0x00);
};

//
// Shutdown
//

PioneerOPUSQUAD.shutdown = function() {
    engine.stopTimer(PioneerOPUSQUAD.jogRingTimer);
    PioneerOPUSQUAD.decks.forEach(function(group, i) {
        const status = PioneerOPUSQUAD.deckStatus(group);
        PioneerOPUSQUAD.setLoopButtonLights(status, 0x00);
        midi.sendShortMsg(0xBB, i, 0x00);
        midi.sendShortMsg(0xBB, 4 + i, 0x00);
        midi.sendShortMsg(0x9B, 4 + i, 0x00);
    });
    midi.sendShortMsg(0xB4, 0x72, 0x00);
};
