// What a keyboard controller does in openDJ, the same on every keyboard:
// shared by the mappings of keyboards with knobs and transport buttons (the
// Komplete Kontrol M32 first). A keyboard's own script reads its hardware,
// calls these, and draws `KeyboardInstrument.lines()` on its screen.
//
// The keys, octave buttons, pitch and mod strips and pedal play over the
// keyboard's own USB MIDI port, which openDJ opens as a keyboard for the
// instrument deck: this file only deals with the controls around them.
//
//   knobs 1-8         the instrument's knobs on the page the deck shows
//                     (`[Instrument1],knob_page`), as endless encoders
//   page < >          the knob page before / after
//   preset up / down  the instrument's presets, in name order
//   play / stop       the instrument deck's step sequencer
//   rec               record the set; with SHIFT, record into the sequencer
//   metro / tempo     the instrument deck's metronome / tap its tempo
//   arp / scale       openDJ's arpeggiator / scale on or off
//   browse            turn: instruments, then that instrument's presets;
//                     press: open or load; SHIFT + press (or back): up a
//                     level, and out

var KeyboardInstrument = {};

KeyboardInstrument.G = "[Instrument1]";
KeyboardInstrument.KNOBS = 8;
// How long the screen shows a knob or a message, and stays in the browser
// after the last turn, in milliseconds.
KeyboardInstrument.SHOW_MS = 1500;
KeyboardInstrument.BROWSE_MS = 8000;
// Characters a screen line holds (a 128-pixel screen with a 5 x 7 font).
KeyboardInstrument.WIDTH = 21;

KeyboardInstrument.init = function() {
    this.touched = -1;
    this.touchedUntil = 0;
    this.message = "";
    this.messageUntil = 0;
    // The browser: null when closed, else "instruments" or "presets".
    this.level = null;
    this.index = 0;
    this.browseUntil = 0;
};

// The deck (1-4) playing the instrument, or 0.
KeyboardInstrument.deck = function() {
    for (let d = 1; d <= 4; d++) {
        if (engine.getValue("[Channel" + d + "]", "live_input") === 3) {
            return d;
        }
    }
    return 0;
};

KeyboardInstrument.channel = function() {
    const d = this.deck();
    return d ? "[Channel" + d + "]" : null;
};

KeyboardInstrument.count = function() {
    return Math.max(0, Math.round(engine.getValue(this.G, "num_parameters")));
};

KeyboardInstrument.pages = function() {
    return Math.max(1, Math.ceil(this.count() / this.KNOBS));
};

KeyboardInstrument.page = function() {
    return Math.min(this.pages() - 1, Math.max(0, Math.round(engine.getValue(this.G, "knob_page"))));
};

// Knob `k` (0-7) moved by `amount` of its whole range (negative: down).
KeyboardInstrument.turnKnob = function(k, amount) {
    const n = this.page() * this.KNOBS + k + 1;
    if (n > this.count() || !this.deck()) {
        return;
    }
    const key = "parameter" + n;
    const v = Math.min(1, Math.max(0, engine.getValue(this.G, key) + amount));
    engine.setValue(this.G, key, v);
    this.touched = k;
    this.touchedUntil = Date.now() + this.SHOW_MS;
};

KeyboardInstrument.stepPage = function(by) {
    const page = Math.min(this.pages() - 1, Math.max(0, this.page() + by));
    engine.setValue(this.G, "knob_page", page);
    this.say("PAGE " + (page + 1) + "/" + this.pages());
};

KeyboardInstrument.stepPreset = function(by) {
    engine.setValue(this.G, by > 0 ? "preset_next" : "preset_prev", 1);
};

KeyboardInstrument.press = function(group, key) {
    engine.setValue(group, key, 1);
    engine.setValue(group, key, 0);
};

KeyboardInstrument.toggle = function(group, key) {
    engine.setValue(group, key, engine.getValue(group, key) > 0 ? 0 : 1);
};

KeyboardInstrument.play = function() {
    engine.setValue(this.G, "seq", 1);
};

KeyboardInstrument.stop = function() {
    engine.setValue(this.G, "seq", 0);
};

KeyboardInstrument.recordSet = function() {
    this.press("[Recording]", "toggle_recording");
};

KeyboardInstrument.recordSequence = function() {
    this.toggle(this.G, "seq_rec");
};

KeyboardInstrument.metronome = function() {
    const ch = this.channel();
    if (ch) {
        this.toggle(ch, "metronome");
    }
};

KeyboardInstrument.tapTempo = function() {
    const ch = this.channel();
    if (ch) {
        this.press(ch, "bpm_tap");
    }
};

KeyboardInstrument.arp = function() {
    this.toggle(this.G, "arp");
};

KeyboardInstrument.arpHold = function() {
    this.toggle(this.G, "arp_hold");
};

// Off, or Auto (the key of the track playing).
KeyboardInstrument.scale = function() {
    engine.setValue(this.G, "scale_mode", engine.getValue(this.G, "scale_mode") > 0 ? 0 : 1);
};

KeyboardInstrument.dice = function() {
    this.press(this.G, "seq_dice");
};

KeyboardInstrument.diceUndo = function() {
    this.press(this.G, "seq_dice_undo");
};

KeyboardInstrument.editor = function() {
    this.toggle(this.G, "show_editor");
};

KeyboardInstrument.say = function(text) {
    this.message = text;
    this.messageUntil = Date.now() + this.SHOW_MS;
};

// The lines of the browser's list: instruments or the instrument's
// presets, as the app shares them.
KeyboardInstrument.list = function(level) {
    const text = engine.getText(this.G, level || this.level);
    return text ? text.split("\n") : [];
};

KeyboardInstrument.browsing = function() {
    if (this.level && Date.now() > this.browseUntil) {
        this.level = null;
    }
    return this.level !== null;
};

KeyboardInstrument.keepBrowsing = function() {
    this.browseUntil = Date.now() + this.BROWSE_MS;
};

// Opens the browser on the instruments, at the one playing.
KeyboardInstrument.openBrowser = function() {
    this.level = "instruments";
    const playing = Math.round(engine.getValue(this.G, "instrument")) - 1;
    this.index = this.deck() && playing >= 0 ? playing : 0;
    this.keepBrowsing();
};

KeyboardInstrument.browseTurn = function(by) {
    if (!this.browsing()) {
        this.openBrowser();
        return;
    }
    const n = this.list().length;
    this.index = n ? Math.min(n - 1, Math.max(0, this.index + by)) : 0;
    this.keepBrowsing();
};

// Press: open the instrument picked (and list its presets), or load the
// preset picked.
KeyboardInstrument.browseEnter = function() {
    if (!this.browsing()) {
        this.openBrowser();
        return;
    }
    const n = this.list().length;
    if (!n) {
        return;
    }
    if (this.level === "instruments") {
        engine.setValue(this.G, "instrument_load", this.index + 1);
        this.level = "presets";
        this.index = 0;
    } else {
        engine.setValue(this.G, "preset_load", this.index + 1);
    }
    this.keepBrowsing();
};

KeyboardInstrument.browseBack = function() {
    if (this.level === "presets") {
        this.openBrowser();
    } else {
        this.level = null;
    }
};

KeyboardInstrument.fit = function(text) {
    return String(text || "").toUpperCase().slice(0, this.WIDTH);
};

KeyboardInstrument.bar = function(value, width) {
    const n = Math.round(Math.min(1, Math.max(0, value)) * width);
    return "#".repeat(n) + "-".repeat(width - n);
};

// Four lines for a screen.
KeyboardInstrument.lines = function() {
    const now = Date.now();
    const name = engine.getText(this.G, "name");
    if (this.browsing()) {
        const items = this.list();
        const title = this.level === "instruments" ? "INSTRUMENTS" : (name || "PRESETS");
        const out = [title + (items.length ? " " + (this.index + 1) + "/" + items.length : "")];
        if (!items.length) {
            out.push(this.level === "instruments" ? "NONE: TICK SOME IN" : "NO PRESETS YET");
            out.push(this.level === "instruments" ? "SETTINGS > PLUGINS" : "");
        }
        // The one picked, with the one before and after it.
        const first = Math.max(0, Math.min(this.index - 1, items.length - 3));
        for (let i = first; i < items.length && out.length < 4; i++) {
            out.push((i === this.index ? ">" : " ") + items[i]);
        }
        return out.map(l => this.fit(l));
    }
    if (!this.deck()) {
        return ["NO INSTRUMENT", "TURN THE WHEEL TO", "PICK ONE", ""].map(l => this.fit(l));
    }
    if (this.touched >= 0 && now < this.touchedUntil) {
        const n = this.page() * this.KNOBS + this.touched + 1;
        const value = engine.getValue(this.G, "parameter" + n);
        return [
            name,
            engine.getText(this.G, "parameter" + n + "_name") || "KNOB " + n,
            Math.round(value * 100) + "%",
            this.bar(value, this.WIDTH),
        ].map(l => this.fit(l));
    }
    const seq = engine.getValue(this.G, "seq") > 0 ? "SEQ >" : "";
    const rec = engine.getValue(this.G, "seq_rec") > 0 ? " REC" : "";
    const page = this.pages() > 1 ? "PAGE " + (this.page() + 1) + "/" + this.pages() : "";
    return [
        name,
        engine.getText(this.G, "preset"),
        page,
        now < this.messageUntil ? this.message : seq + rec,
    ].map(l => this.fit(l));
};
// What a keyboard controller does in openDJ, the same on every keyboard:
// shared by the mappings of keyboards with knobs and transport buttons (the
// Komplete Kontrol M32 first). A keyboard's own script reads its hardware,
// calls these, and draws `KeyboardInstrument.lines()` on its screen.
//
// The keys, octave buttons, pitch and mod strips and pedal play over the
// keyboard's own USB MIDI port, which openDJ opens as a keyboard for the
// instrument deck: this file only deals with the controls around them.
//
//   knobs 1-8         the instrument's knobs on the page the deck shows
//                     (`[Instrument1],knob_page`), as endless encoders
//   page < >          the knob page before / after
//   preset up / down  the instrument's presets, in name order
//   play / stop       the instrument deck's step sequencer
//   rec               record the set; with SHIFT, record into the sequencer
//   metro / tempo     the instrument deck's metronome / tap its tempo
//   arp / scale       openDJ's arpeggiator / scale on or off
//   browse            turn: instruments, then that instrument's presets;
//                     press: open or load; SHIFT + press (or back): up a
//                     level, and out

var KeyboardInstrument = {};

KeyboardInstrument.G = "[Instrument1]";
KeyboardInstrument.KNOBS = 8;
// How long the screen shows a knob or a message, and stays in the browser
// after the last turn, in milliseconds.
KeyboardInstrument.SHOW_MS = 1500;
KeyboardInstrument.BROWSE_MS = 8000;
// Characters a screen line holds (a 128-pixel screen with a 5 x 7 font).
KeyboardInstrument.WIDTH = 21;

KeyboardInstrument.init = function() {
    this.touched = -1;
    this.touchedUntil = 0;
    this.message = "";
    this.messageUntil = 0;
    // The browser: null when closed, else "instruments" or "presets".
    this.level = null;
    this.index = 0;
    this.browseUntil = 0;
};

// The deck (1-4) playing the instrument, or 0.
KeyboardInstrument.deck = function() {
    for (let d = 1; d <= 4; d++) {
        if (engine.getValue("[Channel" + d + "]", "live_input") === 3) {
            return d;
        }
    }
    return 0;
};

KeyboardInstrument.channel = function() {
    const d = this.deck();
    return d ? "[Channel" + d + "]" : null;
};

KeyboardInstrument.count = function() {
    return Math.max(0, Math.round(engine.getValue(this.G, "num_parameters")));
};

KeyboardInstrument.pages = function() {
    return Math.max(1, Math.ceil(this.count() / this.KNOBS));
};

KeyboardInstrument.page = function() {
    return Math.min(this.pages() - 1, Math.max(0, Math.round(engine.getValue(this.G, "knob_page"))));
};

// Knob `k` (0-7) moved by `amount` of its whole range (negative: down).
KeyboardInstrument.turnKnob = function(k, amount) {
    const n = this.page() * this.KNOBS + k + 1;
    if (n > this.count() || !this.deck()) {
        return;
    }
    const key = "parameter" + n;
    const v = Math.min(1, Math.max(0, engine.getValue(this.G, key) + amount));
    engine.setValue(this.G, key, v);
    this.touched = k;
    this.touchedUntil = Date.now() + this.SHOW_MS;
};

KeyboardInstrument.stepPage = function(by) {
    const page = Math.min(this.pages() - 1, Math.max(0, this.page() + by));
    engine.setValue(this.G, "knob_page", page);
    this.say("PAGE " + (page + 1) + "/" + this.pages());
};

KeyboardInstrument.stepPreset = function(by) {
    engine.setValue(this.G, by > 0 ? "preset_next" : "preset_prev", 1);
};

KeyboardInstrument.press = function(group, key) {
    engine.setValue(group, key, 1);
    engine.setValue(group, key, 0);
};

KeyboardInstrument.toggle = function(group, key) {
    engine.setValue(group, key, engine.getValue(group, key) > 0 ? 0 : 1);
};

KeyboardInstrument.play = function() {
    engine.setValue(this.G, "seq", 1);
};

KeyboardInstrument.stop = function() {
    engine.setValue(this.G, "seq", 0);
};

KeyboardInstrument.recordSet = function() {
    this.press("[Recording]", "toggle_recording");
};

KeyboardInstrument.recordSequence = function() {
    this.toggle(this.G, "seq_rec");
};

KeyboardInstrument.metronome = function() {
    const ch = this.channel();
    if (ch) {
        this.toggle(ch, "metronome");
    }
};

KeyboardInstrument.tapTempo = function() {
    const ch = this.channel();
    if (ch) {
        this.press(ch, "bpm_tap");
    }
};

KeyboardInstrument.arp = function() {
    this.toggle(this.G, "arp");
};

KeyboardInstrument.arpHold = function() {
    this.toggle(this.G, "arp_hold");
};

// Off, or Auto (the key of the track playing).
KeyboardInstrument.scale = function() {
    engine.setValue(this.G, "scale_mode", engine.getValue(this.G, "scale_mode") > 0 ? 0 : 1);
};

KeyboardInstrument.dice = function() {
    this.press(this.G, "seq_dice");
};

KeyboardInstrument.diceUndo = function() {
    this.press(this.G, "seq_dice_undo");
};

KeyboardInstrument.editor = function() {
    this.toggle(this.G, "show_editor");
};

KeyboardInstrument.say = function(text) {
    this.message = text;
    this.messageUntil = Date.now() + this.SHOW_MS;
};

// The lines of the browser's list: instruments or the instrument's
// presets, as the app shares them.
KeyboardInstrument.list = function(level) {
    const text = engine.getText(this.G, level || this.level);
    return text ? text.split("\n") : [];
};

KeyboardInstrument.browsing = function() {
    if (this.level && Date.now() > this.browseUntil) {
        this.level = null;
    }
    return this.level !== null;
};

KeyboardInstrument.keepBrowsing = function() {
    this.browseUntil = Date.now() + this.BROWSE_MS;
};

// Opens the browser on the instruments, at the one playing.
KeyboardInstrument.openBrowser = function() {
    this.level = "instruments";
    const playing = Math.round(engine.getValue(this.G, "instrument")) - 1;
    this.index = this.deck() && playing >= 0 ? playing : 0;
    this.keepBrowsing();
};

KeyboardInstrument.browseTurn = function(by) {
    if (!this.browsing()) {
        this.openBrowser();
        return;
    }
    const n = this.list().length;
    this.index = n ? Math.min(n - 1, Math.max(0, this.index + by)) : 0;
    this.keepBrowsing();
};

// Press: open the instrument picked (and list its presets), or load the
// preset picked.
KeyboardInstrument.browseEnter = function() {
    if (!this.browsing()) {
        this.openBrowser();
        return;
    }
    const n = this.list().length;
    if (!n) {
        return;
    }
    if (this.level === "instruments") {
        engine.setValue(this.G, "instrument_load", this.index + 1);
        this.level = "presets";
        this.index = 0;
    } else {
        engine.setValue(this.G, "preset_load", this.index + 1);
    }
    this.keepBrowsing();
};

KeyboardInstrument.browseBack = function() {
    if (this.level === "presets") {
        this.openBrowser();
    } else {
        this.level = null;
    }
};

KeyboardInstrument.fit = function(text) {
    return String(text || "").toUpperCase().slice(0, this.WIDTH);
};

KeyboardInstrument.bar = function(value, width) {
    const n = Math.round(Math.min(1, Math.max(0, value)) * width);
    return "#".repeat(n) + "-".repeat(width - n);
};

// Four lines for a screen.
KeyboardInstrument.lines = function() {
    const now = Date.now();
    const name = engine.getText(this.G, "name");
    if (this.browsing()) {
        const items = this.list();
        const title = this.level === "instruments" ? "INSTRUMENTS" : (name || "PRESETS");
        const out = [title + (items.length ? " " + (this.index + 1) + "/" + items.length : "")];
        if (!items.length) {
            out.push(this.level === "instruments" ? "NONE: TICK SOME IN" : "NO PRESETS YET");
            out.push(this.level === "instruments" ? "SETTINGS > PLUGINS" : "");
        }
        // The one picked, with the one before and after it.
        const first = Math.max(0, Math.min(this.index - 1, items.length - 3));
        for (let i = first; i < items.length && out.length < 4; i++) {
            out.push((i === this.index ? ">" : " ") + items[i]);
        }
        return out.map(l => this.fit(l));
    }
    if (!this.deck()) {
        return ["NO INSTRUMENT", "TURN THE WHEEL TO", "PICK ONE", ""].map(l => this.fit(l));
    }
    if (this.touched >= 0 && now < this.touchedUntil) {
        const n = this.page() * this.KNOBS + this.touched + 1;
        const value = engine.getValue(this.G, "parameter" + n);
        return [
            name,
            engine.getText(this.G, "parameter" + n + "_name") || "KNOB " + n,
            Math.round(value * 100) + "%",
            this.bar(value, this.WIDTH),
        ].map(l => this.fit(l));
    }
    const seq = engine.getValue(this.G, "seq") > 0 ? "SEQ >" : "";
    const rec = engine.getValue(this.G, "seq_rec") > 0 ? " REC" : "";
    const page = this.pages() > 1 ? "PAGE " + (this.page() + 1) + "/" + this.pages() : "";
    return [
        name,
        engine.getText(this.G, "preset"),
        page,
        now < this.messageUntil ? this.message : seq + rec,
    ].map(l => this.fit(l));
};

// For 128 x 32 one-bit screens (Native Instruments' M32 and Mikro MK3):
// `lines` (four) as 512 bytes, four rows of 128 columns, one byte eight
// pixels down with bit 0 the top, a set bit lit.
KeyboardInstrument.pixels = function(lines) {
    const buf = new Array(512).fill(0);
    for (let l = 0; l < 4; l++) {
        const text = this.fit(lines[l]);
        for (let c = 0; c < text.length; c++) {
            const glyph = this.FONT[text[c]] || this.FONT["?"];
            for (let x = 0; x < 5; x++) {
                buf[l * 128 + c * 6 + x] = glyph[x];
            }
        }
    }
    return buf;
};

// A 5 x 7 font (as the Maschine Mikro MK3 script's), five columns a
// character, bit 0 the top row.
KeyboardInstrument.FONT = {
    " ": [0x00, 0x00, 0x00, 0x00, 0x00], "!": [0x00, 0x00, 0x5f, 0x00, 0x00],
    "#": [0x14, 0x7f, 0x14, 0x7f, 0x14], "%": [0x23, 0x13, 0x08, 0x64, 0x62],
    "+": [0x08, 0x08, 0x3e, 0x08, 0x08], "-": [0x08, 0x08, 0x08, 0x08, 0x08],
    ".": [0x00, 0x60, 0x60, 0x00, 0x00], "/": [0x20, 0x10, 0x08, 0x04, 0x02],
    ":": [0x00, 0x36, 0x36, 0x00, 0x00], "<": [0x08, 0x14, 0x22, 0x41, 0x00],
    ">": [0x41, 0x22, 0x14, 0x08, 0x00], "?": [0x02, 0x01, 0x51, 0x09, 0x06],
    "0": [0x3e, 0x51, 0x49, 0x45, 0x3e], "1": [0x00, 0x42, 0x7f, 0x40, 0x00],
    "2": [0x42, 0x61, 0x51, 0x49, 0x46], "3": [0x21, 0x41, 0x45, 0x4b, 0x31],
    "4": [0x18, 0x14, 0x12, 0x7f, 0x10], "5": [0x27, 0x45, 0x45, 0x45, 0x39],
    "6": [0x3c, 0x4a, 0x49, 0x49, 0x30], "7": [0x01, 0x71, 0x09, 0x05, 0x03],
    "8": [0x36, 0x49, 0x49, 0x49, 0x36], "9": [0x06, 0x49, 0x49, 0x29, 0x1e],
    "A": [0x7e, 0x11, 0x11, 0x11, 0x7e], "B": [0x7f, 0x49, 0x49, 0x49, 0x36],
    "C": [0x3e, 0x41, 0x41, 0x41, 0x22], "D": [0x7f, 0x41, 0x41, 0x22, 0x1c],
    "E": [0x7f, 0x49, 0x49, 0x49, 0x41], "F": [0x7f, 0x09, 0x09, 0x01, 0x01],
    "G": [0x3e, 0x41, 0x41, 0x51, 0x32], "H": [0x7f, 0x08, 0x08, 0x08, 0x7f],
    "I": [0x00, 0x41, 0x7f, 0x41, 0x00], "J": [0x20, 0x40, 0x41, 0x3f, 0x01],
    "K": [0x7f, 0x08, 0x14, 0x22, 0x41], "L": [0x7f, 0x40, 0x40, 0x40, 0x40],
    "M": [0x7f, 0x02, 0x04, 0x02, 0x7f], "N": [0x7f, 0x04, 0x08, 0x10, 0x7f],
    "O": [0x3e, 0x41, 0x41, 0x41, 0x3e], "P": [0x7f, 0x09, 0x09, 0x09, 0x06],
    "Q": [0x3e, 0x41, 0x51, 0x21, 0x5e], "R": [0x7f, 0x09, 0x19, 0x29, 0x46],
    "S": [0x46, 0x49, 0x49, 0x49, 0x31], "T": [0x01, 0x01, 0x7f, 0x01, 0x01],
    "U": [0x3f, 0x40, 0x40, 0x40, 0x3f], "V": [0x1f, 0x20, 0x40, 0x20, 0x1f],
    "W": [0x7f, 0x20, 0x18, 0x20, 0x7f], "X": [0x63, 0x14, 0x08, 0x14, 0x63],
    "Y": [0x03, 0x04, 0x78, 0x04, 0x03], "Z": [0x61, 0x51, 0x49, 0x45, 0x43],
    "(": [0x00, 0x1c, 0x22, 0x41, 0x00], ")": [0x00, 0x41, 0x22, 0x1c, 0x00],
    "'": [0x00, 0x05, 0x03, 0x00, 0x00], ",": [0x00, 0x50, 0x30, 0x00, 0x00],
    "&": [0x36, 0x49, 0x55, 0x22, 0x50], "_": [0x40, 0x40, 0x40, 0x40, 0x40],
    "=": [0x14, 0x14, 0x14, 0x14, 0x14], "*": [0x14, 0x08, 0x3e, 0x08, 0x14],
};

