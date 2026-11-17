//! Collection Health's Tags page: the genres and each of the DJ's tag
//! groups, how many tracks have one, the most used names, names spelled
//! like another and names used once, and merging names into one. Genres
//! are written to the files' tags too (see [`Browser::make_changes`]).

use std::collections::{HashMap, HashSet};

use opendj_library::cleanup::Field;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::cleanup::genre_key;
use super::{is_browsed, Browser, Contents};
use crate::{CleanupUi, GenreRow, TagGroupCard};

/// What the Tags page looks at: the genres, or one tag group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(super) enum TagSet {
    #[default]
    Genre,
    Group(i64),
}

impl TagSet {
    fn field(self) -> Field {
        match self {
            TagSet::Genre => Field::Genre,
            TagSet::Group(g) => Field::Tag(g),
        }
    }

    /// The names a track has in it.
    fn names(self, t: &opendj_engine::Track) -> Vec<&str> {
        match self {
            TagSet::Genre => Some(t.genre.trim())
                .filter(|g| !g.is_empty())
                .into_iter()
                .collect(),
            TagSet::Group(g) => t.info.tags_in(g).collect(),
        }
    }
}

/// The Tags page: which set it shows, its Show tab and the ticked names.
#[derive(Default)]
pub(super) struct TagsPage {
    pub(super) set: TagSet,
    /// 0 every name, 1 spelled like another, 2 used once.
    pub(super) show: usize,
    pub(super) picked: HashSet<String>,
}

/// How a set is used.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TagStats {
    /// Tracks with at least one name in the set, of how many.
    pub(super) tagged: usize,
    pub(super) of: usize,
    /// Every name with how many tracks have it, most used first.
    pub(super) names: Vec<(String, usize)>,
    /// Names spelled alike but for case, spaces or punctuation: the
    /// spelling kept (the most used), then the others.
    pub(super) twins: Vec<(String, Vec<String>)>,
}

impl TagStats {
    /// From each track's names in the set.
    pub(super) fn new<'a>(tracks: impl IntoIterator<Item = Vec<&'a str>>) -> TagStats {
        let (mut tagged, mut of) = (0, 0);
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for names in tracks {
            of += 1;
            if !names.is_empty() {
                tagged += 1;
            }
            for n in names {
                *counts.entry(n).or_default() += 1;
            }
        }
        let mut names: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(n, c)| (n.to_string(), c))
            .collect();
        names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut by_key: HashMap<String, Vec<&(String, usize)>> = HashMap::new();
        for n in &names {
            by_key.entry(genre_key(&n.0)).or_default().push(n);
        }
        let mut twins: Vec<(String, Vec<String>)> = by_key
            .into_values()
            .filter(|v| v.len() > 1)
            .map(|v| {
                // The most used; between equals, one with capitals.
                let keep = v
                    .iter()
                    .max_by_key(|n| {
                        (
                            n.1,
                            n.0.chars().any(char::is_uppercase),
                            std::cmp::Reverse(&n.0),
                        )
                    })
                    .map(|n| n.0.clone())
                    .unwrap_or_default();
                let others = v
                    .iter()
                    .map(|n| n.0.clone())
                    .filter(|n| *n != keep)
                    .collect();
                (keep, others)
            })
            .collect();
        twins.sort_by_key(|t| t.0.to_lowercase());
        TagStats {
            tagged,
            of,
            names,
            twins,
        }
    }

    /// Names used on one track only.
    pub(super) fn once(&self) -> usize {
        self.names.iter().filter(|n| n.1 == 1).count()
    }

    /// Names that would go in a roll-up.
    pub(super) fn to_roll_up(&self) -> usize {
        self.twins.iter().map(|t| t.1.len()).sum()
    }

    fn alike(&self) -> HashSet<&str> {
        self.twins
            .iter()
            .flat_map(|(k, o)| std::iter::once(k.as_str()).chain(o.iter().map(String::as_str)))
            .collect()
    }
}

/// "House 120 · Techno 80 · Disco 31"
fn top_names(stats: &TagStats) -> String {
    stats
        .names
        .iter()
        .take(3)
        .map(|(n, c)| format!("{n} {c}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

impl Browser {
    /// Whether the Tags page is shown.
    pub(super) fn shows_tags(&self) -> bool {
        self.saved()
            && self.tree == super::COLLECTION
            && self
                .nodes
                .get(self.current_node)
                .is_some_and(|n| matches!(n.contents, Contents::Tags))
    }

    /// Shows the Tags page.
    pub(super) fn open_tags(&mut self) {
        self.show_health(true);
        let Some(n) = self
            .nodes
            .iter()
            .rposition(|n| matches!(n.contents, Contents::Tags))
        else {
            return;
        };
        self.review_at = None;
        self.dupe_tab = None;
        self.current_node = n;
        self.folders_focused = false;
        self.refresh_tree();
        self.refresh_rows();
    }

    /// The genres, then each tag group, with their names.
    fn tag_sets(&self) -> Vec<(TagSet, String)> {
        std::iter::once((TagSet::Genre, "Genre".to_string()))
            .chain(
                self.filter
                    .names
                    .tags
                    .iter()
                    .map(|g| (TagSet::Group(g.id), g.name.clone())),
            )
            .collect()
    }

    /// How one set is used across the collection.
    pub(super) fn tag_stats(&self, set: TagSet) -> TagStats {
        TagStats::new(
            self.library
                .iter()
                .filter(|t| !is_browsed(t) && self.in_collection(t))
                .map(|t| set.names(t)),
        )
    }

    /// Names spelled like another, in every set: the Tags row's count.
    pub(super) fn tags_to_roll_up(&self) -> usize {
        self.tag_sets()
            .into_iter()
            .map(|(s, _)| self.tag_stats(s).to_roll_up())
            .sum()
    }

    /// The names the page lists, A to Z, with their tracks.
    fn tag_rows(&self, stats: &TagStats) -> Vec<(String, usize)> {
        let alike = stats.alike();
        let mut rows: Vec<(String, usize)> = stats
            .names
            .iter()
            .filter(|(n, c)| match self.cleanup.tags.show {
                1 => alike.contains(n.as_str()),
                2 => *c == 1,
                _ => true,
            })
            .cloned()
            .collect();
        rows.sort_by(|a, b| {
            a.0.to_lowercase()
                .cmp(&b.0.to_lowercase())
                .then(a.0.cmp(&b.0))
        });
        rows
    }

    /// Shows the Tags page as things are now.
    pub(super) fn refresh_tags_ui(&self, ui: &CleanupUi) {
        let shown = self.shows_tags();
        ui.set_genres_open(shown);
        if !shown {
            return;
        }
        let page = &self.cleanup.tags;
        let sets = self.tag_sets();
        let mut current = None;
        let cards: Vec<TagGroupCard> = sets
            .iter()
            .map(|(set, name)| {
                let s = self.tag_stats(*set);
                let card = TagGroupCard {
                    name: name.clone().into(),
                    percent: (s.tagged * 100).checked_div(s.of).unwrap_or(0) as i32,
                    names: s.names.len() as i32,
                    twins: s.to_roll_up() as i32,
                    once: s.once() as i32,
                    top: top_names(&s).into(),
                };
                if *set == page.set {
                    current = Some(s);
                }
                card
            })
            .collect();
        let at = sets.iter().position(|(s, _)| *s == page.set).unwrap_or(0);
        let stats = current.unwrap_or_else(|| self.tag_stats(TagSet::Genre));
        let alike = stats.alike();
        let rows: Vec<GenreRow> = self
            .tag_rows(&stats)
            .into_iter()
            .map(|(name, count)| GenreRow {
                picked: page.picked.contains(&name),
                alike: alike.contains(name.as_str()),
                name: name.into(),
                count: count as i32,
            })
            .collect();
        ui.set_tag_groups(ModelRc::new(VecModel::from(cards)));
        ui.set_tag_group(at as i32);
        ui.set_tag_show(page.show as i32);
        ui.set_tag_counts(ModelRc::new(VecModel::from(vec![
            stats.names.len() as i32,
            alike.len() as i32,
            stats.once() as i32,
        ])));
        ui.set_roll_ups(stats.to_roll_up() as i32);
        ui.set_genres(ModelRc::new(VecModel::from(rows)));
        ui.set_genres_picked(page.picked.len() as i32);
    }

    /// Picks the set the Tags page lists.
    pub(super) fn pick_tag_set(&mut self, i: usize) {
        if let Some(&(set, _)) = self.tag_sets().get(i) {
            if set != self.cleanup.tags.set {
                self.cleanup.tags = super::tag_health::TagsPage {
                    set,
                    ..Default::default()
                };
                self.set_into("");
            }
        }
        self.refresh_cleanup_ui();
    }

    pub(super) fn show_tags(&mut self, show: usize) {
        self.cleanup.tags.show = show.min(2);
        self.refresh_cleanup_ui();
    }

    fn set_into(&self, name: &str) {
        if let Some(app) = self.app.upgrade() {
            app.global::<CleanupUi>().set_into(SharedString::from(name));
        }
    }

    /// Ticks or unticks a name; the most used of those ticked fills in the
    /// name to merge into, unless one was typed.
    pub(super) fn toggle_tag(&mut self, row: usize) {
        let stats = self.tag_stats(self.cleanup.tags.set);
        let Some((name, _)) = self.tag_rows(&stats).get(row).cloned() else {
            return;
        };
        let picked = &mut self.cleanup.tags.picked;
        if !picked.remove(&name) {
            picked.insert(name);
        }
        if let Some(app) = self.app.upgrade() {
            let ui = app.global::<CleanupUi>();
            let into = ui.get_into().to_string();
            let picked = &self.cleanup.tags.picked;
            if into.trim().is_empty() || stats.names.iter().any(|n| n.0 == into) {
                let best = stats
                    .names
                    .iter()
                    .filter(|n| picked.contains(&n.0))
                    .max_by_key(|n| (n.1, n.0.chars().any(char::is_uppercase)))
                    .map(|n| n.0.clone());
                ui.set_into(best.unwrap_or_default().into());
            }
        }
        self.refresh_cleanup_ui();
    }

    /// The changes giving every track with a name in `from` the name
    /// `into` instead.
    fn tag_changes(
        &self,
        set: TagSet,
        merges: &[(String, HashSet<String>)],
    ) -> Vec<(usize, Field, String)> {
        let into_for = |n: &str| {
            merges
                .iter()
                .find(|m| m.1.contains(n))
                .map(|m| m.0.as_str())
        };
        (0..self.library.len())
            .filter_map(|i| {
                let t = &self.library[i];
                if is_browsed(t) {
                    return None;
                }
                let names = set.names(t);
                if !names.iter().any(|n| into_for(n).is_some()) {
                    return None;
                }
                let new: Vec<&str> = names.iter().map(|n| into_for(n).unwrap_or(n)).collect();
                Some((i, set.field(), new.join(", ")))
            })
            .collect()
    }

    /// Makes the merges in one batch the undo line takes back, and drops
    /// tags no track has any more. Says what was done.
    fn merge_names(
        &mut self,
        set: TagSet,
        merges: Vec<(String, HashSet<String>)>,
        what: &str,
    ) -> String {
        let changes = self.tag_changes(set, &merges);
        let s = self.make_changes(changes, what);
        if let (TagSet::Group(g), Some(db)) = (set, &self.db) {
            let gone: HashSet<&String> = merges.iter().flat_map(|m| m.1.iter()).collect();
            for tag in db.tags_of_group(g).unwrap_or_default() {
                if tag.tracks == 0 && gone.contains(&tag.name) {
                    if let Err(e) = db.delete_tag(tag.id) {
                        eprintln!("can't drop tag {}: {e}", tag.name);
                    }
                }
            }
        }
        self.cleanup.tags.picked.clear();
        self.set_into("");
        self.refresh_cleanup_ui();
        self.refresh_tree();
        self.refresh_rows();
        s
    }

    /// Gives every track with a ticked name the name typed in.
    pub(super) fn merge_tags(&mut self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let into = app.global::<CleanupUi>().get_into().trim().to_string();
        let picked = self.cleanup.tags.picked.clone();
        if into.is_empty() || picked.is_empty() {
            return;
        }
        let set = self.cleanup.tags.set;
        let n = picked.len();
        let s = self.merge_names(set, vec![(into.clone(), picked)], "tag merge");
        self.health_status = format!("Merged {} into \"{into}\". {s}", names_word(n));
        self.refresh_health_ui();
    }

    /// Merges every name spelled like another into its most used spelling.
    pub(super) fn roll_up_tags(&mut self) {
        let set = self.cleanup.tags.set;
        let stats = self.tag_stats(set);
        let n = stats.to_roll_up();
        if n == 0 {
            return;
        }
        let merges = stats
            .twins
            .into_iter()
            .map(|(keep, others)| (keep, others.into_iter().collect()))
            .collect();
        let s = self.merge_names(set, merges, "tag roll-up");
        self.health_status = format!("Rolled up {} spelled like another. {s}", names_word(n));
        self.refresh_health_ui();
    }
}

/// "1 name" or "3 names".
fn names_word(n: usize) -> String {
    if n == 1 {
        "1 name".into()
    } else {
        format!("{n} names")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_count_coverage_twins_and_once() {
        let tracks = vec![
            vec!["Deep House"],
            vec!["Deep House"],
            vec!["Deep-House"],
            vec!["deep house"],
            vec!["Techno"],
            vec![],
        ];
        let s = TagStats::new(tracks);
        assert_eq!((s.tagged, s.of), (5, 6));
        assert_eq!(s.names[0], ("Deep House".into(), 2));
        assert_eq!(s.twins.len(), 1);
        assert_eq!(s.twins[0].0, "Deep House");
        let mut others = s.twins[0].1.clone();
        others.sort();
        assert_eq!(others, vec!["Deep-House", "deep house"]);
        assert_eq!(s.to_roll_up(), 2);
        // Deep-House, deep house, Techno.
        assert_eq!(s.once(), 3);
        assert_eq!(top_names(&s), "Deep House 2 · Deep-House 1 · Techno 1");
    }

    #[test]
    fn capitals_win_a_tie() {
        let s = TagStats::new(vec![vec!["synth"], vec!["Synth"]]);
        assert_eq!(
            s.twins,
            vec![("Synth".to_string(), vec!["synth".to_string()])]
        );
    }
}
