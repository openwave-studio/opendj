//! Turns detected beats into a beat grid, steady unless the track really
//! drifts.
//!
//! Beat detectors place each beat on its own, so even a track made on a
//! computer comes back with beats a few milliseconds early or late. Taking
//! the tempo from neighbouring beats turns that jitter into small tempo
//! changes that aren't there. Instead the beats are numbered and a straight
//! line (one tempo) is fitted through them; only where the beats move away
//! from that line for a while, as live drums and old disco do, does the grid
//! start a new tempo section.
//!
//! That is the [`GridMode::Auto`] fit. A track can also be fitted with one
//! tempo throughout ([`GridMode::Fixed`]), or bar by bar so the grid follows
//! a drummer's pushes and pulls ([`GridMode::Flexible`]).

use std::sync::atomic::{AtomicU8, Ordering};

use crate::beats::{BeatGrid, GridMarker};

/// How far (in seconds) the beats may sit from the grid, going by the
/// median of a few in a row, before a new tempo section starts.
const TOLERANCE: f64 = 0.03;
/// Beats in a row whose median distance is checked against [`TOLERANCE`].
const RUN: usize = 8;
/// Fewest beats in a tempo section.
const MIN_SECTION: usize = 32;
/// Fewest beats for a grid at all.
const MIN_BEATS: usize = 16;
/// A grid whose beats sit further than this from it (root mean square, in
/// seconds) is marked unsure.
const UNSURE_RMS: f64 = 0.04;
/// A grid is marked unsure when fewer than this share of the downbeats fall
/// on its bar lines.
const UNSURE_DOWNBEATS: f64 = 0.6;
/// How far (in BPM) a fitted tempo may be from a whole or half BPM to be
/// tried at it.
const ROUND_RANGE: f64 = 0.1;
/// How much further (in seconds, the median distance) the beats may sit
/// from the grid at the round tempo than from the fitted one for the round
/// tempo to be taken.
const ROUND_SLACK: f64 = 0.003;
/// Two neighbouring sections closer than this in tempo (as a fraction) are
/// one section.
const SAME_TEMPO: f64 = 0.002;

/// How far (in seconds, the median of a bar's beats) a bar may sit from
/// its section's line in a Flexible grid before a new section starts.
const FLEX_TOLERANCE: f64 = 0.015;
/// Fewest bars in a Flexible grid's section.
const FLEX_MIN_BARS: usize = 2;

/// How a track's grid follows its tempo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GridMode {
    /// One tempo unless the beats drift away from it for a while.
    #[default]
    Auto,
    /// One tempo for the whole track.
    Fixed,
    /// Follows the beats bar by bar, for live drums.
    Flexible,
}

impl GridMode {
    pub const ALL: [GridMode; 3] = [GridMode::Fixed, GridMode::Auto, GridMode::Flexible];

    /// The mode's number, as settings, controls and the cache keep it.
    pub fn id(self) -> u8 {
        match self {
            GridMode::Auto => 0,
            GridMode::Fixed => 1,
            GridMode::Flexible => 2,
        }
    }

    pub fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(GridMode::Auto),
            1 => Some(GridMode::Fixed),
            2 => Some(GridMode::Flexible),
            _ => None,
        }
    }

    /// The mode's name, as the library keeps it and searches name it.
    pub fn name(self) -> &'static str {
        match self {
            GridMode::Auto => "auto",
            GridMode::Fixed => "fixed",
            GridMode::Flexible => "flexible",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))
    }
}

static DEFAULT_MODE: AtomicU8 = AtomicU8::new(0);

/// The mode tracks without one of their own are fitted in.
pub fn default_mode() -> GridMode {
    GridMode::from_id(DEFAULT_MODE.load(Ordering::Relaxed)).unwrap_or_default()
}

/// Sets the mode tracks without one of their own are fitted in.
pub fn set_default_mode(mode: GridMode) {
    DEFAULT_MODE.store(mode.id(), Ordering::Relaxed);
}

/// A grid fitted to detected beats.
#[derive(Clone, Debug, PartialEq)]
pub struct Fitted {
    pub grid: BeatGrid,
    /// The beats or downbeats fit the grid poorly: worth a look.
    pub unsure: bool,
}

/// Fits a grid to `beats` (times in seconds, ascending; missed beats are
/// fine) whose tempo is near `bpm`. With `downbeats`, the grid's beat 0, and
/// so bar 1, falls on a downbeat. None with too few beats. The fit is
/// [`GridMode::Auto`]'s; see [`fit_mode`] for the others.
pub fn fit(beats: &[f64], downbeats: Option<&[f64]>, bpm: f64) -> Option<Fitted> {
    fit_mode(beats, downbeats, bpm, GridMode::Auto)
}

/// Fits a grid as [`fit`] does, in `mode`.
pub fn fit_mode(
    beats: &[f64],
    downbeats: Option<&[f64]>,
    bpm: f64,
    mode: GridMode,
) -> Option<Fitted> {
    if bpm <= 0.0 {
        return None;
    }
    let mut numbered = number(beats, 60.0 / bpm);
    if numbered.len() < MIN_BEATS {
        return None;
    }
    // Found in every mode: it numbers again the beats after a slip.
    let found = sections(&mut numbered);

    // Bar 1 on the downbeats: the beat number most of them land on, mod 4.
    let first = numbered[0].0;
    let (bar, downbeat_share) = match downbeats {
        Some(d) if !d.is_empty() => {
            let mut votes = [0usize; 4];
            for &t in d {
                if let Some(n) = nearest_number(&numbered, t, 60.0 / bpm) {
                    votes[n.rem_euclid(4.0) as usize] += 1;
                }
            }
            let best = (0..4).max_by_key(|&k| votes[k]).unwrap();
            (best as f64, votes[best] as f64 / d.len() as f64)
        }
        _ => (first.rem_euclid(4.0), 1.0),
    };
    // Start the grid on the last bar line at or before the first beat.
    let start = first - (first - bar).rem_euclid(4.0);

    let grid_markers = match mode {
        GridMode::Flexible => flexible_markers(&numbered, bar, start),
        GridMode::Fixed => line_markers(&numbered, &[(0, numbered.len())], start),
        GridMode::Auto => {
            let period = |(a, b): (usize, usize)| Line::through(&numbered[a..b]).period;
            // Neighbours at nearly the same tempo are one section.
            let mut merged: Vec<(usize, usize)> = Vec::new();
            for s in found {
                match merged.last_mut() {
                    Some(last) if (period(s) / period(*last) - 1.0).abs() < SAME_TEMPO => {
                        last.1 = s.1
                    }
                    _ => merged.push(s),
                }
            }
            line_markers(&numbered, &merged, start)
        }
    };
    let grid = BeatGrid::new(grid_markers);

    let rms = (numbered
        .iter()
        .map(|&(_, t)| (t - grid.snap(t, false)).powi(2))
        .sum::<f64>()
        / numbered.len() as f64)
        .sqrt();
    Some(Fitted {
        grid,
        unsure: rms > UNSURE_RMS || downbeat_share < UNSURE_DOWNBEATS,
    })
}

/// The share of `beats` that fall on `grid`, within the tolerance a tempo
/// section allows. It doesn't depend on counting the beats, so a stretch
/// where a tracker followed the off-beats can't throw it.
pub fn share_on(beats: &[f64], grid: &BeatGrid) -> f64 {
    let on = beats
        .iter()
        .filter(|&&t| (t - grid.snap(t, false)).abs() < TOLERANCE)
        .count();
    on as f64 / beats.len().max(1) as f64
}

/// Numbers the beats (beat number, seconds), counting the beats missed in a
/// gap. Each beat is numbered on the grid of the few before it, so a beat
/// that lands on a number already taken (a stray onset, or a run of them
/// the tracker followed) is dropped, whichever of the two is further off.
fn number(beats: &[f64], period: f64) -> Vec<(f64, f64)> {
    /// Beats before each one that give the grid it is numbered on.
    const LOCAL: usize = 16;
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(beats.len());
    for &t in beats {
        let Some(&(last_n, last_t)) = out.last() else {
            out.push((0.0, t));
            continue;
        };
        let recent = &out[out.len().saturating_sub(LOCAL)..];
        let line = if recent.len() >= LOCAL / 2 {
            Line::through(recent)
        } else {
            let offset =
                recent.iter().map(|&(n, b)| b - n * period).sum::<f64>() / recent.len() as f64;
            Line { offset, period }
        };
        let n = ((t - line.offset) / line.period).round();
        if n > last_n {
            out.push((n, t));
        } else if n == last_n && (t - line.at(n)).abs() < (last_t - line.at(n)).abs() {
            *out.last_mut().unwrap() = (n, t);
        }
    }
    out
}

/// The number of the beat nearest `t`, if one is within half a beat.
fn nearest_number(numbered: &[(f64, f64)], t: f64, period: f64) -> Option<f64> {
    let i = numbered.partition_point(|&(_, b)| b < t);
    let near = [i.checked_sub(1), Some(i)]
        .into_iter()
        .flatten()
        .filter_map(|k| numbered.get(k))
        .min_by(|a, b| (a.1 - t).abs().total_cmp(&(b.1 - t).abs()))?;
    ((near.1 - t).abs() < period / 2.0).then_some(near.0)
}

/// Splits the numbered beats into runs that each keep one tempo, as
/// (first, end) index ranges. Where the tracker slipped a beat (counted one
/// too many or too few through a fill), the beats after it are numbered
/// again instead.
fn sections(numbered: &mut [(f64, f64)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < numbered.len() {
        if i - start < MIN_SECTION || numbered.len() - i < MIN_SECTION {
            i += 1;
            continue;
        }
        let line = Line::through(&numbered[start..i]);
        // Do the beats ahead keep away from this section's line, shifted
        // by `slip` beats?
        let away = |beats: &[(f64, f64)], slip: f64| {
            let mut off: Vec<f64> = beats
                .iter()
                .map(|&(n, t)| (t - line.at(n + slip)).abs())
                .collect();
            off.sort_by(f64::total_cmp);
            off[off.len() / 2] > TOLERANCE
        };
        // The next few beats, and the ones after them too? A fill, or a run
        // the tracker took on the off-beats, comes back to the line; a
        // change of tempo doesn't.
        let ahead = &numbered[i..i + MIN_SECTION];
        let later = &numbered[i + MIN_SECTION..(i + 2 * MIN_SECTION).min(numbered.len())];
        if !(away(&numbered[i..i + RUN], 0.0)
            && away(ahead, 0.0)
            && (later.len() < RUN || away(later, 0.0)))
        {
            i += 1;
            continue;
        }
        if let Some(slip) = [-1.0, 1.0, -2.0, 2.0]
            .into_iter()
            .find(|&slip| !away(ahead, slip))
        {
            for b in &mut numbered[i..] {
                b.0 += slip;
            }
            i += 1;
            continue;
        }
        // Start the new section where two lines fit best: the beats may
        // have begun to leave a little before or after.
        let end = i + RUN;
        let cut = (i.saturating_sub(2 * RUN).max(start + MIN_SECTION)..end - 1)
            .min_by(|&a, &b| {
                let err = |c: usize| {
                    squared_error(&numbered[start..c]) + squared_error(&numbered[c..end])
                };
                err(a).total_cmp(&err(b))
            })
            .unwrap_or(i);
        out.push((start, cut));
        start = cut;
        i = cut;
    }
    out.push((start, numbered.len()));
    out
}

/// A marker where each section starts, on the line through its beats; the
/// first starts at beat number `start` (bar 1's line).
fn line_markers(
    numbered: &[(f64, f64)],
    sections: &[(usize, usize)],
    start: f64,
) -> Vec<GridMarker> {
    let mut markers: Vec<GridMarker> = sections
        .iter()
        .map(|&(a, b)| {
            let beats = &numbered[a..b];
            let line = Line::through(beats).rounded(beats);
            GridMarker {
                seconds: line.at(numbered[a].0),
                bpm: 60.0 / line.period,
            }
        })
        .collect();
    let (a, b) = sections[0];
    markers[0].seconds = Line::through(&numbered[a..b])
        .rounded(&numbered[a..b])
        .at(start);
    markers
}

/// A Flexible grid's markers: on bar lines (beat numbers `bar` mod 4, from
/// `start`), each where the beats around it put that bar line, with
/// straight lines between. A stretch grows a bar at a time while the bars
/// in it stay within [`FLEX_TOLERANCE`] of the line from its first bar line
/// to the one after its last bar; two bars in a row off it end the stretch
/// (one bar off on its own, a fill or a stray beat, doesn't).
fn flexible_markers(numbered: &[(f64, f64)], bar: f64, start: f64) -> Vec<GridMarker> {
    // The index of each bar's first beat, and the bar's number.
    let mut bars: Vec<usize> = Vec::new();
    let mut ids: Vec<f64> = Vec::new();
    for (i, &(n, _)) in numbered.iter().enumerate() {
        let id = ((n - bar) / 4.0).floor();
        if ids.last() != Some(&id) {
            bars.push(i);
            ids.push(id);
        }
    }
    bars.push(numbered.len());
    let count = ids.len();
    // Where the beats around bar line `k` (k == count: after the last bar)
    // put it, as (beat number, seconds).
    let knot = |k: usize| -> (f64, f64) {
        let n = if k < count {
            bar + 4.0 * ids[k]
        } else {
            bar + 4.0 * (ids[count - 1] + 1.0)
        };
        let from = bars[k.saturating_sub(1)];
        let to = bars[(k + 1).min(count)];
        let line = Line::through(&numbered[from..to.max(from + 2).min(numbered.len())]);
        (n, line.at(n))
    };
    // Whether bar `k`'s beats keep to the line from `a` to `b`.
    let on = |a: (f64, f64), b: (f64, f64), k: usize| {
        let period = (b.1 - a.1) / (b.0 - a.0);
        let mut off: Vec<f64> = numbered[bars[k]..bars[k + 1]]
            .iter()
            .map(|&(n, t)| (t - (a.1 + (n - a.0) * period)).abs())
            .collect();
        median(&mut off) <= FLEX_TOLERANCE
    };
    let mut knots = vec![knot(0)];
    let mut first = 0;
    let mut end = FLEX_MIN_BARS.min(count);
    while end < count {
        // Bar `end` joins the stretch unless two bars in a row are off the
        // line to the bar line after it (the bar after counts too).
        let (a, b) = (knots[knots.len() - 1], knot(end + 1));
        let off = |k: usize| k < count && !on(a, b, k);
        if !(first..=end).any(|k| off(k) && off(k + 1)) {
            end += 1;
            continue;
        }
        knots.push(knot(end));
        first = end;
        end = (first + FLEX_MIN_BARS).min(count);
    }
    knots.push(knot(count));
    // Bar 1's line may be before the first beat.
    let lead = knots[0];
    let period0 = (knots[1].1 - lead.1) / (knots[1].0 - lead.0);
    knots[0] = (start, lead.1 + (start - lead.0) * period0);
    knots
        .windows(2)
        .map(|w| GridMarker {
            seconds: w[0].1,
            bpm: 60.0 * (w[1].0 - w[0].0) / (w[1].1 - w[0].1),
        })
        .collect()
}

/// How far `beats` sit from the line through them: the sum of squares.
fn squared_error(beats: &[(f64, f64)]) -> f64 {
    let line = Line::through(beats);
    beats.iter().map(|&(n, t)| (t - line.at(n)).powi(2)).sum()
}

/// Seconds as a straight line in beat numbers: `at(n) = offset + n * period`.
#[derive(Clone, Copy, Debug)]
struct Line {
    offset: f64,
    period: f64,
}

impl Line {
    /// The line through `beats`, kept from being pulled by beats far off it
    /// (a fill, a stray onset): a first guess from the median slope between
    /// beats half the run apart, then least squares through the beats near
    /// that guess.
    fn through(beats: &[(f64, f64)]) -> Self {
        let all = || {
            let mut s = Sums::default();
            for &b in beats {
                s.add(b);
            }
            s.line()
        };
        let half = beats.len() / 2;
        if half < 2 {
            return all();
        }
        let mut slopes: Vec<f64> = beats
            .iter()
            .zip(&beats[half..])
            .map(|(a, b)| (b.1 - a.1) / (b.0 - a.0))
            .collect();
        let period = median(&mut slopes);
        let mut offsets: Vec<f64> = beats.iter().map(|&(n, t)| t - n * period).collect();
        let guess = Line {
            offset: median(&mut offsets),
            period,
        };
        let mut s = Sums::default();
        for &b in beats
            .iter()
            .filter(|&&(n, t)| (t - guess.at(n)).abs() < 2.0 * TOLERANCE)
        {
            s.add(b);
        }
        if s.count * 2.0 > beats.len() as f64 {
            s.line()
        } else {
            all()
        }
    }

    fn at(&self, n: f64) -> f64 {
        self.offset + n * self.period
    }

    /// This line at the nearest whole BPM, else half BPM, when `beats` sit
    /// on that as well: tracks are mostly made at a round tempo, and a
    /// fit a few hundredths off it drifts across a long track. A tempo the
    /// beats really keep away from a round one stays.
    fn rounded(self, beats: &[(f64, f64)]) -> Line {
        let off = |line: &Line| {
            let mut d: Vec<f64> = beats.iter().map(|&(n, t)| (t - line.at(n)).abs()).collect();
            median(&mut d)
        };
        if beats.is_empty() || self.period <= 0.0 {
            return self;
        }
        let bpm = 60.0 / self.period;
        let fitted = off(&self);
        for step in [1.0, 0.5] {
            let round = (bpm / step).round() * step;
            if (round - bpm).abs() > ROUND_RANGE {
                continue;
            }
            let period = 60.0 / round;
            let mut offsets: Vec<f64> = beats.iter().map(|&(n, t)| t - n * period).collect();
            let line = Line {
                offset: median(&mut offsets),
                period,
            };
            if off(&line) <= fitted + ROUND_SLACK {
                return line;
            }
        }
        self
    }
}

/// The median of `v` (which it reorders).
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Running sums for a least-squares line.
#[derive(Clone, Copy, Default)]
struct Sums {
    count: f64,
    n: f64,
    t: f64,
    nn: f64,
    nt: f64,
}

impl Sums {
    fn add(&mut self, (n, t): (f64, f64)) {
        self.count += 1.0;
        self.n += n;
        self.t += t;
        self.nn += n * n;
        self.nt += n * t;
    }

    fn line(&self) -> Line {
        let det = self.count * self.nn - self.n * self.n;
        if self.count < 2.0 || det.abs() < 1e-12 {
            return Line {
                offset: self.t / self.count.max(1.0),
                period: 0.5,
            };
        }
        let period = (self.count * self.nt - self.n * self.t) / det;
        Line {
            offset: (self.t - period * self.n) / self.count,
            period,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny repeatable jitter, -1..1.
    fn jitter(k: usize) -> f64 {
        ((k as f64 * 12.9898).sin() * 43758.5453).fract()
    }

    #[test]
    fn jittery_steady_beats_get_one_tempo() {
        let period = 60.0 / 124.0;
        let beats: Vec<f64> = (0..600)
            .map(|k| 0.3 + k as f64 * period + 0.012 * jitter(k))
            .collect();
        let f = fit(&beats, None, 124.0).unwrap();
        assert!(!f.grid.is_dynamic(), "{:?}", f.grid);
        assert!((f.grid.bpm_at(100.0) - 124.0).abs() < 0.01);
        assert!(!f.unsure);
        for &b in &beats {
            assert!((b - f.grid.snap(b, false)).abs() < 0.02);
        }
    }

    #[test]
    fn missed_and_stray_beats_dont_matter() {
        let period = 0.5;
        let beats: Vec<f64> = (0..300)
            .filter(|k| k % 7 != 3)
            .flat_map(|k| {
                let t = k as f64 * period;
                // An extra onset now and then, a fifth of a beat late.
                if k % 11 == 0 {
                    vec![t, t + 0.1]
                } else {
                    vec![t]
                }
            })
            .collect();
        let f = fit(&beats, None, 120.0).unwrap();
        assert!(!f.grid.is_dynamic(), "{:?}", f.grid);
        assert!((f.grid.bpm_at(10.0) - 120.0).abs() < 0.01);
    }

    #[test]
    fn a_fill_where_the_tracker_slipped_stays_steady() {
        // Steady at 120 BPM, but through a fill the tracker found ten beats
        // where there are nine.
        let mut beats: Vec<f64> = (0..100).map(|k| k as f64 * 0.5).collect();
        let fill: Vec<f64> = (1..10).map(|k| 50.0 + k as f64 * 0.45).collect();
        beats.extend(fill);
        beats.extend((109..250).map(|k| k as f64 * 0.5));
        let f = fit(&beats, None, 120.0).unwrap();
        assert!(!f.grid.is_dynamic(), "{:?}", f.grid);
        assert!((f.grid.bpm_at(100.0) - 120.0).abs() < 0.01, "{:?}", f.grid);
    }

    #[test]
    fn a_real_tempo_change_starts_a_section() {
        // 128 beats at 120 BPM, then 160 at 126 BPM.
        let mut beats: Vec<f64> = (0..128).map(|k| k as f64 * 0.5).collect();
        let at = 64.0;
        beats.extend((0..160).map(|k| at + k as f64 * 60.0 / 126.0));
        let f = fit(&beats, None, 123.0).unwrap();
        assert!(f.grid.is_dynamic(), "{:?}", f.grid);
        assert!((f.grid.bpm_at(20.0) - 120.0).abs() < 0.2, "{:?}", f.grid);
        assert!((f.grid.bpm_at(120.0) - 126.0).abs() < 0.2, "{:?}", f.grid);
        for &b in &beats {
            assert!(
                (b - f.grid.snap(b, false)).abs() < 0.03,
                "{b}: {:?}",
                f.grid
            );
        }
    }

    #[test]
    fn a_slow_drift_is_followed() {
        // Live drums speeding up from 118 to 122 BPM over 400 beats.
        let mut beats = vec![0.0];
        for k in 1..400 {
            let bpm = 118.0 + 4.0 * k as f64 / 400.0;
            beats.push(beats[k - 1] + 60.0 / bpm);
        }
        let f = fit(&beats, None, 120.0).unwrap();
        assert!(f.grid.is_dynamic(), "{:?}", f.grid);
        for &b in &beats {
            assert!(
                (b - f.grid.snap(b, false)).abs() < 0.04,
                "{b}: {:?}",
                f.grid
            );
        }
    }

    #[test]
    fn bar_one_falls_on_a_downbeat() {
        // Beats from 0.25 s; the downbeats are every fourth beat from the
        // third one.
        let period = 0.5;
        let beats: Vec<f64> = (0..200).map(|k| 0.25 + k as f64 * period).collect();
        let downbeats: Vec<f64> = beats.iter().skip(2).step_by(4).copied().collect();
        let f = fit(&beats, Some(&downbeats), 120.0).unwrap();
        for &d in &downbeats {
            let n = f.grid.beat_at(d).round();
            assert_eq!(n.rem_euclid(4.0), 0.0, "{d}: beat {n}");
        }
        assert!(!f.unsure);
    }

    #[test]
    fn scattered_downbeats_make_it_unsure() {
        let beats: Vec<f64> = (0..200).map(|k| k as f64 * 0.5).collect();
        let downbeats: Vec<f64> = beats.iter().step_by(3).copied().collect();
        assert!(fit(&beats, Some(&downbeats), 120.0).unwrap().unsure);
    }

    #[test]
    fn a_tempo_near_a_whole_bpm_is_rounded() {
        // 400 beats at 90 BPM, jittered: the fit lands a little off 90.
        let beats: Vec<f64> = (0..400)
            .map(|k| 0.07 + k as f64 * 60.0 / 90.0 + 0.015 * jitter(k * 7 + 1))
            .collect();
        let f = fit(&beats, None, 90.0).unwrap();
        assert_eq!(f.grid.markers()[0].bpm, 90.0, "{:?}", f.grid);
        let unrounded = Line::through(&number(&beats, 60.0 / 90.0)).period;
        assert_ne!(60.0 / unrounded, 90.0);
        // A track that really is 59.96 BPM keeps it: over 400 beats, 60
        // would drift a fifth of a second.
        let beats: Vec<f64> = (0..400).map(|k| k as f64 * 60.0 / 59.96).collect();
        let f = fit(&beats, None, 60.0).unwrap();
        assert!(
            (f.grid.markers()[0].bpm - 59.96).abs() < 0.001,
            "{:?}",
            f.grid
        );
        // Near a half BPM too.
        let beats: Vec<f64> = (0..300)
            .map(|k| k as f64 * 60.0 / 127.5 + 0.015 * jitter(k * 3 + 2))
            .collect();
        let f = fit(&beats, None, 127.5).unwrap();
        assert_eq!(f.grid.markers()[0].bpm, 127.5, "{:?}", f.grid);
    }

    #[test]
    fn fixed_keeps_one_tempo_through_a_change() {
        let mut beats: Vec<f64> = (0..128).map(|k| k as f64 * 0.5).collect();
        beats.extend((0..160).map(|k| 64.0 + k as f64 * 60.0 / 126.0));
        let f = fit_mode(&beats, None, 123.0, GridMode::Fixed).unwrap();
        assert!(!f.grid.is_dynamic(), "{:?}", f.grid);
        assert!(
            f.unsure,
            "a fixed grid that misses the beats is worth a look"
        );
    }

    /// A drummer pushing and pulling between 116 and 124 BPM every 16 bars.
    fn drummer() -> Vec<f64> {
        let mut beats = vec![0.2];
        for k in 1..512 {
            let bpm = 120.0 + 4.0 * (k as f64 * std::f64::consts::TAU / 64.0).sin();
            beats.push(beats[k - 1] + 60.0 / bpm);
        }
        beats
    }

    #[test]
    fn flexible_follows_a_drummer_closer_than_auto() {
        let beats = drummer();
        let worst = |g: &BeatGrid| {
            beats
                .iter()
                .map(|&b| (b - g.snap(b, false)).abs())
                .fold(0.0, f64::max)
        };
        let flexible = fit_mode(&beats, None, 120.0, GridMode::Flexible).unwrap();
        let auto = fit_mode(&beats, None, 120.0, GridMode::Auto).unwrap();
        assert!(flexible.grid.is_dynamic());
        assert!(worst(&flexible.grid) < 0.02, "{:?}", flexible.grid);
        assert!(worst(&auto.grid) > worst(&flexible.grid), "{:?}", auto.grid);
        assert!(!flexible.unsure);
    }

    #[test]
    fn flexible_leaves_a_steady_track_steady() {
        // Steady 124 BPM, each beat up to 10 ms early or late, as Beat
        // This!'s 20 ms frames place them.
        let period = 60.0 / 124.0;
        let beats: Vec<f64> = (0..600)
            .map(|k| 0.3 + k as f64 * period + 0.010 * jitter(k))
            .collect();
        let f = fit_mode(&beats, None, 124.0, GridMode::Flexible).unwrap();
        assert!(f.grid.markers().len() <= 2, "{:?}", f.grid);
        assert!((f.grid.bpm_at(100.0) - 124.0).abs() < 0.05, "{:?}", f.grid);
    }

    #[test]
    fn flexible_sections_start_on_bar_lines() {
        let beats = drummer();
        let downbeats: Vec<f64> = beats.iter().skip(1).step_by(4).copied().collect();
        let f = fit_mode(&beats, Some(&downbeats), 120.0, GridMode::Flexible).unwrap();
        for m in f.grid.markers() {
            let n = f.grid.beat_at(m.seconds).round();
            assert_eq!(n.rem_euclid(4.0), 0.0, "{:?}", f.grid);
        }
    }

    #[test]
    fn modes_round_trip_by_name_and_number() {
        for m in GridMode::ALL {
            assert_eq!(GridMode::from_name(m.name()), Some(m));
            assert_eq!(GridMode::from_id(m.id()), Some(m));
        }
    }

    #[test]
    fn too_few_beats_give_no_grid() {
        assert_eq!(fit(&[0.0, 0.5, 1.0], None, 120.0), None);
    }
}
