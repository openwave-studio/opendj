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
//   PLUG-IN            the instrument's own window
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

KompleteKontrolM32.OFF = 0x00;
KompleteKontrolM32.DIM = 0x10;
KompleteKontrolM32.ON = 0x7f;

KompleteKontrolM32.init = function(_id) {
    KeyboardInstrument.init();
    this.last = null;
    this.held = new Array(this.BUTTONS).fill(false);
    this.lastLights = "";
    this.lastScreen = ["", ""];
    this.timer = engine.beginTimer(50, () => this.refresh());
    this.refresh();
};

KompleteKontrolM32.shutdown = function() {
    engine.stopTimer(this.timer);
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

KompleteKontrolM32.press = function(b) {
    const B = this.B;
    const K = KeyboardInstrument;
    const shift = this.held[B.SHIFT];
    switch (b) {
    case B.LEFT: K.stepPage(-1); break;
    case B.RIGHT: K.stepPage(1); break;
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
    case B.PLUGIN: K.editor(); break;
    case B.BROWSER: K.openBrowser(); break;
    case B.TRACK: K.level = null; break;
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
KompleteKontrolM32.refresh = function() {
    this.sendLights();
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
    const pixels = KeyboardInstrument.pixels(KeyboardInstrument.lines());
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
