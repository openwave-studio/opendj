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
    /// The synth's own sound categories ("Bass", "Pad"), by the number its
    /// presets carry. With them the picker groups presets by category
    /// instead of by bank.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// How openDJ reads preset names and categories from the synth itself
    /// ("arturia-microfreak"); empty when it can't.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub names_from: String,
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
    /// The name is the preset's own, not its bank and number.
    pub named: bool,
    pub bank: String,
    /// Its category's name, empty without one.
    pub category: String,
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
            names: self.macro_names(),
        }
    }

    /// Every preset, bank by bank, with its factory name.
    pub fn preset_list(&self) -> Vec<SynthPreset> {
        let bank_select = self.presets.bank_select == "msb";
        let mut out = Vec::new();
        for b in &self.presets.banks {
            for p in 0..b.count {
                let own = b.names.get(p as usize).filter(|n| !n.is_empty());
                let name = own
                    .cloned()
                    .unwrap_or_else(|| format!("{} {}", b.name, p as u16 + 1));
                out.push(SynthPreset {
                    name,
                    named: own.is_some(),
                    bank: b.name.clone(),
                    category: String::new(),
                    bank_msb: bank_select.then_some(b.msb),
                    program: p,
                });
            }
        }
        out
    }

    /// The groups the preset picker shows on its left: "All" then the
    /// categories when the synth has them, else its banks when there is
    /// more than one, else none.
    pub fn preset_groups(&self) -> Vec<String> {
        if !self.presets.categories.is_empty() {
            std::iter::once("All".to_string())
                .chain(self.presets.categories.iter().cloned())
                .collect()
        } else if self.presets.banks.len() > 1 {
            self.presets.banks.iter().map(|b| b.name.clone()).collect()
        } else {
            Vec::new()
        }
    }

    /// Whether preset `p` is in group `group` of [`Self::preset_groups`].
    pub fn in_group(&self, p: &SynthPreset, group: usize) -> bool {
        let groups = self.preset_groups();
        match groups.get(group) {
            None => true,
            Some(_) if !self.presets.categories.is_empty() => {
                group == 0 || p.category == groups[group]
            }
            Some(g) => &p.bank == g,
        }
    }

    /// The preset's number as the picker shows it: within its bank for a
    /// synth with named banks ("3"), else counted through all of them
    /// ("042").
    pub fn preset_number(&self, index: usize, p: &SynthPreset) -> String {
        if self.presets.categories.is_empty() && self.presets.banks.len() > 1 {
            format!("{}", p.program as u16 + 1)
        } else {
            format!("{:03}", index + 1)
        }
    }

    /// Fills `list` with the names and categories the synth told
    /// ([`read_name`]), one per preset in order; `None` keeps the
    /// profile's.
    pub fn apply_names(&self, list: &mut [SynthPreset], read: &[Option<ReadPreset>]) {
        for (p, r) in list.iter_mut().zip(read) {
            let Some(r) = r else { continue };
            if r.empty || r.name.trim().is_empty() {
                p.name = "Init".into();
                p.named = true;
            } else {
                p.name = r.name.trim().to_string();
                p.named = true;
            }
            p.category = self
                .presets
                .categories
                .get(r.category as usize)
                .cloned()
                .unwrap_or_default();
        }
    }
}

/// A preset's name and category as the synth reports them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadPreset {
    pub name: String,
    pub category: u8,
    /// An empty slot (the synth shows "Init").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub empty: bool,
}

/// The SysEx that asks the synth for preset `index`'s name (counted from 0
/// through all banks), with `seq` (0 to 127) to match its answer; `None`
/// when `names_from` names no protocol openDJ knows.
///
/// "arturia-microfreak": `F0 00 20 6B 07 01 seq 03 19 bank program 00 F7`
/// asks for the preset's header, as Arturia's own editor does.
pub fn name_request(names_from: &str, seq: u8, index: usize) -> Option<Vec<u8>> {
    match names_from {
        "arturia-microfreak" => {
            let (bank, program) = ((index / 128) as u8, (index % 128) as u8);
            Some(vec![
                0xF0,
                0x00,
                0x20,
                0x6B,
                0x07,
                0x01,
                seq & 127,
                0x03,
                0x19,
                bank & 127,
                program,
                0x00,
                0xF7,
            ])
        }
        _ => None,
    }
}

/// Reads the synth's answer to [`name_request`] `seq`, or `None` when
/// `reply` is something else.
///
/// The MicroFreak answers with operation 0x52 and a 35-byte header: byte 3
/// bit 0x08 marks an empty slot, byte 10 is the category and bytes 12 to 25
/// the name, NUL padded.
pub fn read_name(names_from: &str, seq: u8, reply: &[u8]) -> Option<ReadPreset> {
    match names_from {
        "arturia-microfreak" => {
            if reply.len() < 10 || reply[..6] != [0xF0, 0x00, 0x20, 0x6B, 0x07, 0x01] {
                return None;
            }
            if reply[6] != seq & 127 || reply[8] != 0x52 {
                return None;
            }
            let len = reply[7] as usize;
            let payload = reply.get(9..9 + len)?;
            if payload.len() < 26 {
                return None;
            }
            let name: String = payload[12..26]
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| (c & 0x7F) as char)
                .collect();
            Some(ReadPreset {
                name,
                category: payload[10],
                empty: payload[3] & 0x08 != 0,
            })
        }
        _ => None,
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
        assert!(!presets[16].named);
        // The picker groups by bank and counts within it.
        let groups = p.preset_groups();
        assert_eq!(groups.len(), 16);
        assert_eq!(groups[1], "Classic Bass");
        assert!(p.in_group(&presets[18], 1));
        assert!(!p.in_group(&presets[2], 1));
        assert_eq!(p.preset_number(18, &presets[18]), "3");
    }

    #[test]
    fn the_microfreak_names_its_presets_and_their_categories() {
        let p = find("arturia-microfreak").unwrap();
        let ask = name_request(&p.presets.names_from, 5, 130).unwrap();
        assert_eq!(
            ask,
            [0xF0, 0x00, 0x20, 0x6B, 0x07, 0x01, 5, 3, 0x19, 1, 2, 0, 0xF7]
        );
        // Its answer: a 35-byte header with the category at 10 and the name
        // from 12.
        let mut header = vec![0u8; 35];
        header[10] = 5;
        header[12..23].copy_from_slice(b"Glass Choir");
        let mut reply = vec![0xF0, 0x00, 0x20, 0x6B, 0x07, 0x01, 5, 35, 0x52];
        reply.extend(&header);
        reply.push(0xF7);
        let read = read_name(&p.presets.names_from, 5, &reply).unwrap();
        assert_eq!(read.name, "Glass Choir");
        assert!(!read.empty);
        // Another request's answer is not this one.
        assert_eq!(read_name(&p.presets.names_from, 6, &reply), None);
        let mut presets = p.preset_list();
        let mut reads = vec![None; 512];
        reads[41] = Some(read);
        reads[42] = Some(ReadPreset {
            name: String::new(),
            category: 10,
            empty: true,
        });
        p.apply_names(&mut presets, &reads);
        assert_eq!(presets[41].name, "Glass Choir");
        assert_eq!(presets[41].category, "Pad");
        assert_eq!(presets[42].name, "Init");
        // "All" then the categories; numbers count through every bank.
        let groups = p.preset_groups();
        assert_eq!(groups[0], "All");
        assert_eq!(groups[6], "Pad");
        assert!(p.in_group(&presets[41], 6) && p.in_group(&presets[41], 0));
        assert!(!p.in_group(&presets[0], 6));
        assert_eq!(p.preset_number(41, &presets[41]), "042");
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
