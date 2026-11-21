//! The preview player, `[PreviewDeck1]` as in Mixxx: listens to a track in
//! the headphones without loading it on a deck. It plays into the cue
//! (headphone) mix only, so the audience never hears it.
//!
//! Controls: `play`, `playposition` (0 to 1; set it to seek), `duration`,
//! `track_loaded` and `volume`. `main` on also plays it in the main mix,
//! for listening back to a recorded set in the Mixed Set Editor.
//!
//! The audio arrives over a wait-free queue, and audio it is done with goes
//! back over another to be freed off the audio thread, as deck tracks do.
//! A long recording can instead stream in (see [`Stream`]): frames decoded
//! a few seconds ahead of the playhead, so it is never held whole.

use std::sync::Arc;

use opendj_control::{Behavior, ControlHandle, ControlRegistry};

use crate::track::Audio;

pub const PREVIEW_GROUP: &str = "[PreviewDeck1]";

/// A file streamed into the player from a point in it: its frames as they
/// are decoded, at the file's rate.
pub(crate) struct Stream {
    pub(crate) frames: rtrb::Consumer<[f32; 2]>,
    /// The frame of the file the stream starts at.
    pub(crate) start: usize,
    /// Frames in the whole file.
    pub(crate) total: usize,
    pub(crate) rate: u32,
}

/// What the player holds.
pub(crate) enum Loaded {
    Audio(Arc<Audio>),
    Stream(Stream),
}

impl Loaded {
    /// Frames in the whole file.
    fn len(&self) -> f64 {
        match self {
            Loaded::Audio(a) => a.len() as f64,
            Loaded::Stream(s) => s.total as f64,
        }
    }

    fn rate(&self) -> u32 {
        match self {
            Loaded::Audio(a) => a.sample_rate,
            Loaded::Stream(s) => s.rate,
        }
    }

    fn seconds(&self) -> f64 {
        match self {
            Loaded::Audio(a) => a.duration(),
            Loaded::Stream(s) => s.total as f64 / s.rate.max(1) as f64,
        }
    }
}

/// The other side of the player's queues, for [`crate::AudioEngine::preview`].
pub(crate) struct PreviewInput {
    audio: rtrb::Producer<(Option<Loaded>, bool)>,
    done: rtrb::Consumer<Loaded>,
}

impl PreviewInput {
    /// Puts `audio` in the player at its start, playing if `play`, or
    /// empties it. Frees audio the player has finished with.
    pub(crate) fn send(&mut self, audio: Option<Arc<Audio>>, play: bool) {
        self.load(audio.map(Loaded::Audio), play);
    }

    /// Puts a stream in the player at its start, playing if `play`.
    pub(crate) fn send_stream(&mut self, stream: Stream, play: bool) {
        self.load(Some(Loaded::Stream(stream)), play);
    }

    fn load(&mut self, loaded: Option<Loaded>, play: bool) {
        while self.done.pop().is_ok() {}
        let _ = self.audio.push((loaded, play));
    }
}

pub(crate) struct Preview {
    audio: Option<Loaded>,
    /// Play position in source frames.
    pos: f64,
    /// Source frames per output frame.
    step: f64,
    rate: f64,
    /// A stream's frames either side of the play position.
    around: [[f32; 2]; 2],
    play: ControlHandle,
    position: ControlHandle,
    duration: ControlHandle,
    loaded: ControlHandle,
    volume: ControlHandle,
    main: ControlHandle,
    /// The position this player last published, so a different one is a seek.
    written: f64,
    gain: f32,
    incoming: rtrb::Consumer<(Option<Loaded>, bool)>,
    done: rtrb::Producer<Loaded>,
}

impl Preview {
    pub(crate) fn new(reg: &ControlRegistry, sample_rate: f64) -> (Self, PreviewInput) {
        let g = PREVIEW_GROUP;
        let (audio_tx, incoming) = rtrb::RingBuffer::new(8);
        let (done, done_rx) = rtrb::RingBuffer::new(16);
        (
            Preview {
                audio: None,
                pos: 0.0,
                step: 1.0,
                rate: sample_rate,
                around: [[0.0; 2]; 2],
                play: reg.toggle(g, "play"),
                position: reg.create(g, "playposition", 0.0, 0.0, 1.0, Behavior::Linear),
                duration: reg.indicator(g, "duration", 86_400.0),
                loaded: reg.indicator(g, "track_loaded", 1.0),
                volume: reg.create(g, "volume", 1.0, 0.0, 1.0, Behavior::Linear),
                main: reg.toggle(g, "main"),
                written: 0.0,
                gain: 0.0,
                incoming,
                done,
            },
            PreviewInput {
                audio: audio_tx,
                done: done_rx,
            },
        )
    }

    /// Whether it plays in the main mix too.
    pub(crate) fn on_main(&self) -> bool {
        self.main.get_bool()
    }

    fn len(&self) -> f64 {
        self.audio.as_ref().map_or(0.0, Loaded::len)
    }

    /// Takes new audio and seeks; once per buffer, before [`Preview::next`].
    pub(crate) fn update(&mut self) {
        while let Ok((audio, play)) = self.incoming.pop() {
            if let Some(old) = std::mem::replace(&mut self.audio, audio) {
                // If the queue is full it is freed here; rare and harmless.
                let _ = self.done.push(old);
            }
            self.pos = match &self.audio {
                Some(Loaded::Stream(s)) => s.start as f64,
                _ => 0.0,
            };
            self.around = [[0.0; 2]; 2];
            self.written = self.pos / self.len().max(1.0);
            self.position.set(self.written);
            self.play.set_bool(play && self.audio.is_some());
            self.step = self
                .audio
                .as_ref()
                .map_or(1.0, |a| a.rate() as f64 / self.rate);
        }
        self.loaded.set_bool(self.audio.is_some());
        self.duration
            .set(self.audio.as_ref().map_or(0.0, Loaded::seconds));
        let wanted = self.position.get();
        if (wanted - self.written).abs() > 1e-9 {
            // A stream seeks by being sent again from the new place.
            if let Some(Loaded::Audio(_)) = self.audio {
                self.pos = wanted.clamp(0.0, 1.0) * self.len();
            }
            self.written = wanted;
        }
        if self.audio.is_none() {
            self.play.set_bool(false);
        }
    }

    /// The next frame of the preview, for the cue mix.
    #[inline]
    pub(crate) fn next(&mut self) -> [f32; 2] {
        let playing = self.play.get_bool();
        // A short ramp, so starting and stopping don't click.
        let target = if playing {
            self.volume.get() as f32
        } else {
            0.0
        };
        self.gain += (target - self.gain) * 0.002;
        if self.gain < 1e-4 && !playing {
            return [0.0; 2];
        }
        let g = self.gain;
        let (a, b, t) = match &mut self.audio {
            None => return [0.0; 2],
            Some(Loaded::Audio(audio)) => {
                let i = self.pos as usize;
                let t = (self.pos - i as f64) as f32;
                let (a, b) = (audio.frame(i), audio.frame(i + 1));
                if playing {
                    self.pos += self.step;
                }
                (a, b, t)
            }
            Some(Loaded::Stream(s)) => {
                let t = self.pos.fract() as f32;
                let [a, b] = self.around;
                if playing {
                    // Waits on the decoder rather than skip ahead.
                    let next = self.pos + self.step;
                    let mut whole = next.floor() - self.pos.floor();
                    while whole >= 1.0 {
                        match s.frames.pop() {
                            Ok(f) => self.around = [self.around[1], f],
                            Err(_) => return [0.0; 2],
                        }
                        whole -= 1.0;
                        self.pos = self.pos.floor() + 1.0;
                    }
                    self.pos = next;
                }
                (a, b, t)
            }
        };
        [
            (a[0] + (b[0] - a[0]) * t) * g,
            (a[1] + (b[1] - a[1]) * t) * g,
        ]
    }

    /// Publishes where it is, and stops at the end; once per buffer.
    pub(crate) fn publish(&mut self) {
        let len = self.len();
        if len <= 0.0 {
            return;
        }
        if self.pos >= len {
            self.pos = 0.0;
            self.play.set_bool(false);
        }
        self.written = self.pos / len;
        self.position.set(self.written);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_from_where_it_is_set_and_stops_at_the_end() {
        let reg = ControlRegistry::new();
        let (mut p, mut input) = Preview::new(&reg, 100.0);
        let frames: Vec<[f32; 2]> = (0..100).map(|i| [i as f32 / 100.0; 2]).collect();
        input.send(Some(Arc::new(Audio::from_frames(100, frames))), false);
        p.update();
        assert!(reg.get(PREVIEW_GROUP, "track_loaded").unwrap().get_bool());
        assert_eq!(reg.get(PREVIEW_GROUP, "duration").unwrap().get(), 1.0);
        // Stopped, it is silent.
        assert_eq!(p.next(), [0.0; 2]);

        reg.get(PREVIEW_GROUP, "playposition").unwrap().set(0.5);
        reg.get(PREVIEW_GROUP, "play").unwrap().set(1.0);
        p.update();
        let mut last = [0.0; 2];
        for _ in 0..40 {
            last = p.next();
        }
        p.publish();
        assert!(last[0] > 0.0);
        let at = reg.get(PREVIEW_GROUP, "playposition").unwrap().get();
        assert!((at - 0.9).abs() < 1e-6, "{at}");

        for _ in 0..20 {
            p.next();
        }
        p.publish();
        assert!(!reg.get(PREVIEW_GROUP, "play").unwrap().get_bool());

        input.send(None, true);
        p.update();
        assert!(!reg.get(PREVIEW_GROUP, "track_loaded").unwrap().get_bool());
    }

    #[test]
    fn streams_from_its_start_and_waits_for_the_decoder() {
        let reg = ControlRegistry::new();
        let (mut p, mut input) = Preview::new(&reg, 100.0);
        let (mut tx, frames) = rtrb::RingBuffer::new(64);
        for i in 0..10 {
            tx.push([i as f32; 2]).unwrap();
        }
        let stream = Stream {
            frames,
            start: 50,
            total: 200,
            rate: 100,
        };
        input.send_stream(stream, true);
        p.update();
        assert_eq!(reg.get(PREVIEW_GROUP, "duration").unwrap().get(), 2.0);
        assert_eq!(reg.get(PREVIEW_GROUP, "playposition").unwrap().get(), 0.25);
        // Let the start ramp settle, then it plays the frames in order.
        let mut heard = Vec::new();
        for _ in 0..10 {
            heard.push(p.next()[0]);
        }
        assert!(heard.windows(2).all(|w| w[1] >= w[0]), "{heard:?}");
        p.publish();
        let at = reg.get(PREVIEW_GROUP, "playposition").unwrap().get();
        assert!((at - 0.3).abs() < 1e-6, "{at}");
        // Out of frames it holds its place in silence.
        assert_eq!(p.next(), [0.0; 2]);
        p.publish();
        let held = reg.get(PREVIEW_GROUP, "playposition").unwrap().get();
        assert!((held - 0.3).abs() < 0.006, "{held}");
    }
}
