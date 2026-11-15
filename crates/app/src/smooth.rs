//! Steady waveform motion from play positions that arrive with timing noise.
//!
//! The engine's position is exact, but when the UI reads it the answer jumps
//! a little with audio-callback timing, a millisecond or so each way, which
//! shows as jitter when the waveform moves hundreds of pixels a second. This
//! advances the drawn position by the speed times the real time since the
//! last frame, and only eases it towards the engine's position. Jumps (seeks,
//! loops, cues) and speed changes (scratching, starting and stopping) are
//! followed at once.
//!
//! It used to step by a learnt frame interval instead, to hide timing noise
//! in when frames are drawn. But frames don't come at one steady rate: the
//! display's refresh changes (ProMotion), and other UI changes draw extra
//! frames in between. Each such frame stepped too far or not far enough,
//! the drift built up to tens of milliseconds and was then pulled or
//! snapped back: a sideways tug every few seconds.

use std::time::Instant;

/// Bigger differences from the engine are jumps, followed at once (seconds).
const JUMP: f64 = 0.04;
/// Share of the remaining difference corrected each frame.
const CORRECTION: f64 = 0.1;

#[derive(Default)]
pub struct SmoothClock {
    /// What was drawn last, the speed then, and when.
    last: Option<(f64, f64, Instant)>,
}

impl SmoothClock {
    /// The position to draw at `now`, given the engine's `position` and
    /// `speed` (track seconds per second) for that moment.
    pub fn next(&mut self, position: f64, speed: f64, now: Instant) -> f64 {
        let shown = match self.last {
            Some((shown, last_speed, at)) if speed != 0.0 && (speed - last_speed).abs() < 0.02 => {
                let dt = now.saturating_duration_since(at).as_secs_f64();
                let predicted = shown + speed * dt;
                let error = position - predicted;
                if error.abs() > JUMP {
                    position
                } else {
                    predicted + error * CORRECTION
                }
            }
            _ => position,
        };
        self.last = Some((shown, speed, now));
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Plays at 1x, drawing at `times` (seconds) while the engine's answer
    /// is off by up to 1 ms; returns the worst gap between a frame's step
    /// and the time since the last, and between the drawn position and the
    /// engine's, after the first 30 frames.
    fn play(times: &[f64]) -> (f64, f64) {
        let mut clock = SmoothClock::default();
        let start = Instant::now();
        let (mut previous, mut step_off, mut lag): (Option<(f64, f64)>, f64, f64) =
            (None, 0.0, 0.0);
        for (i, &t) in times.iter().enumerate() {
            let noise = [0.001, -0.0005, 0.0, -0.001, 0.0005][i % 5];
            let shown = clock.next(10.0 + t + noise, 1.0, start + Duration::from_secs_f64(t));
            if let (true, Some((p, pt))) = (i > 30, previous) {
                step_off = step_off.max((shown - p - (t - pt)).abs());
                lag = lag.max((shown - 10.0 - t).abs());
            }
            previous = Some((shown, t));
        }
        (step_off, lag)
    }

    #[test]
    fn evens_out_noisy_positions() {
        let times: Vec<f64> = (0..600).map(|i| 0.01 + i as f64 / 120.0).collect();
        let (step_off, lag) = play(&times);
        assert!(step_off < 0.0003, "steps off by {step_off}");
        assert!(lag < 0.001, "lags by {lag}");
    }

    #[test]
    fn stays_with_the_engine_when_frames_come_unevenly() {
        // 120 Hz with a skipped frame now and then, frames drawn up to 2 ms
        // early or late, and an extra frame drawn in between now and then.
        // The clock that stepped by a learnt frame interval drifted off the
        // engine here and was pulled back: a sideways tug.
        let mut seed = 7u64;
        let mut random = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut times = vec![];
        let mut vsync = 0.0;
        for _ in 0..1200 {
            vsync += if random() < 0.1 {
                1.0 / 60.0
            } else {
                1.0 / 120.0
            };
            times.push(vsync + (random() - 0.5) * 0.004);
            if random() < 0.25 {
                times.push(vsync + 0.002 + random() * 0.004);
            }
        }
        times.sort_by(f64::total_cmp);
        let (step_off, lag) = play(&times);
        assert!(step_off < 0.0003, "steps off by {step_off}");
        assert!(lag < 0.001, "lags by {lag}");
    }

    #[test]
    fn follows_jumps_and_stops_at_once() {
        let mut clock = SmoothClock::default();
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        clock.next(5.0, 1.0, ms(0));
        clock.next(5.008, 1.0, ms(8));
        // A seek.
        assert_eq!(clock.next(42.0, 1.0, ms(16)), 42.0);
        // Stopped.
        assert_eq!(clock.next(42.5, 0.0, ms(24)), 42.5);
        // Scratching: the speed changes every frame.
        assert_eq!(clock.next(42.4, -1.3, ms(32)), 42.4);
        assert_eq!(clock.next(42.39, -0.4, ms(40)), 42.39);
    }
}
