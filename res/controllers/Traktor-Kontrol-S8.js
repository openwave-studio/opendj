// Traktor Kontrol S8, played as Traktor plays it: two deck sections and a
// four-channel mixer (see Traktor-Kontrol-S5-S8.js). openDJ's own mapping, written from the S8's published HID report
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

var TraktorS8Section = function(side) {
    var o = 8 * side;
    var at = function(b, m) { return [b + o, m]; };
    return {
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
    };
};

// Channels A-D. Low EQ on C and D follows the other channels' pattern.
var TraktorS8Channel = function(n, cue, filter, fx, xf, fxLeds) {
    var base = 69 + 10 * n;
    return {
        cue: cue, filter: filter, fx: fx, xf: xf, fxLeds: fxLeds, cueLed: null, filterLed: null,
        knobs: [base, base + 2, base + 4, base + 6, base + 8],
        fader: 33 + 2 * n,
    };
};

var TraktorS8 = new TraktorKontrolMixerController({
    buttonsLength: 41,
    slidersLength: 109,
    sections: [TraktorS8Section(0), TraktorS8Section(1)],
    channels: [
        TraktorS8Channel(0, [20, 0x08], [20, 0x02], [[22, 0x20], [22, 0x40]], [0x20, 0x10], [46, 47]),
        TraktorS8Channel(1, [23, 0x20], [23, 0x08], [[25, 0x10], [25, 0x20]], [0x08, 0x04], [50, 51]),
        TraktorS8Channel(2, [20, 0x04], [20, 0x01], [[22, 0x08], [22, 0x10]], [0x80, 0x40], [44, 45]),
        TraktorS8Channel(3, [23, 0x40], [23, 0x10], [[25, 0x40], [25, 0x80]], [0x02, 0x01], [52, 53]),
    ],
    xfByte: 24,
    crossfader: 41,
    quantize: [25, 0x08],
    mic: [22, 0x04],
    mixerLeds: 73, snapLed: 48, quantLed: 49,
});
