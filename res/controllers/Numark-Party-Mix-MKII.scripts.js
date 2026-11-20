// Numark Party Mix MKII and Party Mix Live, on top of the first Party Mix's
// script (Numark-Party-Mix.scripts.js, loaded first). They share its layout,
// but the pad mode buttons send other notes, the jog wheels are touch
// sensitive instead of having a SCRATCH button, and the pads send a second
// set of notes (0x1C-0x1F) for deleting cues and stopping samples.

NumarkPartyMix.PadModeControls = {
    HOTCUE: 0x00,
    LOOP: 0x0E,
    SAMPLER: 0x0B,
    EFX: 0x0F,
};

NumarkPartyMix.initFirstModel = NumarkPartyMix.init;

NumarkPartyMix.init = function(id, debugging) {
    NumarkPartyMix.initFirstModel(id, debugging);
    for (let i = 0; i < 2; i++) {
        const deck = NumarkPartyMix.deck[i];
        const deckNumber = i + 1;

        // Touching the platter scratches; letting go plays on.
        deck.wheelTouch = function(_channel, _control, value) {
            deck.scratchModeEnabled = value > 0;
            if (!deck.scratchModeEnabled) {
                engine.scratchDisable(deckNumber);
            }
        };

        // The pads' second notes: delete the hot cue, or stop the sample.
        const pads = deck.padSection;
        pads.padPressSecond = function(_channel, control, value, _status, group) {
            if (value === 0) {
                return;
            }
            const i = control - 0x1C;
            if (pads.currentMode === pads.modes[NumarkPartyMix.PadModeControls.HOTCUE]) {
                engine.setValue(group, `hotcue_${i + 1}_clear`, 1);
            } else if (pads.currentMode === pads.modes[NumarkPartyMix.PadModeControls.SAMPLER]) {
                engine.setValue(`[Sampler${i + 1 + (deckNumber - 1) * 4}]`, "stop", 1);
            }
        };
    }
};
