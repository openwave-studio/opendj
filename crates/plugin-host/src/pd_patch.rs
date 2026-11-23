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
//! - MIDI in: `[notein]`, `[ctlin]`, `[bendin]`. A patch with `[notein]` is
//!   an instrument.
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
}

/// What reading a patch's text tells, without running it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatchInfo {
    pub params: Vec<PatchParam>,
    /// It reads notes (`[notein]`): an instrument.
    pub plays_notes: bool,
    /// It has an audio input (`[adc~]`): an effect.
    pub takes_audio: bool,
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
            "r" | "receive" if args.get(1) == Some(&"@hv_param") => {
                let num = |i: usize, or: f64| {
                    args.get(i)
                        .and_then(|w| w.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .unwrap_or(or)
                };
                let (min, max) = (num(2, 0.0), num(3, 1.0));
                let default = num(4, min);
                add(&mut info, args[0], args[0], min, max, default);
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
                    add(&mut info, receive, name, min, max, default);
                }
            }
            _ => {}
        }
    }
    info
}

fn add(info: &mut PatchInfo, receive: &str, name: &str, min: f64, max: f64, default: f64) {
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
    fn ids_follow_the_receive_name() {
        assert_eq!(param_id("cutoff"), param_id("cutoff"));
        assert_ne!(param_id("cutoff"), param_id("mix"));
    }
}
