// Traktor Kontrol Z1 MK2, played as Traktor plays it: a two-channel mixer
// whose knobs turn into stem volumes in STEMS mode, Mixer FX with four
// shortcuts and a filter, and a deck A/B or C/D switch. openDJ's own
// mapping, written from the Z1 MK2's published HID report layout and
// Native Instruments' manual. Its displays stay dark.
//
// Input report 0x01 (data[0] is the ID):
//   data[1]  EQ L 0x01, STEMS L 0x02, A/B-C/D 0x04, EQ R 0x08, STEMS R 0x10,
//            FX ON L 0x20, FX ON R 0x40, FX 1 0x80
//   data[2]  FX 2 0x01, FX 3 0x02, FX 4 0x04, FILTER 0x08, CUE L 0x10, CUE R 0x20
//   then 12-bit little-endian words: 3 gain L, 5 hi L, 7 mid L, 9 low L,
//   11 FX L, 13 gain R, 15 hi R, 17 mid R, 19 low R, 21 FX R, 23 headphone
//   mix, 25 main, 27 headphone level, 29 fader L, 31 fader R, 33 crossfader
// LED report 0x80, 46 bytes after the ID (indexes below count from 0):
//   0-9 meter L, 10-19 meter R (bottom first, 0x7E on), 20 EQ L, 21 STEMS L,
//   23 EQ R, 24 STEMS R, 25 FX ON L, 26 FX ON R, 27-30 FX 1-4, 31 FILTER,
//   32 CUE L, 33 CUE R, 34-39 bottom L, 40-45 bottom R.

var TraktorZ1MK2 = {};

// Colour codes: odd steps are dim, +2 is bright.
TraktorZ1MK2.BLUE = 0x2C;
TraktorZ1MK2.ORANGE = 0x0C;
TraktorZ1MK2.WHITE = 0x44;
TraktorZ1MK2.GREEN = 0x1C;

TraktorZ1MK2.init = function() {
    this.cd = false;
    this.stems = [false, false];
    this.last = null;
    this.leds = new Uint8Array(46);
    this.connections = [];
    for (var d = 1; d <= 4; d++) {
        var deck = "[Channel" + d + "]";
        var eq = "[EqualizerRack1_" + deck + "_Effect1]";
        engine.softTakeover(deck, "pregain", true);
        engine.softTakeover(deck, "volume", true);
        engine.softTakeover("[QuickEffectRack1_" + deck + "]", "super1", true);
        for (var p = 1; p <= 3; p++) {
            engine.softTakeover(eq, "parameter" + p, true);
        }
        for (var s = 1; s <= 4; s++) {
            engine.softTakeover("[Channel" + d + "_Stem" + s + "]", "volume", true);
        }
    }
    this.connectLights();
    this.lightAll();
};

TraktorZ1MK2.shutdown = function() {
    this.leds = new Uint8Array(46);
    this.send();
};

TraktorZ1MK2.deck = function(side) {
    return "[Channel" + (side + 1 + (this.cd ? 2 : 0)) + "]";
};

TraktorZ1MK2.incomingData = function(data) {
    if (data[0] !== 0x01 || data.length < 35) {
        return;
    }
    var last = this.last;
    this.last = Array.prototype.slice.call(data, 0, 35);
    if (last === null) {
        return;
    }
    var self = this;
    var pressed = function(b, mask) {
        return (data[b] & mask) !== 0 && (last[b] & mask) === 0;
    };
    var word = function(bytes, at) {
        return (bytes[at] | (bytes[at + 1] << 8)) & 0x0FFF;
    };

    if (pressed(1, 0x04)) {
        this.cd = !this.cd;
        // The knobs pick the other decks up where they are.
        this.lightAll();
    }
    for (var side = 0; side < 2; side++) {
        if (pressed(1, side ? 0x08 : 0x01)) {
            this.stems[side] = false;
            this.lightAll();
        }
        if (pressed(1, side ? 0x10 : 0x02) && engine.getValue(this.deck(side), "stem_count") > 0) {
            this.stems[side] = true;
            this.lightAll();
        }
        if (pressed(1, side ? 0x40 : 0x20)) {
            script.toggleControl("[QuickEffectRack1_" + this.deck(side) + "]", "enabled");
        }
        if (pressed(2, side ? 0x20 : 0x10)) {
            script.toggleControl(this.deck(side), "pfl");
        }
    }
    // FILTER picks Mixer FX 1, the filter; 1-4 the next four, on both channels.
    var picks = [[2, 0x08], [1, 0x80], [2, 0x01], [2, 0x02], [2, 0x04]];
    picks.forEach(function(pick, n) {
        if (pressed(pick[0], pick[1])) {
            for (var d = 1; d <= 4; d++) {
                engine.setValue("[QuickEffectRack1_[Channel" + d + "]]", "loaded_chain_preset", n + 1);
            }
            self.lightAll();
        }
    });

    var knobs = [
        [3, 0, "gain"], [5, 0, "hi"], [7, 0, "mid"], [9, 0, "low"], [11, 0, "fx"],
        [13, 1, "gain"], [15, 1, "hi"], [17, 1, "mid"], [19, 1, "low"], [21, 1, "fx"],
        [23, -1, "headMix"], [27, -1, "headGain"],
        [29, 0, "volume"], [31, 1, "volume"], [33, -1, "crossfader"],
    ];
    knobs.forEach(function(k) {
        var raw = word(data, k[0]);
        if (raw === word(last, k[0])) {
            return;
        }
        // Knobs with a centre detent send 0x7FF there: exactly the middle.
        var value = raw === 0x7FF ? 0.5 : raw / 4095;
        if (k[1] < 0) {
            engine.setParameter("[Master]", k[2], value);
        } else {
            self.knob(k[1], k[2], value);
        }
    });
};

TraktorZ1MK2.knob = function(side, name, value) {
    var deck = this.deck(side);
    var eq = "[EqualizerRack1_" + deck + "_Effect1]";
    var stem = ["gain", "hi", "mid", "low"].indexOf(name);
    if (this.stems[side] && stem >= 0) {
        // STEMS: gain, hi, mid and low set the four stems' volumes, from
        // silent to full volume as in Traktor.
        var group = "[" + deck.slice(1, -1) + "_Stem" + (stem + 1) + "]";
        engine.setValue(group, "volume", value);
        return;
    }
    switch (name) {
    case "gain": engine.setParameter(deck, "pregain", value); break;
    case "hi": engine.setParameter(eq, "parameter3", value); break;
    case "mid": engine.setParameter(eq, "parameter2", value); break;
    case "low": engine.setParameter(eq, "parameter1", value); break;
    case "fx": engine.setParameter("[QuickEffectRack1_" + deck + "]", "super1", value); break;
    case "volume": engine.setParameter(deck, "volume", value); break;
    }
};

// --- Lights ------------------------------------------------------------------

TraktorZ1MK2.send = function() {
    controller.sendOutputReport(0x80, this.leds.buffer);
};

TraktorZ1MK2.connectLights = function() {
    var self = this;
    var relight = function() { self.lightAll(); };
    for (var d = 1; d <= 4; d++) {
        var deck = "[Channel" + d + "]";
        ["pfl", "stem_count", "track_loaded"].forEach(function(key) {
            self.connections.push(engine.makeConnection(deck, key, relight));
        });
        this.connections.push(engine.makeConnection("[QuickEffectRack1_" + deck + "]", "enabled", relight));
        this.connections.push(engine.makeConnection("[QuickEffectRack1_" + deck + "]", "loaded_chain_preset", relight));
        this.connections.push(engine.makeConnection(deck, "vu_meter", function() { self.lightMeters(); }));
    }
};

TraktorZ1MK2.lightMeters = function() {
    for (var side = 0; side < 2; side++) {
        var level = engine.getValue(this.deck(side), "vu_meter");
        for (var i = 0; i < 10; i++) {
            this.leds[10 * side + i] = level * 10 > i + 0.5 ? 0x7E : 0x00;
        }
    }
    this.send();
};

TraktorZ1MK2.lightAll = function() {
    var bright = function(color, on) { return color + (on ? 2 : 0); };
    var preset = engine.getValue("[QuickEffectRack1_" + this.deck(0) + "]", "loaded_chain_preset");
    for (var side = 0; side < 2; side++) {
        var deck = this.deck(side);
        // Decks A and B in blue, C and D in orange, as in Traktor.
        var color = this.cd ? this.ORANGE : this.BLUE;
        var hasStems = engine.getValue(deck, "stem_count") > 0;
        this.leds[side ? 23 : 20] = bright(this.WHITE, !this.stems[side]);
        this.leds[side ? 24 : 21] = hasStems ? bright(this.GREEN, this.stems[side]) : 0;
        this.leds[25 + side] = bright(this.BLUE, engine.getValue("[QuickEffectRack1_" + deck + "]", "enabled"));
        this.leds[32 + side] = bright(this.ORANGE, engine.getValue(deck, "pfl"));
        for (var i = 0; i < 6; i++) {
            this.leds[34 + 6 * side + i] = color;
        }
    }
    for (var n = 0; n < 4; n++) {
        this.leds[27 + n] = bright(this.WHITE, preset === n + 2);
    }
    this.leds[31] = bright(this.WHITE, preset === 1);
    this.lightMeters();
};
