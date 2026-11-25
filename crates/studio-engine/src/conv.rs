//! Convolution with an impulse response, for Hybrid Reverb's convolution
//! side: uniformly partitioned, overlap-add in the frequency domain. The
//! response is cut into blocks, each block's spectrum is kept, and every
//! block of input is multiplied with all of them as it moves through a
//! delay line of spectra. Everything is allocated when it's built; the
//! audio thread only runs FFTs with scratch it already has. The output comes
//! [`Convolver::latency`] samples late.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// The longest response used; anything after is cut off.
pub const MAX_IR_SECONDS: f32 = 8.0;

/// A mono input through a stereo impulse response.
pub struct Convolver {
    block: usize,
    parts: usize,
    bins: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    /// The response's spectra, block after block, per side.
    ir: [Vec<Complex<f32>>; 2],
    /// The input's last `parts` spectra; `head` is the newest.
    fdl: Vec<Complex<f32>>,
    head: usize,
    input: Vec<f32>,
    filled: usize,
    time: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    acc: Vec<Complex<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    ifft_scratch: Vec<Complex<f32>>,
    overlap: [Vec<f32>; 2],
    out: [Vec<f32>; 2],
}

impl Convolver {
    /// `ir` at the engine's rate. Longer responses use longer blocks, which
    /// costs latency but keeps the work per sample about the same.
    pub fn new(ir: &[[f32; 2]], rate: f64) -> Convolver {
        let len = ir.len().clamp(1, (MAX_IR_SECONDS * rate as f32) as usize);
        let block = (len / 256).next_power_of_two().clamp(128, 2048);
        let parts = len.div_ceil(block);
        let n = block * 2;
        let bins = block + 1;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let mut time = vec![0.0; n];
        let mut spectrum = vec![Complex::default(); bins];
        let mut fft_scratch = fft.make_scratch_vec();
        let ir_spectra = [0, 1].map(|c| {
            let mut all = Vec::with_capacity(parts * bins);
            for p in 0..parts {
                time.fill(0.0);
                for (i, t) in time[..block].iter_mut().enumerate() {
                    *t = ir.get(p * block + i).map_or(0.0, |f| f[c]);
                }
                fft.process_with_scratch(&mut time, &mut spectrum, &mut fft_scratch)
                    .ok();
                all.extend_from_slice(&spectrum);
            }
            all
        });
        Convolver {
            block,
            parts,
            bins,
            ifft_scratch: ifft.make_scratch_vec(),
            fft,
            ifft,
            ir: ir_spectra,
            fdl: vec![Complex::default(); parts * bins],
            head: 0,
            input: vec![0.0; block],
            filled: 0,
            time,
            spectrum,
            acc: vec![Complex::default(); bins],
            fft_scratch,
            overlap: [vec![0.0; block], vec![0.0; block]],
            out: [vec![0.0; block], vec![0.0; block]],
        }
    }

    /// How late the output is, in samples.
    pub fn latency(&self) -> usize {
        self.block
    }

    /// Takes one input sample and gives one output frame.
    #[inline]
    pub fn process(&mut self, x: f32) -> [f32; 2] {
        let i = self.filled;
        let y = [self.out[0][i], self.out[1][i]];
        self.input[i] = x;
        self.filled += 1;
        if self.filled == self.block {
            self.filled = 0;
            self.run_block();
        }
        y
    }

    fn run_block(&mut self) {
        let (b, bins) = (self.block, self.bins);
        // The new block's spectrum goes in the delay line.
        self.time[..b].copy_from_slice(&self.input);
        self.time[b..].fill(0.0);
        self.fft
            .process_with_scratch(&mut self.time, &mut self.spectrum, &mut self.fft_scratch)
            .ok();
        self.head = (self.head + self.parts - 1) % self.parts;
        let at = self.head * bins;
        self.fdl[at..at + bins].copy_from_slice(&self.spectrum);
        let scale = 1.0 / (2 * b) as f32;
        for c in 0..2 {
            self.acc.fill(Complex::default());
            for p in 0..self.parts {
                let slot = ((self.head + p) % self.parts) * bins;
                let x = &self.fdl[slot..slot + bins];
                let h = &self.ir[c][p * bins..(p + 1) * bins];
                for ((a, x), h) in self.acc.iter_mut().zip(x).zip(h) {
                    *a += x * h;
                }
            }
            // The inverse wants a real DC and Nyquist bin.
            self.acc[0].im = 0.0;
            self.acc[bins - 1].im = 0.0;
            self.ifft
                .process_with_scratch(&mut self.acc, &mut self.time, &mut self.ifft_scratch)
                .ok();
            for i in 0..b {
                self.out[c][i] = (self.time[i] + self.overlap[c][i]) * scale;
                self.overlap[c][i] = self.time[b + i];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The output is the input convolved with the response, `latency` late.
    #[test]
    fn matches_direct_convolution() {
        let ir: Vec<[f32; 2]> = (0..1000)
            .map(|i| {
                let v = (i as f32 * 0.37).sin() * (-(i as f32) / 300.0).exp();
                [v, -v * 0.5]
            })
            .collect();
        let mut c = Convolver::new(&ir, 48_000.0);
        let lat = c.latency();
        let input: Vec<f32> = (0..3000)
            .map(|i| ((i * 7919) % 101) as f32 / 50.0 - 1.0)
            .collect();
        let out: Vec<[f32; 2]> = input.iter().map(|&x| c.process(x)).collect();
        for n in (lat..3000).step_by(37) {
            let want: f32 = (0..ir.len())
                .filter(|&k| k <= n - lat)
                .map(|k| input[n - lat - k] * ir[k][0])
                .sum();
            assert!(
                (out[n][0] - want).abs() < 1e-3,
                "{n}: {} vs {want}",
                out[n][0]
            );
            assert!((out[n][1] + want * 0.5).abs() < 1e-3);
        }
    }
}
#[cfg(test)]
mod bench {
    #[test]
    #[ignore]
    fn cost() {
        for secs in [0.7f32, 3.0, 8.0] {
            let ir: Vec<[f32; 2]> = (0..(secs * 48_000.0) as usize)
                .map(|i| [((i * 31) % 7) as f32 * 0.01; 2])
                .collect();
            let mut c = super::Convolver::new(&ir, 48_000.0);
            let t = std::time::Instant::now();
            let mut acc = 0.0;
            for i in 0..480_000 {
                acc += c.process((i % 13) as f32 * 0.1)[0];
            }
            let ns = t.elapsed().as_nanos() as f64 / 480_000.0;
            println!(
                "{secs} s IR: {ns:.0} ns/sample, block {} ({acc})",
                c.latency()
            );
        }
    }
}
