//! Duplicates: the same track in the library more than once, grouped by
//! what tells the copies apart, so the easy cases can be fixed in one go.
//!
//! Copies are tracks with the same artist, title and remix (ignoring case,
//! punctuation and "Original Mix"). Those within a few seconds of each
//! other are the same recording, and one of them is kept: the one in the
//! Collection folder, then a lossless file over a lossy one, then the
//! higher bitrate, then the one with cues or a grid, then the most plays.
//! Copies of different lengths are versions (an edit and an extended mix),
//! which the DJ looks at before anything happens.
//!
//! Merging moves everything the other copies had onto the one kept (their
//! lists, history, cues, plays, rating and any tags it lacks), takes them
//! out of the library, and moves their files to a quarantine folder,
//! which only the DJ empties.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use opendj_engine::{Source, Track};
use rusqlite::{params, OptionalExtension};

use crate::query::normalized;
use crate::stages::{Folders, Stage};
use crate::{now, path_text, row_id, Error, Library};

/// Copies whose lengths are this close (in seconds) are the same recording.
pub const SAME_LENGTH: f64 = 3.0;

/// Copies whose lengths are this close (in seconds) are surely the same
/// recording; up to [`SAME_LENGTH`] they likely are.
pub const SURE_LENGTH: f64 = 1.0;

/// How sure a group's match is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Match {
    /// The same file, or the same name and length to the second: merged
    /// without a look.
    Sure,
    /// The same name and nearly the same length: worth a look first.
    Likely,
    /// Different lengths: versions the DJ decides about.
    Versions,
}

impl Match {
    pub fn title(self) -> &'static str {
        match self {
            Match::Sure => "Sure",
            Match::Likely => "Likely",
            Match::Versions => "Versions",
        }
    }
}

/// The name given to the quarantine folder, beside the stage folders.
pub const QUARANTINE: &str = "_Quarantine";

/// What tells a group's copies apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Case {
    /// The same file, byte for byte, in two places.
    SameFile,
    /// The same recording in a better format: lossless, or a higher bitrate.
    BetterCopy,
    /// The same recording in the same format, in different folders.
    SameFormat,
    /// Different lengths: an edit and an extended mix, say.
    Versions,
}

impl Case {
    pub const ALL: [Case; 4] = [
        Case::SameFile,
        Case::BetterCopy,
        Case::SameFormat,
        Case::Versions,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Case::SameFile => "Same file",
            Case::BetterCopy => "Better copy",
            Case::SameFormat => "Same format",
            Case::Versions => "Different lengths",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Case::SameFile => "The same file in two places. The one in your Play Ready folder is kept.",
            Case::BetterCopy => "A lossless or higher-bitrate copy is kept.",
            Case::SameFormat => "Same format in different folders. The one in your Play Ready folder is kept.",
            Case::Versions => "Different lengths, so maybe an edit and an extended mix. Link them as versions, or keep one.",
        }
    }

    /// Whether merging is safe without looking: nothing is lost.
    pub fn easy(self) -> bool {
        self != Case::Versions
    }
}

/// What a copy's file is like, for picking the one to keep.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FileFacts {
    /// Bytes on disk; 0 if unknown.
    pub size: u64,
    pub lossless: bool,
    /// Kilobits per second; 0 if unknown.
    pub bitrate: u32,
}

/// Whether a file of this format (its extension, upper case) with this
/// bit depth is lossless: FLAC, WAV, AIFF, or ALAC in an M4A.
pub fn lossless(format: &str, bit_depth: Option<u8>) -> bool {
    match format {
        "FLAC" | "WAV" | "AIF" | "AIFF" | "WV" | "APE" => true,
        "M4A" | "MP4" => bit_depth.is_some(),
        _ => false,
    }
}

/// Copies of the same track found among `tracks`: the same artist, title
/// and remix, or the same sound (pairs of ids in `alike`, see
/// [`same_sound`]), two or more of them. Missing files are left out, and
/// so are tracks without a title unless they sound like another.
pub fn copies<'a>(
    tracks: impl IntoIterator<Item = &'a Track>,
    alike: &[(usize, usize)],
) -> Vec<Vec<&'a Track>> {
    let sounding: HashSet<usize> = alike.iter().flat_map(|&(a, b)| [a, b]).collect();
    let mut by_name: HashMap<(String, String, String), Vec<&Track>> = HashMap::new();
    for t in tracks {
        if t.info.missing || !matches!(t.source, Source::File(_)) {
            continue;
        }
        let key = if t.title.trim().is_empty() {
            if !sounding.contains(&t.id) {
                continue;
            }
            // Untitled: a group of its own until a sound joins it.
            (format!("#{}", t.id), String::new(), String::new())
        } else {
            (
                normalized(&t.artist),
                normalized(&t.title),
                normalized(&t.info.remix),
            )
        };
        by_name.entry(key).or_default().push(t);
    }
    // Groups holding tracks that sound alike become one.
    let groups: Vec<Vec<&Track>> = by_name.into_values().collect();
    let mut at: HashMap<usize, usize> = HashMap::new();
    for (i, g) in groups.iter().enumerate() {
        for t in g {
            at.insert(t.id, i);
        }
    }
    let mut parent: Vec<usize> = (0..groups.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for &(a, b) in alike {
        if let (Some(&x), Some(&y)) = (at.get(&a), at.get(&b)) {
            let (x, y) = (root(&mut parent, x), root(&mut parent, y));
            if x != y {
                parent[y] = x;
            }
        }
    }
    let mut joined: HashMap<usize, Vec<&Track>> = HashMap::new();
    for (i, g) in groups.into_iter().enumerate() {
        let r = root(&mut parent, i);
        joined.entry(r).or_default().extend(g);
    }
    let mut out: Vec<Vec<&Track>> = joined.into_values().filter(|g| g.len() > 1).collect();
    for g in &mut out {
        g.sort_by_key(|t| t.id);
    }
    out.sort_by_key(|g| g[0].id);
    out
}

/// A group of copies, the one to keep first.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub case: Case,
    /// How sure the match is.
    pub matched: Match,
    /// Track ids, the one kept first.
    pub tracks: Vec<usize>,
}

impl Group {
    /// Whether merging it is safe without a look: the same recording, and
    /// a sure match.
    pub fn easy(&self) -> bool {
        self.case.easy() && self.matched == Match::Sure
    }
}

/// Which of two copies is better to keep: `Less` when `a` is.
fn rank(a: &Track, fa: &FileFacts, b: &Track, fb: &FileFacts, folders: &Folders) -> Ordering {
    let in_collection = |t: &Track| {
        t.info.stage == Stage::Collection.name()
            || matches!(&t.source, Source::File(p) if folders.stage_of(p) == Some(Stage::Collection))
    };
    let prepared = |t: &Track| !t.info.hotcues.is_empty() || t.info.fixed_grid;
    in_collection(b)
        .cmp(&in_collection(a))
        .then(fb.lossless.cmp(&fa.lossless))
        .then(fb.bitrate.cmp(&fa.bitrate))
        .then(prepared(b).cmp(&prepared(a)))
        .then(b.info.play_count.cmp(&a.info.play_count))
        .then(a.id.cmp(&b.id))
}

/// Sorts copies of one track into groups by case, each with the copy to
/// keep first. `facts` tells about each file; `same_bytes` whether two
/// files are identical; `linked` whether two tracks are linked (see
/// [`Library::link`]): recordings linked to each other are versions the
/// DJ keeps, so they make no group.
pub fn groups(
    copies: &[&Track],
    folders: &Folders,
    facts: impl Fn(&Track) -> FileFacts,
    same_bytes: impl Fn(&Track, &Track) -> bool,
    linked: impl Fn(usize, usize) -> bool,
    alike: impl Fn(usize, usize) -> bool,
) -> Vec<Group> {
    let facts: HashMap<usize, FileFacts> = copies.iter().map(|t| (t.id, facts(t))).collect();
    let ranked = |list: &mut Vec<&Track>| {
        list.sort_by(|a, b| rank(a, &facts[&a.id], b, &facts[&b.id], folders));
    };
    // Recordings: copies within a few seconds of each other.
    let mut by_length: Vec<&Track> = copies.to_vec();
    by_length.sort_by(|a, b| a.duration.total_cmp(&b.duration));
    let mut recordings: Vec<Vec<&Track>> = Vec::new();
    for t in by_length {
        match recordings.last_mut() {
            Some(r)
                if t.duration <= 0.0
                    || r.last()
                        .is_some_and(|l| t.duration - l.duration <= SAME_LENGTH) =>
            {
                r.push(t)
            }
            _ => recordings.push(vec![t]),
        }
    }
    let recordings_ids: Vec<Vec<usize>> = recordings
        .iter()
        .map(|r| r.iter().map(|t| t.id).collect())
        .collect();
    let mut out = Vec::new();
    let mut kept = Vec::new();
    for mut r in recordings {
        ranked(&mut r);
        let keep = r[0];
        kept.push(keep);
        if r.len() < 2 {
            continue;
        }
        let others = &r[1..];
        let case = if others.iter().all(|o| same_bytes(keep, o)) {
            Case::SameFile
        } else if others.iter().any(|o| {
            let (k, o) = (&facts[&keep.id], &facts[&o.id]);
            k.lossless != o.lossless || k.bitrate != o.bitrate
        }) {
            Case::BetterCopy
        } else {
            Case::SameFormat
        };
        let close = others.iter().all(|o| {
            keep.duration > 0.0
                && o.duration > 0.0
                && (keep.duration - o.duration).abs() <= SURE_LENGTH
        });
        let sound = others.iter().all(|o| alike(keep.id, o.id));
        let matched = if case == Case::SameFile || close || sound {
            Match::Sure
        } else {
            Match::Likely
        };
        out.push(Group {
            case,
            matched,
            tracks: r.iter().map(|t| t.id).collect(),
        });
    }
    let all_linked = recordings_linked(&recordings_ids, &linked);
    if kept.len() > 1 && !all_linked {
        ranked(&mut kept);
        out.push(Group {
            case: Case::Versions,
            matched: Match::Versions,
            tracks: kept.iter().map(|t| t.id).collect(),
        });
    }
    out
}

/// Fingerprint items skipped at the start (fades and silence), and how
/// many after them are compared.
const SOUND_SKIP: usize = 20;
const SOUND_ITEMS: usize = 400;
/// Items shared by more tracks than this say nothing (silence, a tone).
const SOUND_COMMON: usize = 12;

/// Pairs of tracks that sound the same, from their Chromaprint
/// fingerprints (by track id): many of their fingerprint items are equal,
/// and lined up, the rest differ in only a few bits. Re-encodes and
/// renamed copies match; different mixes don't.
pub fn same_sound(prints: &HashMap<usize, Vec<u32>>) -> Vec<(usize, usize)> {
    let sample = |p: &[u32]| -> Vec<u32> {
        p.iter()
            .skip(SOUND_SKIP)
            .take(SOUND_ITEMS)
            .copied()
            .collect()
    };
    let mut index: HashMap<u32, Vec<usize>> = HashMap::new();
    for (&id, p) in prints {
        let items: HashSet<u32> = sample(p).into_iter().collect();
        for v in items {
            index.entry(v).or_default().push(id);
        }
    }
    let mut shared: HashMap<(usize, usize), usize> = HashMap::new();
    for ids in index
        .values()
        .filter(|ids| ids.len() > 1 && ids.len() <= SOUND_COMMON)
    {
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                *shared.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    let mut out: Vec<(usize, usize)> = shared
        .into_iter()
        .filter(|&((a, b), n)| {
            let (pa, pb) = (sample(&prints[&a]), sample(&prints[&b]));
            let fewer = pa.len().min(pb.len());
            fewer >= 50 && n * 5 >= fewer && close_prints(&pa, &pb)
        })
        .map(|(pair, _)| pair)
        .collect();
    out.sort_unstable();
    out
}

/// Whether two fingerprints, lined up at their best offset (a few items
/// either way), differ in few bits on average.
fn close_prints(a: &[u32], b: &[u32]) -> bool {
    (-8i32..=8)
        .filter_map(|shift| {
            let pairs: Vec<(u32, u32)> = a
                .iter()
                .enumerate()
                .filter_map(|(i, &x)| {
                    let j = i as i32 + shift;
                    (j >= 0)
                        .then(|| b.get(j as usize).map(|&y| (x, y)))
                        .flatten()
                })
                .collect();
            (pairs.len() >= 50).then(|| {
                pairs.iter().map(|(x, y)| (x ^ y).count_ones()).sum::<u32>() as f64
                    / pairs.len() as f64
            })
        })
        .any(|bits| bits < 6.0)
}

/// Whether every two recordings have a link between them.
fn recordings_linked(recordings: &[Vec<usize>], linked: impl Fn(usize, usize) -> bool) -> bool {
    recordings.iter().enumerate().all(|(i, a)| {
        recordings[i + 1..]
            .iter()
            .all(|b| a.iter().any(|&x| b.iter().any(|&y| linked(x, y))))
    })
}

/// Whether two files hold the same bytes. Files of different sizes, or
/// that can't be read, don't.
pub fn same_bytes(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() || ma.len() == 0 {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let (Ok(na), Ok(nb)) = (fa.read(&mut ba), fb.read(&mut bb)) else {
            return false;
        };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

/// Where merged copies' files go today: `_Quarantine/<date>` beside the
/// Collection folder. `None` without a Collection folder.
pub fn quarantine_dir(folders: &Folders, date: &str) -> Option<PathBuf> {
    Some(quarantine_root(folders)?.join(date))
}

/// The quarantine folder, beside the Collection folder.
pub fn quarantine_root(folders: &Folders) -> Option<PathBuf> {
    Some(folders.collection.as_ref()?.parent()?.join(QUARANTINE))
}

/// Text columns a kept copy takes from the others when it has none.
const FILL_TEXT: [&str; 13] = [
    "artist", "title", "album", "genre", "key", "comment", "grouping", "remix", "year", "label",
    "composer", "remixer", "isrc",
];

impl Library {
    /// Merges copies of a track into the one kept: it takes their lists and
    /// history, cues and markers (when it has none), links, the highest
    /// rating, all their plays, the earliest date added, the latest play,
    /// and any tags, colour, gain, tempo or grid it lacks. The other copies
    /// leave the library ([`Library::remove_tracks`]) and, with `quarantine`
    /// given, their files' moves there are queued as one batch, which is
    /// returned (made by [`Library::apply_moves`], taken back by
    /// [`Library::undo_batch`]). `batch` adds the moves to a batch made by
    /// an earlier merge, so several groups merged at once undo together.
    pub fn merge_copies(
        &mut self,
        keep: usize,
        others: &[usize],
        quarantine: Option<&Path>,
        batch: Option<i64>,
    ) -> Result<Option<i64>, Error> {
        let Some(k) = row_id(keep) else {
            return Ok(None);
        };
        let others: Vec<i64> = others
            .iter()
            .filter_map(|&o| row_id(o))
            .filter(|&o| o != k)
            .collect();
        if others.is_empty() {
            return Ok(None);
        }
        let tx = self.db.transaction()?;
        for &o in &others {
            // Every copy's tags, after the kept one's own.
            tx.execute(
                "INSERT OR IGNORE INTO track_tags (track_id, tag_id, position)
                 SELECT ?1, tag_id, position + 1000 FROM track_tags WHERE track_id = ?2",
                params![k, o],
            )?;
            for col in FILL_TEXT {
                tx.execute(
                    &format!(
                        "UPDATE tracks SET {col} = (SELECT {col} FROM tracks WHERE id = ?2)
                         WHERE id = ?1 AND {col} = ''"
                    ),
                    params![k, o],
                )?;
            }
            tx.execute(
                "UPDATE tracks SET
                    rating = MAX(rating, (SELECT rating FROM tracks WHERE id = ?2)),
                    play_count = play_count + (SELECT play_count FROM tracks WHERE id = ?2),
                    added = CASE
                        WHEN added = 0 THEN (SELECT added FROM tracks WHERE id = ?2)
                        WHEN (SELECT added FROM tracks WHERE id = ?2) = 0 THEN added
                        ELSE MIN(added, (SELECT added FROM tracks WHERE id = ?2)) END,
                    last_played = MAX(COALESCE(last_played, 0),
                                      COALESCE((SELECT last_played FROM tracks WHERE id = ?2), 0)),
                    color = COALESCE(color, (SELECT color FROM tracks WHERE id = ?2)),
                    gain = COALESCE(gain, (SELECT gain FROM tracks WHERE id = ?2)),
                    track_number = CASE WHEN track_number = 0
                        THEN (SELECT track_number FROM tracks WHERE id = ?2) ELSE track_number END,
                    disc_number = CASE WHEN disc_number = 0
                        THEN (SELECT disc_number FROM tracks WHERE id = ?2) ELSE disc_number END,
                    bpm = CASE WHEN bpm <= 0 THEN (SELECT bpm FROM tracks WHERE id = ?2) ELSE bpm END,
                    key_source = CASE WHEN key_source IS NULL
                        THEN (SELECT key_source FROM tracks WHERE id = ?2) ELSE key_source END
                 WHERE id = ?1",
                params![k, o],
            )?;
            tx.execute(
                "UPDATE tracks SET last_played = NULLIF(last_played, 0) WHERE id = ?1",
                [k],
            )?;
            tx.execute(
                "UPDATE tracks SET grid = (SELECT grid FROM tracks WHERE id = ?2),
                        grid_source = (SELECT grid_source FROM tracks WHERE id = ?2)
                 WHERE id = ?1 AND grid IS NULL",
                params![k, o],
            )?;
            let has = |table: &str| -> Result<bool, Error> {
                tx.query_row(
                    &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE track_id = ?1)"),
                    [k],
                    |r| r.get(0),
                )
            };
            if !has("cues")? {
                tx.execute(
                    "UPDATE cues SET track_id = ?1 WHERE track_id = ?2",
                    params![k, o],
                )?;
            }
            if !has("markers")? {
                tx.execute(
                    "UPDATE markers SET track_id = ?1 WHERE track_id = ?2",
                    params![k, o],
                )?;
            }
            tx.execute(
                "UPDATE playlist_tracks SET track_id = ?1 WHERE track_id = ?2",
                params![k, o],
            )?;
            tx.execute(
                "UPDATE OR IGNORE track_links SET track = ?1 WHERE track = ?2",
                params![k, o],
            )?;
            tx.execute(
                "UPDATE OR IGNORE track_links SET linked = ?1 WHERE linked = ?2",
                params![k, o],
            )?;
            tx.execute(
                "DELETE FROM track_links WHERE track = ?1 OR linked = ?1 OR track = linked",
                [o],
            )?;
            tx.execute(
                "UPDATE wanted SET track_id = ?1 WHERE track_id = ?2",
                params![k, o],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO removed (track_id, removed) VALUES (?1, ?2)",
                params![o, now()],
            )?;
        }
        // A list holds the kept copy once; history keeps every play.
        tx.execute(
            "DELETE FROM playlist_tracks
             WHERE track_id = ?1
               AND playlist_id IN (SELECT id FROM playlists WHERE kind IN (0, 5))
               AND EXISTS (SELECT 1 FROM playlist_tracks p
                           WHERE p.playlist_id = playlist_tracks.playlist_id
                             AND p.track_id = ?1 AND p.position < playlist_tracks.position)",
            [k],
        )?;
        let batch = match quarantine {
            None => None,
            Some(dir) => Some(queue_quarantine(&tx, &others, dir, batch)?),
        };
        tx.commit()?;
        Ok(batch)
    }
}

impl Library {
    /// Takes tracks out of the library ([`Library::remove_tracks`]) and,
    /// with `quarantine` given, queues their files' moves there as one
    /// batch, which is returned (made by [`Library::apply_moves`], taken
    /// back by [`Library::undo_batch`]). Returns `None` when no track was
    /// taken out.
    pub fn quarantine_tracks(
        &mut self,
        ids: &[usize],
        quarantine: Option<&Path>,
    ) -> Result<Option<i64>, Error> {
        let rows: Vec<i64> = ids.iter().filter_map(|&id| row_id(id)).collect();
        if rows.is_empty() {
            return Ok(None);
        }
        let tx = self.db.transaction()?;
        let at = now();
        for &row in &rows {
            tx.execute(
                "INSERT OR IGNORE INTO removed (track_id, removed) VALUES (?1, ?2)",
                params![row, at],
            )?;
        }
        let batch = match quarantine {
            None => None,
            Some(dir) => Some(queue_quarantine(&tx, &rows, dir, None)?),
        };
        tx.commit()?;
        Ok(batch)
    }
}

/// Queues moves of these tracks' files (library rows) to `dir`, as
/// `batch` or a new one, and returns the batch. Names already in `dir`, or
/// taken by another file of the batch, get " (2)" and so on.
fn queue_quarantine(
    tx: &rusqlite::Transaction,
    rows: &[i64],
    dir: &Path,
    batch: Option<i64>,
) -> Result<i64, Error> {
    let batch: i64 = match batch {
        Some(b) => b,
        None => tx.query_row(
            "SELECT COALESCE(MAX(batch), 0) + 1 FROM file_moves",
            [],
            |r| r.get(0),
        )?,
    };
    let mut taken: HashSet<PathBuf> = HashSet::new();
    for &o in rows {
        let Some((path, stage)): Option<(String, String)> = tx
            .query_row("SELECT path, stage FROM tracks WHERE id = ?1", [o], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?
        else {
            continue;
        };
        let to = free_place(dir, Path::new(&path), |p| p.exists() || taken.contains(p));
        taken.insert(to.clone());
        tx.execute(
            "DELETE FROM file_moves WHERE track_id = ?1 AND done IS NULL AND undone IS NULL",
            [o],
        )?;
        tx.execute(
            "INSERT INTO file_moves (batch, track_id, from_path, to_path, from_stage, to_stage, queued)
             VALUES (?1, ?2, ?3, ?4, ?5, 'quarantine', ?6)",
            params![batch, o, path, path_text(&to), stage, now()],
        )?;
        tx.execute("UPDATE tracks SET stage = 'quarantine' WHERE id = ?1", [o])?;
    }
    Ok(batch)
}

/// A place in `dir` for the file at `path`, with " (2)" and so on added to
/// its name while `taken` says the place is in use.
fn free_place(dir: &Path, path: &Path, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let stem = path
        .file_stem()
        .map_or_else(|| "track".into(), |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut to = dir.join(format!("{stem}{ext}"));
    let mut n = 2;
    while taken(&to) {
        to = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    to
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::file_track;
    use opendj_engine::Hotcue;

    fn copy(id: usize, path: &str, seconds: f64) -> Track {
        let mut t = file_track(path, "Clouds");
        t.id = id;
        t.duration = seconds;
        t
    }

    fn folders() -> Folders {
        Folders {
            inbox: vec![PathBuf::from("/m/Downloads")],
            triage: Some(PathBuf::from("/m/DB/_Staging")),
            collection: Some(PathBuf::from("/m/DB/Library")),
            albums: None,
        }
    }

    #[test]
    fn groups_copies_by_case_and_keeps_the_best() {
        let a = copy(1, "/m/DB/_Staging/Clouds.mp3", 271.0);
        let b = copy(2, "/m/DB/Library/Clouds.mp3", 272.0);
        let c = copy(3, "/m/DB/_Staging/Clouds.flac", 271.5);
        let d = copy(4, "/m/DB/Library/Clouds (edit).mp3", 258.0);
        let all = [&a, &b, &c, &d];
        assert_eq!(copies(all, &[]).len(), 1);
        let facts = |t: &Track| FileFacts {
            size: 100,
            lossless: t.id == 3,
            bitrate: if t.id == 3 { 900 } else { 320 },
        };
        let groups = groups(
            &all,
            &folders(),
            facts,
            |_, _| false,
            |_, _| false,
            |_, _| false,
        );
        // The Collection copy wins over the lossless one in _Staging.
        assert_eq!(
            groups,
            [
                Group {
                    case: Case::Versions,
                    matched: Match::Versions,
                    tracks: vec![2, 4]
                },
                Group {
                    case: Case::BetterCopy,
                    matched: Match::Sure,
                    tracks: vec![2, 3, 1]
                },
            ]
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
        );
        // Versions linked to each other are left alone.
        let linked = super::groups(
            &all,
            &folders(),
            facts,
            |_, _| false,
            |a, b| a + b == 6,
            |_, _| false,
        );
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].case, Case::BetterCopy);
        // Identical files.
        let same = groups_of(&[&a, &b], |_, _| true);
        assert_eq!(same[0].case, Case::SameFile);
        assert_eq!(same[0].matched, Match::Sure);
        let format = groups_of(&[&a, &b], |_, _| false);
        assert_eq!(format[0].case, Case::SameFormat);
        assert_eq!(format[0].tracks, [2, 1]);
        // A second apart is sure; two seconds is likely.
        assert_eq!(format[0].matched, Match::Sure);
        assert!(format[0].easy());
        let near = copy(5, "/m/DB/_Staging/Clouds 2.mp3", 273.0);
        let likely = groups_of(&[&a, &near], |_, _| false);
        assert_eq!(likely[0].matched, Match::Likely);
        assert!(!likely[0].easy());
    }

    fn groups_of(t: &[&Track], same: impl Fn(&Track, &Track) -> bool) -> Vec<Group> {
        groups(
            t,
            &folders(),
            |_| FileFacts::default(),
            same,
            |_, _| false,
            |_, _| false,
        )
    }

    #[test]
    fn cues_and_plays_break_ties() {
        let mut a = copy(1, "/m/x/a.mp3", 200.0);
        let b = copy(2, "/m/y/a.mp3", 200.0);
        a.info.hotcues.push(Hotcue {
            number: 1,
            seconds: 1.0,
            ..Default::default()
        });
        assert_eq!(groups_of(&[&b, &a], |_, _| false)[0].tracks, [1, 2]);
    }

    #[test]
    fn copies_by_sound() {
        // A renamed file with no tags sounds like a tagged FLAC.
        let flac = copy(1, "/m/DB/Library/Clouds.flac", 271.0);
        let mut renamed = copy(2, "/m/Downloads/Track 03.mp3", 271.0);
        renamed.title.clear();
        renamed.artist.clear();
        assert!(copies([&flac, &renamed], &[]).is_empty());
        let found = copies([&flac, &renamed], &[(1, 2)]);
        assert_eq!(found.len(), 1);
        let facts = |t: &Track| FileFacts {
            size: 1,
            lossless: t.id == 1,
            bitrate: if t.id == 1 { 900 } else { 128 },
        };
        let g = groups(
            &found[0],
            &folders(),
            facts,
            |_, _| false,
            |_, _| false,
            |_, _| true,
        );
        assert_eq!(g[0].tracks, [1, 2]);
        assert_eq!(g[0].matched, Match::Sure);
    }

    #[test]
    fn same_sound_matches_close_prints_only() {
        // A made-up fingerprint, a copy with a few bits flipped, and another.
        let mut seed = 7u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        let a: Vec<u32> = (0..500).map(|_| next()).collect();
        let b: Vec<u32> = a
            .iter()
            .enumerate()
            .map(|(i, &v)| if i % 3 == 0 { v ^ 0b101 } else { v })
            .collect();
        let c: Vec<u32> = (0..500).map(|_| next()).collect();
        let prints = HashMap::from([(1, a), (2, b), (3, c)]);
        assert_eq!(same_sound(&prints), [(1, 2)]);
    }

    #[test]
    fn different_remixes_are_not_copies() {
        let a = copy(1, "/m/a.mp3", 200.0);
        let mut b = copy(2, "/m/b.mp3", 200.0);
        b.info.remix = "Club Mix".into();
        assert!(copies([&a, &b], &[]).is_empty());
    }

    #[test]
    fn compares_bytes() {
        let dir = std::env::temp_dir().join(format!("opendj-dupes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b, c) = (dir.join("a"), dir.join("b"), dir.join("c"));
        std::fs::write(&a, b"same bytes").unwrap();
        std::fs::write(&b, b"same bytes").unwrap();
        std::fs::write(&c, b"diff bytes").unwrap();
        assert!(same_bytes(&a, &b));
        assert!(!same_bytes(&a, &c));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn merging_moves_everything_onto_the_kept_copy() {
        let mut lib = Library::in_memory().unwrap();
        let keep = lib
            .add_file(&file_track("/m/DB/Library/Clouds.flac", "Clouds"))
            .unwrap();
        let mut other = file_track("/m/DB/_Staging/Clouds.mp3", "Clouds");
        other.album = "Interloper".into();
        let other = lib.add_file(&other).unwrap();
        lib.set_rating(other.id, 5).unwrap();
        lib.save_cues(
            other.id,
            &[Hotcue {
                number: 1,
                seconds: 12.0,
                ..Default::default()
            }],
        )
        .unwrap();
        let list = lib
            .create_playlist("Warm-up", None, crate::PlaylistKind::List)
            .unwrap();
        lib.add_to_playlist(list, &[keep.id, other.id]).unwrap();
        lib.link(other.id, keep.id).unwrap();

        let q = PathBuf::from("/q/2026-10-28");
        let batch = lib
            .merge_copies(keep.id, &[other.id], Some(&q), None)
            .unwrap();
        assert!(batch.is_some());
        let left = lib.all_tracks(|_| true).unwrap();
        assert_eq!(left.len(), 1);
        let k = &left[0];
        assert_eq!(
            (k.id, k.rating, k.album.as_str()),
            (keep.id, 5, "Interloper")
        );
        assert_eq!(k.info.hotcues.len(), 1);
        // The list holds it once, and it isn't linked to itself.
        assert_eq!(lib.playlist_tracks(list).unwrap(), [keep.id]);
        assert!(lib.linked(keep.id).unwrap().is_empty());
        // The other file's move to quarantine waits.
        let moves = lib.pending_moves().unwrap();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].to, q.join("Clouds.mp3"));
        assert_eq!(moves[0].to_stage, "quarantine");
    }

    #[test]
    fn quarantined_tracks_leave_and_come_back() {
        let mut lib = Library::in_memory().unwrap();
        let a = lib
            .add_file(&file_track("/m/DB/_Staging/Rain.mp3", "Rain"))
            .unwrap();
        let b = lib
            .add_file(&file_track("/m/Inbox/Rain.mp3", "Rain"))
            .unwrap();
        let q = PathBuf::from("/q/2026-10-28");
        let batch = lib
            .quarantine_tracks(&[a.id, b.id], Some(&q))
            .unwrap()
            .unwrap();
        assert!(lib.all_tracks(|_| true).unwrap().is_empty());
        // Both wait to move, the second under another name.
        let mut to: Vec<PathBuf> = lib
            .pending_moves()
            .unwrap()
            .into_iter()
            .map(|m| m.to)
            .collect();
        to.sort();
        assert_eq!(to, [q.join("Rain (2).mp3"), q.join("Rain.mp3")]);
        lib.undo_batch(batch).unwrap();
        lib.restore_tracks(Some(&[a.id, b.id])).unwrap();
        assert_eq!(lib.all_tracks(|_| true).unwrap().len(), 2);
        assert!(lib.pending_moves().unwrap().is_empty());
    }

    #[test]
    fn quarantine_names_never_collide() {
        let taken = [PathBuf::from("/q/a.mp3"), PathBuf::from("/q/a (2).mp3")];
        let to = free_place(Path::new("/q"), Path::new("/x/a.mp3"), |p| {
            taken.iter().any(|t| t == p)
        });
        assert_eq!(to, PathBuf::from("/q/a (3).mp3"));
        assert_eq!(
            quarantine_dir(&folders(), "2026-10-28"),
            Some(PathBuf::from("/m/DB/_Quarantine/2026-10-28"))
        );
    }
}
