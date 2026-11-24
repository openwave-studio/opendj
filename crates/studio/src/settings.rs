//! The studio's settings, in `studio.json` beside openDJ's `settings.json`.
//! Until the studio saves its own, it starts from openDJ's audio choices:
//! its main output, its first input, its sample rate and buffer.

use std::path::PathBuf;

use serde_json::{json, Value};

/// The sound card, as the Audio settings choose it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioSettings {
    /// The output device by name; empty for the system default.
    pub output: String,
    /// The input device by name (empty for the system default); None for
    /// no input.
    pub input: Option<String>,
    /// The rate to run at, if the output can; else the device's own. Read
    /// when the studio opens.
    pub sample_rate: Option<u32>,
    /// Frames per callback.
    pub buffer: u32,
}

/// Frames per callback unless the settings say otherwise: 5.3 ms at 48 kHz.
pub const DEFAULT_BUFFER: u32 = 256;

impl Default for AudioSettings {
    fn default() -> AudioSettings {
        AudioSettings {
            output: String::new(),
            input: Some(String::new()),
            sample_rate: None,
            buffer: DEFAULT_BUFFER,
        }
    }
}

/// openDJ's config folder (`OPENDJ_CONFIG`, else the system's).
fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("OPENDJ_CONFIG") {
        return Some(PathBuf::from(d));
    }
    let base = opendj_engine::paths::config_base()?;
    Some(base.join(opendj_engine::paths::app_folder_name()))
}

fn read(path: Option<PathBuf>) -> Value {
    path.and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}

/// The saved audio settings, else openDJ's, else the defaults.
pub fn audio() -> AudioSettings {
    let dir = config_dir();
    let own = read(dir.as_ref().map(|d| d.join("studio.json")));
    if own["audio"].is_object() {
        return from_studio(&own["audio"]);
    }
    from_opendj(&read(dir.map(|d| d.join("settings.json"))))
}

fn from_studio(a: &Value) -> AudioSettings {
    AudioSettings {
        output: a["output"].as_str().unwrap_or_default().to_string(),
        input: if a["input"].is_null() {
            None
        } else {
            Some(a["input"].as_str().unwrap_or_default().to_string())
        },
        sample_rate: a["sample_rate"].as_u64().map(|r| r as u32),
        buffer: a["buffer_frames"]
            .as_u64()
            .map_or(DEFAULT_BUFFER, |n| n as u32),
    }
}

/// openDJ's main output (its master route, or the older `main_device`),
/// its first input, rate and buffer.
fn from_opendj(v: &Value) -> AudioSettings {
    let routes = v["outputs"].as_array();
    let output = routes
        .and_then(|list| list.iter().find(|r| r["role"] == "master"))
        .and_then(|r| r["device"].as_str())
        .or_else(|| v["main_device"].as_str())
        .unwrap_or_default();
    let input = v["inputs"]
        .as_array()
        .and_then(|list| list.first())
        .and_then(|r| r["device"].as_str())
        .unwrap_or_default();
    AudioSettings {
        output: output.to_string(),
        input: Some(input.to_string()),
        sample_rate: v["sample_rate"].as_u64().map(|r| r as u32),
        buffer: v["buffer_frames"]
            .as_u64()
            .map_or(DEFAULT_BUFFER, |n| n as u32),
    }
}

/// Saves the audio settings in `studio.json`, keeping anything else there.
pub fn save_audio(a: &AudioSettings) {
    let Some(dir) = config_dir() else {
        return;
    };
    let path = dir.join("studio.json");
    let mut all = read(Some(path.clone()));
    if !all.is_object() {
        all = json!({});
    }
    all["audio"] = json!({
        "output": a.output,
        "input": a.input,
        "sample_rate": a.sample_rate,
        "buffer_frames": a.buffer,
    });
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_vec_pretty(&all) {
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("{}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_from_opendj_audio() {
        let v = json!({
            "outputs": [
                {"role": "headphones", "device": "Phones", "pair": 0},
                {"role": "master", "device": "Scarlett 4i4", "pair": 0},
            ],
            "inputs": [{"role": "microphone", "device": "Scarlett 4i4", "channel": 0}],
            "sample_rate": 48000,
            "buffer_frames": 128,
        });
        assert_eq!(
            from_opendj(&v),
            AudioSettings {
                output: "Scarlett 4i4".into(),
                input: Some("Scarlett 4i4".into()),
                sample_rate: Some(48_000),
                buffer: 128,
            }
        );
        assert_eq!(from_opendj(&Value::Null), AudioSettings::default());
        let own = json!({"output": "", "input": null, "buffer_frames": 512});
        assert_eq!(
            from_studio(&own),
            AudioSettings {
                input: None,
                buffer: 512,
                ..AudioSettings::default()
            }
        );
    }
}
