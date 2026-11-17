//! The karaoke screen's sung-word color, lines shown and fill. Whether the
//! cover shows is chosen on the screen itself.
use super::*;
use crate::karaoke_look::{self, Look, COLORS, FILLS, LINES};

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
            PrefItem {
                choices: strings(&LINES),
                chosen: look.many_lines as i32,
                ..item(karaoke_look::LINES_KEY, MENU, "Lines", "Few: the line to sing big in the middle, with the lines either side of it. Many: a column of the whole song that scrolls as it is sung. With the cover on, the lyrics are always a column.")
            },
            PrefItem {
                choices: strings(&FILLS),
                chosen: look.whole_line as i32,
                ..item(karaoke_look::FILL_KEY, MENU, "Light up", "Word by word: the line fills as each word is sung. Whole line: the line lights up all at once when it starts.")
            },
        ]
    }
}

pub(super) fn picked(id: &str, index: usize) -> bool {
    crate::karaoke_look::choose(id, index)
}
