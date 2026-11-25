// Pioneer-all-in-one-script.js
// ****************************************************************************
// * Shared openDJ script for the Pioneer DJ / AlphaTheta all-in-one systems
// * driven in PC (MIDI) control mode: XDJ-RX, XDJ-RX2, XDJ-RR, XDJ-XZ,
// * XDJ-AERO, XDJ-R1, XDJ-AN and OMNIS-DUO. Each unit's own script calls
// * PioneerAllInOne.create() with what sets it apart; the .midi.xml files
// * bind the unit's messages to the functions made here.
// *
// * Written from the units' MIDI message lists, built on openDJ's XDJ-RX3
// * and OPUS-QUAD scripts (themselves from Mixxx's DDJ-400 and DDJ-FLX4
// * mappings: Warker, nschloe, dj3730, jusko, Robert904).
// ****************************************************************************
//
//  Every deck sends on its own MIDI channel (deck 1 on channel 1, and so
//  on), so each function finds its deck from the group the mapping gives it.
//
//  Options (all optional):
//    decks           2 or 4 (default 2).
//    mixerMidiMode   true: send B4 65 00 on start so the mixer sends MIDI and
//                    openDJ mixes; 7F on shutdown hands it back.
//    jogDisplay      true: the jog displays show the playhead and the cue
//                    point (channel 12, CC 0-7 with LSB +0x20, 0-359).
//    loadLights      true: LOAD flashes when a track loads (9B 00-03).
//    padModes        the notes of the pad mode buttons, on the deck channel:
//                    the one pressed lights, the others go dark.
//    padModeStatus   "deck" (default) or "pad": the mode buttons light on the
//                    deck's channel, or on its pad channel (padChannel).
//    padChannel      the pads' first MIDI channel, 0-based (deck 1).
//    colorFx         {CC or note: Color FX number} for the SOUND COLOR FX
//                    buttons (1 Filter, 2 Space, 3 Dub Echo, 4 Noise,
//                    5 Crush, 6 Pitch); colorFxStatus is their status byte.
//    beatFxEffects   {CC or note: loaded_effect} for the Beat FX selector.
//    beatFxStatus    the status byte of the BEAT FX ON/OFF light.
//    beatFxOnNumber  the CC or note of the BEAT FX ON/OFF light.
//    deckFx          true: the unit has an FX section on each deck (XDJ-AERO,
//                    XDJ-R1): FX 1 drives effect unit 1 on deck 1, FX 2
//                    effect unit 2 on deck 2.
//    deckFxEffects   {note: loaded_effect} for the deck FX buttons.
//    samplers        true: the pads' SAMPLER page plays samplers 1 to 16.

var PioneerAllInOne = {};

PioneerAllInOne.create = function(options) {
    const opts = options || {};
    const self = {};
    const deckCount = opts.decks || 2;

    self.decks = [];
    for (let i = 1; i <= deckCount; i++) {
        self.decks.push("[Channel" + i + "]");
    }

    const deckIndex = function(group) {
        const m = group.match(script.channelRegEx);
        return m ? parseInt(m[1]) - 1 : 0;
    };
    const deckStatus = function(group) {
        return 0x90 + deckIndex(group);
    };

    self.vinylMode = [true, true, true, true];
    self.loopAdjustIn = [false, false, false, false];
    self.alpha = 1.0 / 8;
    self.beta = self.alpha / 32;
    self.fastSeekScale = 75;
    self.loopAdjustMultiply = 50;
    self.quickJumpSize = 32;
    self.tempoRanges = [0.06, 0.10, 0.16, 1.00];
    self.tempoMSB = {};
    self.padMode = {};

    const pressed = function(fn) {
        return function(channel, control, value, status, group) {
            if (value > 0) {
                fn(channel, control, value, status, group);
            }
        };
    };
    const trigger = function(group, key) {
        engine.setValue(group, key, 1);
        engine.setValue(group, key, 0);
    };

    //
    // Init and shutdown
    //

    self.init = function() {
        if (opts.mixerMidiMode) {
            // openDJ mixes: the mixer sends MIDI rather than mixing itself.
            midi.sendShortMsg(0xB4, 0x65, 0x00);
        }
        self.decks.forEach(function(group) {
            engine.softTakeover(group, "rate", true);
            if (opts.loadLights) {
                engine.makeConnection(group, "track_loaded", self.trackLoadedLight);
            }
            if (opts.padModes && opts.padModes.length) {
                self.padMode[group] = opts.padModes[0];
                self.padModeLights(group);
            }
        });
        if (opts.samplers && engine.getValue("[App]", "num_samplers") < 16) {
            engine.setValue("[App]", "num_samplers", 16);
        }
        if (opts.beatFxEffects) {
            engine.softTakeover(self.beatFxSlot, "meta", true);
        }
        if (opts.colorFx) {
            self.decks.forEach(function(group) {
                engine.makeConnection("[QuickEffectRack1_" + group + "]", "loaded_chain_preset", self.colorFxLights);
            });
            self.colorFxLights();
        }
        if (opts.deckFx) {
            [1, 2].forEach(function(unit) {
                engine.setValue("[EffectRack1_EffectUnit" + unit + "]", "group_[Channel" + unit + "]_enable", 1);
            });
        }
        if (opts.jogDisplay) {
            self.jogDisplays = [[-1, -1], [-1, -1], [-1, -1], [-1, -1]];
            self.jogDisplayTimer = engine.beginTimer(40, self.jogDisplayUpdate);
        }
    };

    self.shutdown = function() {
        if (self.jogDisplayTimer) {
            engine.stopTimer(self.jogDisplayTimer);
        }
        self.decks.forEach(function(group, i) {
            if (opts.loadLights) {
                midi.sendShortMsg(0x9B, i, 0x00);
            }
            if (opts.jogDisplay) {
                self.sendJogDisplay(i, 0);
                self.sendJogDisplay(4 + i, 0x3FFF);
            }
        });
        if (opts.mixerMidiMode) {
            midi.sendShortMsg(0xB4, 0x65, 0x7F);
        }
    };

    //
    // Lights
    //

    self.trackLoadedLight = function(value, group) {
        midi.sendShortMsg(0x9B, deckIndex(group), value > 0 ? 0x7F : 0x00);
    };

    // A 14-bit value as two CCs on the jog display channel (12).
    self.sendJogDisplay = function(msb, value) {
        midi.sendShortMsg(0xBB, msb, (value >> 7) & 0x7F);
        midi.sendShortMsg(0xBB, msb + 0x20, value & 0x7F);
    };

    // The position bar (0 to 359 degrees) goes round once per turn of a
    // record at 33 1/3 rpm; the cue marker shows where the cue point sits on
    // that turn, hidden (0x3FFF) with no track.
    self.jogDisplayUpdate = function() {
        const degrees = function(seconds) {
            const turns = Math.max(0, seconds) * (100 / 3) / 60;
            return Math.floor((turns - Math.floor(turns)) * 360) % 360;
        };
        self.decks.forEach(function(group, i) {
            let bar = 0;
            let cue = 0x3FFF;
            if (engine.getValue(group, "track_loaded")) {
                bar = degrees(engine.getValue(group, "playposition") * engine.getValue(group, "duration"));
                // openDJ: cue_point is in seconds.
                const point = engine.getValue(group, "cue_point");
                if (point >= 0) {
                    cue = degrees(point);
                }
            }
            const last = self.jogDisplays[i];
            if (bar !== last[0]) {
                last[0] = bar;
                self.sendJogDisplay(i, bar);
            }
            if (cue !== last[1]) {
                last[1] = cue;
                self.sendJogDisplay(4 + i, cue);
            }
        });
    };

    //
    // Browser
    //

    // SHIFT + the browse knob zooms every deck's waveform.
    self.waveformZoom = function(_channel, _control, value) {
        const up = value < 0x40;
        self.decks.forEach(function(group) {
            script.triggerControl(group, up ? "waveform_zoom_up" : "waveform_zoom_down", 100);
        });
    };

    //
    // Transport, tempo and sync
    //

    self.play = pressed(function(_c, _n, _v, _s, group) {
        script.toggleControl(group, "play");
    });
    self.toStart = pressed(function(_c, _n, _v, _s, group) {
        trigger(group, "start");
    });
    self.keylock = pressed(function(_c, _n, _v, _s, group) {
        script.toggleControl(group, "keylock");
    });
    self.slip = pressed(function(_c, _n, _v, _s, group) {
        script.toggleControl(group, "slip_enabled");
    });
    self.reverse = pressed(function(_c, _n, _v, _s, group) {
        script.toggleControl(group, "reverse");
    });
    self.sync = pressed(function(_c, _n, _v, _s, group) {
        script.toggleControl(group, "sync_enabled");
    });
    // MASTER (and SYNC held or with SHIFT): this deck leads the others.
    self.master = pressed(function(_c, _n, _v, _s, group) {
        engine.setValue(group, "sync_leader", engine.getValue(group, "sync_leader") === 2 ? 0 : 2);
    });
    self.tempoReset = pressed(function(_c, _n, _v, _s, group) {
        trigger(group, "rate_set_zero");
    });
    // QUANTIZE has one button for every deck.
    self.quantize = pressed(function() {
        const on = !engine.getValue(self.decks[0], "quantize");
        self.decks.forEach(function(group) {
            engine.setValue(group, "quantize", on);
        });
    });

    // TEMPO RANGE: ±6, ±10, ±16 %, WIDE.
    self.tempoRange = pressed(function(_c, _n, _v, _s, group) {
        const curr = engine.getValue(group, "rateRange");
        let idx = 0;
        for (let i = 0; i < self.tempoRanges.length; i++) {
            if (Math.abs(curr - self.tempoRanges[i]) < 0.001) {
                idx = (i + 1) % self.tempoRanges.length;
                break;
            }
        }
        engine.setValue(group, "rateRange", self.tempoRanges[idx]);
    });
    // One button for each range (XDJ-R1).
    [6, 10, 16, 100].forEach(function(percent, i) {
        self["tempoRange" + percent] = pressed(function(_c, _n, _v, _s, group) {
            engine.setValue(group, "rateRange", self.tempoRanges[i]);
        });
    });

    // Units on older firmware send the MSB alone (XDJ-RX list ver. 1.00), so
    // it moves the tempo by itself; the LSB that may follow refines it.
    self.tempoSliderMSB = function(_channel, _control, value, _status, group) {
        self.tempoMSB[group] = value;
        engine.setValue(group, "rate", 1 - ((value << 7) / 0x2000));
    };
    // "-" side 0, "+" side 16383.
    self.tempoSliderLSB = function(_channel, _control, value, _status, group) {
        const fullValue = ((self.tempoMSB[group] || 0) << 7) + value;
        engine.setValue(group, "rate", 1 - (fullValue / 0x2000));
    };

    // The needle strip or touch display: jumps there while the deck is
    // paused.
    self.needle = function(_channel, _control, value, _status, group) {
        if (value > 0 && !engine.getValue(group, "play")) {
            engine.setValue(group, "playposition", value / 0x7F);
        }
    };

    // TRACK SEARCH jumps 32 beats; SEARCH and BEAT JUMP by the deck's beat
    // jump size.
    self.quickJumpForward = pressed(function(_c, _n, _v, _s, group) {
        engine.setValue(group, "beatjump", self.quickJumpSize);
    });
    self.quickJumpBack = pressed(function(_c, _n, _v, _s, group) {
        engine.setValue(group, "beatjump", -self.quickJumpSize);
    });
    self.searchForward = pressed(function(_c, _n, _v, _s, group) {
        engine.setValue(group, "beatjump", engine.getValue(group, "beatjump_size"));
    });
    self.searchBack = pressed(function(_c, _n, _v, _s, group) {
        engine.setValue(group, "beatjump", -engine.getValue(group, "beatjump_size"));
    });

    //
    // Jog wheels
    //

    // VINYL / JOG MODE: scratching on touch, or bending; the button shows
    // which.
    self.jogMode = pressed(function(_c, control, _v, status, group) {
        const i = deckIndex(group);
        self.vinylMode[i] = !self.vinylMode[i];
        midi.sendShortMsg(status, control, self.vinylMode[i] ? 0x7F : 0x00);
    });

    self.jogTurn = function(_channel, _control, value, _status, group) {
        const i = deckIndex(group);
        const delta = value - 64;
        if (self.loopAdjustIn[i] && engine.getValue(group, "loop_enabled") > 0) {
            engine.setValue(group, "loop_start_position",
                delta * self.loopAdjustMultiply + engine.getValue(group, "loop_start_position"));
            return;
        }
        if (engine.isScratching(i + 1)) {
            engine.scratchTick(i + 1, delta);
        } else {
            engine.setValue(group, "jog_bend", delta);
        }
    };

    // The side of the platter only bends.
    self.jogBend = function(_channel, _control, value, _status, group) {
        engine.setValue(group, "jog_bend", value - 64);
    };

    self.jogSearch = function(_channel, _control, value, _status, group) {
        engine.setValue(group, "jog_search", (value - 64) * self.fastSeekScale);
    };

    self.jogTouch = function(_channel, _control, value, _status, group) {
        const i = deckIndex(group);
        if (self.loopAdjustIn[i]) {
            return;
        }
        if (value !== 0 && self.vinylMode[i]) {
            engine.scratchEnable(i + 1, 720, 33 + 1 / 3, self.alpha, self.beta);
        } else {
            engine.scratchDisable(i + 1);
        }
    };

    //
    // Loops
    //

    // LOOP IN held: the jog moves the loop start.
    self.loopAdjust = pressed(function(_c, _n, _v, _s, group) {
        const i = deckIndex(group);
        self.loopAdjustIn[i] = engine.getValue(group, "loop_enabled") > 0 && !self.loopAdjustIn[i];
    });

    self.reloop = pressed(function(_c, _n, _v, _s, group) {
        trigger(group, "reloop_toggle");
        self.loopAdjustIn[deckIndex(group)] = false;
    });

    // CALL ◀ ▶ halve and double the loop playing.
    self.loopHalve = pressed(function(_c, _n, _v, _s, group) {
        trigger(group, "loop_halve");
    });
    self.loopDouble = pressed(function(_c, _n, _v, _s, group) {
        trigger(group, "loop_double");
    });

    // The BEAT LOOP knob: turning halves (left) or doubles (right) the loop
    // size and the loop playing; ±1 to ±30, 98 to 127 to the left.
    self.beatLoopTurn = function(_channel, _control, value, _status, group) {
        const steps = value >= 0x40 ? value - 0x80 : value;
        if (steps === 0) {
            return;
        }
        const size = engine.getValue(group, "beatloop_size") * (steps > 0 ? 2 : 0.5);
        if (size >= 0.03125 && size <= 512) {
            engine.setValue(group, "beatloop_size", size);
        }
        if (engine.getValue(group, "loop_enabled")) {
            trigger(group, steps > 0 ? "loop_double" : "loop_halve");
        }
    };

    // Pressing it: a loop of that size, or out of the loop playing.
    self.beatLoopPressed = pressed(function(_c, _n, _v, _s, group) {
        trigger(group, engine.getValue(group, "loop_enabled") ? "reloop_toggle" : "beatloop_activate");
    });

    //
    // Pads
    //

    // The pad mode buttons: the unit changes what its pads send by itself;
    // the script only lights the mode in use.
    self.padModeLights = function(group) {
        if (!opts.padModes) {
            return;
        }
        const status = opts.padModeStatus === "pad"
            ? 0x90 + opts.padChannel + deckIndex(group)
            : deckStatus(group);
        opts.padModes.forEach(function(note) {
            midi.sendShortMsg(status, note, note === self.padMode[group] ? 0x7F : 0x00);
        });
    };
    self.padModePressed = pressed(function(_c, control, _v, _s, group) {
        if (opts.padModes && opts.padModes.indexOf(control) >= 0) {
            self.padMode[group] = control;
            self.padModeLights(group);
        }
    });

    // BEAT JUMP pads: ◀ ▶ by 1, 2, 4 and 8 beats on the top row and 16,
    // 32, 64 and 128 with SHIFT.
    const jumps = [-1, 1, -2, 2, -4, 4, -8, 8];
    // PAD FX pads: loop rolls of 1/16 to 8 beats.
    const rolls = ["0.0625", "0.125", "0.25", "0.5", "1", "2", "4", "8"];
    // AUTO LOOP pads: beat loops of 1/4 to 32 beats.
    const loops = ["0.25", "0.5", "1", "2", "4", "8", "16", "32"];

    for (let pad = 1; pad <= 8; pad++) {
        const n = pad - 1;
        self["beatJumpPad" + pad] = pressed(function(_c, _no, _v, _s, group) {
            engine.setValue(group, "beatjump", jumps[n]);
        });
        self["beatJumpShiftPad" + pad] = pressed(function(_c, _no, _v, _s, group) {
            engine.setValue(group, "beatjump", jumps[n] * 16);
        });
        self["rollPad" + pad] = function(_channel, _control, value, _status, group) {
            engine.setValue(group, "beatlooproll_" + rolls[n] + "_activate", value > 0 ? 1 : 0);
        };
        self["loopPad" + pad] = pressed(function(_c, _no, _v, _s, group) {
            trigger(group, "beatloop_" + loops[n] + "_toggle");
        });
        // SAMPLER pads: samplers 1-8 on decks 1 and 3, 9-16 on decks 2 and 4.
        const sampler = function(group) {
            return "[Sampler" + (pad + (deckIndex(group) % 2) * 8) + "]";
        };
        self["samplerPad" + pad] = pressed(function(_c, _no, _v, _s, group) {
            const s = sampler(group);
            if (engine.getValue(s, "track_loaded")) {
                trigger(s, "cue_gotoandplay");
            } else {
                trigger(s, "LoadSelectedTrack");
            }
        });
        self["samplerStopPad" + pad] = pressed(function(_c, _no, _v, _s, group) {
            const s = sampler(group);
            if (engine.getValue(s, "play")) {
                trigger(s, "stop");
            } else {
                trigger(s, "eject");
            }
        });
    }

    //
    // Mixer
    //

    // CH CUE: the group is the channel.
    self.pfl = pressed(function(_c, _n, _v, _s, group) {
        script.toggleControl(group, "pfl");
    });

    // CROSSFADER ASSIGN: A (0), THRU (0x40) or B (0x7F).
    self.crossfaderAssign = function(_channel, _control, value, _status, group) {
        engine.setValue(group, "orientation", value < 0x20 ? 0 : (value > 0x60 ? 2 : 1));
    };

    // SOUND COLOR FX buttons: each picks what the COLOR knobs play; pressing
    // the lit one goes back to the filter.
    self.colorFxPressed = pressed(function(_c, control) {
        const preset = opts.colorFx && opts.colorFx[control];
        if (preset === undefined) {
            return;
        }
        const current = engine.getValue("[QuickEffectRack1_[Channel1]]", "loaded_chain_preset");
        const next = current === preset ? 1 : preset;
        self.decks.forEach(function(group) {
            engine.setValue("[QuickEffectRack1_" + group + "]", "loaded_chain_preset", next);
        });
    });

    self.colorFxLights = function() {
        if (!opts.colorFx) {
            return;
        }
        const current = engine.getValue("[QuickEffectRack1_[Channel1]]", "loaded_chain_preset");
        Object.keys(opts.colorFx).forEach(function(control) {
            const lit = opts.colorFx[control] === current && current !== 1;
            midi.sendShortMsg(opts.colorFxStatus || 0xB4, parseInt(control), lit ? 0x7F : 0x00);
        });
    };

    //
    // BEAT FX: openDJ's Beat FX unit
    //

    self.beatFxUnit = "[EffectRack1_EffectUnit5]";
    self.beatFxSlot = "[EffectRack1_EffectUnit5_Effect1]";

    // The effect buttons (or selector positions) load the nearest openDJ
    // effect (loaded_effect: 1 Echo, 2 Reverb, 3 Filter, 4 Flanger,
    // 6 Phaser, 7 Gate, 8 Roll, 10 Brake, 12 Spiral).
    self.beatFxSelect = pressed(function(_c, control) {
        const effect = opts.beatFxEffects && opts.beatFxEffects[control];
        if (effect !== undefined) {
            engine.setValue(self.beatFxSlot, "loaded_effect", effect);
        }
    });

    self.beatFxLevel = function(_channel, _control, value) {
        engine.setParameter(self.beatFxSlot, "meta", value / 0x7F);
    };

    self.beatFxOnOff = pressed(function() {
        script.toggleControl(self.beatFxSlot, "enabled");
    });

    self.beatFxBeatsDown = pressed(function() {
        trigger(self.beatFxSlot, "beats_halve");
    });
    self.beatFxBeatsUp = pressed(function() {
        trigger(self.beatFxSlot, "beats_double");
    });
    self.beatFxTap = pressed(function() {
        trigger(self.beatFxUnit, "bpm_tap");
    });

    //
    // Deck FX (XDJ-AERO, XDJ-R1): the group is the effect unit.
    //

    // An effect button loads its effect and turns it on; pressing the one
    // playing turns it off.
    self.deckFxPressed = pressed(function(_c, control, _v, _s, group) {
        const effect = opts.deckFxEffects && opts.deckFxEffects[control];
        if (effect === undefined) {
            return;
        }
        const slot = group.replace("]", "_Effect1]");
        if (engine.getValue(slot, "loaded_effect") === effect && engine.getValue(slot, "enabled")) {
            engine.setValue(slot, "enabled", 0);
        } else {
            engine.setValue(slot, "loaded_effect", effect);
            engine.setValue(slot, "enabled", 1);
        }
    });

    self.deckFxLevel = function(_channel, _control, value, _status, group) {
        engine.setParameter(group.replace("]", "_Effect1]"), "meta", value / 0x7F);
    };

    // The BEAT knob: ±1 to ±30, 98 to 127 to the left.
    self.deckFxBeats = function(_channel, _control, value, _status, group) {
        const steps = value >= 0x40 ? value - 0x80 : value;
        if (steps !== 0) {
            trigger(group.replace("]", "_Effect1]"), steps > 0 ? "beats_double" : "beats_halve");
        }
    };

    self.deckFxTap = pressed(function(_c, _n, _v, _s, group) {
        trigger(group, "bpm_tap");
    });

    return self;
};
