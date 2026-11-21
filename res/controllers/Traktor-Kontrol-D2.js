// Traktor Kontrol D2, played as Traktor plays it: one deck section (see
// Traktor-Kontrol-Deck-Section.js) with A-D buttons that send decks to the
// effect unit, or pick the deck with DECK held. openDJ's own mapping,
// written from the D2's published HID report layout and Native Instruments'
// manual. Its display stays dark.
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

TraktorD2.init = function() {
    this.section = new TraktorKontrolDeck({
        bd: false,
        buttons: {
            shift: [8, 0x80], deck: [7, 0x04], flux: [7, 0x10],
            play: [8, 0x10], cue: [8, 0x20], sync: [8, 0x40],
            hotcue: [8, 0x08], loop: [8, 0x04], freeze: [8, 0x02], remix: [8, 0x01],
            browsePush: [2, 0x08], loopPush: [6, 0x10],
            fx: [[2, 0x80], [3, 0x04], [3, 0x02], [3, 0x01]],
            on: [[4, 0x10], [4, 0x08], [4, 0x04], [4, 0x02]],
            pads: [[7, 0x08], [7, 0x01], [6, 0x01], [6, 0x02],
                [7, 0x20], [7, 0x40], [7, 0x80], [7, 0x02]],
            assign: [[5, 0x01], [5, 0x02], [5, 0x04], [5, 0x08]],
        },
        encoders: 1,
        strip: 13,
        knobs: 1, knobTurn: 1000, knobWrap: 65536,
        faders: 9,
        fx: 17,
        ledReport: 0x80, ledLength: 122, bgr: true, deckLeds: 118,
    });
    this.buttons = null;
    this.knobs = null;
    this.connections = [];
    TraktorKontrolDeck.softTakeover();
    var section = this.section;
    TraktorKontrolDeck.connect(this.connections,
        function() { section.lightAll(); },
        function() { section.lightStrip(); });
    section.lightAll();
};

TraktorD2.shutdown = function() {
    this.section.leds = new Uint8Array(122);
    this.section.send();
};

TraktorD2.incomingData = function(data) {
    var last;
    if (data[0] === 0x01 && data.length >= 17) {
        last = this.buttons;
        this.buttons = Array.prototype.slice.call(data, 0, 17);
        if (last !== null) {
            this.section.buttons(data, last);
        }
    } else if (data[0] === 0x02 && data.length >= 25) {
        last = this.knobs;
        this.knobs = Array.prototype.slice.call(data, 0, 25);
        if (last !== null) {
            this.section.sliders(data, last);
        }
    }
};
