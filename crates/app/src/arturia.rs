//! Arturia's preset library: the presets its instruments share, listed in
//! one SQLite file (`db.db3`) beside a folder per instrument. Read only,
//! for the instruments' own presets (see `crate::presets`), with the
//! library's tags: what each sound is, its genres, styles and character,
//! and whether it is liked.
//!
//! A preset file is the instrument's whole sound as it saves it (a text
//! archive), so it loads as the plugin's state as it is (see
//! [`plugin_state`]).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

/// What a preset file starts with: the instrument's own saved sound.
const ARCHIVE: &[u8] = b"22 serialization::archive";

/// One preset of the library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub name: String,
    pub path: PathBuf,
    /// The instrument folder it is in ("Mini V4", "Pigments"): the
    /// plugin's own name.
    pub folder: String,
    /// Made for Analog Lab too, which plays the other instruments' sounds.
    pub analog_lab: bool,
    /// Its sound type, as `crate::presets::TYPES` names it.
    pub types: Vec<String>,
    /// The library's own type and sub-type, as it writes them ("Keys",
    /// "Bells"); empty for none.
    pub arturia_type: String,
    pub arturia_subtype: String,
    pub genres: Vec<String>,
    pub styles: Vec<String>,
    /// Its characteristics (see `crate::presets::CHARACTERS`).
    pub characters: Vec<String>,
    pub liked: bool,
}

/// The library's folder: the one holding `db.db3`.
pub fn library() -> Option<PathBuf> {
    let dir = if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Arturia/Presets")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("PROGRAMDATA")?)
            .join("Arturia")
            .join("Presets")
    } else {
        return None;
    };
    dir.join("db.db3").is_file().then_some(dir)
}

/// When the library last changed, to read it again only then.
pub fn changed(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir.join("db.db3"))
        .and_then(|m| m.modified())
        .ok()
}

/// Every preset the library lists and doesn't hide, with its tags.
pub fn read(dir: &Path) -> Result<Vec<Found>, String> {
    let db = Connection::open_with_flags(
        dir.join("db.db3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("can't open it ({e})"))?;
    let err = |e: rusqlite::Error| e.to_string();
    let mut out: Vec<Found> = Vec::new();
    let mut ids: Vec<i64> = Vec::new();
    {
        let mut q = db
            .prepare(
                "SELECT p.key_id, p.name, p.file_path, IFNULL(p.favorite, 0),
                        IFNULL(p.analoglab_factory, 0), IFNULL(t.name, ''), IFNULL(s.name, '')
                 FROM Preset_Id p
                 LEFT JOIN Types_V2 t ON t.key_id = p.type_v2
                 LEFT JOIN Subtypes_V2 s ON s.key_id = p.subtype_v2
                 WHERE IFNULL(p.hide_in_browser, 0) = 0",
            )
            .map_err(err)?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })
            .map_err(err)?;
        for row in rows.flatten() {
            let (id, name, file, liked, analog_lab, kind, subtype) = row;
            let Some((folder, path)) = locate(dir, &file) else {
                continue;
            };
            ids.push(id);
            out.push(Found {
                name,
                path,
                folder,
                analog_lab: analog_lab != 0,
                types: sound_types(&kind),
                arturia_type: kind,
                arturia_subtype: subtype,
                liked: liked != 0,
                ..Found::default()
            });
        }
    }
    let at: std::collections::HashMap<i64, usize> =
        ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut q = db
        .prepare(
            "SELECT pc.preset_key, ct.name, c.name
             FROM Preset_Characteristics pc
             JOIN Characteristics c ON c.key_id = pc.characteristic_key
             JOIN Characteristics_Types ct ON ct.key_id = c.type_key",
        )
        .map_err(err)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(err)?;
    for (id, group, value) in rows.flatten() {
        let Some(found) = at.get(&id).map(|&i| &mut out[i]) else {
            continue;
        };
        tag(found, &group, &value);
    }
    Ok(out)
}

/// Where the library's `file` is under `dir`, and its instrument folder.
/// The library writes whole paths; they are taken from the folder after
/// `Presets`, so a library that moved (or a Windows path) still works.
fn locate(dir: &Path, file: &str) -> Option<(String, PathBuf)> {
    let parts: Vec<&str> = file.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let start = parts
        .iter()
        .rposition(|p| p.eq_ignore_ascii_case("Presets"))?
        + 1;
    let rest = parts.get(start..).filter(|r| r.len() >= 2)?;
    let path = rest.iter().fold(dir.to_path_buf(), |p, part| p.join(part));
    Some((rest[0].to_string(), path))
}

/// A tag the library gives (`group` is Characteristics, Genres or
/// Styles), kept on `found` as written once (see [`tidy`]).
fn tag(found: &mut Found, group: &str, value: &str) {
    let value = tidy(value);
    if value.is_empty() {
        return;
    }
    let list = match group {
        "Genres" => &mut found.genres,
        "Styles" => &mut found.styles,
        "Characteristics" => &mut found.characters,
        _ => return,
    };
    if !list.contains(&value) {
        list.push(value);
    }
}

/// A tag as written once: "Dub / Reggae" and "Dub/Reggae" are one.
fn tidy(value: &str) -> String {
    value
        .trim()
        .split('/')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" / ")
}

/// The sound type for the library's type: openDJ's types are the
/// library's, so it is the same, as `crate::presets::TYPES` spells it.
fn sound_types(kind: &str) -> Vec<String> {
    let kind = kind.trim();
    if kind.is_empty() {
        return Vec::new();
    }
    let known = crate::presets::TYPES
        .iter()
        .find(|t| t.eq_ignore_ascii_case(kind));
    vec![known.map_or_else(|| kind.to_string(), |t| t.to_string())]
}

/// Whether `bytes` are an Arturia preset file.
pub fn is_preset(bytes: &[u8]) -> bool {
    bytes.starts_with(ARCHIVE)
}

/// The sound a preset file makes for plugin `key`, as the plugin host
/// takes it: for VST3 the file as both the component's and the
/// controller's state; for an Audio Unit its class info, with the file as
/// its controller's state.
pub fn plugin_state(key: &str, file: &[u8], name: &str) -> Result<Vec<u8>, String> {
    if key.starts_with("vst3:") {
        return Ok(opendj_engine::plugins::plugin_presets::pack_states(
            file, file,
        ));
    }
    let id = key.strip_prefix("au:").ok_or("its format can't load it")?;
    au_class_info(id, file, name)
}

/// An Audio Unit's class info for unit `id` ("aumu:MIN4:Artu") holding
/// `file` as its controller's state, as a binary property list.
fn au_class_info(id: &str, file: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let code = |s: &str| -> Result<i64, String> {
        let b = s.as_bytes();
        if b.len() != 4 {
            return Err(format!("“{id}” isn't an Audio Unit id"));
        }
        Ok(i64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    };
    let mut parts = id.splitn(3, ':');
    let (Some(t), Some(s), Some(m)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!("“{id}” isn't an Audio Unit id"));
    };
    let mut d = plist::Dictionary::new();
    d.insert("type".into(), plist::Value::Integer(code(t)?.into()));
    d.insert("subtype".into(), plist::Value::Integer(code(s)?.into()));
    d.insert(
        "manufacturer".into(),
        plist::Value::Integer(code(m)?.into()),
    );
    d.insert("version".into(), plist::Value::Integer(0.into()));
    d.insert("name".into(), plist::Value::String(name.into()));
    d.insert("Controller State".into(), plist::Value::Data(file.to_vec()));
    let mut out = Vec::new();
    plist::Value::Dictionary(d)
        .to_writer_binary(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("opendj-arturia-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A small library as Arturia writes it.
    fn library_at(dir: &Path) {
        let db = Connection::open(dir.join("db.db3")).unwrap();
        db.execute_batch(
            "CREATE TABLE Types_V2 (key_id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE Subtypes_V2 (key_id INTEGER PRIMARY KEY, name TEXT, type_key INTEGER);
             CREATE TABLE Characteristics_Types (key_id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE Characteristics (key_id INTEGER PRIMARY KEY, name TEXT, type_key INTEGER);
             CREATE TABLE Preset_Id (key_id INTEGER PRIMARY KEY, name TEXT, type_v2 INTEGER,
                 subtype_v2 INTEGER, file_path TEXT, favorite INTEGER, hide_in_browser INTEGER,
                 analoglab_factory INTEGER);
             CREATE TABLE Preset_Characteristics (key_id INTEGER PRIMARY KEY, preset_key INTEGER,
                 characteristic_key INTEGER);
             INSERT INTO Types_V2 VALUES (1, 'Bass'), (2, 'Electric Piano');
             INSERT INTO Subtypes_V2 VALUES (1, 'Plucked Bass', 1), (2, 'Classic EP', 2);
             INSERT INTO Characteristics_Types VALUES (1, 'Characteristics'), (2, 'Genres'), (3, 'Styles');
             INSERT INTO Characteristics VALUES (1, 'Analog', 1), (2, 'Dub / Reggae', 2),
                 (3, 'Dub/Reggae', 2), (4, 'Warm', 3), (5, 'Punchy', 3);
             INSERT INTO Preset_Id VALUES
                 (1, 'Kick Bass', 1, 1, '/Library/Arturia/Presets/Mini V4/Factory/Factory/Kick Bass', 1, 0, 0),
                 (2, 'Tines', 2, 2, '/Library/Arturia/Presets/Stage-73 V2/Factory/Factory/Tines', 0, 0, 1),
                 (3, 'Hidden', 1, 1, '/Library/Arturia/Presets/Mini V4/Factory/Factory/Hidden', 0, 1, 0),
                 (4, 'Odd', 1, 1, 'Odd', 0, 0, 0);
             INSERT INTO Preset_Characteristics VALUES (1, 1, 1), (2, 1, 2), (3, 1, 3), (4, 1, 4),
                 (5, 2, 5);",
        )
        .unwrap();
    }

    #[test]
    fn reads_presets_with_their_tags() {
        let dir = temp("read");
        library_at(&dir);
        let found = read(&dir).unwrap();
        assert_eq!(
            found.len(),
            2,
            "hidden and unplaceable presets are left out"
        );
        let kick = &found[0];
        assert_eq!(kick.name, "Kick Bass");
        assert_eq!(kick.folder, "Mini V4");
        assert_eq!(kick.path, dir.join("Mini V4/Factory/Factory/Kick Bass"));
        assert_eq!(kick.types, ["Bass"]);
        assert_eq!(
            (kick.arturia_type.as_str(), kick.arturia_subtype.as_str()),
            ("Bass", "Plucked Bass")
        );
        assert_eq!(kick.genres, ["Dub / Reggae"]);
        assert_eq!(kick.styles, ["Warm"]);
        assert_eq!(kick.characters, ["Analog"]);
        assert!(kick.liked);
        let tines = &found[1];
        assert_eq!(tines.types, ["Electric Piano"]);
        assert_eq!(tines.styles, ["Punchy"]);
        assert!(tines.analog_lab && !tines.liked);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn places_windows_paths_too() {
        let dir = PathBuf::from("lib");
        assert_eq!(
            locate(&dir, r"C:\ProgramData\Arturia\Presets\Pigments\User\Mine"),
            Some((
                "Pigments".into(),
                dir.join("Pigments").join("User").join("Mine")
            ))
        );
        assert_eq!(locate(&dir, "/Library/Arturia/Presets/db.db3"), None);
    }

    #[test]
    fn a_preset_is_a_vst3_state_twice() {
        let file = b"22 serialization::archive 10 0 7 0 7 4 Kick".to_vec();
        assert!(is_preset(&file));
        let state = plugin_state("vst3:/x/Mini V4.vst3#ABC", &file, "Kick").unwrap();
        let (a, b) = opendj_engine::plugins::plugin_presets::unpack_states(&state).unwrap();
        assert_eq!((a, b), (&file[..], &file[..]));
    }

    #[test]
    fn a_preset_is_an_au_controller_state() {
        let file = b"22 serialization::archive 10".to_vec();
        let state = plugin_state("au:aumu:MIN4:Artu", &file, "Kick").unwrap();
        let v = plist::Value::from_reader(std::io::Cursor::new(state)).unwrap();
        let d = v.as_dictionary().unwrap();
        assert_eq!(
            d["type"].as_signed_integer(),
            Some(i64::from(u32::from_be_bytes(*b"aumu")))
        );
        assert_eq!(
            d["manufacturer"].as_signed_integer(),
            Some(i64::from(u32::from_be_bytes(*b"Artu")))
        );
        assert_eq!(d["Controller State"].as_data(), Some(&file[..]));
        assert!(plugin_state("clap:/x.clap#id", &file, "Kick").is_err());
    }
}
