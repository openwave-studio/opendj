//! The karaoke screen's sung-word color. What the screen shows (cover,
//! lyrics) is chosen on the screen itself.
use super::*;
use crate::karaoke_look::{self, Look, COLORS};

impl Prefs {
    pub(super) fn karaoke_items(&self) -> Vec<PrefItem> {
        let look = Look::from_settings(&settings::load());
        vec![
            item("", HEADING, "KARAOKE", ""),
            PrefItem {
                choices: strings(&COLORS.map(|(name, _)| name)),
                chosen: look.color_index() as i32,
                ..item(karaoke_look::COLOR_KEY, MENU, "Sung words", "The color the words fill with as they are sung, and the count-in dots, on the karaoke screen and its web page.")
            },
        ]
    }
}

pub(super) fn picked(id: &str, index: usize) -> bool {
    crate::karaoke_look::choose(id, index)
}
