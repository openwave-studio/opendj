//! The karaoke screen's saved look: the sung-word color (in Settings) and
//! whether the cover shows beside the lyrics (its own button and key).
//! Color indexes are saved and sent to the web page; append colors rather
//! than reorder them.
use serde_json::Value;
use std::cell::Cell;

pub(crate) const COLOR_KEY: &str = "karaoke_color";
pub(crate) const COVER_KEY: &str = "karaoke_cover";
pub(crate) const COLORS: [(&str, u32); 6] = [
    ("Lilac", 0xb69cff),
    ("Ocean", 0x66cfff),
    ("Mint", 0x79edba),
    ("Amber", 0xffce70),
    ("Rose", 0xff8eb6),
    ("White", 0xffffff),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Look {
    /// The cover beside the lyrics.
    pub cover: bool,
    color_index: usize,
}

impl Look {
    pub fn from_settings(saved: &Value) -> Self {
        Self {
            cover: saved[COVER_KEY].as_bool().unwrap_or(false),
            color_index: saved[COLOR_KEY]
                .as_u64()
                .unwrap_or(0)
                .min((COLORS.len() - 1) as u64) as usize,
        }
    }

    pub fn color_index(self) -> usize {
        self.color_index
    }

    pub fn color(self) -> u32 {
        COLORS[self.color_index].1
    }
}

thread_local! {
    static CACHED: Cell<Option<Look>> = const { Cell::new(None) };
}

/// Cached on the UI thread: the karaoke screen must never read settings
/// from disk while it draws.
pub(crate) fn current() -> Look {
    CACHED.with(|cached| {
        if let Some(look) = cached.get() {
            return look;
        }
        let look = Look::from_settings(&crate::settings::load());
        cached.set(Some(look));
        look
    })
}

fn save(look: Look) {
    crate::settings::set(COVER_KEY, serde_json::json!(look.cover));
    CACHED.with(|cached| cached.set(Some(look)));
}

pub(crate) fn toggle_cover() {
    let look = current();
    save(Look {
        cover: !look.cover,
        ..look
    });
}

/// A Settings menu pick; false when `id` isn't a karaoke row.
pub(crate) fn choose(id: &str, index: usize) -> bool {
    if id != COLOR_KEY {
        return false;
    }
    crate::settings::set(id, serde_json::json!(index.min(COLORS.len() - 1)));
    CACHED.with(|cached| cached.set(None));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_and_bad_values_keep_the_lyrics_screen() {
        for saved in [
            json!({}),
            json!({"karaoke_cover": "yes"}),
            json!({"karaoke_cover": 1}),
        ] {
            assert_eq!(Look::from_settings(&saved), Look::default());
        }
    }

    #[test]
    fn decodes_the_palette_and_bounds_color_indexes() {
        let look = Look::from_settings(&json!({"karaoke_color": 3}));
        assert_eq!(look.color(), 0xffce70);
        assert_eq!(
            Look::from_settings(&json!({"karaoke_color": 999})).color(),
            0xffffff
        );
    }
}
