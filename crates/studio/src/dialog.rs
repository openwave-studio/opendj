//! The system's Open and Save dialogs, on their own thread so the window
//! keeps drawing: macOS asks AppleScript, Linux zenity, Windows its own.

use std::path::PathBuf;

/// Asks for a file to open; `picked` gets it on the dialog's thread, unless
/// the dialog was cancelled.
pub fn open_file(prompt: &str, picked: impl FnOnce(PathBuf) + Send + 'static) {
    let prompt = prompt.to_string();
    std::thread::spawn(move || {
        if let Some(path) = system_dialog(&prompt, None) {
            picked(path);
        }
    });
}

/// Asks where to save, starting from `name`; `picked` gets the path.
pub fn save_file(prompt: &str, name: &str, picked: impl FnOnce(PathBuf) + Send + 'static) {
    let prompt = prompt.to_string();
    let name = name.to_string();
    std::thread::spawn(move || {
        if let Some(path) = system_dialog(&prompt, Some(&name)) {
            picked(path);
        }
    });
}

#[cfg(windows)]
fn system_dialog(prompt: &str, save_as: Option<&str>) -> Option<PathBuf> {
    let dialog = rfd::FileDialog::new().set_title(prompt);
    match save_as {
        Some(name) => dialog
            .set_file_name(name)
            .add_filter(
                "openDJ Studio set",
                &[opendj_studio_engine::set_file::EXTENSION],
            )
            .save_file(),
        None => dialog
            .add_filter("Sets", &[opendj_studio_engine::set_file::EXTENSION, "als"])
            .pick_file(),
    }
}

#[cfg(not(windows))]
fn system_dialog(prompt: &str, save_as: Option<&str>) -> Option<PathBuf> {
    let output = if cfg!(target_os = "macos") {
        let script = match save_as {
            Some(name) => format!(
                "POSIX path of (choose file name with prompt {} default name {})",
                applescript_string(prompt),
                applescript_string(name)
            ),
            None => format!(
                "POSIX path of (choose file with prompt {})",
                applescript_string(prompt)
            ),
        };
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
    } else {
        let mut cmd = std::process::Command::new("zenity");
        cmd.arg("--file-selection").arg(format!("--title={prompt}"));
        if let Some(name) = save_as {
            cmd.arg("--save")
                .arg("--confirm-overwrite")
                .arg(format!("--filename={name}"));
        }
        cmd.output()
    };
    match output {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (!path.is_empty()).then(|| PathBuf::from(path))
        }
        // Cancelled (error -128), or the script failed: say which.
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.contains("-128") && !err.trim().is_empty() {
                eprintln!("file dialog failed: {}", err.trim());
            }
            None
        }
        Err(e) => {
            eprintln!("can't open a file dialog: {e}");
            None
        }
    }
}

/// `text` as an AppleScript string literal.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn applescript_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}
