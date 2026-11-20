//! Two side panel tabs for building lists while browsing anywhere else: the
//! Shortlist, a scratch list kept across restarts, and Playlist, one of the
//! DJ's playlists picked to work on. What is dropped, reordered or removed
//! in the Playlist tab goes straight into that playlist. Two tabs, so the
//! tab itself says whether a saved list is being changed.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{Browser, COLLECTION};
use crate::{Choice, SidelistUi};

/// The side panel's Shortlist tab.
pub(super) const SIDELIST_TAB: usize = 1;
/// The side panel's Playlist tab.
pub(super) const PLAYLIST_TAB: usize = 7;

/// Whether side panel tab `tab` holds one of these lists.
pub(super) fn is_list_tab(tab: usize) -> bool {
    tab == SIDELIST_TAB || tab == PLAYLIST_TAB
}

/// Settings: the playlist the Playlist tab shows, and the Shortlist's
/// tracks (by track id).
const SHOWN_KEY: &str = "sidelist_list";
const SCRATCH_KEY: &str = "sidelist_scratch";

fn saved_list() -> Option<i64> {
    crate::settings::load()[SHOWN_KEY].as_i64()
}

fn saved_scratch() -> Vec<usize> {
    crate::settings::load()[SCRATCH_KEY]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_u64().map(|v| v as usize))
                .collect()
        })
        .unwrap_or_default()
}

/// Keeps `ids` as the scratch pad's tracks.
pub(super) fn save_scratch(ids: &[usize]) {
    crate::settings::set(SCRATCH_KEY, serde_json::json!(ids));
}

impl Browser {
    /// The playlist the Playlist tab shows, if it still exists.
    pub(super) fn sidelist_list(&self) -> Option<i64> {
        let id = saved_list()?;
        let lists = self.db.as_ref()?.playlists().ok()?;
        // Playlists only: crates are big collections, not lists to work on.
        lists
            .iter()
            .any(|p| p.id == id && p.kind != opendj_library::PlaylistKind::Crate)
            .then_some(id)
    }

    /// Fills the Shortlist and the Playlist tab.
    pub(super) fn load_sidelist(&mut self) {
        let shown = match (self.sidelist_list(), &self.db) {
            (Some(list), Some(db)) => db.playlist_tracks(list).unwrap_or_default(),
            _ => Vec::new(),
        };
        let index = |ids: Vec<usize>| -> Vec<usize> {
            ids.into_iter()
                .filter_map(|id| self.index_of.get(&id).copied())
                .collect()
        };
        self.side[SIDELIST_TAB] = index(saved_scratch());
        self.side[PLAYLIST_TAB] = index(shown);
        self.refresh_sidelist_ui();
    }

    /// The list side tab `tab` holds: `Some(None)` for the Shortlist,
    /// `Some(Some(id))` for the playlist the Playlist tab shows, `None`
    /// when it holds none.
    pub(super) fn side_list(&self, tab: usize) -> Option<Option<i64>> {
        match tab {
            SIDELIST_TAB => Some(None),
            PLAYLIST_TAB => self.sidelist_list().map(Some),
            _ => None,
        }
    }

    /// Keeps what side tab `tab` holds now: in the playlist it shows, or
    /// as the Shortlist.
    pub(super) fn side_changed(&mut self, tab: usize) {
        let ids: Vec<usize> = self.side[tab].iter().map(|&i| self.library[i].id).collect();
        match self.side_list(tab) {
            Some(Some(list)) => {
                if let Some(db) = &mut self.db {
                    if let Err(e) = db.set_playlist_tracks(list, &ids) {
                        eprintln!("can't change the list: {e}");
                    }
                }
                self.refresh_crates();
                if self.nodes.get(self.current_node).and_then(|n| n.playlist) == Some(list) {
                    self.refresh_rows();
                }
                self.refresh_tree();
            }
            Some(None) => save_scratch(&ids),
            None => {}
        }
    }

    /// Shows the DJ's playlist `list` in the Playlist tab and opens it.
    pub(super) fn show_in_sidelist(&mut self, list: i64) {
        crate::settings::set(SHOWN_KEY, serde_json::json!(list));
        self.load_sidelist();
        if let Some(app) = self.app.upgrade() {
            app.set_side_tab(PLAYLIST_TAB as i32);
        }
        self.refresh_side();
    }

    /// Adds the library tracks `ids` to the end of the Shortlist.
    pub(crate) fn add_to_sidelist(&mut self, ids: Vec<usize>) {
        if ids.is_empty() {
            return;
        }
        self.side[SIDELIST_TAB].extend(ids);
        self.side_changed(SIDELIST_TAB);
        self.refresh_side();
    }

    /// Adds the tracks of list rows `first..=last` (or the picked ones) to
    /// the Shortlist.
    pub(crate) fn add_rows_to_sidelist(&mut self, first: usize, last: usize) {
        let ids = self.playable_rows(first, last);
        self.add_to_sidelist(ids);
    }

    /// Makes the Shortlist a playlist at the top of Playlists, shows it in
    /// the Playlist tab and starts a new, empty Shortlist.
    pub(super) fn save_scratch_as_playlist(&mut self) {
        if self.side[SIDELIST_TAB].is_empty() {
            return;
        }
        let ids: Vec<usize> = self.side[SIDELIST_TAB]
            .iter()
            .map(|&i| self.library[i].id)
            .collect();
        let Some(db) = &mut self.db else { return };
        let taken: Vec<String> = db
            .playlists()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.name)
            .collect();
        let taken: Vec<&str> = taken.iter().map(String::as_str).collect();
        let name = super::untaken_name("Shortlist", &taken);
        let made = db
            .create_top_playlist(&name)
            .and_then(|list| db.add_to_playlist(list, &ids).map(|_| list));
        match made {
            Ok(list) => {
                save_scratch(&[]);
                self.reload_tree(None);
                self.show_in_sidelist(list);
            }
            Err(e) => eprintln!("can't save the Shortlist: {e}"),
        }
    }

    /// Fills the Playlist tab's picker with the DJ's playlists, the one
    /// shown marked.
    pub(super) fn refresh_sidelist_ui(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let ui = app.global::<SidelistUi>();
        let shown = self.sidelist_list();
        let choice = |text: String, header: bool, value: i32| Choice {
            text: text.into(),
            header,
            value,
        };
        // The DJ's lists are only known while the collection's tree is up.
        if self.tree == COLLECTION && !self.menu_lists.is_empty() {
            let mut choices = vec![choice("PLAYLISTS".into(), true, -1)];
            for (i, l) in self.menu_lists.iter().enumerate() {
                if !l.is_crate {
                    let text = if l.folder.is_empty() {
                        l.name.clone()
                    } else {
                        format!("{} › {}", l.folder, l.name)
                    };
                    choices.push(choice(text, false, i as i32 + 1));
                }
            }
            ui.set_choices(ModelRc::new(VecModel::from(choices)));
        }
        let (name, current) = match shown {
            Some(list) => {
                let at = self.menu_lists.iter().position(|l| l.id == list);
                let name = at.map(|i| self.menu_lists[i].name.clone()).or_else(|| {
                    let db = self.db.as_ref()?;
                    let lists = db.playlists().ok()?;
                    lists.into_iter().find(|p| p.id == list).map(|p| p.name)
                });
                (name.unwrap_or_default(), at.map_or(-1, |i| i as i32 + 1))
            }
            None => ("Pick a Playlist".to_string(), -1),
        };
        ui.set_name(SharedString::from(name));
        ui.set_current(current);
        ui.set_shows_list(shown.is_some());
    }

    /// The Playlist tab's picker line `value` was picked.
    pub(super) fn pick_sidelist(&mut self, value: i32) {
        if value > 0 {
            if let Some(list) = self.menu_lists.get(value as usize - 1).map(|l| l.id) {
                self.show_in_sidelist(list);
            }
        }
    }
}
