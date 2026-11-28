//! Asking before quitting. Closing the main window, or picking Quit from
//! the menu or its key, asks first while something would stop: a deck
//! playing, a recording, a broadcast on air. The question names what stops,
//! and Cancel is the default, so a stray Return doesn't end a set.
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

/// What the question says under "Quit openDJ?", or None when quitting
/// shouldn't ask. Empty when it asks with nothing running (Always).
pub fn question(mode: Mode, busy: &Busy) -> Option<String> {
    let mut parts = Vec::new();
    match busy.decks.as_slice() {
        [] => {}
        [d] => parts.push(format!("Deck {d} is playing")),
        decks => {
            let numbers: Vec<String> = decks.iter().map(|d| d.to_string()).collect();
            parts.push(format!("Decks {} are playing", and_list(&numbers)));
        }
    }
    if busy.recording {
        parts.push("the mix is recording".into());
    }
    if busy.on_air {
        parts.push("the broadcast is on air".into());
    }
    match mode {
        Mode::Never => None,
        Mode::Busy if parts.is_empty() => None,
        _ if parts.is_empty() => Some(String::new()),
        _ => {
            let mut text = and_list(&parts);
            if let Some(first) = text.get(..1) {
                text = first.to_uppercase() + &text[1..];
            }
            Some(format!("{text}."))
        }
    }
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

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
    let Some(body) = question(mode(), busy) else {
        return false;
    };
    let prompt = app.global::<QuitPrompt>();
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
            Some(String::new())
        );
    }

    #[test]
    fn names_what_would_stop() {
        assert_eq!(
            question(Mode::Busy, &busy(&[2], false, false)).as_deref(),
            Some("Deck 2 is playing.")
        );
        assert_eq!(
            question(Mode::Busy, &busy(&[1, 2], true, false)).as_deref(),
            Some("Decks 1 and 2 are playing and the mix is recording.")
        );
        assert_eq!(
            question(Mode::Always, &busy(&[1, 3, 4], true, true)).as_deref(),
            Some(
                "Decks 1, 3 and 4 are playing, the mix is recording and the broadcast is \
                 on air."
            )
        );
        assert_eq!(
            question(Mode::Busy, &busy(&[], false, true)).as_deref(),
            Some("The broadcast is on air.")
        );
    }

    #[test]
    fn saved_choices_outside_the_list_mean_the_default() {
        assert_eq!(Mode::from_index(7), Mode::Busy);
        assert_eq!(Mode::from_index(2), Mode::Never);
    }
}
