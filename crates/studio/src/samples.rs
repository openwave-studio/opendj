//! The browser's Samples: audio files from the open set's folder and the
//! usual sample folders, found on a thread of their own. Dragging one onto
//! a MIDI track plays it on a Simpler.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The open set's folder (its Samples sit beside it).
pub static SET_FOLDER: OnceLock<PathBuf> = OnceLock::new();

/// The most files the list holds.
const MAX_FILES: usize = 5_000;
/// How deep it looks into each folder.
const MAX_DEPTH: usize = 8;
const EXTENSIONS: &[&str] = &["wav", "aif", "aiff", "flac", "mp3", "ogg", "m4a"];

/// The folders it looks in, those that exist.
pub fn folders() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(f) = SET_FOLDER.get() {
        list.push(f.clone());
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        list.push(home.join("Music/openDJ/Samples"));
        // A Live user's own library of samples.
        list.push(home.join("Music/Ableton/User Library/Samples"));
    }
    list.retain(|f| f.is_dir());
    list.dedup();
    list
}

/// Every sample under `folders`, by name, each with the folder it's in.
pub fn scan(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for f in folders {
        walk(f, 0, &mut found);
    }
    found.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    found.dedup();
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || found.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if found.len() >= MAX_FILES {
            return;
        }
        let hidden = p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if p.is_dir() {
            walk(&p, depth + 1, found);
        } else if is_sample(&p) {
            found.push(p);
        }
    }
}

pub fn is_sample(p: &Path) -> bool {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| EXTENSIONS.contains(&e.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_audio_files_by_name_and_skips_the_rest() {
        let dir = std::env::temp_dir().join(format!("studio-samples-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Drums/Kicks")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        for f in [
            "Drums/Kicks/b kick.wav",
            "Drums/a snare.AIF",
            "notes.txt",
            ".hidden/c.wav",
        ] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let found = scan(std::slice::from_ref(&dir));
        let names: Vec<_> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, ["a snare.AIF", "b kick.wav"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
