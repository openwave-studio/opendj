// Pioneer-DDJ-FLX10-screens.js
// ****************************************************************************
// * openDJ: the DDJ-FLX10's jog screens over USB HID.
// ****************************************************************************
//
//  The FLX10's MIDI messages put BPM, time and a playhead on the jog
//  screens; the waveform needs its USB HID interface (5). This mapping
//  sends it, beside the "Pioneer DDJ-FLX10" MIDI mapping, which switches
//  the screens to HID with SysEx messages once this one runs (see its
//  screensRunning) and leaves the screens to it.
//
//  What the screens take was worked out by Veezuhz and Loui1979 from what
//  DJ apps send (github.com/Veezuhz/Mixxx_FLX10_Controller_Mapping,
//  github.com/Loui1979/MIXXX-FLX10). This is openDJ's own code, written
//  from their notes. Every report is 128 bytes with no report ID: byte 0
//  is the deck (0x10, 0x20, 0x30, 0x40), byte 1 what the report is.
//
//    xx27  the deck's state, many times a second: time played and the
//          track's length, BPM, tempo, key. The screen places the waveform
//          by the time played.
//    xx30  a track is loaded: its length.
//    xx39  the HOT CUE labels.
//    xx35  how many waveform entries the track has (three reports).
//    xx36  waveform entries, 19 a report: 150 a second of the track, each
//          16 bits, little-endian: height (5 bits) and the low, mid and
//          high bands as red, green and blue (3 bits each).
//    xx2f  cue data (sent empty).
//
//  After loading, a report of entries at the playhead goes now and then,
//  or the screen forgets the waveform.
//
//  Not tried on the hardware by openDJ yet: the settings below are what
//  to try changing if the screens misbehave.
//
var PioneerDDJFLX10Screens = {};

// What the MIDI mapping and this one share (see the MIDI script).
PioneerDDJFLX10Screens.screens = "[DDJFLX10Screens]";

// Settings.
PioneerDDJFLX10Screens.stateMs = 5;            // xx27 every 5 ms (200 a second)
PioneerDDJFLX10Screens.trickleMs = 125;        // an xx36 at the playhead this often
PioneerDDJFLX10Screens.uploadPerTick = 8;      // waveform reports sent each tick while loading
PioneerDDJFLX10Screens.analysisWaitMs = 30000; // how long to wait for a track's analysis
PioneerDDJFLX10Screens.entriesPerSecond = 150;
PioneerDDJFLX10Screens.entriesPerReport = 19;
// A paused or scratched deck may step back a little as the audio catches
// up; steps back shorter than this while playing are held, not shown, as
// the screen redraws on every step back.
PioneerDDJFLX10Screens.holdBackMs = 80;

PioneerDDJFLX10Screens.deckByte = [0x10, 0x20, 0x30, 0x40];
// xx27 byte 31: decks 1 and 2 are the screens' own, 3 and 4 the others.
PioneerDDJFLX10Screens.deckRole = [0x02, 0x01, 0x04, 0x03];

PioneerDDJFLX10Screens.report = function(deck, kind) {
    const r = new Uint8Array(128);
    r[0] = PioneerDDJFLX10Screens.deckByte[deck];
    r[1] = kind;
    return r;
};

PioneerDDJFLX10Screens.send = function(report) {
    // In order, every one: the FIFO keeps reports of the same ID (0 here)
    // from replacing each other.
    controller.send(report, null, 0, true);
};

// The deck's key on the screen (openDJ's `key`: 1-12 C to B major, 13-24 C
// to B minor) as its Camelot number: majors 0x81, 0x83... from C (8B) a
// semitone at a time, minors 0x82, 0x84... from A minor (8A); 0x80 none.
PioneerDDJFLX10Screens.keyByte = function(key) {
    key = Math.round(key);
    if (key >= 1 && key <= 12) {
        return 0x81 + 2 * (key - 1);
    }
    if (key >= 13 && key <= 24) {
        return 0x82 + 2 * ((key - 22 + 12) % 12);
    }
    return 0x80;
};

// Seconds as the screen counts them: minutes, seconds, milliseconds
// (little-endian, never above 999).
PioneerDDJFLX10Screens.putTime = function(r, at, seconds) {
    const ms = Math.max(0, Math.floor(seconds * 1000));
    const sub = Math.min(999, ms % 1000);
    r[at] = Math.min(255, Math.floor(ms / 60000));
    r[at + 1] = Math.floor(ms / 1000) % 60;
    r[at + 2] = sub & 0xFF;
    r[at + 3] = sub >> 8;
};

PioneerDDJFLX10Screens.rateRatio = function(group) {
    const dir = engine.getValue(group, "rate_dir") < 0 ? -1 : 1;
    return 1 + engine.getValue(group, "rate") * engine.getValue(group, "rateRange") * dir;
};

// xx27: the deck's state.
PioneerDDJFLX10Screens.state = function(deck, playedMs, durationS, bpm, ratio, key) {
    const r = PioneerDDJFLX10Screens.report(deck, 0x27);
    r[2] = 0xB4;
    r[3] = 0x88;
    r[4] = 0x01;
    // Time played (wall time, at the deck's tempo).
    PioneerDDJFLX10Screens.putTime(r, 5, playedMs / 1000);
    r[8] &= 0x03;
    // The track's length at the deck's tempo.
    PioneerDDJFLX10Screens.putTime(r, 9, durationS / ratio);
    if (bpm > 0) {
        r[13] = Math.floor(bpm) & 0xFF;
        r[14] = (Math.floor((bpm - Math.floor(bpm)) * 16) & 0x0F) << 4;
    }
    // Tempo in hundredths of a percent, signed 16-bit little-endian.
    const tempo = Math.max(-32768, Math.min(32767, Math.round((ratio - 1) * 10000))) & 0xFFFF;
    r[15] = 0x01;
    r[16] = tempo & 0xFF;
    r[17] = tempo >> 8;
    r[19] = 0x07;
    r[20] = 0x0E;
    // Two counters that follow the time played: one turns over 2048 times
    // a second, the other (decks 1 and 2 only) steps every 123.2/1024 s.
    const ticks = Math.floor(playedMs / 1000) * 1024 + Math.floor((playedMs % 1000) * 1024 / 1000);
    r[21] = (ticks * 2) & 0xFF;
    r[22] = deck < 2 ? Math.floor(ticks / 123.2) % 15 : 0;
    r[25] = 0x80;
    r[29] = PioneerDDJFLX10Screens.keyByte(key);
    r[30] = 0x0D;
    r[31] = PioneerDDJFLX10Screens.deckRole[deck];
    r[32] = 0xAD;
    r[33] = 0x05;
    return r;
};

// The reports a newly loaded track needs, waveform `entries` (Uint16Array)
// and all.
PioneerDDJFLX10Screens.loadReports = function(deck, durationS, entries) {
    const S = PioneerDDJFLX10Screens;
    const out = [];
    const length = S.report(deck, 0x30);
    length[2] = 0x01;
    length[4] = 0x01;
    S.putTime(length, 6, durationS);
    out.push(length);
    out.push.apply(out, S.hotCueLabels(deck));
    const count = Math.min(0xFFFF, Math.round(durationS * S.entriesPerSecond));
    for (let i = 0; i < 3; ++i) {
        const r = S.report(deck, 0x35);
        r[2] = count & 0xFF;
        r[3] = count >> 8;
        out.push(r);
    }
    for (let at = 0; at < entries.length; at += S.entriesPerReport) {
        out.push(S.entriesReport(deck, entries, at));
    }
    const cues = S.report(deck, 0x2F);
    cues[2] = 0x01;
    cues[4] = 0x01;
    out.push(cues);
    return out;
};

// xx36: entries from `at` on.
PioneerDDJFLX10Screens.entriesReport = function(deck, entries, at) {
    const S = PioneerDDJFLX10Screens;
    const r = S.report(deck, 0x36);
    r[6] = S.entriesPerReport;
    r[10] = at & 0xFF;
    r[11] = (at >> 8) & 0xFF;
    r[12] = (at >> 16) & 0xFF;
    r[13] = (at >>> 24) & 0xFF;
    const end = Math.min(entries.length, at + S.entriesPerReport);
    for (let i = at, o = 14; i < end; ++i, o += 2) {
        r[o] = entries[i] & 0xFF;
        r[o + 1] = entries[i] >> 8;
    }
    return r;
};

// xx39: "HOT CUE" and eight empty hot cue slots, 32 bytes apart, over
// three reports of 122 bytes each.
PioneerDDJFLX10Screens.hotCueLabels = function(deck) {
    const S = PioneerDDJFLX10Screens;
    const body = new Uint8Array(3 * 122);
    const label = "HOT CUE";
    for (let i = 0; i < label.length; ++i) {
        body[1 + i] = label.charCodeAt(i);
    }
    for (let slot = 1; slot <= 9; ++slot) {
        if (slot >= 2) {
            body[32 * slot - 1] = 0x02;
        }
        if (slot <= 8) {
            body[32 * slot] = 0x3F;
        }
    }
    const out = [];
    for (let part = 0; part < 3; ++part) {
        const r = S.report(deck, 0x39);
        r[2] = part + 1;
        r[4] = 3;
        r.set(body.subarray(part * 122, (part + 1) * 122), 6);
        out.push(r);
    }
    return out;
};

// openDJ's waveform (four bytes an entry: low, mid, high, all, 0-255) as
// the screen's entries: height from the whole signal, colour from the
// bands, the strongest at full brightness.
PioneerDDJFLX10Screens.entries = function(wave) {
    const n = Math.floor(wave.length / 4);
    const out = new Uint16Array(n);
    for (let i = 0; i < n; ++i) {
        const low = wave[4 * i];
        const mid = wave[4 * i + 1];
        const high = wave[4 * i + 2];
        const all = wave[4 * i + 3];
        const top = Math.max(low, mid, high, 1);
        const shade = function(v) { return Math.round(v / top * 7); };
        const height = Math.round(all * 31 / 255);
        out[i] = (shade(low) << 13) | (shade(mid) << 10) | (shade(high) << 7) | (height << 2);
    }
    return out;
};

PioneerDDJFLX10Screens.init = function() {
    const S = PioneerDDJFLX10Screens;
    S.gate = 0;
    S.decks = [0, 1, 2, 3].map(function(i) {
        return {
            group: "[Channel" + (i + 1) + "]",
            loadedAt: 0,      // when the track came (0: nothing to send)
            entries: null,    // the waveform sent
            queue: [],        // reports still to send for the track
            played: {ms: 0, target: -1, at: 0},
            trickleAt: 0,
            trickled: -1,
        };
    });
    S.decks.forEach(function(d, i) {
        const loaded = function() { S.trackChanged(i); };
        engine.makeConnection(d.group, "track_loaded", loaded);
        engine.makeConnection(d.group, "duration", loaded);
    });
    // The MIDI mapping switched the screens over: send everything again.
    engine.makeConnection(S.screens, "gate", function(value) {
        S.gate = value;
        S.decks.forEach(function(_, i) { S.trackChanged(i); });
    });
    S.gate = engine.getValue(S.screens, "gate");
    S.timer = engine.beginFastTimer(S.stateMs, S.tick);
    engine.setValue(S.screens, "running", 1);
};

PioneerDDJFLX10Screens.shutdown = function() {
    const S = PioneerDDJFLX10Screens;
    engine.stopTimer(S.timer);
    engine.setValue(S.screens, "running", 0);
};

// Input reports are the screens' acknowledgements; nothing to do.
PioneerDDJFLX10Screens.incomingData = function() {};

PioneerDDJFLX10Screens.trackChanged = function(i) {
    const d = PioneerDDJFLX10Screens.decks[i];
    d.loadedAt = Date.now();
    d.entries = null;
    d.queue = [];
    d.trickled = -1;
    d.played.target = -1;
};

// The time played to show, in milliseconds: the deck's own, kept from
// stepping back a little while it plays, and moving on between the
// engine's updates.
PioneerDDJFLX10Screens.played = function(d, now, seconds, ratio, playing) {
    const p = d.played;
    const target = seconds / ratio * 1000;
    if (target !== p.target) {
        p.target = target;
        p.at = now;
    }
    let ms = target + (playing ? Math.min(100, now - p.at) : 0);
    if (playing && ms < p.ms && p.ms - ms < PioneerDDJFLX10Screens.holdBackMs) {
        ms = p.ms;
    }
    p.ms = ms;
    return ms;
};

PioneerDDJFLX10Screens.tick = function() {
    const S = PioneerDDJFLX10Screens;
    if (!(S.gate > 0)) {
        return;
    }
    const now = Date.now();
    S.decks.forEach(function(d, i) {
        const group = d.group;
        const durationS = engine.getValue(group, "duration");
        if (!engine.getValue(group, "track_loaded") || !(durationS > 0)) {
            return;
        }
        const ratio = Math.max(0.01, S.rateRatio(group));
        const seconds = engine.getValue(group, "playposition") * durationS;
        const playing = engine.getValue(group, "play") > 0;
        const ms = S.played(d, now, seconds, ratio, playing);
        S.send(S.state(i, ms, durationS, engine.getValue(group, "bpm"), ratio, engine.getValue(group, "key")));

        if (d.entries === null && d.loadedAt) {
            // Wait for the analysis to cover the track, then load it.
            const wave = engine.getWaveform(group, S.entriesPerSecond);
            const whole = wave.length / 4 >= Math.floor(durationS * S.entriesPerSecond) - S.entriesPerSecond;
            if (whole || (wave.length > 0 && now - d.loadedAt > S.analysisWaitMs)) {
                d.entries = S.entries(wave);
                d.queue = S.loadReports(i, durationS, d.entries);
            }
        }
        if (d.queue.length > 0) {
            d.queue.splice(0, S.uploadPerTick).forEach(S.send);
            if (d.queue.length === 0) {
                // Loading leaves the screen at the end of the track: bring
                // it back to the playhead at once.
                d.trickleAt = 0;
            }
            return;
        }
        if (d.entries && d.entries.length > 0 && now - d.trickleAt >= S.trickleMs) {
            const last = Math.max(0, d.entries.length - S.entriesPerReport);
            const at = Math.max(0, Math.min(last, Math.floor(seconds * S.entriesPerSecond)));
            if (at !== d.trickled) {
                S.send(S.entriesReport(i, d.entries, at));
                d.trickled = at;
                d.trickleAt = now;
            }
        }
    });
};
