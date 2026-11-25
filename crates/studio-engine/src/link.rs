//! Ableton Link for the studio: the session's tempo and beat, read on the
//! audio thread each block, so launch points are bars of the session and
//! line up with openDJ's (both use a quantum of 4).
//!
//! Turning Link on and off opens and closes sockets, so the UI thread does
//! that ([`LinkClock::handle`]); the audio thread only captures and commits
//! session states, which Link makes real-time safe.

use std::sync::Arc;

use rusty_link::{AblLink, HostTimeFilter, SessionState};

use crate::engine::{Span, QUANTUM};

pub struct LinkClock {
    link: Arc<AblLink>,
    state: SessionState,
    filter: HostTimeFilter,
    rate: f64,
    /// Frames rendered: the sample clock the host time filter smooths.
    frames: u64,
    /// When this block's first frame is heard, in Link's microseconds.
    heard_at: i64,
    /// Output latency added to the heard time, in microseconds.
    latency: i64,
}

impl LinkClock {
    pub fn new(tempo: f64, rate: f64) -> LinkClock {
        LinkClock {
            link: Arc::new(AblLink::new(tempo)),
            state: SessionState::new(),
            filter: HostTimeFilter::new(),
            rate,
            frames: 0,
            heard_at: 0,
            latency: 0,
        }
    }

    /// The Link instance, for the UI thread to turn on and off.
    pub fn handle(&self) -> Arc<AblLink> {
        self.link.clone()
    }

    /// Sets the output latency (device buffer and converter), so the beat
    /// read is the one heard.
    pub fn set_latency(&mut self, seconds: f64) {
        self.latency = (seconds * 1e6) as i64;
    }

    /// The session over the next `frames` frames, or None while Link is off.
    pub fn begin(&mut self, frames: usize) -> Option<Span> {
        let now = self.link.clock_micros();
        let host = self.filter.sample_time_to_host_time(now, self.frames);
        self.frames += frames as u64;
        self.heard_at = host + self.latency;
        if !self.link.is_enabled() {
            return None;
        }
        self.link.capture_audio_session_state(&mut self.state);
        let tempo = self.state.tempo();
        let start = self.state.beat_at_time(self.heard_at, QUANTUM);
        let micros = (frames as f64 / self.rate * 1e6) as i64;
        let end = self.state.beat_at_time(self.heard_at + micros, QUANTUM);
        Some(Span { start, end, tempo })
    }

    /// When the last block's first frame is heard, in Link's microseconds.
    pub fn heard_at(&self) -> i64 {
        self.heard_at
    }

    /// Changes the session's tempo from this block on.
    pub fn set_tempo(&mut self, bpm: f64) {
        if !self.link.is_enabled() {
            return;
        }
        self.state.set_tempo(bpm, self.heard_at);
        self.link.commit_audio_session_state(&self.state);
    }
}
