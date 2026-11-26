//! openDJ Studio's own set file (`.odset`): the tracks with their clips,
//! devices (racks and plug-in sounds included), mixer and I/O, and the
//! tempo and key. It is JSON, so a set can be read and compared as text;
//! a plug-in's saved sound is base64 in it. Samples and recordings stay
//! where they are and the file names them; they load again on open.

use crate::clip::{Sound, Track};
use crate::scale::Key;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The file's extension, without the dot.
pub const EXTENSION: &str = "odset";

/// The layout this build writes; a file with a higher number is refused,
/// since parts of it would be lost on saving again.
pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetFile {
    pub format: u32,
    pub tempo: f64,
    #[serde(default)]
    pub key: Key,
    /// The Main fader (0..1).
    pub main_volume: f32,
    /// The return tracks' names and colours.
    #[serde(default)]
    pub returns: Vec<(String, u32)>,
    pub tracks: Vec<Track>,
}

impl SetFile {
    pub fn new(tracks: Vec<Track>, tempo: f64) -> SetFile {
        SetFile {
            format: FORMAT,
            tempo,
            key: Key::default(),
            main_volume: crate::control::UNITY,
            returns: Vec::new(),
            tracks,
        }
    }
}

/// Writes `set` to `path`: to a file beside it first, then moved over it,
/// so a failed save never leaves half a set.
pub fn write(path: &Path, set: &SetFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(set).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} isn't a file name", path.display()))?;
    let mut temp = name.to_os_string();
    temp.push(".saving");
    let temp = path.with_file_name(temp);
    std::fs::write(&temp, text).map_err(|e| format!("can't write {}: {e}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("can't save {}: {e}", path.display())
    })
}

/// Reads a set saved by [`write`]. Samples, recordings and reverb rooms
/// come back unloaded (only their paths are kept).
pub fn read(path: &Path) -> Result<SetFile, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("can't open {}: {e}", path.display()))?;
    from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn from_str(text: &str) -> Result<SetFile, String> {
    let set: SetFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if set.format > FORMAT {
        return Err("saved by a newer openDJ Studio".into());
    }
    Ok(set)
}

/// A plug-in's saved sound as base64 text.
pub(crate) mod bytes {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::sync::Arc;

    pub fn serialize<S: Serializer>(v: &Option<Arc<Vec<u8>>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => s.serialize_some(&STANDARD.encode(b.as_slice())),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Arc<Vec<u8>>>, D::Error> {
        let text: Option<String> = Option::deserialize(d)?;
        text.map(|t| {
            STANDARD
                .decode(t.as_bytes())
                .map(Arc::new)
                .map_err(serde::de::Error::custom)
        })
        .transpose()
    }
}

/// [`Sound`] as a set file has it: a Groove Box kit by its name (one this
/// build doesn't have plays the first kit).
#[derive(Serialize, Deserialize)]
enum SavedSound {
    Drums(String),
    Bass,
    Pad,
    Lead,
    Audio,
}

impl Serialize for Sound {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match *self {
            Sound::Drums(kit) => SavedSound::Drums(kit.into()),
            Sound::Bass => SavedSound::Bass,
            Sound::Pad => SavedSound::Pad,
            Sound::Lead => SavedSound::Lead,
            Sound::Audio => SavedSound::Audio,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Sound {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Sound, D::Error> {
        use opendj_engine::kits::KITS;
        Ok(match SavedSound::deserialize(d)? {
            SavedSound::Drums(name) => Sound::Drums(
                KITS.iter()
                    .find(|k| k.name == name)
                    .map_or(KITS[0].name, |k| k.name),
            ),
            SavedSound::Bass => Sound::Bass,
            SavedSound::Pad => Sound::Pad,
            SavedSound::Lead => Sound::Lead,
            SavedSound::Audio => Sound::Audio,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{demo_set, PluginFormat, PluginRef};
    use crate::devices::Settings;
    use std::sync::Arc;

    #[test]
    fn a_set_comes_back_as_it_was_saved() {
        let mut tracks = demo_set();
        tracks[1].volume = Some(0.7);
        tracks[1].sends = [0.25, 0.5];
        tracks[1].params = Some([0.1; crate::sound::PARAMS]);
        tracks[2].devices.push(crate::clip::DeviceInfo {
            name: "Synth".into(),
            kind: "CLAP".into(),
            on: true,
            plugin: Some(PluginRef {
                name: "Synth".into(),
                vendor: "Someone".into(),
                format: PluginFormat::Clap {
                    file: "/plugins/synth.clap".into(),
                    id: "com.someone.synth".into(),
                },
                instrument: true,
                state: Some(Arc::new(vec![0, 1, 2, 255, 128])),
            }),
            builtin: None,
        });
        let (name, rack) = crate::devices::catalog()
            .into_iter()
            .find(|(_, s)| matches!(s, Settings::Rack(_)))
            .expect("a rack in the catalog");
        tracks[3].devices.push(crate::clip::DeviceInfo {
            name: name.into(),
            kind: "Built-in".into(),
            on: true,
            plugin: None,
            builtin: Some(rack),
        });
        let mut set = SetFile::new(tracks, 128.5);
        set.key.root = 2;
        set.returns = vec![("A Reverb".into(), 0x112233)];
        let dir = std::env::temp_dir().join(format!("odset-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Song.odset");
        write(&path, &set).unwrap();
        let back = read(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(back.tempo, 128.5);
        assert_eq!(back.key.root, 2);
        assert_eq!(back.returns, set.returns);
        assert_eq!(back.tracks.len(), set.tracks.len());
        for (a, b) in set.tracks.iter().zip(&back.tracks) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.sound, b.sound);
            assert_eq!(a.volume, b.volume);
            assert_eq!(a.sends, b.sends);
            assert_eq!(a.params, b.params);
            assert_eq!(a.devices, b.devices);
            assert_eq!(a.io, b.io);
            for (x, y) in a.slots.iter().zip(&b.slots) {
                match (x, y) {
                    (Some(x), Some(y)) => {
                        assert_eq!(x.name, y.name);
                        assert_eq!(x.beats, y.beats);
                        assert_eq!(x.notes, y.notes);
                        assert_eq!(x.locks, y.locks);
                        assert_eq!(x.stage_starts, y.stage_starts);
                        assert_eq!(x.stages, y.stages);
                    }
                    (None, None) => {}
                    _ => panic!("a slot changed"),
                }
            }
        }
    }

    #[test]
    fn a_set_from_a_newer_studio_is_refused() {
        let mut set = SetFile::new(Vec::new(), 120.0);
        set.format = FORMAT + 1;
        let text = serde_json::to_string(&set).unwrap();
        assert!(from_str(&text).is_err());
    }
}
