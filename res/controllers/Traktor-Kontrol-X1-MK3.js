// Traktor Kontrol X1 MK3, played as Traktor plays it: two effect units (or
// the mixer) on top, browse and loop encoders, four hot cues and transport
// per deck. openDJ's own mapping, written from the X1 MK3's published HID
// report layout (input report 0x01, LED report 0x80) and Native
// Instruments' manual. The five displays stay dark.
//
// Input report 0x01 (byte 0 is the ID):
//   byte 1  SHIFT 0x01, MODE 0x02, browse push L 0x04 / R 0x10,
//           loop push L 0x08 / R 0x20
//   byte 2  FX buttons, left 1-4 0x01-0x08, right 1-4 0x10-0x80 (top first)
//   byte 3  hot cues, left 1-4 0x01-0x08, right 1-4 0x10-0x80
//   byte 4  PLAY, CUE, REV, SYNC: left 0x01-0x08, right 0x10-0x80
//   byte 5  assign ← →, nudge ← →: left 0x01-0x08, right 0x10-0x80
//   byte 7  left browse (high nibble) and loop (low nibble), 0-15 wrapping
//   byte 8  right browse and loop
//   bytes 9-24  knobs, 12 bits little-endian: left 1-4 then right 1-4
// LED report 0x80, 48 bytes after the ID (offsets below count from 1):
//   1 SHIFT, 2 MODE, 3-6 left FX, 7-10 right FX, 11-22 left hot cues (RGB),
//   23-34 right hot cues (RGB), 35-38 left PLAY CUE REV SYNC, 39-42 right,
//   43-45 left underglow (RGB), 46-48 right underglow (RGB).

var TraktorX1MK3 = {};

// Modes of the top section, stepped by MODE.
TraktorX1MK3.MODE_FX12 = 0;
TraktorX1MK3.MODE_FX34 = 1;
TraktorX1MK3.MODE_MIXER = 2;

TraktorX1MK3.ON = 0x7F;
TraktorX1MK3.DIM = 0x0A;

TraktorX1MK3.init = function() {
    this.mode = this.MODE_FX12;
    this.shift = false;
    this.syncHeld = [false, false];
    this.loopPushed = [false, false];
    this.last = null;
    this.leds = new Uint8Array(48);
    this.connections = [];
    // Knobs pick up where the software is, as the X1 MK3 does in Traktor.
    for (var u = 1; u <= 4; u++) {
        engine.softTakeover("[EffectRack1_EffectUnit" + u + "]", "mix", true);
        for (var e = 1; e <= 3; e++) {
            engine.softTakeover("[EffectRack1_EffectUnit" + u + "_Effect" + e + "]", "meta", true);
        }
    }
    for (var d = 1; d <= 2; d++) {
        var eq = "[EqualizerRack1_[Channel" + d + "]_Effect1]";
        for (var p = 1; p <= 3; p++) {
            engine.softTakeover(eq, "parameter" + p, true);
        }
        engine.softTakeover("[Channel" + d + "]", "volume", true);
    }
    this.connectLights();
    this.lightAll();
};

TraktorX1MK3.shutdown = function() {
    this.leds = new Uint8Array(48);
    this.send();
};

TraktorX1MK3.deck = function(side) {
    return "[Channel" + (side + 1) + "]";
};

TraktorX1MK3.unit = function(side) {
    var n = side + 1 + (this.mode === this.MODE_FX34 ? 2 : 0);
    return "[EffectRack1_EffectUnit" + n + "]";
};

TraktorX1MK3.incomingData = function(data) {
    if (data[0] !== 0x01 || data.length < 25) {
        return;
    }
    var last = this.last;
    this.last = Array.prototype.slice.call(data, 0, 25);
    if (last === null) {
        return;
    }
    var bit = function(bytes, b, mask) {
        return (bytes[b] & mask) !== 0;
    };
    var changed = function(b, mask) {
        return bit(data, b, mask) !== bit(last, b, mask);
    };
    var self = this;
    var button = function(b, mask, fn) {
        if (changed(b, mask)) {
            fn.call(self, bit(data, b, mask));
        }
    };

    button(1, 0x01, function(down) {
        this.shift = down;
        this.setLed(1, down ? this.ON : this.DIM);
        this.send();
    });
    button(1, 0x02, function(down) {
        if (down) {
            this.mode = (this.mode + 1) % 3;
            this.lightAll();
        }
    });
    for (var side = 0; side < 2; side++) {
        var s = side;
        button(1, side ? 0x10 : 0x04, function(down) { this.browsePush(s, down); });
        button(1, side ? 0x20 : 0x08, function(down) { this.loopPush(s, down); });
        for (var i = 0; i < 4; i++) {
            var n = i;
            button(2, 1 << (i + 4 * side), function(down) { this.fxButton(s, n, down); });
            button(3, 1 << (i + 4 * side), function(down) { this.hotcue(s, n, down); });
        }
        button(4, side ? 0x10 : 0x01, function(down) { this.play(s, down); });
        button(4, side ? 0x20 : 0x02, function(down) { this.cue(s, down); });
        button(4, side ? 0x40 : 0x04, function(down) { this.rev(s, down); });
        button(4, side ? 0x80 : 0x08, function(down) { this.sync(s, down); });
        button(5, side ? 0x10 : 0x01, function(down) { this.assign(s, 0, down); });
        button(5, side ? 0x20 : 0x02, function(down) { this.assign(s, 1, down); });
        button(5, side ? 0x40 : 0x04, function(down) { this.nudge(s, -1, down); });
        button(5, side ? 0x80 : 0x08, function(down) { this.nudge(s, 1, down); });

        var browse = this.step(last[7 + side] >> 4, data[7 + side] >> 4);
        if (browse !== 0) {
            this.browseTurn(side, browse);
        }
        var loop = this.step(last[7 + side] & 0x0F, data[7 + side] & 0x0F);
        if (loop !== 0) {
            this.loopTurn(side, loop);
        }
        for (var k = 0; k < 4; k++) {
            var at = 9 + 2 * (4 * side + k);
            var value = (data[at] | (data[at + 1] << 8)) & 0x0FFF;
            var before = (last[at] | (last[at + 1] << 8)) & 0x0FFF;
            if (value !== before) {
                this.knob(side, k, value / 4095);
            }
        }
    }
};

// One step of a 4-bit encoder that wraps from 15 to 0.
TraktorX1MK3.step = function(before, now) {
    if (before === now) {
        return 0;
    }
    var d = (now - before + 16) % 16;
    return d < 8 ? d : d - 16;
};

// --- Top section: effect units, or the mixer -------------------------------

TraktorX1MK3.knob = function(side, k, value) {
    if (this.mode === this.MODE_MIXER) {
        var deck = this.deck(side);
        if (k === 3) {
            engine.setParameter(deck, "volume", value);
        } else {
            engine.setParameter("[EqualizerRack1_" + deck + "_Effect1]", "parameter" + (3 - k), value);
        }
        return;
    }
    var unit = this.unit(side);
    if (k === 0) {
        engine.setParameter(unit, "mix", value);
    } else {
        engine.setParameter(unit.slice(0, -1) + "_Effect" + k + "]", "meta", value);
    }
};

TraktorX1MK3.fxButton = function(side, n, down) {
    if (!down) {
        return;
    }
    if (this.mode === this.MODE_MIXER) {
        var deck = this.deck(side);
        if (n === 3) {
            script.toggleControl(deck, "pfl");
        } else {
            script.toggleControl("[EqualizerRack1_" + deck + "_Effect1]", "button_parameter" + (3 - n));
        }
        return;
    }
    var unit = this.unit(side);
    if (n === 0) {
        script.toggleControl(unit, "enabled");
    } else if (this.shift) {
        script.triggerControl(unit.slice(0, -1) + "_Effect" + n + "]", "next_effect");
    } else {
        script.toggleControl(unit.slice(0, -1) + "_Effect" + n + "]", "enabled");
    }
};

// The arrows over each effect unit send it to the left or right deck.
TraktorX1MK3.assign = function(side, to, down) {
    if (!down || this.mode === this.MODE_MIXER) {
        return;
    }
    var n = side + 1 + (this.shift || this.mode === this.MODE_FX34 ? 2 : 0);
    script.toggleControl("[EffectRack1_EffectUnit" + n + "]", "group_" + this.deck(to) + "_enable");
};

// --- Browse and loop -------------------------------------------------------

TraktorX1MK3.browseTurn = function(side, steps) {
    engine.setValue("[Library]", "focused_widget", this.shift ? 2 : 3);
    engine.setValue("[Library]", "MoveVertical", steps);
};

TraktorX1MK3.browsePush = function(side, down) {
    if (!down) {
        return;
    }
    if (this.shift) {
        script.triggerControl("[Library]", "GoToItem");
    } else {
        script.triggerControl(this.deck(side), "LoadSelectedTrack");
    }
};

TraktorX1MK3.loopTurn = function(side, steps) {
    var deck = this.deck(side);
    if (this.syncHeld[side]) {
        // Hold SYNC and turn: tempo, in bigger steps with the encoder pushed.
        var key = steps > 0 ? "rate_perm_up" : "rate_perm_down";
        script.triggerControl(deck, this.loopPushed[side] ? key : key + "_small");
        return;
    }
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

TraktorX1MK3.loopPush = function(side, down) {
    this.loopPushed[side] = down;
    if (!down || this.syncHeld[side]) {
        return;
    }
    var deck = this.deck(side);
    if (engine.getValue(deck, "loop_enabled")) {
        script.triggerControl(deck, "reloop_toggle");
    } else {
        script.triggerControl(deck, "beatloop_activate");
    }
};

// --- Hot cues and transport --------------------------------------------------

TraktorX1MK3.hotcue = function(side, n, down) {
    var key = "hotcue_" + (n + 1) + (this.shift ? "_clear" : "_activate");
    engine.setValue(this.deck(side), key, down ? 1 : 0);
};

TraktorX1MK3.play = function(side, down) {
    if (down) {
        script.toggleControl(this.deck(side), "play");
    }
};

TraktorX1MK3.cue = function(side, down) {
    engine.setValue(this.deck(side), this.shift ? "start" : "cue_default", down ? 1 : 0);
};

// REV plays backwards while held, in flux; SHIFT + REV turns flux on or off.
TraktorX1MK3.rev = function(side, down) {
    var deck = this.deck(side);
    if (this.shift) {
        if (down) {
            script.toggleControl(deck, "slip_enabled");
        }
        return;
    }
    engine.setValue(deck, "reverseroll", down ? 1 : 0);
};

TraktorX1MK3.sync = function(side, down) {
    this.syncHeld[side] = down;
    if (!down) {
        return;
    }
    var deck = this.deck(side);
    if (this.shift) {
        var leader = engine.getValue(deck, "sync_leader") > 0;
        engine.setValue(deck, "sync_leader", leader ? 0 : 1);
    } else {
        script.toggleControl(deck, "sync_enabled");
    }
};

TraktorX1MK3.nudge = function(side, dir, down) {
    var deck = this.deck(side);
    if (this.shift) {
        if (down) {
            script.triggerControl(deck, dir > 0 ? "beatjump_forward" : "beatjump_backward");
        }
        return;
    }
    engine.setValue(deck, dir > 0 ? "rate_temp_up" : "rate_temp_down", down ? 1 : 0);
};

// --- Lights ------------------------------------------------------------------

TraktorX1MK3.setLed = function(offset, value) {
    this.leds[offset - 1] = value;
};

TraktorX1MK3.setRgb = function(offset, rgb) {
    this.leds[offset - 1] = rgb[0];
    this.leds[offset] = rgb[1];
    this.leds[offset + 1] = rgb[2];
};

TraktorX1MK3.send = function() {
    controller.sendOutputReport(0x80, this.leds.buffer);
};

TraktorX1MK3.connectLights = function() {
    var self = this;
    var relight = function() { self.lightAll(); };
    var keys = ["play_indicator", "cue_indicator", "reverseroll", "slip_enabled", "sync_enabled",
        "sync_leader", "loop_enabled", "pfl", "track_loaded"];
    for (var d = 1; d <= 2; d++) {
        var deck = "[Channel" + d + "]";
        keys.forEach(function(key) {
            self.connections.push(engine.makeConnection(deck, key, relight));
        });
        for (var h = 1; h <= 4; h++) {
            this.connections.push(engine.makeConnection(deck, "hotcue_" + h + "_status", relight));
        }
        for (var p = 1; p <= 3; p++) {
            this.connections.push(engine.makeConnection("[EqualizerRack1_" + deck + "_Effect1]",
                "button_parameter" + p, relight));
        }
    }
    for (var u = 1; u <= 4; u++) {
        var unit = "[EffectRack1_EffectUnit" + u + "]";
        this.connections.push(engine.makeConnection(unit, "enabled", relight));
        for (var e = 1; e <= 3; e++) {
            this.connections.push(engine.makeConnection(unit.slice(0, -1) + "_Effect" + e + "]", "enabled", relight));
        }
    }
};

// Hot cues light blue on deck A, orange on deck B, as in Traktor.
TraktorX1MK3.DECK_COLORS = [[0x00, 0x40, 0x7F], [0x7F, 0x40, 0x00]];

TraktorX1MK3.lightAll = function() {
    var on = function(v, self) { return v ? self.ON : self.DIM; };
    this.setLed(1, on(this.shift, this));
    this.setLed(2, [this.DIM, 0x40, this.ON][this.mode]);
    for (var side = 0; side < 2; side++) {
        var deck = this.deck(side);
        for (var n = 0; n < 4; n++) {
            var lit;
            if (this.mode === this.MODE_MIXER) {
                lit = n === 3 ? engine.getValue(deck, "pfl")
                    : engine.getValue("[EqualizerRack1_" + deck + "_Effect1]", "button_parameter" + (3 - n));
            } else {
                var unit = this.unit(side);
                lit = n === 0 ? engine.getValue(unit, "enabled")
                    : engine.getValue(unit.slice(0, -1) + "_Effect" + n + "]", "enabled");
            }
            this.setLed(3 + 4 * side + n, on(lit, this));
            var color = this.DECK_COLORS[side];
            var set = engine.getValue(deck, "hotcue_" + (n + 1) + "_status") > 0;
            var scale = set ? 1 : 0.1;
            this.setRgb(11 + 12 * side + 3 * n, color.map(function(c) { return Math.round(c * scale); }));
        }
        var t = 35 + 4 * side;
        this.setLed(t, on(engine.getValue(deck, "play_indicator"), this));
        this.setLed(t + 1, on(engine.getValue(deck, "cue_indicator"), this));
        this.setLed(t + 2, on(engine.getValue(deck, this.shift ? "slip_enabled" : "reverseroll"), this));
        this.setLed(t + 3, on(engine.getValue(deck, "sync_enabled"), this));
        // Underglow: green while a loop plays.
        this.setRgb(43 + 3 * side, engine.getValue(deck, "loop_enabled") ? [0x00, 0x7F, 0x00] : [0, 0, 0]);
    }
    this.send();
};
