//! Crates: the DJ's long-lived lists for sorting music, a track in as many
//! as it fits (Yoga, Journeys, Dance), each track once and in no order of
//! their own, as against playlists, which keep an order. Also what
//! Collection Health finds in the lists: entries for tracks taken out of the
//! library, repeats, empty lists and names used twice.

use std::collections::HashMap;

use crate::{track_id, Error, Library, PlaylistKind};

/// The source of a playlist made at the top of Playlists rather than in a
/// folder, so it isn't taken for a crate.
pub const TOP_PLAYLIST: &str = "opendj";

/// The source of a folder at the top of Crates rather than of Playlists.
pub const CRATE_FOLDER: &str = "opendj-crates";

/// Whether a list is a crate.
pub fn is_crate(p: &crate::Playlist) -> bool {
    p.kind == PlaylistKind::Crate
}

/// Whether a list or folder came in with another app's import (its source
/// names the app), as against one the DJ made.
pub fn is_imported(p: &crate::Playlist) -> bool {
    p.source
        .as_deref()
        .is_some_and(|s| s != TOP_PLAYLIST && s != CRATE_FOLDER)
}

/// Moves the lists and folders the DJ made inside `source`'s imported
/// folders out to the top of Playlists (what's in them moves along), so
/// taking that import's lists away leaves them be. Returns how many moved.
pub(crate) fn rescue_own_lists(tx: &rusqlite::Transaction, source: &str) -> Result<usize, Error> {
    let own: Vec<i64> = tx
        .prepare(
            "SELECT c.id FROM playlists c JOIN playlists p ON c.parent = p.id
              WHERE p.source = ?1 AND (c.source IS NULL OR c.source = ?2 OR c.source = ?3)
              ORDER BY c.position, c.id",
        )?
        .query_map(rusqlite::params![source, TOP_PLAYLIST, CRATE_FOLDER], |r| {
            r.get(0)
        })?
        .collect::<Result<_, _>>()?;
    for &id in &own {
        let position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM playlists WHERE parent IS NULL",
            [],
            |r| r.get(0),
        )?;
        // Out to the top: a crate to Crates, anything else to Playlists.
        tx.execute(
            "UPDATE playlists SET parent = NULL, position = ?2, source = NULL WHERE id = ?1",
            rusqlite::params![id, position],
        )?;
    }
    Ok(own.len())
}

impl Library {
    /// Moves the DJ's own lists out of every import's folders (see
    /// [`rescue_own_lists`]); made before imports were read-only.
    pub fn rescue_own_lists(&mut self) -> Result<usize, Error> {
        let tx = self.db.transaction()?;
        let sources: Vec<String> = tx
            .prepare(
                "SELECT DISTINCT source FROM playlists
                  WHERE source IS NOT NULL AND source NOT IN (?1, ?2)",
            )?
            .query_map([TOP_PLAYLIST, CRATE_FOLDER], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut moved = 0;
        for s in &sources {
            moved += rescue_own_lists(&tx, s)?;
        }
        tx.commit()?;
        Ok(moved)
    }

    /// Makes another app's imported lists and folders the DJ's own: they
    /// stay where they are and can be edited, and that app's next import
    /// no longer replaces them. Returns how many changed hands.
    pub fn adopt_import(&self, source: &str) -> Result<usize, Error> {
        self.db.execute(
            "UPDATE playlists SET source = NULL WHERE source = ?1",
            [source],
        )
    }

    /// Puts list or folder `id` first among the lists beside it, so a new
    /// one shows at the top of its folder or section.
    pub fn put_first(&self, id: i64) -> Result<(), Error> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "UPDATE playlists SET position = position + 1
             WHERE parent IS (SELECT parent FROM playlists WHERE id = ?1) AND id != ?1",
            [id],
        )?;
        tx.execute("UPDATE playlists SET position = 0 WHERE id = ?1", [id])?;
        tx.commit()
    }

    /// Makes an empty crate in `folder`, or at the top of Crates.
    pub fn create_crate(&self, name: &str, folder: Option<i64>) -> Result<i64, Error> {
        self.create_playlist(name, folder, PlaylistKind::Crate)
    }

    /// Makes a folder at the top of Crates.
    pub fn create_crate_folder(&self, name: &str) -> Result<i64, Error> {
        let id = self.create_playlist(name, None, PlaylistKind::Folder)?;
        self.db.execute(
            "UPDATE playlists SET source = ?2 WHERE id = ?1",
            rusqlite::params![id, CRATE_FOLDER],
        )?;
        Ok(id)
    }

    /// Turns a playlist into a crate (each track once, the first kept) or
    /// a crate into a playlist (in the order its tracks went in).
    pub fn set_crate(&mut self, id: i64, krate: bool) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        let (from, to) = if krate {
            (PlaylistKind::List, PlaylistKind::Crate)
        } else {
            (PlaylistKind::Crate, PlaylistKind::List)
        };
        let changed = tx.execute(
            "UPDATE playlists SET kind = ?3 WHERE id = ?1 AND kind = ?2",
            rusqlite::params![id, from.to_i64(), to.to_i64()],
        )?;
        if changed > 0 && krate {
            tx.execute(
                "DELETE FROM playlist_tracks
                  WHERE playlist_id = ?1
                    AND EXISTS (SELECT 1 FROM playlist_tracks p
                                 WHERE p.playlist_id = ?1
                                   AND p.track_id = playlist_tracks.track_id
                                   AND p.position < playlist_tracks.position)",
                [id],
            )?;
        }
        tx.commit()
    }

    /// Moves a list or folder to the end of `folder`, or to the top of
    /// Crates (`crates`) or Playlists. What's inside a folder moves along.
    pub fn move_playlist(&self, id: i64, folder: Option<i64>, crates: bool) -> Result<(), Error> {
        self.place_playlist(id, folder, crates, None)
    }

    /// Puts a list or folder in `folder` (or at the top of Crates when
    /// `crates`, else Playlists) right before its list `before`, or at the
    /// end when there is none. The lists beside it keep their order. What's
    /// inside a folder moves along.
    pub fn place_playlist(
        &self,
        id: i64,
        folder: Option<i64>,
        crates: bool,
        before: Option<i64>,
    ) -> Result<(), Error> {
        let tx = self.db.unchecked_transaction()?;
        let mut siblings: Vec<i64> = tx
            .prepare(
                "SELECT id FROM playlists WHERE parent IS ?1 AND id != ?2 ORDER BY position, id",
            )?
            .query_map(rusqlite::params![folder, id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let at = before
            .and_then(|b| siblings.iter().position(|&s| s == b))
            .unwrap_or(siblings.len());
        siblings.insert(at, id);
        // Only a folder at the top needs marking to sit under Crates; a
        // playlist at the top keeps the mark that keeps it out of Crates.
        let source = match folder {
            None if crates => Some(CRATE_FOLDER),
            _ => None,
        };
        tx.execute(
            "UPDATE playlists SET parent = ?2,
                    source = CASE WHEN kind = ?4 THEN ?3
                                  WHEN kind = ?5 AND ?2 IS NULL THEN ?6
                                  WHEN kind = ?5 THEN NULL
                                  ELSE source END
              WHERE id = ?1",
            rusqlite::params![
                id,
                folder,
                source,
                PlaylistKind::Folder.to_i64(),
                PlaylistKind::List.to_i64(),
                TOP_PLAYLIST
            ],
        )?;
        let mut set = tx.prepare("UPDATE playlists SET position = ?2 WHERE id = ?1")?;
        for (position, sibling) in siblings.iter().enumerate() {
            set.execute(rusqlite::params![sibling, position as i64])?;
        }
        drop(set);
        tx.commit()?;
        Ok(())
    }

    /// Makes an empty playlist at the top of Playlists, out of any folder.
    pub fn create_top_playlist(&self, name: &str) -> Result<i64, Error> {
        let id = self.create_playlist(name, None, PlaylistKind::List)?;
        self.db.execute(
            "UPDATE playlists SET source = ?2 WHERE id = ?1",
            rusqlite::params![id, TOP_PLAYLIST],
        )?;
        Ok(id)
    }

    /// The crates: the DJ's own lists at the top of the tree, all but the
    /// Inbox list downloads are noted in.
    fn crate_ids(&self) -> Result<HashMap<i64, String>, Error> {
        let inbox = self.inbox_list_id();
        Ok(self
            .playlists()?
            .into_iter()
            .filter(|p| is_crate(p) && Some(p.id) != inbox)
            .map(|p| (p.id, p.name))
            .collect())
    }

    /// The crates each track is in, by track id, each crate once, in name
    /// order.
    pub fn crates_by_track(&self) -> Result<HashMap<usize, Vec<String>>, Error> {
        let crates = self.crate_ids()?;
        let mut q = self
            .db
            .prepare("SELECT DISTINCT playlist_id, track_id FROM playlist_tracks")?;
        let mut out: HashMap<usize, Vec<String>> = HashMap::new();
        for row in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (list, track) = row?;
            if let Some(name) = crates.get(&list) {
                out.entry(track_id(track)).or_default().push(name.clone());
            }
        }
        for names in out.values_mut() {
            names.sort_by_key(|n| n.to_lowercase());
            names.dedup();
        }
        Ok(out)
    }
}

/// What can be wrong with one of the DJ's lists, on Collection Health's
/// Lists page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListIssueKind {
    /// Entries for tracks taken out of the collection.
    Removed,
    /// The same track more than once.
    Twice,
    /// Nothing in it.
    Empty,
    /// Another list or folder beside it has its name.
    SameName,
}

/// One list's problem.
#[derive(Clone, Debug, PartialEq)]
pub struct ListIssue {
    pub list: i64,
    pub name: String,
    pub kind: ListIssueKind,
    /// Entries it concerns: removed tracks, or repeats.
    pub count: usize,
    /// Whether tidying it needs no look: entries for removed tracks,
    /// repeats in a crate, an imported list that was always empty.
    pub sure: bool,
}

/// The DJ's lists and what is wrong with them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListReport {
    /// Lists and crates looked at.
    pub lists: usize,
    pub issues: Vec<ListIssue>,
}

impl ListReport {
    /// Lists with nothing wrong.
    pub fn tidy(&self) -> usize {
        let bad: std::collections::HashSet<i64> = self.issues.iter().map(|i| i.list).collect();
        self.lists.saturating_sub(bad.len())
    }

    pub fn sure(&self) -> usize {
        self.issues.iter().filter(|i| i.sure).count()
    }
}

impl Library {
    /// The lists and crates Collection Health looks at: the DJ's and
    /// imported ones, not history, saved searches or the purchases Inbox.
    fn checked_lists(&self) -> Result<Vec<crate::Playlist>, Error> {
        let inbox = self.inbox_list_id();
        Ok(self
            .playlists()?
            .into_iter()
            .filter(|p| matches!(p.kind, PlaylistKind::List | PlaylistKind::Crate))
            .filter(|p| Some(p.id) != inbox)
            .collect())
    }

    /// What is wrong with the DJ's lists. A list the DJ made in the last
    /// month, or one in a gig's folder, may stay empty: one made for a gig
    /// starts that way.
    pub fn list_report(&self) -> Result<ListReport, Error> {
        let month_ago = crate::now() - 30 * 24 * 3600;
        let all = self.playlists()?;
        let lists = self.checked_lists()?;
        let sizes = self.list_sizes()?;
        let count_by = |sql: &str| -> Result<HashMap<i64, usize>, Error> {
            let mut q = self.db.prepare(sql)?;
            let rows = q.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as usize))
            })?;
            rows.collect()
        };
        let removed = count_by(
            "SELECT playlist_id, COUNT(*) FROM playlist_tracks
              WHERE track_id IN (SELECT track_id FROM removed) GROUP BY playlist_id",
        )?;
        let twice = count_by(
            "SELECT playlist_id, COUNT(*) - COUNT(DISTINCT track_id) FROM playlist_tracks
              GROUP BY playlist_id HAVING COUNT(*) > COUNT(DISTINCT track_id)",
        )?;
        let kind_of: HashMap<i64, PlaylistKind> = all.iter().map(|p| (p.id, p.kind)).collect();
        let mut names: HashMap<(Option<i64>, String), usize> = HashMap::new();
        for p in all.iter().filter(|p| {
            matches!(
                p.kind,
                PlaylistKind::List | PlaylistKind::Crate | PlaylistKind::Folder | PlaylistKind::Set
            )
        }) {
            *names
                .entry((p.parent, p.name.trim().to_lowercase()))
                .or_default() += 1;
        }
        let mut issues = Vec::new();
        for p in &lists {
            let issue = |kind, count, sure| ListIssue {
                list: p.id,
                name: p.name.clone(),
                kind,
                count,
                sure,
            };
            if let Some(&n) = removed.get(&p.id) {
                issues.push(issue(ListIssueKind::Removed, n, true));
            }
            if let Some(&n) = twice.get(&p.id) {
                issues.push(issue(ListIssueKind::Twice, n, is_crate(p)));
            }
            let in_gig = p
                .parent
                .is_some_and(|f| kind_of.get(&f) == Some(&PlaylistKind::Set));
            let new = !is_imported(p) && p.created > month_ago;
            if !sizes.contains_key(&p.id) && !in_gig && !new {
                issues.push(issue(ListIssueKind::Empty, 0, is_imported(p)));
            }
            if names
                .get(&(p.parent, p.name.trim().to_lowercase()))
                .is_some_and(|&n| n > 1)
            {
                issues.push(issue(ListIssueKind::SameName, 0, false));
            }
        }
        Ok(ListReport {
            lists: lists.len(),
            issues,
        })
    }

    /// Fixes one list's problem: takes out entries for removed tracks or
    /// repeats (the first stays), or deletes an empty list. A name shared
    /// with another is left for the DJ to rename. Returns entries or lists
    /// that went.
    pub fn fix_list(&mut self, list: i64, kind: ListIssueKind) -> Result<usize, Error> {
        Ok(match kind {
            ListIssueKind::Removed => self.db.execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1
                   AND track_id IN (SELECT track_id FROM removed)",
                [list],
            )?,
            ListIssueKind::Twice => self.db.execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND position NOT IN
                   (SELECT MIN(position) FROM playlist_tracks WHERE playlist_id = ?1
                     GROUP BY track_id)",
                [list],
            )?,
            ListIssueKind::Empty => {
                let empty = !self.list_sizes()?.contains_key(&list);
                if empty {
                    self.delete_playlist(list)?;
                }
                empty as usize
            }
            ListIssueKind::SameName => 0,
        })
    }

    /// Fixes every sure problem; returns lists deleted and entries taken
    /// out.
    pub fn tidy_sure_lists(&mut self) -> Result<(usize, usize), Error> {
        let (mut lists, mut entries) = (0, 0);
        for i in self.list_report()?.issues.into_iter().filter(|i| i.sure) {
            let n = self.fix_list(i.list, i.kind)?;
            if i.kind == ListIssueKind::Empty {
                lists += n;
            } else {
                entries += n;
            }
        }
        Ok((lists, entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::file_track;

    fn library() -> (Library, Vec<usize>) {
        let mut lib = Library::in_memory().unwrap();
        lib.sync(&[
            file_track("/m/a.mp3", "A"),
            file_track("/m/b.mp3", "B"),
            file_track("/m/c.mp3", "C"),
        ])
        .unwrap();
        let ids = lib.tracks(|_| true).unwrap().iter().map(|t| t.id).collect();
        (lib, ids)
    }

    #[test]
    fn list_report_finds_and_fixes_problems() {
        let (mut lib, ids) = library();
        let yoga = lib.create_crate("Yoga", None).unwrap();
        let mine = lib.create_crate("Empty", None).unwrap();
        // Made long ago: a new one may stay empty.
        lib.db
            .execute("UPDATE playlists SET created = 0 WHERE id = ?1", [mine])
            .unwrap();
        lib.create_crate("New", None).unwrap();
        let imported = lib
            .create_playlist("Old", None, PlaylistKind::List)
            .unwrap();
        lib.db
            .execute(
                "UPDATE playlists SET source = 'vdj' WHERE id = ?1",
                [imported],
            )
            .unwrap();
        let gig = lib
            .create_playlist("Friday", None, PlaylistKind::Set)
            .unwrap();
        lib.create_playlist("Opening", Some(gig), PlaylistKind::List)
            .unwrap();
        let set = lib
            .create_playlist("Set", None, PlaylistKind::List)
            .unwrap();
        lib.create_playlist("Set", None, PlaylistKind::Folder)
            .unwrap();
        lib.add_to_playlist(yoga, &[ids[0], ids[1]]).unwrap();
        // A crate brought in from another app may hold a track twice.
        lib.db
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, position, track_id)
                 SELECT playlist_id, 99, track_id FROM playlist_tracks
                  WHERE playlist_id = ?1 ORDER BY position LIMIT 1",
                [yoga],
            )
            .unwrap();
        lib.add_to_playlist(set, &[ids[2], ids[2], ids[1]]).unwrap();
        lib.remove_tracks(&[ids[1]]).unwrap();
        let r = lib.list_report().unwrap();
        let has = |list, kind, sure| {
            r.issues
                .iter()
                .any(|i| i.list == list && i.kind == kind && i.sure == sure)
        };
        assert!(has(yoga, ListIssueKind::Removed, true));
        assert!(has(yoga, ListIssueKind::Twice, true));
        assert!(has(set, ListIssueKind::Twice, false));
        assert!(has(set, ListIssueKind::SameName, false));
        assert!(has(mine, ListIssueKind::Empty, false));
        assert!(has(imported, ListIssueKind::Empty, true));
        // The gig's list may stay empty.
        assert_eq!(
            r.issues
                .iter()
                .filter(|i| i.kind == ListIssueKind::Empty)
                .count(),
            2
        );
        assert_eq!((r.lists, r.tidy()), (6, 2));
        assert_eq!(lib.tidy_sure_lists().unwrap(), (1, 3));
        assert_eq!(lib.playlist_tracks(yoga).unwrap(), vec![ids[0]]);
        let r = lib.list_report().unwrap();
        assert_eq!(r.sure(), 0);
        assert!(has_list(&lib, mine) && !has_list(&lib, imported));
        assert_eq!(lib.fix_list(set, ListIssueKind::Twice).unwrap(), 1);
        assert_eq!(lib.playlist_tracks(set).unwrap(), vec![ids[2]]);
    }

    fn has_list(lib: &Library, id: i64) -> bool {
        lib.playlists().unwrap().iter().any(|p| p.id == id)
    }

    #[test]
    fn a_playlist_at_the_top_is_not_a_crate() {
        let (lib, _) = library();
        let list = lib.create_top_playlist("Gig").unwrap();
        let krate = lib.create_crate("Yoga", None).unwrap();
        let lists = lib.playlists().unwrap();
        let get = |id| lists.iter().find(|p| p.id == id).unwrap();
        assert!(!is_crate(get(list)));
        assert!(is_crate(get(krate)));
        assert!(!lib.crate_ids().unwrap().contains_key(&list));
    }

    #[test]
    fn crates_are_their_own_kind() {
        let (mut lib, ids) = library();
        let yoga = lib.create_crate("Yoga", None).unwrap();
        let folder = lib
            .create_playlist("Gigs", None, PlaylistKind::Folder)
            .unwrap();
        let friday = lib
            .create_playlist("Friday", Some(folder), PlaylistKind::List)
            .unwrap();
        let inbox = lib.inbox_list().unwrap();
        lib.add_to_playlist(yoga, &[ids[0], ids[1]]).unwrap();
        lib.add_to_playlist(friday, &[ids[0], ids[2]]).unwrap();
        lib.add_to_playlist(inbox, &[ids[2]]).unwrap();
        let crates = lib.crates_by_track().unwrap();
        assert_eq!(crates[&ids[0]], ["Yoga"]);
        assert_eq!(crates[&ids[1]], ["Yoga"]);
        assert!(!crates.contains_key(&ids[2]));
    }

    #[test]
    fn a_crate_holds_each_track_once_and_can_live_in_a_folder() {
        let (mut lib, ids) = library();
        let folder = lib.create_crate_folder("Moods").unwrap();
        let calm = lib.create_crate("Calm", Some(folder)).unwrap();
        lib.add_to_playlist(calm, &[ids[0], ids[0], ids[1]])
            .unwrap();
        lib.add_to_playlist(calm, &[ids[1], ids[2]]).unwrap();
        assert_eq!(lib.playlist_tracks(calm).unwrap(), [ids[0], ids[1], ids[2]]);
        assert_eq!(lib.crates_by_track().unwrap()[&ids[2]], ["Calm"]);
        let moods = lib.playlists().unwrap();
        let moods = moods.iter().find(|p| p.id == folder).unwrap();
        assert!(!is_imported(moods));
    }

    #[test]
    fn crates_stay_in_crate_folders() {
        let (mut lib, _) = library();
        let moods = lib.create_crate_folder("Moods").unwrap();
        let calm = lib.create_crate("Calm", Some(moods)).unwrap();
        assert_eq!(lib.rescue_own_lists().unwrap(), 0);
        let lists = lib.playlists().unwrap();
        assert_eq!(
            lists.iter().find(|p| p.id == calm).unwrap().parent,
            Some(moods)
        );
    }

    #[test]
    fn folders_move_between_crates_and_playlists() {
        let (lib, _) = library();
        let gigs = lib
            .create_playlist("Gigs", None, PlaylistKind::Folder)
            .unwrap();
        let friday = lib
            .create_playlist("Friday", Some(gigs), PlaylistKind::List)
            .unwrap();
        lib.move_playlist(gigs, None, true).unwrap();
        let get = |lib: &Library, id| {
            lib.playlists()
                .unwrap()
                .into_iter()
                .find(|p| p.id == id)
                .unwrap()
        };
        assert_eq!(get(&lib, gigs).source.as_deref(), Some(CRATE_FOLDER));
        assert_eq!(get(&lib, friday).parent, Some(gigs));
        let moods = lib.create_crate_folder("Moods").unwrap();
        lib.move_playlist(gigs, Some(moods), false).unwrap();
        let g = get(&lib, gigs);
        assert_eq!((g.parent, g.source), (Some(moods), None));
        lib.move_playlist(friday, None, false).unwrap();
        assert_eq!(get(&lib, friday).parent, None);
    }

    #[test]
    fn lists_go_where_they_are_dropped() {
        let (lib, _) = library();
        let gigs = lib
            .create_playlist("Gigs", None, PlaylistKind::Folder)
            .unwrap();
        let mk = |name| {
            lib.create_playlist(name, Some(gigs), PlaylistKind::List)
                .unwrap()
        };
        let (warm, peak, close) = (mk("Warm-up"), mk("Peak"), mk("Close"));
        let order = |lib: &Library| -> Vec<i64> {
            lib.playlists()
                .unwrap()
                .into_iter()
                .filter(|p| p.parent == Some(gigs))
                .map(|p| p.id)
                .collect()
        };
        // Before a sibling, then to the end, then first.
        lib.place_playlist(close, Some(gigs), false, Some(peak))
            .unwrap();
        assert_eq!(order(&lib), [warm, close, peak]);
        lib.place_playlist(warm, Some(gigs), false, None).unwrap();
        assert_eq!(order(&lib), [close, peak, warm]);
        lib.place_playlist(warm, Some(gigs), false, Some(close))
            .unwrap();
        assert_eq!(order(&lib), [warm, close, peak]);
        // A new list goes first.
        lib.put_first(peak).unwrap();
        assert_eq!(order(&lib), [peak, warm, close]);
        lib.place_playlist(peak, Some(gigs), false, None).unwrap();
        // Out to the top of Playlists: still a playlist, not a crate.
        lib.place_playlist(peak, None, false, Some(gigs)).unwrap();
        let p = lib
            .playlists()
            .unwrap()
            .into_iter()
            .find(|p| p.id == peak)
            .unwrap();
        assert_eq!((p.parent, p.source.as_deref()), (None, Some(TOP_PLAYLIST)));
        let top: Vec<i64> = lib
            .playlists()
            .unwrap()
            .into_iter()
            .filter(|p| p.id == peak || p.id == gigs)
            .map(|p| p.id)
            .collect();
        assert_eq!(top, [peak, gigs]);
    }

    #[test]
    fn lists_change_kind() {
        let (mut lib, ids) = library();
        let gig = lib.create_top_playlist("Gig").unwrap();
        lib.add_to_playlist(gig, &[ids[1], ids[0], ids[1]]).unwrap();
        lib.set_crate(gig, true).unwrap();
        assert_eq!(lib.playlist_tracks(gig).unwrap(), [ids[1], ids[0]]);
        assert!(lib.crate_ids().unwrap().contains_key(&gig));
        lib.set_crate(gig, false).unwrap();
        assert!(!lib.crate_ids().unwrap().contains_key(&gig));
    }
}
