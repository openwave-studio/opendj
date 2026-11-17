//! Tap tempo: taps in time give a tempo.
//!
//! The tempo is a straight line fitted through the last [`TAPS`] taps (beat
//! number against time), so every tap counts, not just the first and last.
//! Taps further apart than [`TAP_GAP`] start over. A tap that is not on a beat
//! of the tempo so far (a slip) is left out; two in a row are a new tempo
//! and start over from them. A tap that lands a whole number of beats on (a
//! missed beat) still counts, as that beat.

use opendj_control::ControlHandle;

/// Taps further apart than this, in seconds, start a new tap tempo.
pub(crate) const TAP_GAP: f64 = 2.0;
/// Taps the tempo is fitted through.
const TAPS: usize = 16;
/// Taps before there is a tempo: two intervals, as one is too rough.
const FIRST: usize = 3;
/// How far off a beat, as a fraction of one, a tap may land and still count.
const SLIP: f64 = 0.15;
/// Missed beats a tap may skip and still count.
const MAX_SKIP: f64 = 4.0;

/// The most a press may wait for the audio thread to see it, in seconds:
/// about a large buffer.
const MAX_WAIT: f64 = 0.1;

/// When a press of `tap`, seen in the buffer starting at `now` on the engine
/// clock, really came: the audio thread looks once a buffer, which alone
/// would put taps up to a buffer late and make the tempo jump.
pub(crate) fn tap_time(now: f64, tap: &ControlHandle) -> f64 {
    now - tap.pressed_ago().unwrap_or(0.0).clamp(0.0, MAX_WAIT)
}

#[derive(Default)]
pub(crate) struct TapTempo {
    /// The taps counted so far, oldest first: beat number and time.
    taps: [(f64, f64); TAPS],
    n: usize,
    /// The last tap, counted or not, for the gap to the next one.
    last: Option<f64>,
    /// A tap left out as off the beat, which a second one would start over
    /// from.
    slip: Option<f64>,
}

impl TapTempo {
    /// A tap at `now` seconds; the tapped BPM once there are enough in time.
    pub(crate) fn tap(&mut self, now: f64) -> Option<f64> {
        let gap = self.last.map_or(f64::INFINITY, |last| now - last);
        if gap <= 0.0 {
            // The same press twice (or out of order): nothing new.
            return None;
        }
        self.last = Some(now);
        if gap > TAP_GAP {
            self.start(&[now]);
            return None;
        }
        let (last_beat, last_time) = self.taps[self.n - 1];
        let beat = if self.n >= FIRST {
            let beats = (now - last_time) / self.period();
            let whole = beats.round();
            if !(1.0..=MAX_SKIP).contains(&whole) || (beats - whole).abs() > SLIP {
                match self.slip.take() {
                    Some(slip) => self.start(&[slip, now]),
                    None => self.slip = Some(now),
                }
                return self.bpm();
            }
            last_beat + whole
        } else {
            last_beat + 1.0
        };
        self.slip = None;
        if self.n == TAPS {
            self.taps.rotate_left(1);
            self.n -= 1;
        }
        self.taps[self.n] = (beat, now);
        self.n += 1;
        self.bpm()
    }

    fn start(&mut self, times: &[f64]) {
        self.n = times.len();
        for (i, &t) in times.iter().enumerate() {
            self.taps[i] = (i as f64, t);
        }
        self.slip = None;
    }

    fn bpm(&self) -> Option<f64> {
        (self.n >= FIRST).then(|| 60.0 / self.period())
    }

    /// Seconds a beat, fitted by least squares through the counted taps.
    fn period(&self) -> f64 {
        let taps = &self.taps[..self.n];
        let k = taps.len() as f64;
        let mb = taps.iter().map(|t| t.0).sum::<f64>() / k;
        let mt = taps.iter().map(|t| t.1).sum::<f64>() / k;
        let (mut cov, mut var) = (0.0, 0.0);
        for &(b, t) in taps {
            cov += (b - mb) * (t - mt);
            var += (b - mb) * (b - mb);
        }
        cov / var
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little random number generator, so the tests repeat.
    struct Rng(u64);
    impl Rng {
        /// Uniform in -1..1.
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        }
    }

    /// Taps at `bpm`, each up to `jitter` seconds early or late, and the
    /// tempo after each.
    fn tap_steady(
        t: &mut TapTempo,
        bpm: f64,
        jitter: f64,
        taps: usize,
        seed: u64,
    ) -> Vec<Option<f64>> {
        let mut rng = Rng(seed);
        (0..taps)
            .map(|i| t.tap(10.0 + i as f64 * 60.0 / bpm + jitter * rng.next()))
            .collect()
    }

    #[test]
    fn steady_taps_give_the_tempo_from_the_third() {
        let mut t = TapTempo::default();
        let got = tap_steady(&mut t, 124.0, 0.0, 5, 1);
        assert_eq!(got[..2], [None, None]);
        for bpm in got[2..].iter().flatten() {
            assert!((bpm - 124.0).abs() < 1e-9, "{bpm}");
        }
    }

    #[test]
    fn jittered_steady_taps_settle_and_stay_put() {
        // Hands land up to 20 ms either side of the beat. The tempo closes
        // in as taps come (within 1.5 BPM by the 8th tap, 1 by the 12th,
        // 0.65 from the 16th on) and then holds steady.
        for seed in 1..500 {
            let mut t = TapTempo::default();
            let got = tap_steady(&mut t, 128.0, 0.02, 32, seed);
            for (i, bpm) in got.iter().enumerate().skip(7) {
                let off = (bpm.unwrap() - 128.0).abs();
                let most = if i + 1 < 12 {
                    1.5
                } else if i + 1 < 16 {
                    1.0
                } else {
                    0.65
                };
                assert!(off < most, "seed {seed}, tap {}: {got:?}", i + 1);
                if i >= 16 {
                    let step = (bpm.unwrap() - got[i - 1].unwrap()).abs();
                    assert!(step < 0.4, "seed {seed}, tap {}: {got:?}", i + 1);
                }
            }
        }
    }

    #[test]
    fn a_pause_starts_over() {
        let mut t = TapTempo::default();
        tap_steady(&mut t, 120.0, 0.0, 4, 1);
        // 2.5 s later, a new tempo.
        let start = 10.0 + 1.5 + 2.5;
        assert_eq!(t.tap(start), None);
        assert_eq!(t.tap(start + 0.4), None);
        let bpm = t.tap(start + 0.8).unwrap();
        assert!((bpm - 150.0).abs() < 1e-9, "{bpm}");
    }

    #[test]
    fn a_slipped_tap_is_left_out() {
        let mut t = TapTempo::default();
        tap_steady(&mut t, 120.0, 0.0, 6, 1);
        // Beat 6 lands 150 ms late (30% of a beat), then back in time.
        assert_eq!(t.tap(10.0 + 3.0 + 0.15), Some(120.0));
        let bpm = t.tap(10.0 + 3.5).unwrap();
        assert!((bpm - 120.0).abs() < 1e-9, "{bpm}");
    }

    #[test]
    fn a_missed_beat_still_counts() {
        let mut t = TapTempo::default();
        tap_steady(&mut t, 120.0, 0.0, 4, 1);
        // Beat 4 not tapped; beat 5 is.
        let bpm = t.tap(10.0 + 2.5).unwrap();
        assert!((bpm - 120.0).abs() < 1e-9, "{bpm}");
    }

    #[test]
    fn two_taps_off_the_beat_are_a_new_tempo() {
        let mut t = TapTempo::default();
        tap_steady(&mut t, 120.0, 0.0, 5, 1);
        // Last tap at 12.0; now 100 BPM (0.6 s apart).
        assert_eq!(t.tap(12.6), Some(120.0));
        assert_eq!(t.tap(13.2), None);
        let bpm = t.tap(13.8).unwrap();
        assert!((bpm - 100.0).abs() < 1e-9, "{bpm}");
    }

    #[test]
    fn the_same_tap_twice_is_one() {
        let mut t = TapTempo::default();
        tap_steady(&mut t, 120.0, 0.0, 4, 1);
        assert_eq!(t.tap(11.5), None);
        let bpm = t.tap(12.0).unwrap();
        assert!((bpm - 120.0).abs() < 1e-9, "{bpm}");
    }
}
