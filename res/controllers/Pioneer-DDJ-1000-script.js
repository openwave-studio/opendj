// Pioneer-DDJ-1000-script.js
// ****************************************************************************
// * openDJ mapping script for the Pioneer DDJ-1000: what it changes in
// * openDJ's DDJ-FLX10 script, which drives the rest. The DDJ-FLX10 is the
// * DDJ-1000's successor and sends the same MIDI for the decks, pads, mixer,
// * Beat FX and browser. Written from Pioneer's "DDJ-1000 MIDI Message List",
// * cross-checked with rekordbox's DDJ-1000 mapping.
// ****************************************************************************
//
//  Changed here: the four SOUND COLOR FX buttons (D-ECHO, PITCH, NOISE,
//  FILTER), the PITCH Beat FX, MASTER TEMPO, the SEARCH buttons, TAP (SHIFT
//  + CUE), DECK SELECT, and the jog displays, which the DDJ-1000 takes on
//  each deck's own channel: the playhead going round, the cue point, BPM,
//  time, tempo, key and key shift, and MASTER and SYNC. The displays take
//  no waveform or artwork over MIDI.
//
//  SEARCH |<< goes back to the start of the track; >>| loads the next
//  track in the list, only while the deck is paused. Held, they search.
//  With SHIFT they call the previous and next hot cue.
//
//  The jogs are taken to count as the FLX10's do (5760 ticks a turn): the
//  same size of platter. If scratching feels too fast or slow on the
//  hardware, PioneerDDJ1000.jogTicksPerTurn is the number to change.
//
//  Not mapped yet: MEMORY and DELETE, the pad PAGE buttons, INPUT SELECT,
//  the mic, SAMPLER CUE, MASTER CUE and the fader start messages.

var PioneerDDJ1000 = {};

PioneerDDJ1000.jogTicksPerTurn = 5760;

(function() {
    const base = PioneerDDJFLX10;

    base.jogTicksPerTurn = PioneerDDJ1000.jogTicksPerTurn;
    base.jogScale = 720 / base.jogTicksPerTurn;

    // SOUND COLOR FX: D-ECHO, PITCH, NOISE and FILTER (notes 0x00 to 0x03,
    // 8 higher with SHIFT), as the engine's COLOR_FX numbers (1 Filter,
    // 3 Dub Echo, 4 Noise, 6 Pitch).
    base.colorFxButtons = {0x00: 3, 0x01: 6, 0x02: 4, 0x03: 1};

    // BEAT FX SELECT: PITCH where the FLX10 has STRETCH; Chorus is openDJ's
    // nearest (as on the DDJ-800).
    base.beatFxEffects[0x29] = 14;

    // The FLX10 puts its jog displays on channel 16 and lights stem part
    // buttons the DDJ-1000 doesn't have; drop those messages.
    const send = midi.sendShortMsg;
    midi.sendShortMsg = function(status, data1, value) {
        status &= 0xFF;
        if (status === 0x9F || status === 0xBF) {
            return;
        }
        if (status >= 0x90 && status <= 0x93 && data1 >= 0x0D && data1 <= 0x0F) {
            return;
        }
        send(status, data1, value);
    };
    PioneerDDJ1000.send = send;

    // The HID jog screens are the FLX10's alone.
    base.screensRunning = function() {};

    // MASTER, SYNC and MASTER TEMPO on the deck's own channel.
    const light = function(note) {
        return function(value, group) {
            send(0x90 + base.deckIndex(group), note, value > 0 ? 0x7F : 0x00);
        };
    };
    base.leaderDisplay = light(0x59);
    base.syncDisplay = light(0x5A);
    base.keylockDisplay = light(0x1A);

    base.jogDisplayUpdate = function() { PioneerDDJ1000.jogDisplayUpdate(); };
})();

PioneerDDJ1000.init = function() {
    PioneerDDJFLX10.decks.forEach(function(group, i) {
        // Show the display's information, light the jog ring, elapsed time.
        PioneerDDJ1000.send(0x90 + i, 0x5D, 0x00);
        PioneerDDJ1000.send(0x90 + i, 0x5B, 0x01);
        PioneerDDJ1000.send(0x90 + i, 0x44, 0x00);
    });
    PioneerDDJ1000.displays = PioneerDDJFLX10.decks.map(function() { return [-1, -1, -1, -1, -1, -1, -1, -1]; });
};

PioneerDDJ1000.shutdown = function() {
    PioneerDDJFLX10.decks.forEach(function(group, i) {
        PioneerDDJ1000.send(0x90 + i, 0x5D, 0x7F);
        PioneerDDJ1000.send(0x90 + i, 0x5B, 0x00);
        PioneerDDJ1000.send(0x90 + i, 0x1A, 0x00);
    });
};

// A 14-bit value as MSB on `cc` and LSB on `cc + 0x20`.
PioneerDDJ1000.send14 = function(status, cc, value) {
    PioneerDDJ1000.send(status, cc, (value >> 7) & 0x7F);
    PioneerDDJ1000.send(status, cc + 0x20, value & 0x7F);
};

// The playhead, cue point, BPM, tempo, time and key, sent when they change
// (the FLX10 script's timer calls this every 40 ms).
PioneerDDJ1000.jogDisplayUpdate = function() {
    const base = PioneerDDJFLX10;
    if (PioneerDDJ1000.displays === undefined) {
        return;
    }
    base.decks.forEach(function(group, i) {
        const loaded = engine.getValue(group, "track_loaded");
        const duration = engine.getValue(group, "duration");
        const seconds = engine.getValue(group, "playposition") * duration;
        let cue = 0x3FFF;
        // openDJ: cue_point is in seconds.
        const point = engine.getValue(group, "cue_point");
        if (loaded && point >= 0) {
            cue = base.degrees(point);
        }
        const elapsed = Math.max(0, Math.floor(loaded ? seconds : 0));
        const now = [
            loaded ? base.degrees(seconds) : 0,
            cue,
            loaded ? Math.min(9999, Math.round(engine.getValue(group, "bpm") * 10)) : 0,
            Math.min(99, Math.floor(elapsed / 60)),
            elapsed % 60,
            loaded ? base.displayKey(engine.getValue(group, "key")) : 0,
            0x0D + Math.max(-12, Math.min(12, Math.round(engine.getValue(group, "pitch_adjust")))),
            base.displaySpeed(group),
        ];
        const last = PioneerDDJ1000.displays[i];
        const cc = 0xB0 + i;
        const note = 0x90 + i;
        if (now[0] !== last[0]) { PioneerDDJ1000.send14(cc, 0x14, now[0]); }
        if (now[1] !== last[1]) { PioneerDDJ1000.send14(cc, 0x17, now[1]); }
        if (now[2] !== last[2]) { PioneerDDJ1000.send14(cc, 0x15, now[2]); }
        if (now[3] !== last[3]) { PioneerDDJ1000.send(note, 0x42, now[3]); }
        if (now[4] !== last[4]) { PioneerDDJ1000.send(note, 0x43, now[4]); }
        if (now[5] !== last[5]) { PioneerDDJ1000.send(note, 0x49, now[5]); }
        if (now[6] !== last[6]) { PioneerDDJ1000.send(note, 0x4A, now[6]); }
        if (now[7] !== last[7]) { PioneerDDJ1000.send14(cc, 0x16, now[7]); }
        PioneerDDJ1000.displays[i] = now;
    });
};

// MASTER TEMPO toggles key lock.
PioneerDDJ1000.keylock = function(_channel, _control, value, _status, group) {
    if (value) {
        script.toggleControl(group, "keylock");
    }
};

// SEARCH |<<: back to the start of the track.
PioneerDDJ1000.trackStart = function(_channel, _control, value, _status, group) {
    if (value) {
        engine.setValue(group, "start", 1);
    }
};

// SEARCH >>|: the next track in the list onto the deck, as on a CDJ; while
// the deck plays nothing happens, so a set isn't cut short.
PioneerDDJ1000.trackNext = function(_channel, _control, value, _status, group) {
    if (!value || engine.getValue(group, "play") > 0) {
        return;
    }
    engine.setValue("[Library]", "MoveDown", 1);
    engine.setValue(group, "LoadSelectedTrack", 1);
};

// DECK SELECT: the DDJ-1000's list gives note 0x72 on channel 3 (left) or
// 4 (right) for a press. Rekordbox also reads the deck state note 0x3C,
// which the FLX10 script follows; whichever comes, the side ends on the
// right deck. A press flips the side unless 0x3C has just said which.
PioneerDDJ1000.lastDeckState = 0;
PioneerDDJ1000.deckSwitch = function(channel, _control, value) {
    if (value !== 0x7F) {
        return;
    }
    const side = channel % 2;
    if (Date.now() - PioneerDDJ1000.lastDeckState < 250) {
        return;
    }
    const now = PioneerDDJFLX10.sideDeck[side];
    PioneerDDJFLX10.deckSelected(now >= 2 ? side : side + 2, 0x72, 0x7F);
};

(function() {
    const deckSelected = PioneerDDJFLX10.deckSelected;
    PioneerDDJFLX10.deckSelected = function(channel, control, value, status, group) {
        if (control === 0x3C && value === 0x7F) {
            PioneerDDJ1000.lastDeckState = Date.now();
        }
        return deckSelected(channel, control, value, status, group);
    };
})();
