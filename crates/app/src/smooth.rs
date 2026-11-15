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

use std::time::{Duration, Instant};

/// Bigger differences from the engine are jumps, followed at once (seconds).
const JUMP: f64 = 0.04;
/// Share of the remaining difference corrected each frame.
const CORRECTION: f64 = 0.1;

#[derive(Default)]
pub struct SmoothClock {
    /// What was drawn last, the speed then, and when.
    last: Option<(f64, f64, Instant)>,
    /// Since the last report: jumps to the engine's position while playing
    /// steadily, and the biggest difference eased away (seconds).
    snaps: u32,
    worst: f64,
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
                    self.snaps += 1;
                    position
                } else {
                    self.worst = self.worst.max(error.abs());
                    predicted + error * CORRECTION
                }
            }
            _ => position,
        };
        self.last = Some((shown, speed, now));
        shown
    }

    /// Jumps and the biggest difference eased away since the last call.
    fn take_report(&mut self) -> (u32, f64) {
        let r = (self.snaps, self.worst);
        (self.snaps, self.worst) = (0, 0.0);
        r
    }
}

/// When frames are shown, steadier than when they are drawn.
///
/// A frame is drawn a little early or late (later still when two decks
/// scroll or the machine is busy) but shown at the display's next refresh.
/// Positions are worked out for the time this gives: it advances by whole
/// frame intervals and eases 15% of the way towards the real time each
/// frame, so it never drifts off. Frames too far from that pattern (extra
/// frames drawn in between, a refresh-rate change) take the real time.
#[derive(Default)]
pub struct FrameTimer {
    /// The last frame's time.
    last: Option<Instant>,
    /// The frame interval in seconds, once known.
    frame: Option<f64>,
    /// The first frames' intervals, which start the estimate.
    first: Vec<f64>,
    /// Recent frames off the pattern.
    missed: u32,
    /// Frames off the pattern since the last report.
    off_pattern: u32,
    /// The display's refresh interval in seconds, when the system says,
    /// and when it was last asked.
    refresh: Option<f64>,
    refresh_asked: Option<Instant>,
}

/// Frames counted before the interval is known.
const FIRST_FRAMES: usize = 24;

impl FrameTimer {
    /// The refresh rate of the display the window is on, in Hz, if the
    /// system says: reported in the log next to the frame interval, since a
    /// window on a 60 Hz monitor can be asked to draw at the built-in
    /// screen's 120 Hz, which would show as uneven motion.
    pub fn wants_refresh_rate(&mut self, now: Instant) -> bool {
        let due = self
            .refresh_asked
            .is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_secs(2));
        if due {
            self.refresh_asked = Some(now);
        }
        due
    }

    /// Sets the display's refresh rate, as `wants_refresh_rate` asks.
    pub fn set_refresh_rate(&mut self, hz: Option<f64>) {
        self.refresh = hz.filter(|&h| h > 0.0).map(|h| 1.0 / h);
    }

    /// The time to draw a frame drawn at `now` for.
    pub fn next(&mut self, now: Instant) -> Instant {
        let at = match (self.last, self.frame) {
            (Some(last), Some(f)) => {
                let dt = signed_secs(now, last);
                let n = (dt / f).round().max(1.0);
                let error = dt - n * f;
                if error.abs() > 0.35 * f {
                    // Off the pattern. Many in a row: the refresh rate changed,
                    // so learn the interval again.
                    self.missed += 1;
                    self.off_pattern += 1;
                    if self.missed >= FIRST_FRAMES as u32 {
                        (self.frame, self.missed) = (None, 0);
                        self.first.clear();
                    }
                    now
                } else {
                    self.missed = self.missed.saturating_sub(1);
                    self.frame = Some(f + 0.01 * error / n);
                    shift(last, n * f + 0.15 * error)
                }
            }
            (Some(last), None) => {
                let dt = signed_secs(now, last);
                if dt > 0.002 && dt < 0.1 {
                    self.first.push(dt);
                }
                if self.first.len() >= FIRST_FRAMES {
                    // Their whole span over how many intervals it holds: one
                    // frame drawn late makes the next early, so their errors
                    // cancel. A skipped frame counts as more, in medians.
                    let mut sorted = self.first.clone();
                    sorted.sort_by(f64::total_cmp);
                    let median = sorted[sorted.len() / 2];
                    let span: f64 = self.first.iter().sum();
                    let count: f64 = self
                        .first
                        .iter()
                        .map(|d| (d / median).round().max(1.0))
                        .sum();
                    self.frame = Some(span / count);
                }
                now
            }
            _ => now,
        };
        self.last = Some(at);
        at
    }
}

/// How smoothly the waveforms are drawn, written to the log every
/// `REPORT_EVERY` while a deck plays: what's needed to tell a slow or
/// uneven frame from a clock that drifts, on a machine we can't watch.
#[derive(Default)]
pub struct FrameStats {
    since: Option<Instant>,
    last: Option<Instant>,
    frames: u32,
    /// Frames more than one and a half intervals after the last.
    late: u32,
    longest: f64,
    draw_total: f64,
    draw_longest: f64,
}

const REPORT_EVERY: Duration = Duration::from_secs(10);

impl FrameStats {
    /// A frame started drawing at `start` and took `took`, while `moving`.
    pub fn frame(
        &mut self,
        start: Instant,
        took: Duration,
        moving: bool,
        scale: f32,
        timer: &mut FrameTimer,
        clocks: &mut [SmoothClock],
    ) {
        if !moving {
            *self = Self::default();
            return;
        }
        let since = *self.since.get_or_insert(start);
        if let Some(last) = self.last {
            let dt = signed_secs(start, last);
            self.longest = self.longest.max(dt);
            if timer.frame.is_some_and(|f| dt > 1.5 * f) {
                self.late += 1;
            }
        }
        self.last = Some(start);
        self.frames += 1;
        let took = took.as_secs_f64();
        self.draw_total += took;
        self.draw_longest = self.draw_longest.max(took);
        let elapsed = start.saturating_duration_since(since);
        if elapsed < REPORT_EVERY {
            return;
        }
        let (mut snaps, mut worst) = (0, 0.0f64);
        for c in clocks.iter_mut() {
            let (s, w) = c.take_report();
            snaps += s;
            worst = worst.max(w);
        }
        eprintln!(
            "waveform frames at {scale}x: {} in {:.1} s, interval {:.2} ms, late {} (longest {:.1} ms), \
             off pattern {}, display {}, drawing {:.2} ms avg {:.1} ms max, position jumps {}, eased up to {:.1} ms",
            self.frames,
            elapsed.as_secs_f64(),
            timer.frame.unwrap_or(0.0) * 1e3,
            self.late,
            self.longest * 1e3,
            timer.off_pattern,
            timer
                .refresh
                .map_or("unknown".to_string(), |r| format!("{:.0} Hz", 1.0 / r)),
            self.draw_total / self.frames as f64 * 1e3,
            self.draw_longest * 1e3,
            snaps,
            worst * 1e3,
        );
        timer.off_pattern = 0;
        *self = Self {
            since: Some(start),
            last: Some(start),
            ..Self::default()
        };
    }
}

/// `a - b` in seconds, negative if `a` is earlier.
fn signed_secs(a: Instant, b: Instant) -> f64 {
    match a.checked_duration_since(b) {
        Some(d) => d.as_secs_f64(),
        None => -b.duration_since(a).as_secs_f64(),
    }
}

/// `t` moved by `secs`, either way.
fn shift(t: Instant, secs: f64) -> Instant {
    if secs >= 0.0 {
        t + Duration::from_secs_f64(secs)
    } else {
        t - Duration::from_secs_f64(-secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Draws at `times` (seconds), shown at refreshes `vsyncs` apart;
    /// returns how far the gap between the timer's time and the refresh
    /// each frame is shown at varies, after the first 30 frames.
    fn shown_spread(times: &[f64], vsync: f64) -> f64 {
        let mut timer = FrameTimer::default();
        let start = Instant::now();
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for (i, &t) in times.iter().enumerate() {
            let at = timer.next(start + Duration::from_secs_f64(t));
            let gap = signed_secs(at, start) - (t / vsync).ceil() * vsync;
            if i > 30 {
                (lo, hi) = (lo.min(gap), hi.max(gap));
            }
        }
        hi - lo
    }

    #[test]
    fn frame_times_follow_the_refresh_not_the_drawing() {
        // 120 Hz, each frame drawn 1-5 ms before the refresh it shows at.
        let vsync = 1.0 / 120.0;
        let early = [0.001, 0.005, 0.002, 0.004, 0.003, 0.001, 0.005];
        let times: Vec<f64> = (1..1200)
            .map(|i| i as f64 * vsync - early[i % early.len()])
            .collect();
        // The drawing times wander 4 ms against the refresh; the timer's
        // much less.
        let spread = shown_spread(&times, vsync);
        assert!(spread < 0.001, "{spread}");
        let mut timer = FrameTimer::default();
        let start = Instant::now();
        let ats: Vec<f64> = times
            .iter()
            .map(|&t| signed_secs(timer.next(start + Duration::from_secs_f64(t)), start))
            .collect();
        for (i, w) in ats.windows(2).enumerate().skip(30) {
            let step = w[1] - w[0];
            assert!((step - vsync).abs() < 0.0005, "frame {i}: step {step}");
        }
    }

    #[test]
    fn frame_times_never_drift_off() {
        // A slow first frame, refresh switching between 120 and 60 Hz,
        // frames drawn up to 2 ms early or late and extra frames between.
        let mut seed = 3u64;
        let mut random = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut timer = FrameTimer::default();
        let start = Instant::now();
        let mut vsync = 0.05;
        for i in 0..3000 {
            vsync += if (i / 200) % 2 == 0 {
                1.0 / 120.0
            } else {
                1.0 / 60.0
            };
            let mut times = vec![vsync + (random() - 0.5) * 0.004];
            if random() < 0.2 {
                times.push(vsync + 0.003 + random() * 0.003);
            }
            for t in times {
                let at = signed_secs(timer.next(start + Duration::from_secs_f64(t)), start);
                assert!(
                    (at - t).abs() < 0.004,
                    "frame {i}: {} ms off",
                    (at - t) * 1e3
                );
            }
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
}
