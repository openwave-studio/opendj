//! Makes the collection's audio fingerprints on a thread of its own, one
//! file at a time, so Collection Health finds copies by their sound. Each
//! is kept in the library with the file's modification time, so a file is
//! only listened to again once it changes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use opendj_engine::background::{self, Task};

use crate::cover_scan::mtime;
use crate::lookup;

/// A fingerprint made: the track's id, its file's modification time and
/// the fingerprint (empty when the file couldn't be decoded).
pub type Sound = (usize, i64, Vec<u32>);

pub struct SoundScan {
    requests: Sender<(usize, PathBuf)>,
    done: Receiver<Sound>,
    /// Ids sent and not yet back.
    queued: HashSet<usize>,
    /// Files sent, and those back, since the queue was last empty.
    total: usize,
    finished: usize,
}

impl SoundScan {
    pub fn start() -> Self {
        let (requests, rx) = channel::<(usize, PathBuf)>();
        let (tx, done) = channel();
        std::thread::Builder::new()
            .name("sound-scan".into())
            .spawn(move || {
                for (id, path) in rx {
                    background::wait_while_held(Task::SoundCheck, || true);
                    let print = lookup::raw_fingerprint(&path)
                        .map(|(p, _)| p)
                        .unwrap_or_default();
                    if tx.send((id, mtime(&path), print)).is_err() {
                        break;
                    }
                    // Leaves the computer to the decks between files.
                    std::thread::sleep(Duration::from_millis(100));
                }
            })
            .expect("spawn sound scan thread");
        Self {
            requests,
            done,
            queued: HashSet::new(),
            total: 0,
            finished: 0,
        }
    }

    /// Queues a file to fingerprint, unless it is queued already.
    pub fn add(&mut self, id: usize, path: &Path) {
        if self.queued.insert(id) {
            self.total += 1;
            let _ = self.requests.send((id, path.to_path_buf()));
        }
    }

    /// Whether files are still waiting.
    pub fn busy(&self) -> bool {
        !self.queued.is_empty()
    }

    /// Files done and how many in all, while some are waiting.
    pub fn progress(&self) -> Option<(usize, usize)> {
        (!self.queued.is_empty()).then_some((self.finished, self.total))
    }

    /// Fingerprints made since the last call.
    pub fn take(&mut self) -> Vec<Sound> {
        let done: Vec<_> = self.done.try_iter().collect();
        for (id, ..) in &done {
            if self.queued.remove(id) {
                self.finished += 1;
            }
        }
        if self.queued.is_empty() {
            self.total = 0;
            self.finished = 0;
        }
        done
    }
}
