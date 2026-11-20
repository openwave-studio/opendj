//! A streaming service's catalog search (TIDAL): typing in the list's
//! search box while the service is shown lists artists, albums and tracks
//! under the box, as the player's search does. An artist opens their page
//! in the dropdown (top tracks, then albums); an album opens its tracks in
//! the list; a track loads onto a free deck through the add-on that plays
//! the service's tracks, or downloads on its right-click.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::{searches, Browser, SEARCH_PAUSE};
use crate::sources::Catalog;
use crate::spotify::{self, Hit, Hits, Request, StreamTrack};
use crate::{AppWindow, CatalogRow, CatalogSearch, StoreUi};

/// How big a result's picture is fetched, in pixels (shown at 34).
const PICTURE_SIDE: u32 = 80;

/// A square picture's side length and RGBA pixels.
type Thumbnail = (u32, Vec<u8>);

/// A row of the dropdown: what a click on it does.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Pick {
    /// Lists an album's tracks: its key (see [`spotify::Hit::key`]).
    Open(String),
    /// Shows an artist's page: their key and name.
    Artist(String, String),
    Track(Box<StreamTrack>),
}

/// The catalog search's state, kept by the browser.
pub(super) struct CatalogState {
    /// The words searched for last, or the artist key whose page was
    /// asked for, to match the answer to.
    words: String,
    /// The artist whose page shows (key, name), and the words to go back to.
    artist: Option<(String, String)>,
    searched: String,
    /// When to ask, once typing pauses.
    due: Option<std::time::Instant>,
    picks: Vec<Pick>,
    /// Each row's picture's address ("" for none), and the pictures in.
    pictures: Vec<String>,
    images: HashMap<String, slint::Image>,
    want_picture: Sender<String>,
    pictures_in: Receiver<(String, Option<Thumbnail>)>,
}

impl CatalogState {
    pub(super) fn new() -> Self {
        let (want_picture, wanted) = channel::<String>();
        let (tx, pictures_in) = channel();
        std::thread::Builder::new()
            .name("catalog pictures".into())
            .spawn(move || {
                for url in wanted {
                    let thumb = crate::coverart::thumbnail(&url, PICTURE_SIDE);
                    if tx.send((url, thumb)).is_err() {
                        return;
                    }
                }
            })
            .expect("spawn catalog pictures thread");
        Self {
            words: String::new(),
            artist: None,
            searched: String::new(),
            due: None,
            picks: Vec::new(),
            pictures: Vec::new(),
            images: HashMap::new(),
            want_picture,
            pictures_in,
        }
    }
}

type Row = (Pick, CatalogRow, String);

/// The dropdown's rows for what a search found, and what each does.
pub(super) fn rows_of(hits: Hits) -> Vec<Row> {
    let mut rows = artist_rows(hits.artists);
    rows.extend(album_rows(hits.albums));
    rows.extend(track_rows(hits.tracks));
    rows
}

/// An artist's page: their top tracks, then their albums.
pub(super) fn artist_page(hits: Hits) -> Vec<Row> {
    let mut rows = track_rows(hits.tracks);
    rows.extend(album_rows(hits.albums));
    rows
}

fn artist_rows(artists: Vec<Hit>) -> Vec<Row> {
    artists
        .into_iter()
        .map(|a| {
            let row = CatalogRow {
                kind: 1,
                title: a.name.as_str().into(),
                detail: "Artist".into(),
                initials: initials(&a.name).into(),
                ..Default::default()
            };
            (Pick::Artist(a.key, a.name), row, a.picture)
        })
        .collect()
}

fn album_rows(albums: Vec<Hit>) -> Vec<Row> {
    let mut rows = Vec::new();
    for a in albums {
        let mut detail = String::from("Album");
        if !a.by.is_empty() {
            detail.push_str(" · ");
            detail.push_str(&a.by);
        }
        if a.count > 0 {
            detail.push_str(&format!(
                " · {} track{}",
                a.count,
                if a.count == 1 { "" } else { "s" }
            ));
        }
        let row = CatalogRow {
            kind: 2,
            title: a.name.as_str().into(),
            detail: detail.into(),
            initials: initials(&a.name).into(),
            ..Default::default()
        };
        rows.push((Pick::Open(a.key), row, a.picture));
    }
    rows
}

fn track_rows(tracks: Vec<(StreamTrack, String)>) -> Vec<Row> {
    let mut rows = Vec::new();
    for (t, cover) in tracks {
        let mut detail = t.artist();
        if t.duration > 0.0 {
            let secs = t.duration.round() as u64;
            detail.push_str(&format!(" · {}:{:02}", secs / 60, secs % 60));
        }
        if t.bpm > 0.0 {
            detail.push_str(&format!(" · {:.0} BPM", t.bpm));
        }
        if !t.key.is_empty() {
            detail.push_str(" · ");
            detail.push_str(&t.key);
        }
        let row = CatalogRow {
            kind: 0,
            title: t.title.as_str().into(),
            detail: detail.into(),
            initials: initials(&t.title).into(),
            ..Default::default()
        };
        rows.push((Pick::Track(Box::new(t)), row, cover));
    }
    rows
}

/// Up to two letters standing in for a missing picture.
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

impl Browser {
    pub(super) fn install_catalog(app: &AppWindow, b: &Rc<RefCell<Self>>) {
        let ui = app.global::<CatalogSearch>();
        let w = Rc::downgrade(b);
        ui.on_act(move |row, mode| {
            if let Some(b) = w.upgrade() {
                b.borrow_mut().catalog_act(row.max(0) as usize, mode);
            }
        });
        let w = Rc::downgrade(b);
        ui.on_back(move || {
            if let Some(b) = w.upgrade() {
                b.borrow_mut().catalog_back();
            }
        });
        let w = Rc::downgrade(b);
        ui.on_all(move || {
            if let Some(b) = w.upgrade() {
                b.borrow_mut().catalog_all();
            }
        });
    }

    /// Browses `service`'s catalog through an add-on from now on (see
    /// [`Catalog`]).
    pub fn use_catalog(&mut self, service: &str, catalog: Catalog) {
        if let Some(st) = self.streams.iter().find(|s| s.id == service) {
            st.service.send(Request::Catalog(catalog));
        }
    }

    /// Typing in the search box of a service that searches: asks for what
    /// matches once typing pauses. Clearing it shows the playlist again.
    pub(super) fn search_catalog(&mut self, text: &str) {
        let Some(st) = self.shown_stream() else {
            return;
        };
        if !searches(st.id) {
            return;
        }
        let words = text.trim();
        if words.is_empty() {
            self.stream_search.clear();
        }
        self.leave_artist();
        if words.chars().count() < 2 {
            self.catalog.due = None;
            self.catalog.words.clear();
            self.show_catalog(false, "");
            return;
        }
        self.catalog.words = words.to_string();
        self.catalog.due = Some(std::time::Instant::now() + SEARCH_PAUSE);
        self.show_catalog(true, "Searching…");
    }

    /// Asks for the search once typing has paused. Called every frame.
    pub(super) fn poll_catalog(&mut self) {
        if self
            .catalog
            .due
            .is_some_and(|due| due <= std::time::Instant::now())
        {
            self.catalog.due = None;
            let words = self.catalog.words.clone();
            if let Some(st) = self.shown_stream() {
                st.service.send(Request::Search(words));
            }
        }
        let mut changed = false;
        for (url, thumb) in self.catalog.pictures_in.try_iter().collect::<Vec<_>>() {
            if let Some((side, rgba)) = thumb {
                let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &rgba, side, side,
                );
                self.catalog
                    .images
                    .insert(url, slint::Image::from_rgba8(pixels));
                changed = true;
            }
        }
        if changed {
            self.fill_catalog_pictures();
        }
    }

    /// What a search found, when it is still the one asked for.
    pub(super) fn catalog_found(&mut self, words: &str, hits: Hits) {
        if words != self.catalog.words {
            return;
        }
        let rows = if self.catalog.artist.is_some() {
            artist_page(hits)
        } else {
            rows_of(hits)
        };
        self.catalog.picks = rows.iter().map(|(p, _, _)| p.clone()).collect();
        self.catalog.pictures = rows.iter().map(|(_, _, u)| u.clone()).collect();
        for url in &self.catalog.pictures {
            if !url.is_empty() && !self.catalog.images.contains_key(url) {
                let _ = self.catalog.want_picture.send(url.clone());
            }
        }
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let ui = app.global::<CatalogSearch>();
        let model: Vec<CatalogRow> = rows.into_iter().map(|(_, r, _)| r).collect();
        ui.set_rows(ModelRc::new(VecModel::from(model)));
        ui.set_current(-1);
        ui.set_note(SharedString::new());
        self.fill_catalog_pictures();
    }

    /// A search that went wrong: why, in the dropdown.
    pub(super) fn catalog_failed(&mut self, words: &str, error: &str) {
        if words == self.catalog.words {
            self.catalog.picks.clear();
            self.show_catalog(true, error);
        }
    }

    fn fill_catalog_pictures(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let rows = app.global::<CatalogSearch>().get_rows();
        for (i, url) in self.catalog.pictures.iter().enumerate() {
            let Some(image) = self.catalog.images.get(url) else {
                continue;
            };
            if let Some(mut row) = rows.row_data(i).filter(|r| !r.has_cover) {
                row.cover = image.clone();
                row.has_cover = true;
                rows.set_row_data(i, row);
            }
        }
    }

    /// Opens or closes the dropdown, emptying it with `note` in its place.
    fn show_catalog(&mut self, open: bool, note: &str) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let ui = app.global::<CatalogSearch>();
        if !open {
            self.leave_artist();
        }
        if !open || !note.is_empty() {
            self.catalog.picks.clear();
            self.catalog.pictures.clear();
            ui.set_rows(ModelRc::default());
        }
        ui.set_note(note.into());
        ui.set_current(-1);
        ui.set_open(open);
    }

    /// Lists `key` (a search's or an artist's or album's tracks) in place
    /// of the playlist, and empties the search box.
    fn list_catalog(&mut self, key: String) {
        self.stream_search = key;
        self.search.clear();
        self.catalog.words.clear();
        self.catalog.due = None;
        if let Some(app) = self.app.upgrade() {
            app.global::<StoreUi>().set_search_text(SharedString::new());
        }
        self.show_catalog(false, "");
        self.fetch_current_playlist();
        self.refresh_rows();
    }

    /// A click on a row (mode 0), or Download on a track's menu (1).
    fn catalog_act(&mut self, row: usize, mode: i32) {
        let Some(pick) = self.catalog.picks.get(row).cloned() else {
            return;
        };
        match pick {
            Pick::Open(key) => self.list_catalog(key),
            Pick::Artist(key, name) => self.open_artist(key, name),
            Pick::Track(t) => {
                let service = self
                    .shown_stream()
                    .map(|s| s.id.to_string())
                    .unwrap_or_default();
                if mode == 1 {
                    self.download_stream(&t, &service);
                } else {
                    self.play_stream(&t, &service, None);
                }
                if let Some(app) = self.app.upgrade() {
                    app.global::<CatalogSearch>().set_open(false);
                }
            }
        }
    }

    /// Shows an artist's page in the dropdown, asking the service for it.
    fn open_artist(&mut self, key: String, name: String) {
        if self.catalog.artist.is_none() {
            self.catalog.searched = self.catalog.words.clone();
        }
        self.catalog.words = key.clone();
        self.catalog.due = None;
        self.catalog.artist = Some((key.clone(), name.clone()));
        if let Some(st) = self.shown_stream() {
            st.service.send(Request::Artist(key));
        }
        self.show_catalog(true, "Loading…");
        if let Some(app) = self.app.upgrade() {
            app.global::<CatalogSearch>().set_artist(name.into());
        }
    }

    /// Back from an artist's page to what the search found.
    fn catalog_back(&mut self) {
        if self.catalog.artist.is_none() {
            return;
        }
        let words = std::mem::take(&mut self.catalog.searched);
        self.leave_artist();
        self.catalog.words = words.clone();
        if let Some(st) = self.shown_stream() {
            st.service.send(Request::Search(words));
        }
        self.show_catalog(true, "Searching…");
    }

    /// Forgets the artist page shown, if any.
    fn leave_artist(&mut self) {
        if self.catalog.artist.take().is_some() {
            if let Some(app) = self.app.upgrade() {
                app.global::<CatalogSearch>()
                    .set_artist(SharedString::new());
            }
        }
    }

    /// Return with no row picked: every track found (or the artist's), in
    /// the list.
    fn catalog_all(&mut self) {
        if let Some((key, _)) = self.catalog.artist.clone() {
            self.list_catalog(key);
            return;
        }
        let words = self.catalog.words.clone();
        if !words.is_empty() {
            self.list_catalog(format!("{}{words}", spotify::SEARCH));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_come_artists_then_albums_then_tracks() {
        let hits = Hits {
            artists: vec![Hit {
                key: "artist:1".into(),
                name: "Radiohead".into(),
                ..Default::default()
            }],
            albums: vec![Hit {
                key: "album:2".into(),
                name: "OK Computer".into(),
                by: "Radiohead".into(),
                count: 12,
                picture: "c.jpg".into(),
            }],
            tracks: vec![(
                StreamTrack {
                    id: "3".into(),
                    title: "Airbag".into(),
                    artists: vec!["Radiohead".into()],
                    duration: 284.0,
                    ..Default::default()
                },
                "c.jpg".into(),
            )],
        };
        let rows = rows_of(hits);
        let kinds: Vec<i32> = rows.iter().map(|(_, r, _)| r.kind).collect();
        assert_eq!(kinds, [1, 2, 0]);
        assert_eq!(
            rows[0].0,
            Pick::Artist("artist:1".into(), "Radiohead".into())
        );
        assert_eq!(rows[1].1.detail, "Album · Radiohead · 12 tracks");
        assert_eq!(rows[2].1.detail, "Radiohead · 4:44");
        assert_eq!(rows[2].2, "c.jpg");
        assert_eq!(rows[0].1.initials, "R");
        assert_eq!(initials("OK Computer"), "OC");
    }
}
