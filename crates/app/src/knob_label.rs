//! Knob names under the instrument deck's knobs (`ParamKnob` in
//! `ui/deck.slint`), in the condensed title font: on one line when the
//! name fits, else on two lines broken at a space, as even as they go.
//! Words are never split. A name that fits neither way steps down a size
//! or two before the last line is cut short with "…".

use read_fonts::FontRef;
use skrifa::MetadataProvider;

/// The font the names are set in: Roboto Condensed Medium (the UI's
/// `Theme.title-font`, weight 500).
const FONT: &[u8] = include_bytes!("../ui/fonts/RobotoCondensed-Medium.ttf");

/// How much smaller each step down is, and how many there are.
const STEP: f32 = 1.0;
const STEPS: usize = 2;

/// A name laid out to fit: its lines joined by "\n", the size to set it at
/// and whether some of it had to be cut.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub size: f32,
    pub cut: bool,
}

/// How wide `text` is in the label font at `size` pixels: the glyphs'
/// advances, without kerning.
fn width(text: &str, size: f32) -> f32 {
    let Ok(font) = FontRef::new(FONT) else {
        return text.chars().count() as f32 * size * 0.45;
    };
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(
        skrifa::instance::Size::new(size),
        skrifa::instance::LocationRef::default(),
    );
    text.chars()
        .map(|c| {
            charmap
                .map(c)
                .and_then(|g| metrics.advance_width(g))
                .unwrap_or(size * 0.45)
        })
        .sum()
}

/// `name` in at most `lines` lines no wider than `max` at `size`, or `None`
/// if a word can't fit.
fn wrap(name: &str, size: f32, max: f32, lines: usize) -> Option<String> {
    if width(name, size) <= max {
        return Some(name.to_string());
    }
    if lines < 2 {
        return None;
    }
    let words: Vec<&str> = name.split_whitespace().collect();
    (1..words.len())
        .map(|i| (words[..i].join(" "), words[i..].join(" ")))
        .map(|(a, b)| (width(&a, size).max(width(&b, size)), a, b))
        .filter(|(w, _, _)| *w <= max)
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, a, b)| format!("{a}\n{b}"))
}

/// `text` cut short with "…" to fit `max`.
fn elide(text: &str, size: f32, max: f32) -> String {
    if width(text, size) <= max {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while chars.len() > 1 {
        chars.pop();
        let s: String = chars.iter().collect();
        let s = format!("{}…", s.trim_end());
        if width(&s, size) <= max {
            return s;
        }
    }
    "…".into()
}

/// `name` laid out for a knob label `max` pixels wide, at `size` pixels or
/// a step or two smaller, in at most `lines` lines.
pub fn layout(name: &str, max: f32, size: f32, lines: usize) -> Label {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    for step in 0..=STEPS {
        let s = size - step as f32 * STEP;
        if let Some(text) = wrap(&name, s, max, lines) {
            return Label {
                text,
                size: s,
                cut: false,
            };
        }
    }
    // Nothing fits whole: the smallest size, whole words on the first
    // line, the rest cut short on the last.
    let s = size - STEPS as f32 * STEP;
    let words: Vec<&str> = name.split_whitespace().collect();
    let text = if lines < 2 || words.len() < 2 {
        elide(&name, s, max)
    } else {
        let mut first = words[0].to_string();
        let mut i = 1;
        while i < words.len() && width(&format!("{first} {}", words[i]), s) <= max {
            first = format!("{first} {}", words[i]);
            i += 1;
        }
        format!(
            "{}\n{}",
            elide(&first, s, max),
            elide(&words[i..].join(" "), s, max)
        )
    };
    Label {
        text,
        size: s,
        cut: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_stay_on_one_line() {
        let l = layout("Cutoff", 78.0, 11.5, 2);
        assert_eq!(
            l,
            Label {
                text: "Cutoff".into(),
                size: 11.5,
                cut: false
            }
        );
    }

    #[test]
    fn long_names_break_at_a_space_evenly() {
        let l = layout("Filter Envelope Amount", 78.0, 11.5, 2);
        assert!(!l.cut);
        assert_eq!(l.text, "Filter Envelope\nAmount");
        let l = layout("Oscillator 1 Semitones", 78.0, 11.5, 2);
        assert_eq!(l.text, "Oscillator 1\nSemitones");
    }

    #[test]
    fn words_are_never_split() {
        for name in [
            "OSC Mod Amount",
            "Osc 2 Unison Voices",
            "Modulation Wheel Depth",
            "Sub Oscillator Volume",
        ] {
            let l = layout(name, 78.0, 11.5, 2);
            assert!(!l.cut, "{name}");
            let words: Vec<&str> = name.split(' ').collect();
            let got: Vec<&str> = l.text.split(['\n', ' ']).collect();
            assert_eq!(got, words, "{name}");
        }
    }

    #[test]
    fn one_line_steps_down_then_cuts() {
        let l = layout("Velocity Sensitivity", 66.0, 11.0, 1);
        assert!(l.size < 11.0);
        let l = layout("Filter Envelope Amount Depth", 40.0, 11.0, 1);
        assert!(l.cut && l.text.ends_with('…'));
    }

    #[test]
    fn spaces_are_tidied() {
        assert_eq!(layout("  FB  / Ext In ", 78.0, 11.5, 2).text, "FB / Ext In");
    }
}
