// One deck section of the Traktor Kontrol D2 or S8, played as Traktor plays
// it: one deck at a time (A or C, or B or D) with an effect unit on top, the
// performance knobs, faders and ON buttons for stems and the second effect
// unit, eight pads in HOTCUE, LOOP, FREEZE (slicer) and REMIX (sample)
// modes, a touch strip and transport. openDJ's own code, shared by
// Traktor-Kontrol-D2.js and Traktor-Kontrol-S8.js, which say where each
// control sits in their reports.
//
// The layout handed to `new TraktorKontrolDeck(layout)`:
//   bd          true for the section that plays decks B and D
//   buttons     [byte, mask] in the buttons report for shift, deck, flux,
//               play, cue, sync, hotcue, loop, freeze, remix, browsePush,
//               loopPush (null when unknown); fx, on (4 each), pads (8),
//               assign (A-D, or null)
//   encoders    byte with browse (high nibble) and loop (low nibble), or null
//   strip       first byte of the touch strip (16 bits, 0 untouched, 1-1024)
//   knobs       first byte of the endless performance knobs (16-bit
//               counters), or null without them
//   knobTurn    counts in a turn; knobWrap, where the counter wraps
//   faders, fx  first bytes of the performance faders (or null) and FX
//               knobs (12 bits, 0-4078); buttons.on is null without faders
//   ledReport   the LED report's ID, ledLength its length
//   bgr         true when the pads take blue, green, red
//   deckLeds    offset of the A-D lights, or null
// LED offsets (shared by both): 0-23 pads; 24 FX SELECT; 25-28 FX 1-4;
// 40-43 ON 1-4; 44-51 HOTCUE, LOOP, FREEZE, REMIX (white, blue); 52 FLUX;
// 53-54 DECK (white, blue); 55 SHIFT; 56-57 SYNC (green, red); 58 CUE;
// 59 PLAY; 60-67 loop encoder ring (white x4, blue x4); 68-92 touch strip
// blue, 93-117 orange.

var TraktorKontrolDeck = function(layout) {
    this.layout = layout;
    this.bd = layout.bd;
    this.bottom = false;   // C or D in focus instead of A or B
    this.mode = this.HOTCUE;
    this.shift = false;
    this.deckHeld = false;
    this.deckAssigned = false;
    this.strip = 0;
    this.stripLit = -1;
    this.leds = new Uint8Array(layout.ledLength);
};

TraktorKontrolDeck.prototype.HOTCUE = 0;
TraktorKontrolDeck.prototype.LOOP = 1;
TraktorKontrolDeck.prototype.FREEZE = 2;
TraktorKontrolDeck.prototype.REMIX = 3;
TraktorKontrolDeck.prototype.ON = 0x7F;
TraktorKontrolDeck.prototype.DIM = 0x0A;

// LOOP pads as in Traktor: loops of 1/8, 1/4, 1/2 and 1 beat on top; below,
// jump back by the loop, back 1 beat, forward 1 beat, forward by the loop.
TraktorKontrolDeck.prototype.LOOP_SIZES = ["0.125", "0.25", "0.5", "1"];

// Pad colours as in Traktor: blue cues, green loops, white jumps, the slicer
// in white, samples in the remix deck's lilac.
TraktorKontrolDeck.prototype.COLORS = {
    cue: [0x00, 0x30, 0x7F],
    loop: [0x00, 0x7F, 0x10],
    jump: [0x60, 0x60, 0x60],
    slice: [0x7F, 0x7F, 0x7F],
    sample: [0x50, 0x30, 0x7F],
};

TraktorKontrolDeck.softTakeover = function() {
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
};

TraktorKontrolDeck.prototype.deckNumber = function() {
    return 1 + (this.bd ? 1 : 0) + (this.bottom ? 2 : 0);
};

TraktorKontrolDeck.prototype.deck = function() {
    return "[Channel" + this.deckNumber() + "]";
};

// The effect unit on top: 1 with decks A and C, 2 with B and D. The
// performance knobs play unit 3 or 4, as in Traktor with four FX units.
TraktorKontrolDeck.prototype.unit = function(lower) {
    return "[EffectRack1_EffectUnit" + (1 + (this.bd ? 1 : 0) + (lower ? 2 : 0)) + "]";
};

TraktorKontrolDeck.prototype.hasStems = function() {
    return engine.getValue(this.deck(), "stem_count") > 0;
};

// --- Buttons report ----------------------------------------------------------

TraktorKontrolDeck.prototype.buttons = function(data, last) {
    var self = this;
    var b = this.layout.buttons;
    var button = function(at, fn) {
        if (!at) {
            return;
        }
        var now = (data[at[0]] & at[1]) !== 0;
        if (now !== ((last[at[0]] & at[1]) !== 0)) {
            fn.call(self, now);
        }
    };

    button(b.shift, function(down) {
        this.shift = down;
        this.lightAll();
    });
    button(b.deck, this.deckButton);
    for (var n = 0; n < 4; n++) {
        var i = n;
        if (b.assign) {
            button(b.assign[n], function(down) { this.assign(i, down); });
        }
        button(b.fx[n], function(down) {
            if (down) {
                this.unitButton(this.unit(false), i);
            }
        });
        if (b.on) {
            button(b.on[n], function(down) { this.onButton(i, down); });
        }
        button([b.hotcue, b.loop, b.freeze, b.remix][n], function(down) {
            if (down) {
                this.mode = i;
                this.lightAll();
            }
        });
    }
    b.pads.forEach(function(pad, p) {
        button(pad, function(down) { this.pad(p, down); });
    });
    button(b.flux, function(down) {
        if (down) {
            script.toggleControl(this.deck(), "slip_enabled");
        }
    });
    button(b.play, function(down) {
        if (down) {
            script.toggleControl(this.deck(), "play");
        }
    });
    button(b.cue, function(down) {
        engine.setValue(this.deck(), this.shift ? "start" : "cue_default", down ? 1 : 0);
    });
    button(b.sync, function(down) {
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
    button(b.browsePush, function(down) {
        if (down) {
            script.triggerControl(this.shift ? "[Library]" : this.deck(),
                this.shift ? "GoToItem" : "LoadSelectedTrack");
        }
    });
    button(b.loopPush, function(down) {
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

    var e = this.layout.encoders;
    if (e !== null && e !== undefined) {
        var browse = this.step(last[e] >> 4, data[e] >> 4);
        if (browse !== 0) {
            engine.setValue("[Library]", "focused_widget", this.shift ? 2 : 3);
            engine.setValue("[Library]", "MoveVertical", browse);
        }
        var loop = this.step(last[e] & 0x0F, data[e] & 0x0F);
        if (loop !== 0) {
            this.loopTurn(loop);
        }
    }
    var s = this.layout.strip;
    this.touchStrip((data[s] | (data[s + 1] << 8)) & 0x7FF);
};

// One step of a 4-bit encoder that wraps from 15 to 0.
TraktorKontrolDeck.prototype.step = function(before, now) {
    if (before === now) {
        return 0;
    }
    var d = (now - before + 16) % 16;
    return d < 8 ? d : d - 16;
};

// --- Knobs and faders report -------------------------------------------------

TraktorKontrolDeck.prototype.sliders = function(data, last) {
    var word = function(bytes, at) {
        return bytes[at] | (bytes[at + 1] << 8);
    };
    var l = this.layout;
    for (var n = 0; n < 4; n++) {
        var at;
        if (l.knobs !== null) {
            // Performance knobs: endless counters.
            at = l.knobs + 2 * n;
            var turned = word(data, at) - word(last, at);
            if (turned > l.knobWrap / 2) {
                turned -= l.knobWrap;
            } else if (turned < -l.knobWrap / 2) {
                turned += l.knobWrap;
            }
            if (turned !== 0) {
                this.performanceKnob(n, turned / l.knobTurn);
            }
        }
        if (l.faders !== null) {
            at = l.faders + 2 * n;
            var fader = word(data, at) & 0x0FFF;
            if (fader !== (word(last, at) & 0x0FFF)) {
                this.fader(n, Math.min(fader / 4078, 1));
            }
        }
        at = l.fx + 2 * n;
        var fx = word(data, at) & 0x0FFF;
        if (fx !== (word(last, at) & 0x0FFF)) {
            this.unitKnob(this.unit(false), n, Math.min(fx / 4078, 1));
        }
    }
};

// --- Decks -------------------------------------------------------------------

// DECK switches between the two decks; held, A, B, C or D picks the deck.
TraktorKontrolDeck.prototype.deckButton = function(down) {
    this.deckHeld = down;
    if (down) {
        this.deckAssigned = false;
        return;
    }
    if (!this.deckAssigned) {
        this.bottom = !this.bottom;
    }
    this.lightAll();
};

// A-D send the decks to the effect unit on top (SHIFT: the second unit).
TraktorKontrolDeck.prototype.assign = function(n, down) {
    if (!down) {
        return;
    }
    if (this.deckHeld) {
        this.bd = n % 2 === 1;
        this.bottom = n >= 2;
        this.deckAssigned = true;
        this.lightAll();
        return;
    }
    script.toggleControl(this.unit(this.shift), "group_[Channel" + (n + 1) + "]_enable");
};

// --- Effects -----------------------------------------------------------------

TraktorKontrolDeck.prototype.unitKnob = function(unit, n, value) {
    if (n === 0) {
        engine.setParameter(unit, "mix", value);
    } else {
        engine.setParameter(unit.slice(0, -1) + "_Effect" + n + "]", "meta", value);
    }
};

TraktorKontrolDeck.prototype.unitButton = function(unit, n) {
    if (n === 0) {
        script.toggleControl(unit, "enabled");
    } else if (this.shift) {
        script.triggerControl(unit.slice(0, -1) + "_Effect" + n + "]", "next_effect");
    } else {
        script.toggleControl(unit.slice(0, -1) + "_Effect" + n + "]", "enabled");
    }
};

// --- Performance section: stems, or the second effect unit -------------------

TraktorKontrolDeck.prototype.performanceKnob = function(n, turned) {
    var unit = this.unit(true);
    var group = n === 0 ? unit : unit.slice(0, -1) + "_Effect" + n + "]";
    var key = n === 0 ? "mix" : "meta";
    var value = Math.max(0, Math.min(1, engine.getParameter(group, key) + turned));
    engine.setParameter(group, key, value);
};

// ON: with stems, each stem's FX send (SHIFT mutes it); otherwise the
// second effect unit's buttons.
TraktorKontrolDeck.prototype.onButton = function(n, down) {
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
TraktorKontrolDeck.prototype.fader = function(n, value) {
    if (this.hasStems()) {
        engine.setValue("[Channel" + this.deckNumber() + "_Stem" + (n + 1) + "]", "volume", value);
    }
};

// --- Pads --------------------------------------------------------------------

TraktorKontrolDeck.prototype.pad = function(p, down) {
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
        script.triggerControl(sampler, this.shift ? "stop" : "cue_gotoandplay");
        break;
    }
};

// --- Loop and touch strip ----------------------------------------------------

TraktorKontrolDeck.prototype.loopTurn = function(steps) {
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
TraktorKontrolDeck.prototype.touchStrip = function(position) {
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

TraktorKontrolDeck.prototype.send = function() {
    controller.sendOutputReport(this.layout.ledReport, this.leds.buffer);
};

// Calls relight when anything a light shows changes, and strip for the
// play position.
TraktorKontrolDeck.connect = function(connections, relight, strip) {
    var keys = ["play_indicator", "cue_indicator", "sync_enabled", "slip_enabled", "loop_enabled",
        "slicer_enabled", "stem_count", "track_loaded"];
    for (var d = 1; d <= 4; d++) {
        var deck = "[Channel" + d + "]";
        keys.forEach(function(key) {
            connections.push(engine.makeConnection(deck, key, relight));
        });
        for (var h = 1; h <= 8; h++) {
            connections.push(engine.makeConnection(deck, "hotcue_" + h + "_status", relight));
            connections.push(engine.makeConnection(deck, "slicer_" + h + "_status", relight));
        }
        for (var s = 1; s <= 4; s++) {
            var stem = "[Channel" + d + "_Stem" + s + "]";
            connections.push(engine.makeConnection(stem, "mute", relight));
            connections.push(engine.makeConnection("[QuickEffectRack1_" + stem + "]", "enabled", relight));
        }
        connections.push(engine.makeConnection(deck, "playposition", strip));
    }
    for (var u = 1; u <= 4; u++) {
        var unit = "[EffectRack1_EffectUnit" + u + "]";
        connections.push(engine.makeConnection(unit, "enabled", relight));
        for (var e = 1; e <= 3; e++) {
            connections.push(engine.makeConnection(unit.slice(0, -1) + "_Effect" + e + "]", "enabled", relight));
        }
        for (var c = 1; c <= 4; c++) {
            connections.push(engine.makeConnection(unit, "group_[Channel" + c + "]_enable", relight));
        }
    }
    for (var p = 1; p <= 8; p++) {
        connections.push(engine.makeConnection("[Sampler" + p + "]", "play", relight));
        connections.push(engine.makeConnection("[Sampler" + p + "]", "track_loaded", relight));
    }
};

TraktorKontrolDeck.prototype.setPad = function(p, rgb) {
    var order = this.layout.bgr ? [2, 1, 0] : [0, 1, 2];
    for (var c = 0; c < 3; c++) {
        this.leds[3 * p + c] = rgb[order[c]];
    }
};

TraktorKontrolDeck.prototype.lightPads = function() {
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

TraktorKontrolDeck.prototype.lightAll = function() {
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
        if (this.layout.deckLeds !== null) {
            this.leds[this.layout.deckLeds + n] =
                on(engine.getValue(this.shift ? lower : top, "group_[Channel" + (n + 1) + "]_enable"));
        }
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
TraktorKontrolDeck.prototype.lightStrip = function() {
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
