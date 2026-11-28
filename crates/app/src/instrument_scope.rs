//! The instrument deck's scope: the sound as a wave (OSC) or as its
//! spectrum (FREQ), drawn as path commands for `InstrumentScope` in
//! deck.slint.
//!
//! The wave starts each frame where it rises through zero, so a steady
//! note holds still, and is scaled to its recent loudest so a quiet patch
//! still fills the box. The spectrum rises at once and falls slowly, so it
//! doesn't flicker, and is drawn in one of three styles picked from the
//! scope's right-click menu: a smooth curve, segmented bars or a curve
//! mirrored about the middle. The style and the scope's Hz labels and grid
//! lines are kept in the settings.

use opendj_engine::{InstrumentView, SCOPE_SAMPLES};

use slint::ComponentHandle;

use crate::{settings, AppWindow, InstrumentsUi};

/// The box the path commands are drawn in: the scope's own shape (320 by
/// 60 in the deck), so lines keep their width along and across.
pub const VIEW_W: f32 = 160.0;
pub const VIEW_H: f32 = 30.0;

/// Points along the spectrum, low to high.
const POINTS: usize = 160;
/// Bars in the Bars style, each the loudest of its points.
const BARS: usize = 40;
/// The spectrum's floor: quieter shows as nothing.
const FLOOR: f32 = -72.0;
/// How fast the spectrum falls, in dB a frame (a frame is 40 ms): about
/// 45 dB a second.
const FALL: f32 = 1.8;
/// How fast a bar's cap falls once it has waited [`CAP_HOLD`] frames.
const CAP_FALL: f32 = 0.56;
const CAP_HOLD: u8 = 15;
/// Room left above the spectrum's top and the wave's peaks.
const HEADROOM: f32 = 1.5;
/// Samples back from the newest that the wave may start at, looking for
/// a rise through zero: a little over a 30 Hz cycle at 48 kHz.
const TRIGGER_REACH: usize = 1600;
/// Each frame the wave's scale lets go of an old peak by this much.
const GAIN_RELEASE: f32 = 0.97;
/// The wave is never scaled up more than this (quieter is near silence).
const QUIETEST: f32 = 0.02;

pub const STYLE_KEY: &str = "instrument_scope_style";
pub const LABELS_KEY: &str = "instrument_scope_labels";
pub const GRID_KEY: &str = "instrument_scope_grid";

/// How the spectrum is drawn, in the menu's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Smooth,
    Bars,
    Mirror,
}

impl Style {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Style::Bars,
            2 => Style::Mirror,
            _ => Style::Smooth,
        }
    }
}

/// What the scope draws this frame: the area to fill and the line along
/// its edge (for bars, their caps), both as path commands in a
/// [`VIEW_W`] by [`VIEW_H`] box.
#[derive(Default, PartialEq)]
pub struct Frame {
    pub fill: String,
    pub line: String,
}

/// The scope's moving state between frames.
pub struct Scope {
    levels: Vec<f32>,
    caps: Vec<f32>,
    cap_wait: Vec<u8>,
    bars: Vec<f32>,
    peak: f32,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            levels: vec![FLOOR; POINTS],
            caps: vec![FLOOR; BARS],
            cap_wait: vec![0; BARS],
            bars: vec![FLOOR; BARS],
            peak: QUIETEST,
        }
    }
}

impl Scope {
    /// Whether the spectrum has fallen all the way: nothing left to draw
    /// while the sound stays silent.
    pub fn settled(&self) -> bool {
        self.levels.iter().chain(&self.caps).all(|&v| v <= FLOOR)
    }

    /// The spectrum, `db` from [`InstrumentView::spectrum`] with
    /// [`POINTS`] points, drawn in `style`.
    pub fn spectrum(&mut self, db: &[f32], style: Style) -> Frame {
        for (level, &v) in self.levels.iter_mut().zip(db) {
            let v = v.max(FLOOR);
            *level = if v > *level {
                v
            } else {
                (*level - FALL).max(v)
            };
        }
        let per = POINTS / BARS;
        for b in 0..BARS {
            let v = db[b * per..(b + 1) * per]
                .iter()
                .fold(FLOOR, |m, &x| m.max(x));
            let bar = &mut self.bars[b];
            *bar = if v > *bar { v } else { (*bar - FALL).max(v) };
            if *bar >= self.caps[b] {
                self.caps[b] = *bar;
                self.cap_wait[b] = CAP_HOLD;
            } else if self.cap_wait[b] > 0 {
                self.cap_wait[b] -= 1;
            } else {
                self.caps[b] = (self.caps[b] - CAP_FALL).max(FLOOR);
            }
        }
        match style {
            Style::Smooth => self.smooth(),
            Style::Bars => self.draw_bars(),
            Style::Mirror => self.mirror(),
        }
    }

    fn x(i: usize, count: usize) -> f32 {
        i as f32 * VIEW_W / (count.max(2) - 1) as f32
    }

    fn smooth(&self) -> Frame {
        let ys: Vec<f32> = self
            .levels
            .iter()
            .map(|&v| VIEW_H - (VIEW_H - HEADROOM) * share(v))
            .collect();
        let xs: Vec<f32> = (0..POINTS).map(|i| Self::x(i, POINTS)).collect();
        let mut line = String::new();
        curve(&mut line, &xs, &ys);
        let mut fill = line.clone();
        fill.push_str(&format!("L {VIEW_W} {VIEW_H} L 0 {VIEW_H} Z"));
        Frame { fill, line }
    }

    fn mirror(&self) -> Frame {
        let mid = VIEW_H / 2.0;
        let reach = mid - HEADROOM / 2.0;
        let xs: Vec<f32> = (0..POINTS).map(|i| Self::x(i, POINTS)).collect();
        let up: Vec<f32> = self
            .levels
            .iter()
            .map(|&v| mid - reach * share(v))
            .collect();
        let down: Vec<f32> = up.iter().map(|y| 2.0 * mid - y).collect();
        let mut line = String::new();
        curve(&mut line, &xs, &up);
        let mut fill = line.clone();
        let (rx, rd): (Vec<f32>, Vec<f32>) = (
            xs.iter().rev().copied().collect(),
            down.iter().rev().copied().collect(),
        );
        curve_on(&mut fill, &rx, &rd);
        fill.push('Z');
        curve(&mut line, &xs, &down);
        Frame { fill, line }
    }

    fn draw_bars(&self) -> Frame {
        use std::fmt::Write;
        let gap = 1.0;
        let width = (VIEW_W - gap * (BARS as f32 + 1.0)) / BARS as f32;
        let (seg, seg_gap) = (1.5, 0.5);
        let room = VIEW_H - HEADROOM - 1.0;
        let (mut fill, mut line) = (String::new(), String::new());
        for b in 0..BARS {
            let x = gap + b as f32 * (width + gap);
            let lit = (share(self.bars[b]) * room / (seg + seg_gap)).floor() as usize;
            for s in 0..lit {
                let bottom = VIEW_H - 0.5 - s as f32 * (seg + seg_gap);
                let _ = write!(
                    fill,
                    "M {x:.2} {bottom:.2} L {:.2} {bottom:.2} L {:.2} {:.2} L {x:.2} {:.2} Z ",
                    x + width,
                    x + width,
                    bottom - seg,
                    bottom - seg
                );
            }
            if self.caps[b] > FLOOR {
                let y = VIEW_H - 0.5 - share(self.caps[b]) * room - 0.75;
                let _ = write!(line, "M {x:.2} {y:.2} L {:.2} {y:.2} ", x + width);
            }
        }
        Frame { fill, line }
    }

    /// The wave, from [`InstrumentView::wave`]: [`SCOPE_SAMPLES`] samples
    /// as bins of their lowest and highest, starting where the wave rises
    /// through zero and scaled to its recent loudest.
    pub fn wave(&mut self, wave: &[f32]) -> Frame {
        const BINS: usize = 128;
        let n = wave.len();
        if n < SCOPE_SAMPLES + 1 {
            return Frame::default();
        }
        let latest = n - SCOPE_SAMPLES;
        let earliest = latest.saturating_sub(TRIGGER_REACH).max(1);
        let start = (earliest..=latest)
            .rev()
            .find(|&i| wave[i - 1] < 0.0 && wave[i] >= 0.0)
            .unwrap_or(latest);
        let per = SCOPE_SAMPLES / BINS;
        let bins: Vec<(f32, f32)> = (0..BINS)
            .map(|b| {
                let s = &wave[start + b * per..start + (b + 1) * per];
                s.iter()
                    .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)))
            })
            .collect();
        let loudest = bins.iter().fold(0.0f32, |m, &(lo, hi)| m.max(-lo).max(hi));
        self.peak = loudest.max(self.peak * GAIN_RELEASE).max(QUIETEST);
        let mid = VIEW_H / 2.0;
        let reach = (mid - HEADROOM) / self.peak;
        let y = |v: f32| mid - (v * reach).clamp(-mid, mid);
        let xs: Vec<f32> = (0..BINS).map(|i| Self::x(i, BINS)).collect();
        let hi: Vec<f32> = bins.iter().map(|&(_, h)| y(h)).collect();
        let lo: Vec<f32> = bins.iter().map(|&(l, _)| y(l)).collect();
        let mut line = String::new();
        curve(&mut line, &xs, &hi);
        let mut fill = line.clone();
        let (rx, rl): (Vec<f32>, Vec<f32>) = (
            xs.iter().rev().copied().collect(),
            lo.iter().rev().copied().collect(),
        );
        curve_on(&mut fill, &rx, &rl);
        fill.push('Z');
        curve(&mut line, &xs, &lo);
        Frame { fill, line }
    }
}

/// How much of the scope's height a level in dB takes, 0 to 1.
fn share(db: f32) -> f32 {
    ((db - FLOOR) / -FLOOR).clamp(0.0, 1.0)
}

/// A smooth curve through the points (Catmull-Rom, as cubic Béziers),
/// starting a new piece of path.
fn curve(out: &mut String, xs: &[f32], ys: &[f32]) {
    use std::fmt::Write;
    if xs.is_empty() {
        return;
    }
    let _ = write!(out, "M {:.2} {:.2} ", xs[0], ys[0]);
    segments(out, xs, ys);
}

/// As [`curve`], carrying on from where the path is (a line to its first
/// point).
fn curve_on(out: &mut String, xs: &[f32], ys: &[f32]) {
    use std::fmt::Write;
    if xs.is_empty() {
        return;
    }
    let _ = write!(out, "L {:.2} {:.2} ", xs[0], ys[0]);
    segments(out, xs, ys);
}

fn segments(out: &mut String, xs: &[f32], ys: &[f32]) {
    use std::fmt::Write;
    let n = xs.len();
    for i in 0..n.saturating_sub(1) {
        let p0 = i.saturating_sub(1);
        let p3 = (i + 2).min(n - 1);
        let _ = write!(
            out,
            "C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
            xs[i] + (xs[i + 1] - xs[p0]) / 6.0,
            ys[i] + (ys[i + 1] - ys[p0]) / 6.0,
            xs[i + 1] - (xs[p3] - xs[i]) / 6.0,
            ys[i + 1] - (ys[p3] - ys[i]) / 6.0,
            xs[i + 1],
            ys[i + 1]
        );
    }
}

/// Shows the saved look on the scope and saves it when the menu changes it.
pub fn install(app: &AppWindow) {
    let ui = app.global::<InstrumentsUi>();
    ui.set_scope_style(settings::number(STYLE_KEY, 0.0).clamp(0.0, 2.0) as i32);
    ui.set_scope_labels(settings::flag(LABELS_KEY, true));
    ui.set_scope_grid(settings::flag(GRID_KEY, false));
    let weak = app.as_weak();
    ui.on_scope_look_changed(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let ui = app.global::<InstrumentsUi>();
        settings::set(STYLE_KEY, serde_json::json!(ui.get_scope_style()));
        settings::set(LABELS_KEY, serde_json::json!(ui.get_scope_labels()));
        settings::set(GRID_KEY, serde_json::json!(ui.get_scope_grid()));
    });
}

/// The spectrum's [`POINTS`] from the view.
pub fn spectrum_db(view: &InstrumentView) -> Vec<f32> {
    view.spectrum(POINTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, level: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| level * (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn the_spectrum_falls_slowly() {
        let mut scope = Scope::default();
        let loud = vec![0.0; POINTS];
        let _ = scope.spectrum(&loud, Style::Smooth);
        let quiet = vec![-100.0; POINTS];
        let _ = scope.spectrum(&quiet, Style::Smooth);
        assert!((scope.levels[0] - (0.0 - FALL)).abs() < 1e-4);
        assert!(!scope.settled());
        for _ in 0..200 {
            let _ = scope.spectrum(&quiet, Style::Bars);
        }
        assert!(scope.settled());
    }

    #[test]
    fn a_steady_wave_holds_still() {
        let mut scope = Scope::default();
        let a = scope.wave(&sine(120.0, 0.3, 8192));
        // The same note, a cycle and a bit later (120 Hz is 400 samples).
        let later: Vec<f32> = sine(120.0, 0.3, 8192 + 530)[530..].to_vec();
        let b = scope.wave(&later);
        let numbers = |f: &Frame| -> Vec<f32> {
            f.line
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect()
        };
        let (a, b) = (numbers(&a), numbers(&b));
        assert_eq!(a.len(), b.len());
        let worst = a
            .iter()
            .zip(&b)
            .fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
        assert!(worst < 0.1, "moved by {worst}");
    }

    #[test]
    fn a_quiet_wave_fills_the_box() {
        let mut scope = Scope::default();
        let frame = scope.wave(&sine(220.0, 0.05, 8192));
        // The highest point sits near the top, not a twentieth of the way.
        let ys: Vec<f32> = frame
            .line
            .split_whitespace()
            .filter_map(|t| t.parse::<f32>().ok())
            .skip(1)
            .step_by(2)
            .collect();
        let top = ys.iter().fold(f32::MAX, |m, &y| m.min(y));
        assert!(top < HEADROOM + 1.0, "top at {top}");
    }

    #[test]
    fn every_style_draws_inside_the_box() {
        let db: Vec<f32> = (0..POINTS).map(|i| -10.0 - i as f32 * 0.3).collect();
        for style in [Style::Smooth, Style::Bars, Style::Mirror] {
            let mut scope = Scope::default();
            let f = scope.spectrum(&db, style);
            assert!(!f.fill.is_empty(), "{style:?}");
            for t in f.fill.split_whitespace().chain(f.line.split_whitespace()) {
                if let Ok(v) = t.parse::<f32>() {
                    assert!((-0.01..=VIEW_W + 0.01).contains(&v), "{style:?}: {v}");
                }
            }
        }
    }
}
