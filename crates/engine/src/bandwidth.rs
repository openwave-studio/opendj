//! How high a track's sound reaches, to tell a real lossless file from a
//! lossy one saved as FLAC, WAV or AIFF.
//!
//! Lossy encoders cut the top of the spectrum off with a steep low-pass
//! (MP3 at 128 kbps near 16 kHz, 192 kbps near 18-19 kHz, AAC similar).
//! Decoding and saving as lossless keeps that cliff. A CD master goes up
//! to about 20 kHz before its own anti-alias filter, so a cliff well below
//! that in a "lossless" file gives it away.
//!
//! The average power spectrum is measured over the whole track (skipping
//! silence), and the cutoff is where it falls by [`CLIFF_DB`] or more
//! within [`WINDOW_HZ`] and stays down above it.

use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;

const FFT: usize = 4096;
/// Width of the bands compared on each side of a candidate cutoff.
const WINDOW_HZ: f32 = 500.0;
/// How far the level must fall at the cutoff.
const CLIFF_DB: f32 = 25.0;
/// Cutoffs are looked for above this: below it, it's the music.
const LOWEST_HZ: f32 = 11_000.0;
/// A cutoff below this in a file sampled at 44.1 kHz or more means it was
/// lossy once.
pub const LOSSY_BELOW_HZ: f32 = 19_500.0;

/// Measures where a track's spectrum is cut off, fed a block at a time.
pub struct Bandwidth {
    rate: f32,
    power: Vec<f64>,
    block: Vec<f32>,
    window: Vec<f32>,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    blocks: usize,
}

impl Bandwidth {
    pub fn new(sample_rate: u32) -> Self {
        let window = (0..FFT)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT as f32).cos())
            .collect();
        Self {
            rate: sample_rate.max(1) as f32,
            power: vec![0.0; FFT / 2],
            block: Vec::with_capacity(FFT),
            window,
            fft: FftPlanner::new().plan_fft_forward(FFT),
            blocks: 0,
        }
    }

    /// Stereo frames, in order.
    pub fn feed(&mut self, frames: &[[f32; 2]]) {
        for f in frames {
            self.block.push((f[0] + f[1]) * 0.5);
            if self.block.len() == FFT {
                self.take_block();
            }
        }
    }

    fn take_block(&mut self) {
        let rms = (self.block.iter().map(|x| x * x).sum::<f32>() / FFT as f32).sqrt();
        // Silence says nothing about the spectrum.
        if rms > 1e-3 {
            let mut buf: Vec<Complex32> = self
                .block
                .iter()
                .zip(&self.window)
                .map(|(x, w)| Complex32::new(x * w, 0.0))
                .collect();
            self.fft.process(&mut buf);
            for (p, c) in self.power.iter_mut().zip(&buf) {
                *p += c.norm_sqr() as f64;
            }
            self.blocks += 1;
        }
        self.block.clear();
    }

    /// Where the spectrum is cut off, in Hz; `None` when it isn't (or too
    /// little sound was heard to say).
    pub fn cutoff(&self) -> Option<f32> {
        if self.blocks < 8 {
            return None;
        }
        let bin_hz = self.rate / FFT as f32;
        let w = (WINDOW_HZ / bin_hz).round().max(2.0) as usize;
        let db = |from: usize, to: usize| -> f32 {
            let to = to.min(self.power.len());
            if to <= from {
                return f32::NEG_INFINITY;
            }
            let mean = self.power[from..to].iter().sum::<f64>() / (to - from) as f64;
            (10.0 * (mean + 1e-30).log10()) as f32
        };
        let top = (self.power.len() as f32 * 0.98) as usize;
        let lowest = (LOWEST_HZ / bin_hz) as usize;
        // The biggest fall from one band to the next, the band above staying
        // down all the way up.
        let mut best: Option<(usize, f32)> = None;
        for i in lowest.max(w)..top.saturating_sub(w) {
            let below = db(i - w, i);
            let above = db(i, i + w);
            let rest = db(i, top);
            let fall = below - above.max(rest);
            if fall >= CLIFF_DB && best.is_none_or(|(_, f)| fall > f) {
                best = Some((i, fall));
            }
        }
        best.map(|(i, _)| i as f32 * bin_hz)
    }
}

/// Where the spectrum of `audio` is cut off (see [`Bandwidth`]).
pub fn cutoff(audio: &crate::track::Audio) -> Option<f32> {
    let mut b = Bandwidth::new(audio.sample_rate);
    for chunk in audio.decoded_chunks() {
        b.feed(chunk);
    }
    b.cutoff()
}

/// Whether a lossless file cut off at `cutoff` was lossy before: cut well
/// below what its sample rate can hold.
pub fn was_lossy(cutoff: Option<f32>, sample_rate: u32) -> bool {
    match cutoff {
        Some(hz) => sample_rate >= 44_100 && hz < LOSSY_BELOW_HZ,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three seconds of many tones up to `top` Hz, with quiet noise across
    /// the whole spectrum the way dither leaves it.
    fn music(top: f32, rate: u32) -> Vec<[f32; 2]> {
        let mut seed = 12345u32;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        let tones: Vec<(f32, f32, f32)> = (0..200)
            .map(|i| {
                let f = 60.0 + (top - 60.0) * (i as f32 + rnd()) / 200.0;
                (f, rnd() * std::f32::consts::TAU, 0.02 / (f / 1000.0).sqrt())
            })
            .collect();
        (0..rate as usize * 3)
            .map(|n| {
                let t = n as f32 / rate as f32;
                let s: f32 = tones
                    .iter()
                    .map(|(f, ph, a)| a * (std::f32::consts::TAU * f * t + ph).sin())
                    .sum();
                let x = s + (rnd() - 0.5) * 3e-5;
                [x, x]
            })
            .collect()
    }

    fn measured(top: f32, rate: u32) -> Option<f32> {
        let mut b = Bandwidth::new(rate);
        b.feed(&music(top, rate));
        b.cutoff()
    }

    #[test]
    fn finds_an_mp3_style_cutoff() {
        let hz = measured(16_000.0, 44_100).expect("a cutoff");
        assert!((15_500.0..16_500.0).contains(&hz), "{hz}");
        assert!(was_lossy(Some(hz), 44_100));
    }

    #[test]
    fn full_band_music_is_not_cut() {
        let hz = measured(21_000.0, 44_100);
        assert!(!was_lossy(hz, 44_100), "{hz:?}");
    }

    #[test]
    fn silence_says_nothing() {
        let mut b = Bandwidth::new(44_100);
        b.feed(&vec![[0.0, 0.0]; 44_100 * 3]);
        assert_eq!(b.cutoff(), None);
    }
}
