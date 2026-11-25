//! Scale mode for the instrument: notes played move to the nearest note
//! of a scale, so any key played sounds in key with the mix.
//!
//! `[Instrument1],scale_mode` is 0 Off, 1 Auto (the master deck's key) or
//! 2 the root and scale picked (`scale_root` 0 to 11 from C, `scale_type`
//! an index into [`SCALES`]). `scale_mask` shows the notes the scale has
//! now, a bit per pitch class from C, and `scale_now` which scale that is,
//! 1 + type × 12 + root; both 0 while every note plays as it is.
//!
//! Chord mode (`chord_mode`, see [`CHORDS`]) plays a chord built on the
//! scale from each key; `chord_learn` takes the next chord held as Mine.
//! `scale_off` is what a key outside the scale does: 0 moves to the
//! nearest note (Snap), 1 plays nothing (Mute). A chord can strum: its
//! notes start `chord_strum` ms apart, from the lowest, the highest or
//! each way in turn (`chord_strum_from`, see [`STRUM_FROM`]), getting
//! louder or softer along the way (`chord_crescendo`, -1 to 1).

use opendj_plugin_host::Midi;

use crate::key::Key;

/// The scales to pick from, as `scale_type` numbers them.
pub const SCALES: [&str; 9] = [
    "Major",
    "Minor",
    "Major pentatonic",
    "Minor pentatonic",
    "Dorian",
    "Phrygian",
    "Mixolydian",
    "Harmonic minor",
    "Blues",
];

/// Each scale's notes, in semitones from its root.
const STEPS: [&[u8]; 9] = [
    &[0, 2, 4, 5, 7, 9, 11],
    &[0, 2, 3, 5, 7, 8, 10],
    &[0, 2, 4, 7, 9],
    &[0, 3, 5, 7, 10],
    &[0, 2, 3, 5, 7, 9, 10],
    &[0, 1, 3, 5, 7, 8, 10],
    &[0, 2, 4, 5, 7, 9, 10],
    &[0, 2, 3, 5, 7, 8, 11],
    &[0, 3, 5, 6, 7, 10],
];

/// `chord_strum_from` values: which end of a chord a strum starts from.
pub const STRUM_FROM: [&str; 3] = ["Low", "High", "Alternate"];
pub const FROM_LOW: u8 = 0;
pub const FROM_HIGH: u8 = 1;
pub const FROM_ALTERNATE: u8 = 2;

/// The longest strum, from one note to the next, in ms.
pub const MAX_STRUM_MS: f64 = 200.0;

/// How a chord plays out: the frames between its notes, which end goes
/// first, and how much louder (above 0) or softer (below) the later notes
/// get. `mute` silences keys outside the scale instead of moving them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Feel {
    pub strum: u32,
    pub from: u8,
    pub crescendo: f32,
    pub mute: bool,
}

/// A strummed note waiting for its frame (counted from this buffer's
/// start), and the key that played it.
#[derive(Clone, Copy)]
struct Pending {
    due: u32,
    ch: u8,
    key: u8,
    note: u8,
    vel: u8,
}

/// The most strummed notes waiting at once.
const MAX_PENDING: usize = 128;

/// `scale_mode` values.
pub const OFF: u8 = 0;
pub const AUTO: u8 = 1;
pub const PICKED: u8 = 2;

/// The notes of scale `scale` (see [`SCALES`]) on `root` (0 to 11 from C),
/// a bit per pitch class from C.
pub fn mask(root: u8, scale: usize) -> u16 {
    STEPS[scale.min(STEPS.len() - 1)]
        .iter()
        .fold(0, |m, s| m | 1 << ((root + s) % 12))
}

/// The scale a mode plays in, as (root, scale): none with Off, or with
/// Auto while no deck leads with a key (`master`).
pub fn scale_for(mode: u8, root: u8, scale: usize, master: Option<Key>) -> Option<(u8, usize)> {
    match mode {
        AUTO => master.map(|k| (k.tonic % 12, usize::from(k.minor))),
        PICKED => Some((root % 12, scale.min(STEPS.len() - 1))),
        _ => None,
    }
}

/// [`scale_for`]'s notes; 0 for none.
pub fn mask_for(mode: u8, root: u8, scale: usize, master: Option<Key>) -> u16 {
    scale_for(mode, root, scale, master).map_or(0, |(r, s)| mask(r, s))
}

/// The note of `mask` nearest `key`, the lower one when two are as near;
/// `key` itself with no scale.
pub fn nearest(key: u8, mask: u16) -> u8 {
    let in_scale = |k: i32| (0..=127).contains(&k) && mask >> (k % 12) & 1 == 1;
    if mask & 0xfff == 0 || in_scale(key as i32) {
        return key;
    }
    (1..12)
        .flat_map(|d| [key as i32 - d, key as i32 + d])
        .find(|&k| in_scale(k))
        .map_or(key, |k| k as u8)
}

/// `chord_mode` values: one key plays the key alone, or a chord built on
/// it. Triad and Seventh stack every other note of the scale (a major
/// triad and a dominant seventh with no scale); Power is root, fifth and
/// octave; Mine is the chord learned (`chord_shape`), moved into the scale.
pub const CHORDS: [&str; 5] = ["Off", "Triad", "Seventh", "Power", "Mine"];
pub const CHORD_OFF: u8 = 0;
pub const TRIAD: u8 = 1;
pub const SEVENTH: u8 = 2;
pub const POWER: u8 = 3;
pub const MINE: u8 = 4;

/// The learned chord before one is learned: a major triad. A bit per
/// semitone above the chord's lowest note.
pub const DEFAULT_SHAPE: u32 = 1 | 1 << 4 | 1 << 7;

/// The notes `key` plays in chord mode `mode`, scale `mask` (0 none) and
/// learned chord `shape`, a bit per note.
pub fn chord(key: u8, mask: u16, mode: u8, shape: u32) -> u128 {
    let root = nearest(key, mask);
    let bit = |n: i32| {
        if (0..=127).contains(&n) {
            1u128 << n
        } else {
            0
        }
    };
    let stacked = |count: usize, plain: &[i32]| -> u128 {
        if mask & 0xfff == 0 {
            return plain.iter().fold(0, |m, &i| m | bit(root as i32 + i));
        }
        // Every other scale note up from the root.
        let up = (root as i32..root as i32 + 36).filter(|&n| mask >> (n % 12) & 1 == 1);
        up.step_by(2).take(count).fold(0, |m, n| m | bit(n))
    };
    match mode {
        TRIAD => stacked(3, &[0, 4, 7]),
        SEVENTH => stacked(4, &[0, 4, 7, 10]),
        POWER => bit(root as i32) | bit(root as i32 + 7) | bit(root as i32 + 12),
        MINE if shape != 0 => (0..32)
            .filter(|i| shape >> i & 1 == 1)
            .map(|i| root as i32 + i)
            .filter(|n| (0..=127).contains(n))
            .fold(0, |m, n| m | bit(nearest(n as u8, mask) as i32)),
        _ => bit(root as i32),
    }
}

/// Moves notes to a scale, or plays a chord for each, as they come in,
/// and ends each where it began even if the scale or chord changed while
/// it was held. Two keys landing on the same note hold it until both are
/// let go.
pub struct NoteMapper {
    /// The notes each key plays, by channel and key.
    to: [[u128; 128]; 16],
    /// Keys holding each note, by channel and note.
    holding: [[u8; 128]; 16],
    mode: u8,
    shape: u32,
    /// Learning a chord: the notes played since, and the keys held now.
    learning: bool,
    learned: u128,
    learn_held: u8,
    /// A chord just learned, for `chord_shape`.
    new_shape: Option<u32>,
    feel: Feel,
    /// The next Alternate strum goes down.
    down_next: bool,
    /// Keys silenced for being outside the scale, a bit per key by channel.
    muted: [u128; 16],
    pending: Vec<Pending>,
}

impl Default for NoteMapper {
    fn default() -> Self {
        Self {
            to: [[0; 128]; 16],
            holding: [[0; 128]; 16],
            mode: CHORD_OFF,
            shape: DEFAULT_SHAPE,
            learning: false,
            learned: 0,
            learn_held: 0,
            new_shape: None,
            feel: Feel::default(),
            down_next: false,
            muted: [0; 16],
            pending: Vec::with_capacity(MAX_PENDING),
        }
    }
}

impl NoteMapper {
    /// The chord mode, learned chord and whether a chord is being learned
    /// (the keys then play as they are and the chord held becomes Mine).
    pub fn set_chord(&mut self, mode: u8, shape: u32, learn: bool) {
        self.mode = mode;
        self.shape = shape;
        if learn != self.learning {
            self.learning = learn;
            self.learned = 0;
            self.learn_held = 0;
        }
    }

    /// How chords strum and what keys outside the scale do.
    pub fn set_feel(&mut self, feel: Feel) {
        self.feel = feel;
    }

    /// Adds the strummed notes due in this buffer of `frames` to `out`,
    /// and counts the rest down to the next. Call once a buffer, after
    /// [`NoteMapper::map`].
    pub fn flush(&mut self, frames: usize, out: &mut Vec<Midi>) {
        let frames = frames as u32;
        self.pending.retain_mut(|p| {
            if p.due < frames {
                if out.len() < out.capacity() {
                    out.push(Midi::note_on(p.due, p.ch, p.note, p.vel));
                }
                false
            } else {
                p.due -= frames;
                true
            }
        });
    }

    /// Forgets the strummed notes `key` of `ch` has still to play.
    fn drop_pending(&mut self, ch: usize, key: usize) {
        self.pending
            .retain(|p| !(usize::from(p.ch) == ch && usize::from(p.key) == key));
    }

    /// The chord learned, once its keys are let go.
    pub fn take_learned(&mut self) -> Option<u32> {
        self.new_shape.take()
    }

    /// Lets go of `notes` once held by one key of `ch`, sending a note-off
    /// for each no other key holds.
    fn release(&mut self, ch: usize, notes: u128, frame: u32, out: &mut Vec<Midi>) {
        for n in (0..128).filter(|n| notes >> n & 1 == 1) {
            let held = &mut self.holding[ch][n];
            *held = held.saturating_sub(1);
            if *held == 0 && out.len() < out.capacity() {
                out.push(Midi::note_off(frame, ch as u8, n as u8));
            }
        }
    }

    /// Adds what `m` plays in scale `mask` to `out`, up to its capacity.
    pub fn map(&mut self, m: Midi, mask: u16, out: &mut Vec<Midi>) {
        if let Some((ch, key, _)) = m.as_note_on() {
            let (ch, key) = (ch as usize, key as usize);
            let mode = if self.learning { CHORD_OFF } else { self.mode };
            let outside = mask & 0xfff != 0 && mask >> (key % 12) & 1 == 0;
            let notes = if self.feel.mute && outside && !self.learning {
                0
            } else {
                chord(key as u8, mask, mode, self.shape)
            };
            // Played again without a note-off: let go of the old notes.
            self.drop_pending(ch, key);
            let old = std::mem::replace(&mut self.to[ch][key], notes);
            if old != 0 {
                self.release(ch, old, m.frame, out);
                self.learn_held = self.learn_held.saturating_sub(1);
            }
            if notes == 0 {
                self.muted[ch] |= 1 << key;
                return;
            }
            self.muted[ch] &= !(1 << key);
            if self.learning {
                self.learned |= notes;
                self.learn_held += 1;
            }
            let count = notes.count_ones();
            let down = match self.feel.from {
                FROM_HIGH => true,
                FROM_ALTERNATE => {
                    self.down_next = !self.down_next;
                    !self.down_next
                }
                _ => false,
            };
            let mut i = 0;
            for n in (0..128).filter(|n| notes >> n & 1 == 1) {
                self.holding[ch][n] = self.holding[ch][n].saturating_add(1);
            }
            let order = (0..128u32).map(|n| if down { 127 - n } else { n });
            for n in order.filter(|&n| notes >> n & 1 == 1) {
                let vel = feel_velocity(m.data[2], i, count, self.feel.crescendo);
                let due = m.frame + i * self.feel.strum;
                if i == 0 || self.feel.strum == 0 {
                    if out.len() < out.capacity() {
                        out.push(Midi::note_on(due, ch as u8, n as u8, vel));
                    }
                } else if self.pending.len() < MAX_PENDING {
                    self.pending.push(Pending {
                        due,
                        ch: ch as u8,
                        key: key as u8,
                        note: n as u8,
                        vel,
                    });
                }
                i += 1;
            }
        } else if let Some((ch, key)) = m.as_note_off() {
            let (ch, key) = (ch as usize, key as usize);
            if self.muted[ch] >> key & 1 == 1 {
                self.muted[ch] &= !(1 << key);
                return;
            }
            self.drop_pending(ch, key);
            let notes = std::mem::take(&mut self.to[ch][key]);
            if notes == 0 {
                if out.len() < out.capacity() {
                    out.push(m);
                }
                return;
            }
            self.release(ch, notes, m.frame, out);
            if self.learning {
                self.learn_held = self.learn_held.saturating_sub(1);
                if self.learn_held == 0 && self.learned != 0 {
                    let low = self.learned.trailing_zeros();
                    self.new_shape = Some((self.learned >> low) as u32);
                    self.learning = false;
                    self.learned = 0;
                }
            }
        } else {
            if m.data[0] & 0xf0 == 0xb0 && matches!(m.data[1], 120 | 123) {
                // Everything stops: forget every note (without the
                // allocation a fresh mapper would make).
                self.to = [[0; 128]; 16];
                self.holding = [[0; 128]; 16];
                self.muted = [0; 16];
                self.pending.clear();
                self.learning = false;
                self.learned = 0;
                self.learn_held = 0;
            }
            if out.len() < out.capacity() {
                out.push(m);
            }
        }
    }
}

/// The velocity of a chord's `i`-th note of `count`, played at `vel`:
/// with a crescendo above 0 the first notes are softer and the last at
/// `vel`, below 0 the other way round.
fn feel_velocity(vel: u8, i: u32, count: u32, crescendo: f32) -> u8 {
    if count < 2 || crescendo == 0.0 {
        return vel;
    }
    let t = i as f32 / (count - 1) as f32;
    let c = crescendo.clamp(-1.0, 1.0);
    let cut = if c > 0.0 { c * (1.0 - t) } else { -c * t };
    (f32::from(vel) * (1.0 - 0.7 * cut))
        .round()
        .clamp(1.0, 127.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_hold_their_notes() {
        assert_eq!(mask(0, 0), 0b1010_1011_0101, "C major: C D E F G A B");
        assert_eq!(mask(9, 1), mask(0, 0), "A minor has C major's notes");
        assert_eq!(mask(0, 2).count_ones(), 5);
        assert_eq!(mask(9, 3), mask(0, 2), "A minor pentatonic is C major's");
    }

    #[test]
    fn notes_move_to_the_nearest_in_the_scale() {
        let c_major = mask(0, 0);
        assert_eq!(nearest(60, c_major), 60);
        assert_eq!(nearest(61, c_major), 60, "C# goes down to C");
        assert_eq!(nearest(66, c_major), 65, "F# to F");
        assert_eq!(nearest(61, 0), 61, "no scale");
        // C major pentatonic has no F: F goes to E, the lower of E and G.
        let penta = mask(0, 2);
        assert_eq!(nearest(65, penta), 64);
        assert_eq!(nearest(71, penta), 72, "B up to C");
    }

    #[test]
    fn auto_follows_the_master_key_and_waits_without_one() {
        let a_minor = Key {
            tonic: 9,
            minor: true,
        };
        assert_eq!(mask_for(AUTO, 0, 0, Some(a_minor)), mask(9, 1));
        assert_eq!(mask_for(AUTO, 0, 0, None), 0);
        assert_eq!(mask_for(PICKED, 2, 3, Some(a_minor)), mask(2, 3));
        assert_eq!(mask_for(OFF, 2, 3, Some(a_minor)), 0);
    }

    fn play(s: &mut NoteMapper, m: Midi, mask: u16) -> Vec<[u8; 3]> {
        let mut out = Vec::with_capacity(16);
        s.map(m, mask, &mut out);
        out.iter().map(|m| m.data).collect()
    }

    #[test]
    fn a_note_ends_where_it_began() {
        let mut s = NoteMapper::default();
        let c_major = mask(0, 0);
        assert_eq!(
            play(&mut s, Midi::note_on(0, 0, 61, 100), c_major),
            [[0x90, 60, 100]]
        );
        // The scale changes while it is held: its note-off still ends 60.
        assert_eq!(
            play(&mut s, Midi::note_off(0, 0, 61), mask(2, 0)),
            [[0x80, 60, 0]]
        );
        // C and C# both play C: it sounds until both are let go.
        play(&mut s, Midi::note_on(0, 0, 60, 100), c_major);
        play(&mut s, Midi::note_on(0, 0, 61, 100), c_major);
        assert!(play(&mut s, Midi::note_off(0, 0, 61), c_major).is_empty());
        assert_eq!(
            play(&mut s, Midi::note_off(0, 0, 60), c_major),
            [[0x80, 60, 0]]
        );
    }

    fn notes(bits: u128) -> Vec<u8> {
        (0..128).filter(|n| bits >> n & 1 == 1).collect()
    }

    #[test]
    fn chords_are_built_on_the_scale() {
        let c_major = mask(0, 0);
        assert_eq!(notes(chord(60, c_major, TRIAD, 0)), [60, 64, 67], "C");
        assert_eq!(notes(chord(62, c_major, TRIAD, 0)), [62, 65, 69], "Dm");
        assert_eq!(
            notes(chord(71, c_major, SEVENTH, 0)),
            [71, 74, 77, 81],
            "Bm7b5"
        );
        assert_eq!(
            notes(chord(61, c_major, TRIAD, 0)),
            [60, 64, 67],
            "C# moves to C first"
        );
        assert_eq!(
            notes(chord(62, 0, TRIAD, 0)),
            [62, 66, 69],
            "no scale: major"
        );
        assert_eq!(notes(chord(60, 0, POWER, 0)), [60, 67, 72]);
        assert_eq!(notes(chord(60, 0, CHORD_OFF, 0)), [60]);
        // A minor seventh learned, played on D in C major: D F A C.
        let m7 = 1 | 1 << 3 | 1 << 7 | 1 << 10;
        assert_eq!(notes(chord(62, c_major, MINE, m7)), [62, 65, 69, 72]);
    }

    #[test]
    fn a_chord_ends_with_its_key() {
        let mut s = NoteMapper::default();
        s.set_chord(TRIAD, DEFAULT_SHAPE, false);
        let on = play(&mut s, Midi::note_on(0, 0, 60, 90), mask(0, 0));
        assert_eq!(on, [[0x90, 60, 90], [0x90, 64, 90], [0x90, 67, 90]]);
        // The chord mode changes while held: the same three notes end.
        s.set_chord(POWER, DEFAULT_SHAPE, false);
        let off = play(&mut s, Midi::note_off(0, 0, 60), mask(0, 0));
        assert_eq!(off, [[0x80, 60, 0], [0x80, 64, 0], [0x80, 67, 0]]);
    }

    #[test]
    fn a_chord_held_is_learned() {
        let mut s = NoteMapper::default();
        s.set_chord(MINE, DEFAULT_SHAPE, true);
        // Learning plays the keys as they are.
        for k in [57, 60, 64] {
            assert_eq!(play(&mut s, Midi::note_on(0, 0, k, 100), 0).len(), 1);
        }
        play(&mut s, Midi::note_off(0, 0, 60), 0);
        play(&mut s, Midi::note_off(0, 0, 57), 0);
        assert_eq!(s.take_learned(), None, "one key still held");
        play(&mut s, Midi::note_off(0, 0, 64), 0);
        assert_eq!(s.take_learned(), Some(1 | 1 << 3 | 1 << 7), "A minor");
    }

    #[test]
    fn new_scales_hold_their_notes() {
        assert_eq!(mask(2, 4), mask(0, 0), "D Dorian has C major's notes");
        assert_eq!(mask(4, 5), mask(0, 0), "E Phrygian too");
        assert_eq!(mask(7, 6), mask(0, 0), "and G Mixolydian");
        assert_eq!(
            mask(9, 7),
            mask(9, 1) & !(1 << 7) | 1 << 8,
            "A harmonic minor: G#"
        );
        assert_eq!(mask(0, 8).count_ones(), 6, "blues: six notes");
    }

    #[test]
    fn keys_outside_the_scale_can_be_muted() {
        let mut s = NoteMapper::default();
        s.set_feel(Feel {
            mute: true,
            ..Feel::default()
        });
        let c_major = mask(0, 0);
        assert!(play(&mut s, Midi::note_on(0, 0, 61, 90), c_major).is_empty());
        assert!(play(&mut s, Midi::note_off(0, 0, 61), c_major).is_empty());
        assert_eq!(play(&mut s, Midi::note_on(0, 0, 60, 90), c_major).len(), 1);
    }

    #[test]
    fn a_chord_strums() {
        let mut s = NoteMapper::default();
        s.set_chord(TRIAD, DEFAULT_SHAPE, false);
        s.set_feel(Feel {
            strum: 100,
            from: FROM_HIGH,
            crescendo: 1.0,
            mute: false,
        });
        let mut out = Vec::with_capacity(16);
        s.map(Midi::note_on(10, 0, 60, 100), 0, &mut out);
        assert_eq!(out.len(), 1, "the first note at once");
        assert_eq!((out[0].frame, out[0].data[1]), (10, 67), "from the top");
        s.flush(64, &mut out);
        assert_eq!(out.len(), 1, "the next is 100 frames on");
        s.flush(64, &mut out);
        assert_eq!((out[1].frame, out[1].data[1]), (110 - 64, 64));
        assert!(out[0].data[2] < out[1].data[2], "getting louder");
        // Let go before the last note: it never plays.
        out.clear();
        s.map(Midi::note_off(0, 0, 60), 0, &mut out);
        s.flush(256, &mut out);
        assert!(out.iter().all(|m| m.data[0] & 0xf0 == 0x80));
    }
}
