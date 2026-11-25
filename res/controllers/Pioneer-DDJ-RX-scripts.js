// Pioneer DDJ-RX: what the RX adds to Mixxx's DDJ-SX mapping, which drives
// everything else (the RX sends the DDJ-SX2's messages). On the RX, note
// 0x35 on the deck channels is QUANTIZE, where the SX reads it as a jog
// touch without vinyl mode.

var PioneerDDJRX = {};

PioneerDDJRX.connections = [];

PioneerDDJRX.init = function() {
    for (var i = 0; i < 4; i++) {
        PioneerDDJRX.connections.push(
            engine.makeConnection("[Channel" + (i + 1) + "]", "quantize", PioneerDDJRX.quantizeLed));
        PioneerDDJRX.quantizeLed(engine.getValue("[Channel" + (i + 1) + "]", "quantize"), "[Channel" + (i + 1) + "]");
        PioneerDDJRX.connections.push(
            engine.makeConnection("[QuickEffectRack1_[Channel" + (i + 1) + "]]", "loaded_chain_preset", PioneerDDJRX.colorFxLights));
    }
};

PioneerDDJRX.shutdown = function() {
    for (var i = 0; i < 4; i++) {
        midi.sendShortMsg(0x90 + i, 0x35, 0x00);
    }
    Object.keys(PioneerDDJRX.colorFxButtons).forEach(function(control) {
        midi.sendShortMsg(0x96, parseInt(control), 0x00);
    });
};

PioneerDDJRX.quantizeLed = function(value, group) {
    var deck = parseInt(group.substring(8, 9)) - 1;
    midi.sendShortMsg(0x90 + deck, 0x35, value ? 0x7F : 0x00);
};

PioneerDDJRX.quantize = function(channel, control, value, status, group) {
    if (value) {
        script.toggleControl(group, "quantize");
    }
};

// SOUND COLOR FX buttons: each picks the Color FX the COLOR knobs play on
// every deck; pressing the lit one goes back to the filter. The buttons
// differ between the RX, RZ and RZX, so each mapping binds every button to
// the handler of its Color FX (numbers as in the engine's COLOR_FX: 1
// Filter, 2 Space, 3 Dub Echo, 4 Noise, 5 Crush, 6 Pitch). A button lights
// once it has been pressed (SHIFT + a button does the same, 8 or 16 higher).
PioneerDDJRX.colorFxButtons = {};

PioneerDDJRX.colorFx = function(preset) {
    return function(_channel, control, value) {
        if (!value) {
            return;
        }
        PioneerDDJRX.colorFxButtons[control] = preset;
        var current = engine.getValue("[QuickEffectRack1_[Channel1]]", "loaded_chain_preset");
        var next = current === preset ? 1 : preset;
        for (var i = 0; i < 4; i++) {
            engine.setValue("[QuickEffectRack1_[Channel" + (i + 1) + "]]", "loaded_chain_preset", next);
        }
        PioneerDDJRX.colorFxLights();
    };
};

PioneerDDJRX.colorFxFilter = PioneerDDJRX.colorFx(1);
PioneerDDJRX.colorFxSpace = PioneerDDJRX.colorFx(2);
PioneerDDJRX.colorFxDubEcho = PioneerDDJRX.colorFx(3);
PioneerDDJRX.colorFxNoise = PioneerDDJRX.colorFx(4);
PioneerDDJRX.colorFxCrush = PioneerDDJRX.colorFx(5);
PioneerDDJRX.colorFxPitch = PioneerDDJRX.colorFx(6);

PioneerDDJRX.colorFxLights = function() {
    var current = engine.getValue("[QuickEffectRack1_[Channel1]]", "loaded_chain_preset");
    Object.keys(PioneerDDJRX.colorFxButtons).forEach(function(control) {
        var preset = PioneerDDJRX.colorFxButtons[control];
        midi.sendShortMsg(0x96, parseInt(control), preset === current && preset !== 1 ? 0x7F : 0x00);
    });
};
