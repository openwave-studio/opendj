//! Where the UI thread's time goes between frames, for the frame log.
//!
//! The waveforms and the jog marker only move smoothly when every frame is
//! drawn before the display's next refresh. Their own drawing takes about a
//! millisecond, so a late frame means something else held the UI thread:
//! the UI timer's work (polls) or Slint drawing the rest of the window. The
//! UI timer marks laps as it goes, the rendering notifier times Slint's
//! drawing, and [`take_report`] sums them up for the log line.

use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Load {
    /// When the UI timer's tick started, and its last lap.
    tick: Option<Instant>,
    lap: Option<Instant>,
    /// The longest tick, and its longest lap with the lap's name.
    tick_max: Duration,
    slowest: Option<(&'static str, Duration)>,
    /// Ticks over a frame (16 ms).
    long_ticks: u32,
    /// Slint drawing the window, from before to after rendering.
    render_from: Option<Instant>,
    render_max: Duration,
    render_total: Duration,
    renders: u32,
}

thread_local! {
    static LOAD: RefCell<Load> = RefCell::new(Load::default());
}

const FRAME: Duration = Duration::from_millis(16);

/// The UI timer's tick starts.
pub fn tick_start() {
    let now = Instant::now();
    LOAD.with_borrow_mut(|l| {
        l.tick = Some(now);
        l.lap = Some(now);
    });
}

/// The UI timer finished the work called `name` since the last lap.
pub fn lap(name: &'static str) {
    let now = Instant::now();
    LOAD.with_borrow_mut(|l| {
        if let Some(from) = l.lap.replace(now) {
            let took = now - from;
            if l.slowest.is_none_or(|(_, d)| took > d) {
                l.slowest = Some((name, took));
            }
        }
    });
}

/// The UI timer's tick ends.
pub fn tick_end() {
    lap("rest of tick");
    let now = Instant::now();
    LOAD.with_borrow_mut(|l| {
        if let Some(from) = l.tick.take() {
            let took = now - from;
            l.tick_max = l.tick_max.max(took);
            if took > FRAME {
                l.long_ticks += 1;
            }
        }
    });
}

/// Slint starts drawing a frame.
pub fn render_start() {
    LOAD.with_borrow_mut(|l| l.render_from = Some(Instant::now()));
}

/// Slint finished drawing a frame.
pub fn render_end() {
    let now = Instant::now();
    LOAD.with_borrow_mut(|l| {
        if let Some(from) = l.render_from.take() {
            let took = now - from;
            l.render_max = l.render_max.max(took);
            l.render_total += took;
            l.renders += 1;
        }
    });
}

/// What the UI thread did since the last report, for the frame log.
pub fn take_report() -> String {
    LOAD.with_borrow_mut(|l| {
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        let avg = if l.renders > 0 {
            ms(l.render_total) / l.renders as f64
        } else {
            0.0
        };
        let slowest = l
            .slowest
            .map_or("none".to_string(), |(n, d)| format!("{n} {:.1} ms", ms(d)));
        let report = format!(
            "window drawing {avg:.2} ms avg {:.1} ms max, UI timer {:.1} ms max ({} over 16 ms), slowest step {slowest}",
            ms(l.render_max),
            ms(l.tick_max),
            l.long_ticks,
        );
        let (tick, lap) = (l.tick, l.lap);
        *l = Load {
            tick,
            lap,
            ..Load::default()
        };
        report
    })
}
