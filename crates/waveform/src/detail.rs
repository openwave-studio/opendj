//! The real wave, zoomed right in. Once a screen pixel covers about a
//! millisecond of audio or less, a scrolling view draws each pixel column
//! from the track's own samples (their lowest and highest) instead of the
//! summary, so single cycles of the bass and the gaps between hits show.
//!
//! Only the columns on screen are worked out, on the CPU, each frame. A
//! column keeps its slices while the view scrolls (the picture moves a whole
//! pixel at a time), so most frames only work out the few columns that
//! scrolled in; a zoom step or a jump works them all out again, at most
//! [`BUDGET`] frames of audio a frame, the rest drawn from the summary until
//! their turn comes.

use std::sync::Arc;

/// A track's decoded audio, for drawing its real wave.
pub trait Samples: Send + Sync {
    /// Frames a second.
    fn sample_rate(&self) -> f64;
    /// Frames in the track.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Frames that can be read now, from the start (all of them once the
    /// track is decoded).
    fn ready(&self) -> usize;
    /// The lowest and highest sample over frames `from..to`, reading each
    /// frame as its louder channel.
    fn min_max(&self, from: usize, to: usize) -> [f32; 2];
    /// The track's loudest sample, which its summary is scaled to; none
    /// until it is known, and the real wave isn't drawn until then.
    fn top(&self) -> Option<f32>;
}

/// A column not worked out yet: drawn from the summary.
pub(crate) const UNKNOWN: [f32; 2] = [1.0, -1.0];

/// The most frames of audio a view reads in one frame.
pub(crate) const BUDGET: usize = 1 << 19;

/// The columns a view last drew, kept for the next frame.
#[derive(Default)]
pub(crate) struct Columns {
    samples: Option<Arc<dyn Samples>>,
    /// Slices per column, and the first column's index (columns counted
    /// from slice 0).
    width: f32,
    first: i64,
    cols: Vec<[f32; 2]>,
    spare: Vec<[f32; 2]>,
}

impl Columns {
    pub fn set_samples(&mut self, samples: Option<Arc<dyn Samples>>) {
        let same = match (&self.samples, &samples) {
            (Some(a), Some(b)) => std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.samples = samples;
            self.cols.clear();
        }
    }

    pub fn samples(&self) -> Option<&Arc<dyn Samples>> {
        self.samples.as_ref()
    }

    /// The lowest and highest sample of `count` columns of `width` slices
    /// each, from column `first`, at `bins_per_second` slices a second;
    /// [`UNKNOWN`] for those not read yet. Columns from the last call are
    /// kept.
    pub fn update(
        &mut self,
        first: i64,
        count: usize,
        width: f32,
        bins_per_second: f64,
    ) -> &[[f32; 2]] {
        let Some(s) = &self.samples else {
            self.cols.clear();
            return &self.cols;
        };
        if width != self.width {
            self.cols.clear();
            self.width = width;
        }
        let per_column = s.sample_rate() / bins_per_second * width as f64;
        let (len, ready) = (s.len(), s.ready());
        let mut budget = BUDGET;
        let mut cols = std::mem::take(&mut self.spare);
        cols.clear();
        for i in 0..count as i64 {
            let column = first + i;
            let kept = usize::try_from(column - self.first)
                .ok()
                .and_then(|j| self.cols.get(j))
                .filter(|c| c[0] <= c[1]);
            if let Some(c) = kept {
                cols.push(*c);
                continue;
            }
            if column < 0 {
                cols.push([0.0; 2]);
                continue;
            }
            // From the frame at or before the column's start to the one at
            // or after its end, so neighbours share a frame and join up.
            let from = (column as f64 * per_column).floor() as usize;
            let to = (((column + 1) as f64 * per_column).ceil() as usize + 1).min(len);
            if from >= to {
                cols.push([0.0; 2]);
            } else if to > ready || to - from > budget {
                cols.push(UNKNOWN);
            } else {
                budget -= to - from;
                cols.push(s.min_max(from, to));
            }
        }
        self.spare = std::mem::replace(&mut self.cols, cols);
        self.first = first;
        &self.cols
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A sine at 100 Hz, 48 000 frames a second, counting frames read.
    struct Sine {
        len: usize,
        ready: usize,
        read: AtomicUsize,
    }

    impl Samples for Sine {
        fn sample_rate(&self) -> f64 {
            48_000.0
        }
        fn len(&self) -> usize {
            self.len
        }
        fn ready(&self) -> usize {
            self.ready
        }
        fn min_max(&self, from: usize, to: usize) -> [f32; 2] {
            self.read.fetch_add(to - from, Ordering::Relaxed);
            (from..to)
                .map(|i| (i as f32 * std::f32::consts::TAU * 100.0 / 48_000.0).sin())
                .fold([f32::MAX, f32::MIN], |[lo, hi], v| [lo.min(v), hi.max(v)])
        }
        fn top(&self) -> Option<f32> {
            Some(1.0)
        }
    }

    fn sine(len: usize, ready: usize) -> Arc<Sine> {
        Arc::new(Sine {
            len,
            ready,
            read: AtomicUsize::new(0),
        })
    }

    #[test]
    fn columns_follow_the_wave() {
        let s = sine(48_000, 48_000);
        let mut c = Columns::default();
        c.set_samples(Some(s.clone()));
        // Half a slice a column: 24 frames, a 20th of a 100 Hz cycle.
        let cols = c.update(0, 40, 0.5, 1000.0).to_vec();
        // The first quarter cycle (120 frames) rises from 0 to 1.
        assert!(cols[0][0].abs() < 0.01);
        assert!(cols[4][1] > 0.99);
        // Half a cycle on it is all below the line.
        assert!(cols[12][1] < 0.0 && cols[14][0] < -0.99);
        // Neighbours join: each starts where the last ended.
        for w in cols.windows(2) {
            assert!(w[1][0] <= w[0][1] && w[1][1] >= w[0][0]);
        }
    }

    #[test]
    fn scrolling_reads_only_the_new_columns() {
        let s = sine(480_000, 480_000);
        let mut c = Columns::default();
        c.set_samples(Some(s.clone()));
        c.update(100, 1000, 1.0, 1000.0);
        let first = s.read.swap(0, Ordering::Relaxed);
        assert!(first >= 48 * 1000);
        // Three pixels on: three new columns.
        c.update(103, 1000, 1.0, 1000.0);
        let more = s.read.swap(0, Ordering::Relaxed);
        assert!(more <= 3 * 50, "{more}");
        // A zoom step reads them all again.
        c.update(103, 1000, 1.5, 1000.0);
        assert!(s.read.swap(0, Ordering::Relaxed) >= 72 * 1000);
    }

    #[test]
    fn undecoded_and_over_budget_columns_wait() {
        // Only the first second decoded.
        let s = sine(480_000, 48_000);
        let mut c = Columns::default();
        c.set_samples(Some(s.clone()));
        let cols = c.update(990, 20, 1.0, 1000.0).to_vec();
        assert!(cols[0][0] <= cols[0][1]);
        assert_eq!(cols[15], UNKNOWN);
        // Wider than the budget: the rest come in later frames.
        let s = sine(48_000_000, 48_000_000);
        c.set_samples(Some(s.clone()));
        let cols = c.update(0, 8000, 4.0, 1000.0).to_vec();
        let known = cols.iter().filter(|c| c[0] <= c[1]).count();
        assert!(known > 0 && known < 8000);
        let cols = c.update(0, 8000, 4.0, 1000.0).to_vec();
        assert!(cols.iter().filter(|c| c[0] <= c[1]).count() > known);
    }

    #[test]
    fn before_and_past_the_track_is_silence() {
        let s = sine(4_800, 4_800);
        let mut c = Columns::default();
        c.set_samples(Some(s));
        let cols = c.update(-5, 120, 1.0, 1000.0).to_vec();
        assert_eq!(cols[0], [0.0; 2]);
        assert_eq!(cols[119], [0.0; 2]);
    }
}
