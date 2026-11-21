//! A deck's decoded audio, as its scrolling waveform reads it to draw the
//! real wave when zoomed right in (see `opendj_waveform::detail`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use opendj_engine::track::Audio;
use opendj_waveform::Samples;

pub struct DeckSamples {
    audio: Arc<Audio>,
    /// The loudest sample, found on a thread of its own once the track is
    /// decoded: the summary is scaled to it, and so is the real wave.
    top: Arc<OnceLock<f32>>,
    looking: AtomicBool,
}

impl DeckSamples {
    pub fn new(audio: Arc<Audio>) -> Arc<Self> {
        Arc::new(DeckSamples {
            audio,
            top: Arc::new(OnceLock::new()),
            looking: AtomicBool::new(false),
        })
    }

    pub fn audio(&self) -> &Arc<Audio> {
        &self.audio
    }
}

impl Samples for DeckSamples {
    fn sample_rate(&self) -> f64 {
        self.audio.sample_rate as f64
    }

    fn len(&self) -> usize {
        self.audio.len()
    }

    fn ready(&self) -> usize {
        if self.audio.is_complete() {
            self.audio.len()
        } else {
            self.audio.decoded()
        }
    }

    fn min_max(&self, from: usize, to: usize) -> [f32; 2] {
        (from..to)
            .map(|i| {
                let [l, r] = self.audio.frame(i);
                if l.abs() >= r.abs() {
                    l
                } else {
                    r
                }
            })
            .fold([f32::MAX, f32::MIN], |[lo, hi], v| [lo.min(v), hi.max(v)])
    }

    fn top(&self) -> Option<f32> {
        if let Some(top) = self.top.get() {
            return Some(*top);
        }
        if self.audio.is_complete() && !self.looking.swap(true, Ordering::AcqRel) {
            let (audio, top) = (self.audio.clone(), self.top.clone());
            let found = std::thread::Builder::new()
                .name("waveform top".into())
                .spawn(move || {
                    let loudest = audio
                        .decoded_chunks()
                        .flatten()
                        .fold(0.0f32, |m, [l, r]| m.max(l.abs()).max(r.abs()));
                    let _ = top.set(loudest);
                });
            if found.is_err() {
                // Try again next frame.
                self.looking.store(false, Ordering::Release);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_louder_channel_and_finds_the_top() {
        let frames = vec![[0.1, -0.5], [0.3, 0.2], [-0.2, 0.25], [0.9, 0.0]];
        let s = DeckSamples::new(Arc::new(Audio::from_frames(44_100, frames)));
        assert_eq!(s.min_max(0, 3), [-0.5, 0.3]);
        assert_eq!(s.ready(), 4);
        let mut top = s.top();
        for _ in 0..200 {
            if top.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            top = s.top();
        }
        assert_eq!(top, Some(0.9));
    }
}
