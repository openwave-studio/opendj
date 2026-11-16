//! The Info tab's Tags tab rows that aren't tags: the crates the tracks
//! shown are in (chips, with a + menu to add them to another or a new
//! one) and their stage (Inbox, In Prep or Play Ready, from a ▾).

use std::collections::BTreeMap;

use opendj_library::query::COLOR_NAMES;
use opendj_library::stages::Stage;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::info::{CRATES_ID, STAGE_ID};
use super::Browser;
use crate::{InfoField, TagChip, TrackInfo, ValueChoice};

/// The stages the picker offers, in order, as the track menu names them.
const STAGES: [(Stage, &str); 3] = [
    (Stage::Inbox, "Inbox"),
    (Stage::Triage, "In Prep"),
    (Stage::Collection, "Play Ready"),
];

/// What the picker calls stage `name` (as the library keeps it).
pub(super) fn stage_label(name: &str) -> &'static str {
    STAGES
        .iter()
        .find(|(s, _)| s.name() == name)
        .map_or("", |(_, label)| label)
}

/// `chosen` crates of `all` (name, how many tracks), most used first,
/// leaving out `taken` and keeping those that contain `find`.
fn crates_found(
    all: &BTreeMap<String, usize>,
    taken: &[String],
    find: &str,
) -> Vec<(String, usize)> {
    let find = find.trim().to_lowercase();
    let mut out: Vec<(String, usize)> = all
        .iter()
        .filter(|(name, _)| {
            name.to_lowercase().contains(&find)
                && !taken.iter().any(|t| t.eq_ignore_ascii_case(name))
        })
        .map(|(n, c)| (n.clone(), *c))
        .collect();
    out.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    out
}

impl Browser {
    /// Every crate and how many tracks it has.
    fn crate_counts(&self) -> BTreeMap<String, usize> {
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        if let Some(db) = &self.db {
            for p in db.playlists().unwrap_or_default() {
                if opendj_library::sets::is_crate(&p) {
                    out.entry(p.name).or_default();
                }
            }
        }
        for names in self.crates.values() {
            for n in names {
                *out.entry(n.clone()).or_default() += 1;
            }
        }
        out
    }

    /// The crates of the tracks shown, and on how many of them each is.
    fn shown_crates(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for (_, t) in self.info_tracks() {
            for n in self.crates.get(&t.id).into_iter().flatten() {
                *counts.entry(n.clone()).or_default() += 1;
            }
        }
        counts.into_iter().collect()
    }

    /// The Tags tab's Stage picker and Crates chips, for the tracks shown.
    pub(super) fn list_fields(&self) -> Vec<InfoField> {
        let mut out = Vec::new();
        let shown = self.info_tracks();
        if shown.is_empty() {
            return out;
        }
        if self.folders.is_set() {
            let stages: Vec<&str> = shown.iter().map(|(_, t)| t.info.stage.as_str()).collect();
            let value = if stages.iter().all(|s| *s == stages[0]) {
                stage_label(stages[0]).to_string()
            } else {
                format!("{} stages", {
                    let mut s = stages.clone();
                    s.sort();
                    s.dedup();
                    s.len()
                })
            };
            out.push(InfoField {
                hint: "Not in a stage".into(),
                ..super::info::tags_field(STAGE_ID, "STAGE", value, super::info::PICK)
            });
        }
        let looks = &self.filter.crate_looks;
        let chips: Vec<TagChip> = self
            .shown_crates()
            .into_iter()
            .map(|(name, count)| {
                // The crate's icon in its color, as the sidebar draws it.
                let look = looks.get(&name.to_lowercase());
                let color = look.and_then(|l| l.color);
                TagChip {
                    tag: name.into(),
                    count: count as i32,
                    tint: slint::Color::from_argb_encoded(0xff00_0000 | color.unwrap_or(0)),
                    has_color: color.is_some(),
                    icon: look.map_or(0, |l| l.icon as i32),
                    icon_tint: color
                        .and_then(|rgb| COLOR_NAMES.iter().position(|&(_, c)| c == rgb))
                        .map_or(-1, |i| i as i32),
                }
            })
            .collect();
        out.push(InfoField {
            chips: ModelRc::new(VecModel::from(chips)),
            hint: "+ crate".into(),
            ..super::info::tags_field(CRATES_ID, "CRATES", "", super::info::CHIPS)
        });
        out
    }

    /// The menu of the Stage picker or the Crates +: what can be picked,
    /// those containing `find` for crates.
    pub(super) fn list_choices(&self, id: &str, find: &str) -> Vec<(String, i32)> {
        if id == STAGE_ID {
            return STAGES.iter().map(|(_, l)| (l.to_string(), -1)).collect();
        }
        // Crates every track shown is in already aren't offered again.
        let n = self.info_tracks().len();
        let taken: Vec<String> = self
            .shown_crates()
            .into_iter()
            .filter(|(_, c)| *c == n)
            .map(|(name, _)| name)
            .collect();
        crates_found(&self.crate_counts(), &taken, find)
            .into_iter()
            .map(|(name, c)| (name, c as i32))
            .collect()
    }

    /// Fills the Info tab's menu with `choices`.
    pub(super) fn set_choices(&self, choices: Vec<(String, i32)>) {
        if let Some(app) = self.app.upgrade() {
            let choices: Vec<ValueChoice> = choices
                .into_iter()
                .map(|(value, count)| ValueChoice {
                    value: value.into(),
                    count,
                })
                .collect();
            app.global::<TrackInfo>()
                .set_choices(ModelRc::new(VecModel::from(choices)));
        }
    }

    /// Moves the tracks shown to the stage the picker calls `label`.
    pub(super) fn info_set_stage(&mut self, label: &str) {
        let Some((stage, _)) = STAGES.iter().find(|(_, l)| *l == label) else {
            return;
        };
        let indices: Vec<usize> = self.info_tracks().iter().map(|(i, _)| *i).collect();
        self.change_stage(&indices, *stage, None);
        self.refresh_shown_info();
    }

    /// Puts the tracks shown in crate `name` (made when there is none by
    /// that name), or takes them out of it.
    pub(super) fn change_crate(&mut self, name: &str, add: bool) {
        let name = name.trim();
        if name.is_empty() || !self.saved() {
            return;
        }
        let shown = self.info_tracks();
        let indices: Vec<usize> = shown.iter().map(|(i, _)| *i).collect();
        let Some(db) = &self.db else { return };
        let found = db
            .playlists()
            .unwrap_or_default()
            .into_iter()
            .filter(opendj_library::sets::is_crate)
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| p.id);
        if add {
            let id = match found {
                Some(id) => id,
                None => match db.create_crate(name, None) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("can't make a crate: {e}");
                        return;
                    }
                },
            };
            // Only those not in it already, so none is in twice.
            let ids = db.playlist_tracks(id).unwrap_or_default();
            let new: Vec<usize> = indices
                .into_iter()
                .filter(|&i| !ids.contains(&self.library[i].id))
                .collect();
            self.add_to_playlist(&new, id);
            if found.is_none() {
                self.refresh_tree();
            }
        } else if let Some(id) = found {
            let Some(db) = &mut self.db else { return };
            let wanted: Vec<usize> = shown.iter().map(|(_, t)| t.id).collect();
            // From the end, so the places before stay where they are.
            let ids = db.playlist_tracks(id).unwrap_or_default();
            for (at, track) in ids.iter().enumerate().rev() {
                if wanted.contains(track) {
                    if let Err(e) = db.remove_from_playlist(id, at) {
                        eprintln!("can't take a track out of the crate: {e}");
                    }
                }
            }
            self.refresh_crates();
            if self.nodes.get(self.current_node).and_then(|n| n.playlist) == Some(id) {
                self.refresh_rows();
            }
        }
        self.refresh_shown_info();
    }

    /// Shows the Info tab again after a change it didn't make itself.
    fn refresh_shown_info(&mut self) {
        if let Some(app) = self.app.upgrade() {
            self.refresh_info(&app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_crates_most_used_first() {
        let all: BTreeMap<String, usize> =
            [("Yoga", 3), ("Warm-up", 12), ("Peak", 7), ("Empty", 0)]
                .map(|(n, c)| (n.to_string(), c))
                .into_iter()
                .collect();
        let names = |v: Vec<(String, usize)>| v.into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        assert_eq!(
            names(crates_found(&all, &[], "")),
            ["Warm-up", "Peak", "Yoga", "Empty"]
        );
        assert_eq!(
            names(crates_found(&all, &["peak".into()], "")),
            ["Warm-up", "Yoga", "Empty"]
        );
        assert_eq!(
            names(crates_found(&all, &[], "a")),
            ["Warm-up", "Peak", "Yoga"]
        );
    }

    #[test]
    fn names_stages_as_the_track_menu_does() {
        assert_eq!(stage_label("inbox"), "Inbox");
        assert_eq!(stage_label("triage"), "In Prep");
        assert_eq!(stage_label("collection"), "Play Ready");
        assert_eq!(stage_label(""), "");
    }
}
