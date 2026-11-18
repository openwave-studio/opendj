//! The Sidelist: a scratch pad kept across restarts, or one of the DJ's
//! playlists or crates shown beside the track list, so a list can be built
//! while browsing anywhere else. What is dropped, reordered or removed there
//! goes straight into the list it shows.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{Browser, COLLECTION};
use crate::{Choice, SidelistUi};

/// The side panel's Sidelist tab.
pub(super) const SIDELIST_TAB: usize = 1;

/// Settings: the list the Sidelist shows, and the scratch pad's tracks (by
/// track id) for when it shows none.
const SHOWN_KEY: &str = "sidelist_list";
const SCRATCH_KEY: &str = "sidelist_scratch";

/// The `value` of the Save scratch pad as playlist line.
const SAVE_SCRATCH: i32 = -2;

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

impl Browser {
    /// The list the Sidelist shows, if it still exists.
    fn sidelist_list(&self) -> Option<i64> {
        let id = saved_list()?;
        let lists = self.db.as_ref()?.playlists().ok()?;
        lists.iter().any(|p| p.id == id).then_some(id)
    }

    /// Fills the Sidelist from the list it shows, or the scratch pad.
    pub(super) fn load_sidelist(&mut self) {
        let ids = match (self.sidelist_list(), &self.db) {
            (Some(list), Some(db)) => db.playlist_tracks(list).unwrap_or_default(),
            _ => saved_scratch(),
        };
        self.side[SIDELIST_TAB] = ids
            .into_iter()
            .filter_map(|id| self.index_of.get(&id).copied())
            .collect();
        self.refresh_sidelist_ui();
    }

    /// Keeps what the Sidelist holds now: in the list it shows, or as the
    /// scratch pad.
    pub(super) fn sidelist_changed(&mut self) {
        let ids: Vec<usize> = self.side[SIDELIST_TAB]
            .iter()
            .map(|&i| self.library[i].id)
            .collect();
        match self.sidelist_list() {
            Some(list) => {
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
            None => crate::settings::set(SCRATCH_KEY, serde_json::json!(ids)),
        }
    }

    /// Shows the DJ's list `list` in the Sidelist, or the scratch pad, and
    /// opens the Sidelist tab.
    pub(super) fn show_in_sidelist(&mut self, list: Option<i64>) {
        crate::settings::set(SHOWN_KEY, serde_json::json!(list));
        self.load_sidelist();
        if let Some(app) = self.app.upgrade() {
            app.set_side_tab(SIDELIST_TAB as i32);
        }
        self.refresh_side();
    }

    /// Adds the library tracks `ids` to the end of what the Sidelist shows.
    pub(crate) fn add_to_sidelist(&mut self, ids: Vec<usize>) {
        if ids.is_empty() {
            return;
        }
        self.side[SIDELIST_TAB].extend(ids);
        self.sidelist_changed();
        self.refresh_side();
    }

    /// Adds the tracks of list rows `first..=last` (or the picked ones) to
    /// the Sidelist.
    pub(crate) fn add_rows_to_sidelist(&mut self, first: usize, last: usize) {
        let ids = self.playable_rows(first, last);
        self.add_to_sidelist(ids);
    }

    /// Makes the scratch pad a playlist at the top of Playlists, shows it
    /// in the Sidelist and starts a new, empty scratch pad.
    fn save_scratch_as_playlist(&mut self) {
        if self.sidelist_list().is_some() || self.side[SIDELIST_TAB].is_empty() {
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
        let name = super::untaken_name("Sidelist", &taken);
        let made = db
            .create_top_playlist(&name)
            .and_then(|list| db.add_to_playlist(list, &ids).map(|_| list));
        match made {
            Ok(list) => {
                crate::settings::set(SCRATCH_KEY, serde_json::json!([]));
                self.reload_tree(None);
                self.show_in_sidelist(Some(list));
            }
            Err(e) => eprintln!("can't save the Sidelist: {e}"),
        }
    }

    /// Fills the Sidelist's picker: the scratch pad, then the DJ's crates
    /// and playlists, with the one shown marked.
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
            let mut choices = vec![choice("Scratch pad".into(), false, 0)];
            for crates in [true, false] {
                choices.push(choice(
                    if crates { "CRATES" } else { "PLAYLISTS" }.into(),
                    true,
                    -1,
                ));
                for (i, l) in self.menu_lists.iter().enumerate() {
                    if l.is_crate == crates {
                        let text = if l.folder.is_empty() {
                            l.name.clone()
                        } else {
                            format!("{} › {}", l.folder, l.name)
                        };
                        choices.push(choice(text, false, i as i32 + 1));
                    }
                }
            }
            if shown.is_none() && !self.side[SIDELIST_TAB].is_empty() {
                choices.push(choice(String::new(), true, -1));
                choices.push(choice(
                    "Save scratch pad as playlist".into(),
                    false,
                    SAVE_SCRATCH,
                ));
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
            None => ("Scratch pad".to_string(), 0),
        };
        ui.set_name(SharedString::from(name));
        ui.set_current(current);
        ui.set_shows_list(shown.is_some());
    }

    /// The Sidelist picker's line `value` was picked.
    pub(super) fn pick_sidelist(&mut self, value: i32) {
        match value {
            SAVE_SCRATCH => self.save_scratch_as_playlist(),
            0 => self.show_in_sidelist(None),
            v if v > 0 => {
                if let Some(list) = self.menu_lists.get(v as usize - 1).map(|l| l.id) {
                    self.show_in_sidelist(Some(list));
                }
            }
            _ => {}
        }
    }
}
