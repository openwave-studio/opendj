//! File › Export › Stem Files…: an NI STEM file (`.stem.mp4`) for each
//! track of the list picked in the sidebar that has high-quality stems,
//! written on a thread of its own into a folder the DJ picks (see
//! [`opendj_engine::stem_file`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use opendj_engine::{stem_file, stems, Source};

use super::Browser;
use crate::LibraryEdit;

/// A track to write: its file and tags.
struct Job {
    path: PathBuf,
    tags: stem_file::Tags,
}

enum Message {
    /// How many are done, of how many.
    Progress(usize, usize),
    /// The files written, and the errors.
    Done(Vec<PathBuf>, Vec<String>),
}

pub(super) struct State {
    picked_tx: Sender<(u64, PathBuf)>,
    picked_rx: Receiver<(u64, PathBuf)>,
    /// What the folder dialog open now is for: its number, the tracks, and
    /// how many tracks of the list have no high-quality stems yet.
    waiting: Option<(u64, Vec<Job>, usize)>,
    next: u64,
    /// The export running, and how many tracks it skipped.
    running: Option<(Receiver<Message>, usize)>,
}

impl Default for State {
    fn default() -> Self {
        let (picked_tx, picked_rx) = mpsc::channel();
        Self {
            picked_tx,
            picked_rx,
            waiting: None,
            next: 0,
            running: None,
        }
    }
}

impl Browser {
    pub(super) fn install_stem_files(
        edit: &LibraryEdit,
        with: &(impl Fn(&dyn Fn(&mut Browser)) + Clone + 'static),
    ) {
        let w = with.clone();
        edit.on_export_stem_files(move |row| w(&|b| b.export_stem_files(row as usize)));
    }

    /// Asks where to save the stem files of the tree's row `row`.
    fn export_stem_files(&mut self, row: usize) {
        let Some(&node) = self.visible_nodes.get(row) else {
            return;
        };
        if self.stem_files.running.is_some() {
            self.engine
                .alert("Still saving stem files from the last export.".into());
            return;
        }
        let mut jobs = Vec::new();
        let mut missing = 0;
        for t in self.node_tracks(node) {
            let track = &self.library[t];
            let Source::File(path) = &track.source else {
                continue;
            };
            match stems::find(path) {
                Some(stems::StemSource::Files(_)) => jobs.push(Job {
                    path: path.clone(),
                    tags: stem_file::Tags {
                        title: track.title.clone(),
                        artist: track.artist.clone(),
                        album: track.album.clone(),
                    },
                }),
                // Already a stem file.
                Some(stems::StemSource::NiStem(_)) => {}
                None => missing += 1,
            }
        }
        if jobs.is_empty() {
            self.engine
                .alert("No tracks here have HQ stems yet. Prepare HQ Stems for All first.".into());
            return;
        }
        self.stem_files.next += 1;
        let ticket = self.stem_files.next;
        self.stem_files.waiting = Some((ticket, jobs, missing));
        let tx = self.stem_files.picked_tx.clone();
        let prompt = format!(
            "Choose where to save the stem files of \"{}\"",
            self.node_title(node)
        );
        crate::prefs::choose_folder(&prompt, move |dir| {
            let _ = tx.send((ticket, dir));
        });
    }

    /// Starts an export once its folder is picked, and tells how it goes.
    pub fn poll_stem_files(&mut self) {
        while let Ok((ticket, dir)) = self.stem_files.picked_rx.try_recv() {
            let Some((t, jobs, missing)) = self.stem_files.waiting.take() else {
                continue;
            };
            if t != ticket {
                self.stem_files.waiting = Some((t, jobs, missing));
                continue;
            }
            let (tx, rx) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("stem-files".into())
                .spawn(move || write_all(&dir, jobs, &tx));
            match spawned {
                Ok(_) => self.stem_files.running = Some((rx, missing)),
                Err(e) => self.engine.alert(format!("Couldn't save stem files: {e}")),
            }
        }
        let Some((rx, missing)) = &self.stem_files.running else {
            return;
        };
        let missing = *missing;
        let mut finished = None;
        let mut progress = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                Message::Progress(done, of) => progress = Some((done, of)),
                Message::Done(files, errors) => finished = Some((files, errors)),
            }
        }
        if let Some((done, of)) = progress.filter(|_| finished.is_none()) {
            self.engine
                .alert(format!("Saving stem files… {done} of {of}"));
        }
        let Some((files, errors)) = finished else {
            return;
        };
        self.stem_files.running = None;
        if let Some(first) = files.first() {
            crate::files::reveal(first);
        }
        let mut note = match files.len() {
            1 => "Saved 1 stem file.".to_string(),
            n => format!("Saved {n} stem files."),
        };
        if missing > 0 {
            note.push_str(&match missing {
                1 => " 1 track has no HQ stems yet.".to_string(),
                n => format!(" {n} tracks have no HQ stems yet."),
            });
        }
        if let Some(e) = errors.first() {
            eprintln!("stem files: {}", errors.join("; "));
            note.push_str(&format!(" Couldn't save {}: {e}", errors.len()));
        }
        self.engine.alert(note);
    }
}

/// Writes each job's stem file into `dir`, telling `tx` how it goes.
fn write_all(dir: &Path, jobs: Vec<Job>, tx: &Sender<Message>) {
    let mut written = Vec::new();
    let mut errors = Vec::new();
    let mut names = HashSet::new();
    let of = jobs.len();
    for (i, job) in jobs.into_iter().enumerate() {
        let _ = tx.send(Message::Progress(i, of));
        let to = dir.join(unique(&stem_file::file_name(&job.path), &mut names));
        match stem_file::write(&job.path, &to, &job.tags, &|| true) {
            Ok(()) => written.push(to),
            Err(e) => errors.push(format!("{}: {e}", job.path.display())),
        }
    }
    let _ = tx.send(Message::Done(written, errors));
}

/// `name`, or with a number before `.stem.mp4` if this export already
/// used it (two tracks of the same file name in different folders).
fn unique(name: &str, used: &mut HashSet<String>) -> String {
    let base = name.trim_end_matches(".stem.mp4");
    let mut n = 1;
    let mut candidate = name.to_string();
    while !used.insert(candidate.to_lowercase()) {
        n += 1;
        candidate = format!("{base} {n}.stem.mp4");
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_names_get_a_number() {
        let mut used = HashSet::new();
        assert_eq!(unique("Song.stem.mp4", &mut used), "Song.stem.mp4");
        assert_eq!(unique("Song.stem.mp4", &mut used), "Song 2.stem.mp4");
        assert_eq!(unique("song.stem.mp4", &mut used), "song 3.stem.mp4");
    }
}
