//! Player mode's Now Playing view (ui/nowplaying.slint): fills
//! `NowPlayingUi` with the track the Player bar shows, its art, title,
//! artist, album and year, while the view is open.

use std::time::Duration;

use opendj_engine::LoadedTracks;
use slint::{ComponentHandle, Model, Timer, TimerMode};

use crate::{AppWindow, NowPlayingUi, Theme, UpNextUi};

/// Starts filling the view. Keep the timer for as long as the app runs.
pub fn install(app: &AppWindow, loaded: LoadedTracks) -> Timer {
    let weak = app.as_weak();
    // The track and cover last shown, so the view is only set again when
    // they change.
    let mut shown = String::new();
    // The same for Up Next's Now playing row.
    let mut up_shown = String::new();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let Some(app) = weak.upgrade() else { return };
        show_up_next(&app, &mut up_shown);
        let ui = app.global::<NowPlayingUi>();
        if !ui.get_open() {
            shown.clear();
            return;
        }
        let d = app.get_player_deck().clamp(0, 1) as usize;
        let Some(info) = app.get_decks().row_data(d) else {
            return;
        };
        let track = loaded.read().ok().and_then(|t| t.get(d).cloned().flatten());
        let album = track
            .as_ref()
            .map(|t| album_line(&t.album, &t.info.year))
            .unwrap_or_default();
        let key = format!(
            "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
            info.loaded, info.title, info.artist, info.has_cover, album
        );
        if key == shown {
            return;
        }
        shown = key;
        let theme = app.global::<Theme>();
        ui.set_loaded(info.loaded);
        ui.set_title(info.title.clone());
        ui.set_artist(info.artist.clone());
        ui.set_cover(info.cover.clone());
        ui.set_has_cover(info.has_cover);
        ui.set_initials(info.initials.clone());
        ui.set_tint(theme.invoke_deck_color(d as i32));
        ui.set_album(album.into());
    });
    timer
}

/// Fills Up Next's Now playing row with the track the Player bar shows,
/// when it changed since `shown`.
fn show_up_next(app: &AppWindow, shown: &mut String) {
    let d = app.get_player_deck().clamp(0, 1) as usize;
    let Some(info) = app.get_decks().row_data(d) else {
        return;
    };
    let key = format!(
        "{}\u{1}{}\u{1}{}\u{1}{}",
        info.loaded, info.title, info.artist, info.has_cover
    );
    if key == *shown {
        return;
    }
    *shown = key;
    let ui = app.global::<UpNextUi>();
    ui.set_now_loaded(info.loaded);
    ui.set_now_title(info.title.clone());
    ui.set_now_artist(info.artist.clone());
    ui.set_now_cover(info.cover.clone());
    ui.set_now_has_cover(info.has_cover);
    ui.set_now_initials(info.initials.clone());
    ui.set_now_tint(app.global::<Theme>().invoke_deck_color(d as i32));
}

/// "Night Shift · 2024": the album and its year, either alone, or nothing.
fn album_line(album: &str, year: &str) -> String {
    match (album.trim(), year.trim()) {
        ("", "") => String::new(),
        (a, "") => a.to_string(),
        ("", y) => y.to_string(),
        (a, y) => format!("{a} · {y}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_and_year() {
        assert_eq!(album_line("Night Shift", "2024"), "Night Shift · 2024");
        assert_eq!(album_line("Night Shift", ""), "Night Shift");
        assert_eq!(album_line(" ", "2024"), "2024");
        assert_eq!(album_line("", ""), "");
    }
}
