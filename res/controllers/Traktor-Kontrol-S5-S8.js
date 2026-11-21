// The Traktor Kontrol S5 and S8: two deck sections (see
// Traktor-Kontrol-Deck-Section.js), the left on decks A and C with effect
// unit 1, the right on B and D with unit 2, either side of a four-channel
// mixer whose channels run C, A, B, D. openDJ's own code, shared by
// Traktor-Kontrol-S5.js and Traktor-Kontrol-S8.js, which say where each
// control sits in their reports.
//
// The layout handed to `new TraktorKontrolMixerController(layout)`:
//   buttonsLength, slidersLength  the two input reports' lengths, with the ID
//   sections    the two deck sections' layouts
//   channels    A-D: [byte, mask] of cue, filter (FILTER ON), fx (FX 1 and 2);
//               xf, the crossfader assign masks (left, right) in byte xfByte,
//               or null; knobs, the bytes of gain, high, mid, low and filter
//               (null where unknown); fader; fxLeds, cueLed and filterLed,
//               offsets in the mixer LED report (cueLed and filterLed null
//               when those lights go in feature report 0xF4)
//   crossfader  its byte; quantize and mic, [byte, mask] or null
//   mixerLeds   the mixer LED report's length; snapLed and quantLed offsets

var TraktorKontrolMixerController = function(layout) {
    this.layout = layout;
};

TraktorKontrolMixerController.prototype.ON = 0x7F;
TraktorKontrolMixerController.prototype.DIM = 0x0A;

TraktorKontrolMixerController.prototype.init = function() {
    var l = this.layout;
    this.sections = [new TraktorKontrolDeck(l.sections[0]), new TraktorKontrolDeck(l.sections[1])];
    this.buttons = null;
    this.knobs = null;
    this.mixerLeds = new Uint8Array(l.mixerLeds);
    this.sentCue = -1;
    this.sentFilter = -1;
    this.lastShift = false;
    this.connections = [];
    TraktorKontrolDeck.softTakeover();
    for (var d = 1; d <= 4; d++) {
        var deck = "[Channel" + d + "]";
        var eq = "[EqualizerRack1_" + deck + "_Effect1]";
        engine.softTakeover(deck, "pregain", true);
        engine.softTakeover(deck, "volume", true);
        engine.softTakeover("[QuickEffectRack1_" + deck + "]", "super1", true);
        for (var p = 1; p <= 3; p++) {
            engine.softTakeover(eq, "parameter" + p, true);
        }
    }
    var self = this;
    var mixer = function() { self.lightMixer(); };
    TraktorKontrolDeck.connect(this.connections,
        function() { self.lightAll(); },
        function() { self.sections.forEach(function(s) { s.lightStrip(); }); });
    for (var c = 1; c <= 4; c++) {
        var ch = "[Channel" + c + "]";
        this.connections.push(engine.makeConnection(ch, "pfl", mixer));
        this.connections.push(engine.makeConnection(ch, "quantize", mixer));
        this.connections.push(engine.makeConnection("[QuickEffectRack1_" + ch + "]", "enabled", mixer));
    }
    this.lightAll();
};

TraktorKontrolMixerController.prototype.shutdown = function() {
    this.sections.forEach(function(s) {
        s.leds = new Uint8Array(s.layout.ledLength);
        s.send();
    });
    this.mixerLeds = new Uint8Array(this.layout.mixerLeds);
    controller.sendOutputReport(0x82, this.mixerLeds.buffer);
    if (this.featureLeds()) {
        this.sendFeature(0x26, 0);
        this.sendFeature(0x25, 0);
    }
};

TraktorKontrolMixerController.prototype.shift = function() {
    return this.sections[0].shift || this.sections[1].shift;
};

TraktorKontrolMixerController.prototype.featureLeds = function() {
    return this.layout.channels[0].cueLed === null;
};

TraktorKontrolMixerController.prototype.incomingData = function(data) {
    var l = this.layout;
    var last;
    if (data[0] === 0x01 && data.length >= l.buttonsLength) {
        last = this.buttons;
        this.buttons = Array.prototype.slice.call(data, 0, l.buttonsLength);
        if (last !== null) {
            this.sections[0].buttons(data, last);
            this.sections[1].buttons(data, last);
            this.mixerButtons(data, last);
        }
    } else if (data[0] === 0x02 && data.length >= l.slidersLength) {
        last = this.knobs;
        this.knobs = Array.prototype.slice.call(data, 0, l.slidersLength);
        if (last !== null) {
            this.sections[0].sliders(data, last);
            this.sections[1].sliders(data, last);
            this.mixerKnobs(data, last);
        }
    }
};

// --- Mixer -------------------------------------------------------------------

TraktorKontrolMixerController.prototype.mixerButtons = function(data, last) {
    var l = this.layout;
    var pressed = function(at) {
        return at !== null && (data[at[0]] & at[1]) !== 0 && (last[at[0]] & at[1]) === 0;
    };
    var shift = this.shift();
    for (var n = 0; n < 4; n++) {
        var deck = "[Channel" + (n + 1) + "]";
        var c = l.channels[n];
        if (pressed(c.cue)) {
            script.toggleControl(deck, "pfl");
        }
        if (pressed(c.filter)) {
            script.toggleControl("[QuickEffectRack1_" + deck + "]", "enabled");
        }
        // FX 1 and 2 send the channel to effect units 1 and 2 (SHIFT: 3 and 4).
        for (var f = 0; f < 2; f++) {
            if (pressed(c.fx[f])) {
                script.toggleControl("[EffectRack1_EffectUnit" + (f + 1 + (shift ? 2 : 0)) + "]",
                    "group_" + deck + "_enable");
            }
        }
        if (c.xf !== null) {
            var mask = c.xf[0] | c.xf[1];
            var xf = data[l.xfByte] & mask;
            if (xf !== (last[l.xfByte] & mask)) {
                engine.setValue(deck, "orientation", xf & c.xf[0] ? 0 : (xf & c.xf[1] ? 2 : 1));
            }
        }
    }
    if (pressed(l.quantize)) {
        var on = engine.getValue("[Channel1]", "quantize") ? 0 : 1;
        for (var d = 1; d <= 4; d++) {
            engine.setValue("[Channel" + d + "]", "quantize", on);
        }
    }
    if (pressed(l.mic)) {
        script.toggleControl("[Microphone]", "talkover");
    }
    // The mixer's FX lights follow SHIFT (units 3 and 4).
    if (shift !== this.lastShift) {
        this.lastShift = shift;
        this.lightMixer();
    }
};

TraktorKontrolMixerController.prototype.mixerKnobs = function(data, last) {
    var word = function(bytes, at) {
        return (bytes[at] | (bytes[at + 1] << 8)) & 0x0FFF;
    };
    var turn = function(at, group, key) {
        if (at !== null && word(data, at) !== word(last, at)) {
            engine.setParameter(group, key, Math.min(word(data, at) / 4078, 1));
        }
    };
    for (var n = 0; n < 4; n++) {
        var deck = "[Channel" + (n + 1) + "]";
        var eq = "[EqualizerRack1_" + deck + "_Effect1]";
        var c = this.layout.channels[n];
        turn(c.knobs[0], deck, "pregain");
        turn(c.knobs[1], eq, "parameter3");
        turn(c.knobs[2], eq, "parameter2");
        turn(c.knobs[3], eq, "parameter1");
        turn(c.knobs[4], "[QuickEffectRack1_" + deck + "]", "super1");
        turn(c.fader, deck, "volume");
    }
    turn(this.layout.crossfader, "[Master]", "crossfader");
};

// --- Lights ------------------------------------------------------------------

TraktorKontrolMixerController.prototype.sendFeature = function(command, mask) {
    var report = new Uint8Array(33);
    report[0] = command;
    report[1] = mask;
    controller.sendFeatureReport(0xF4, report.buffer);
};

TraktorKontrolMixerController.prototype.lightMixer = function() {
    var self = this;
    var l = this.layout;
    var on = function(v) { return v ? self.ON : self.DIM; };
    var shift = this.shift();
    var cue = 0;
    var filter = 0;
    for (var n = 0; n < 4; n++) {
        var deck = "[Channel" + (n + 1) + "]";
        var c = l.channels[n];
        for (var f = 0; f < 2; f++) {
            var unit = "[EffectRack1_EffectUnit" + (f + 1 + (shift ? 2 : 0)) + "]";
            this.mixerLeds[c.fxLeds[f]] = on(engine.getValue(unit, "group_" + deck + "_enable"));
        }
        var pfl = engine.getValue(deck, "pfl") > 0;
        var fx = engine.getValue("[QuickEffectRack1_" + deck + "]", "enabled") > 0;
        if (c.cueLed !== null) {
            this.mixerLeds[c.cueLed] = on(pfl);
            this.mixerLeds[c.filterLed] = on(fx);
        }
        cue |= pfl ? 1 << n : 0;
        filter |= fx ? 1 << n : 0;
    }
    this.mixerLeds[l.snapLed] = this.DIM;
    this.mixerLeds[l.quantLed] = on(engine.getValue("[Channel1]", "quantize"));
    controller.sendOutputReport(0x82, this.mixerLeds.buffer);
    if (!this.featureLeds()) {
        return;
    }
    if (cue !== this.sentCue) {
        this.sentCue = cue;
        this.sendFeature(0x26, cue);
    }
    if (filter !== this.sentFilter) {
        this.sentFilter = filter;
        this.sendFeature(0x25, filter);
    }
};

TraktorKontrolMixerController.prototype.lightAll = function() {
    this.sections.forEach(function(s) { s.lightAll(); });
    this.lightMixer();
};
