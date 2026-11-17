//! HOLD for the instrument while the arpeggiator is off: the notes played
//! keep sounding after their keys are let go, until new keys are played
//! after letting go of all of them, or HOLD is switched off. With the
//! arpeggiator on, `[Instrument1],arp_hold` is its own hold instead (see
//! [`crate::arp`]).

use opendj_plugin_host::Midi;

/// The most notes it keeps at once.
const MAX: usize = 32;

pub struct Latch {
    on: bool,
    /// The notes sounding, as (channel, key).
    sounding: Vec<(u8, u8)>,
    /// Keys down now, as (channel, key).
    down: Vec<(u8, u8)>,
}

impl Default for Latch {
    fn default() -> Self {
        // Room for every note up front: the audio thread never allocates.
        Self {
            on: false,
            sounding: Vec::with_capacity(MAX),
            down: Vec::with_capacity(MAX),
        }
    }
}

fn push(out: &mut Vec<Midi>, m: Midi) {
    if out.len() < out.capacity() {
        out.push(m);
    }
}

fn off(out: &mut Vec<Midi>, (ch, key): (u8, u8)) {
    push(
        out,
        Midi {
            frame: 0,
            data: [0x80 | ch, key, 0],
        },
    );
}

impl Latch {
    /// HOLD on or off. Switching it off ends the notes it kept whose keys
    /// are up.
    pub fn set(&mut self, on: bool, out: &mut Vec<Midi>) {
        if on == self.on {
            return;
        }
        self.on = on;
        if !on {
            for &n in &self.sounding {
                if !self.down.contains(&n) {
                    off(out, n);
                }
            }
            self.sounding.clear();
        }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Forgets the notes it kept without ending them, as when the
    /// arpeggiator takes over (it ends every note itself).
    pub fn forget(&mut self) {
        self.sounding.clear();
        self.down.clear();
    }

    /// Passes `incoming` to `out`, keeping note-offs back while HOLD is on.
    pub fn process(&mut self, incoming: &[Midi], out: &mut Vec<Midi>) {
        for &m in incoming {
            if let Some((ch, key, _)) = m.as_note_on() {
                if self.on && self.down.is_empty() {
                    // New keys after letting go: the kept notes end.
                    for &n in &self.sounding {
                        off(out, n);
                    }
                    self.sounding.clear();
                }
                if self.down.len() < MAX && !self.down.contains(&(ch, key)) {
                    self.down.push((ch, key));
                }
                if self.on && self.sounding.len() < MAX && !self.sounding.contains(&(ch, key)) {
                    self.sounding.push((ch, key));
                }
                push(out, m);
            } else if let Some(n) = m.as_note_off() {
                self.down.retain(|d| *d != n);
                if !self.on || !self.sounding.contains(&n) {
                    push(out, m);
                }
            } else {
                if m.data[0] & 0xf0 == 0xb0 && matches!(m.data[1], 120 | 123) {
                    self.sounding.clear();
                    self.down.clear();
                }
                push(out, m);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(key: u8) -> Midi {
        Midi {
            frame: 0,
            data: [0x90, key, 100],
        }
    }

    fn off_(key: u8) -> Midi {
        Midi {
            frame: 0,
            data: [0x80, key, 0],
        }
    }

    fn run(l: &mut Latch, input: &[Midi]) -> Vec<(u8, u8)> {
        let mut out = Vec::with_capacity(64);
        l.process(input, &mut out);
        out.iter().map(|m| (m.data[0] & 0xf0, m.data[1])).collect()
    }

    #[test]
    fn hold_keeps_a_chord_until_new_keys() {
        let mut l = Latch::default();
        let mut out = Vec::with_capacity(64);
        l.set(true, &mut out);
        assert_eq!(
            run(&mut l, &[on(60), on(64), off_(60), off_(64)]),
            [(0x90, 60), (0x90, 64)]
        );
        // A new key after letting go ends the chord first.
        assert_eq!(run(&mut l, &[on(67)]), [(0x80, 60), (0x80, 64), (0x90, 67)]);
        // Switching HOLD off ends what it kept once the key is up.
        assert_eq!(run(&mut l, &[off_(67)]), []);
        l.set(false, &mut out);
        assert_eq!(out.iter().map(|m| m.data[1]).collect::<Vec<_>>(), [67]);
    }

    #[test]
    fn hold_off_passes_notes_through() {
        let mut l = Latch::default();
        assert_eq!(run(&mut l, &[on(60), off_(60)]), [(0x90, 60), (0x80, 60)]);
    }

    #[test]
    fn switching_hold_off_leaves_keys_still_down() {
        let mut l = Latch::default();
        let mut out = Vec::with_capacity(64);
        l.set(true, &mut out);
        run(&mut l, &[on(60), on(64), off_(64)]);
        l.set(false, &mut out);
        assert_eq!(out.iter().map(|m| m.data[1]).collect::<Vec<_>>(), [64]);
        // The key still down ends as usual.
        assert_eq!(run(&mut l, &[off_(60)]), [(0x80, 60)]);
    }
}
