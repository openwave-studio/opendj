// Traktor Kontrol S8, played as Traktor plays it: two deck sections (see
// Traktor-Kontrol-Deck-Section.js), the left on decks A and C with effect
// unit 1, the right on B and D with unit 2, either side of a four-channel
// mixer. openDJ's own mapping, written from the S8's published HID report
// layout and Native Instruments' manual. Its displays stay dark.
//
// That published layout is incomplete, so this mapping is partial:
// - the browse and loop encoders' turns aren't in it, so they do nothing;
//   their pushes are where the D2 has them, one byte along (a guess);
// - low EQ on channels C and D is read from bytes 95-96 and 105-106,
//   following the other channels (the layout gives a 4-bit value there);
// - the deck section lights are predicted from the D2's, not tested;
// - SNAP, MIC 2, the master tempo encoder and the A-D deck assign buttons
//   do nothing yet.
//
// Input report 0x01, buttons (41 bytes with the ID), per deck section
// (left at the bytes below, right 8 bytes on):
//   byte 4  FX SELECT 0x40, FX 1 0x80, browse push 0x08 (guess)
//   byte 5  FX 4 0x01, FX 3 0x02, FX 2 0x04
//   byte 6  ON 4 0x02, ON 3 0x04, ON 2 0x08, ON 1 0x10
//   byte 7  pad 3 0x01, pad 4 0x02, loop push 0x10 (guess)
//   byte 8  pad 2 0x01, pad 8 0x02, DECK 0x04, pad 1 0x08, FLUX 0x10,
//           pad 5 0x20, pad 6 0x40, pad 7 0x80
//   byte 9  REMIX 0x01, FREEZE 0x02, LOOP 0x04, HOTCUE 0x08, PLAY 0x10,
//           CUE 0x20, SYNC 0x40, SHIFT 0x80
//   touch strips at bytes 28-29 and 34-35, 0 untouched, 1-1024
// Mixer buttons:
//   byte 20  FILTER ON C 0x01, A 0x02, CUE C 0x04, A 0x08
//   byte 22  MIC 1 0x04, FX C 1 0x08, C 2 0x10, A 1 0x20, A 2 0x40, SNAP 0x80
//   byte 23  FILTER ON B 0x08, D 0x10, CUE B 0x20, D 0x40
//   byte 24  crossfader assign: A left 0x20 / right 0x10, B 0x08 / 0x04,
//            C 0x80 / 0x40, D 0x02 / 0x01
//   byte 25  MIC 2 0x01, QUANT 0x08, FX B 1 0x10, B 2 0x20, D 1 0x40, D 2 0x80
// Input report 0x02, knobs and faders (109 bytes with the ID), 12 bits:
//   performance knobs (endless 16-bit counters) left 1-8, right 9-16;
//   performance faders left 17-24, right 25-32; channel faders A-D 33-40;
//   crossfader 41; FX knobs left 43-50, right 51-58; per channel A-D from
//   69, 79, 89, 99: gain, high, mid, low, filter.
// LED reports: 0x80 left and 0x81 right deck section (118 bytes each, the
// D2's layout without A-D), 0x82 mixer (73 bytes: FX assign C1 44, C2 45,
// A1 46, A2 47, SNAP 48, QUANT 49, B1 50, B2 51, D1 52, D2 53). CUE and
// FILTER ON lights go in feature report 0xF4: [0x26, mask] and [0x25, mask],
// A 0x01, B 0x02, C 0x04, D 0x08.

var TraktorS8 = {};

TraktorS8.ON = 0x7F;
TraktorS8.DIM = 0x0A;

// Mixer channels A-D: [byte, mask] of CUE, FILTER ON, FX 1 and FX 2, the
// crossfader assign left and right, the mixer lights of FX 1 and 2.
TraktorS8.CHANNELS = [
    {cue: [20, 0x08], filter: [20, 0x02], fx: [[22, 0x20], [22, 0x40]], xf: [0x20, 0x10], leds: [46, 47]},
    {cue: [23, 0x20], filter: [23, 0x08], fx: [[25, 0x10], [25, 0x20]], xf: [0x08, 0x04], leds: [50, 51]},
    {cue: [20, 0x04], filter: [20, 0x01], fx: [[22, 0x08], [22, 0x10]], xf: [0x80, 0x40], leds: [44, 45]},
    {cue: [23, 0x40], filter: [23, 0x10], fx: [[25, 0x40], [25, 0x80]], xf: [0x02, 0x01], leds: [52, 53]},
];

TraktorS8.section = function(side) {
    var o = 8 * side;
    var at = function(b, m) { return [b + o, m]; };
    return new TraktorKontrolDeck({
        bd: side === 1,
        buttons: {
            shift: at(9, 0x80), deck: at(8, 0x04), flux: at(8, 0x10),
            play: at(9, 0x10), cue: at(9, 0x20), sync: at(9, 0x40),
            hotcue: at(9, 0x08), loop: at(9, 0x04), freeze: at(9, 0x02), remix: at(9, 0x01),
            browsePush: at(4, 0x08), loopPush: at(7, 0x10),
            fx: [at(4, 0x80), at(5, 0x04), at(5, 0x02), at(5, 0x01)],
            on: [at(6, 0x10), at(6, 0x08), at(6, 0x04), at(6, 0x02)],
            pads: [at(8, 0x08), at(8, 0x01), at(7, 0x01), at(7, 0x02),
                at(8, 0x20), at(8, 0x40), at(8, 0x80), at(8, 0x02)],
            assign: null,
        },
        encoders: null,
        strip: side ? 34 : 28,
        knobs: 1 + 8 * side, knobTurn: 1000, knobWrap: 65536,
        faders: 17 + 8 * side,
        fx: 43 + 8 * side,
        ledReport: 0x80 + side, ledLength: 118, bgr: false, deckLeds: null,
    });
};

TraktorS8.init = function() {
    this.sections = [this.section(0), this.section(1)];
    this.buttons = null;
    this.knobs = null;
    this.mixerLeds = new Uint8Array(73);
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
    TraktorKontrolDeck.connect(this.connections,
        function() { self.lightAll(); },
        function() { self.sections.forEach(function(s) { s.lightStrip(); }); });
    for (var c = 1; c <= 4; c++) {
        var ch = "[Channel" + c + "]";
        this.connections.push(engine.makeConnection(ch, "pfl", function() { self.lightMixer(); }));
        this.connections.push(engine.makeConnection(ch, "quantize", function() { self.lightMixer(); }));
        this.connections.push(engine.makeConnection("[QuickEffectRack1_" + ch + "]", "enabled",
            function() { self.lightMixer(); }));
    }
    this.lightAll();
};

TraktorS8.shutdown = function() {
    this.sections.forEach(function(s) {
        s.leds = new Uint8Array(118);
        s.send();
    });
    this.mixerLeds = new Uint8Array(73);
    controller.sendOutputReport(0x82, this.mixerLeds.buffer);
    this.sendFeature(0x26, 0);
    this.sendFeature(0x25, 0);
};

TraktorS8.shift = function() {
    return this.sections[0].shift || this.sections[1].shift;
};

TraktorS8.incomingData = function(data) {
    var last;
    if (data[0] === 0x01 && data.length >= 41) {
        last = this.buttons;
        this.buttons = Array.prototype.slice.call(data, 0, 41);
        if (last !== null) {
            this.sections[0].buttons(data, last);
            this.sections[1].buttons(data, last);
            this.mixerButtons(data, last);
        }
    } else if (data[0] === 0x02 && data.length >= 109) {
        last = this.knobs;
        this.knobs = Array.prototype.slice.call(data, 0, 109);
        if (last !== null) {
            this.sections[0].sliders(data, last);
            this.sections[1].sliders(data, last);
            this.mixerKnobs(data, last);
        }
    }
};

// --- Mixer -------------------------------------------------------------------

TraktorS8.mixerButtons = function(data, last) {
    var pressed = function(at) {
        return (data[at[0]] & at[1]) !== 0 && (last[at[0]] & at[1]) === 0;
    };
    var shift = this.shift();
    for (var n = 0; n < 4; n++) {
        var deck = "[Channel" + (n + 1) + "]";
        var c = this.CHANNELS[n];
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
        var xf = data[24] & (c.xf[0] | c.xf[1]);
        if (xf !== (last[24] & (c.xf[0] | c.xf[1]))) {
            engine.setValue(deck, "orientation", xf & c.xf[0] ? 0 : (xf & c.xf[1] ? 2 : 1));
        }
    }
    if (pressed([25, 0x08])) {
        var on = engine.getValue("[Channel1]", "quantize") ? 0 : 1;
        for (var d = 1; d <= 4; d++) {
            engine.setValue("[Channel" + d + "]", "quantize", on);
        }
    }
    if (pressed([22, 0x04])) {
        script.toggleControl("[Microphone]", "talkover");
    }
    // The mixer's FX lights follow SHIFT (units 3 and 4).
    if (shift !== this.lastShift) {
        this.lastShift = shift;
        this.lightMixer();
    }
};

TraktorS8.mixerKnobs = function(data, last) {
    var word = function(bytes, at) {
        return (bytes[at] | (bytes[at + 1] << 8)) & 0x0FFF;
    };
    var moved = function(at) {
        return word(data, at) !== word(last, at);
    };
    var value = function(at) {
        return Math.min(word(data, at) / 4078, 1);
    };
    for (var n = 0; n < 4; n++) {
        var deck = "[Channel" + (n + 1) + "]";
        var eq = "[EqualizerRack1_" + deck + "_Effect1]";
        var base = 69 + 10 * n;
        var knobs = [
            [base, deck, "pregain"], [base + 2, eq, "parameter3"], [base + 4, eq, "parameter2"],
            [base + 6, eq, "parameter1"], [base + 8, "[QuickEffectRack1_" + deck + "]", "super1"],
            [33 + 2 * n, deck, "volume"],
        ];
        knobs.forEach(function(k) {
            if (moved(k[0])) {
                engine.setParameter(k[1], k[2], value(k[0]));
            }
        });
    }
    if (moved(41)) {
        engine.setParameter("[Master]", "crossfader", value(41));
    }
};

// --- Lights ------------------------------------------------------------------

TraktorS8.sendFeature = function(command, mask) {
    var report = new Uint8Array(33);
    report[0] = command;
    report[1] = mask;
    controller.sendFeatureReport(0xF4, report.buffer);
};

TraktorS8.lightMixer = function() {
    var self = this;
    var on = function(v) { return v ? self.ON : self.DIM; };
    var shift = this.shift();
    var cue = 0;
    var filter = 0;
    for (var n = 0; n < 4; n++) {
        var deck = "[Channel" + (n + 1) + "]";
        var c = this.CHANNELS[n];
        for (var f = 0; f < 2; f++) {
            var unit = "[EffectRack1_EffectUnit" + (f + 1 + (shift ? 2 : 0)) + "]";
            this.mixerLeds[c.leds[f]] = on(engine.getValue(unit, "group_" + deck + "_enable"));
        }
        if (engine.getValue(deck, "pfl")) {
            cue |= 1 << n;
        }
        if (engine.getValue("[QuickEffectRack1_" + deck + "]", "enabled")) {
            filter |= 1 << n;
        }
    }
    this.mixerLeds[48] = this.DIM;
    this.mixerLeds[49] = on(engine.getValue("[Channel1]", "quantize"));
    controller.sendOutputReport(0x82, this.mixerLeds.buffer);
    if (cue !== this.sentCue) {
        this.sentCue = cue;
        this.sendFeature(0x26, cue);
    }
    if (filter !== this.sentFilter) {
        this.sentFilter = filter;
        this.sendFeature(0x25, filter);
    }
};

TraktorS8.lightAll = function() {
    this.sections.forEach(function(s) { s.lightAll(); });
    this.lightMixer();
};
