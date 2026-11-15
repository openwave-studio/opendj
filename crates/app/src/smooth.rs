//! Steady waveform motion from play positions that arrive with timing noise.
//!
//! The engine's position is exact, but when the UI reads it the answer jumps
//! a little with audio-callback and frame timing, a millisecond or two each
//! way, which shows as jitter when the waveform moves hundreds of pixels a
//! second. Like a video player's clock, this advances the drawn position by
//! the speed times a steady frame interval, and only eases it towards the
//! engine's position. Jumps (seeks, loops, cues) and speed changes (scratching,
//! starting and stopping) are followed at once.

use std::time::Instant;

/// Bigger differences from the engine are jumps, followed at once (seconds).
const JUMP: f64 = 0.04;
/// Share of the remaining difference corrected each frame.
const CORRECTION: f64 = 0.1;
/// Frames measured before the frame interval is trusted.
const SETTLE: u32 = 8;

#[derive(Default)]
pub struct SmoothClock {
    /// What was drawn last, the speed then, and when.
    last: Option<(f64, f64, Instant)>,
    /// Running estimate of the time between frames, in seconds, and how
    /// many frames it has been measured over.
    frame: Option<f64>,
    measured: u32,
}

impl SmoothClock {
    /// The position to draw at `now`, given the engine's `position` and
    /// `speed` (track seconds per second) for that moment.
    pub fn next(&mut self, position: f64, speed: f64, now: Instant) -> f64 {
        let shown = match self.last {
            Some((shown, last_speed, at)) if speed != 0.0 && (speed - last_speed).abs() < 0.02 => {
                let dt = now.saturating_duration_since(at).as_secs_f64();
                let predicted = shown + speed * self.step(dt);
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

    /// How far to advance for a frame `dt` seconds after the last. Frames
    /// come at the display's rate, but timing a single frame adds noise, so
    /// the clock steps by whole frame intervals (more than one if frames
    /// were dropped), once it knows the interval.
    ///
    /// The interval is learnt from every frame, as its time divided by the
    /// whole intervals it spans: quickly over the first frames, then
    /// slowly. (It used to start from the first frame's time and learn at
    /// 5% a frame from frames clamped to half to one and a half of it: a
    /// slow first frame after play started, or a display switching between
    /// 120 and 60 Hz as ProMotion does, left it wrong for seconds. Each
    /// step then ran ahead or behind until the drift was pulled back or,
    /// past 40 ms, snapped to the engine: a sideways tug every few seconds.)
    fn step(&mut self, dt: f64) -> f64 {
        let Some(f) = self.frame else {
            if dt > 0.0 && dt < 0.1 {
                self.frame = Some(dt);
                self.measured = 1;
            }
            return dt;
        };
        let n = (dt / f).round().max(1.0);
        let one = dt / n;
        if (one - f).abs() < 0.5 * f {
            let rate = if self.measured < SETTLE { 0.3 } else { 0.05 };
            self.frame = Some(f + (one - f) * rate);
            self.measured = self.measured.saturating_add(1);
        }
        if self.measured < SETTLE {
            dt
        } else {
            n * f
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn evens_out_noisy_timing() {
        let mut clock = SmoothClock::default();
        let start = Instant::now();
        let frame = 1.0 / 120.0;
        let mut previous: Option<f64> = None;
        for i in 0..600 {
            // The display shows frame i at a steady `vsync`, but the UI draws
            // it up to 2 ms early or late, and the engine's answer is off by up
            // to 1.5 ms: what an audio callback and a busy UI thread give.
            let vsync = 0.01 + i as f64 * frame;
            let wobble = [0.0, 0.002, -0.0015, 0.001, -0.002][i % 5];
            let now = start + Duration::from_secs_f64(vsync + wobble);
            let noise = [0.0015, -0.001, 0.0005, -0.0015][i % 4];
            let reported = 10.0 + vsync + wobble + noise;
            let shown = clock.next(reported, 1.0, now);
            if i > 120 {
                // Each frame moves almost exactly one frame's worth...
                let step = shown - previous.unwrap();
                assert!((step - frame).abs() < 0.0005, "frame {i}: step {step}");
                // ...and stays with the engine.
                let truth = 10.0 + vsync;
                assert!(
                    (shown - truth).abs() < 0.004,
                    "frame {i}: off by {}",
                    shown - truth
                );
            }
            previous = Some(shown);
        }
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
    /// Plays at 1x through frames `dts` seconds apart; returns the worst
    /// gap between a frame's step and its interval, and between the drawn
    /// position and the engine's, after the first 30 frames.
    fn play(dts: &[f64]) -> (f64, f64) {
        let mut clock = SmoothClock::default();
        let start = Instant::now();
        let (mut t, mut previous): (f64, Option<f64>) = (0.0, None);
        let (mut step_off, mut lag): (f64, f64) = (0.0, 0.0);
        for (i, dt) in dts.iter().enumerate() {
            t += dt;
            let shown = clock.next(10.0 + t, 1.0, start + Duration::from_secs_f64(t));
            if i > 30 {
                step_off = step_off.max((shown - previous.unwrap() - dt).abs());
                lag = lag.max((shown - 10.0 - t).abs());
            }
            previous = Some(shown);
        }
        (step_off, lag)
    }

    #[test]
    fn steady_from_the_start_after_a_slow_first_frame() {
        // Play starts with two slow frames, then 120 Hz: the clock used to
        // take the first as the frame interval and drift ~30 ms off the
        // engine before settling, a tug just after play started.
        let mut dts = vec![0.05, 0.03];
        dts.extend([1.0 / 120.0; 600]);
        let (step_off, lag) = play(&dts);
        assert!(step_off < 0.0005, "steps off by {step_off}");
        assert!(lag < 0.002, "lags by {lag}");
    }

    #[test]
    fn steady_when_the_display_switches_between_120_and_60_hz() {
        // ProMotion: some frames come after one interval, some after two.
        let f = 1.0 / 120.0;
        let dts: Vec<f64> = (0..1200)
            .map(|i| if (i * 7) % 3 == 0 { 2.0 * f } else { f })
            .collect();
        let (step_off, lag) = play(&dts);
        assert!(step_off < 0.0005, "steps off by {step_off}");
        assert!(lag < 0.002, "lags by {lag}");
    }
}
