//! The plug-ins openDJ's scan found (Settings › Plugins keeps them in
//! `audio-plugins.json`), for the browser's Plug-ins list. The studio never
//! scans: it reads what openDJ found, so a plug-in that crashes while it is
//! looked at never takes the studio down either.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::clip::{DeviceInfo, PluginFormat, PluginRef};

/// Every plug-in in openDJ's scan list, by name: instruments and effects.
pub fn load() -> Vec<DeviceInfo> {
    crate::settings::config_dir()
        .map(|d| d.join("audio-plugins.json"))
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .map(|v| from_json(&v))
        .unwrap_or_default()
}

/// The devices a scan list holds (Pure Data patches aren't in it).
pub fn from_json(v: &Value) -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    for f in list(&v["files"]) {
        let Some(path) = f["path"].as_str().map(PathBuf::from) else {
            continue;
        };
        let vst3 = is_vst3(&path);
        for d in list(&f["found"]) {
            let Some(id) = d["id"].as_str().map(String::from) else {
                continue;
            };
            let file = path.clone();
            let format = if vst3 {
                PluginFormat::Vst3File { file, id }
            } else {
                PluginFormat::Clap { file, id }
            };
            out.push(device(&d, format, if vst3 { "VST3" } else { "CLAP" }));
        }
    }
    for d in list(&v["audio_units"]) {
        let Some(id) = d["id"].as_str() else { continue };
        out.push(device(&d, PluginFormat::Au(id.into()), "AU"));
    }
    out.sort_by_key(|d| (d.name.to_lowercase(), d.kind.clone()));
    out
}

fn is_vst3(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("vst3"))
}

fn device(d: &Value, format: PluginFormat, kind: &str) -> DeviceInfo {
    let s = |k: &str| d[k].as_str().unwrap_or_default().to_string();
    let instrument = d["features"]
        .as_array()
        .is_some_and(|a| a.iter().any(|f| f.as_str() == Some("instrument")));
    let name = s("name");
    DeviceInfo {
        name: name.clone(),
        kind: kind.into(),
        on: true,
        plugin: Some(PluginRef {
            name,
            vendor: s("vendor"),
            format,
            instrument,
            state: None,
        }),
        builtin: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_scan_list() {
        let v = serde_json::json!({
            "files": [
                {"path": "/p/Delay.clap", "found": [
                    {"id": "com.x.delay", "name": "Delay", "vendor": "X", "features": ["audio-effect"]}
                ]},
                {"path": "/p/Synth.vst3", "found": [
                    {"id": "ABCD", "name": "Analog", "vendor": "Y", "features": ["instrument", "synth"]}
                ]},
                {"path": "/p/Broken.clap", "found": [], "error": "crashed"}
            ],
            "audio_units": [
                {"id": "aufx:dely:appl", "name": "AUDelay", "vendor": "Apple", "features": []}
            ]
        });
        let found = from_json(&v);
        let names: Vec<_> = found
            .iter()
            .map(|d| (d.name.as_str(), d.kind.as_str()))
            .collect();
        assert_eq!(
            names,
            [("Analog", "VST3"), ("AUDelay", "AU"), ("Delay", "CLAP")]
        );
        let p = found[0].plugin.as_ref().unwrap();
        assert!(p.instrument);
        assert_eq!(
            p.format,
            PluginFormat::Vst3File {
                file: "/p/Synth.vst3".into(),
                id: "ABCD".into()
            }
        );
        assert!(!found[2].plugin.as_ref().unwrap().instrument);
    }
}
