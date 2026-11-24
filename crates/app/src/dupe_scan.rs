//! Reads the files of Collection Health's copies on a thread of their own:
//! each file's size, format and bitrate, and whether copies of one size
//! are the same file byte for byte. Grouping copies needs both, and doing
//! it on the UI thread froze the app while a big collection's copies were
//! read. Answers are kept by path for the session, so copies grouped again
//! (as fingerprints come in) aren't read again.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use opendj_engine::background::{self, Task};
use opendj_engine::tags;
use opendj_library::dupes::{self, FileFacts};

use crate::cover_scan;

/// How often what came in is applied while files are still being read.
const APPLY_EVERY: Duration = Duration::from_secs(2);

/// Two files, in path order.
type Pair = (PathBuf, PathBuf);

/// What reading one group's files found.
struct Found {
    facts: Vec<(PathBuf, FileFacts)>,
    same: Vec<(Pair, bool)>,
}

fn pair(a: &PathBuf, b: &PathBuf) -> Pair {
    if a <= b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}

/// Reads a group of copies' files.
fn read(paths: &[PathBuf]) -> Found {
    let facts: Vec<(PathBuf, FileFacts)> = paths
        .iter()
        .map(|p| {
            // Files iCloud has taken off this computer aren't read.
            let f = if cover_scan::offloaded(p) {
                FileFacts::default()
            } else {
                let info = tags::file_info(p);
                FileFacts {
                    size: info.size,
                    lossless: dupes::lossless(&info.format, info.bit_depth),
                    bitrate: info.bitrate.unwrap_or(0),
                }
            };
            (p.clone(), f)
        })
        .collect();
    let mut same = Vec::new();
    for (i, (a, fa)) in facts.iter().enumerate() {
        for (b, fb) in &facts[i + 1..] {
            let alike = fa.size > 0 && fa.size == fb.size && dupes::same_bytes(a, b);
            same.push((pair(a, b), alike));
        }
    }
    Found { facts, same }
}

pub struct DupeScan {
    requests: Sender<Vec<PathBuf>>,
    done: Receiver<Found>,
    /// Groups sent and not yet back, by their paths in order.
    asked: RefCell<HashSet<Vec<PathBuf>>>,
    waiting: Cell<usize>,
    facts: HashMap<PathBuf, FileFacts>,
    same: HashMap<Pair, bool>,
    /// Answers taken in since the groups were last worked out again.
    fresh: bool,
    applied: Instant,
}

impl DupeScan {
    pub fn start() -> Self {
        let (requests, rx) = channel::<Vec<PathBuf>>();
        let (tx, done) = channel();
        std::thread::Builder::new()
            .name("dupe-scan".into())
            .spawn(move || {
                background::lower_priority();
                for paths in rx {
                    background::wait_while_held(Task::SoundCheck, || true);
                    if tx.send(read(&paths)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn dupe scan thread");
        Self {
            requests,
            done,
            asked: RefCell::new(HashSet::new()),
            waiting: Cell::new(0),
            facts: HashMap::new(),
            same: HashMap::new(),
            fresh: false,
            applied: Instant::now(),
        }
    }

    /// What a file is like, once read.
    pub fn facts(&self, path: &PathBuf) -> Option<FileFacts> {
        self.facts.get(path).copied()
    }

    /// Whether two files are the same byte for byte, once compared.
    pub fn same(&self, a: &PathBuf, b: &PathBuf) -> Option<bool> {
        self.same.get(&pair(a, b)).copied()
    }

    /// Asks for one group of copies' files to be read, unless every one
    /// is read and compared already, or the group is on its way.
    pub fn ask(&self, mut paths: Vec<PathBuf>) {
        paths.sort();
        paths.dedup();
        let known = paths.iter().all(|p| self.facts.contains_key(p))
            && paths.iter().enumerate().all(|(i, a)| {
                paths[i + 1..]
                    .iter()
                    .all(|b| self.same.contains_key(&pair(a, b)))
            });
        if known || paths.len() < 2 {
            return;
        }
        if self.asked.borrow_mut().insert(paths.clone()) && self.requests.send(paths).is_ok() {
            self.waiting.set(self.waiting.get() + 1);
        }
    }

    /// Whether some groups' files are still being read.
    pub fn busy(&self) -> bool {
        self.waiting.get() > 0
    }

    /// Takes in what was read since last time. True when the copies should
    /// be grouped again: once the last group is back, and every few
    /// seconds before that.
    pub fn take(&mut self) -> bool {
        for found in self.done.try_iter() {
            let paths: Vec<PathBuf> = found.facts.iter().map(|(p, _)| p.clone()).collect();
            self.asked.borrow_mut().remove(&paths);
            self.waiting.set(self.waiting.get().saturating_sub(1));
            self.facts.extend(found.facts);
            self.same.extend(found.same);
            self.fresh = true;
        }
        if !self.fresh || (self.busy() && self.applied.elapsed() < APPLY_EVERY) {
            return false;
        }
        self.fresh = false;
        self.applied = Instant::now();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_group_and_answers_by_path() {
        let dir = std::env::temp_dir().join(format!("opendj-dupe-scan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b, c) = (dir.join("a.mp3"), dir.join("b.mp3"), dir.join("c.mp3"));
        std::fs::write(&a, b"same bytes").unwrap();
        std::fs::write(&b, b"same bytes").unwrap();
        std::fs::write(&c, b"other byte").unwrap();
        let mut scan = DupeScan::start();
        scan.ask(vec![a.clone(), b.clone(), c.clone()]);
        assert!(scan.busy());
        // Asking again while it's on its way sends nothing more.
        scan.ask(vec![c.clone(), b.clone(), a.clone()]);
        let start = Instant::now();
        while !scan.take() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!scan.busy());
        assert_eq!(scan.facts(&a).map(|f| f.size), Some(10));
        assert_eq!(scan.same(&b, &a), Some(true));
        assert_eq!(scan.same(&a, &c), Some(false));
        // Everything is known now: nothing is sent.
        scan.ask(vec![a.clone(), b.clone()]);
        assert!(!scan.busy());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
