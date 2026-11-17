//! How each controller's jog wheels feel, set in the Controller Editor's
//! Options: how strongly the rim bends the pitch and how tightly the deck
//! follows the platter when scratching. Each controller (by
//! [`crate::device_key`]) has its own choice; Normal leaves the mapping's
//! own numbers as they are.

use std::collections::HashMap;
use std::sync::Mutex;

/// Rim pitch bend: Fine, Normal, Strong. Multiplies what the mapping sends
/// to `jog` and `jog_bend`.
pub const BEND_CHOICES: [&str; 3] = ["Fine", "Normal", "Strong"];
const BEND_GAIN: [f64; 3] = [0.5, 1.0, 2.0];

/// Scratch: Loose, Normal, Tight. Multiplies the mapping's scratch filter
/// alpha by the first number and beta by the second, so a tighter platter
/// follows the hand sooner (and wobbles a little more on a steady push).
pub const SCRATCH_CHOICES: [&str; 3] = ["Loose", "Normal", "Tight"];
const SCRATCH_GAIN: [(f64, f64); 3] = [(0.5, 0.25), (1.0, 1.0), (2.0, 4.0)];

/// The Normal choice, for both settings.
pub const NORMAL: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feel {
    pub bend: usize,
    pub scratch: usize,
}

impl Default for Feel {
    fn default() -> Self {
        Self {
            bend: NORMAL,
            scratch: NORMAL,
        }
    }
}

static FEEL: Mutex<Option<HashMap<String, Feel>>> = Mutex::new(None);

/// The controller's choices (by [`crate::device_key`]).
pub fn feel(device: &str) -> Feel {
    FEEL.lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(device).copied())
        .unwrap_or_default()
}

/// Sets the controller's choices; takes effect at its next jog movement or
/// platter touch. Out of range choices count as Normal.
pub fn set_feel(device: &str, feel: Feel) {
    let feel = Feel {
        bend: if feel.bend < BEND_CHOICES.len() {
            feel.bend
        } else {
            NORMAL
        },
        scratch: if feel.scratch < SCRATCH_CHOICES.len() {
            feel.scratch
        } else {
            NORMAL
        },
    };
    let mut m = FEEL.lock().unwrap();
    let m = m.get_or_insert_with(HashMap::new);
    if feel == Feel::default() {
        m.remove(device);
    } else {
        m.insert(device.to_string(), feel);
    }
}

/// What the controller's rim and nudge steps (`jog`, `jog_bend`) are
/// multiplied by.
pub fn bend_gain(device: &str) -> f64 {
    BEND_GAIN[feel(device).bend]
}

/// What the controller's scratch filter alpha and beta are multiplied by.
pub fn scratch_gain(device: &str) -> (f64, f64) {
    SCRATCH_GAIN[feel(device).scratch]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_is_the_default_and_leaves_numbers_alone() {
        assert_eq!(bend_gain("jog-feel-test-a"), 1.0);
        assert_eq!(scratch_gain("jog-feel-test-a"), (1.0, 1.0));
        set_feel(
            "jog-feel-test-a",
            Feel {
                bend: 0,
                scratch: 2,
            },
        );
        assert_eq!(bend_gain("jog-feel-test-a"), 0.5);
        assert_eq!(scratch_gain("jog-feel-test-a"), (2.0, 4.0));
        set_feel(
            "jog-feel-test-a",
            Feel {
                bend: 9,
                scratch: 9,
            },
        );
        assert_eq!(feel("jog-feel-test-a"), Feel::default());
    }
}
