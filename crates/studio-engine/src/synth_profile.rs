//! Synth profiles: what openDJ knows about a hardware synth model, so the
//! instrument deck (see `opendj_engine::hardware_synth`) and Studio's
//! External Instrument can play it with named knobs and its presets.
//!
//! A profile names the synth's MIDI port (or part of it), its channel, its
//! control changes grouped as on its panel (14-bit ones with their low CC),
//! the knobs (up to 24) the deck's macro pages start with (8 a page), its preset
//! banks with bank select and factory names, and whether it follows MIDI
//! clock and Start/Stop. The bundled ones are JSON files in `synths/`,
//! written from each maker's published MIDI chart (`source`).

use serde::{Deserialize, Serialize};

use opendj_engine::hardware_synth::HardwareConfig;

/// The bundled profiles' files, in the order they are listed.
const BUNDLED: [&str; 3] = [
    include_str!("../synths/arturia-microfreak.json"),
    include_str!("../synths/arturia-minifreak.json"),
    include_str!("../synths/moog-messenger.json"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SynthProfile {
    /// Kept in settings and presets: `maker-model`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub maker: String,
    /// Where the MIDI chart came from.
    #[serde(default)]
    pub source: String,
    /// The MIDI port's name, or a part of it.
    pub port: String,
    /// 0 to 15: the synth's channel as it ships.
    #[serde(default)]
    pub channel: u8,
    /// "mono" or "stereo": what its line out is.
    #[serde(default = "mono")]
    pub audio: String,
    pub controls: Vec<SynthControl>,
    /// The control change each knob of the deck's macro pages turns (up to
    /// 24, three pages).
    pub macros: Vec<u8>,
    #[serde(default)]
    pub presets: Presets,
    #[serde(default)]
    pub transport: SynthTransport,
}

fn mono() -> String {
    "mono".into()
}

/// One of the synth's control changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SynthControl {
    /// Its section on the synth's panel ("Filter", "VCO 1").
    pub group: String,
    pub name: String,
    pub cc: u8,
    /// For a 14-bit control, the CC its low 7 bits go on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsb: Option<u8>,
    /// Centred at half way (a tune or an envelope amount).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bipolar: bool,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Presets {
    /// "msb" sends Bank Select (CC 0) before the program change; "none"
    /// sends the program change alone.
    #[serde(default)]
    pub bank_select: String,
    #[serde(default)]
    pub banks: Vec<Bank>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bank {
    pub name: String,
    /// The Bank Select value that picks it.
    pub msb: u8,
    /// How many programs it holds (at most 128).
    pub count: u8,
    /// Factory names, in program order; missing ones show as the bank's
    /// name and number.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
}

/// Whether the synth's own sequencer and arpeggiator follow MIDI clock and
/// Start/Stop from openDJ.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct SynthTransport {
    #[serde(default)]
    pub clock: bool,
    #[serde(default)]
    pub start_stop: bool,
}

/// One preset of a synth, as the preset bar lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct SynthPreset {
    pub name: String,
    pub bank: String,
    /// Bank Select to send first, if the synth takes one.
    pub bank_msb: Option<u8>,
    /// 0 to 127.
    pub program: u8,
}

impl SynthProfile {
    /// Reads a profile from its JSON.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let p: SynthProfile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        p.check()?;
        Ok(p)
    }

    fn check(&self) -> Result<(), String> {
        if self.id.is_empty() || self.name.is_empty() {
            return Err("a synth profile needs an id and a name".into());
        }
        let bad = |n: u8| n > 127;
        if self.channel > 15 {
            return Err(format!("{}: channel {} is past 15", self.id, self.channel));
        }
        if self.macros.len() > 24 {
            return Err(format!("{}: more than 24 macro knobs", self.id));
        }
        for c in &self.controls {
            if bad(c.cc) || c.lsb.is_some_and(bad) {
                return Err(format!("{}: {} has a CC past 127", self.id, c.name));
            }
        }
        for b in &self.presets.banks {
            if b.count > 128 || bad(b.msb) {
                return Err(format!("{}: bank {} is out of range", self.id, b.name));
            }
        }
        Ok(())
    }

    /// The control on `cc`.
    pub fn control(&self, cc: u8) -> Option<&SynthControl> {
        self.controls.iter().find(|c| c.cc == cc)
    }

    /// The name each macro knob shows: its control's name, with its group
    /// where two controls share a name ("Filter Attack", "Amp Attack").
    pub fn macro_names(&self) -> Vec<String> {
        self.macros
            .iter()
            .map(|&cc| match self.control(cc) {
                Some(c) => {
                    let shared = self.controls.iter().filter(|o| o.name == c.name).count() > 1;
                    if shared {
                        format!("{} {}", c.group, c.name)
                    } else {
                        c.name.clone()
                    }
                }
                None => format!("CC {cc}"),
            })
            .collect()
    }

    /// What the instrument deck loads: the synth on `port` and `channel`,
    /// its macros sending their control changes (14-bit where the synth
    /// takes them).
    pub fn hardware_config(&self, port: &str, channel: u8) -> HardwareConfig {
        HardwareConfig {
            port: if port.is_empty() {
                self.port.clone()
            } else {
                port.to_string()
            },
            channel: channel & 15,
            ccs: self.macros.iter().map(|&cc| Some(cc)).collect(),
            lsbs: self
                .macros
                .iter()
                .map(|&cc| self.control(cc).and_then(|c| c.lsb))
                .collect(),
        }
    }

    /// Every preset, bank by bank, with its factory name.
    pub fn preset_list(&self) -> Vec<SynthPreset> {
        let bank_select = self.presets.bank_select == "msb";
        let mut out = Vec::new();
        for b in &self.presets.banks {
            for p in 0..b.count {
                let name = b
                    .names
                    .get(p as usize)
                    .filter(|n| !n.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("{} {}", b.name, p as u16 + 1));
                out.push(SynthPreset {
                    name,
                    bank: b.name.clone(),
                    bank_msb: bank_select.then_some(b.msb),
                    program: p,
                });
            }
        }
        out
    }
}

/// The synth profiles openDJ ships with.
pub fn bundled() -> Vec<SynthProfile> {
    BUNDLED
        .iter()
        .map(|t| SynthProfile::from_json(t).expect("a bundled synth profile reads"))
        .collect()
}

/// The bundled profile `id`.
pub fn find(id: &str) -> Option<SynthProfile> {
    bundled().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_profiles_read_and_fill_their_macro_pages() {
        let all = bundled();
        assert_eq!(all.len(), 3);
        for p in &all {
            assert!((17..=24).contains(&p.macros.len()), "{}", p.id);
            for cc in &p.macros {
                assert!(p.control(*cc).is_some(), "{}: macro CC {cc}", p.id);
            }
            let mut ccs: Vec<u8> = p.controls.iter().map(|c| c.cc).collect();
            ccs.sort_unstable();
            ccs.dedup();
            assert_eq!(ccs.len(), p.controls.len(), "{}: a CC twice", p.id);
        }
    }

    #[test]
    fn the_minifreak_turns_its_filter_from_the_first_knobs() {
        let p = find("arturia-minifreak").unwrap();
        let names = p.macro_names();
        assert_eq!(names[0], "Cutoff");
        assert_eq!(names[1], "Resonance");
        let c = p.hardware_config("", 0);
        assert_eq!(c.port, "MiniFreak");
        assert_eq!(c.ccs[0], Some(74));
        assert_eq!(c.lsbs[0], None);
        // 384 presets in three banks of 128.
        let presets = p.preset_list();
        assert_eq!(presets.len(), 384);
        assert_eq!(presets[128].bank_msb, Some(1));
        assert_eq!(presets[128].program, 0);
    }

    #[test]
    fn the_microfreak_turns_its_filter_from_the_first_knobs() {
        let p = find("arturia-microfreak").unwrap();
        let names = p.macro_names();
        assert_eq!(names[..3], ["Cutoff", "Resonance", "Filter Amount"]);
        // Two Holds: the group tells them apart.
        assert!(
            names.contains(&"Cycling Envelope Hold".to_string()),
            "{names:?}"
        );
        assert!(names.contains(&"Keyboard Hold".to_string()), "{names:?}");
        let c = p.hardware_config("", 0);
        assert_eq!(c.port, "MicroFreak");
        assert_eq!(c.ccs[0], Some(23));
        // 512 presets in four banks of 128.
        let presets = p.preset_list();
        assert_eq!(presets.len(), 512);
        assert_eq!(presets[384].bank_msb, Some(3));
    }

    #[test]
    fn the_messengers_knobs_are_14_bit_and_its_banks_named() {
        let p = find("moog-messenger").unwrap();
        let c = p.hardware_config("Moog Messenger", 3);
        assert_eq!(c.channel, 3);
        assert_eq!(c.ccs[0], Some(19));
        assert_eq!(c.lsbs[0], Some(51));
        let names = p.macro_names();
        // Two envelopes have an Attack: the group tells them apart.
        assert!(names.contains(&"Filter EG Attack".to_string()), "{names:?}");
        assert!(names.contains(&"Amp EG Attack".to_string()), "{names:?}");
        let presets = p.preset_list();
        assert_eq!(presets.len(), 256);
        assert_eq!(presets[16].name, "Classic Bass 1");
    }

    #[test]
    fn a_bad_profile_says_why() {
        let e = SynthProfile::from_json(
            r#"{"id":"x","name":"X","port":"X","channel":16,"controls":[],"macros":[]}"#,
        )
        .unwrap_err();
        assert!(e.contains("channel"), "{e}");
    }
}
