// Traktor Kontrol D2, played as Traktor plays it: one deck at a time (A or
// C, or B or D) with an effect unit on top, the performance knobs, faders and
// ON buttons for stems and the second effect unit, eight pads in HOT CUE,
// LOOP, FREEZE (slicer) and REMIX (sample) modes, a touch strip and
// transport. openDJ's own mapping, written from the D2's published HID
// report layout and Native Instruments' manual. Its display stays dark.
//
// Input report 0x01, buttons (17 bytes with the ID):
//   byte 1  browse encoder (high nibble) and loop encoder (low nibble), 0-15
//   byte 2  screen left 3 0x01, browse push 0x08, screen left 2 0x10,
//           screen left 1 0x20, FX SELECT 0x40, FX 1 0x80
//   byte 3  FX 4 0x01, FX 3 0x02, FX 2 0x04, screen right 1-4 0x08-0x40
//   byte 4  EDIT 0x01, ON 4 0x02, ON 3 0x04, ON 2 0x08, ON 1 0x10,
//           CAPTURE 0x20, screen left 4 0x40, BACK 0x80
//   byte 5  A 0x01, B 0x02, C 0x04, D 0x08
//   byte 6  pad 3 0x01, pad 4 0x02, loop push 0x10
//   byte 7  pad 2 0x01, pad 8 0x02, DECK 0x04, pad 1 0x08, FLUX 0x10,
//           pad 5 0x20, pad 6 0x40, pad 7 0x80
//   byte 8  REMIX 0x01, FREEZE 0x02, LOOP 0x04, HOTCUE 0x08, PLAY 0x10,
//           CUE 0x20, SYNC 0x40, SHIFT 0x80
//   bytes 9-10  touch sensors; bytes 13-14 touch strip, 0 untouched, 1-1024
// Input report 0x02, knobs and faders (25 bytes with the ID):
//   bytes 1-8   performance knobs 1-4, endless: 16-bit counters, 1000 a turn
//   bytes 9-16  faders 1-4, bytes 17-24 FX knobs 1-4: 12 bits, 0-4078
// LED report 0x80, 122 bytes after the ID (indexes below count from 0):
//   0-23 pads 1-8 as blue, green, red; 24 FX SELECT; 25-28 FX 1-4;
//   29-36 screen buttons; 37 BACK, 38 CAPTURE, 39 EDIT; 40-43 ON 1-4;
//   44-51 HOTCUE, LOOP, FREEZE, REMIX (white, blue); 52 FLUX;
//   53-54 DECK (white, blue); 55 SHIFT; 56-57 SYNC (green, red); 58 CUE;
//   59 PLAY; 60-67 loop encoder ring (white x4, blue x4); 68-92 touch strip
//   blue, 93-117 orange; 118-121 A-D.

var TraktorD2 = {};

TraktorD2.HOTCUE = 0;
TraktorD2.LOOP = 1;
TraktorD2.FREEZE = 2;
TraktorD2.REMIX = 3;

TraktorD2.ON = 0x7F;
TraktorD2.DIM = 0x0A;

// Pad numbers in the report: [byte, mask] for pads 1-8, top row first.
TraktorD2.PADS = [[7, 0x08], [7, 0x01], [6, 0x01], [6, 0x02],
    [7, 0x20], [7, 0x40], [7, 0x80], [7, 0x02]];

// LOOP pads as in Traktor: loops of 1/8, 1/4, 1/2 and 1 beat on top; below,
// jump back by the loop, back 1 beat, forward 1 beat, forward by the loop.
TraktorD2.LOOP_SIZES = ["0.125", "0.25", "0.5", "1"];

TraktorD2.init = function() {
    this.bd = false;       // decks B and D instead of A and C
    this.bottom = false;   // C or D in focus instead of A or B
    this.mode = this.HOTCUE;
    this.shift = false;
    this.deckHeld = false;
    this.deckAssigned = false;
    this.buttons = null;
    this.knobs = null;
    this.strip = 0;
    this.stripLit = -1;
    this.leds = new Uint8Array(122);
    this.connections = [];
    for (var u = 1; u <= 4; u++) {
        engine.softTakeover("[EffectRack1_EffectUnit" + u + "]", "mix", true);
        for (var e = 1; e <= 3; e++) {
            engine.softTakeover("[EffectRack1_EffectUnit" + u + "_Effect" + e + "]", "meta", true);
        }
    }
    for (var d = 1; d <= 4; d++) {
        for (var s = 1; s <= 4; s++) {
            engine.softTakeover("[Channel" + d + "_Stem" + s + "]", "volume", true);
        }
    }
    this.connectLights();
    this.lightAll();
};

TraktorD2.shutdown = function() {
    this.leds = new Uint8Array(122);
    this.send();
};

TraktorD2.deckNumber = function() {
    return 1 + (this.bd ? 1 : 0) + (this.bottom ? 2 : 0);
};

TraktorD2.deck = function() {
    return "[Channel" + this.deckNumber() + "]";
};

// The effect unit on top: 1 with decks A and C, 2 with B and D. The
// performance knobs play unit 3 or 4, as in Traktor with four FX units.
TraktorD2.unit = function(lower) {
    return "[EffectRack1_EffectUnit" + (1 + (this.bd ? 1 : 0) + (lower ? 2 : 0)) + "]";
};

TraktorD2.hasStems = function() {
    return engine.getValue(this.deck(), "stem_count") > 0;
};

TraktorD2.incomingData = function(data) {
    if (data[0] === 0x01 && data.length >= 17) {
        this.buttonReport(data);
    } else if (data[0] === 0x02 && data.length >= 25) {
        this.knobReport(data);
    }
};

TraktorD2.buttonReport = function(data) {
    var last = this.buttons;
    this.buttons = Array.prototype.slice.call(data, 0, 17);
    if (last === null) {
        return;
    }
    var self = this;
    var button = function(b, mask, fn) {
        var now = (data[b] & mask) !== 0;
        if (now !== ((last[b] & mask) !== 0)) {
            fn.call(self, now);
        }
    };

    button(8, 0x80, function(down) {
        this.shift = down;
        this.lightAll();
    });
    button(7, 0x04, this.deckButton);
    for (var n = 0; n < 4; n++) {
        var i = n;
        button(5, 1 << n, function(down) { this.assign(i, down); });
        button(n === 0 ? 2 : 3, [0x80, 0x04, 0x02, 0x01][n], function(down) { this.fxButton(i, down); });
        button(4, 0x10 >> n, function(down) { this.onButton(i, down); });
        button(8, 0x08 >> n, function(down) {
            if (down) {
                this.mode = i;
                this.lightAll();
            }
        });
    }
    this.PADS.forEach(function(pad, p) {
        button(pad[0], pad[1], function(down) { this.pad(p, down); });
    });
    button(7, 0x10, function(down) {
        if (down) {
            script.toggleControl(this.deck(), "slip_enabled");
        }
    });
    button(8, 0x10, function(down) {
        if (down) {
            script.toggleControl(this.deck(), "play");
        }
    });
    button(8, 0x20, function(down) {
        engine.setValue(this.deck(), this.shift ? "start" : "cue_default", down ? 1 : 0);
    });
    button(8, 0x40, function(down) {
        if (!down) {
            return;
        }
        if (this.shift) {
            var leader = engine.getValue(this.deck(), "sync_leader") > 0;
            engine.setValue(this.deck(), "sync_leader", leader ? 0 : 1);
        } else {
            script.toggleControl(this.deck(), "sync_enabled");
        }
    });
    button(2, 0x08, function(down) {
        if (down) {
            script.triggerControl(this.shift ? "[Library]" : this.deck(),
                this.shift ? "GoToItem" : "LoadSelectedTrack");
        }
    });
    button(6, 0x10, function(down) {
        if (!down) {
            return;
        }
        var deck = this.deck();
        if (engine.getValue(deck, "loop_enabled")) {
            script.triggerControl(deck, "reloop_toggle");
        } else {
            script.triggerControl(deck, "beatloop_activate");
        }
    });

    var browse = this.step(last[1] >> 4, data[1] >> 4);
    if (browse !== 0) {
        engine.setValue("[Library]", "focused_widget", this.shift ? 2 : 3);
        engine.setValue("[Library]", "MoveVertical", browse);
    }
    var loop = this.step(last[1] & 0x0F, data[1] & 0x0F);
    if (loop !== 0) {
        this.loopTurn(loop);
    }
    this.touchStrip(data[13] | (data[14] << 8));
};

// One step of a 4-bit encoder that wraps from 15 to 0.
TraktorD2.step = function(before, now) {
    if (before === now) {
        return 0;
    }
    var d = (now - before + 16) % 16;
    return d < 8 ? d : d - 16;
};

TraktorD2.knobReport = function(data) {
    var last = this.knobs;
    this.knobs = Array.prototype.slice.call(data, 0, 25);
    if (last === null) {
        return;
    }
    var word = function(bytes, at) {
        return bytes[at] | (bytes[at + 1] << 8);
    };
    for (var n = 0; n < 4; n++) {
        // Performance knobs: endless, a thousand counts a turn.
        var turned = word(data, 1 + 2 * n) - word(last, 1 + 2 * n);
        if (turned > 32767) {
            turned -= 65536;
        } else if (turned < -32768) {
            turned += 65536;
        }
        if (turned > 500) {
            turned -= 1000;
        } else if (turned < -500) {
            turned += 1000;
        }
        if (turned !== 0) {
            this.performanceKnob(n, turned / 1000);
        }
        var fader = word(data, 9 + 2 * n) & 0x0FFF;
        if (fader !== (word(last, 9 + 2 * n) & 0x0FFF)) {
            this.fader(n, Math.min(fader / 4078, 1));
        }
        var fx = word(data, 17 + 2 * n) & 0x0FFF;
        if (fx !== (word(last, 17 + 2 * n) & 0x0FFF)) {
            this.fxKnob(n, Math.min(fx / 4078, 1));
        }
    }
};

// --- Decks -------------------------------------------------------------------

// DECK switches between the two decks; held, A, B, C or D picks the deck.
TraktorD2.deckButton = function(down) {
    this.deckHeld = down;
    if (down) {
        this.deckAssigned = false;
        return;
    }
    if (!this.deckAssigned) {
        this.bottom = !this.bottom;
    }
    this.stripLit = -1;
    this.lightAll();
};

// A-D send the decks to the effect unit on top (SHIFT: the second unit).
TraktorD2.assign = function(n, down) {
    if (!down) {
        return;
    }
    if (this.deckHeld) {
        this.bd = n % 2 === 1;
        this.bottom = n >= 2;
        this.deckAssigned = true;
        this.stripLit = -1;
        this.lightAll();
        return;
    }
    script.toggleControl(this.unit(this.shift), "group_[Channel" + (n + 1) + "]_enable");
};

// --- Effects -----------------------------------------------------------------

TraktorD2.fxKnob = function(n, value) {
    this.unitKnob(this.unit(false), n, value);
};

TraktorD2.unitKnob = function(unit, n, value) {
    if (n === 0) {
        engine.setParameter(unit, "mix", value);
    } else {
        engine.setParameter(unit.slice(0, -1) + "_Effect" + n + "]", "meta", value);
    }
};

TraktorD2.fxButton = function(n, down) {
    if (down) {
        this.unitButton(this.unit(false), n);
    }
};

TraktorD2.unitButton = function(unit, n) {
    if (n === 0) {
        script.toggleControl(unit, "enabled");
    } else if (this.shift) {
        script.triggerControl(unit.slice(0, -1) + "_Effect" + n + "]", "next_effect");
    } else {
        script.toggleControl(unit.slice(0, -1) + "_Effect" + n + "]", "enabled");
    }
};

// --- Performance section: stems, or the second effect unit -------------------

TraktorD2.performanceKnob = function(n, turned) {
    var unit = this.unit(true);
    var group = n === 0 ? unit : unit.slice(0, -1) + "_Effect" + n + "]";
    var key = n === 0 ? "mix" : "meta";
    var value = Math.max(0, Math.min(1, engine.getParameter(group, key) + turned));
    engine.setParameter(group, key, value);
};

// ON: with stems, each stem's FX send (SHIFT mutes it); otherwise the
// second effect unit's buttons.
TraktorD2.onButton = function(n, down) {
    if (!down) {
        return;
    }
    if (this.hasStems()) {
        var stem = "[Channel" + this.deckNumber() + "_Stem" + (n + 1) + "]";
        if (this.shift) {
            script.toggleControl(stem, "mute");
        } else {
            script.toggleControl("[QuickEffectRack1_" + stem + "]", "enabled");
        }
        return;
    }
    this.unitButton(this.unit(true), n);
};

// Faders: stem volumes, silent to full as in Traktor.
TraktorD2.fader = function(n, value) {
    if (this.hasStems()) {
        engine.setValue("[Channel" + this.deckNumber() + "_Stem" + (n + 1) + "]", "volume", value);
    }
};

// --- Pads --------------------------------------------------------------------

TraktorD2.pad = function(p, down) {
    var deck = this.deck();
    switch (this.mode) {
    case this.HOTCUE:
        engine.setValue(deck, "hotcue_" + (p + 1) + (this.shift ? "_clear" : "_activate"), down ? 1 : 0);
        break;
    case this.LOOP:
        if (!down) {
            return;
        }
        if (p < 4) {
            script.triggerControl(deck, "beatloop_" + this.LOOP_SIZES[p] + "_activate");
        } else if (p === 5 || p === 6) {
            script.triggerControl(deck, p === 5 ? "beatjump_1_backward" : "beatjump_1_forward");
        } else {
            engine.setValue(deck, "beatjump", (p === 4 ? -1 : 1) * engine.getValue(deck, "beatloop_size"));
        }
        break;
    case this.FREEZE:
        if (this.shift) {
            if (down) {
                engine.setValue(deck, "slicer_enabled", 0);
            }
            return;
        }
        engine.setValue(deck, "slicer_" + (p + 1) + "_activate", down ? 1 : 0);
        break;
    case this.REMIX:
        if (!down) {
            return;
        }
        var sampler = "[Sampler" + (p + 1) + "]";
        if (this.shift) {
            script.triggerControl(sampler, "stop");
        } else {
            script.triggerControl(sampler, "cue_gotoandplay");
        }
        break;
    }
};

// --- Browse, loop and touch strip --------------------------------------------

TraktorD2.loopTurn = function(steps) {
    var deck = this.deck();
    if (this.shift) {
        // Jump by the loop's length; an active loop moves along.
        if (engine.getValue(deck, "loop_enabled")) {
            engine.setValue(deck, "loop_move_loops", steps > 0 ? 1 : -1);
        } else {
            engine.setValue(deck, "beatjump", (steps > 0 ? 1 : -1) * engine.getValue(deck, "beatloop_size"));
        }
        return;
    }
    script.triggerControl(deck, steps > 0 ? "loop_double" : "loop_halve");
};

// The touch strip bends (or scrubs, paused) by sliding along it; with SHIFT
// a touch jumps to that place in the track.
TraktorD2.touchStrip = function(position) {
    var before = this.strip;
    this.strip = position;
    if (position === 0) {
        return;
    }
    var deck = this.deck();
    if (this.shift) {
        engine.setValue(deck, "playposition", Math.min(position / 1024, 1));
    } else if (before !== 0) {
        engine.setValue(deck, "jog", (position - before) / 16);
    }
};

// --- Lights ------------------------------------------------------------------

TraktorD2.send = function() {
    controller.sendOutputReport(0x80, this.leds.buffer);
};

TraktorD2.connectLights = function() {
    var self = this;
    var relight = function() { self.lightAll(); };
    var keys = ["play_indicator", "cue_indicator", "sync_enabled", "slip_enabled", "loop_enabled",
        "slicer_enabled", "stem_count", "track_loaded"];
    for (var d = 1; d <= 4; d++) {
        var deck = "[Channel" + d + "]";
        keys.forEach(function(key) {
            self.connections.push(engine.makeConnection(deck, key, relight));
        });
        for (var h = 1; h <= 8; h++) {
            this.connections.push(engine.makeConnection(deck, "hotcue_" + h + "_status", relight));
            this.connections.push(engine.makeConnection(deck, "slicer_" + h + "_status", relight));
        }
        for (var s = 1; s <= 4; s++) {
            var stem = "[Channel" + d + "_Stem" + s + "]";
            this.connections.push(engine.makeConnection(stem, "mute", relight));
            this.connections.push(engine.makeConnection("[QuickEffectRack1_" + stem + "]", "enabled", relight));
        }
        this.connections.push(engine.makeConnection(deck, "playposition", function() { self.lightStrip(); }));
    }
    for (var u = 1; u <= 4; u++) {
        var unit = "[EffectRack1_EffectUnit" + u + "]";
        this.connections.push(engine.makeConnection(unit, "enabled", relight));
        for (var e = 1; e <= 3; e++) {
            this.connections.push(engine.makeConnection(unit.slice(0, -1) + "_Effect" + e + "]", "enabled", relight));
        }
        for (var c = 1; c <= 4; c++) {
            this.connections.push(engine.makeConnection(unit, "group_[Channel" + c + "]_enable", relight));
        }
    }
    for (var p = 1; p <= 8; p++) {
        this.connections.push(engine.makeConnection("[Sampler" + p + "]", "play", relight));
        this.connections.push(engine.makeConnection("[Sampler" + p + "]", "track_loaded", relight));
    }
};

TraktorD2.setPad = function(p, rgb) {
    // The pads take blue, green, red.
    this.leds[3 * p] = rgb[2];
    this.leds[3 * p + 1] = rgb[1];
    this.leds[3 * p + 2] = rgb[0];
};

// Pad colours as in Traktor: blue cues, green loops, white jumps, the slicer
// in white, samples in the remix deck's lilac.
TraktorD2.COLORS = {
    cue: [0x00, 0x30, 0x7F],
    loop: [0x00, 0x7F, 0x10],
    jump: [0x60, 0x60, 0x60],
    slice: [0x7F, 0x7F, 0x7F],
    sample: [0x50, 0x30, 0x7F],
};

TraktorD2.lightPads = function() {
    var deck = this.deck();
    var scale = function(rgb, lit) {
        return rgb.map(function(c) { return lit ? c : Math.round(c / 10); });
    };
    for (var p = 0; p < 8; p++) {
        var rgb = [0, 0, 0];
        switch (this.mode) {
        case this.HOTCUE:
            rgb = engine.getValue(deck, "hotcue_" + (p + 1) + "_status") > 0 ? this.COLORS.cue : [0, 0, 0];
            break;
        case this.LOOP:
            if (p < 4) {
                var on = engine.getValue(deck, "loop_enabled")
                    && engine.getValue(deck, "beatloop_size") === parseFloat(this.LOOP_SIZES[p]);
                rgb = scale(this.COLORS.loop, on);
            } else {
                rgb = scale(this.COLORS.jump, p === 4 || p === 7);
            }
            break;
        case this.FREEZE:
            rgb = scale(this.COLORS.slice, engine.getValue(deck, "slicer_" + (p + 1) + "_status") > 0);
            break;
        case this.REMIX:
            var sampler = "[Sampler" + (p + 1) + "]";
            rgb = engine.getValue(sampler, "track_loaded")
                ? scale(this.COLORS.sample, engine.getValue(sampler, "play")) : [0, 0, 0];
            break;
        }
        this.setPad(p, rgb);
    }
};

TraktorD2.lightAll = function() {
    var self = this;
    var on = function(v) { return v ? self.ON : self.DIM; };
    var deck = this.deck();
    var top = this.unit(false);
    var lower = this.unit(true);
    this.lightPads();
    this.leds[24] = this.DIM;
    for (var n = 0; n < 4; n++) {
        this.leds[25 + n] = on(engine.getValue(n === 0 ? top : top.slice(0, -1) + "_Effect" + n + "]", "enabled"));
        var lit;
        if (this.hasStems()) {
            var stem = "[Channel" + this.deckNumber() + "_Stem" + (n + 1) + "]";
            lit = this.shift ? !engine.getValue(stem, "mute")
                : engine.getValue("[QuickEffectRack1_" + stem + "]", "enabled");
        } else {
            lit = engine.getValue(n === 0 ? lower : lower.slice(0, -1) + "_Effect" + n + "]", "enabled");
        }
        this.leds[40 + n] = on(lit);
        // Pad modes: the chosen one in the deck's colour, blue on A or B,
        // white on C or D, as in Traktor.
        var chosen = this.mode === n;
        this.leds[44 + 2 * n] = chosen && this.bottom ? this.ON : (chosen ? 0 : this.DIM);
        this.leds[45 + 2 * n] = chosen && !this.bottom ? this.ON : 0;
        this.leds[118 + n] = on(engine.getValue(this.shift ? lower : top, "group_[Channel" + (n + 1) + "]_enable"));
    }
    this.leds[52] = on(engine.getValue(deck, "slip_enabled"));
    this.leds[53] = this.bottom ? this.ON : 0;
    this.leds[54] = this.bottom ? 0 : this.ON;
    this.leds[55] = on(this.shift);
    this.leds[56] = on(engine.getValue(deck, "sync_enabled"));
    this.leds[57] = 0;
    this.leds[58] = on(engine.getValue(deck, "cue_indicator"));
    this.leds[59] = on(engine.getValue(deck, "play_indicator"));
    var looping = engine.getValue(deck, "loop_enabled") > 0;
    for (var r = 0; r < 4; r++) {
        this.leds[60 + r] = looping ? 0 : this.DIM;
        this.leds[64 + r] = looping ? this.ON : 0;
    }
    this.stripLit = -1;
    this.lightStrip();
};

// The touch strip shows how far the track has played: blue on A or B,
// orange on C or D.
TraktorD2.lightStrip = function() {
    var played = Math.round(engine.getValue(this.deck(), "playposition") * 25);
    played = Math.max(0, Math.min(25, played));
    if (played === this.stripLit) {
        return;
    }
    this.stripLit = played;
    for (var i = 0; i < 25; i++) {
        var lit = i < played ? this.ON : 0;
        this.leds[68 + i] = this.bottom ? 0 : lit;
        this.leds[93 + i] = this.bottom ? lit : 0;
    }
    this.send();
};
