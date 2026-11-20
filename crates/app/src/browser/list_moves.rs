//! Adding tracks to the DJ's lists, moving them from one list to another
//! (⌥-drag), taking them out and putting a playlist in a new order, each
//! followed by a note with Undo.

use std::collections::HashSet;

use slint::{ComponentHandle, DataTransfer};

use super::health::tracks;
use super::sidelist::{is_list_tab, save_scratch, SIDELIST_TAB};
use super::{columns, dragged, Browser, Dragged, Undo, KIND_CRATE, KIND_LIST};
use crate::LibraryEdit;

/// Where dragged tracks come from, when moving them takes them out of it.
#[derive(Clone, PartialEq, Debug)]
enum Source {
    /// Rows of the open list (`.0`), in the track list.
    Open(i64, Vec<usize>),
    /// Rows `first..=last` of side tab `.0`, the Shortlist or Playlist.
    Side(usize, usize, usize),
}

/// `ids` with the entries at `positions` taken out.
fn without(ids: &[usize], positions: &HashSet<usize>) -> Vec<usize> {
    ids.iter()
        .enumerate()
        .filter(|(p, _)| !positions.contains(p))
        .map(|(_, &id)| id)
        .collect()
}

/// `ids` with the entries at `positions` moved, in their order, to go in
/// before entry `at` (as the list was).
fn moved_to(ids: &[usize], positions: &HashSet<usize>, at: usize) -> Vec<usize> {
    let picked: Vec<usize> = ids
        .iter()
        .enumerate()
        .filter(|(p, _)| positions.contains(p))
        .map(|(_, &id)| id)
        .collect();
    let mut rest = without(ids, positions);
    let at = (0..at.min(ids.len()))
        .filter(|p| !positions.contains(p))
        .count();
    rest.splice(at..at, picked);
    rest
}

impl Browser {
    /// The DJ's own list or crate open in the track list.
    fn open_list(&self) -> Option<i64> {
        let n = self.nodes.get(self.current_node)?;
        ((n.kind == KIND_LIST || n.kind == KIND_CRATE) && !n.locked)
            .then_some(n.playlist)
            .flatten()
    }

    /// What a note calls list `list`.
    fn note_name(&self, list: Option<i64>) -> String {
        let Some(list) = list else {
            return "the Shortlist".to_string();
        };
        self.menu_lists
            .iter()
            .find(|l| l.id == list)
            .map(|l| l.name.clone())
            .or_else(|| {
                let lists = self.db.as_ref()?.playlists().ok()?;
                lists.into_iter().find(|p| p.id == list).map(|p| p.name)
            })
            .unwrap_or_else(|| "the list".to_string())
    }

    fn is_crate(&self, list: i64) -> bool {
        self.menu_lists.iter().any(|l| l.id == list && l.is_crate)
    }

    /// The tracks (by id) lists hold now, to take a change back.
    fn snapshot(&self, lists: &[Option<i64>]) -> Vec<(Option<i64>, Vec<usize>)> {
        let mut seen = HashSet::new();
        lists
            .iter()
            .filter(|l| seen.insert(**l))
            .map(|&l| {
                let ids = match l {
                    Some(list) => self
                        .db
                        .as_ref()
                        .and_then(|db| db.playlist_tracks(list).ok())
                        .unwrap_or_default(),
                    None => self.side[SIDELIST_TAB]
                        .iter()
                        .map(|&i| self.library[i].id)
                        .collect(),
                };
                (l, ids)
            })
            .collect()
    }

    /// Shows `done` under the track list and, with `before`, offers to
    /// take the change back.
    fn note(&mut self, done: String, undo: String, before: Vec<(Option<i64>, Vec<usize>)>) {
        if !before.is_empty() {
            self.undo = Some(Undo {
                batch: None,
                text: undo,
                restore: Vec::new(),
                changes: None,
                relinked: Vec::new(),
                lists: before,
            });
        }
        if let Some(app) = self.app.upgrade() {
            let edit = app.global::<LibraryEdit>();
            edit.set_note_again(self.again.is_some());
            edit.set_list_note(done.into());
            edit.set_note_serial(edit.get_note_serial() + 1);
            self.refresh_stage_ui(&edit);
        }
    }

    /// Shows what changed in the lists everywhere they appear.
    fn lists_changed(&mut self) {
        self.refresh_crates();
        self.load_sidelist();
        self.refresh_side();
        self.refresh_rows();
        self.refresh_tree();
    }

    /// Adds library tracks `indices` to list `list`, with a note. A
    /// playlist skips the tracks it already has, and the note offers to add
    /// them again.
    pub(super) fn add_tracks(&mut self, indices: &[usize], list: i64) {
        if indices.is_empty() {
            return;
        }
        self.adopt(indices);
        let before = self.snapshot(&[Some(list)]);
        let mut seen: HashSet<usize> = before[0].1.iter().copied().collect();
        let (new, skipped): (Vec<usize>, Vec<usize>) = indices
            .iter()
            .partition(|&&i| seen.insert(self.library[i].id));
        if !new.is_empty() {
            self.add_to_playlist(&new, list);
            if self.sidelist_list() == Some(list) {
                self.load_sidelist();
                self.refresh_side();
            }
            self.refresh_tree();
        }
        self.again = (!skipped.is_empty() && !self.is_crate(list)).then(|| (list, skipped.clone()));
        let name = self.note_name(Some(list));
        let done = match (new.len(), skipped.len()) {
            (n, 0) => format!("Added {} to {name}", tracks(n)),
            (0, 1) => format!("Already in {name}"),
            (0, s) => format!("All {s} already in {name}"),
            (n, s) => format!("Added {} to {name}, {s} already there", tracks(n)),
        };
        let undo = format!("Undo Add to {name} ({})", tracks(new.len()));
        self.note(done, undo, if new.is_empty() { Vec::new() } else { before });
    }

    /// Adds the tracks the last add skipped, as further copies.
    pub(super) fn add_again(&mut self) {
        let Some((list, indices)) = self.again.take() else {
            return;
        };
        let before = self.snapshot(&[Some(list)]);
        self.add_to_playlist(&indices, list);
        self.lists_changed();
        let name = self.note_name(Some(list));
        self.note(
            format!("Added {} again to {name}", tracks(indices.len())),
            format!("Undo Add to {name} ({})", tracks(indices.len())),
            before,
        );
    }

    /// Where a drag's tracks come from, when moving can take them out:
    /// the open list's rows or the Sidelist's.
    fn source_of(&self, data: &DataTransfer) -> Option<Source> {
        match dragged(data)? {
            Dragged::Rows(first, last) => Some(Source::Open(
                self.open_list()?,
                self.chosen_rows(first, last),
            )),
            Dragged::Side(tab, first, last)
                if is_list_tab(tab) && self.side_list(tab).is_some() =>
            {
                Some(Source::Side(tab, first, last))
            }
            _ => None,
        }
    }

    /// The list (or, `None`, the scratch pad) a source is part of.
    fn source_list(&self, source: &Source) -> Option<i64> {
        match source {
            Source::Open(list, _) => Some(*list),
            Source::Side(tab, ..) => self.side_list(*tab).flatten(),
        }
    }

    /// Takes the tracks of `source` out of where they are.
    fn take_out(&mut self, source: Source) {
        match source {
            Source::Open(list, rows) => self.remove_open_rows(list, &rows),
            Source::Side(tab, first, last) => {
                let side = &mut self.side[tab];
                if first < side.len() {
                    let end = (last + 1).min(side.len());
                    side.drain(first..end);
                }
                self.side_changed(tab);
            }
        }
    }

    /// Takes rows `rows` of the open list `list` out of it.
    fn remove_open_rows(&mut self, list: i64, rows: &[usize]) {
        let Some(db) = &self.db else { return };
        let entries = db.playlist_tracks(list).unwrap_or_default();
        // Rows may be sorted or filtered: each row is the nth time its
        // track appears among the rows, and so in the list.
        let mut positions = HashSet::new();
        for &row in rows {
            let Some(&Some(index)) = self.rows.get(row) else {
                continue;
            };
            let id = self.library[index].id;
            let nth = self.rows[..row]
                .iter()
                .filter(|r| r.is_some_and(|i| self.library[i].id == id))
                .count();
            if let Some(p) = entries
                .iter()
                .enumerate()
                .filter(|(_, t)| **t == id)
                .nth(nth)
                .map(|(p, _)| p)
            {
                positions.insert(p);
            }
        }
        if positions.is_empty() {
            return;
        }
        let kept = without(&entries, &positions);
        if let Some(db) = &mut self.db {
            if let Err(e) = db.set_playlist_tracks(list, &kept) {
                eprintln!("can't change the list: {e}");
            }
        }
    }

    /// Takes the open list's rows `first..=last` (or the picked ones) out
    /// of it, with a note.
    pub(super) fn remove_rows_from_list(&mut self, first: usize, last: usize) {
        let Some(list) = self.open_list() else { return };
        let rows = self.chosen_rows(first, last);
        if rows.is_empty() {
            return;
        }
        let before = self.snapshot(&[Some(list)]);
        self.remove_open_rows(list, &rows);
        self.again = None;
        self.lists_changed();
        let name = self.note_name(Some(list));
        self.note(
            format!("Took {} out of {name}", tracks(rows.len())),
            format!("Undo Remove from {name} ({})", tracks(rows.len())),
            before,
        );
    }

    /// Tracks dropped on list `list` in the tree: added, or with ⌥
    /// (`moving`) moved there from the list they were dragged out of.
    pub(super) fn drop_tracks_on_list(&mut self, data: &DataTransfer, list: i64, moving: bool) {
        let source = self.source_of(data).filter(|_| moving);
        let ids = self.dropped_tracks(data);
        let Some(source) = source else {
            self.add_tracks(&ids, list);
            return;
        };
        let from = self.source_list(&source);
        if from == Some(list) || ids.is_empty() {
            return;
        }
        let before = self.snapshot(&[Some(list), from]);
        let have: HashSet<usize> = before[0].1.iter().copied().collect();
        let new: Vec<usize> = ids
            .iter()
            .copied()
            .filter(|&i| !have.contains(&self.library[i].id))
            .collect();
        self.add_to_playlist(&new, list);
        self.take_out(source);
        self.again = None;
        self.lists_changed();
        self.moved_note(ids.len(), from, Some(list), before);
    }

    /// Tracks dropped into side tab `tab` (the Shortlist or Playlist) at
    /// `at`; with ⌥ (`moving`) they leave the list they came from.
    pub(super) fn drop_tracks_on_sidelist(
        &mut self,
        tab: usize,
        data: &DataTransfer,
        at: usize,
        moving: bool,
    ) {
        let Some(to) = self.side_list(tab) else {
            return;
        };
        let source = self.source_of(data).filter(|_| moving);
        let ids = self.dropped_tracks(data);
        if ids.is_empty() {
            return;
        }
        let from = source.as_ref().map(|s| self.source_list(s));
        if from.is_some_and(|f| f == to) {
            return;
        }
        let before = if source.is_some() {
            self.snapshot(&[to, from.flatten()])
        } else {
            Vec::new()
        };
        let side = &mut self.side[tab];
        let at = at.min(side.len());
        side.splice(at..at, ids.iter().copied());
        self.side_changed(tab);
        if let Some(source) = source {
            self.take_out(source);
            self.lists_changed();
            self.moved_note(ids.len(), from.flatten(), to, before);
        } else {
            self.refresh_side();
        }
    }

    /// Tracks dropped on side tab `tab`'s title: added to the end of its
    /// list, with a note since the list may not be in view.
    pub(super) fn drop_tracks_on_side_tab(
        &mut self,
        tab: usize,
        data: &DataTransfer,
        moving: bool,
    ) {
        if tab == 0 {
            let ids = self.dropped_tracks(data);
            if !ids.is_empty() {
                self.side[0].extend(ids);
                self.refresh_side();
            }
            return;
        }
        let Some(list) = self.side_list(tab) else {
            return;
        };
        if moving && self.source_of(data).is_some() {
            self.drop_tracks_on_sidelist(tab, data, usize::MAX, true);
            return;
        }
        let before = self.snapshot(&[list]);
        let n = self.side[tab].len();
        self.drop_tracks_on_sidelist(tab, data, usize::MAX, false);
        let added = self.side[tab].len().saturating_sub(n);
        if added > 0 {
            let name = self.note_name(list);
            self.note(
                format!("Added {} to {name}", tracks(added)),
                format!("Undo Add to {name} ({})", tracks(added)),
                before,
            );
        }
    }

    /// Tracks dropped on the open list from a side list or a deck: added
    /// to its end, or with ⌥ (`moving`) moved out of the side list.
    pub(super) fn drop_tracks_on_open(&mut self, data: &DataTransfer, moving: bool) {
        let Some(list) = self.open_list() else { return };
        if !matches!(dragged(data), Some(Dragged::Side(..) | Dragged::Deck(_))) {
            return;
        }
        self.drop_tracks_on_list(data, list, moving);
    }

    fn moved_note(
        &mut self,
        n: usize,
        from: Option<i64>,
        to: Option<i64>,
        before: Vec<(Option<i64>, Vec<usize>)>,
    ) {
        let to_name = self.note_name(to);
        let from_name = self.note_name(from);
        self.note(
            format!("Moved {} from {from_name} to {to_name}", tracks(n)),
            format!("Undo Move to {to_name} ({})", tracks(n)),
            before,
        );
    }

    /// The open list can be put in a new order by dragging its rows: a
    /// playlist, listed by # from the top, every entry shown.
    fn can_reorder(&self) -> bool {
        let Some(list) = self.open_list() else {
            return false;
        };
        !self.is_crate(list)
            && self.sort_column == columns::ORDER
            && self.ascending
            && self.search.is_empty()
            && !self.filtering()
            && self.rows.iter().all(Option::is_some)
            && self
                .db
                .as_ref()
                .and_then(|db| db.playlist_tracks(list).ok())
                .is_some_and(|t| t.len() == self.rows.len())
    }

    /// Whether the open list takes a drop of `data`: the Sidelist's rows
    /// or a deck's track, or its own rows to put them in a new order.
    pub(super) fn open_list_accepts(&self, data: &DataTransfer) -> bool {
        match dragged(data) {
            Some(Dragged::Side(..) | Dragged::Deck(_)) => true,
            Some(Dragged::Rows(..)) => self.can_reorder(),
            None => false,
        }
    }

    /// The open playlist's rows `first..=last` (or the picked ones) moved
    /// to go in before row `at`.
    pub(super) fn reorder_open(&mut self, first: usize, last: usize, at: usize) {
        if !self.can_reorder() {
            return;
        }
        let Some(list) = self.open_list() else { return };
        let rows: HashSet<usize> = self.chosen_rows(first, last).into_iter().collect();
        let before = self.snapshot(&[Some(list)]);
        let ids = &before[0].1;
        let order = moved_to(ids, &rows, at);
        if order == *ids {
            return;
        }
        if let Some(db) = &mut self.db {
            if let Err(e) = db.set_playlist_tracks(list, &order) {
                eprintln!("can't change the list: {e}");
                return;
            }
        }
        self.again = None;
        self.lists_changed();
        let name = self.note_name(Some(list));
        self.note(
            format!("Moved {} in {name}", tracks(rows.len())),
            format!("Undo Reorder {name}"),
            before,
        );
    }

    /// What a track drag would do on tree row `row`: "Add 3" or "Move 3",
    /// or nothing when it can't go there.
    pub(super) fn drop_badge(&self, row: usize, data: &DataTransfer, moving: bool) -> String {
        let Some(list) = self.own_list_at(row) else {
            return String::new();
        };
        let n = match dragged(data) {
            Some(Dragged::Rows(first, last)) => self.chosen_rows(first, last).len(),
            Some(Dragged::Side(_, first, last)) => last + 1 - first,
            Some(Dragged::Deck(_)) => 1,
            None => return String::new(),
        };
        let source = self.source_of(data).filter(|_| moving);
        match source {
            Some(s) if self.source_list(&s) == Some(list) => String::new(),
            Some(_) => format!("Move {n}"),
            None => format!("Add {n}"),
        }
    }

    /// Empties the scratch pad, with Undo.
    pub(super) fn clear_scratch(&mut self) {
        if self.side[SIDELIST_TAB].is_empty() {
            return;
        }
        let before = self.snapshot(&[None]);
        let n = self.side[SIDELIST_TAB].len();
        self.side[SIDELIST_TAB].clear();
        self.side_changed(SIDELIST_TAB);
        self.refresh_side();
        self.note(
            format!("Cleared {} from the Shortlist", tracks(n)),
            "Undo Clear Shortlist".to_string(),
            before,
        );
    }

    /// Takes rows `first..=last` out of side tab `tab` (the Shortlist or
    /// the playlist the Playlist tab shows), with Undo.
    pub(super) fn remove_from_sidelist(&mut self, tab: usize, first: usize, last: usize) {
        let Some(list) = self.side_list(tab) else {
            return;
        };
        let side = &self.side[tab];
        if first >= side.len() {
            return;
        }
        let end = (last + 1).min(side.len());
        let before = self.snapshot(&[list]);
        self.side[tab].drain(first..end);
        self.side_changed(tab);
        self.refresh_side();
        let name = self.note_name(list);
        self.note(
            format!("Removed {} from {name}", tracks(end - first)),
            format!("Undo Remove from {name}"),
            before,
        );
    }

    /// Puts lists back as they were before the last add, move or removal.
    pub(super) fn restore_lists(&mut self, lists: Vec<(Option<i64>, Vec<usize>)>) {
        for (list, ids) in lists {
            match list {
                Some(list) => {
                    if let Some(db) = &mut self.db {
                        if let Err(e) = db.set_playlist_tracks(list, &ids) {
                            eprintln!("can't put the list back: {e}");
                        }
                    }
                }
                None => save_scratch(&ids),
            }
        }
        self.again = None;
        self.lists_changed();
        if let Some(app) = self.app.upgrade() {
            let edit = app.global::<LibraryEdit>();
            edit.set_list_note("".into());
            self.refresh_stage_ui(&edit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_move_together_to_where_they_are_dropped() {
        let ids = [10, 11, 12, 13, 14];
        let picked: HashSet<usize> = [1, 3].into();
        assert_eq!(moved_to(&ids, &picked, 0), [11, 13, 10, 12, 14]);
        assert_eq!(moved_to(&ids, &picked, 5), [10, 12, 14, 11, 13]);
        assert_eq!(moved_to(&ids, &picked, 3), [10, 12, 11, 13, 14]);
        assert_eq!(without(&ids, &picked), [10, 12, 14]);
    }
}
