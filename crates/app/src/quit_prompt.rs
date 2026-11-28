//! Asking before quitting. Closing the main window, or picking Quit from
//! the menu or its key, asks first while something would stop: a deck
//! playing, a recording, a broadcast on air: "openDJ is playing / Do you
//! really want to quit?", with Quit and Cancel. Cancel is the default,
//! so a stray Return doesn't end a set.
//!
//! Settings › Decks picks when to ask: only while something would stop
//! (the default), always, or never.

use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use slint::ComponentHandle;

use crate::settings;
use crate::{AppWindow, QuitPrompt};

pub const KEY: &str = "ask_before_quit";
/// The choices in Settings, in [`Mode`] order.
pub const LABELS: [&str; 3] = ["While playing or on air", "Always", "Never"];

/// When quitting asks first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Only while something would stop.
    Busy = 0,
    Always = 1,
    Never = 2,
}

impl Mode {
    fn from_index(i: usize) -> Self {
        match i {
            1 => Mode::Always,
            2 => Mode::Never,
            _ => Mode::Busy,
        }
    }
}

static MODE: AtomicU32 = AtomicU32::new(Mode::Busy as u32);

fn mode() -> Mode {
    Mode::from_index(MODE.load(Ordering::Relaxed) as usize)
}

/// Reads the saved choice; called once at start.
pub fn load() {
    let m = Mode::from_index(settings::number(KEY, 0.0).max(0.0) as usize);
    MODE.store(m as u32, Ordering::Relaxed);
}

/// Which of [`LABELS`] is in use.
pub fn chosen() -> usize {
    mode() as usize
}

/// Picks one of [`LABELS`] and saves it.
pub fn choose(index: usize) {
    let m = Mode::from_index(index);
    MODE.store(m as u32, Ordering::Relaxed);
    settings::set(KEY, serde_json::json!(m as u32));
}

/// What quitting now would stop.
#[derive(Debug, Default)]
pub struct Busy {
    /// The playing decks, numbered from 1.
    pub decks: Vec<usize>,
    pub recording: bool,
    pub on_air: bool,
}

/// The question's title, or None when quitting shouldn't ask. "openDJ is
/// playing" while something would stop (a deck, a recording, a broadcast),
/// with "Do you really want to quit?" under it; "Quit openDJ?" alone when
/// it asks with nothing running (Always).
pub fn question(mode: Mode, busy: &Busy) -> Option<&'static str> {
    let running = !busy.decks.is_empty() || busy.recording || busy.on_air;
    match mode {
        Mode::Never => None,
        Mode::Busy if !running => None,
        _ if running => Some(PLAYING),
        _ => Some("Quit openDJ?"),
    }
}

const PLAYING: &str = "openDJ is playing";

/// Asks before the window closes and before Quit from the menu or its key.
/// `busy` says what quitting would stop, each time it's asked.
pub fn install(app: &AppWindow, busy: impl Fn() -> Busy + 'static) {
    let busy = Rc::new(busy);
    let prompt = app.global::<QuitPrompt>();
    {
        let (weak, busy) = (app.as_weak(), busy.clone());
        prompt.on_request(move || {
            if let Some(app) = weak.upgrade() {
                if !ask(&app, &busy()) {
                    quit();
                }
            }
        });
    }
    prompt.on_quit(quit);
    {
        let weak = app.as_weak();
        prompt.on_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<QuitPrompt>().set_open(false);
            }
        });
    }
    let weak = app.as_weak();
    app.window()
        .on_close_requested(move || match weak.upgrade() {
            Some(app) if ask(&app, &busy()) => slint::CloseRequestResponse::KeepWindowShown,
            _ => {
                // The whole app, even with the mini player or karaoke open.
                quit();
                slint::CloseRequestResponse::HideWindow
            }
        });
}

/// Shows the question if quitting should ask now; whether it did.
fn ask(app: &AppWindow, busy: &Busy) -> bool {
    let Some(title) = question(mode(), busy) else {
        return false;
    };
    let prompt = app.global::<QuitPrompt>();
    prompt.set_title(title.into());
    let body = if title == PLAYING {
        "Do you really want to quit?"
    } else {
        ""
    };
    prompt.set_body(body.into());
    prompt.set_open(true);
    true
}

fn quit() {
    let _ = slint::quit_event_loop();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy(decks: &[usize], recording: bool, on_air: bool) -> Busy {
        Busy {
            decks: decks.to_vec(),
            recording,
            on_air,
        }
    }

    #[test]
    fn quits_without_asking_when_nothing_would_stop() {
        assert_eq!(question(Mode::Busy, &Busy::default()), None);
        assert_eq!(question(Mode::Never, &busy(&[1], true, true)), None);
    }

    #[test]
    fn always_asks_even_with_nothing_running() {
        assert_eq!(
            question(Mode::Always, &Busy::default()),
            Some("Quit openDJ?")
        );
    }

    #[test]
    fn says_opendj_is_playing_while_anything_would_stop() {
        for b in [
            busy(&[2], false, false),
            busy(&[1, 2], true, false),
            busy(&[], true, false),
            busy(&[], false, true),
        ] {
            assert_eq!(question(Mode::Busy, &b), Some(PLAYING));
            assert_eq!(question(Mode::Always, &b), Some(PLAYING));
        }
    }

    #[test]
    fn saved_choices_outside_the_list_mean_the_default() {
        assert_eq!(Mode::from_index(7), Mode::Busy);
        assert_eq!(Mode::from_index(2), Mode::Never);
    }
}
