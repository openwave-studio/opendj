//! What a Pure Data patch offers openDJ, read from its text without running
//! it, so the app can list patches without a helper. The patch itself runs
//! in the helper (`crate::pd`, feature `pd`).
//!
//! A patch talks to openDJ by plain Pd conventions, so the same file works
//! in Pd, plugdata and hvcc:
//!
//! - Audio in and out: `[adc~ 1 2]` and `[dac~ 1 2]`.
//! - Parameters: `[r cutoff @hv_param 20 20000 1000]` (hvcc's form: name,
//!   min, max, default), or a number box (`nbx`) with a receive name, as the
//!   `.amxd` converter writes for a Live dial. Each becomes a knob; turning
//!   it sends the value to that receive name.
//!   Sliders (`hsl`, `vsl`) and radio buttons (`hradio`, `vradio`) with a
//!   receive name become knobs the same way.
//! - MIDI in: `[notein]`, `[ctlin]`, `[bendin]`. A patch with `[notein]` is
//!   an instrument, and so is one that only plays (`[dac~]` and no
//!   `[adc~]`), such as a drum machine with its own sequencer.
//! - cyclone and ELSE objects, built in (`[lores~]`, `[else/knob]`).
//! - The mix's transport: `[r opendj-bpm]`, `[r opendj-beat]` (beats since
//!   the grid's start) and `[r opendj-playing]` (1 or 0).

/// Receive names openDJ sends the transport to.
pub const BPM: &str = "opendj-bpm";
pub const BEAT: &str = "opendj-beat";
pub const PLAYING: &str = "opendj-playing";

/// A parameter a patch declares: the receive name it listens on, and its
/// range.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchParam {
    pub receive: String,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// A number box, slider or radio: it holds its own value (set by the
    /// patch as it loads), so openDJ sends one only when the knob moves.
    pub widget: bool,
}

/// What reading a patch's text tells, without running it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatchInfo {
    pub params: Vec<PatchParam>,
    /// It reads notes (`[notein]`): an instrument.
    pub plays_notes: bool,
    /// It has an audio input (`[adc~]`): an effect.
    pub takes_audio: bool,
    /// It has an audio output (`[dac~]`).
    pub makes_sound: bool,
}

impl PatchInfo {
    /// An instrument: it reads notes, or it plays without taking audio in.
    pub fn is_instrument(&self) -> bool {
        self.plays_notes || (self.makes_sound && !self.takes_audio)
    }
}

/// A stable id for a parameter, from its receive name (FNV-1a), so a knob
/// the DJ picked stays on it when the patch is edited.
pub fn param_id(receive: &str) -> u32 {
    receive.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    })
}

/// Reads a patch's parameters and kind from its text (see the module's
/// conventions). Receive names with `$` (local to one copy of an
/// abstraction) can't be reached from outside and are skipped, as are
/// repeats of a name.
pub fn read_patch(text: &str) -> PatchInfo {
    let mut info = PatchInfo::default();
    for record in records(text) {
        let words: Vec<&str> = record.split_whitespace().collect();
        if words.len() < 5 || words[0] != "#X" || words[1] != "obj" {
            continue;
        }
        let class = words[4];
        let args = &words[5..];
        match class {
            "notein" => info.plays_notes = true,
            "adc~" => info.takes_audio = true,
            "dac~" => info.makes_sound = true,
            "r" | "receive" if args.get(1) == Some(&"@hv_param") => {
                let num = |i: usize, or: f64| {
                    args.get(i)
                        .and_then(|w| w.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .unwrap_or(or)
                };
                let (min, max) = (num(2, 0.0), num(3, 1.0));
                let default = num(4, min);
                add(&mut info, args[0], args[0], min, max, default, false);
            }
            // nbx w h min max log init send receive label x y font size bg fg
            // label-color value log-height
            "nbx" if args.len() >= 17 => {
                let receive = args[7];
                let label = args[8];
                let name = if label == "empty" { receive } else { label };
                let num = |i: usize| args[i].parse::<f64>().ok().filter(|v| v.is_finite());
                if let (Some(min), Some(max)) = (num(2), num(3)) {
                    let default = num(16).unwrap_or(min);
                    add(&mut info, receive, name, min, max, default, true);
                }
            }
            // hsl/vsl w h min max log init send receive label x y font size
            // bg fg label-color position steady, where position is the
            // knob's place along the slider in hundredths of a pixel.
            "hsl" | "vsl" if args.len() >= 17 => {
                let num = |i: usize| args[i].parse::<f64>().ok().filter(|v| v.is_finite());
                let (Some(min), Some(max)) = (num(2), num(3)) else {
                    continue;
                };
                let long = num(if class == "hsl" { 0 } else { 1 }).unwrap_or(128.0);
                let at = (num(16).unwrap_or(0.0) / ((long - 1.0).max(1.0) * 100.0)).clamp(0.0, 1.0);
                let default = if args[4] == "1" && min > 0.0 && max > 0.0 {
                    min * (max / min).powf(at)
                } else {
                    min + (max - min) * at
                };
                let name = if args[8] == "empty" { args[7] } else { args[8] };
                add(&mut info, args[7], name, min, max, default, true);
            }
            // hradio/vradio size new-old init count send receive label x y
            // font size bg fg label-color value
            "hradio" | "vradio" if args.len() >= 15 => {
                let Some(count) = args[3].parse::<f64>().ok().filter(|v| *v >= 2.0) else {
                    continue;
                };
                let default = args[14].parse::<f64>().unwrap_or(0.0);
                let name = if args[6] == "empty" { args[5] } else { args[6] };
                add(&mut info, args[5], name, 0.0, count - 1.0, default, true);
            }
            _ => {}
        }
    }
    info
}

fn add(
    info: &mut PatchInfo,
    receive: &str,
    name: &str,
    min: f64,
    max: f64,
    default: f64,
    widget: bool,
) {
    if receive == "empty" || receive.contains('$') || max <= min {
        return;
    }
    if info.params.iter().any(|p| p.receive == receive) {
        return;
    }
    info.params.push(PatchParam {
        receive: receive.to_string(),
        name: name.replace('_', " "),
        min,
        max,
        default: default.clamp(min, max),
        widget,
    });
}

/// A patch file's records: split at each `;` that isn't escaped.
fn records(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if c == '\\' {
            cur.push(c);
            escaped = true;
        } else if c == ';' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "#N canvas 0 50 450 300 12;\n\
        #X obj 30 30 adc~ 1 2;\n\
        #X obj 30 80 r cutoff @hv_param 20 20000 1000;\n\
        #X obj 30 120 receive mix @hv_param 0 1;\n\
        #X obj 30 160 r $0-local @hv_param 0 1 0.5;\n\
        #X obj 200 20 nbx 6 20 0 2 0 1 empty gain Gain_dB 0 -10 0 12 #dfd9ef #292132 #292132 0.5 256;\n\
        #X obj 200 60 nbx 6 20 0 1 0 1 empty empty Unreachable 0 -10 0 12 #dfd9ef #292132 #292132 0 256;\n\
        #X text 10 200 a comment\\; with r x @hv_param 0 1 in it;\n\
        #X obj 30 250 dac~ 1 2;\n";

    #[test]
    fn reads_parameters_from_the_patch() {
        let info = read_patch(PATCH);
        assert!(info.takes_audio);
        assert!(!info.plays_notes);
        let names: Vec<_> = info.params.iter().map(|p| p.receive.as_str()).collect();
        assert_eq!(names, ["cutoff", "mix", "gain"]);
        assert_eq!(info.params[0].min, 20.0);
        assert_eq!(info.params[0].max, 20000.0);
        assert_eq!(info.params[0].default, 1000.0);
        // No default: the minimum.
        assert_eq!(info.params[1].default, 0.0);
        // A number box: its label names it, its receive name is sent to.
        assert_eq!(info.params[2].name, "Gain dB");
        assert_eq!(info.params[2].default, 0.5);
    }

    #[test]
    fn a_patch_with_notein_is_an_instrument() {
        let info = read_patch("#X obj 10 10 notein;\n#X obj 10 40 dac~;\n");
        assert!(info.plays_notes);
        assert!(!info.takes_audio);
    }

    #[test]
    fn a_patch_that_only_plays_is_an_instrument() {
        assert!(read_patch("#X obj 10 10 metro 120;\n#X obj 10 40 dac~;\n").is_instrument());
        assert!(!read_patch(PATCH).is_instrument());
        assert!(!read_patch("#X obj 10 10 osc~ 440;\n").is_instrument());
    }

    #[test]
    fn sliders_and_radios_are_knobs() {
        let info = read_patch(
            "#X obj 230 54 hsl 91 19 0 60 0 0 swg swg-r swing -2 -11 0 10 #fcfcfc #202020 #202020 4500 1;\n\
             #X obj 10 10 vsl 15 101 1 1000 1 0 empty freq-r empty 0 -9 0 10 #fcfcfc #000000 #000000 5000 1;\n\
             #X obj 10 10 hradio 15 1 0 4 empty mode-r mode 0 -8 0 10 #fcfcfc #000000 #000000 2;\n\
             #X obj 10 10 hsl 128 15 0 1 0 0 empty $0-x empty 0 -9 0 10 #fcfcfc #000000 #000000 0 1;\n",
        );
        let names: Vec<_> = info
            .params
            .iter()
            .map(|p| (p.receive.as_str(), p.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [("swg-r", "swing"), ("freq-r", "freq-r"), ("mode-r", "mode")]
        );
        // Halfway along: half the range, or the geometric middle when log.
        assert!((info.params[0].default - 30.0).abs() < 1e-9);
        assert!((info.params[1].default - 1000f64.sqrt()).abs() < 1e-6);
        assert_eq!((info.params[2].max, info.params[2].default), (3.0, 2.0));
    }

    #[test]
    fn ids_follow_the_receive_name() {
        assert_eq!(param_id("cutoff"), param_id("cutoff"));
        assert_ne!(param_id("cutoff"), param_id("mix"));
    }
}
