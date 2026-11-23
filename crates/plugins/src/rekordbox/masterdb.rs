//! rekordbox 6 and 7's own library, `master.db` (SQLCipher, opened with
//! rbox): its tracks with their tags, cues and loops, and from each track's
//! analysis files its beat grid and phrases; its playlists, folders,
//! intelligent playlists and history.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opendj_engine::import::{ImportedEntry, ImportedList, ImportedListKind, StreamedTrack};
use opendj_engine::{Hotcue, LibraryInfo, Source, Track, MEMORY_CUE};
use rbox_masterdb::{
    DjmdContent, DjmdContentView, HistoryType, LogicalOperator, MasterDb, PlaylistType, SmartList,
};

use super::common::{self, Condition, Mood};
use super::dates;
use super::{HISTORY, PLAYLISTS, SMART};
use crate::ImportedLibrary;

/// A track's analysis files: the beat grid's, and the phrases' if any.
struct AnlzFiles {
    dat: rbox_anlz::Anlz,
    ext: Option<rbox_anlz::Anlz>,
}

/// An open library: rekordbox's own file, or a private copy with its dates
/// fixed when rbox can't read some of them (see [`dates`]), and the folder
/// its analysis files (beat grids, phrases) are in, if one was found.
struct Db {
    db: MasterDb,
    share: Option<PathBuf>,
    _copy: Option<dates::FixedCopy>,
}

impl std::ops::Deref for Db {
    type Target = MasterDb;
    fn deref(&self) -> &MasterDb {
        &self.db
    }
}

impl std::ops::DerefMut for Db {
    fn deref_mut(&mut self) -> &mut MasterDb {
        &mut self.db
    }
}

/// Where the analysis files for the database at `path` are: rekordbox's
/// `share` folder next to it, or, for a copy kept somewhere else (a backup,
/// another drive), the one rekordbox itself uses on this computer.
fn share_dir(path: &Path) -> Option<PathBuf> {
    let own = || {
        rbox_core::locate::locate_library()
            .ok()
            .map(|l| l.analysis_root)
    };
    share_dir_with(path, own)
}

fn share_dir_with(path: &Path, own: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    let next = path.parent().unwrap_or(Path::new(".")).join("share");
    if next.is_dir() {
        return Some(next);
    }
    own().filter(|p| p.is_dir())
}

fn open(path: &Path) -> Result<Db, String> {
    let cant = |e: &dyn std::fmt::Display| format!("can't open rekordbox's database: {e}");
    let share = share_dir(path);
    // A check that fails (an unexpected layout) leaves it to rbox.
    let copy = if dates::needs_fixing(path).unwrap_or(false) {
        Some(dates::FixedCopy::of(path).map_err(|e| cant(&e))?)
    } else {
        None
    };
    let file = copy.as_ref().map_or(path, |c| c.db.as_path());
    let db = match &share {
        Some(share) => {
            let options =
                rbox_core::RekordboxOptions::new(file.to_path_buf(), share.clone(), PathBuf::new())
                    .map_err(|e| cant(&e))?;
            MasterDb::from_options(&options).map_err(|e| cant(&e))?
        }
        None => MasterDb::new(file).map_err(|e| cant(&e))?,
    };
    Ok(Db {
        db,
        share,
        _copy: copy,
    })
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// What `master.db` holds, as a line for the wizard, with how many of its
/// tracks have a beat grid to bring in.
pub fn preview(path: &Path) -> Result<String, String> {
    let mut db = open(path)?;
    let contents: Vec<DjmdContent> = db
        .get_contents()
        .map_err(err)?
        .into_iter()
        .filter(|c| c.rb_local_deleted == 0)
        .collect();
    let lists = read_lists(&mut db, &[PLAYLISTS, SMART, HISTORY], &HashMap::new())?;
    let count = |f: fn(&ImportedListKind) -> bool| lists.iter().filter(|l| f(&l.kind)).count();
    let summary = super::summary(
        contents.len(),
        count(|k| matches!(k, ImportedListKind::List)),
        count(|k| matches!(k, ImportedListKind::Smart { .. })),
        count(|k| matches!(k, ImportedListKind::History { .. })),
    );
    let grids = db.share.as_deref().map(|share| {
        contents
            .iter()
            .filter(|c| anlz_file(share, c.analysis_data_path.as_deref()).is_some())
            .count()
    });
    Ok(format!("{summary} {}", grids_line(grids, contents.len())))
}

/// rekordbox's analysis file (`ANLZ0000.DAT`) for a track, from where
/// master.db says it is (`data`, under `share`), when it's there.
fn anlz_file(share: &Path, data: Option<&str>) -> Option<PathBuf> {
    let data = data?.trim_start_matches(['/', '\\']);
    if data.is_empty() {
        return None;
    }
    Some(share.join(data)).filter(|p| p.is_file())
}

/// How many of `tracks` have a beat grid to bring in, or why none will.
fn grids_line(grids: Option<usize>, tracks: usize) -> String {
    match grids {
        None => "No beat grids: rekordbox's analysis folder (share) isn't next to this file. \
                 Choose the master.db in rekordbox's own folder."
            .to_string(),
        Some(n) if n == tracks && n > 0 => "Beat grids for every track.".to_string(),
        Some(n) => format!("Beat grids for {n} of the tracks."),
    }
}

/// The tracks in `master.db` whose files `exists` says are here, and the
/// lists in `groups`.
pub fn read(
    path: &Path,
    groups: &[&str],
    exists: impl Fn(&Path) -> bool,
) -> Result<ImportedLibrary, String> {
    let mut db = open(path)?;
    let contents: Vec<DjmdContentView> = db
        .get_contents_with_relations()
        .map_err(err)?
        .into_iter()
        .filter(|c| c.rb_local_deleted == 0)
        .collect();
    let mut cues: HashMap<String, Vec<rbox_masterdb::DjmdCue>> = HashMap::new();
    for c in db.get_cues().map_err(err)? {
        if c.rb_local_deleted == 0 {
            cues.entry(c.content_id.clone()).or_default().push(c);
        }
    }
    let my_tags = db.get_my_tags().map_err(err)?;
    let tag_names: HashMap<String, String> = my_tags
        .iter()
        .map(|t| (t.id.clone(), t.name.clone()))
        .collect();
    // Each My Tag category (a top-level folder) becomes one of the DJ's
    // tag groups.
    let category = |t: &rbox_masterdb::DjmdMyTag| {
        tag_names
            .get(&t.parent_id)
            .cloned()
            .unwrap_or_else(|| "My Tag".to_string())
    };
    let mut labels: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for tag in &my_tags {
        if tag.rb_local_deleted != 0 {
            continue;
        }
        for s in db.get_my_tag_songs(&tag.id).map_err(err)? {
            labels
                .entry(s.content_id)
                .or_default()
                .push((category(tag), tag.name.clone()));
        }
    }
    let mut tracks = Vec::new();
    for c in &contents {
        let Some(file) = file_path(c) else {
            continue;
        };
        if !exists(&file) {
            continue;
        }
        let mut t = track(c, file, cues.remove(&c.id).unwrap_or_default());
        t.info.imported_tags = labels.remove(&c.id).unwrap_or_default();
        analysis(
            db.share.as_deref(),
            c.content.analysis_data_path.as_deref(),
            &mut t,
        );
        tracks.push(t);
    }
    let grids = tracks.iter().filter(|t| t.info.fixed_grid).count();
    eprintln!(
        "rekordbox: beat grids for {grids} of {} tracks (analysis files in {})",
        tracks.len(),
        db.share
            .as_deref()
            .map_or("no folder found".into(), |p| p.display().to_string())
    );
    let paths: HashMap<String, ImportedEntry> = contents
        .iter()
        .filter_map(|c| {
            let artist = c.artist.as_ref().map(|a| a.name.as_str());
            Some((c.id.clone(), entry(c, artist)?))
        })
        .collect();
    let lists = read_lists_with(&mut db, groups, &paths, &tag_names)?;
    Ok(ImportedLibrary { tracks, lists })
}

/// The lists in `groups`, for the picker.
pub fn lists(path: &Path, groups: &[&str]) -> Result<Vec<ImportedList>, String> {
    let mut db = open(path)?;
    let paths = entries_of(&mut db)?;
    let tag_names = db
        .get_my_tags()
        .map_err(err)?
        .into_iter()
        .map(|t| (t.id, t.name))
        .collect();
    read_lists_with(&mut db, groups, &paths, &tag_names)
}

fn read_lists(
    db: &mut MasterDb,
    groups: &[&str],
    tag_names: &HashMap<String, String>,
) -> Result<Vec<ImportedList>, String> {
    let paths = entries_of(db)?;
    read_lists_with(db, groups, &paths, tag_names)
}

/// What each track (by its id) is in a list: its file, or the streamed
/// track it stands for.
fn entries_of(db: &mut MasterDb) -> Result<HashMap<String, ImportedEntry>, String> {
    Ok(db
        .get_contents()
        .map_err(err)?
        .iter()
        .filter(|c| c.rb_local_deleted == 0)
        .filter_map(|c| Some((c.id.clone(), entry(c, None)?)))
        .collect())
}

/// The file of a track that has one here; streamed tracks have none.
fn file_path(c: &DjmdContentView) -> Option<PathBuf> {
    let p = c.folder_path.as_deref()?.trim();
    if p.is_empty() || StreamedTrack::parse(p).is_some() {
        return None;
    }
    Some(PathBuf::from(p))
}

/// A track in a list: its file, or for a track from a streaming service
/// (a `ServiceID`, or a service's address where the file would be), the
/// streamed track with what rekordbox knows of it. `artist` is its
/// artist's name when the caller has it.
fn entry(c: &DjmdContent, artist: Option<&str>) -> Option<ImportedEntry> {
    let path = c.folder_path.as_deref().unwrap_or_default().trim();
    let parsed = StreamedTrack::parse(path);
    let service = c.service_id.unwrap_or(0);
    if parsed.is_none() && (service == 0 || looks_like_a_file(path)) {
        return (!path.is_empty()).then(|| ImportedEntry::new(path, None));
    }
    let mut s = parsed.unwrap_or_else(|| StreamedTrack {
        service: "online".into(),
        uri: if path.is_empty() {
            format!("rekordbox:{service}:{}", c.id)
        } else {
            path.to_string()
        },
        ..Default::default()
    });
    let text = |v: &Option<String>| v.as_deref().unwrap_or_default().trim().to_string();
    if s.id.is_empty() && s.service != "online" {
        s.id = text(&c.src_id);
    }
    s.title = Some(text(&c.title))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| text(&c.src_title));
    s.artist = artist
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| text(&c.src_artist_name));
    s.duration = match c.length.unwrap_or(0) {
        l if l > 0 => l as f64,
        _ => c.src_length.unwrap_or(0).max(0) as f64 / 1000.0,
    };
    s.isrc = text(&c.isrc);
    Some(ImportedEntry {
        path: PathBuf::from(&s.uri),
        played_at: None,
        streamed: Some(s),
    })
}

/// A path on a disk: "/Users/…", "C:/…" or "C:\…".
fn looks_like_a_file(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/')
        || (b.len() > 2
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'/' || b[2] == b'\\'))
}

fn name(n: Option<&str>) -> String {
    n.unwrap_or_default().trim().to_string()
}

fn track(c: &DjmdContentView, file: PathBuf, mut cues: Vec<rbox_masterdb::DjmdCue>) -> Track {
    let bpm = c.bpm.unwrap_or(0) as f64 / 100.0;
    cues.sort_by_key(|q| q.in_msec);
    let mut hotcues = Vec::new();
    let mut memory = MEMORY_CUE;
    for q in &cues {
        let seconds = q.in_msec as f64 / 1000.0;
        let end = (q.out_msec > q.in_msec).then(|| q.out_msec as f64 / 1000.0);
        let label = q.comment.clone().unwrap_or_default();
        if q.kind == 0 {
            hotcues.push(Hotcue {
                number: memory,
                seconds,
                name: label,
                end,
                color: common::track_colour(q.color.into()),
            });
            memory += 1;
        } else if let Some(number) = common::hot_cue_number(q.kind.into()) {
            if hotcues.iter().any(|h: &Hotcue| h.number == number) {
                continue;
            }
            hotcues.push(Hotcue {
                number,
                seconds,
                name: label,
                end,
                color: q
                    .color_table_index
                    .and_then(|i| common::hot_cue_colour(i.into())),
            });
        }
    }
    hotcues.sort_by_key(|h| h.number);
    let artist = |a: &Option<rbox_masterdb::DjmdArtist>| name(a.as_ref().map(|a| a.name.as_str()));
    Track {
        id: 0,
        artist: artist(&c.artist),
        title: name(c.title.as_deref()),
        album: name(c.album.as_ref().map(|a| a.name.as_str())),
        genre: name(c.genre.as_ref().map(|g| g.name.as_str())),
        rating: c.rating.unwrap_or(0).clamp(0, 5) as u8,
        bpm,
        key: name(c.key.as_ref().map(|k| k.name.as_str())),
        duration: c.length.unwrap_or(0).max(0) as f64,
        first_beat: 0.0,
        grid: Default::default(),
        source: Source::File(file),
        waveform: Arc::default(),
        audio: None,
        cover: None,
        info: LibraryInfo {
            comment: name(c.commnt.as_deref()),
            color: c
                .color_id
                .as_deref()
                .and_then(|id| id.parse().ok())
                .and_then(common::track_colour),
            play_count: c.dj_play_count.unwrap_or(0).max(0) as u32,
            hotcues,
            year: c
                .release_year
                .filter(|y| *y > 0)
                .map(|y| y.to_string())
                .unwrap_or_default(),
            track_number: c.track_no.unwrap_or(0).max(0) as u32,
            disc_number: c.disc_no.unwrap_or(0).max(0) as u32,
            label: name(c.label.as_ref().map(|l| l.name.as_str())),
            composer: artist(&c.composer),
            remixer: artist(&c.remixer),
            remix: name(c.subtitle.as_deref()),
            isrc: name(c.isrc.as_deref()),
            added: c
                .stock_date
                .as_deref()
                .and_then(crate::virtualdj::lists::day_start)
                .unwrap_or(0),
            ..Default::default()
        },
    }
}

/// The beat grid and phrases from the track's analysis files, when
/// rekordbox has analysed it. Each file is read on its own (rbox's
/// `get_content_anlz_files` panics on an analysis path that doesn't start
/// with `/`, and gives up on the grid when a waveform file is unreadable).
fn analysis(share: Option<&Path>, data: Option<&str>, t: &mut Track) {
    let Some(dat) = share.and_then(|s| anlz_file(s, data)) else {
        return;
    };
    let ext = dat.with_extension("EXT");
    let mut files = match rbox_anlz::Anlz::load(&dat) {
        Ok(dat) => AnlzFiles {
            dat,
            ext: ext
                .is_file()
                .then(|| rbox_anlz::Anlz::load(&ext).ok())
                .flatten(),
        },
        Err(e) => {
            eprintln!("rekordbox: can't read {}: {e}", dat.display());
            return;
        }
    };
    let beats: Vec<(f64, f64)> = match files.dat.get_beat_grid() {
        Ok(Some(g)) => g
            .beats
            .iter()
            .map(|b| (b.time as f64 / 1000.0, b.tempo as f64 / 100.0))
            .collect(),
        _ => Vec::new(),
    };
    if !beats.is_empty() {
        t.set_grid(common::grid_from_beats(&beats));
        t.info.fixed_grid = true;
    }
    let Some(ext) = files.ext.as_mut() else {
        return;
    };
    if let Ok(Some(s)) = ext.get_song_structure() {
        let mood = match s.mood {
            rbox_anlz::Mood::High => Mood::High,
            rbox_anlz::Mood::Low => Mood::Low,
            _ => Mood::Mid,
        };
        let starts: Vec<f64> = beats.iter().map(|b| b.0).collect();
        let phrases: Vec<(u16, u16)> = s.phrases.iter().map(|p| (p.beat, p.kind)).collect();
        t.info.markers = common::phrases(mood, &phrases, s.end_beat, &starts);
    }
}

/// The playlists, intelligent playlists and history in `groups`, with the
/// entry each track makes (by its id in `paths`).
fn read_lists_with(
    db: &mut MasterDb,
    groups: &[&str],
    paths: &HashMap<String, ImportedEntry>,
    tag_names: &HashMap<String, String>,
) -> Result<Vec<ImportedList>, String> {
    let mut out = Vec::new();
    let entries = |ids: Vec<String>| -> Vec<ImportedEntry> {
        ids.iter().filter_map(|id| paths.get(id)).cloned().collect()
    };
    if groups.contains(&PLAYLISTS) || groups.contains(&SMART) {
        let all: Vec<_> = db
            .get_playlists()
            .map_err(err)?
            .into_iter()
            .filter(|p| p.rb_local_deleted == 0)
            .collect();
        let by_id: HashMap<&str, (&str, &str)> = all
            .iter()
            .map(|p| (p.id.as_str(), (p.name.as_str(), p.parent_id.as_str())))
            .collect();
        let mut order = tree_order(&all);
        order.retain(|p| !matches!(p.attribute, PlaylistType::Folder));
        for p in order {
            let folders = folders_of(&by_id, &p.parent_id);
            match p.attribute {
                PlaylistType::List if groups.contains(&PLAYLISTS) => {
                    let mut songs = db.get_playlist_songs(&p.id).map_err(err)?;
                    songs.sort_by_key(|s| s.track_no);
                    out.push(ImportedList {
                        folders,
                        name: p.name.clone(),
                        kind: ImportedListKind::List,
                        entries: entries(songs.into_iter().map(|s| s.content_id).collect()),
                    });
                }
                PlaylistType::SmartList if groups.contains(&SMART) => {
                    let Some(query) = p
                        .smart_list
                        .as_deref()
                        .and_then(|x| SmartList::parse(x).ok())
                        .and_then(|s| smart_query(&s, tag_names))
                    else {
                        eprintln!(
                            "rekordbox: intelligent playlist openDJ's search can't say yet: {}",
                            p.name
                        );
                        continue;
                    };
                    out.push(ImportedList {
                        folders,
                        name: p.name.clone(),
                        kind: ImportedListKind::Smart { query },
                        entries: Vec::new(),
                    });
                }
                _ => {}
            }
        }
    }
    if groups.contains(&HISTORY) {
        let mut sessions = Vec::new();
        for h in db.get_histories().map_err(err)? {
            if h.rb_local_deleted != 0 || matches!(h.attribute, HistoryType::Folder) {
                continue;
            }
            let songs = db.get_history_songs(&h.id).map_err(err)?;
            let started = crate::virtualdj::lists::day_start(&h.date_created)
                .or_else(|| crate::virtualdj::lists::day_start(&h.name))
                .unwrap_or_else(|| h.created_at.timestamp());
            sessions.push(ImportedList {
                folders: Vec::new(),
                name: h.name.clone(),
                kind: ImportedListKind::History { started },
                entries: entries(songs.into_iter().map(|s| s.content_id).collect()),
            });
        }
        sessions.retain(|s| !s.entries.is_empty());
        sessions.sort_by_key(|s| match s.kind {
            ImportedListKind::History { started } => started,
            _ => 0,
        });
        out.extend(sessions);
    }
    Ok(out)
}

/// Lists and folders in the order rekordbox shows them: each folder's
/// children after it, by their place in it.
fn tree_order(all: &[rbox_masterdb::DjmdPlaylist]) -> Vec<&rbox_masterdb::DjmdPlaylist> {
    let mut children: HashMap<&str, Vec<&rbox_masterdb::DjmdPlaylist>> = HashMap::new();
    for p in all {
        children.entry(p.parent_id.as_str()).or_default().push(p);
    }
    for c in children.values_mut() {
        c.sort_by_key(|p| p.seq);
    }
    let mut out = Vec::new();
    let mut stack: Vec<&rbox_masterdb::DjmdPlaylist> =
        children.get("root").cloned().unwrap_or_default();
    stack.reverse();
    while let Some(p) = stack.pop() {
        out.push(p);
        if let Some(c) = children.get(p.id.as_str()) {
            stack.extend(c.iter().rev());
        }
    }
    out
}

/// The folders a list sits in, outermost first.
fn folders_of<'a>(
    by_id: &HashMap<&'a str, (&'a str, &'a str)>,
    mut parent: &'a str,
) -> Vec<String> {
    let mut out = Vec::new();
    while let Some((name, up)) = by_id.get(parent) {
        out.push(name.to_string());
        if out.len() > 64 {
            break;
        }
        parent = up;
    }
    out.reverse();
    out
}

fn smart_query(s: &SmartList, tag_names: &HashMap<String, String>) -> Option<String> {
    let props: Vec<String> = s
        .conditions
        .iter()
        .map(|c| {
            serde_json::to_value(&c.property)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
        })
        .collect();
    let conditions: Vec<Condition> = s
        .conditions
        .iter()
        .zip(&props)
        .map(|(c, p)| Condition {
            property: p,
            operator: c.operator.clone() as i64,
            unit: &c.unit,
            left: &c.left,
            right: &c.right,
        })
        .collect();
    common::smart_query(
        &conditions,
        matches!(s.logical_operator, LogicalOperator::Any),
        |id| tag_names.get(id).cloned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// rbox's demo libraries, when they're there (`RBOX_TESTDATA` points at
    /// rbox's `.testdata`).
    fn demo(version: u8) -> Option<PathBuf> {
        let root = PathBuf::from(std::env::var_os("RBOX_TESTDATA")?);
        Some(root.join(format!("RBv{version}/demo/rekordbox/master.db")))
    }

    #[test]
    fn finds_analysis_files_for_a_copy_kept_elsewhere() {
        let dir = std::env::temp_dir().join(format!("opendj-rb-share-{}", std::process::id()));
        let rekordbox = dir.join("rekordbox");
        let backup = dir.join("backup");
        std::fs::create_dir_all(rekordbox.join("share/PIONEER/USBANLZ/abc")).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        let share = rekordbox.join("share");
        // Next to the database, rekordbox's own folder isn't needed.
        let own = rekordbox.join("master.db");
        assert_eq!(
            share_dir_with(&own, || panic!("not asked")),
            Some(share.clone())
        );
        // A copy elsewhere uses the folder rekordbox itself uses...
        let copy = backup.join("master.db");
        assert_eq!(
            share_dir_with(&copy, || Some(share.clone())),
            Some(share.clone())
        );
        // ...when there is one.
        assert_eq!(share_dir_with(&copy, || None), None);
        assert_eq!(share_dir_with(&copy, || Some(dir.join("gone"))), None);

        let file = "PIONEER/USBANLZ/abc/ANLZ0000.DAT";
        std::fs::write(share.join(file), b"PMAI").unwrap();
        assert_eq!(
            anlz_file(&share, Some(&format!("/{file}"))),
            Some(share.join(file))
        );
        // rbox panics on a path without its leading `/`.
        assert_eq!(anlz_file(&share, Some(file)), Some(share.join(file)));
        assert_eq!(
            anlz_file(&share, Some("/PIONEER/USBANLZ/def/ANLZ0000.DAT")),
            None
        );
        assert_eq!(anlz_file(&share, Some("")), None);
        assert_eq!(anlz_file(&share, None), None);
        // An unreadable file leaves the track without a grid, not the import.
        let mut t = opendj_engine::scan::read_track(0, &share.join(file));
        analysis(Some(&share), Some(file), &mut t);
        assert!(!t.info.fixed_grid);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn says_how_many_tracks_bring_a_beat_grid() {
        assert_eq!(grids_line(Some(12), 12), "Beat grids for every track.");
        assert_eq!(grids_line(Some(9), 12), "Beat grids for 9 of the tracks.");
        assert_eq!(grids_line(Some(0), 0), "Beat grids for 0 of the tracks.");
        assert!(grids_line(None, 12).starts_with("No beat grids"));
    }

    /// A real library, when `OPENDJ_RB_REAL` points at a master.db: every
    /// track here that rekordbox found beats in comes in with its grid,
    /// starting on rekordbox's first beat, with a marker per tempo change.
    #[test]
    fn reads_beat_grids_from_a_real_library() {
        let Some(path) = std::env::var_os("OPENDJ_RB_REAL").map(PathBuf::from) else {
            return;
        };
        eprintln!("{}", preview(&path).unwrap());
        let lib = read(&path, &[], |_| true).unwrap();
        let db = open(&path).unwrap();
        let share = db.share.clone();
        let mut beats: HashMap<PathBuf, Vec<(f64, f64)>> = HashMap::new();
        for c in db.get_contents_with_relations().unwrap() {
            let (Some(file), Some(dat)) = (
                file_path(&c),
                share
                    .as_deref()
                    .and_then(|s| anlz_file(s, c.content.analysis_data_path.as_deref())),
            ) else {
                continue;
            };
            let mut dat = rbox_anlz::Anlz::load(dat).unwrap();
            if let Ok(Some(g)) = dat.get_beat_grid() {
                let b = g
                    .beats
                    .iter()
                    .map(|b| (b.time as f64 / 1000.0, b.tempo as f64 / 100.0));
                beats.insert(file, b.collect());
            }
        }
        let mut grids = 0;
        for t in &lib.tracks {
            let Source::File(file) = &t.source else {
                continue;
            };
            let want = beats.get(file).filter(|b| !b.is_empty());
            assert_eq!(t.info.fixed_grid, want.is_some(), "{}", t.title);
            let Some(want) = want else { continue };
            let changes = 1 + want
                .windows(2)
                .filter(|w| (w[1].1 - w[0].1).abs() > 0.005)
                .count();
            assert_eq!(t.grid.markers().len(), changes, "{}", t.title);
            assert!(
                (t.grid.first_beat() - want[0].0).abs() < 1e-6,
                "{}",
                t.title
            );
            eprintln!(
                "{:45} {:>3} markers, from {:.3} s at {:.2} BPM",
                t.title, changes, want[0].0, want[0].1
            );
            grids += 1;
        }
        eprintln!(
            "{grids} of {} tracks here have rekordbox's grid",
            lib.tracks.len()
        );
    }

    #[test]
    fn reads_rbox_demo_libraries() {
        let Some(path) = demo(6) else {
            eprintln!("RBOX_TESTDATA not set; skipping");
            return;
        };
        let lib = read(&path, &[PLAYLISTS, SMART, HISTORY], |_| true).unwrap();
        let t = lib
            .tracks
            .iter()
            .find(|t| t.title == "Demo Track 1")
            .unwrap();
        assert_eq!(t.artist, "Loopmasters");
        assert_eq!(t.bpm, 128.0);
        assert_eq!(t.key, "Fm");
        assert!(t.info.fixed_grid);
        assert!((t.grid.first_beat() - 0.025).abs() < 1e-6);
        assert_eq!(
            t.info.markers.first().map(|m| m.name.as_str()),
            Some("Chorus")
        );
        assert!(t.info.markers.len() > 10);
        let names: Vec<String> = lib
            .lists
            .iter()
            .map(|l| format!("{}/{}", l.folders.join("/"), l.name))
            .collect();
        assert!(
            names.contains(&"Folder/Sub Playlist".to_string()),
            "{names:?}"
        );
        assert!(preview(&path).unwrap().contains("tracks"));
        let seven = demo(7).unwrap();
        let lib = read(&seven, &[PLAYLISTS], |_| true).unwrap();
        let t = lib
            .tracks
            .iter()
            .find(|t| t.title == "Demo Track 2")
            .unwrap();
        assert_eq!(
            t.info
                .hotcues
                .iter()
                .map(|h| (h.number, h.seconds))
                .collect::<Vec<_>>(),
            [(0, 0.025), (1, 32.025), (2, 64.026)]
        );
        let one = lib
            .tracks
            .iter()
            .find(|t| t.title == "Demo Track 1")
            .unwrap();
        assert!(one.info.hotcues.iter().all(|h| h.is_memory()));
        // Only files that are here come in.
        assert!(read(&seven, &[], |_| false).unwrap().tracks.is_empty());
    }
}
