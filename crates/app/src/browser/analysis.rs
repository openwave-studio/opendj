//! Analysing tracks on request: the track menu's Analyze Track and the
//! tree's Analyze All, how the queue is going (a dot on each queued row,
//! with the Background panel for the rest), and keeping the phrases analysis
//! finds in the library.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendj_engine::{phrases, GridMode, Source, Track, NUM_DECKS};
use slint::Model;

use super::{is_browsed, Browser, Contents};
use crate::LibraryEdit;

/// Where the queue's progress was last shown: rows with a dot and their
/// state (1 waiting, 2 being analysed).
#[derive(Default)]
pub(super) struct Shown {
    pub(super) marks: HashMap<usize, i32>,
    /// The queue as it was then (see [`crate::analysis_queue::AnalysisQueue::signature`]).
    signature: (usize, usize, usize),
    /// Tracks analysed and not yet saved and shown.
    waiting: Vec<Track>,
    /// When analysed tracks were last saved and shown.
    applied: Option<Instant>,
    /// Tracks due to be saved and shown, a few each UI tick while a deck
    /// plays (see [`Shown::slice`]).
    pub(super) applying: std::collections::VecDeque<Track>,
    /// The sidebar's counts wait while a deck plays; when they were last
    /// worked out, and whether they're behind.
    pub(super) tree_at: Option<Instant>,
    pub(super) tree_stale: bool,
}

/// How often tracks analysed in the background are saved and shown, at
/// most: each time means work on the UI thread (saving, the rows, the
/// sidebar's counts).
pub(super) const APPLY_EVERY: Duration = Duration::from_secs(2);

/// Tracks saved and shown per UI tick while a deck plays: each costs a
/// database write and its rows, and a whole batch at once ran over a frame.
const PER_TICK_PLAYING: usize = 1;

/// How often the sidebar's counts are worked out again while a deck plays,
/// at most: it goes through the whole library and lays the sidebar out
/// again, which ran over a frame.
pub(super) const TREE_EVERY_PLAYING: Duration = Duration::from_secs(30);

impl Shown {
    /// The tracks to save and show this tick: all that are due, or only a
    /// few while `playing`, the rest waiting for the next ticks.
    pub(super) fn slice(&mut self, due: Vec<Track>, playing: bool) -> Vec<Track> {
        self.applying.extend(due);
        let n = if playing {
            PER_TICK_PLAYING.min(self.applying.len())
        } else {
            self.applying.len()
        };
        self.applying.drain(..n).collect()
    }

    /// Whether the sidebar's counts should be worked out now, after
    /// analysed tracks changed them: at once while no deck plays, else at
    /// most every [`TREE_EVERY_PLAYING`].
    pub(super) fn tree_due(&mut self, changed: bool, playing: bool) -> bool {
        self.tree_stale |= changed;
        let due = self.tree_stale
            && (!playing
                || self
                    .tree_at
                    .is_none_or(|at| at.elapsed() >= TREE_EVERY_PLAYING));
        if due {
            self.tree_stale = false;
            self.tree_at = Some(Instant::now());
        }
        due
    }

    /// Takes in tracks just analysed, and gives back those to save and
    /// show now: all that are waiting, once [`APPLY_EVERY`] has passed
    /// since last time or the queue is `empty`.
    pub(super) fn due(&mut self, finished: Vec<Track>, empty: bool) -> Vec<Track> {
        self.waiting.extend(finished);
        let due = empty || self.applied.is_none_or(|at| at.elapsed() >= APPLY_EVERY);
        if self.waiting.is_empty() || !due {
            return Vec::new();
        }
        self.applied = Some(Instant::now());
        std::mem::take(&mut self.waiting)
    }
}

/// The deck tracks whose phrases were last looked at, by address, so each
/// is looked at once.
pub(super) type SeenDecks = [usize; NUM_DECKS];

impl Browser {
    pub(super) fn install_analysis(
        edit: &LibraryEdit,
        with: &(impl Fn(&dyn Fn(&mut Browser)) + Clone + 'static),
    ) {
        let w = with.clone();
        edit.on_analyze(move |first, last| {
            w(&|b| {
                let picked = b.picked_rows(first, last);
                b.analyze(&picked);
            })
        });
        let w = with.clone();
        edit.on_analyze_all(move |row| {
            w(&|b| {
                let Some(&node) = b.visible_nodes.get(row as usize) else {
                    return;
                };
                // Mixed sets are hours of many tempos and keys: only
                // analysed when asked for one by one.
                let tracks: Vec<usize> = b
                    .node_tracks(node)
                    .into_iter()
                    .filter(|&t| !b.is_mixed_set(&b.library[t]))
                    .collect();
                b.analyze(&tracks);
            })
        });
        let w = with.clone();
        edit.on_prepare_hq_stems_all(move |row| {
            w(&|b| {
                let Some(&node) = b.visible_nodes.get(row as usize) else {
                    return;
                };
                // One queue, in list order; the background tasks panel shows
                // it and can pause, stop or reorder it. Tracks with stems
                // already are skipped, and mixed sets too (hours long, as
                // for Analyze All).
                for t in b.node_tracks(node) {
                    let track = &b.library[t];
                    if matches!(track.source, Source::File(_)) && !b.is_mixed_set(track) {
                        b.engine.prepare_hq_stems(track);
                    }
                }
            })
        });
        let w = with.clone();
        edit.on_set_grid_mode(move |first, last, mode| {
            w(&|b| {
                let Some(mode) = GridMode::from_id(mode as u8) else {
                    return;
                };
                let picked = b.picked_rows(first, last);
                b.set_grid_mode(&picked, mode);
            })
        });
        let w = with.clone();
        edit.on_prepare_hq_stems(move |first, last| {
            w(&|b| {
                for i in b.picked_rows(first, last) {
                    if let Some(t) = b
                        .library
                        .get(i)
                        .filter(|t| matches!(t.source, Source::File(_)))
                    {
                        b.engine.prepare_hq_stems(t);
                    }
                }
            })
        });
    }

    /// Whether a node of the tree holds library tracks to analyse.
    pub(super) fn analysable(contents: &Contents) -> bool {
        !matches!(
            contents,
            Contents::Stream(_)
                | Contents::Store(_)
                | Contents::OtherStores
                | Contents::Wanted
                | Contents::Dir(_)
                | Contents::Health(None)
        )
    }

    /// Gives library tracks (by index) a grid mode and fits their grids
    /// again in it. A track on a deck goes through the deck; locked grids
    /// stay as they are.
    fn set_grid_mode(&mut self, indices: &[usize], mode: GridMode) {
        for &i in indices {
            let Some(t) = self.library.get(i).cloned() else {
                continue;
            };
            if t.info.grid_locked || !matches!(t.source, Source::File(_)) || is_browsed(&t) {
                continue;
            }
            if let Some(d) = self.loaded.iter().position(|l| *l == Some(t.id)) {
                self.deck_grid_mode[d].set(f64::from(mode.id()));
                continue;
            }
            if t.info.grid_mode == Some(mode) {
                continue;
            }
            let mut e = (*t).clone();
            e.info.grid_mode = Some(mode);
            if let (true, Some(db)) = (self.saved(), &self.db) {
                if let Err(err) = db.set_grid_mode(e.id, e.info.grid_mode) {
                    eprintln!("can't save the grid mode: {err}");
                }
            }
            let e = Arc::new(e);
            self.library[i] = e.clone();
            // An edited grid gives way to the analysed one, as Reset does.
            let mut queued = (*e).clone();
            if !matches!(e.info.grid_from.as_str(), "" | "analysis") {
                queued.info.fixed_grid = true;
                queued.info.grid_analysed = true;
            }
            self.analysis.add_refit(&Arc::new(queued));
        }
        self.show_analysis();
    }

    /// Queues library tracks (by index) to be analysed afresh.
    pub(super) fn analyze(&mut self, indices: &[usize]) {
        let tracks: Vec<Arc<Track>> = indices
            .iter()
            .filter_map(|&i| self.library.get(i))
            .filter(|t| matches!(t.source, Source::File(_)) && !is_browsed(t))
            .cloned()
            .collect();
        self.analysis.add_fresh(&tracks);
        self.show_analysis();
    }

    /// Keeps the phrases a background analysis found, and has the track's
    /// row read its new mini waveform.
    pub(super) fn learn_background(&mut self, t: &Track) {
        let Some(&i) = self.index_of.get(&t.id) else {
            return;
        };
        if let Source::File(path) = &t.source {
            self.waves.forget(path);
            if let Some(row) = self.rows.iter().position(|r| *r == Some(i)) {
                self.wave_rows.retain(|&w| w != row);
                self.fill_waves();
            }
        }
        if !phrases::imported(t) {
            self.keep_phrases(i, phrases::to_markers(&phrases::stored(t), t.duration));
        }
    }

    /// Finds the phrases of each deck's track once its waveform is ready,
    /// and again whenever its grid changes, and keeps them when they
    /// differ from the library's.
    pub fn learn_deck_phrases(&mut self, loaded: &[Option<Arc<Track>>]) {
        for (d, t) in loaded.iter().enumerate().take(NUM_DECKS) {
            let Some(t) = t else {
                self.phrases_seen[d] = 0;
                continue;
            };
            let at = Arc::as_ptr(t) as usize;
            if self.phrases_seen[d] == at || !phrases::waveform_ready(t) {
                continue;
            }
            self.phrases_seen[d] = at;
            let Some(&i) = self.index_of.get(&t.id) else {
                continue;
            };
            if phrases::imported(&self.library[i]) {
                continue;
            }
            self.keep_phrases(i, phrases::to_markers(&phrases::detect(t), t.duration));
        }
    }

    /// Gives library track `i` these detected phrases, saving them, unless
    /// it has them already.
    fn keep_phrases(&mut self, i: usize, found: Vec<opendj_engine::Marker>) {
        let entry = &self.library[i];
        let had: Vec<_> = entry
            .info
            .markers
            .iter()
            .filter(|m| m.kind == opendj_engine::MarkerKind::Phrase && m.detected)
            .cloned()
            .collect();
        if had == found {
            return;
        }
        if let (true, Some(db)) = (self.saved(), &self.db) {
            if let Err(e) = db.save_phrases(entry.id, &found) {
                eprintln!("can't save the phrases: {e}");
            }
        }
        let mut e = (**entry).clone();
        e.info.markers = phrases::with_phrases(&e.info.markers, found);
        self.library[i] = Arc::new(e);
    }

    /// Puts the queue's progress on the rows, when it has changed (the
    /// Background panel shows the rest; see `browser/tasks.rs`).
    pub(super) fn show_analysis(&mut self) {
        let signature = self.analysis.signature();
        if signature == self.analysis_shown.signature {
            return;
        }
        self.analysis_shown.signature = signature;
        self.mark_analysis_rows();
    }

    /// Sets each row's dot from the queue, touching only rows that change.
    pub(super) fn mark_analysis_rows(&mut self) {
        let mut marks = HashMap::new();
        for (row, i) in self.rows.iter().enumerate() {
            let Some(t) = i.and_then(|i| self.library.get(i)) else {
                continue;
            };
            let state = self.analysis.state(t.id);
            if state != 0 {
                marks.insert(row, state);
            }
        }
        let old = std::mem::take(&mut self.analysis_shown.marks);
        let changed = old
            .iter()
            .filter(|(row, _)| !marks.contains_key(row))
            .map(|(&row, _)| (row, 0))
            .chain(
                marks
                    .iter()
                    .filter(|(row, s)| old.get(row) != Some(s))
                    .map(|(&row, &s)| (row, s)),
            )
            .collect::<Vec<_>>();
        for (row, state) in changed {
            if let Some(mut r) = self.list.row_data(row) {
                if r.analysing != state {
                    r.analysing = state;
                    self.list.set_row_data(row, r);
                }
            }
        }
        self.analysis_shown.marks = marks;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysed_tracks_are_shown_in_batches() {
        let mut shown = Shown::default();
        let track = |id| Track {
            id,
            artist: String::new(),
            title: String::new(),
            album: String::new(),
            genre: String::new(),
            rating: 0,
            bpm: 0.0,
            key: String::new(),
            duration: 0.0,
            first_beat: 0.0,
            source: Source::Synth { seed: 1 },
            waveform: Default::default(),
            audio: None,
            cover: None,
            grid: Default::default(),
            info: Default::default(),
        };
        // The first is shown at once.
        assert_eq!(shown.due(vec![track(1)], false).len(), 1);
        // The next ones wait while the queue runs...
        assert!(shown.due(vec![track(2)], false).is_empty());
        assert!(shown.due(vec![track(3)], false).is_empty());
        assert!(shown.due(Vec::new(), false).is_empty());
        // ...and come together once it's done.
        let ids: Vec<usize> = shown.due(Vec::new(), true).iter().map(|t| t.id).collect();
        assert_eq!(ids, [2, 3]);
        assert!(shown.due(Vec::new(), true).is_empty());
    }

    #[test]
    fn analysed_tracks_trickle_in_while_a_deck_plays() {
        let mut shown = Shown::default();
        let track = |id| Track {
            id,
            artist: String::new(),
            title: String::new(),
            album: String::new(),
            genre: String::new(),
            rating: 0,
            bpm: 0.0,
            key: String::new(),
            duration: 0.0,
            first_beat: 0.0,
            source: Source::Synth { seed: 1 },
            waveform: Default::default(),
            audio: None,
            cover: None,
            grid: Default::default(),
            info: Default::default(),
        };
        // Playing: one per tick, the rest wait their turn.
        let ids = |v: Vec<Track>| v.iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(
            ids(shown.slice(vec![track(1), track(2), track(3)], true)),
            [1]
        );
        assert_eq!(ids(shown.slice(Vec::new(), true)), [2]);
        // Stopped: all that are left at once.
        assert_eq!(ids(shown.slice(vec![track(4)], false)), [3, 4]);
        assert!(shown.slice(Vec::new(), true).is_empty());
    }

    #[test]
    fn the_sidebar_counts_wait_while_a_deck_plays() {
        let mut shown = Shown::default();
        // Stopped: at once, and only after something changed.
        assert!(!shown.tree_due(false, false));
        assert!(shown.tree_due(true, false));
        assert!(!shown.tree_due(false, false));
        // Playing: not again within TREE_EVERY_PLAYING...
        assert!(!shown.tree_due(true, true));
        assert!(!shown.tree_due(false, true));
        // ...but as soon as the decks stop.
        assert!(shown.tree_due(false, false));
        // Long enough since the last time: while playing too.
        shown.tree_at = Some(Instant::now() - TREE_EVERY_PLAYING);
        assert!(shown.tree_due(true, true));
    }
}
