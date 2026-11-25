//! The Session in openDJ: a [`Studio`] on a real-time thread of its own,
//! playing into openDJ's `[Session1]` channel through
//! [`opendj_engine::inputs::SessionSource`].
//!
//! openDJ's audio callback must never wait for the Session, however heavy
//! its set. So the Session renders one buffer ahead: each callback takes
//! the block rendered for it and asks for the next one, giving the beat
//! that block will be heard on (the master deck's beat now, plus one
//! buffer) and waking the Session's thread. A block that isn't ready in
//! time is heard as silence on the Session's channel only, and when it
//! does come it is dropped, never played late: the Session stays on the
//! decks' beat.
//!
//! The blocks travel in preallocated buffers over wait-free queues, so
//! neither side allocates or locks.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use opendj_engine::inputs::SessionSource;
use opendj_engine::midi_clock::BeatClock;

use crate::engine::{Lead, Studio};

/// The most frames a buffer carries; a longer one is cut short (the rest is
/// silence), as no sound card plays buffers this long.
pub const MAX_FRAMES: usize = 8192;

/// Blocks in flight: one being heard, one being rendered, and spares for a
/// thread that runs behind.
const BLOCKS: usize = 4;

/// How long the thread sleeps with nothing asked of it before looking at
/// whether it should stop.
const IDLE: Duration = Duration::from_millis(100);

/// What the callback asks the Session's thread for.
#[derive(Clone, Copy, Debug)]
struct Request {
    seq: u64,
    frames: usize,
    /// The beat the block's first frame will be heard on, and the tempo;
    /// None while no deck leads, when the Session keeps its own clock.
    lead: Option<Lead>,
}

/// One rendered buffer.
struct Block {
    seq: u64,
    frames: usize,
    data: Box<[[f32; 2]]>,
}

impl Block {
    fn new() -> Box<Block> {
        Box::new(Block {
            seq: 0,
            frames: 0,
            data: vec![[0.0; 2]; MAX_FRAMES].into_boxed_slice(),
        })
    }
}

/// How the Session keeps up, for the app to show.
#[derive(Default)]
pub struct SessionStats {
    /// Buffers heard as silence because the Session's block wasn't ready.
    late: AtomicU64,
}

impl SessionStats {
    pub fn late(&self) -> u64 {
        self.late.load(Ordering::Relaxed)
    }
}

/// The audio callback's end: what openDJ's `[Session1]` channel plays.
pub struct SessionFeed {
    requests: rtrb::Producer<Request>,
    filled: rtrb::Consumer<Box<Block>>,
    free: rtrb::Producer<Box<Block>>,
    clock: Arc<BeatClock>,
    rate: f64,
    /// The block this buffer should play; 0 before the first request.
    want: u64,
    stats: Arc<SessionStats>,
    /// Wakes the Session's thread; None in tests, which step it by hand.
    wake: Option<std::thread::Thread>,
}

impl SessionFeed {
    /// The beat `frames` from now on the master deck, and its tempo.
    fn next_lead(&self, frames: usize) -> Option<Lead> {
        let now = self.clock.read()?;
        (now.tempo > 0.0).then(|| Lead {
            beat: now.beat + frames as f64 * now.tempo / 60.0 / self.rate,
            tempo: now.tempo,
        })
    }
}

impl SessionSource for SessionFeed {
    fn render(&mut self, out: &mut [[f32; 2]]) {
        out.fill([0.0; 2]);
        let mut heard = false;
        // Blocks for buffers already gone are dropped; the one for this
        // buffer plays.
        while let Ok(seq) = self.filled.peek().map(|b| b.seq) {
            if seq > self.want {
                break;
            }
            let Ok(block) = self.filled.pop() else { break };
            if seq == self.want {
                let n = block.frames.min(out.len());
                out[..n].copy_from_slice(&block.data[..n]);
                heard = true;
            }
            // The queue has room for every block there is.
            let _ = self.free.push(block);
        }
        if self.want > 0 && !heard {
            self.stats.late.fetch_add(1, Ordering::Relaxed);
        }
        let frames = out.len().min(MAX_FRAMES);
        self.want += 1;
        let request = Request {
            seq: self.want,
            frames,
            lead: self.next_lead(frames),
        };
        if self.requests.push(request).is_ok() {
            if let Some(t) = &self.wake {
                t.unpark();
            }
        }
    }
}

/// The Session's thread's end.
struct Worker {
    studio: Studio,
    requests: rtrb::Consumer<Request>,
    filled: rtrb::Producer<Box<Block>>,
    free: rtrb::Consumer<Box<Block>>,
    rate: f64,
}

impl Worker {
    /// Renders what was asked for. When it fell behind and several
    /// requests wait, only the newest is rendered: the others are already
    /// too late to be heard. Returns whether there was anything to do.
    fn step(&mut self) -> bool {
        let mut last = None;
        while let Ok(r) = self.requests.pop() {
            last = Some(r);
        }
        let Some(r) = last else { return false };
        self.render(r);
        true
    }

    fn render(&mut self, r: Request) {
        let Ok(mut block) = self.free.pop() else {
            // Every block is still waiting to be heard: nothing to fill.
            return;
        };
        let started = Instant::now();
        let out = &mut block.data[..r.frames];
        match r.lead {
            Some(lead) => self.studio.process_following(out, lead),
            None => self.studio.process(out),
        }
        let span = r.frames as f64 / self.rate;
        if span > 0.0 {
            let load = started.elapsed().as_secs_f64() / span;
            self.studio.monitor.set_load(load as f32);
        }
        block.seq = r.seq;
        block.frames = r.frames;
        let _ = self.filled.push(block);
    }
}

fn pair(studio: Studio, clock: Arc<BeatClock>) -> (SessionFeed, Worker) {
    let rate = studio.rate();
    let (req_tx, req_rx) = rtrb::RingBuffer::new(BLOCKS * 2);
    let (filled_tx, filled_rx) = rtrb::RingBuffer::new(BLOCKS);
    let (mut free_tx, free_rx) = rtrb::RingBuffer::new(BLOCKS);
    for _ in 0..BLOCKS {
        let _ = free_tx.push(Block::new());
    }
    let feed = SessionFeed {
        requests: req_tx,
        filled: filled_rx,
        free: free_tx,
        clock,
        rate,
        want: 0,
        stats: Arc::default(),
        wake: None,
    };
    let worker = Worker {
        studio,
        requests: req_rx,
        filled: filled_tx,
        free: free_rx,
        rate,
    };
    (feed, worker)
}

/// The Session's running thread. Dropping it stops the thread (and drops
/// the studio there).
pub struct SessionThread {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    stats: Arc<SessionStats>,
}

impl SessionThread {
    pub fn stats(&self) -> Arc<SessionStats> {
        self.stats.clone()
    }
}

impl Drop for SessionThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

/// Starts `studio` on a real-time thread of its own, following the master
/// deck `clock` publishes. The feed goes to
/// `opendj_engine::AudioEngine::set_session`.
pub fn start(
    studio: Studio,
    clock: Arc<BeatClock>,
) -> std::io::Result<(SessionFeed, SessionThread)> {
    let (mut feed, mut worker) = pair(studio, clock);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let rate = worker.rate;
    let thread = std::thread::Builder::new()
        .name("session".into())
        .spawn(move || {
            // Kept for as long as the thread runs; without it the Session
            // still plays, at normal priority.
            let _rt = audio_thread_priority::promote_current_thread_to_real_time(512, rate as u32);
            while !stopping.load(Ordering::Relaxed) {
                if !worker.step() {
                    std::thread::park_timeout(IDLE);
                }
            }
        })?;
    feed.wake = Some(thread.thread().clone());
    let stats = feed.stats.clone();
    Ok((
        feed,
        SessionThread {
            stop,
            thread: Some(thread),
            stats,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{Clip, Note, Sound, Track, SCENES, TRACKS};
    use crate::engine::Command;

    const RATE: f64 = 48_000.0;
    const BLOCK: usize = 256;

    /// One clip on the first track: a held note on every bar.
    fn set() -> Vec<Track> {
        let mut slots = vec![None; SCENES];
        slots[0] = Some(Clip::new(
            "Hit",
            4.0,
            vec![Note {
                start: 0.0,
                length: 1.0,
                pitch: 60,
                velocity: 1.0,
                chance: 1.0,
            }],
        ));
        let mut tracks = vec![Track::new("Lead", Sound::Lead, slots)];
        for _ in 1..TRACKS {
            tracks.push(Track::new("Empty", Sound::Lead, vec![None; SCENES]));
        }
        tracks
    }

    fn first_sound(out: &[[f32; 2]]) -> Option<usize> {
        out.iter().position(|s| s[0] != 0.0 || s[1] != 0.0)
    }

    #[test]
    fn launches_on_the_master_decks_bar() {
        let (studio, mut remote) = Studio::new(&set(), RATE, 120.0);
        let clock = Arc::new(BeatClock::default());
        let (mut feed, mut worker) = pair(studio, clock.clone());
        remote
            .commands
            .push(Command::Launch { track: 0, scene: 0 })
            .ok();
        let tempo = 126.0;
        let per_frame = tempo / 60.0 / RATE;
        let mut beat = 1.5;
        let mut heard = Vec::new();
        let mut out = vec![[0.0; 2]; BLOCK];
        for _ in 0..(4.0 / per_frame) as usize / BLOCK {
            clock.publish(0, beat, tempo);
            feed.render(&mut out);
            heard.extend_from_slice(&out);
            // The thread keeps up.
            worker.step();
            beat += BLOCK as f64 * per_frame;
        }
        // Bar 2 (beat 4) is 2.5 beats after the first buffer.
        let want = (2.5 / per_frame).round() as usize;
        let got = first_sound(&heard).expect("the clip plays");
        assert!(
            got.abs_diff(want) <= 2,
            "first sound at {got}, bar at {want}"
        );
        assert_eq!(feed.stats.late(), 0);
    }

    #[test]
    fn a_late_block_is_silence_and_never_heard_late() {
        let (studio, mut remote) = Studio::new(&set(), RATE, 120.0);
        let clock = Arc::new(BeatClock::default());
        let (mut feed, mut worker) = pair(studio, clock.clone());
        // Playing at once on its own clock, with no deck leading.
        remote
            .commands
            .push(Command::Launch { track: 0, scene: 0 })
            .ok();
        let mut out = vec![[0.0; 2]; BLOCK];
        feed.render(&mut out);
        worker.step();
        feed.render(&mut out);
        assert!(first_sound(&out).is_some(), "the first block plays");
        // The thread misses this buffer: silence.
        feed.render(&mut out);
        assert_eq!(first_sound(&out), None);
        assert_eq!(feed.stats.late(), 1);
        // Two requests wait; only the newest is rendered, and the block
        // for the buffer already gone is not heard now.
        feed.render(&mut out);
        worker.step();
        assert_eq!(feed.stats.late(), 2);
        feed.render(&mut out);
        assert!(first_sound(&out).is_some());
        assert_eq!(feed.stats.late(), 2);
        // The thread takes the next request but finishes after its buffer
        // went by: that block is dropped, not heard a buffer late.
        feed.render(&mut out);
        worker.step();
        feed.render(&mut out);
        let r = worker.requests.pop().expect("a request");
        feed.render(&mut out);
        worker.render(r);
        feed.render(&mut out);
        assert_eq!(first_sound(&out), None);
        assert_eq!(feed.stats.late(), 5);
    }

    #[test]
    fn the_thread_plays_and_stops() {
        let (studio, mut remote) = Studio::new(&set(), RATE, 120.0);
        let clock = Arc::new(BeatClock::default());
        let (mut feed, thread) = start(studio, clock).expect("thread");
        remote
            .commands
            .push(Command::Launch { track: 0, scene: 0 })
            .ok();
        let mut out = vec![[0.0; 2]; BLOCK];
        let mut sounded = false;
        for _ in 0..200 {
            feed.render(&mut out);
            sounded |= first_sound(&out).is_some();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(sounded);
        drop(thread);
    }
}
