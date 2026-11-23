//! Meta's HTDemucs v4 (MIT weights), through the vendored `demucs-core`
//! (third_party/demucs-core). Weights are the safetensors conversion
//! `htdemucs.safetensors` (84 MB).

use std::ops::Range;

use burn::prelude::Backend;
use demucs_core::{Demucs, ModelOptions, CHUNK_STRIDE, TRAINING_LENGTH};

pub use demucs_core::model::metadata::{download_url, HTDEMUCS};

/// The stems in the order [`HtDemucs::separate`] returns them.
pub const STEMS: [&str; 4] = ["drums", "bass", "other", "vocals"];

pub struct HtDemucs<B: Backend> {
    model: Demucs<B>,
}

impl<B: Backend> HtDemucs<B> {
    pub fn load(bytes: &[u8], device: &B::Device) -> Result<Self, String> {
        Demucs::from_bytes(ModelOptions::FourStem, bytes, device.clone())
            .map(|model| HtDemucs { model })
            .map_err(|e| e.to_string())
    }

    /// Compiles the GPU kernels for the model's shapes, which otherwise
    /// happens during the first separation.
    pub fn warm_up(&self) {
        pollster::block_on(self.model.warmup());
    }

    /// Separates a stereo track sampled at `rate` into [`STEMS`]:
    /// `[stem][channel][sample]`, at the same rate and length. The model
    /// runs at 44.1 kHz; other rates are resampled to it and back, so the
    /// stems line up with the track sample for sample.
    pub fn separate(&self, channels: [&[f32]; 2], rate: u32) -> Result<Vec<[Vec<f32>; 2]>, String> {
        let stems = pollster::block_on(self.model.separate(channels[0], channels[1], rate))
            .map_err(|e| e.to_string())?;
        Ok(stems.into_iter().map(|s| [s.left, s.right]).collect())
    }

    /// As [`Self::separate`], telling `progress` each time one of the
    /// model's chunks is done (chunks done, of how many) and giving up with
    /// [`STOPPED`] once `keep_going` says no. `progress` may wait, which
    /// pauses the separation between chunks.
    pub fn separate_watched(
        &self,
        channels: [&[f32]; 2],
        rate: u32,
        progress: &mut dyn FnMut(usize, usize),
        keep_going: &dyn Fn() -> bool,
    ) -> Result<Vec<[Vec<f32>; 2]>, String> {
        let mut watch = Watch {
            progress,
            keep_going,
        };
        let stems = pollster::block_on(self.model.separate_with_listener(
            channels[0],
            channels[1],
            rate,
            &mut watch,
        ))
        .map_err(|e| match e {
            demucs_core::DemucsError::Cancelled => STOPPED.to_string(),
            e => e.to_string(),
        })?;
        Ok(stems.into_iter().map(|s| [s.left, s.right]).collect())
    }
}

impl<B: Backend> HtDemucs<B> {
    /// As [`Self::separate_watched`], for audio at the model's 44.1 kHz
    /// ([`crate::SAMPLE_RATE`]), taking the model's chunks from the one
    /// holding sample `from` outwards (see [`outwards`]) instead of from the
    /// start. Each time a stretch of samples has all the chunks over it,
    /// its stems are final and `ready` gets it with every stem so far
    /// (`[stem][channel][sample]`, only that stretch to be read). The stems
    /// it returns are the same as [`Self::separate`]'s, sample for sample.
    pub fn separate_from(
        &self,
        channels: [&[f32]; 2],
        from: usize,
        progress: &mut dyn FnMut(usize, usize),
        keep_going: &dyn Fn() -> bool,
        ready: &mut Ready,
    ) -> Result<Vec<[Vec<f32>; 2]>, String> {
        let n = channels[0].len().min(channels[1].len());
        let spans = chunk_spans(n);
        let mut out = vec![[vec![0.0f32; n], vec![0.0f32; n]]; STEMS.len()];
        if spans.is_empty() {
            return Ok(out);
        }
        // As `separate` does: one chunk as it is, longer audio as chunks
        // blended by a triangular window and divided by the weights.
        let windowed = spans.len() > 1;
        let mut weight = vec![0.0f32; n];
        let pieces = Pieces::new(&spans);
        let mut emitted = vec![false; pieces.ranges.len()];
        let mut done = vec![false; spans.len()];
        // The chunks over `from` first: where neighbours overlap, its
        // stems need both.
        let at = from.min(n - 1);
        let over: Vec<usize> = (0..spans.len())
            .filter(|&c| spans[c].contains(&at))
            .collect();
        let mut order = outwards(spans.len(), *over.last().unwrap_or(&0));
        if let [before, _] = over[..] {
            order.retain(|&c| c != before);
            order.insert(1, before);
        }
        for (k, &c) in order.iter().enumerate() {
            if !keep_going() {
                return Err(STOPPED.into());
            }
            let span = spans[c].clone();
            let stems = pollster::block_on(
                self.model
                    .separate_segment(&channels[0][span.clone()], &channels[1][span.clone()]),
            )
            .map_err(|e| e.to_string())?;
            let window = if windowed {
                demucs_core::triangular_window(span.len())
            } else {
                vec![1.0; span.len()]
            };
            for stem in &stems {
                let Some(s) = HTDEMUCS.stems.iter().position(|&id| id == stem.id) else {
                    continue;
                };
                for (i, w) in window.iter().enumerate() {
                    out[s][0][span.start + i] += w * stem.left[i];
                    out[s][1][span.start + i] += w * stem.right[i];
                }
            }
            for (i, w) in window.iter().enumerate() {
                weight[span.start + i] += w;
            }
            done[c] = true;
            for p in pieces.of(&span) {
                if emitted[p] || !pieces.covers[p].iter().all(|&c| done[c]) {
                    continue;
                }
                let range = pieces.ranges[p].clone();
                if windowed {
                    for i in range.clone() {
                        let w = weight[i];
                        if w > 0.0 {
                            for stem in out.iter_mut() {
                                stem[0][i] /= w;
                                stem[1][i] /= w;
                            }
                        }
                    }
                }
                emitted[p] = true;
                ready(range, &out);
            }
            progress(k + 1, spans.len());
        }
        if !keep_going() {
            return Err(STOPPED.into());
        }
        Ok(out)
    }
}

/// The samples each of the model's chunks covers, for `n` samples at
/// 44.1 kHz: one chunk up to a segment, else segments [`CHUNK_STRIDE`]
/// apart, the last ending at `n`.
fn chunk_spans(n: usize) -> Vec<Range<usize>> {
    if n == 0 {
        return Vec::new();
    }
    (0..demucs_core::num_chunks(n))
        .map(|c| {
            let start = c * CHUNK_STRIDE;
            start..(start + TRAINING_LENGTH).min(n)
        })
        .collect()
}

/// The stretches between chunk edges, and the chunks over each.
struct Pieces {
    ranges: Vec<Range<usize>>,
    covers: Vec<Vec<usize>>,
}

impl Pieces {
    fn new(spans: &[Range<usize>]) -> Pieces {
        let mut cuts: Vec<usize> = spans.iter().flat_map(|r| [r.start, r.end]).collect();
        cuts.sort_unstable();
        cuts.dedup();
        let ranges: Vec<Range<usize>> = cuts.windows(2).map(|w| w[0]..w[1]).collect();
        let covers = ranges
            .iter()
            .map(|p| {
                (0..spans.len())
                    .filter(|&c| spans[c].start <= p.start && spans[c].end >= p.end)
                    .collect()
            })
            .collect();
        Pieces { ranges, covers }
    }

    /// The pieces inside `span`.
    fn of(&self, span: &Range<usize>) -> impl Iterator<Item = usize> + '_ {
        let span = span.clone();
        (0..self.ranges.len())
            .filter(move |&p| self.ranges[p].start >= span.start && self.ranges[p].end <= span.end)
    }
}

/// The order to take `count` chunks in, from chunk `first` outwards: three
/// ahead for each one behind, so the stems stay ahead of a playing deck
/// while those just before it (where a DJ tends to jump back to) follow.
pub fn outwards(count: usize, first: usize) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    let first = first.min(count - 1);
    let mut order = Vec::with_capacity(count);
    order.push(first);
    let (mut ahead, mut behind) = (first + 1, first);
    while order.len() < count {
        for _ in 0..3 {
            if ahead < count {
                order.push(ahead);
                ahead += 1;
            }
        }
        if behind > 0 {
            behind -= 1;
            order.push(behind);
        }
    }
    order
}

/// Told each stretch of samples whose stems are final, with all the stems
/// so far (`[stem][channel][sample]`); see [`HtDemucs::separate_from`].
pub type Ready<'a> = dyn FnMut(Range<usize>, &[[Vec<f32>; 2]]) + 'a;

/// The error a separation stopped by the DJ ends with.
pub const STOPPED: &str = "stopped";

struct Watch<'a> {
    progress: &'a mut dyn FnMut(usize, usize),
    keep_going: &'a dyn Fn() -> bool,
}

impl demucs_core::listener::ForwardListener for Watch<'_> {
    fn on_event(&mut self, event: demucs_core::listener::ForwardEvent) {
        if let demucs_core::listener::ForwardEvent::ChunkDone { index, total } = event {
            (self.progress)(index + 1, total);
        }
    }

    fn is_cancelled(&self) -> bool {
        !(self.keep_going)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few seconds of a drum-and-bass-ish loop at `rate`: kicks, a bass
    /// tone and a noisy hat, stereo.
    fn loop_at(rate: u32, seconds: f32) -> [Vec<f32>; 2] {
        let n = (rate as f32 * seconds) as usize;
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        let (mut l, mut r) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for i in 0..n {
            let t = i as f32 / rate as f32;
            let beat = t % 0.5;
            let kick = (std::f32::consts::TAU * 55.0 * beat).sin() * (-beat * 30.0).exp();
            let bass = (std::f32::consts::TAU * 110.0 * t).sin() * 0.2;
            let off = (t + 0.25) % 0.5;
            let hat = noise() * (-off * 200.0).exp() * 0.3;
            l.push(0.5 * kick + bass + hat);
            r.push(0.5 * kick + bass * 0.8 + hat * 0.6);
        }
        [l, r]
    }

    /// How far the stems' sum is from the track, against the track: near 0
    /// when they line up, near 1 or more when they drift.
    fn residual(track: &[Vec<f32>; 2], stems: &[[Vec<f32>; 2]]) -> f32 {
        let (mut err, mut sig) = (0.0f64, 0.0f64);
        for c in 0..2 {
            for (i, x) in track[c].iter().enumerate() {
                let sum: f32 = stems
                    .iter()
                    .map(|s| s[c].get(i).copied().unwrap_or(0.0))
                    .sum();
                err += f64::from(x - sum).powi(2);
                sig += f64::from(*x).powi(2);
            }
        }
        (err / sig).sqrt() as f32
    }

    #[test]
    fn chunks_go_outwards_mostly_ahead() {
        assert_eq!(outwards(0, 3), Vec::<usize>::new());
        assert_eq!(outwards(1, 5), vec![0]);
        assert_eq!(outwards(8, 2), vec![2, 3, 4, 5, 1, 6, 7, 0]);
        assert_eq!(outwards(4, 3), vec![3, 2, 1, 0]);
    }

    #[test]
    fn every_sample_is_in_one_piece_with_its_chunks() {
        for n in [
            1,
            TRAINING_LENGTH,
            TRAINING_LENGTH + 1,
            5 * TRAINING_LENGTH + 17,
        ] {
            let spans = chunk_spans(n);
            assert_eq!(spans.last().unwrap().end, n);
            let pieces = Pieces::new(&spans);
            assert_eq!(pieces.ranges.first().unwrap().start, 0);
            assert_eq!(pieces.ranges.last().unwrap().end, n);
            for (p, r) in pieces.ranges.iter().enumerate() {
                assert!(!pieces.covers[p].is_empty() && pieces.covers[p].len() <= 2);
                for (c, span) in spans.iter().enumerate() {
                    let over = span.start < r.end && span.end > r.start;
                    assert_eq!(over, pieces.covers[p].contains(&c), "{n} {r:?} {c}");
                }
            }
        }
    }

    /// Needs the weights and takes a few minutes on the CPU:
    /// `OPENDJ_MODELS=<folder with htdemucs.safetensors> cargo test -p
    /// opendj-separation --release -- --ignored from_the_middle`.
    #[test]
    #[ignore]
    fn separating_from_the_middle_gives_the_same_stems() {
        let dir = std::env::var("OPENDJ_MODELS").expect("OPENDJ_MODELS");
        let bytes = std::fs::read(std::path::Path::new(&dir).join("htdemucs.safetensors")).unwrap();
        let model = HtDemucs::<crate::Cpu>::load(&bytes, &Default::default()).unwrap();
        // Two chunks, starting where they overlap.
        let track = loop_at(crate::SAMPLE_RATE, 12.0);
        let whole = model
            .separate([&track[0], &track[1]], crate::SAMPLE_RATE)
            .unwrap();
        let mut seen = vec![false; track[0].len()];
        // How many chunks were done when the play position's stems were.
        let chunks = std::cell::Cell::new(0);
        let mut at_from = None;
        let from = model
            .separate_from(
                [&track[0], &track[1]],
                7 * 44_100,
                &mut |done, _| chunks.set(done),
                &|| true,
                &mut |r, stems| {
                    if r.contains(&(7 * 44_100)) {
                        at_from = Some(chunks.get() + 1);
                    }
                    for i in r {
                        assert!(!seen[i]);
                        seen[i] = true;
                        for s in 0..4 {
                            assert_eq!(stems[s][0][i], whole[s][0][i], "stem {s} at {i}");
                        }
                    }
                },
            )
            .unwrap();
        assert!(seen.iter().all(|s| *s));
        // Both chunks over it, and no more.
        assert_eq!(at_from, Some(2));
        assert_eq!(from, whole);
    }

    /// Needs the weights and takes a minute on the CPU:
    /// `OPENDJ_MODELS=<folder with htdemucs.safetensors> cargo test -p
    /// opendj-separation --release -- --ignored stems_of_a_48`.
    #[test]
    #[ignore]
    fn stems_of_a_48_khz_track_line_up_with_it() {
        let dir = std::env::var("OPENDJ_MODELS").expect("OPENDJ_MODELS");
        let bytes = std::fs::read(std::path::Path::new(&dir).join("htdemucs.safetensors")).unwrap();
        let model = HtDemucs::<crate::Cpu>::load(&bytes, &Default::default()).unwrap();
        let track = loop_at(48_000, 6.0);
        let stems = model.separate([&track[0], &track[1]], 48_000).unwrap();
        assert!(stems.iter().all(|s| s[0].len() == track[0].len()));
        let aligned = residual(&track, &stems);
        // As before the fix: told 44.1 kHz, then stretched back to 48 kHz.
        let told = model
            .separate([&track[0], &track[1]], crate::SAMPLE_RATE)
            .unwrap();
        let stretched: Vec<[Vec<f32>; 2]> = told
            .iter()
            .map(|s| {
                s.clone()
                    .map(|x| crate::resample(&x, crate::SAMPLE_RATE, 48_000).unwrap())
            })
            .collect();
        let drifted = residual(&track, &stretched);
        eprintln!("residual: aligned {aligned:.3}, stretched {drifted:.3}");
        assert!(aligned < 0.2, "{aligned}");
        assert!(drifted > 0.5, "{drifted}");
    }
}
