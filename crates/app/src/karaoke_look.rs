//! Saved karaoke presentation choices, shared by settings and playback.
//! IDs are persisted and sent to the web page; append styles rather than reorder them.
use serde_json::Value;
use std::cell::Cell;

pub(crate) const STYLE_KEY: &str = "karaoke_style";
pub(crate) const COLOR_KEY: &str = "karaoke_color";
pub(crate) const STYLES: [&str; 2] = ["Original", "Artwork lyrics"];
pub(crate) const COLORS: [(&str, u32); 6] = [
    ("Lilac", 0xb69cff),
    ("Ocean", 0x66cfff),
    ("Mint", 0x79edba),
    ("Amber", 0xffce70),
    ("Rose", 0xff8eb6),
    ("White", 0xffffff),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum Style {
    #[default]
    Original = 0,
    Artwork = 1,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Look {
    pub style: Style,
    color_index: usize,
}

impl Look {
    pub fn from_settings(saved: &Value) -> Self {
        Self {
            style: match saved[STYLE_KEY].as_u64() {
                Some(1) => Style::Artwork,
                _ => Style::Original,
            },
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

/// Cache on the UI thread: lyric animation must never read settings from disk.
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

pub(crate) fn choose(id: &str, index: usize) -> bool {
    let count = match id {
        STYLE_KEY => STYLES.len(),
        COLOR_KEY => COLORS.len(),
        _ => return false,
    };
    crate::settings::set(id, serde_json::json!(index.min(count - 1)));
    CACHED.with(|cached| cached.set(None));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn defaults_and_invalid_styles_preserve_original() {
        for saved in [
            json!({}),
            json!({"karaoke_style":99}),
            json!({"karaoke_style":-1}),
            json!({"karaoke_style":"artwork"}),
        ] {
            assert_eq!(Look::from_settings(&saved), Look::default());
        }
    }
    #[test]
    fn decodes_the_shared_palette_and_bounds_color_indices() {
        let look = Look::from_settings(&json!({"karaoke_style":1, "karaoke_color":2}));
        assert_eq!(look.style, Style::Artwork);
        assert_eq!(look.color(), 0x79edba);
        assert_eq!(
            Look::from_settings(&json!({"karaoke_color":999})).color(),
            0xffffff
        );
    }
}
