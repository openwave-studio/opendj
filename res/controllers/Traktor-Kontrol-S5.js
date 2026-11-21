// Traktor Kontrol S5, played as Traktor plays it: two deck sections and a
// four-channel mixer (see Traktor-Kontrol-S5-S8.js). openDJ's own mapping,
// written from the S5's published HID report layout and Native Instruments'
// manual. Its displays stay dark.
//
// That published layout is incomplete and largely follows the S8's, so this
// mapping is partial and partly a guess:
// - the browse and loop encoders' turns aren't in it, so they do nothing;
//   their pushes are where the D2 and S8 have them;
// - channel D's LOW and FILTER knobs aren't in it, so they do nothing;
// - the deck section lights are predicted from the D2's, not tested;
// - AUX, SNAP, the master tempo encoder and the browse ◄ button do nothing.
//
// Input report 0x01, buttons (30 bytes with the ID): the deck sections as
// on the S8 (left at bytes 4-9, right 12-17), mixer buttons in bytes 20-25
// as on the S8, touch strips at bytes 26-27 and 28-29.
// Input report 0x02, knobs and faders (79 bytes with the ID), 12 bits:
//   channel faders A-D 17-24, crossfader 25, FX knobs left 27-34, right
//   35-42; per channel A-D from 43, 53, 63, 73: gain, high, mid, low, filter.
// LED reports: 0x80 left and 0x81 right deck section (118 bytes each), 0x82
// mixer (73 bytes: CUE A-D 0-3, FILTER ON A-D 4-7, FX assign C1 44, C2 45,
// A1 46, A2 47, B1 48, B2 49, D1 50, D2 51, SNAP 52, QUANT 53).

var TraktorS5Section = function(side) {
    var o = 8 * side;
    var at = function(b, m) { return [b + o, m]; };
    return {
        bd: side === 1,
        buttons: {
            shift: at(9, 0x80), deck: at(8, 0x04), flux: at(8, 0x10),
            play: at(9, 0x10), cue: at(9, 0x20), sync: at(9, 0x40),
            hotcue: at(9, 0x08), loop: null, freeze: at(9, 0x02), remix: at(9, 0x01),
            browsePush: at(4, 0x08), loopPush: at(7, 0x10),
            fx: [at(4, 0x80), at(5, 0x04), at(5, 0x02), at(5, 0x01)],
            on: null,
            pads: [at(8, 0x08), at(8, 0x01), at(7, 0x01), at(7, 0x02),
                at(8, 0x20), at(8, 0x40), at(8, 0x80), at(8, 0x02)],
            assign: null,
        },
        encoders: null,
        strip: side ? 28 : 26,
        knobs: null, faders: null,
        fx: 27 + 8 * side,
        ledReport: 0x80 + side, ledLength: 118, bgr: false, deckLeds: null,
    };
};

var TraktorS5Channel = function(n, cue, filter, fx, fxLeds) {
    var base = 43 + 10 * n;
    return {
        cue: cue, filter: filter, fx: fx, xf: null, fxLeds: fxLeds, cueLed: n, filterLed: 4 + n,
        knobs: n === 3 ? [base, base + 2, base + 4, null, null]
            : [base, base + 2, base + 4, base + 6, base + 8],
        fader: 17 + 2 * n,
    };
};

var TraktorS5 = new TraktorKontrolMixerController({
    buttonsLength: 30,
    slidersLength: 79,
    sections: [TraktorS5Section(0), TraktorS5Section(1)],
    channels: [
        TraktorS5Channel(0, [20, 0x08], [20, 0x02], [[22, 0x20], [22, 0x40]], [46, 47]),
        TraktorS5Channel(1, [23, 0x20], [23, 0x08], [[25, 0x10], [25, 0x20]], [48, 49]),
        TraktorS5Channel(2, [20, 0x04], [20, 0x01], [[22, 0x08], [22, 0x10]], [44, 45]),
        TraktorS5Channel(3, [23, 0x40], [23, 0x10], [[25, 0x40], [25, 0x80]], [50, 51]),
    ],
    xfByte: null,
    crossfader: 25,
    quantize: [25, 0x08],
    mic: null,
    mixerLeds: 73, snapLed: 52, quantLed: 53,
});
