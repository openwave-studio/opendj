// Hercules DJControl Inpulse 300 MK2: runs after the Inpulse 300 script
// (Hercules-DJControl-Inpulse-300-script.js), which does the rest. The MK2
// sends the same notes as the Inpulse 300; LOOP ON took the VINYL button's
// note, so vinyl mode moved to SHIFT + SLIP, and SHIFT + Q picks the tempo
// fader's range.

var DJCi300MK2 = {};

DJCi300MK2.rateRanges = [0.08, 0.16, 0.5];

DJCi300MK2.init = function() {
    // The Inpulse 300 script lights VINYL (now LOOP ON) on start; vinyl mode
    // shows on SLIP's SHIFT light instead.
    midi.sendShortMsg(0x91, 0x03, 0x00);
    midi.sendShortMsg(0x92, 0x03, 0x00);
    midi.sendShortMsg(0x94, 0x01, 0x7F);
    midi.sendShortMsg(0x95, 0x01, 0x7F);
};

// SHIFT + Q: the next tempo range (8 %, 16 %, 50 %).
DJCi300MK2.tempoRange = function(_channel, _control, value, _status, group) {
    if (!value) {
        return;
    }
    var ranges = DJCi300MK2.rateRanges;
    var now = engine.getValue(group, "rateRange");
    var next = ranges[0];
    for (var i = 0; i < ranges.length; i++) {
        if (ranges[i] > now + 0.001) {
            next = ranges[i];
            break;
        }
    }
    engine.setValue(group, "rateRange", next);
};

DJCi300MK2.shutdown = function() {};
