// Pioneer-DDJ-GRV6-script.js
// ****************************************************************************
// * openDJ mapping script for the AlphaTheta (Pioneer DJ) DDJ-GRV6.
// * Built on openDJ's DDJ-FLX6 script (itself from its DDJ-FLX4 script and
// * Mixxx's DDJ-400 and DDJ-FLX4 mappings: Warker, nschloe, dj3730, jusko,
// * Robert904), written from AlphaTheta's "DDJ-GRV6 List of MIDI messages".
// ****************************************************************************
//
//  The GRV6 sends each deck on its own MIDI channel: decks 1 to 4 on
//  channels 1 to 4, their pads on channels 8/9, 10/11, 12/13 and 14/15
//  (the second of each pair with SHIFT held), as the FLX6 does. The DECK
//  buttons pick which deck each side plays; the mixer always has all four
//  channels.
//
//  As on the FLX4 (see its script), the pad mode buttons open the deck's
//  pad tabs and the pads play whatever page is in the tab. The GRV6's
//  buttons are HOT CUE (SHIFT: KEYBOARD), STEMS (SHIFT: PAD FX), BEAT JUMP
//  (SHIFT: BEAT LOOP) and SAMPLER (SHIFT: KEY SHIFT); the STEMS button
//  opens the stems pads.
//
//  BEAT FX drives openDJ's one Beat FX unit: BEAT FX SELECT picks the
//  nearest openDJ effect, CH SELECT sends it to one deck or, on MASTER, the
//  main mix, LEVEL/DEPTH is its depth, ON/OFF turns it on and off (SHIFT:
//  release) and BEAT < > halve and double its length (SHIFT: AUTO and TAP
//  tempo). SOUND COLOR FX turns the COLOR knobs' effect on and off (SHIFT:
//  the next Color FX).
//
//  GROOVE CIRCUIT works on the deck's drum stems (kick and hi-hats; the
//  drums of a 4-stem track), so it needs a track with stems:
//    GAIN is the drums' level (centre: as recorded),
//    DRUM SWAP 1 to 4 take out the kick, the hi-hats, all the drums, or
//      everything but the drums (lit while they do; press again to undo);
//      SHIFT + DRUM SWAP picks Bank A (those) or Bank B (the bass, the
//      instruments, the vocals, everything but the vocals), and Single
//      (one swap at a time) or Multi (they add up),
//    DRUM ROLL 1 to 4 roll the drums while held (1/8, 1/4, 1/2 and 1
//      beat; the whole deck without stems), SHIFT (DRUM TRANS) chops them,
//    DRUM RELEASE echoes the drums out while held and brings them back
//      dry when let go (SHIFT, FX SELECT: echo or reverb).
//  CAPTURE grabs the last bars of the mix into an open Looper deck (SHIFT,
//  Length: 1, 2, 4 or 8 bars); openDJ has no drum slots to fill.
//
//  The BEAT FX display's beat lights show the effect's length.
//
//  The jog rings show the playhead going round as on a 33 1/3 record.
//
//  CUE/LOOP CALL jump to the hot cue before or after the playhead, or
//  while a loop plays halve and double it (SHIFT: 1/2X and 2X).
//
//  SHIFT + CUE turns PART ISOLATOR on: the EQ knobs set the vocals',
//  instruments' and drums' levels. SHIFT + QUANTIZE taps the BPM.
//
//  AlphaTheta's MIDI message list and rekordbox's own GRV6 mapping
//  disagree in one place: tilting the SMART ROTARY SELECTOR right sends
//  note 0x3E (rekordbox), not 0x2E (the list); both are mapped.
//
//  Not mapped yet: CUE/LOOP MEMORY, DUAL, MASTER CUE, the MONO/STEREO switch and the fader start messages.

var PioneerDDJGRV6 = {};

// The four decks: "[ChannelN]" from 0 to 3.
PioneerDDJGRV6.decks = ["[Channel1]", "[Channel2]", "[Channel3]", "[Channel4]"];

// A deck's index (0 to 3) from its group.
PioneerDDJGRV6.deckIndex = function(group) {
    return parseInt(group.match(script.channelRegEx)[1]) - 1;
};

// The status of a deck's notes and its pads (without SHIFT; +1 with it).
PioneerDDJGRV6.deckStatus = function(group) {
    return 0x90 + PioneerDDJGRV6.deckIndex(group);
};
PioneerDDJGRV6.padStatus = function(group) {
    return 0x97 + 2 * PioneerDDJGRV6.deckIndex(group);
};

// The deck playing on each side (left decks 1 and 3, right 2 and 4), as
// the DECK buttons last set it.
PioneerDDJGRV6.sideDeck = [0, 1];

PioneerDDJGRV6.noteSide = function(deck) {
    PioneerDDJGRV6.sideDeck[deck % 2] = deck;
};

// The pad mode buttons' lights, the same notes on every deck.
PioneerDDJGRV6.padModeLights = {
    hotcueMode: 0x1B,
    keyboardMode: 0x69,
    padFX1Mode: 0x1E,   // STEMS
    padFX2Mode: 0x6B,   // PAD FX (SHIFT + STEMS)
    beatJumpMode: 0x20,
    beatLoopMode: 0x6D,
    samplerMode: 0x22,
    keyShiftMode: 0x6F,
};

// Store timer IDs
PioneerDDJGRV6.timers = {};

// Jog wheel constants
// openDJ: whether touching the top scratches follows each deck's VINYL
// switch ([ChannelN] vinyl_mode, on by default) instead of this constant.
PioneerDDJGRV6.vinylMode = function(deckNum) {
    return engine.getValue("[Channel" + deckNum + "]", "vinyl_mode") > 0;
};
PioneerDDJGRV6.alpha = 1.0/8;
// openDJ: ticks a platter turn sends. 720, as the FLX4's and FLX6's and as
// the community GRV6 mapping for Mixxx scratches with; if a turn moves the
// track far more or less than a record would, this is the number to change.
PioneerDDJGRV6.jogTicksPerTurn = 720;
PioneerDDJGRV6.beta = PioneerDDJGRV6.alpha/32;

// Multiplier for fast seek through track using SHIFT+JOGWHEEL
// openDJ: half of Mixxx's 150, which searched too fast.
PioneerDDJGRV6.fastSeekScale = 75;

PioneerDDJGRV6.tempoRanges = [0.06, 0.10, 0.16, 0.25];

PioneerDDJGRV6.shiftButtonDown = [false, false, false, false];

// Jog wheel loop adjust
PioneerDDJGRV6.loopAdjustIn = [false, false, false, false];
PioneerDDJGRV6.loopAdjustOut = [false, false, false, false];
PioneerDDJGRV6.loopAdjustMultiply = 50;

// Beatjump pad (beatjump_size values)
// openDJ: 4, 8, 16 and 32 beats back and forward, as on the deck's screen.
PioneerDDJGRV6.beatjumpSizeForPad = {
    0x20: -4,  // PAD 1
    0x21: 4,   // PAD 2
    0x22: -8,  // PAD 3
    0x23: 8,   // PAD 4
    0x24: -16, // PAD 5
    0x25: 16,  // PAD 6
    0x26: -32, // PAD 7
    0x27: 32   // PAD 8
};

// Stems (KEYBOARD) pads mode status for each deck, without or with SHIFT pressed
PioneerDDJGRV6.stemsPadsModesStatus = {
    "[Channel1]": [0x97, 0x98],
    "[Channel2]": [0x99, 0x9a],
    "[Channel3]": [0x9b, 0x9c],
    "[Channel4]": [0x9d, 0x9e],
};

// Stems (KEYBOARD) pad 1 control (pad control = [this value] + [pad  number] - 1)
PioneerDDJGRV6.stemMutePadsFirstControl = 0x40;

// Pitch shift (KEY SHIFT) pads mode status for each deck, without or with SHIFT pressed
PioneerDDJGRV6.pitchPadsModesStatus = PioneerDDJGRV6.stemsPadsModesStatus;

// Pitch shift (KEY SHIFT) pad 1 control (pad control = [this value] + [pad  number] - 1)
PioneerDDJGRV6.pitchPadsFirstControl = 0x70;

PioneerDDJGRV6.quickJumpSize = 32;

// Used for tempo slider
PioneerDDJGRV6.highResMSB = {
    "[Channel1]": {},
    "[Channel2]": {},
    "[Channel3]": {},
    "[Channel4]": {},
};

PioneerDDJGRV6.toggleLight = function(midiIn, active) {
    midi.sendShortMsg(midiIn.status, midiIn.data1, active ? 0x7F : 0);
};

//
// Init
//

PioneerDDJGRV6.init = function() {
    const decks = PioneerDDJGRV6.decks;
    engine.makeConnection(PioneerDDJGRV6.beatFxSlot, "beats", PioneerDDJGRV6.beatFxBeatsLight);
    engine.trigger(PioneerDDJGRV6.beatFxSlot, "beats");
    decks.forEach(function(group) {
        engine.makeConnection(group, "vu_meter", PioneerDDJGRV6.vuMeterUpdate);
        midi.sendShortMsg(0xB0 + PioneerDDJGRV6.deckIndex(group), 0x02, 0);
        engine.softTakeover(group, "rate", true);
    });
    engine.softTakeover(PioneerDDJGRV6.beatFxSlot, "meta", true);

    const samplerCount = 16;
    if (engine.getValue("[App]", "num_samplers") < samplerCount) {
        engine.setValue("[App]", "num_samplers", samplerCount);
    }
    for (let i = 1; i <= samplerCount; ++i) {
        engine.makeConnection("[Sampler" + i + "]", "play", PioneerDDJGRV6.samplerPlayOutputCallbackFunction);
        engine.makeConnection("[Sampler" + i + "]", "track_loaded", PioneerDDJGRV6.samplerPlayOutputCallbackFunction);
    }

    decks.forEach(function(group) {
        engine.makeConnection(group, "sampler_page", PioneerDDJGRV6.samplerPlayOutputCallbackFunction);

        PioneerDDJGRV6.setLoopButtonLights(PioneerDDJGRV6.deckStatus(group), 0x7F);

        // openDJ: the controller starts in HOT CUE mode; light its button.
        midi.sendShortMsg(PioneerDDJGRV6.deckStatus(group), PioneerDDJGRV6.padModeLights.hotcueMode, 0x7F);

        // openDJ: the pads follow the pages in the deck's pad tabs.
        for (let tab = 1; tab <= 8; ++tab) {
            engine.makeConnection(group, "pad_slot_" + tab, PioneerDDJGRV6.padSlotChanged);
        }

        engine.makeConnection(group, "loop_enabled", PioneerDDJGRV6.loopToggle);

        // Register callbacks for each deck, when a file is loaded and the number of stems is available
        engine.makeConnection(group, "stem_count", PioneerDDJGRV6.stemCountChanged);

        // Register callbacks for each stems of each decks, to change pad lights when muted/unmuted
        for (let stem = 1; stem <= 5; stem++) {
            engine.makeConnection(PioneerDDJGRV6.stemGroup(group, stem), "mute", PioneerDDJGRV6.stemMuteChanged);
        }

        // Register callbacks for each deck, when a file is loaded to reset pitch shift
        engine.makeConnection(group, "track_loaded", PioneerDDJGRV6.pitchAdjusted);

        // Register callbacks for each deck, when the pitch shift is modified
        engine.makeConnection(group, "pitch_adjust", PioneerDDJGRV6.pitchAdjusted);

        engine.makeConnection(group, "sync_leader", PioneerDDJGRV6.syncLeaderLight);

        // openDJ: the GRV6's own lights.
        engine.makeConnection(group, "track_loaded", PioneerDDJGRV6.loadLight);
        PioneerDDJGRV6.loadLight(engine.getValue(group, "track_loaded"), group);
        engine.makeConnection(group, "keylock", PioneerDDJGRV6.deckLight(0x1A));
        engine.makeConnection(group, "loop_enabled", PioneerDDJGRV6.loopActiveLight);
        PioneerDDJGRV6.loopActiveLight(engine.getValue(group, "loop_enabled"), group);
        engine.makeConnection(group, "slip_enabled", PioneerDDJGRV6.deckLight(0x40));
        engine.makeConnection(group, "quantize", PioneerDDJGRV6.deckLight(0x35));
        engine.makeConnection("[QuickEffectRack1_" + group + "]", "enabled", PioneerDDJGRV6.colorFxLight);
        engine.makeConnection(group, "stem_count", PioneerDDJGRV6.drumSwapLights);
        for (let stem = 1; stem <= 5; stem++) {
            engine.makeConnection(PioneerDDJGRV6.stemGroup(group, stem), "mute", PioneerDDJGRV6.drumSwapLights);
        }
        ["keylock", "slip_enabled", "quantize"].forEach(function(key) {
            engine.trigger(group, key);
        });
        PioneerDDJGRV6.drumSwapLights(0, group);

        // openDJ: the STEMS button opens the stems pads and SHIFT + STEMS the
        // pad FX, as printed on the GRV6, while those tabs still hold the
        // FLX4 order openDJ starts with (PAD FX 1 and PAD FX 2).
        if (engine.getValue(group, "pad_slot_2") === 1) {
            engine.setValue(group, "pad_slot_2", 4);
        }
        if (engine.getValue(group, "pad_slot_6") === 6) {
            engine.setValue(group, "pad_slot_6", 1);
        }
        PioneerDDJGRV6.routePads(group);
    });
    PioneerDDJGRV6.samplerPlayOutputCallbackFunction();
    PioneerDDJGRV6.colorFxLight();

    // The jog rings show where each deck's playhead is.
    PioneerDDJGRV6.jogRings = [-1, -1, -1, -1];
    PioneerDDJGRV6.jogRingTimer = engine.beginTimer(40, PioneerDDJGRV6.jogRingUpdate);
};

//
// Waveform zoom
//

PioneerDDJGRV6.waveformZoom = function(midichan, control, value, status, group) {
    if (value === 0x7f) {
        script.triggerControl(group, "waveform_zoom_up", 100);
    } else {
        script.triggerControl(group, "waveform_zoom_down", 100);
    }
};

//
// Channel level lights
//

PioneerDDJGRV6.vuMeterUpdate = function(value, group) {
    midi.sendShortMsg(0xB0 + PioneerDDJGRV6.deckIndex(group), 0x02, value * 127);
};

//
// Jog rings
//

// The ring's 73 steps (0x00 to 0x48, CC 0x00 to 0x03 on channel 16) go round once per turn of a record
// playing at 33 1/3 rpm.
PioneerDDJGRV6.jogRingUpdate = function() {
    PioneerDDJGRV6.decks.forEach(function(group, i) {
        let step = 0;
        if (engine.getValue(group, "track_loaded")) {
            const seconds = engine.getValue(group, "playposition") * engine.getValue(group, "duration");
            const turns = Math.max(0, seconds) * (100 / 3) / 60;
            step = Math.floor((turns - Math.floor(turns)) * 0x49);
        }
        if (step !== PioneerDDJGRV6.jogRings[i]) {
            PioneerDDJGRV6.jogRings[i] = step;
            midi.sendShortMsg(0xBF, i, step);
        }
    });
};

//
// Sync leader (MASTER)
//

// MASTER makes the deck the one the others sync to, and again lets the
// app choose.
PioneerDDJGRV6.masterPressed = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "sync_leader", engine.getValue(group, "sync_leader") === 2 ? 0 : 2);
    }
};

PioneerDDJGRV6.syncLeaderLight = function(value, group) {
    midi.sendShortMsg(PioneerDDJGRV6.deckStatus(group), 0x5C, value > 0 ? 0x7F : 0x00);
};

//
// Effects
//

// openDJ: the BEAT FX section drives openDJ's Beat FX unit, one effect
// sent to deck 1, deck 2 or both, as in rekordbox:
//   FX SELECT steps through the effects (SHIFT: backwards),
//   BEAT < > halve and double its length (SHIFT: AUTO tempo, TAP tempo),
//   LEVEL/DEPTH is the effect's depth (its meta knob),
//   ON/OFF turns it on and off (SHIFT: release, letting the tail ring out).
PioneerDDJGRV6.beatFxUnit = "[EffectRack1_EffectUnit5]";
PioneerDDJGRV6.beatFxSlot = "[EffectRack1_EffectUnit5_Effect1]";

PioneerDDJGRV6.shifted = function() {
    return PioneerDDJGRV6.shiftButtonDown.some(function(down) { return down; });
};

// The BEAT FX display's beat lights (channel 5, CC 0x64) show the effect's
// length: 1/4, 1/2, 3/4, 1, 2, 4, 8, 16 or 32 beats.
PioneerDDJGRV6.beatFxBeatCodes = [
    [0.25, 0x03], [0.5, 0x04], [0.75, 0x21], [1, 0x05], [2, 0x06],
    [4, 0x07], [8, 0x08], [16, 0x09], [32, 0x0A],
];

PioneerDDJGRV6.beatFxBeatsLight = function(value) {
    let code = 0x03;
    let best = Infinity;
    PioneerDDJGRV6.beatFxBeatCodes.forEach(function(pair) {
        const off = Math.abs(Math.log2(Math.max(value, 0.0625) / pair[0]));
        if (off < best) {
            best = off;
            code = pair[1];
        }
    });
    midi.sendShortMsg(0xB4, 0x64, code);
};

PioneerDDJGRV6.beatFxLevelDepthRotate = function(_channel, _control, value) {
    engine.setParameter(PioneerDDJGRV6.beatFxSlot, "meta", value / 0x7F);
};

// BEAT FX SELECT sends the position it lands on (notes 0x20 to 0x2D), here
// to the nearest engine effect (loaded_effect: 1 Echo, 2 Reverb, 3 Filter,
// 4 Flanger, 6 Phaser, 7 Gate, 8 Roll, 12 Spiral, 14 Chorus, 18 Filter
// Echo, 19 Tape Echo).
PioneerDDJGRV6.beatFxEffects = {
    0x20: 19, // DELAY
    0x21: 1, // ECHO
    0x22: 18, // LOW CUT ECHO
    0x23: 12, // SPIRAL
    0x24: 12, // HELIX
    0x25: 2, // REVERB
    0x26: 4, // FLANGER
    0x27: 6, // PHASER
    0x28: 3, // FILTER
    0x29: 7, // TRANS
    0x2A: 14, // PITCH
    0x2B: 8, // ROLL
    0x2C: 6, // MOBIUS SAW
    0x2D: 6, // MOBIUS TRI
};

PioneerDDJGRV6.beatFxSelect = function(_channel, control, value) {
    if (value === 0) { return; }
    const effect = PioneerDDJGRV6.beatFxEffects[control];
    if (effect !== undefined) {
        engine.setValue(PioneerDDJGRV6.beatFxSlot, "loaded_effect", effect);
    }
};

// Halves (-1) or doubles (+1) the effect's length, within the effect's
// own limits; effects without a length stay as they are.
PioneerDDJGRV6.changeBeatFxBeats = function(direction) {
    const key = direction > 0 ? "beats_double" : "beats_halve";
    engine.setValue(PioneerDDJGRV6.beatFxSlot, key, 1);
    engine.setValue(PioneerDDJGRV6.beatFxSlot, key, 0);
};

PioneerDDJGRV6.pushBeatFxUnit = function(key) {
    engine.setValue(PioneerDDJGRV6.beatFxUnit, key, 1);
    engine.setValue(PioneerDDJGRV6.beatFxUnit, key, 0);
};

PioneerDDJGRV6.beatFxLeftPressed = function(_channel, _control, value) {
    if (value === 0) { return; }

    if (PioneerDDJGRV6.shifted()) {
        PioneerDDJGRV6.pushBeatFxUnit("bpm_auto");
    } else {
        PioneerDDJGRV6.changeBeatFxBeats(-1);
    }
};

PioneerDDJGRV6.beatFxRightPressed = function(_channel, _control, value) {
    if (value === 0) { return; }

    if (PioneerDDJGRV6.shifted()) {
        PioneerDDJGRV6.pushBeatFxUnit("bpm_tap");
    } else {
        PioneerDDJGRV6.changeBeatFxBeats(1);
    }
};

PioneerDDJGRV6.beatFxOnOffPressed = function(_channel, _control, value) {
    if (value === 0) { return; }

    const toggleEnabled = !engine.getValue(PioneerDDJGRV6.beatFxSlot, "enabled");
    engine.setValue(PioneerDDJGRV6.beatFxSlot, "enabled", toggleEnabled);
};

PioneerDDJGRV6.beatFxOnOffShiftPressed = function(_channel, _control, value) {
    if (value === 0) { return; }

    PioneerDDJGRV6.pushBeatFxUnit("release");
};

// CH SELECT sends the position it lands on: CH1 to CH4 (notes 0x10 to
// 0x13; with SHIFT 0x18 to 0x1B), the Beat FX unit's route 1 to 4, MST
// (0x14, SHIFT 0x1C), the main mix (route 6), or SP (0x16, SHIFT 0x1E), the
// sampler, which openDJ can't send it to yet (route 0, nowhere).
PioneerDDJGRV6.beatFxRoutes = {
    0x10: 1, 0x11: 2, 0x12: 3, 0x13: 4, 0x14: 6, 0x16: 0,
    0x18: 1, 0x19: 2, 0x1A: 3, 0x1B: 4, 0x1C: 6, 0x1E: 0,
};

PioneerDDJGRV6.beatFxChannelSelect = function(_channel, control, value) {
    if (value === 0) { return; }

    const route = PioneerDDJGRV6.beatFxRoutes[control];
    if (route !== undefined) {
        engine.setValue(PioneerDDJGRV6.beatFxUnit, "route", route);
    }
};

// DECK 1 to 4: the GRV6 says which deck a side now plays with note 0x3C
// on that deck's channel ("control on").
PioneerDDJGRV6.deckSelected = function(channel, _control, value) {
    if (value === 0x7F) {
        PioneerDDJGRV6.noteSide(channel);
    }
};

//
// Loop IN/OUT ADJUST
//

PioneerDDJGRV6.toggleLoopAdjustIn = function(channel, _control, value, _status, group) {
    if (value === 0 || engine.getValue(group, "loop_enabled" === 0)) {
        return;
    }
    PioneerDDJGRV6.loopAdjustIn[channel] = !PioneerDDJGRV6.loopAdjustIn[channel];
    PioneerDDJGRV6.loopAdjustOut[channel] = false;
};

PioneerDDJGRV6.toggleLoopAdjustOut = function(channel, _control, value, _status, group) {
    if (value === 0 || engine.getValue(group, "loop_enabled" === 0)) {
        return;
    }
    PioneerDDJGRV6.loopAdjustOut[channel] = !PioneerDDJGRV6.loopAdjustOut[channel];
    PioneerDDJGRV6.loopAdjustIn[channel] = false;
};

// Two signals are sent here so that the light stays lit/unlit in its shift state too
PioneerDDJGRV6.setReloopLight = function(status, value) {
    midi.sendShortMsg(status, 0x4D, value);
    midi.sendShortMsg(status, 0x50, value);
};


PioneerDDJGRV6.setLoopButtonLights = function(status, value) {
    [0x10, 0x11, 0x77, 0x4C].forEach(function(control) {
        midi.sendShortMsg(status, control, value);
    });
};

PioneerDDJGRV6.startLoopLightsBlink = function(channel, control, status, group) {
    let blink = 0x7F;

    PioneerDDJGRV6.stopLoopLightsBlink(group, control, status);

    PioneerDDJGRV6.timers[group][control] = engine.beginTimer(500, () => {
        blink = 0x7F - blink;

        // When adjusting the loop out position, turn the loop in light off
        if (PioneerDDJGRV6.loopAdjustOut[channel]) {
            midi.sendShortMsg(status, 0x10, 0x00);
            midi.sendShortMsg(status, 0x4C, 0x00);
        } else {
            midi.sendShortMsg(status, 0x10, blink);
            midi.sendShortMsg(status, 0x4C, blink);
        }

        // When adjusting the loop in position, turn the loop out light off
        if (PioneerDDJGRV6.loopAdjustIn[channel]) {
            midi.sendShortMsg(status, 0x11, 0x00);
            midi.sendShortMsg(status, 0x77, 0x00);
        } else {
            midi.sendShortMsg(status, 0x11, blink);
            midi.sendShortMsg(status, 0x77, blink);
        }
    });

};

PioneerDDJGRV6.stopLoopLightsBlink = function(group, control, status) {
    PioneerDDJGRV6.timers[group] = PioneerDDJGRV6.timers[group] || {};

    if (PioneerDDJGRV6.timers[group][control] !== undefined) {
        engine.stopTimer(PioneerDDJGRV6.timers[group][control]);
    }
    PioneerDDJGRV6.timers[group][control] = undefined;
    PioneerDDJGRV6.setLoopButtonLights(status, 0x7F);
};

PioneerDDJGRV6.loopToggle = function(value, group, control) {
    const status = PioneerDDJGRV6.deckStatus(group),
        channel = PioneerDDJGRV6.deckIndex(group);

    PioneerDDJGRV6.setReloopLight(status, value ? 0x7F : 0x00);

    if (value) {
        PioneerDDJGRV6.startLoopLightsBlink(channel, control, status, group);
    } else {
        PioneerDDJGRV6.stopLoopLightsBlink(group, control, status);
        PioneerDDJGRV6.loopAdjustIn[channel] = false;
        PioneerDDJGRV6.loopAdjustOut[channel] = false;
    }
};

//
// CUE/LOOP CALL
//

PioneerDDJGRV6.cueLoopCallLeft = function(_channel, _control, value, _status, group) {
    if (engine.getValue(group, "loop_enabled")) {
        PioneerDDJGRV6.loopHalve(_channel, _control, value, _status, group);
    } else if (value) {
        PioneerDDJGRV6.callHotcue(group, -1);
    }
};

PioneerDDJGRV6.cueLoopCallRight = function(_channel, _control, value, _status, group) {
    if (engine.getValue(group, "loop_enabled")) {
        PioneerDDJGRV6.loopDouble(_channel, _control, value, _status, group);
    } else if (value) {
        PioneerDDJGRV6.callHotcue(group, 1);
    }
};

//
// BEAT SYNC
//
// Note that the controller sends different signals for a short press and a long
// press of the same button.
//

// openDJ: a press turns sync on, as the SYNC button on screen does, and
// another turns it off.
PioneerDDJGRV6.syncPressed = function(channel, control, value, status, group) {
    if (value > 0) {
        script.toggleControl(group, "sync_enabled");
    }
};

PioneerDDJGRV6.syncLongPressed = function(channel, control, value, status, group) {
    if (value) {
        engine.setValue(group, "sync_enabled", 1);
    }
};

PioneerDDJGRV6.cycleTempoRange = function(_channel, _control, value, _status, group) {
    if (value === 0) { return; } // ignore release

    const currRange = engine.getValue(group, "rateRange");
    let idx = 0;

    for (let i = 0; i < this.tempoRanges.length; i++) {
        if (currRange === this.tempoRanges[i]) {
            // idx get the index of the value in tempoRanges following the currently configured one
            // or cycle back to 0 if the current is the last value of the list.
            idx = (i + 1) % this.tempoRanges.length;
            break;
        }
    }
    engine.setValue(group, "rateRange", this.tempoRanges[idx]);
};

//
// Jog wheels
//

PioneerDDJGRV6.jogTurn = function(channel, control, value, _status, group) {
    const deckNum = channel + 1;
    // wheel center at 64; <64 rew >64 fwd
    let newVal = value - 64;

    // loop_in / out adjust
    const loopEnabled = engine.getValue(group, "loop_enabled");
    if (loopEnabled > 0) {
        if (PioneerDDJGRV6.loopAdjustIn[channel]) {
            newVal = newVal * PioneerDDJGRV6.loopAdjustMultiply + engine.getValue(group, "loop_start_position");
            engine.setValue(group, "loop_start_position", newVal);
            return;
        }
        if (PioneerDDJGRV6.loopAdjustOut[channel]) {
            newVal = newVal * PioneerDDJGRV6.loopAdjustMultiply + engine.getValue(group, "loop_end_position");
            engine.setValue(group, "loop_end_position", newVal);
            return;
        }
    }

    // openDJ: the rim (0x21) always bends, even while the deck is still
    // easing back from a scratch, so twisting it never scratches.
    if (control !== 0x21 && engine.isScratching(deckNum)) {
        engine.scratchTick(deckNum, newVal);
    } else {
        // openDJ: the rim (and the platter when not touched) bends the
        // speed while it turns, faster clockwise and slower
        // counterclockwise, as in rekordbox. jog_bend takes rim ticks.
        engine.setValue(group, "jog_bend", newVal);
    }
};


PioneerDDJGRV6.jogSearch = function(_channel, _control, value, _status, group) {
    const newVal = (value - 64) * PioneerDDJGRV6.fastSeekScale;
    // openDJ: SHIFT + turn moves the position (jog_search); plain jog bends
    // the speed as in Mixxx.
    engine.setValue(group, "jog_search", newVal);
};

PioneerDDJGRV6.jogTouch = function(channel, _control, value) {
    const deckNum = channel + 1;
    PioneerDDJGRV6.noteSide(channel);

    // skip while adjusting the loop points
    if (PioneerDDJGRV6.loopAdjustIn[channel] || PioneerDDJGRV6.loopAdjustOut[channel]) {
        return;
    }

    if (value !== 0 && PioneerDDJGRV6.vinylMode(deckNum)) {
        engine.scratchEnable(deckNum, PioneerDDJGRV6.jogTicksPerTurn, 33+1/3, PioneerDDJGRV6.alpha, PioneerDDJGRV6.beta);
    } else {
        engine.scratchDisable(deckNum);
    }
};

//
// Shift button
//

PioneerDDJGRV6.shiftPressed = function(channel, _control, value, _status, _group) {
    PioneerDDJGRV6.shiftButtonDown[channel] = value === 0x7F;
};


//
// Tempo sliders
//
// The tempo option in Mixxx's deck preferences determine whether down/up
// increases/decreases the rate. Therefore it must be inverted here so that the
// UI and the control sliders always move in the same direction.
//

PioneerDDJGRV6.tempoSliderMSB = function(channel, control, value, status, group) {
    PioneerDDJGRV6.highResMSB[group].tempoSlider = value;
};

PioneerDDJGRV6.tempoSliderLSB = function(channel, control, value, status, group) {
    const fullValue = (PioneerDDJGRV6.highResMSB[group].tempoSlider << 7) + value;

    engine.setValue(
        group,
        "rate",
        1 - (fullValue / 0x2000)
    );
};

//
// Beat Jump mode
//
// Note that when we increase/decrease the sizes on the pad buttons, we use the
// value of the first pad (0x21) as an upper/lower limit beyond which we don't
// allow further increasing/decreasing of all the values.
//

PioneerDDJGRV6.beatjumpPadPressed = function(_channel, control, value, _status, group) {
    if (value === 0) {
        return;
    }
    engine.setValue(group, "beatjump_size", Math.abs(PioneerDDJGRV6.beatjumpSizeForPad[control]));
    engine.setValue(group, "beatjump", PioneerDDJGRV6.beatjumpSizeForPad[control]);
};

PioneerDDJGRV6.increaseBeatjumpSizes = function(_channel, control, value, _status, group) {
    if (value === 0 || PioneerDDJGRV6.beatjumpSizeForPad[0x21] * 16 > 16) {
        return;
    }
    Object.keys(PioneerDDJGRV6.beatjumpSizeForPad).forEach(function(pad) {
        PioneerDDJGRV6.beatjumpSizeForPad[pad] = PioneerDDJGRV6.beatjumpSizeForPad[pad] * 16;
    });
    engine.setValue(group, "beatjump_size", PioneerDDJGRV6.beatjumpSizeForPad[0x21]);
};

PioneerDDJGRV6.decreaseBeatjumpSizes = function(_channel, control, value, _status, group) {
    if (value === 0 || PioneerDDJGRV6.beatjumpSizeForPad[0x21] / 16 < 1/16) {
        return;
    }
    Object.keys(PioneerDDJGRV6.beatjumpSizeForPad).forEach(function(pad) {
        PioneerDDJGRV6.beatjumpSizeForPad[pad] = PioneerDDJGRV6.beatjumpSizeForPad[pad] / 16;
    });
    engine.setValue(group, "beatjump_size", PioneerDDJGRV6.beatjumpSizeForPad[0x21]);
};

//
// Sampler mode
//

// openDJ: each deck's sampler pads play the 8 slots of its sampler page
// (`sampler_page`: 0 slots 1-8, 1 slots 9-16), the same 8 the screen shows.
// The mapping names the rekordbox layout (left pads 1-4 and 9-12, right
// pads 5-8 and 13-16), which only says which pad was pressed; the status
// says which deck.
PioneerDDJGRV6.samplerSlot = function(group, status) {
    const n = parseInt(group.match(script.samplerRegEx)[1]) - 1;
    const pad = n % 4 + (n >= 8 ? 4 : 0);
    const deck = PioneerDDJGRV6.decks[Math.floor(((status & 0x0F) - 7) / 2)];
    const page = engine.getValue(deck, "sampler_page");
    return "[Sampler" + (page * 8 + pad + 1) + "]";
};

// Lights a deck's sampler pads: on for a slot with a sample, blinking while
// it plays.
PioneerDDJGRV6.samplerLights = function(deck) {
    const channel = PioneerDDJGRV6.padStatus(deck);
    const page = engine.getValue(deck, "sampler_page");
    for (let pad = 0; pad < 8; ++pad) {
        const group = "[Sampler" + (page * 8 + pad + 1) + "]";
        PioneerDDJGRV6.stopSamplerBlink(channel, 0x30 + pad);
        if (engine.getValue(group, "play") === 1) {
            PioneerDDJGRV6.startSamplerBlink(channel, 0x30 + pad, group);
        } else {
            const on = engine.getValue(group, "track_loaded") ? 0x7F : 0x00;
            midi.sendShortMsg(channel, 0x30 + pad, on);
            midi.sendShortMsg(channel + 1, 0x30 + pad, on);
        }
    }
};

PioneerDDJGRV6.samplerPlayOutputCallbackFunction = function(_value, _group, _control) {
    PioneerDDJGRV6.decks.forEach(PioneerDDJGRV6.samplerLights);
};

// openDJ: the pad tab each pad mode button opens on the deck's screen (see
// the deck's `pad_tab` control): tabs 1-4 for the four buttons, 5-8 for the
// same buttons with SHIFT, whatever page is in the tab.
PioneerDDJGRV6.padTabForMode = {
    0x1B: 0, // HOT CUE
    0x1E: 1, // PAD FX1
    0x20: 2, // BEAT JUMP
    0x22: 3, // SAMPLER
    0x69: 4, // KEYBOARD (SHIFT + HOT CUE)
    0x6B: 5, // PAD FX2 (SHIFT + PAD FX1)
    0x6D: 6, // BEAT LOOP (SHIFT + BEAT JUMP)
    0x6F: 7, // KEY SHIFT (SHIFT + SAMPLER)
};

// openDJ: the first pad note of each pad mode, in tab order.
PioneerDDJGRV6.padModeNotes = [0x00, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];

// openDJ: the pad notes that play each pad panel (numbers as for the deck's
// `pad_panel`): its own pad mode's, or for loop rolls notes of their own
// (see the LOOP ROLL controls). Stems share KEYBOARD's notes (keyboardStems).
PioneerDDJGRV6.panelNotes = [0x00, 0x10, 0x20, 0x30, 0x40, 0x60, 0x50, 0x40, 0x70, 0x78];

// openDJ: the pads follow the screen. Each pad mode's pads play the panel in
// its tab (the deck's `pad_slot_N`), with that panel's lights, whatever it
// is: put STEMS in tab 4 and the SAMPLER button gives stems pads.
PioneerDDJGRV6.routePads = function(group) {
    const first = PioneerDDJGRV6.padStatus(group) & 0x0F;
    const channels = [first, first + 1];
    for (let tab = 0; tab < 8; ++tab) {
        const panel = engine.getValue(group, "pad_slot_" + (tab + 1));
        const to = PioneerDDJGRV6.panelNotes[panel];
        for (let pad = 0; pad < 8; ++pad) {
            const from = PioneerDDJGRV6.padModeNotes[tab] + pad;
            channels.forEach(function(channel) {
                midi.routeNote(channel, from, to === undefined ? from : to + pad);
            });
        }
    }
    // KEYBOARD's notes play stems while the open tab shows them.
    const open = PioneerDDJGRV6.padTabForMode[PioneerDDJGRV6.padMode[group]];
    const stems = engine.getValue(group, "pad_slot_" + (open + 1)) === 4;
    if (stems !== PioneerDDJGRV6.keyboardStems[group]) {
        PioneerDDJGRV6.keyboardStems[group] = stems;
        PioneerDDJGRV6.keyboardLights(group);
    }
};

PioneerDDJGRV6.padSlotChanged = function(_value, group, _control) {
    PioneerDDJGRV6.routePads(group);
};

// openDJ: the pad mode button last pressed on each deck.
PioneerDDJGRV6.padMode = {"[Channel1]": 0x1B, "[Channel2]": 0x1B, "[Channel3]": 0x1B, "[Channel4]": 0x1B};

// openDJ: KEYBOARD plays the last hot cue pressed at eight pitches, as in
// rekordbox. Pressing KEYBOARD again turns its pads into the stems pads
// (see stemPads), and back.
PioneerDDJGRV6.keyboardStems = {"[Channel1]": false, "[Channel2]": false, "[Channel3]": false, "[Channel4]": false};

PioneerDDJGRV6.padModeKeyPressed = function(channel, control, value, _status, _group) {
    const group = PioneerDDJGRV6.decks[channel];
    const status = 0x90 + channel;

    if (value && control in PioneerDDJGRV6.padTabForMode) {
        const again = PioneerDDJGRV6.padMode[group] === control;
        const tab = PioneerDDJGRV6.padTabForMode[control];
        PioneerDDJGRV6.padMode[group] = control;
        engine.setValue(group, "pad_tab", tab);
        // KEYBOARD again swaps its tab between keyboard and stems.
        const slot = "pad_slot_" + (tab + 1);
        const panel = engine.getValue(group, slot);
        if (control === 0x69 && again && (panel === 7 || panel === 4)) {
            engine.setValue(group, slot, panel === 7 ? 4 : 7);
        }
        PioneerDDJGRV6.routePads(group);
    }
    // openDJ: only the chosen mode stays lit.
    if (value) {
        Object.keys(PioneerDDJGRV6.padModeLights).forEach(function(mode) {
            midi.sendShortMsg(status, PioneerDDJGRV6.padModeLights[mode], 0x00);
        });
    }
    const light = {
        0x1B: "hotcueMode", 0x69: "keyboardMode", 0x1E: "padFX1Mode", 0x6B: "padFX2Mode",
        0x20: "beatJumpMode", 0x6D: "beatLoopMode", 0x22: "samplerMode", 0x6F: "keyShiftMode",
    }[control];
    if (light) {
        midi.sendShortMsg(status, PioneerDDJGRV6.padModeLights[light], 0x7F);
    }
};

//
// openDJ: KEYBOARD mode
//

// The engine's KEYBOARD_SEMITONES, pad by pad (keyboard_N_activate).
PioneerDDJGRV6.keyboardSemitones = [0, 1, 2, 3, -4, -3, -2, -1];

PioneerDDJGRV6.keyboardPadPressed = function(channel, control, value, status, group) {
    if (PioneerDDJGRV6.keyboardStems[group]) {
        PioneerDDJGRV6.stemMutePadPressed(channel, control, value, status, group);
        return;
    }
    const pad = control - PioneerDDJGRV6.stemMutePadsFirstControl + 1;
    engine.setValue(group, "keyboard_" + pad + "_activate", value === 0x7F ? 1 : 0);
};

PioneerDDJGRV6.keyboardPadShiftPressed = function(channel, control, value, status, group) {
    if (PioneerDDJGRV6.keyboardStems[group]) {
        PioneerDDJGRV6.stemMutePadShiftPressed(channel, control, value, status, group);
        return;
    }
    // SHIFT + pad plays its pitch too.
    PioneerDDJGRV6.keyboardPadPressed(channel, control, value, status, group);
};

// Lights the KEYBOARD pads: the stems in stems mode, else the pad of the key
// shift playing.
PioneerDDJGRV6.keyboardLights = function(group) {
    if (PioneerDDJGRV6.keyboardStems[group]) {
        PioneerDDJGRV6.stemCountChanged(0, group, "stem_count");
        return;
    }
    const pitch = engine.getValue(group, "pitch_adjust");
    for (let i = 0; i < 8; i++) {
        const lit = Math.abs(PioneerDDJGRV6.keyboardSemitones[i] - pitch) < 0.01;
        PioneerDDJGRV6.stemsPadsModesStatus[group].forEach(function(status) {
            midi.sendShortMsg(status, PioneerDDJGRV6.stemMutePadsFirstControl + i, lit ? 0x7F : 0x00);
        });
    }
};

//
// openDJ: PAD FX1 and PAD FX2
//

// What each pad holds on, as on the deck's screen (deck.slint's pad-fx and
// pad-fx2): a loaded_effect (1 Echo, 2 Reverb, 3 Filter, 4 Flanger,
// 5 Bitcrusher, 6 Phaser, 7 Gate, 8 Roll, 9 Echo Out, 10 Brake, 11 Noise,
// 12 Spiral) and where the meta knob sits.
PioneerDDJGRV6.padFx = {
    0x10: [1, 0.6], 0x11: [2, 0.7], 0x12: [3, 0.2], 0x13: [3, 0.8],
    0x14: [4, 0.7], 0x15: [6, 0.7], 0x16: [5, 0.7], 0x17: [1, 1.0],
    0x50: [8, 0.5], 0x51: [7, 0.5], 0x52: [9, 0.6], 0x53: [10, 0.5],
    0x54: [11, 0.7], 0x55: [12, 0.7], 0x56: [7, 0.9], 0x57: [2, 1.0],
};

// Holding a pad puts its effect on the deck through the deck's pad FX unit
// (units 6 to 9 for decks 1 to 4); letting go turns it off.
PioneerDDJGRV6.padFxPressed = function(_channel, control, value, status, group) {
    const fx = PioneerDDJGRV6.padFx[control];
    if (fx === undefined) {
        return;
    }
    const deck = PioneerDDJGRV6.deckIndex(group) + 1;
    const unit = "[EffectRack1_EffectUnit" + (deck + 5) + "]";
    const slot = "[EffectRack1_EffectUnit" + (deck + 5) + "_Effect1]";
    if (value === 0x7F) {
        engine.setValue(slot, "loaded_effect", fx[0]);
        engine.setValue(slot, "meta", fx[1]);
        engine.setValue(unit, "mix", 1);
        engine.setValue(unit, "enabled", 1);
        engine.setValue(unit, "group_[Channel" + deck + "]_enable", 1);
        engine.setValue(slot, "enabled", 1);
    } else {
        engine.setValue(slot, "enabled", 0);
    }
    midi.sendShortMsg(status, control, value === 0x7F ? 0x7F : 0x00);
};

PioneerDDJGRV6.samplerPadPressed = function(_channel, _control, value, status, mapped) {
    const group = PioneerDDJGRV6.samplerSlot(mapped, status);
    if (engine.getValue(group, "track_loaded")) {
        engine.setValue(group, "cue_gotoandplay", value);
    } else {
        engine.setValue(group, "LoadSelectedTrack", value);
    }
};

PioneerDDJGRV6.samplerPadShiftPressed = function(_channel, _control, value, status, mapped) {
    const group = PioneerDDJGRV6.samplerSlot(mapped, status);
    if (engine.getValue(group, "play")) {
        engine.setValue(group, "cue_gotoandstop", value);
    } else if (engine.getValue(group, "track_loaded")) {
        engine.setValue(group, "eject", value);
    }
};

PioneerDDJGRV6.startSamplerBlink = function(channel, control, group) {
    let val = 0x7f;

    PioneerDDJGRV6.stopSamplerBlink(channel, control);
    PioneerDDJGRV6.timers[channel][control] = engine.beginTimer(250, () => {
        val = 0x7f - val;

        // blink the appropriate pad
        midi.sendShortMsg(channel, control, val);
        // also blink the pad while SHIFT is pressed
        midi.sendShortMsg((channel+1), control, val);

        const isPlaying = engine.getValue(group, "play") === 1;

        if (!isPlaying) {
            // kill timer
            PioneerDDJGRV6.stopSamplerBlink(channel, control);
            // set the pad LED to ON
            midi.sendShortMsg(channel, control, 0x7f);
            // set the pad LED to ON while SHIFT is pressed
            midi.sendShortMsg((channel+1), control, 0x7f);
        }
    });
};

PioneerDDJGRV6.stopSamplerBlink = function(channel, control) {
    PioneerDDJGRV6.timers[channel] = PioneerDDJGRV6.timers[channel] || {};

    if (PioneerDDJGRV6.timers[channel][control] !== undefined) {
        engine.stopTimer(PioneerDDJGRV6.timers[channel][control]);
        PioneerDDJGRV6.timers[channel][control] = undefined;
    }
};


PioneerDDJGRV6.toggleQuantize = function(_channel, _control, value, _status, group) {
    if (value) {
        script.toggleControl(group, "quantize");
    }
};

PioneerDDJGRV6.quickJumpForward = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "beatjump", PioneerDDJGRV6.quickJumpSize);
    }
};

PioneerDDJGRV6.quickJumpBack = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "beatjump", -PioneerDDJGRV6.quickJumpSize);
    }
};

//
// Stems mode
//

// openDJ: the stems pads, laid out as on screen (VirtualDJ's): Vocal,
// Instru, Bass, Acapella on top; Kick, HiHat, Stems FX, Instrumental below.
// A number from 1 is the stem ([ChannelN_StemM]: 1 kick, 2 bass, 3
// instruments, 4 vocals, 5 hi-hats); 0 is Acapella, -1 Instrumental and -2
// Stems FX, which does nothing yet.
PioneerDDJGRV6.stemPads = [4, 3, 2, 0, 1, 5, -2, -1];

PioneerDDJGRV6.stemGroup = function(group, stem) {
    return `[${group.substring(1, group.length-1)}_Stem${stem}]`;
};

// Whether each of the deck's stems plays (index 1 to 5); a stem the track
// doesn't have counts as playing.
PioneerDDJGRV6.stemsOn = function(group) {
    const count = engine.getValue(group, "stem_count");
    const on = [false];
    for (let stem = 1; stem <= 5; stem++) {
        on.push(stem > count || engine.getValue(PioneerDDJGRV6.stemGroup(group, stem), "mute") <= 0.5);
    }
    return on;
};

// Whether the pad for `kind` (see stemPads) is lit.
PioneerDDJGRV6.stemPadLit = function(group, kind) {
    const count = engine.getValue(group, "stem_count");
    if (count < 4) {
        return false;
    }
    const on = PioneerDDJGRV6.stemsOn(group);
    const others = [1, 2, 3, 5].filter(function(s) { return s <= count; });
    if (kind > 0) {
        return kind <= count && on[kind];
    } else if (kind === 0) {
        return on[4] && others.every(function(s) { return !on[s]; });
    } else if (kind === -1) {
        return !on[4] && others.every(function(s) { return on[s]; });
    }
    return false;
};

// Mutes or unmutes the vocals and the rest.
PioneerDDJGRV6.setStems = function(group, vocals, rest) {
    for (let stem = 1; stem <= 5; stem++) {
        const on = stem === 4 ? vocals : rest;
        engine.setValue(PioneerDDJGRV6.stemGroup(group, stem), "mute", on ? 0 : 1);
    }
};

PioneerDDJGRV6.stemMutePadPressed = function(_channel, control, value, _status, group) {
    if (value !== 0x7f) {
        return;
    }
    const kind = PioneerDDJGRV6.stemPads[control - PioneerDDJGRV6.stemMutePadsFirstControl];
    const count = engine.getValue(group, "stem_count");
    if (count < 4 || kind === undefined || kind > count) {
        return;
    }
    if (kind > 0) {
        const stemGroup = PioneerDDJGRV6.stemGroup(group, kind);
        engine.setValue(stemGroup, "mute", engine.getValue(stemGroup, "mute") ? 0 : 1);
    } else if (kind === 0) {
        PioneerDDJGRV6.setStems(group, true, PioneerDDJGRV6.stemPadLit(group, 0));
    } else if (kind === -1) {
        PioneerDDJGRV6.setStems(group, PioneerDDJGRV6.stemPadLit(group, -1), true);
    }
};

// SHIFT + a stem's pad plays that stem alone.
PioneerDDJGRV6.stemMutePadShiftPressed = function(_channel, control, value, _status, group) {
    if (value !== 0x7f) {
        return;
    }
    const kind = PioneerDDJGRV6.stemPads[control - PioneerDDJGRV6.stemMutePadsFirstControl];
    const count = engine.getValue(group, "stem_count");
    if (count < 4 || kind === undefined || kind <= 0 || kind > count) {
        PioneerDDJGRV6.stemMutePadPressed(_channel, control, value, _status, group);
        return;
    }
    for (let stem = 1; stem <= count; stem++) {
        engine.setValue(PioneerDDJGRV6.stemGroup(group, stem), "mute", stem === kind ? 0 : 1);
    }
};

PioneerDDJGRV6.stemCountChanged = function(_value, group, _control) {
    const channel = group.match(/\[Channel\d+/)[0] + "]";
    if (!PioneerDDJGRV6.keyboardStems[channel]) {
        return;
    }
    for (let pad = 0; pad < 8; pad++) {
        const lit = PioneerDDJGRV6.stemPadLit(channel, PioneerDDJGRV6.stemPads[pad]);
        PioneerDDJGRV6.stemsPadsModesStatus[channel].forEach(function(status) {
            midi.sendShortMsg(status, PioneerDDJGRV6.stemMutePadsFirstControl + pad, lit ? 0x7F : 0x00);
        });
    }
};

PioneerDDJGRV6.stemMuteChanged = PioneerDDJGRV6.stemCountChanged;

//
// Pitch Shift mode
//

PioneerDDJGRV6.pitchAdjusted = function(_value, group, _control) {
    // openDJ: the KEYBOARD pads show the key shift too.
    PioneerDDJGRV6.keyboardLights(group);
    const pitchAdjust = Math.round(engine.getValue(group, "pitch_adjust"));
    let lights = 0b00000000;

    if (pitchAdjust === 0) {
        lights = 0b10000001;
    } else if (pitchAdjust === 1) {
        lights = 0b01000000;
    } else if (pitchAdjust === 2) {
        lights = 0b00100000;
    } else if (pitchAdjust === 3) {
        lights = 0b00010000;
    } else if (pitchAdjust === 4) {
        lights = 0b10010000;
    } else if (pitchAdjust === 5) {
        lights = 0b01010000;
    } else if (pitchAdjust === 6) {
        lights = 0b00110000;
    } else if (pitchAdjust === 7) {
        lights = 0b10110000;
    } else if (pitchAdjust === 8) {
        lights = 0b01110000;
    } else if (pitchAdjust > 8) {
        lights = 0b11110000;
    } else if (pitchAdjust === -1) {
        lights = 0b00000010;
    } else if (pitchAdjust === -2) {
        lights = 0b00000100;
    } else if (pitchAdjust === -3) {
        lights = 0b00001000;
    } else if (pitchAdjust === -4) {
        lights = 0b00001001;
    } else if (pitchAdjust === -5) {
        lights = 0b00001010;
    } else if (pitchAdjust === -6) {
        lights = 0b00001100;
    } else if (pitchAdjust === -7) {
        lights = 0b00001101;
    } else if (pitchAdjust === -8) {
        lights = 0b00001110;
    } else if (pitchAdjust < -8) {
        lights = 0b00001111;
    } else {
        lights = 0b11111111;
    }

    for (let i=0; i<8; i++) {
        let code = 0x00;
        const pad = 0b10000000 >>> i;

        if (lights & pad) {
            code = 0x7f;
        } else {
            code = 0x00;
        }

        PioneerDDJGRV6.pitchPadsModesStatus[group].forEach(
            (padMode) => midi.sendShortMsg(
                padMode,
                PioneerDDJGRV6.pitchPadsFirstControl + i,
                code,
            )
        );
    }
};

PioneerDDJGRV6.pitchPadPressed = function(_channel, control, value, _status, group) {
    if (value !== 0x7f) {
        return;
    }

    const pad = control - this.pitchPadsFirstControl;
    let pitch = 0;

    if (pad === 0) {
        pitch = 0;
    } else if (pad === 1) {
        pitch = 1;
    } else if (pad === 2) {
        pitch = 2;
    } else if (pad === 3) {
        pitch = 3;
    } else if (pad === 4) {
        pitch = -3;
    } else if (pad === 5) {
        pitch = -2;
    } else if (pad === 6) {
        pitch = -1;
    } else if (pad === 7) {
        pitch = 0;
    }

    engine.setValue(group, "pitch_adjust", pitch);
};

PioneerDDJGRV6.pitchPadShiftPressed = function(_channel, control, value, _status, group) {
    if (value !== 0x7f) {
        return;
    }

    const pad = control - this.pitchPadsFirstControl;

    let currentPitch = engine.getValue(group, "pitch_adjust");

    if (pad === 0) {
        currentPitch += 1;
    } else if (pad === 1) {
        currentPitch += 2;
    } else if (pad === 2) {
        currentPitch += 3;
    } else if (pad === 3) {
        currentPitch += 4;
    } else if (pad === 4) {
        currentPitch += -4;
    } else if (pad === 5) {
        currentPitch += -3;
    } else if (pad === 6) {
        currentPitch += -2;
    } else if (pad === 7) {
        currentPitch += -1;
    }

    engine.setValue(group, "pitch_adjust", currentPitch);
};


//
// openDJ: the GRV6's own buttons
//

// LOAD 1 to 4 light while their deck has a track (channel 16, notes 0x00
// to 0x03).
PioneerDDJGRV6.loadLight = function(value, group) {
    midi.sendShortMsg(0x9F, PioneerDDJGRV6.deckIndex(group), value ? 0x7F : 0x00);
};

// A deck button lit while a deck control is on.
PioneerDDJGRV6.deckLight = function(note) {
    return function(value, group) {
        midi.sendShortMsg(PioneerDDJGRV6.deckStatus(group), note, value ? 0x7F : 0x00);
    };
};

// MASTER TEMPO: key lock (SHIFT: the next tempo range).
PioneerDDJGRV6.keylock = function(_channel, _control, value, _status, group) {
    if (value) {
        script.toggleControl(group, "keylock");
    }
};

// KEY SYNC matches the key of the other deck; SHIFT goes back to the
// track's own key.
PioneerDDJGRV6.keySync = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "sync_key", 1);
    }
};

PioneerDDJGRV6.keyReset = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "reset_key", 1);
    }
};

// SLIP turns slip on and off; SHIFT + SLIP switches the jog between
// scratching (vinyl) and bending only.
PioneerDDJGRV6.toggleSlip = function(_channel, _control, value, _status, group) {
    if (value) {
        script.toggleControl(group, "slip_enabled");
    }
};

PioneerDDJGRV6.toggleVinyl = function(_channel, _control, value, _status, group) {
    if (value) {
        script.toggleControl(group, "vinyl_mode");
    }
};

// SOUND COLOR FX ON/OFF turns the COLOR knobs' effect on and off on all
// four channels; SHIFT picks the next Color FX (the engine's COLOR_FX: 1
// Filter, 2 Space, 3 Dub Echo, 4 Noise, 5 Crush). Lit while on.
PioneerDDJGRV6.colorFxCount = 5;

PioneerDDJGRV6.colorFxPressed = function(_channel, _control, value) {
    if (!value) { return; }
    const on = engine.getValue("[QuickEffectRack1_[Channel1]]", "enabled") ? 0 : 1;
    PioneerDDJGRV6.decks.forEach(function(group) {
        engine.setValue("[QuickEffectRack1_" + group + "]", "enabled", on);
    });
};

PioneerDDJGRV6.colorFxShiftPressed = function(_channel, _control, value) {
    if (!value) { return; }
    const current = engine.getValue("[QuickEffectRack1_[Channel1]]", "loaded_chain_preset");
    const next = current >= PioneerDDJGRV6.colorFxCount ? 1 : current + 1;
    PioneerDDJGRV6.decks.forEach(function(group) {
        engine.setValue("[QuickEffectRack1_" + group + "]", "loaded_chain_preset", next);
    });
};

PioneerDDJGRV6.colorFxLight = function() {
    const on = engine.getValue("[QuickEffectRack1_[Channel1]]", "enabled");
    midi.sendShortMsg(0x96, 0x00, on ? 0x7F : 0x00);
    midi.sendShortMsg(0x96, 0x08, on ? 0x7F : 0x00);
};

// SHIFT + CUE turns PART ISOLATOR on and off (lit while on): the EQ knobs
// then send their own messages (CC 0x08, 0x0C, 0x10, as rekordbox's
// mapping reads them) and set the levels of the vocals, the instruments
// (bass and instruments) and the drums (kick and hi-hats), centre as
// recorded.
PioneerDDJGRV6.partIsolator = [false, false, false, false];

PioneerDDJGRV6.partIsolatorPressed = function(channel, _control, value, _status, group) {
    if (!value) { return; }
    PioneerDDJGRV6.partIsolator[channel] = !PioneerDDJGRV6.partIsolator[channel];
    midi.sendShortMsg(PioneerDDJGRV6.deckStatus(group), 0x39, PioneerDDJGRV6.partIsolator[channel] ? 0x7F : 0x00);
};

PioneerDDJGRV6.partLevel = function(group, stems, value) {
    stems.forEach(function(stem) {
        if (stem <= engine.getValue(group, "stem_count")) {
            engine.setParameter(PioneerDDJGRV6.stemGroup(group, stem), "volume", value / 0x7F);
        }
    });
};

PioneerDDJGRV6.partIsolatorVocal = function(_channel, _control, value, _status, group) {
    PioneerDDJGRV6.partLevel(group, [4], value);
};

PioneerDDJGRV6.partIsolatorInst = function(_channel, _control, value, _status, group) {
    PioneerDDJGRV6.partLevel(group, [2, 3], value);
};

PioneerDDJGRV6.partIsolatorDrums = function(_channel, _control, value, _status, group) {
    PioneerDDJGRV6.partLevel(group, [1, 5], value);
};

// The GRV6 sends CUE/LOOP CALL as loop halve and double (notes 0x12 and
// 0x13) while it knows a loop plays: tell it (channel 16, notes 0x04 to
// 0x07, as rekordbox does).
PioneerDDJGRV6.loopActiveLight = function(value, group) {
    midi.sendShortMsg(0x9F, 0x04 + PioneerDDJGRV6.deckIndex(group), value ? 0x7F : 0x00);
};

//
// openDJ: GROOVE CIRCUIT
//

// The deck's drum stems: kick (1) and hi-hats (5) when the drums were
// split, else the one drums stem (1). None without stems.
PioneerDDJGRV6.drumStems = function(group) {
    const count = engine.getValue(group, "stem_count");
    if (count < 4) {
        return [];
    }
    return count >= 5 ? [1, 5] : [1];
};

// GAIN: the drums' level, centre as recorded (the stem volume knob runs
// 0 to +6 dB with its centre at full volume).
PioneerDDJGRV6.grooveGain = function(_channel, _control, value, _status, group) {
    PioneerDDJGRV6.drumStems(group).forEach(function(stem) {
        engine.setParameter(PioneerDDJGRV6.stemGroup(group, stem), "volume", value / 0x7F);
    });
};

// DRUM SWAP 1 to 4: which stems each takes out (1 kick, 2 bass,
// 3 instruments, 4 vocals, 5 hi-hats); 0 means the drums, whatever the
// track has. A track with only four stems has one drums stem, so 1 and 2
// both take it out. openDJ has no drum samples to swap in, so a swap takes
// stems out: Bank A (SHIFT + DRUM SWAP 1) the drums, Bank B (SHIFT + DRUM
// SWAP 2) the rest.
PioneerDDJGRV6.drumSwapBanks = [
    {
        0x00: [1],          // the kick
        0x01: [5],          // the hi-hats
        0x02: [0],          // all the drums
        0x03: [2, 3, 4],    // everything but the drums
    },
    {
        0x00: [2],          // the bass
        0x01: [3],          // the instruments
        0x02: [4],          // the vocals
        0x03: [0, 2, 3],    // everything but the vocals
    },
];
PioneerDDJGRV6.drumSwaps = PioneerDDJGRV6.drumSwapBanks[0];

// Each deck's bank (0 A, 1 B), and whether its swaps add up (Multi, SHIFT
// + DRUM SWAP 4) or each one ends the others (Single, SHIFT + DRUM SWAP 3,
// as the GRV6 starts).
PioneerDDJGRV6.drumSwapBank = [0, 0, 0, 0];
PioneerDDJGRV6.drumSwapMulti = [false, false, false, false];

PioneerDDJGRV6.drumSwapStems = function(group, control) {
    const count = engine.getValue(group, "stem_count");
    const drums = PioneerDDJGRV6.drumStems(group);
    const bank = PioneerDDJGRV6.drumSwapBanks[PioneerDDJGRV6.drumSwapBank[PioneerDDJGRV6.deckIndex(group)]];
    let stems = [];
    bank[control].forEach(function(stem) {
        if (stem === 0) {
            stems = stems.concat(drums);
        } else if (stem === 5 && count < 5) {
            stems.push(1);
        } else if (stem <= count) {
            stems.push(stem);
        }
    });
    return stems;
};

// Whether a DRUM SWAP is on: all its stems muted.
PioneerDDJGRV6.drumSwapOn = function(group, control) {
    const stems = PioneerDDJGRV6.drumSwapStems(group, control);
    return stems.length > 0 && stems.every(function(stem) {
        return engine.getValue(PioneerDDJGRV6.stemGroup(group, stem), "mute") > 0.5;
    });
};

PioneerDDJGRV6.drumSwapPressed = function(_channel, control, value, _status, group) {
    if (!value) { return; }
    const stems = PioneerDDJGRV6.drumSwapStems(group, control);
    const mute = PioneerDDJGRV6.drumSwapOn(group, control) ? 0 : 1;
    if (mute && !PioneerDDJGRV6.drumSwapMulti[PioneerDDJGRV6.deckIndex(group)]) {
        // Single: the swap on before ends.
        for (let stem = 1; stem <= engine.getValue(group, "stem_count"); ++stem) {
            engine.setValue(PioneerDDJGRV6.stemGroup(group, stem), "mute", 0);
        }
    }
    stems.forEach(function(stem) {
        engine.setValue(PioneerDDJGRV6.stemGroup(group, stem), "mute", mute);
    });
};

// SHIFT + DRUM SWAP: Bank A, Bank B, Single, Multi.
PioneerDDJGRV6.drumSwapShiftPressed = function(_channel, control, value, _status, group) {
    if (!value) { return; }
    const deck = PioneerDDJGRV6.deckIndex(group);
    if (control <= 0x2D) {
        PioneerDDJGRV6.drumSwapBank[deck] = control - 0x2C;
    } else {
        PioneerDDJGRV6.drumSwapMulti[deck] = control === 0x2F;
    }
    PioneerDDJGRV6.drumSwapLights(0, group);
};

// DRUM SWAP lights its swaps that are on; with SHIFT, the bank and mode.
PioneerDDJGRV6.drumSwapLights = function(_value, group) {
    const channel = group.match(/\[Channel\d+/)[0] + "]";
    const status = PioneerDDJGRV6.deckStatus(channel);
    const deck = PioneerDDJGRV6.deckIndex(channel);
    const multi = PioneerDDJGRV6.drumSwapMulti[deck];
    const shiftLit = [
        PioneerDDJGRV6.drumSwapBank[deck] === 0,
        PioneerDDJGRV6.drumSwapBank[deck] === 1,
        !multi,
        multi,
    ];
    Object.keys(PioneerDDJGRV6.drumSwaps).forEach(function(control) {
        const on = PioneerDDJGRV6.drumSwapOn(channel, Number(control)) ? 0x7F : 0x00;
        midi.sendShortMsg(status, Number(control), on);
        midi.sendShortMsg(status, Number(control) + 0x2C, shiftLit[Number(control)] ? 0x7F : 0x00);
    });
};

// DRUM ROLL 1 to 4, held: the drums roll 1/8, 1/4, 1/2 or 1 beat while the
// rest plays on, through the deck's stem effect (Roll). SHIFT + DRUM ROLL
// (DRUM TRANS) chops the drums in time instead (Trans). Without stems the
// whole deck rolls, and DRUM TRANS does nothing.
PioneerDDJGRV6.drumRollBeats = [0.125, 0.25, 0.5, 1];
// Per deck: the button rolling (its note), and the stem effect to go back to.
PioneerDDJGRV6.drumRollHeld = [0, 0, 0, 0];
PioneerDDJGRV6.drumRollSavedFx = [1, 1, 1, 1];

PioneerDDJGRV6.drumStemFx = function(group, control, value, effect, beats) {
    const deck = PioneerDDJGRV6.deckIndex(group);
    const drums = PioneerDDJGRV6.drumStems(group);
    if (value) {
        if (!PioneerDDJGRV6.drumRollHeld[deck]) {
            PioneerDDJGRV6.drumRollSavedFx[deck] = engine.getValue(group, "stem_fx");
        }
        PioneerDDJGRV6.drumRollHeld[deck] = control;
        engine.setValue(group, "stem_fx", effect);
        engine.setValue(group, "stem_fx_beats", beats);
    } else if (PioneerDDJGRV6.drumRollHeld[deck] !== control) {
        return;
    }
    drums.forEach(function(stem) {
        engine.setValue(PioneerDDJGRV6.stemFxGroup(group, stem), "enabled", value ? 1 : 0);
    });
    midi.sendShortMsg(PioneerDDJGRV6.deckStatus(group), control, value ? 0x7F : 0x00);
    if (!value) {
        PioneerDDJGRV6.drumRollHeld[deck] = 0;
        engine.setValue(group, "stem_fx", PioneerDDJGRV6.drumRollSavedFx[deck]);
    }
};

PioneerDDJGRV6.drumRollPressed = function(_channel, control, value, _status, group) {
    const beats = PioneerDDJGRV6.drumRollBeats[control - 0x04];
    if (!PioneerDDJGRV6.drumStems(group).length) {
        engine.setValue(group, "beatlooproll_" + beats + "_activate", value ? 1 : 0);
        return;
    }
    PioneerDDJGRV6.drumStemFx(group, control, value, 3, beats);
};

PioneerDDJGRV6.drumTransPressed = function(_channel, control, value, _status, group) {
    if (!PioneerDDJGRV6.drumStems(group).length) { return; }
    PioneerDDJGRV6.drumStemFx(group, control, value, 4, PioneerDDJGRV6.drumRollBeats[control - 0x30]);
};

// CAPTURE: openDJ has no drum slots to capture into; it grabs the last
// bars of the mix into the first open Looper deck, as its CAPTURE does.
// SHIFT + CAPTURE (Length) steps how many bars: 1, 2, 4, 8.
PioneerDDJGRV6.capturePressed = function(_channel, _control, value) {
    if (!value) { return; }
    for (let n = 1; n <= 4; ++n) {
        const looper = "[Looper" + n + "]";
        if (engine.getValue(looper, "enabled")) {
            engine.setValue(looper, "capture", 1);
            engine.setValue(looper, "capture", 0);
            return;
        }
    }
};

PioneerDDJGRV6.captureLengthPressed = function(_channel, _control, value) {
    if (!value) { return; }
    const bars = engine.getValue("[Looper1]", "capture_bars");
    const next = bars >= 8 ? 1 : bars * 2;
    for (let n = 1; n <= 4; ++n) {
        engine.setValue("[Looper" + n + "]", "capture_bars", next);
    }
};

// DRUM RELEASE, held (at ON, or latched at LOCK): the drums go into the
// deck's stem effect and fall silent, so their tail rings out; let go and
// they come back dry. SHIFT + DRUM RELEASE (FX SELECT) picks that effect:
// the engine's STEM_FX, 1 Echo or 2 Reverb.
PioneerDDJGRV6.stemFxGroup = function(group, stem) {
    return `[QuickEffectRack1_[${group.substring(1, group.length-1)}_Stem${stem}]]`;
};

PioneerDDJGRV6.drumReleasePressed = function(_channel, _control, value, _status, group) {
    PioneerDDJGRV6.drumStems(group).forEach(function(stem) {
        const stemGroup = PioneerDDJGRV6.stemGroup(group, stem);
        const fxGroup = PioneerDDJGRV6.stemFxGroup(group, stem);
        if (value) {
            engine.setValue(fxGroup, "enabled", 1);
            engine.setValue(stemGroup, "mute", 1);
        } else {
            engine.setValue(stemGroup, "mute", 0);
            engine.setValue(fxGroup, "enabled", 0);
        }
    });
};

PioneerDDJGRV6.drumReleaseShiftPressed = function(_channel, _control, value, _status, group) {
    if (!value) { return; }
    engine.setValue(group, "stem_fx", engine.getValue(group, "stem_fx") >= 2 ? 1 : 2);
};

//
// openDJ: CUE/LOOP CALL
//

// CALL < > jump to the hot cue before or after the playhead (while a loop
// plays: halve and double it); SHIFT (1/2X, 2X) halves and doubles the loop.
PioneerDDJGRV6.callHotcue = function(group, direction) {
    const now = engine.getValue(group, "playposition_seconds");
    let best = 0;
    let bestPos = direction > 0 ? Infinity : -Infinity;
    for (let n = 1; n <= 8; ++n) {
        if (!engine.getValue(group, "hotcue_" + n + "_status")) {
            continue;
        }
        const pos = engine.getValue(group, "hotcue_" + n + "_position");
        // A hot cue within 50 ms of the playhead is the one just called.
        const ahead = direction > 0 ? pos > now + 0.05 && pos < bestPos : pos < now - 0.05 && pos > bestPos;
        if (ahead) {
            best = n;
            bestPos = pos;
        }
    }
    if (best) {
        const key = engine.getValue(group, "play") ? "gotoandplay" : "gotoandstop";
        engine.setValue(group, "hotcue_" + best + "_" + key, 1);
        engine.setValue(group, "hotcue_" + best + "_" + key, 0);
    }
};

PioneerDDJGRV6.loopHalve = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "loop_halve", value ? 1 : 0);
};

PioneerDDJGRV6.loopDouble = function(_channel, _control, value, _status, group) {
    engine.setValue(group, "loop_double", value ? 1 : 0);
};


//
// Shutdown
//

PioneerDDJGRV6.shutdown = function() {
    engine.stopTimer(PioneerDDJGRV6.jogRingTimer);

    PioneerDDJGRV6.decks.forEach(function(group, i) {
        const status = PioneerDDJGRV6.deckStatus(group);
        const pads = PioneerDDJGRV6.padStatus(group);

        // reset vumeter and jog ring
        midi.sendShortMsg(0xB0 + i, 0x02, 0x00);
        midi.sendShortMsg(0xBF, i, 0x00);

        // turn off all pad LEDs
        for (let pad = 0; pad <= 0x7F; ++pad) {
            midi.sendShortMsg(pads, pad, 0x00);
            midi.sendShortMsg(pads + 1, pad, 0x00);
        }

        // turn off loop in and out lights and reloop lights
        PioneerDDJGRV6.setLoopButtonLights(status, 0x00);
        PioneerDDJGRV6.setReloopLight(status, 0x00);
    });
    for (let deck = 0; deck < 4; ++deck) {
        midi.sendShortMsg(0x9F, deck, 0x00);
        midi.sendShortMsg(0x9F, 0x04 + deck, 0x00);
    }
    midi.sendShortMsg(0x96, 0x00, 0x00);
};
