// Native Instruments Komplete Kontrol M32: openDJ's instrument deck from
// the keyboard (see Keyboard-Instrument.js for what each control does).
//
// The keys, octave buttons, pitch and mod strips and pedal come in over the
// M32's USB MIDI port, which openDJ opens as a keyboard for the instrument
// deck (the mapping's `keys="midi"`). Its buttons, knobs, lights and screen
// are USB HID (17cc:1860), with no NI software needed. Report layout as
// decoded from the firmware in NI's updater by the MIT-licensed
// komplete-keylab-32 project:
//
//   in  0x01  buttons: bits from byte 1 (see B); knobs 1-8 as uint16 LE
//             counters 0-999 from byte 6; the 4-D encoder's turn as two
//             4-bit counters in byte 24
//   out 0x80  lights: 21 bytes, one a button (B 0-18), 0-0x7f
//   out 0xe0  the 128 x 32 screen: x, page, width, pages (uint16 LE each),
//             then 256 bytes of 8-pixel columns (a clear bit lights)
//
//   KNOBS 1-8          the instrument's knobs on the page shown; SHIFT: fine
//   < (M) / > (S)      knob page before / after
//   PRESET UP / DOWN   previous / next preset of the instrument
//   PLAY / STOP        the step sequencer on / off
//   REC                record the set; SHIFT + REC: record into the sequencer
//   METRO / TEMPO      the instrument deck's metronome / tap tempo
//   ARP / SCALE        arpeggiator / scale (Auto) on or off; SHIFT + ARP: hold
//   IDEAS / UNDO       a new random pattern (Dice) / undo it
//   PLUG-IN            the instrument's own window (SHIFT + PLUG-IN, MIDI,
//                      and SHIFT + TRACK, INSTANCE, are left to the M32)
//   4-D encoder        turn: browse instruments, then presets; press: open
//                      or load; SHIFT + press or LEFT: back; UP / DOWN step
//   BROWSER / TRACK    open / close the browser

var KompleteKontrolM32 = {};

// Buttons, as the bits of report 0x01 from byte 1; the lights of 0-18
// are the same bytes of report 0x80.
KompleteKontrolM32.B = {
    SHIFT: 0, SCALE: 1, ARP: 2, UNDO: 3, QUANTIZE: 4, IDEAS: 5, LOOP: 6, METRO: 7,
    TEMPO: 8, PLAY: 9, REC: 10, STOP: 11, PRESET_UP: 12, PRESET_DOWN: 13, LEFT: 14, RIGHT: 15,
    BROWSER: 16, PLUGIN: 17, TRACK: 18,
    UP: 21, PUSH_LEFT: 22, PUSH_RIGHT: 23, DOWN: 24, PUSH: 33,
};
KompleteKontrolM32.BUTTONS = 40;
KompleteKontrolM32.LIGHTS = 21;
KompleteKontrolM32.KNOB_BYTE = 6;
KompleteKontrolM32.ENCODER_BYTE = 24;
// A knob's counter runs 0-999 and wraps; how far one count turns a knob,
// of its whole range, and with SHIFT.
KompleteKontrolM32.PER_COUNT = 0.002;
KompleteKontrolM32.FINE_PER_COUNT = 0.0004;
// In the M32's own MIDI mode ("Template 1" on its screen) knobs 1-8 send
// controllers on its MIDI port instead, which openDJ hands to incomingMidi
// below: from these, one set for each of its 4 pages (< > step them, as
// they step the deck's knob page).
KompleteKontrolM32.KNOB_CCS = [14, 22, 102, 110];
// How long after a knob moves over HID its MIDI controllers are left alone.
KompleteKontrolM32.HID_KNOBS_HOLD_MS = 5000;
// The most one knob message moves its count, turning fast.
KompleteKontrolM32.MAX_CC_STEP = 16;

KompleteKontrolM32.OFF = 0x00;
KompleteKontrolM32.DIM = 0x10;
KompleteKontrolM32.ON = 0x7f;

KompleteKontrolM32.init = function(_id) {
    KeyboardInstrument.init();
    this.last = null;
    this.ccs = [];
    this.coverAt = 0;
    this.drawnAt = 0;
    this.hidKnobsAt = -Infinity;
    this.held = new Array(this.BUTTONS).fill(false);
    this.lastLights = "";
    this.lastScreen = ["", ""];
    this.timer = engine.beginTimer(50, () => this.refresh());
    // The M32 writes its own "Template 1" over the screen at times (in its
    // MIDI mode): draw all of it again now and then.
    this.redrawTimer = engine.beginTimer(1000, () => {
        this.lastScreen = ["", ""];
    });
    this.refresh();
};

KompleteKontrolM32.shutdown = function() {
    engine.stopTimer(this.timer);
    engine.stopTimer(this.redrawTimer);
    controller.sendOutputReport(0x80, new Array(this.LIGHTS).fill(0), true);
    this.sendHalf(0, new Array(256).fill(0xff));
    this.sendHalf(1, new Array(256).fill(0xff));
};

KompleteKontrolM32.bit = function(data, b) {
    return (data[1 + (b >> 3)] & (1 << (b & 7))) !== 0;
};

KompleteKontrolM32.incomingData = function(data, _length) {
    if (data[0] !== 0x01) {
        return;
    }
    const last = this.last;
    this.last = data.slice ? data.slice() : Array.from(data);
    if (!last) {
        // The first report sets where the knobs and buttons are.
        for (let b = 0; b < this.BUTTONS; b++) {
            this.held[b] = this.bit(data, b);
        }
        return;
    }
    for (let b = 0; b < this.BUTTONS; b++) {
        const down = this.bit(data, b);
        if (down !== this.held[b]) {
            this.held[b] = down;
            if (down) {
                KeyboardInstrument.touch();
                this.press(b);
            }
        }
    }
    const fine = this.held[this.B.SHIFT];
    for (let k = 0; k < KeyboardInstrument.KNOBS; k++) {
        const o = this.KNOB_BYTE + 2 * k;
        const now = data[o] | (data[o + 1] << 8);
        const before = last[o] | (last[o + 1] << 8);
        if (now !== before) {
            this.hidKnobsAt = Date.now();
            const delta = ((now - before + 1500) % 1000) - 500;
            KeyboardInstrument.turnKnob(k, delta * (fine ? this.FINE_PER_COUNT : this.PER_COUNT));
        }
    }
    const now = data[this.ENCODER_BYTE];
    const before = last[this.ENCODER_BYTE];
    if (now !== before) {
        // Two 4-bit counters: whichever moved.
        const delta = (now & 0x0f) !== (before & 0x0f)
            ? (((now & 0x0f) - (before & 0x0f) + 24) % 16) - 8
            : (((now >> 4) - (before >> 4) + 24) % 16) - 8;
        if (delta) {
            KeyboardInstrument.browseTurn(delta);
        }
    }
    this.refresh();
};

// A knob in the M32's MIDI mode. Its value is absolute (0-127, stopping
// at each end): the change from the one before turns the instrument's
// knob, so the two never jump to match each other.
KompleteKontrolM32.incomingMidi = function(status, control, value) {
    const first = this.KNOB_CCS.find(c => control >= c && control < c + KeyboardInstrument.KNOBS);
    if ((status & 0xf0) !== 0xb0 || first === undefined) {
        return;
    }
    const k = control - first;
    // With NI's host agent running the knobs come over HID (above), and
    // any controllers they also send here are the same turn.
    if (Date.now() - this.hidKnobsAt < this.HID_KNOBS_HOLD_MS) {
        this.ccs[control] = value;
        return;
    }
    this.coverTemplate();
    const before = this.ccs[control];
    this.ccs[control] = value;
    // A jump means the M32's count isn't where openDJ last put it: start
    // from here.
    if (before === undefined || before === value || Math.abs(value - before) > this.MAX_CC_STEP) {
        return;
    }
    const fine = this.held[this.B.SHIFT];
    KeyboardInstrument.turnKnob(k, (value - before) / 127 * (fine ? 0.2 : 1));
    this.refresh();
};

// In its MIDI mode the M32 shows its own "Template 1" readout as soon as a
// knob turns or < > is pressed. Drawing over it while the knob turns only
// flickers (the readout comes back with every step, and shows the knob's
// value too, as openDJ keeps its count): draw the screen again once
// COVER_AFTER_MS after the last one.
KompleteKontrolM32.COVER_AFTER_MS = 300;
KompleteKontrolM32.coverTemplate = function() {
    this.coverAt = Date.now() + this.COVER_AFTER_MS;
};

KompleteKontrolM32.press = function(b) {
    const B = this.B;
    const K = KeyboardInstrument;
    const shift = this.held[B.SHIFT];
    switch (b) {
    case B.LEFT: K.stepPage(-1); this.coverTemplate(); break;
    case B.RIGHT: K.stepPage(1); this.coverTemplate(); break;
    case B.PRESET_UP: K.stepPreset(-1); break;
    case B.PRESET_DOWN: K.stepPreset(1); break;
    case B.PLAY: K.play(); break;
    case B.STOP: K.stop(); break;
    case B.REC:
        if (shift) {
            K.recordSequence();
        } else {
            K.recordSet();
        }
        break;
    case B.METRO: K.metronome(); break;
    case B.TEMPO: K.tapTempo(); break;
    case B.ARP:
        if (shift) {
            K.arpHold();
        } else {
            K.arp();
        }
        break;
    case B.SCALE: K.scale(); break;
    case B.IDEAS: K.dice(); break;
    case B.UNDO: K.diceUndo(); break;
    // SHIFT + PLUG-IN is the M32's own MIDI (mode) button and SHIFT +
    // TRACK its INSTANCE button: left to the keyboard and NI's agent.
    case B.PLUGIN: if (!shift) { K.editor(); } break;
    case B.BROWSER: K.openBrowser(); break;
    case B.TRACK: if (!shift) { K.level = null; } break;
    case B.PUSH:
        if (shift) {
            K.browseBack();
        } else {
            K.browseEnter();
        }
        break;
    case B.PUSH_LEFT: K.browseBack(); break;
    case B.PUSH_RIGHT: K.browseEnter(); break;
    case B.UP: K.browseTurn(-1); break;
    case B.DOWN: K.browseTurn(1); break;
    default: break;
    }
};

// Lights and screen, each sent only when it changed.
// In its MIDI mode each knob keeps its own 0-127 count, which stops at the
// ends. Sending it the deck's knob values back keeps the count where the
// knob is (the M32 takes them on all its pages' controllers), so it stops
// just where the knob does. With NI's agent the knobs are endless and come
// over HID, so nothing is sent.
KompleteKontrolM32.syncKnobCcs = function() {
    if (Date.now() - this.hidKnobsAt < this.HID_KNOBS_HOLD_MS) {
        return;
    }
    for (let k = 0; k < KeyboardInstrument.KNOBS; k++) {
        const v = KeyboardInstrument.knobValue(k);
        if (v === null) {
            continue;
        }
        const value = Math.round(v * 127);
        for (const first of this.KNOB_CCS) {
            const cc = first + k;
            if (this.ccs[cc] !== value) {
                this.ccs[cc] = value;
                midi.sendShortMsg(0xb0, cc, value);
            }
        }
    }
};

// The screen is drawn from the timer, at most every DRAW_EVERY_MS: a screen
// write holds up reading the knobs, so drawing on each knob report would
// pile reports up and the knobs would run on after they stop.
KompleteKontrolM32.DRAW_EVERY_MS = 100;
KompleteKontrolM32.refresh = function() {
    this.sendLights();
    this.syncKnobCcs();
    const now = Date.now();
    if (this.coverAt && now >= this.coverAt) {
        this.coverAt = 0;
        this.lastScreen = ["", ""];
        this.drawnAt = 0;
    }
    // Not while the M32's own readout is up.
    if (this.coverAt || now - this.drawnAt < this.DRAW_EVERY_MS) {
        return;
    }
    this.drawnAt = now;
    this.drawScreen();
};

KompleteKontrolM32.sendLights = function() {
    const B = this.B;
    const K = KeyboardInstrument;
    const G = K.G;
    const out = new Array(this.LIGHTS).fill(this.OFF);
    const lit = (b, on) => {
        out[b] = on ? this.ON : this.DIM;
    };
    const ch = K.channel();
    const playing = !!ch;
    lit(B.SHIFT, this.held[B.SHIFT]);
    if (playing) {
        const page = K.page();
        lit(B.LEFT, false);
        lit(B.RIGHT, false);
        if (page > 0) {
            out[B.LEFT] = this.ON;
        }
        if (page < K.pages() - 1) {
            out[B.RIGHT] = this.ON;
        }
        lit(B.PRESET_UP, false);
        lit(B.PRESET_DOWN, false);
        lit(B.PLAY, engine.getValue(G, "seq") > 0);
        lit(B.STOP, false);
        lit(B.METRO, engine.getValue(ch, "metronome") > 0);
        lit(B.TEMPO, false);
        lit(B.ARP, engine.getValue(G, "arp") > 0);
        lit(B.SCALE, engine.getValue(G, "scale_mode") > 0);
        lit(B.IDEAS, false);
        lit(B.UNDO, engine.getValue(G, "seq_can_undo") > 0);
        lit(B.PLUGIN, engine.getValue(G, "show_editor") > 0);
    }
    out[B.REC] = engine.getValue("[Recording]", "status") > 0 ? this.ON : this.DIM;
    lit(B.BROWSER, K.browsing());
    const key = out.join(",");
    if (key !== this.lastLights) {
        this.lastLights = key;
        controller.sendOutputReport(0x80, out);
    }
};

KompleteKontrolM32.drawScreen = function() {
    const pixels = KeyboardInstrument.screen();
    // Each half only when it changed: a screen write holds up the knobs.
    for (let half = 0; half < 2; half++) {
        const bytes = pixels.slice(half * 256, half * 256 + 256).map(v => v ^ 0xff);
        const key = bytes.join(",");
        if (key !== this.lastScreen[half]) {
            this.lastScreen[half] = key;
            this.sendHalf(half, bytes);
        }
    }
};

KompleteKontrolM32.sendHalf = function(half, bytes) {
    // x 0, page 0 or 2, width 128, two pages.
    controller.sendOutputReport(0xe0, [0x00, 0x00, half * 2, 0x00, 0x80, 0x00, 0x02, 0x00].concat(bytes), true);
};
