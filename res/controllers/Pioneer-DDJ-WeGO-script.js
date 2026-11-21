// Pioneer-DDJ-WeGO-script.js
// ****************************************************************************
// * openDJ mapping script shared by the Pioneer DDJ-WeGO, DDJ-WeGO2,
// * DDJ-WeGO3, DDJ-WeGO4 and DDJ-ERGO mappings. These controllers number their
// * decks, jogs, loops and hot cue pads the same way: deck n on MIDI channel
// * n, the mixer and browser on channel 7, the effects on channels 5 and 6.
// * Written from AlphaTheta's MIDI message lists and rekordbox's own list of
// * the controllers' messages.
// ****************************************************************************

var PioneerDDJWeGO = {};

// openDJ: touching the platter follows each deck's Vinyl mode setting.
PioneerDDJWeGO.alpha = 1.0 / 8;
PioneerDDJWeGO.beta = PioneerDDJWeGO.alpha / 32;
PioneerDDJWeGO.ticksPerTurn = 720;
PioneerDDJWeGO.bendScale = 1.0;
PioneerDDJWeGO.searchScale = 150;
PioneerDDJWeGO.tempoMSB = {};
PioneerDDJWeGO.connections = [];

PioneerDDJWeGO.deckNumber = function(group) {
    return parseInt(group.match(script.channelRegEx)[1]);
};

// Each side's effects work on its own decks: unit 1 on decks 1 and 3, unit
// 2 on decks 2 and 4.
PioneerDDJWeGO.init = function() {
    [1, 2, 3, 4].forEach(function(deck) {
        const unit = "[EffectRack1_EffectUnit" + (deck % 2 ? 1 : 2) + "]";
        engine.setValue(unit, "group_[Channel" + deck + "]_enable", 1);
    });
};

PioneerDDJWeGO.shutdown = function() {
    PioneerDDJWeGO.connections.forEach(function(c) { c.disconnect(); });
};

// The platter's top: touching it scratches when the deck's Vinyl mode is on.
PioneerDDJWeGO.jogTouch = function(_channel, _control, value, _status, group) {
    const deck = PioneerDDJWeGO.deckNumber(group);
    if (value && engine.getValue(group, "vinyl_mode") > 0) {
        engine.scratchEnable(deck, PioneerDDJWeGO.ticksPerTurn, 33 + 1 / 3,
            PioneerDDJWeGO.alpha, PioneerDDJWeGO.beta);
    } else {
        engine.scratchDisable(deck);
    }
};

// Turning the platter (0x22 with vinyl mode, 0x23 without) or the wheel's
// side (0x21): scratch while touched, bend otherwise. The side never
// scratches.
PioneerDDJWeGO.jogTurn = function(_channel, control, value, _status, group) {
    const deck = PioneerDDJWeGO.deckNumber(group);
    const ticks = value - 64;
    if (control !== 0x21 && engine.isScratching(deck)) {
        engine.scratchTick(deck, ticks);
    } else {
        engine.setValue(group, "jog_bend", ticks * PioneerDDJWeGO.bendScale);
    }
};

// SHIFT and the jog: move through the track.
PioneerDDJWeGO.jogSearch = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "jog_search", (value - 64) * PioneerDDJWeGO.searchScale);
};

// TEMPO: 14 bits, the top end slows down (as the deck's own fader).
PioneerDDJWeGO.tempoMsb = function(_channel, _control, value, _status, group) {
    PioneerDDJWeGO.tempoMSB[group] = value;
};

PioneerDDJWeGO.tempoLsb = function(_channel, _control, value, _status, group) {
    const full = ((PioneerDDJWeGO.tempoMSB[group] || 0) << 7) + value;
    engine.setValue(group, "rate", 1 - full / 0x2000);
};

// Relative knobs send 0x01-0x3F turning right and 0x7F-0x41 turning left.
PioneerDDJWeGO.direction = function(value) {
    return value < 0x40 ? 1 : -1;
};

// AUTO LOOP knob: right doubles the loop, left halves it.
PioneerDDJWeGO.loopSize = function(_channel, _control, value, _status, group) {
    script.triggerControl(group, PioneerDDJWeGO.direction(value) > 0 ? "loop_double" : "loop_halve", 50);
};

// SHIFT and the AUTO LOOP knob: move the loop a beat.
PioneerDDJWeGO.loopMove = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "loop_move", PioneerDDJWeGO.direction(value));
};

// SHIFT and the BROWSE knob: zoom the waveforms.
PioneerDDJWeGO.waveformZoom = function(_channel, _control, value) {
    const key = PioneerDDJWeGO.direction(value) > 0 ? "waveform_zoom_up" : "waveform_zoom_down";
    ["[Channel1]", "[Channel2]"].forEach(function(group) {
        script.triggerControl(group, key, 50);
    });
};
