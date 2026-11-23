//! Resampling a stretch of a track at a time, so stems can be separated at
//! the models' 44.1 kHz and brought back to a track at another rate a chunk
//! at a time, as they come, instead of all at once at the end.
//!
//! [`resample`](crate::resample) is polyphase: every `orig` input samples
//! give exactly `new` output samples, each from the input around it. So a
//! stretch of input that starts on a block of `orig` and reaches a few
//! blocks past what an output needs resamples to exactly the same samples
//! as the whole track would.

use std::ops::Range;

/// A pair of sample rates, and how stretches resample from one to the other.
#[derive(Clone, Copy, Debug)]
pub struct Rates {
    pub from: u32,
    pub to: u32,
    /// Input samples per block.
    orig: usize,
    /// Output samples per block.
    new: usize,
    /// Blocks of input either side that an output sample can depend on.
    margin: usize,
}

impl Rates {
    pub fn new(from: u32, to: u32) -> Rates {
        let g = gcd(from.max(1), to.max(1));
        let (orig, new) = ((from.max(1) / g) as usize, (to.max(1) / g) as usize);
        // As `resample`: six zero crossings of a sinc at 0.99 of the lower
        // rate's Nyquist.
        let width = (6.0 * orig as f64 / (orig.min(new) as f64 * 0.99)).ceil() as usize;
        Rates {
            from,
            to,
            orig,
            new,
            margin: width.div_ceil(orig) + 1,
        }
    }

    /// The length of `len` input samples once resampled.
    pub fn out_len(&self, len: usize) -> usize {
        (len * self.new).div_ceil(self.orig)
    }

    /// The input samples, of `len`, that resampling needs for output samples
    /// `out`: starting on a block, with the margin either side.
    pub fn input_for(&self, out: Range<usize>, len: usize) -> Range<usize> {
        let first = out.start / self.new;
        let last = out.end.div_ceil(self.new);
        let start = (first.saturating_sub(self.margin) * self.orig).min(len);
        let end = ((last + self.margin) * self.orig).min(len);
        start..end
    }

    /// Output samples `out` of the whole input resampled, from `input`, the
    /// input samples from `at` on, as [`Rates::input_for`] gives them.
    pub fn resample_part(&self, input: &[f32], at: usize, out: Range<usize>) -> Vec<f32> {
        debug_assert_eq!(at % self.orig, 0, "a stretch starts on a block");
        let y = crate::resample(input, self.from, self.to).unwrap_or_default();
        let first = at / self.orig * self.new;
        out.map(|n| {
            n.checked_sub(first)
                .and_then(|i| y.get(i))
                .copied()
                .unwrap_or(0.0)
        })
        .collect()
    }

    /// All of `x` resampled, as [`crate::resample`] would, in `parts`
    /// stretches side by side on as many threads.
    pub fn resample_all(&self, x: &[f32], parts: usize) -> Vec<f32> {
        if self.from == self.to {
            return x.to_vec();
        }
        let total = self.out_len(x.len());
        let parts = parts.clamp(1, total.div_ceil(self.new).max(1));
        let step = total.div_ceil(parts).div_ceil(self.new) * self.new;
        let pieces: Vec<Vec<f32>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..total)
                .step_by(step.max(1))
                .map(|start| {
                    let out = start..(start + step).min(total);
                    s.spawn(move || {
                        let input = self.input_for(out.clone(), x.len());
                        self.resample_part(&x[input.clone()], input.start, out)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        pieces.concat()
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(n: usize) -> Vec<f32> {
        let mut seed = 7u32;
        (0..n)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (i as f32 * 0.05).sin() * 0.5
                    + ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.3
            })
            .collect()
    }

    #[test]
    fn stretches_resample_exactly_as_the_whole() {
        for (from, to) in [
            (44_100, 48_000),
            (48_000, 44_100),
            (96_000, 44_100),
            (22_050, 44_100),
        ] {
            let rates = Rates::new(from, to);
            let x = signal(from as usize / 3 + 17);
            let whole = crate::resample(&x, from, to).unwrap();
            assert_eq!(whole.len(), rates.out_len(x.len()), "{from} → {to}");
            for out in [
                0..1000,
                5000..9000,
                whole.len() - 700..whole.len(),
                333..334,
            ] {
                let input = rates.input_for(out.clone(), x.len());
                let part = rates.resample_part(&x[input.clone()], input.start, out.clone());
                assert_eq!(part, whole[out.clone()], "{from} → {to}, {out:?}");
            }
            assert_eq!(rates.resample_all(&x, 5), whole, "{from} → {to}, threaded");
        }
    }
}
