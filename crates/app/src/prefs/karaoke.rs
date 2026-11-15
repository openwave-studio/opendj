//! Karaoke presentation choices, kept off the audience screen.
use super::*;
use crate::karaoke_look::{self, Look, COLORS, STYLES};

impl Prefs {
    pub(super) fn karaoke_items(&self) -> Vec<PrefItem> {
        let look = Look::from_settings(&settings::load());
        vec![
            item("", HEADING, "KARAOKE", ""),
            PrefItem {
                choices: strings(&STYLES),
                chosen: look.style as i32,
                ..item(karaoke_look::STYLE_KEY, MENU, "Screen style", "Original keeps the centered karaoke screen. Artwork lyrics uses a soft cover background and large, left-aligned lyrics. Applies live to the window and web page.")
            },
            PrefItem {
                choices: strings(&COLORS.map(|(name, _)| name)),
                chosen: look.color_index() as i32,
                ..item(karaoke_look::COLOR_KEY, MENU, "Sung words", "Original style’s highlight color for sung words and count-in dots.")
            },
        ]
    }
}

pub(super) fn picked(id: &str, index: usize) -> bool {
    crate::karaoke_look::choose(id, index)
}
