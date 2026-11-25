//! Scales, roots and pad layouts: the key the pads play in and stage clips
//! are written in. The scale list follows the ones Push players know from
//! Live.

use crate::clip::Sound;

/// A scale: its name and the semitones above the root of its degrees.
pub struct Scale {
    pub name: &'static str,
    pub steps: &'static [u8],
}

const fn s(name: &'static str, steps: &'static [u8]) -> Scale {
    Scale { name, steps }
}

pub const SCALES: [Scale; 32] = [
    s("Major", &[0, 2, 4, 5, 7, 9, 11]),
    s("Minor", &[0, 2, 3, 5, 7, 8, 10]),
    s("Dorian", &[0, 2, 3, 5, 7, 9, 10]),
    s("Mixolydian", &[0, 2, 4, 5, 7, 9, 10]),
    s("Lydian", &[0, 2, 4, 6, 7, 9, 11]),
    s("Phrygian", &[0, 1, 3, 5, 7, 8, 10]),
    s("Locrian", &[0, 1, 3, 5, 6, 8, 10]),
    s("Harmonic Minor", &[0, 2, 3, 5, 7, 8, 11]),
    s("Melodic Minor", &[0, 2, 3, 5, 7, 9, 11]),
    s("Minor Pentatonic", &[0, 3, 5, 7, 10]),
    s("Major Pentatonic", &[0, 2, 4, 7, 9]),
    s("Minor Blues", &[0, 3, 5, 6, 7, 10]),
    s("Major Blues", &[0, 2, 3, 4, 7, 9]),
    s("Whole Tone", &[0, 2, 4, 6, 8, 10]),
    s("Half-Whole Dim", &[0, 1, 3, 4, 6, 7, 9, 10]),
    s("Whole-Half Dim", &[0, 2, 3, 5, 6, 8, 9, 11]),
    s("Super Locrian", &[0, 1, 3, 4, 6, 8, 10]),
    s("Lydian Dominant", &[0, 2, 4, 6, 7, 9, 10]),
    s("Spanish", &[0, 1, 4, 5, 7, 8, 10]),
    s("Eight-Tone Spanish", &[0, 1, 3, 4, 5, 6, 8, 10]),
    s("Hungarian Minor", &[0, 2, 3, 6, 7, 8, 11]),
    s("Bhairav", &[0, 1, 4, 5, 7, 8, 11]),
    s("Persian", &[0, 1, 4, 5, 6, 8, 11]),
    s("Neapolitan Major", &[0, 1, 3, 5, 7, 9, 11]),
    s("Neapolitan Minor", &[0, 1, 3, 5, 7, 8, 11]),
    s("Enigmatic", &[0, 1, 4, 6, 8, 10, 11]),
    s("Prometheus", &[0, 2, 4, 6, 9, 10]),
    s("Egyptian", &[0, 2, 5, 7, 10]),
    s("Hirajoshi", &[0, 2, 3, 7, 8]),
    s("In-Sen", &[0, 1, 5, 7, 10]),
    s("Iwato", &[0, 1, 5, 6, 10]),
    s("Kumoi", &[0, 2, 3, 7, 9]),
];

pub const NOTE_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// How notes are laid out on the pads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// A step right, a fourth up (Push's default).
    FourthUp,
    /// A step up, a fourth right.
    FourthRight,
    /// A step right, a third up.
    ThirdUp,
    /// Each row an octave above the one below.
    OctaveUp,
    /// The notes in a line, eight to a row.
    Sequential,
}

impl Layout {
    pub const ALL: [Layout; 5] = [
        Layout::FourthUp,
        Layout::FourthRight,
        Layout::ThirdUp,
        Layout::OctaveUp,
        Layout::Sequential,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Layout::FourthUp => "4th ↑",
            Layout::FourthRight => "4th →",
            Layout::ThirdUp => "3rd ↑",
            Layout::OctaveUp => "Octave ↑",
            Layout::Sequential => "Sequent.",
        }
    }
}

/// The key the pads play in and stage clips are written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// Index into [`SCALES`].
    pub scale: usize,
    /// 0 (C) to 11 (B).
    pub root: u8,
    pub layout: Layout,
    /// Every semitone on the pads, with notes outside the scale dimmed;
    /// otherwise only the scale's notes.
    pub chromatic: bool,
}

impl Default for Key {
    /// A minor: the demo set's key.
    fn default() -> Key {
        Key {
            scale: 1,
            root: 9,
            layout: Layout::FourthUp,
            chromatic: false,
        }
    }
}

impl Key {
    pub fn scale(&self) -> &'static Scale {
        &SCALES[self.scale % SCALES.len()]
    }

    pub fn name(&self) -> String {
        format!(
            "{} {}",
            NOTE_NAMES[self.root as usize % 12],
            self.scale().name
        )
    }

    /// Semitones above the root of scale degree `d` (may pass the octave).
    pub fn degree(&self, d: i32) -> i32 {
        let steps = self.scale().steps;
        let n = steps.len() as i32;
        steps[d.rem_euclid(n) as usize] as i32 + 12 * d.div_euclid(n)
    }

    /// The root note in the octave a melodic sound's pads start from.
    pub fn home(&self, sound: Sound) -> i32 {
        let c = match sound {
            Sound::Bass => 24,
            Sound::Pad => 36,
            _ => 48,
        };
        c + self.root as i32 % 12
    }

    /// The note of pad (`row` from the bottom, `col`) above `home`.
    pub fn pad(&self, home: i32, row: usize, col: usize) -> i32 {
        let (row, col) = (row as i32, col as i32);
        let n = self.scale().steps.len() as i32;
        if self.chromatic {
            home + match self.layout {
                Layout::FourthUp => row * 5 + col,
                Layout::FourthRight => col * 5 + row,
                Layout::ThirdUp => row * 4 + col,
                Layout::OctaveUp => row * 12 + col,
                Layout::Sequential => row * 8 + col,
            }
        } else {
            home + self.degree(match self.layout {
                Layout::FourthUp => row * 3 + col,
                Layout::FourthRight => col * 3 + row,
                Layout::ThirdUp => row * 2 + col,
                Layout::OctaveUp => row * n + col,
                Layout::Sequential => row * 8 + col,
            })
        }
    }

    /// Whether `pitch` is in the scale.
    pub fn in_scale(&self, pitch: i32) -> bool {
        let pc = (pitch - self.root as i32).rem_euclid(12) as u8;
        self.scale().steps.contains(&pc)
    }

    pub fn is_root(&self, pitch: i32) -> bool {
        (pitch - self.root as i32).rem_euclid(12) == 0
    }

    /// The scale `by` places along the list, wrapping.
    pub fn next_scale(&mut self, by: i32) {
        let n = SCALES.len() as i32;
        self.scale = (self.scale as i32 + by).rem_euclid(n) as usize;
    }

    pub fn next_layout(&mut self, by: i32) {
        let n = Layout::ALL.len() as i32;
        let i = Layout::ALL
            .iter()
            .position(|l| *l == self.layout)
            .unwrap_or(0) as i32;
        self.layout = Layout::ALL[(i + by).rem_euclid(n) as usize];
    }
}

/// The roots on Push's buttons, as Push lays them out: around the circle
/// of fifths, C G D A E B over F Bb Eb Ab Db Gb.
pub const UPPER_ROOTS: [u8; 6] = [0, 7, 2, 9, 4, 11];
pub const LOWER_ROOTS: [u8; 6] = [5, 10, 3, 8, 1, 6];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minor_matches_the_old_fixed_key() {
        let k = Key::default();
        assert_eq!(k.name(), "A Minor");
        // The lead's lowest pad was A3 (57), a fourth up is D4.
        let home = k.home(Sound::Lead);
        assert_eq!(home, 57);
        assert_eq!(k.home(Sound::Bass), 33);
        assert_eq!(k.pad(home, 0, 1), 59);
        assert_eq!(k.pad(home, 1, 0), 62);
        assert_eq!(k.pad(home, 0, 7), 69);
    }

    #[test]
    fn layouts_and_chromatic() {
        let mut k = Key {
            scale: 0,
            root: 0,
            ..Key::default()
        };
        let home = k.home(Sound::Lead);
        assert_eq!(home, 48);
        k.layout = Layout::OctaveUp;
        assert_eq!(k.pad(home, 1, 0), 60);
        k.layout = Layout::ThirdUp;
        assert_eq!(k.pad(home, 1, 0), 52);
        k.chromatic = true;
        k.layout = Layout::FourthUp;
        assert_eq!(k.pad(home, 0, 1), 49);
        assert!(!k.in_scale(49));
        assert!(k.in_scale(50));
        assert!(k.is_root(60));
        // A pentatonic scale wraps after five degrees.
        let p = Key {
            scale: 9,
            ..Key::default()
        };
        assert_eq!(p.degree(5), 12);
        assert_eq!(p.degree(-1), -2);
    }
}
