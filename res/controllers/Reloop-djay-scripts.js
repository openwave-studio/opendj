// Shared runtime for openDJ's Reloop mappings that are built from the
// control lists djay Pro's own mappings use. Each controller's data file
// (Reloop-<model>-data.js) lists, for every MIDI message, the rows of
// actions it does, the condition each row needs (the deck's pad mode, a
// stems EQ switch, which deck the FX section is on) and how its light
// shows; this file does the actions and keeps the lights up to date.
//
// A row is {p: action path, c: [variable, "==" or "!=", value],
// t: "rotary" | "rotary-64", f: flipped, h: "hold" | "on" | "off",
// led: [off, on] or true for the pad mode's colours, o: [status, number]
// for a light on another message}. Paths read like djay's: "deck1.playPause",
// "mixer.crossfade", "sampler.deck1.player3.playing", "set modifier1 4".

var ReloopDJ = {};

ReloopDJ.loopSize = function(text) {
    // "025" is 0.25, "00625" 0.0625, "16" 16.
    return text.charAt(0) === "0" && text.length > 1 ? Number("0." + text.substr(1)) : Number(text);
};

ReloopDJ.controller = function(data) {
    var c = {data: data, vars: {}, held: {}, sent: {}, vinyl: {}, timer: 0};

    c.init = function() {
        c.vars = {};
        Object.keys(data.vars || {}).forEach(function(v) {
            c.vars[v] = data.vars[v];
        });
        for (var d = 1; d <= data.decks; d++) {
            c.vinyl[d] = true;
        }
        (data.pickup || []).forEach(function(g) {
            engine.softTakeover(g, "rate", true);
        });
        c.sent = {};
        c.refresh();
        c.timer = engine.beginTimer(60, c.refresh);
    };

    c.shutdown = function() {
        if (c.timer) {
            engine.stopTimer(c.timer);
            c.timer = 0;
        }
        Object.keys(c.sent).forEach(function(k) {
            var s = k.split(":");
            midi.sendShortMsg(Number(s[0]), Number(s[1]), 0);
        });
    };

    c.holds = function(row) {
        if (!row.c) {
            return true;
        }
        var v = c.vars[row.c[0]] || 0;
        return row.c[1] === "!=" ? v !== row.c[2] : v === row.c[2];
    };

    // One MIDI message: every row whose condition holds runs.
    c.input = function(_channel, control, value, status, _group) {
        var pitchBend = (status & 0xF0) === 0xE0;
        var key = status * 256 + (pitchBend ? 0 : control);
        var rows = data.rows[key];
        if (!rows) {
            return;
        }
        if ((status & 0xF0) === 0x80) {
            value = 0;
        }
        var abs = pitchBend ? ((value << 7) | control) / 16383 : value / 127;
        (data.follow || []).forEach(function(f) {
            // [variable, channel, value]: the FX section follows the deck
            // last touched on that side.
            if ((status & 0x0F) === f[1]) {
                c.vars[f[0]] = f[2];
            }
        });
        var todo = rows.filter(c.holds);
        todo.forEach(function(row) {
            var delta = 0;
            if (row.t === "rotary") {
                delta = value < 64 ? value : value - 128;
            } else if (row.t === "rotary-64") {
                delta = value - 64;
            }
            if (row.f) {
                delta = -delta;
            }
            var ctx = {
                row: row,
                key: key,
                value: value,
                v: row.f && !row.t ? 1 - abs : abs,
                delta: delta,
                press: value > 0,
            };
            c.held[row.p] = ctx.press;
            c.run(row.p, ctx);
        });
    };

    c.toggle = function(group, name) {
        engine.setValue(group, name, engine.getValue(group, name) ? 0 : 1);
    };

    // A button: on press, or held for as long as it's down.
    c.button = function(ctx, group, name) {
        var h = ctx.row.h;
        if (h === "hold") {
            engine.setValue(group, name, ctx.press ? 1 : 0);
        } else if (ctx.press) {
            if (h === "on") {
                engine.setValue(group, name, 1);
            } else if (h === "off") {
                engine.setValue(group, name, 0);
            } else {
                c.toggle(group, name);
            }
        }
    };

    c.push = function(ctx, group, name) {
        if (ctx.press) {
            engine.setValue(group, name, 1);
        }
    };

    c.stemSolo = function(deck, stem) {
        var prefix = "[Channel" + deck + "_Stem";
        var alone = !engine.getValue(prefix + stem + "]", "mute");
        for (var i = 1; i <= 4; i++) {
            if (i !== stem && !engine.getValue(prefix + i + "]", "mute")) {
                alone = false;
            }
        }
        for (var j = 1; j <= 4; j++) {
            engine.setValue(prefix + j + "]", "mute", alone || j === stem ? 0 : 1);
        }
    };

    c.fx = function(deck, n) {
        return "[EffectRack1_EffectUnit" + deck + (n ? "_Effect" + n : "") + "]";
    };

    c.metaStep = function(fx, step) {
        var v = engine.getParameter(fx, "meta") + step;
        engine.setParameter(fx, "meta", Math.max(0, Math.min(1, v)));
    };

    c.run = function(path, ctx) {
        var m, s = path.split(".");
        if (path.indexOf("set ") === 0) {
            // "set modifier1 4": a pad mode button.
            var w = path.split(" ");
            if (ctx.press) {
                c.vars[w[1]] = Number(w[2]);
            }
            return;
        }
        if (path.indexOf("flip ") === 0) {
            if (ctx.press) {
                var name = path.split(" ")[1];
                c.vars[name] = c.vars[name] ? 0 : 1;
            }
            return;
        }
        if (s[0] === "mixer") {
            c.mixer(s[1], ctx);
            return;
        }
        if (s[0] === "library") {
            c.library(s[1], ctx);
            return;
        }
        if (s[0] === "sampler") {
            c.sampler(s, ctx);
            return;
        }
        m = /^deck(\d+)$/.exec(s[0]);
        if (m && s[1]) {
            c.deck(Number(m[1]), s[1], ctx);
        }
    };

    c.mixer = function(name, ctx) {
        var m;
        if (name === "crossfade") {
            engine.setParameter("[Master]", "crossfader", ctx.v);
        } else if (name === "masterLevel") {
            engine.setParameter("[Master]", "gain", ctx.v);
        } else if (name === "monitorMix") {
            engine.setParameter("[Master]", "headMix", ctx.v);
        } else if (name === "monitorLevel") {
            engine.setParameter("[Master]", "headGain", ctx.v);
        } else if ((m = /^lineVolume(\d)$/.exec(name))) {
            engine.setParameter("[Channel" + m[1] + "]", "volume", ctx.v);
        } else if ((m = /^monitorActive(\d)$/.exec(name))) {
            c.button(ctx, "[Channel" + m[1] + "]", "pfl");
        } else if ((m = /^crossfadeAssignment(\d)(Left|Through|Right)$/.exec(name))) {
            if (ctx.press) {
                engine.setValue("[Channel" + m[1] + "]", "orientation",
                    {Left: 0, Through: 1, Right: 2}[m[2]]);
            }
        }
    };

    c.library = function(name, ctx) {
        var m;
        if (name === "libraryRotary") {
            engine.setValue("[Library]", "MoveVertical", -ctx.delta);
        } else if (name === "sectionRotary") {
            engine.setValue("[Playlist]", "SelectPlaylist", -ctx.delta);
        } else if (name === "toggleLibrarySource") {
            c.push(ctx, "[Library]", "GoToItem");
        } else if (name === "libraryBack") {
            c.push(ctx, "[Library]", "MoveFocusBackward");
        } else if (name === "nextSection" || name === "nextSource") {
            c.push(ctx, "[Library]", "MoveFocusForward");
        } else if ((m = /^load(\d)$/.exec(name))) {
            c.push(ctx, "[Channel" + m[1] + "]", "LoadSelectedTrack");
        }
    };

    c.sampler = function(s, ctx) {
        // sampler.deck1.player3.playing
        var n = s[2] ? Number(s[2].replace("player", "")) : 0;
        if (!n) {
            if (s[1] === "volume") {
                for (var i = 1; i <= 8; i++) {
                    engine.setParameter("[Sampler" + i + "]", "volume", ctx.v);
                }
            }
            return;
        }
        var g = "[Sampler" + n + "]";
        if (s[3] === "playing") {
            c.push(ctx, g, "cue_gotoandplay");
        } else if (s[3] === "paused") {
            c.push(ctx, g, "cue_gotoandstop");
        } else if (s[3] === "volume") {
            engine.setParameter(g, "volume", ctx.v);
        }
    };

    c.deck = function(d, name, ctx) {
        var g = "[Channel" + d + "]";
        var m, n;
        switch (name) {
        case "playPause":
            c.button(ctx, g, "play");
            return;
        case "cue":
            engine.setValue(g, "cue_default", ctx.press ? 1 : 0);
            return;
        case "gotoZero":
            c.push(ctx, g, "start");
            return;
        case "gotoBeginningAndPlay":
            c.push(ctx, g, "start_play");
            return;
        case "gotoBeginningAndPause":
            c.push(ctx, g, "start_stop");
            return;
        case "bpmSync":
            c.button(ctx, g, "sync_enabled");
            return;
        case "bpmTap":
            c.push(ctx, g, "bpm_tap");
            return;
        case "fxBpmTap":
            c.push(ctx, g, "bpm_tap");
            return;
        case "censor":
            engine.setValue(g, "reverseroll", ctx.press ? 1 : 0);
            return;
        case "reverse":
            c.button(ctx, g, "reverse");
            return;
        case "deckSlipToggle":
            c.button(ctx, g, "slip_enabled");
            return;
        case "downBeat":
            c.push(ctx, g, "beats_translate_curpos");
            return;
        case "key":
            c.button(ctx, g, "keylock");
            return;
        case "matchKey":
            c.push(ctx, g, "sync_key");
            return;
        case "resetPitch":
            c.push(ctx, g, "reset_key");
            return;
        case "resetSpeed":
            if (ctx.press) {
                engine.setValue(g, "rate", 0);
            }
            return;
        case "pitchMinus":
            c.push(ctx, g, "pitch_down");
            return;
        case "pitchPlus":
            c.push(ctx, g, "pitch_up");
            return;
        case "speed":
            engine.setParameter(g, "rate", ctx.v);
            return;
        case "tempoSliderRangeNext":
            if (ctx.press) {
                var ranges = [0.08, 0.16, 0.5, 1];
                var now = engine.getValue(g, "rateRange");
                var next = ranges[0];
                for (var r = 0; r < ranges.length; r++) {
                    if (ranges[r] > now + 0.001) {
                        next = ranges[r];
                        break;
                    }
                }
                engine.setValue(g, "rateRange", next);
            }
            return;
        case "jogPitchBendModeToggle":
            if (ctx.row.h === "hold") {
                c.vinyl[d] = !ctx.press;
            } else if (ctx.press) {
                c.vinyl[d] = !c.vinyl[d];
            }
            return;
        case "scratchingMode":
            // Touching the jog's top.
            if (ctx.press && c.vinyl[d]) {
                engine.scratchEnable(d, 800, 33 + 1 / 3, 1 / 8, 1 / 8 / 32);
            } else if (!ctx.press) {
                engine.scratchDisable(d);
            }
            return;
        case "scratchingMove":
            if (engine.isScratching(d)) {
                engine.scratchTick(d, ctx.delta);
            } else {
                engine.setValue(g, "jog", ctx.delta / 4);
            }
            return;
        case "jogSeekMove":
            engine.setValue(g, "jog", ctx.delta * 4);
            return;
        case "skipBackward":
            c.push(ctx, g, "beatjump_backward");
            return;
        case "skipForward":
            c.push(ctx, g, "beatjump_forward");
            return;
        case "skipDurationUp":
        case "skipDurationDown":
            if (ctx.press) {
                var size = engine.getValue(g, "beatjump_size");
                size = name === "skipDurationUp" ? size * 2 : size / 2;
                engine.setValue(g, "beatjump_size", Math.max(1 / 32, Math.min(512, size)));
            }
            return;
        case "skipRotary":
            if (ctx.delta) {
                engine.setValue(g, ctx.delta > 0 ? "beatjump_forward" : "beatjump_backward", 1);
            }
            return;
        case "autoLoopDurationRotary":
            if (ctx.delta) {
                engine.setValue(g, ctx.delta > 0 ? "loop_double" : "loop_halve", 1);
            }
            return;
        case "autoLoopDurationHalf":
            c.push(ctx, g, "loop_halve");
            return;
        case "autoLoopDurationDouble":
            c.push(ctx, g, "loop_double");
            return;
        case "autoLoopOnOff":
            if (ctx.press) {
                engine.setValue(g, engine.getValue(g, "loop_enabled") ? "reloop_toggle" : "beatloop_activate", 1);
            }
            return;
        case "loopIn":
            engine.setValue(g, "loop_in", ctx.press ? 1 : 0);
            return;
        case "loopOut":
            engine.setValue(g, "loop_out", ctx.press ? 1 : 0);
            return;
        case "reloop":
            c.push(ctx, g, "reloop_toggle");
            return;
        case "filter":
            engine.setParameter("[QuickEffectRack1_" + g + "]", "super1", ctx.v);
            return;
        case "gain":
            engine.setParameter(g, "pregain", ctx.v);
            return;
        case "highEQ":
        case "midEQ":
        case "lowEQ":
            engine.setParameter("[EqualizerRack1_" + g + "_Effect1]",
                "parameter" + {lowEQ: 1, midEQ: 2, highEQ: 3}[name], ctx.v);
            return;
        case "loadTrack":
            c.push(ctx, g, "LoadSelectedTrack");
            return;
        case "unmixerEQDrums":
            engine.setParameter("[Channel" + d + "_Stem1]", "volume", ctx.v);
            return;
        case "unmixerEQHarmonic":
            engine.setParameter("[Channel" + d + "_Stem2]", "volume", ctx.v);
            engine.setParameter("[Channel" + d + "_Stem3]", "volume", ctx.v);
            return;
        case "unmixerEQAcapella":
            engine.setParameter("[Channel" + d + "_Stem4]", "volume", ctx.v);
            return;
        }
        if ((m = /^hotcue(\d+)$/.exec(name))) {
            engine.setValue(g, "hotcue_" + m[1] + "_activate", ctx.press ? 1 : 0);
        } else if ((m = /^hotcueLoop(\d+)$/.exec(name))) {
            // Jump to the hot cue and loop from there.
            engine.setValue(g, "hotcue_" + m[1] + "_activate", ctx.press ? 1 : 0);
            if (ctx.press && !engine.getValue(g, "loop_enabled")) {
                engine.setValue(g, "beatloop_activate", 1);
            }
        } else if ((m = /^clearCue(\d+)$/.exec(name))) {
            c.push(ctx, g, "hotcue_" + m[1] + "_clear");
        } else if ((m = /^pitchPlay(\d+)$/.exec(name))) {
            engine.setValue(g, "keyboard_" + m[1] + "_activate", ctx.press ? 1 : 0);
        } else if ((m = /^pitchPlayCue(\d+)$/.exec(name))) {
            if (ctx.press) {
                engine.setValue(g, "hotcue_focus", Number(m[1]));
            }
        } else if ((m = /^autoLoop([\d]+)$/.exec(name))) {
            c.push(ctx, g, "beatloop_" + ReloopDJ.loopSize(m[1]) + "_toggle");
        } else if ((m = /^bounceLoop([\d]+)$/.exec(name))) {
            engine.setValue(g, "beatlooproll_" + ReloopDJ.loopSize(m[1]) + "_activate", ctx.press ? 1 : 0);
        } else if ((m = /^slice(\d)$/.exec(name))) {
            if (ctx.press) {
                engine.setValue(g, "slicer_enabled", 1);
            }
            engine.setValue(g, "slicer_" + m[1] + "_activate", ctx.press ? 1 : 0);
        } else if ((m = /^instantFx(\d)$/.exec(name))) {
            n = Number(m[1]);
            if (!ctx.press) {
                return;
            }
            if (n <= 3) {
                c.toggle(c.fx(d, n), "enabled");
            } else if (n === 4) {
                engine.setValue(c.fx(d), "enabled", 1);
                c.toggle(c.fx(d), "group_" + g + "_enable");
            } else if (n <= 7) {
                engine.setValue(c.fx(d, n - 4), "next_effect", 1);
            }
        } else if ((m = /^stem(\d)$/.exec(name))) {
            if (ctx.press) {
                c.toggle("[Channel" + d + "_Stem" + m[1] + "]", "mute");
            }
        } else if ((m = /^stemSolo(\d)$/.exec(name))) {
            if (ctx.press) {
                c.stemSolo(d, Number(m[1]));
            }
        } else if ((m = /^fx(\d)Enabled$/.exec(name))) {
            var fx = c.fx(d, m[1]);
            c.button(ctx, fx, "enabled");
            if (engine.getValue(fx, "enabled")) {
                engine.setValue(c.fx(d), "enabled", 1);
                engine.setValue(c.fx(d), "group_" + g + "_enable", 1);
            }
        } else if ((m = /^fx(\d)SelectNext$/.exec(name))) {
            c.push(ctx, c.fx(d, m[1]), "next_effect");
        } else if ((m = /^fx(\d)SelectPrev$/.exec(name))) {
            c.push(ctx, c.fx(d, m[1]), "prev_effect");
        } else if (/^fx\dWetDryValue$/.test(name)) {
            engine.setParameter(c.fx(d), "mix", ctx.v);
        } else if ((m = /^fx(\d)ParameterValue$/.exec(name))) {
            if (ctx.row.t) {
                c.metaStep(c.fx(d, m[1]), ctx.delta / 64);
            } else {
                engine.setParameter(c.fx(d, m[1]), "meta", ctx.v);
            }
        } else if ((m = /^fx(\d)ParameterValue(Minus|Plus)$/.exec(name))) {
            if (ctx.press) {
                c.metaStep(c.fx(d, m[1]), m[2] === "Plus" ? 1 / 16 : -1 / 16);
            }
        } else if ((m = /^fx(\d)ParameterDefaultValue$/.exec(name))) {
            if (ctx.press) {
                engine.setParameter(c.fx(d, m[1]), "meta", 0.5);
            }
        }
    };

    // Whether a row's light is on.
    c.lit = function(path) {
        var s = path.split(".");
        var m, w;
        if (path.indexOf("set ") === 0) {
            w = path.split(" ");
            return (c.vars[w[1]] || 0) === Number(w[2]);
        }
        if (path.indexOf("flip ") === 0) {
            return !!c.vars[path.split(" ")[1]];
        }
        if (s[0] === "mixer") {
            m = /^monitorActive(\d)$/.exec(s[1]);
            if (m) {
                return engine.getValue("[Channel" + m[1] + "]", "pfl") > 0;
            }
            m = /^crossfadeAssignment(\d)(Left|Through|Right)$/.exec(s[1]);
            if (m) {
                return engine.getValue("[Channel" + m[1] + "]", "orientation") ===
                    {Left: 0, Through: 1, Right: 2}[m[2]];
            }
            return !!c.held[path];
        }
        if (s[0] === "sampler" && s[3]) {
            var sg = "[Sampler" + s[2].replace("player", "") + "]";
            if (s[3] === "playing") {
                return engine.getValue(sg, "play") > 0;
            }
            return engine.getValue(sg, "track_loaded") > 0 && !engine.getValue(sg, "play");
        }
        m = /^deck(\d+)$/.exec(s[0]);
        if (!m || !s[1]) {
            return !!c.held[path];
        }
        var d = Number(m[1]);
        var g = "[Channel" + d + "]";
        var name = s[1];
        var on = function(k) {
            return engine.getValue(g, k) > 0;
        };
        switch (name) {
        case "playPause":
            return on("play");
        case "cue":
            return on("cue_indicator");
        case "bpmSync":
            return on("sync_enabled");
        case "key":
            return on("keylock");
        case "deckSlipToggle":
            return on("slip_enabled");
        case "reverse":
            return on("reverse");
        case "jogPitchBendModeToggle":
            return !!c.vinyl[d];
        case "autoLoopOnOff":
        case "reloop":
        case "loopIn":
        case "loopOut":
            return on("loop_enabled");
        }
        if ((m = /^(hotcue|hotcueLoop|clearCue)(\d+)$/.exec(name))) {
            return on("hotcue_" + m[2] + "_enabled");
        }
        if ((m = /^pitchPlayCue(\d+)$/.exec(name))) {
            return engine.getValue(g, "hotcue_focus") === Number(m[1]);
        }
        if ((m = /^autoLoop([\d]+)$/.exec(name))) {
            return on("beatloop_" + ReloopDJ.loopSize(m[1]) + "_enabled");
        }
        if ((m = /^slice(\d)$/.exec(name))) {
            return on("slicer_" + m[1] + "_status");
        }
        if ((m = /^instantFx(\d)$/.exec(name))) {
            var n = Number(m[1]);
            if (n <= 3) {
                return engine.getValue(c.fx(d, n), "enabled") > 0;
            }
            if (n === 4) {
                return engine.getValue(c.fx(d), "group_" + g + "_enable") > 0;
            }
            return !!c.held[path];
        }
        if ((m = /^stem(\d)$/.exec(name))) {
            return !engine.getValue("[Channel" + d + "_Stem" + m[1] + "]", "mute");
        }
        if ((m = /^stemSolo(\d)$/.exec(name))) {
            for (var i = 1; i <= 4; i++) {
                if (!engine.getValue("[Channel" + d + "_Stem" + i + "]", "mute") !== (i === Number(m[1]))) {
                    return false;
                }
            }
            return true;
        }
        if ((m = /^fx(\d)Enabled$/.exec(name))) {
            return engine.getValue(c.fx(d, m[1]), "enabled") > 0;
        }
        return !!c.held[path];
    };

    c.send = function(status, no, v) {
        var k = status + ":" + no;
        if (c.sent[k] !== v) {
            c.sent[k] = v;
            midi.sendShortMsg(status, no, v);
        }
    };

    // Lights and meters, sent when they change.
    c.refresh = function() {
        var done = {};
        data.leds.forEach(function(row) {
            // [status, number, row]: the first row whose condition holds
            // owns the light.
            var k = row[0] + ":" + row[1];
            if (done[k] || !c.holds(row[2])) {
                return;
            }
            done[k] = true;
            var led = row[2].led;
            var pair = led === true ? c.modeColours(row[2]) : led;
            c.send(row[0], row[1], c.lit(row[2].p) ? pair[1] : pair[0]);
        });
        data.outputs.forEach(function(o) {
            // [status, number, what, deck, max]
            var g = "[Channel" + o[3] + "]";
            var v = 0;
            switch (o[2]) {
            case "meter":
                v = engine.getValue(g, "vu_meter");
                break;
            case "masterLeft":
                v = engine.getValue("[Master]", "vu_meter_left");
                break;
            case "masterRight":
                v = engine.getValue("[Master]", "vu_meter_right");
                break;
            case "progress":
                v = engine.getValue(g, "playposition");
                break;
            case "endOfSong": {
                var left = (1 - engine.getValue(g, "playposition")) * engine.getValue(g, "duration");
                v = engine.getValue(g, "play") > 0 && engine.getValue(g, "duration") > 0 && left < 30 ? 1 : 0;
                break;
            }
            case "keylock":
                v = engine.getValue(g, "keylock");
                break;
            case "loaded":
                v = engine.getValue(g, "track_loaded");
                break;
            }
            c.send(o[0], o[1], Math.round(Math.max(0, Math.min(1, v)) * o[4]));
        });
    };

    // A pad's colours in its pad mode: [dimmed, lit].
    c.modeColours = function(row) {
        var i = data.modeIndex[row.c[0] + ":" + row.c[2]] || 0;
        return [data.colours.dim[i], data.colours.on[i]];
    };

    return c;
};
