//! Words the app shares with controller scripts, for controllers with a
//! screen: the instrument's name, its preset, its knobs' names. Controls
//! hold numbers only; these are set by the app as they change and read by
//! scripts with `engine.getText(group, key)`.
//!
//! The keys the app keeps (see `crates/app/src/presets.rs`):
//!
//! - `[Instrument1],name`: the instrument playing ("" for none)
//! - `[Instrument1],preset`: its preset, if one is loaded unchanged
//! - `[Instrument1],parameterN_name`: what knob N turns
//! - `[Instrument1],instruments`: every instrument that can play, one a
//!   line, in the order `[Instrument1],instrument` counts them (1 = first)
//! - `[Instrument1],presets`: the instrument's presets, one a line, in the
//!   order `[Instrument1],preset_load` counts them (1 = first)

use std::collections::HashMap;
use std::sync::Mutex;

static TEXTS: Mutex<Option<HashMap<(String, String), String>>> = Mutex::new(None);

/// Sets the text `group`,`key`; an empty text removes it.
pub fn set_text(group: &str, key: &str, text: &str) {
    let mut texts = TEXTS.lock().unwrap();
    let texts = texts.get_or_insert_with(HashMap::new);
    if text.is_empty() {
        texts.remove(&(group.to_string(), key.to_string()));
    } else {
        texts.insert((group.to_string(), key.to_string()), text.to_string());
    }
}

/// The text `group`,`key`, empty when none was set.
pub fn text(group: &str, key: &str) -> String {
    TEXTS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|t| t.get(&(group.to_string(), key.to_string())).cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_set_read_and_removed() {
        assert_eq!(text("[Test]", "name"), "");
        set_text("[Test]", "name", "Surge XT");
        assert_eq!(text("[Test]", "name"), "Surge XT");
        set_text("[Test]", "name", "");
        assert_eq!(text("[Test]", "name"), "");
    }
}
