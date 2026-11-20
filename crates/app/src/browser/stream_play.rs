//! Streamed tracks fetched to play: a TIDAL track (in TIDAL, or standing in
//! a list brought in from another app) double-clicked, dragged onto a deck
//! or picked from the catalog search downloads through the add-on that
//! plays the service's tracks. The deck shows its title and artist and how
//! the download goes over its waveform, then loads it. The file joins the
//! collection in the Inbox and takes the streamed track's place in the
//! lists it stood in.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use slint::Model;

use opendj_engine::scan::read_track;

use super::{open_in_browser, Browser, NUM_DECKS};
use crate::sources::{FetchState, PlayUpdate, StreamPlay};
use crate::spotify::StreamTrack;

/// How long a failed fetch stays on its deck.
const FAILED_SHOWN: Duration = Duration::from_secs(8);

/// How each streamed track downloading is going, by link, for its rows:
/// how far it got, whether it failed, and when a failed one goes. `shown`
/// says the rows show some, so they are cleared once none are left.
#[derive(Default)]
pub struct RowFetches {
    by_link: HashMap<String, (f32, bool, Option<Instant>)>,
    shown: bool,
}

/// A streamed track downloading to load onto a deck, as the deck shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct DeckFetch {
    pub title: String,
    pub artist: String,
    /// "Downloading 42%", or why it couldn't.
    pub note: String,
    /// From 0 to 1.
    pub progress: f32,
    pub failed: bool,
    link: String,
    /// When a failed one goes.
    until: Option<Instant>,
}

impl Browser {
    /// The streamed track at a list row, with its service ("tidal").
    pub(super) fn stream_row(&self, row: usize) -> Option<(StreamTrack, String)> {
        if self.placed.entries.is_some() {
            let (service, t) = self.placed_at(row)?;
            return Some((t.clone(), service.clone()));
        }
        let t = self.stream_track(row)?.clone();
        let service = self.shown_stream()?.id.to_string();
        Some((t, service))
    }

    fn stream_play(t: &StreamTrack, service: &str, deck: Option<usize>) -> StreamPlay {
        StreamPlay {
            link: t.page.clone(),
            title: t.title.clone(),
            artist: t.artist(),
            service: service.to_string(),
            stream_id: t.id.clone(),
            deck,
        }
    }

    /// Plays a streamed track: on `deck`, else a deck that isn't playing,
    /// through an add-on that plays its service's tracks; else on its page.
    pub(super) fn play_stream(&mut self, t: &StreamTrack, service: &str, deck: Option<usize>) {
        if !self.plays_link(&t.page) {
            if !t.page.is_empty() {
                open_in_browser(&t.page);
            }
            return;
        }
        let deck = deck
            .filter(|&d| d < NUM_DECKS)
            .or_else(|| (0..self.decks_shown).find(|&d| !self.play[d].get_bool()));
        if let Some(d) = deck {
            self.fetches[d] = Some(DeckFetch {
                title: t.title.clone(),
                artist: t.artist(),
                note: "Finding it…".into(),
                progress: 0.0,
                failed: false,
                link: t.page.clone(),
                until: None,
            });
        }
        self.row_fetches
            .by_link
            .insert(t.page.clone(), (0.0, false, None));
        self.stream_plays.push(Self::stream_play(t, service, deck));
    }

    /// Downloads a streamed track into the collection, without loading it:
    /// its rows show how it goes, and it is analysed once in.
    pub(super) fn download_stream(&mut self, t: &StreamTrack, service: &str) {
        if self.plays_link(&t.page) && !self.row_fetches.by_link.contains_key(&t.page) {
            self.row_fetches
                .by_link
                .insert(t.page.clone(), (0.0, false, None));
            self.stream_plays.push(Self::stream_play(t, service, None));
        }
    }

    /// Download on a streamed row's menu: downloads the row, or every row
    /// picked, without loading any onto a deck.
    pub(super) fn fetch_rows(&mut self, first: usize, last: usize) {
        let rows = self.chosen_rows(first.min(last), first.max(last));
        let picked: Vec<(StreamTrack, String)> = rows
            .into_iter()
            .filter(|&row| self.rows.get(row).is_some_and(Option::is_none))
            .filter_map(|row| self.stream_row(row))
            .collect();
        for (t, service) in picked {
            self.download_stream(&t, &service);
        }
    }

    /// Whether a streamed row can be fetched through an add-on.
    pub(super) fn fetchable_row(&self, row: usize) -> bool {
        self.stream_row(row)
            .is_some_and(|(t, _)| self.plays_link(&t.page))
    }

    /// Streamed tracks to fetch, for the frame loop to hand to the add-on.
    pub fn take_stream_plays(&mut self) -> Vec<StreamPlay> {
        std::mem::take(&mut self.stream_plays)
    }

    /// How a fetch is going: shown on its deck, and once the file is in, it
    /// takes the streamed track's place in its lists and loads.
    pub fn stream_update(&mut self, u: PlayUpdate) {
        let deck = u.play.deck.filter(|&d| {
            self.fetches[d]
                .as_ref()
                .is_some_and(|f| f.link == u.play.link)
        });
        match &u.state {
            FetchState::Fetching(_, p) => {
                if let Some(f) = self.row_fetches.by_link.get_mut(&u.play.link) {
                    f.0 = *p;
                }
            }
            FetchState::Failed(_) => {
                self.row_fetches.by_link.insert(
                    u.play.link.clone(),
                    (0.0, true, Some(Instant::now() + FAILED_SHOWN)),
                );
            }
            FetchState::Done(_) => {
                self.row_fetches.by_link.remove(&u.play.link);
            }
        }
        match u.state {
            FetchState::Fetching(note, progress) => {
                if let Some(f) = deck.and_then(|d| self.fetches[d].as_mut()) {
                    f.note = note;
                    f.progress = progress;
                }
            }
            FetchState::Failed(why) => {
                if let Some(f) = deck.and_then(|d| self.fetches[d].as_mut()) {
                    f.note = format!("Couldn't download: {why}");
                    f.failed = true;
                    f.until = Some(Instant::now() + FAILED_SHOWN);
                }
            }
            FetchState::Done(path) => {
                if let Some(d) = deck {
                    self.fetches[d] = None;
                }
                let index = match self.path_index.get(&path) {
                    Some(&i) => Some(i),
                    None => self.add_browsed(read_track(0, &path)),
                };
                let Some(i) = index else { return };
                self.fill_placeholders(&u.play, i);
                // Onto its deck, unless that started playing meanwhile.
                let deck = deck.filter(|&d| !self.play[d].get_bool()).or_else(|| {
                    deck.and_then(|_| (0..self.decks_shown).find(|&d| !self.play[d].get_bool()))
                });
                match deck {
                    Some(d) => {
                        if self.playable(i) {
                            self.engine.load_track(d, self.library[i].clone());
                        }
                    }
                    // Downloaded without a deck to go to: analysed now,
                    // ready to play.
                    None if u.play.deck.is_none() => self.analyze(&[i]),
                    None => {}
                }
            }
        }
    }

    /// The downloaded track (an index into `library`) takes the streamed
    /// track's place in every list it stood in.
    fn fill_placeholders(&mut self, play: &StreamPlay, i: usize) {
        if play.stream_id.is_empty() || super::is_browsed(&self.library[i]) {
            return;
        }
        let id = self.library[i].id;
        let Some(db) = self.db.as_mut() else { return };
        match db.fill_streamed(&play.service, &play.stream_id, id) {
            Ok(lists) if !lists.is_empty() => {
                if self.placed.entries.is_some() {
                    self.load_placed();
                    self.refresh_rows();
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("can't put the download in its lists: {e}"),
        }
    }

    /// Shows each downloading streamed track's progress on its rows (in
    /// the mini waveform's place). Called every frame; does nothing once
    /// no row shows one.
    pub fn show_row_fetches(&mut self) {
        let now = Instant::now();
        self.row_fetches
            .by_link
            .retain(|_, f| f.2.is_none_or(|t| t > now));
        if self.row_fetches.by_link.is_empty() && !self.row_fetches.shown {
            return;
        }
        self.row_fetches.shown = !self.row_fetches.by_link.is_empty();
        for row in 0..self.list.row_count() {
            let want = self
                .stream_track(row)
                .and_then(|t| self.row_fetches.by_link.get(&t.page))
                .copied();
            let Some(mut r) = self.list.row_data(row) else {
                continue;
            };
            let (fetching, fetch, failed) = match want {
                Some((p, failed, _)) => (true, p, failed),
                None => (false, 0.0, false),
            };
            if r.fetching != fetching || r.fetch != fetch || r.fetch_failed != failed {
                r.fetching = fetching;
                r.fetch = fetch;
                r.fetch_failed = failed;
                self.list.set_row_data(row, r);
            }
        }
    }

    /// What each deck is fetching, for the deck's title, artist and
    /// waveform; failed ones go after a while.
    pub fn deck_fetches(&mut self) -> [Option<DeckFetch>; NUM_DECKS] {
        let now = Instant::now();
        for f in &mut self.fetches {
            if f.as_ref().and_then(|f| f.until).is_some_and(|t| t <= now) {
                *f = None;
            }
        }
        self.fetches.clone()
    }
}
