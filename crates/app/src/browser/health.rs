//! Collection Health in the browser (see [`opendj_library::health`]): a node in
//! the collection's tree with one node per check under it, the checks'
//! page, and the fixes each check offers for its tracks.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opendj_engine::{tags, Track};
use opendj_library::cleanup::Field;
use opendj_library::dupes::{self, Case, FileFacts, Group, Match};
use opendj_library::health::{self, Area, Check, Facts};
use opendj_library::query::Indexed;
use opendj_library::relocate::{self, Lookup};
use opendj_library::stages::Stage;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{
    file_path, is_browsed, Browser, Contents, Node, Undo, KIND_AREA, KIND_HEALTH,
    KIND_HEALTH_SECTION, KIND_IGNORED,
};
use crate::cover_scan;
use crate::{
    settings, HealthArea, HealthCard, HealthChip, HealthHold, HealthLine, HealthReview,
    HealthStage, HealthStep, HealthUi,
};

/// The word the "Tag as mix" fix adds to a track's first tag field.
const MIX_TAG: &str = "mix";

/// A fix a check offers.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fix {
    /// Out of the library; the file stays.
    Remove,
    /// Points missing tracks at the files found for them.
    Relink,
    /// Asks the DJ for a missing track's file.
    Locate,
    /// The check leaves it alone: for "Not music", it is music after all.
    MarkMusic,
    ToTriage,
    FillFromName,
    TagMix,
    /// Merges each listed group into the copy picked to keep.
    KeepBest,
    /// Copies of different lengths stay, linked to each other.
    LinkVersions,
    /// Opens the review table with what the clean-up rules would change.
    CleanUp,
    /// Looks online for covers and embeds the largest.
    FindCovers,
    /// Looks tracks up online and opens the review table with what was
    /// found.
    LookUp,
    /// Opens the review table with each album's tracks given the album
    /// tag most of them have.
    SameAlbum,
    /// Missing tracks go on the Wanted list, to buy again, and out of the
    /// collection.
    ToWanted,
    /// Looks for missing files in a folder the DJ picks.
    FindInFolder,
}

impl Fix {
    fn label(self) -> &'static str {
        match self {
            Fix::Remove => "Remove from collection",
            Fix::MarkMusic => "It's music",
            Fix::Relink => "Relink",
            Fix::Locate => "Find file…",
            Fix::ToWanted => "Move to Wanted",
            Fix::FindInFolder => "Look in a folder…",
            Fix::ToTriage => "Move to In Prep",
            Fix::FillFromName => "Fill from file name…",
            Fix::TagMix => "Tag as #mix",
            Fix::KeepBest => "Keep the best copies",
            Fix::LinkVersions => "Link as versions",
            Fix::CleanUp => "Clean up…",
            Fix::FindCovers => "Find covers",
            Fix::LookUp => "Look up online",
            Fix::SameAlbum => "Use one album tag…",
        }
    }
}

/// The fixes a check offers, the one most often wanted first.
fn fixes(check: Check, stages: bool) -> Vec<Fix> {
    match check {
        Check::NotMusic => vec![Fix::Remove, Fix::MarkMusic],
        Check::Stray if stages => vec![Fix::ToTriage, Fix::Remove],
        Check::Stray => vec![Fix::Remove],
        Check::Outside if stages => vec![Fix::ToTriage, Fix::Remove],
        Check::Outside => vec![Fix::Remove],
        // The bar's "Analyze" does these.
        Check::NotAnalyzed => Vec::new(),
        Check::MissingInfo => vec![Fix::LookUp, Fix::FillFromName],
        // The bar's "Auto-fix" relinks the files found surely (see
        // `relocate`), and merges the easy copies; "Relink" takes the
        // likely ones too.
        Check::Missing => vec![
            Fix::Relink,
            Fix::FindInFolder,
            Fix::Locate,
            Fix::ToWanted,
            Fix::Remove,
        ],
        Check::Long => vec![Fix::TagMix],
        Check::Duplicate => Vec::new(),
        Check::Junk => vec![Fix::CleanUp, Fix::LookUp],
        Check::NoCover => vec![Fix::FindCovers],
        Check::AlbumTags => vec![Fix::SameAlbum],
        Check::NoRating | Check::LowBitrate => Vec::new(),
    }
}

/// "1 track" or "3 tracks".
pub(super) fn tracks(n: usize) -> String {
    format!("{n} track{}", if n == 1 { "" } else { "s" })
}

/// "1 copy" or "3 copies".
fn copies(n: usize) -> String {
    format!("{n} cop{}", if n == 1 { "y" } else { "ies" })
}

/// The Show tabs over the Duplicates page: every copy, then each case.
pub(super) const DUPE_TABS: [Option<Case>; 5] = [
    None,
    Some(Case::SameFile),
    Some(Case::BetterCopy),
    Some(Case::SameFormat),
    Some(Case::Versions),
];

/// The checks in the order the sidebar and the Overview list them:
/// section by section.
pub(super) fn checks_in_order() -> Vec<Check> {
    Area::ALL.iter().flat_map(|a| a.checks()).collect()
}

/// How a check stands, for its dot and tile: nothing found, something to
/// look at, something that won't play, or a goal (Prep).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CheckState {
    Clear = 0,
    Check = 1,
    Problem = 2,
    Goal = 3,
}

pub(super) fn check_state(check: Check, count: usize) -> CheckState {
    match (check.area(), count) {
        (_, 0) => CheckState::Clear,
        (Area::Files, _) => CheckState::Problem,
        (Area::Prep, _) => CheckState::Goal,
        _ => CheckState::Check,
    }
}

/// The check to start with: the first that finds a problem, else the first
/// goal with tracks.
fn start_check(counts: &[(Check, usize)]) -> Option<Check> {
    let first = |problem: bool| {
        counts
            .iter()
            .find(|(c, n)| c.area().problem() == problem && *n > 0)
            .map(|(c, _)| *c)
    };
    first(true).or_else(|| first(false))
}

/// A step on a check's page: what it does, and the button that does it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    /// The check's sure fixes, at once (see `Browser::fix_sure`).
    Sure,
    /// One of the check's fixes, on every listed track.
    Fix(Fix),
    /// The review card, one track at a time.
    Review,
}

/// A check's steps, in the order to do them, with a title and a line on
/// each. Steps whose fix isn't offered (no stage folders, say) are left
/// out when the page is shown.
fn steps(check: Check) -> Vec<(Step, &'static str, &'static str)> {
    use Step::*;
    match check {
        Check::Missing => vec![
            (
                Sure,
                "Relink the sure ones",
                "Each has exactly one match in your folders.",
            ),
            (
                Fix(self::Fix::Relink),
                "Relink likely matches",
                "Same name or same artist, title and length, renamed.",
            ),
            (
                Review,
                "Decide on the rest",
                "Look in a folder, find each file, move to Wanted to buy again, or remove.",
            ),
        ],
        Check::LowBitrate => vec![(
            Review,
            "Listen to each",
            "Find a better copy, or keep it if it sounds fine.",
        )],
        Check::Outside | Check::Stray => vec![
            (
                Fix(self::Fix::ToTriage),
                "Move them into In Prep",
                "openDJ moves the files into your In Prep folder.",
            ),
            (
                Review,
                "Check each",
                "Keep, move or remove them one by one.",
            ),
        ],
        Check::Long => vec![
            (
                Fix(self::Fix::TagMix),
                "Tag the mixes",
                "Mark recorded sets with #mix so filters can leave them out.",
            ),
            (
                Review,
                "Check each",
                "Ignore long pieces that are fine as they are.",
            ),
        ],
        Check::NotMusic => vec![
            (
                Fix(self::Fix::Remove),
                "Remove them",
                "Audiobooks and podcasts leave the collection. Files stay.",
            ),
            (Review, "Check each", "Mark any that are music after all."),
        ],
        Check::Duplicate => vec![
            (
                Sure,
                "Merge the sure ones",
                "The same file, or the same name and length to the second: nothing is lost.",
            ),
            (
                Review,
                "Check the rest",
                "Likely copies a second or two apart: pick the one to keep, or ignore them.",
            ),
            (
                Fix(self::Fix::LinkVersions),
                "Link versions",
                "Different lengths: an edit and an extended mix stay, linked.",
            ),
        ],
        Check::Junk => vec![
            (
                Sure,
                "Clean the sure ones",
                "Track numbers, sites and video words the rules know.",
            ),
            (
                Fix(self::Fix::CleanUp),
                "Clean up the rest…",
                "See every change in a table before it's made.",
            ),
            (
                Review,
                "Check each",
                "Fix or ignore what the rules don't know.",
            ),
        ],
        Check::MissingInfo => vec![
            (
                Fix(self::Fix::FillFromName),
                "Fill from file names…",
                "Artist and title from file names, in a table to check first.",
            ),
            (
                Fix(self::Fix::LookUp),
                "Look up online",
                "MusicBrainz and AcoustID; you pick what's used.",
            ),
            (
                Review,
                "Edit the rest",
                "Type in what's left, one track at a time.",
            ),
        ],
        Check::NoCover => vec![
            (
                Fix(self::Fix::FindCovers),
                "Find covers online",
                "The largest cover found is put in each file.",
            ),
            (
                Review,
                "Check each",
                "Add a picture yourself, or ignore it.",
            ),
        ],
        Check::AlbumTags => vec![
            (
                Fix(self::Fix::SameAlbum),
                "Use one album tag…",
                "Each folder's tracks get the album name most of them have.",
            ),
            (
                Review,
                "Check each",
                "Ignore folders that hold more than one album.",
            ),
        ],
        Check::NotAnalyzed => vec![(
            Sure,
            "Analyze them",
            "In the background at low priority; it pauses while a deck plays.",
        )],
        Check::NoRating => Vec::new(),
    }
}

/// Collection Health's tree: the Overview, then each section's heading
/// with a node per check under it, then Ignored.
pub(super) fn health_nodes() -> Vec<Node> {
    let mut nodes = vec![Node::new(
        "Overview",
        0,
        KIND_HEALTH,
        Contents::Health(None),
    )];
    for a in Area::ALL {
        nodes.push(Node::new(
            a.title(),
            0,
            KIND_HEALTH_SECTION,
            Contents::Area(a),
        ));
        nodes.extend(
            a.checks()
                .into_iter()
                .map(|c| Node::new(c.title(), 0, KIND_AREA, Contents::Health(Some(c)))),
        );
    }
    nodes.push(Node::new(
        "Organization",
        0,
        KIND_HEALTH_SECTION,
        Contents::Tags,
    ));
    nodes.push(Node::new("Tags", 0, KIND_AREA, Contents::Tags));
    nodes.push(Node::new("Lists", 0, KIND_AREA, Contents::Lists));
    nodes.push(Node::new("Ignored", 0, KIND_IGNORED, Contents::Ignored));
    nodes.push(Node::new(
        "Quarantine",
        0,
        KIND_IGNORED,
        Contents::Quarantine,
    ));
    nodes
}

/// The Quarantine page's line on a track taken out: where its file is,
/// and when it left.
pub(super) fn removed_detail(r: &opendj_library::health::Removed, root: Option<&Path>) -> String {
    let when = chrono::DateTime::from_timestamp(r.removed, 0)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%-d %b %Y")
                .to_string()
        })
        .unwrap_or_default();
    let what = match (r.quarantined, root.is_some_and(|q| r.path.starts_with(q))) {
        (_, true) => "File in quarantine",
        (false, false) if !r.path.exists() => "File not found",
        // Moves wait while Move files is off.
        (true, false) => "File waits to go to quarantine",
        (false, false) => "File where it was",
    };
    if when.is_empty() {
        what.into()
    } else {
        format!("{what} · {when}")
    }
}

/// What the Quarantine page's buttons do.
#[derive(Clone, Copy, Debug, PartialEq)]
enum QuarantineFix {
    BringBack,
    /// Files of tracks taken out that stayed in the DJ's folders.
    Move,
    ShowFolder,
}

impl QuarantineFix {
    fn label(self) -> &'static str {
        match self {
            QuarantineFix::BringBack => "Bring back",
            QuarantineFix::Move => "Move files to quarantine",
            QuarantineFix::ShowFolder => "Show folder",
        }
    }
}

/// "86%" of `part` in `whole`, rounded down so 99.6% isn't "100%".
fn percent(part: usize, whole: usize) -> i32 {
    (part * 100).checked_div(whole).map_or(100, |p| p as i32)
}

/// Every audio file under `roots`, by its file name in lower case.
/// The audio files below these folders.
fn audio_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = roots.to_vec();
    let mut seen = HashSet::new();
    while let Some(dir) = stack.pop() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let hidden = e.file_name().to_string_lossy().starts_with('.');
            match e.file_type() {
                Ok(t) if t.is_dir() && !hidden => stack.push(path),
                Ok(t) if t.is_file() && opendj_engine::scan::is_audio(&path) => out.push(path),
                _ => {}
            }
        }
    }
    out
}

impl Browser {
    pub(super) fn install_health(
        ui: &HealthUi,
        with: &(impl Fn(&dyn Fn(&mut Browser)) + Clone + 'static),
    ) {
        let w = with.clone();
        ui.on_show(move |on| w(&|b| b.show_health(on)));
        let w = with.clone();
        ui.on_review(move |i| {
            w(&|b| {
                b.review(
                    usize::try_from(i)
                        .ok()
                        .and_then(|i| Check::ALL.get(i).copied()),
                )
            })
        });
        let w = with.clone();
        ui.on_fix(move |action, first, last| {
            w(&|b| {
                if matches!(b.current_shown(), Some(Contents::Ignored)) {
                    let indices = b.picked_rows(first, last);
                    b.bring_back(&indices);
                    return;
                }
                if b.shows_quarantine() {
                    let ids = b.picked_removed(first, last);
                    match b.quarantine_fixes().get(action.max(0) as usize) {
                        Some(QuarantineFix::Move) => b.quarantine_removed_files(&ids),
                        Some(QuarantineFix::ShowFolder) => {
                            if let Some(dir) = b.quarantine_root().filter(|d| d.is_dir()) {
                                crate::files::reveal(&dir);
                            }
                        }
                        _ => b.bring_back_removed(&ids),
                    }
                    return;
                }
                let Some(check) = b.listed_check() else {
                    return;
                };
                let Some(&fix) = b.offered(check).get(action.max(0) as usize) else {
                    return;
                };
                let indices = b.picked_rows(first, last);
                b.fix(fix, &indices);
            })
        });
        let w = with.clone();
        ui.on_ignore(move |first, last| {
            w(&|b| {
                let Some(check) = b.listed_check() else {
                    return;
                };
                let indices = b.picked_rows(first, last);
                b.ignore(check, &indices);
            })
        });
        let w = with.clone();
        ui.on_fix_sure(move || {
            w(&|b| {
                if let Some(check) = b.listed_check() {
                    b.fix_sure(check);
                }
            })
        });
        let w = with.clone();
        ui.on_run_step(move |i| {
            w(&|b| {
                if let Ok(i) = usize::try_from(i) {
                    b.run_step(i);
                }
            })
        });
        let w = with.clone();
        ui.on_review_key(move |key| w(&|b| b.review_key(&key)));
        let w = with.clone();
        ui.on_pick_chip(move |i| {
            w(&|b| {
                if let Some(&tab) = usize::try_from(i).ok().and_then(|i| DUPE_TABS.get(i)) {
                    b.dupe_tab = tab;
                    b.review_at = None;
                    b.refresh_tree();
                    b.refresh_rows();
                }
            })
        });
        let w = with.clone();
        ui.on_restore(move || w(&|b| b.restore_removed()));
        let w = with.clone();
        ui.on_tidy_lists(move || w(&|b| b.open_lists()));
        let w = with.clone();
        ui.on_keep_this(move |row| w(&|b| b.keep_this(row)));
        let w = with.clone();
        ui.on_toggle_copies(move |row| w(&|b| b.toggle_copies(row)));
        let w = with.clone();
        ui.on_quarantine(move |first, last| {
            w(&|b| {
                let indices = b.picked_rows(first, last);
                b.health_status = b.quarantine(&indices);
                b.refresh_crates();
                b.refresh_tree();
                b.refresh_rows();
            })
        });
        let w = with.clone();
        ui.on_show_quarantine(move || {
            w(&|b| {
                if let Some(dir) = b.quarantine_root().filter(|d| d.is_dir()) {
                    crate::files::reveal(&dir);
                }
            })
        });
    }

    /// What a node lists: a section heading lists its first check, and
    /// Duplicates lists the case its Show tab picks.
    pub(super) fn resolved(&self, contents: &Contents) -> Contents {
        match contents {
            Contents::Area(a) => Contents::Health(a.checks().first().copied()),
            Contents::Health(Some(Check::Duplicate)) => match self.dupe_tab {
                Some(case) => Contents::Dupes(case),
                None => contents.clone(),
            },
            c => c.clone(),
        }
    }

    /// What the selected node lists (see [`Self::resolved`]).
    pub(super) fn current_shown(&self) -> Option<Contents> {
        self.nodes
            .get(self.current_node)
            .map(|n| self.resolved(&n.contents))
    }

    /// How a check stands (see [`CheckState`]).
    pub(super) fn check_state(&self, check: Check) -> CheckState {
        check_state(check, self.health_count(check))
    }

    /// Selects a check's node, with all its tracks listed.
    fn open_check(&mut self, check: Check) {
        self.show_health(true);
        let Some(n) = self
            .nodes
            .iter()
            .position(|n| matches!(n.contents, Contents::Health(Some(c)) if c == check))
        else {
            return;
        };
        self.dupe_tab = None;
        self.review_at = None;
        self.health_status.clear();
        self.current_node = n;
        self.folders_focused = false;
        if let Some(app) = self.app.upgrade() {
            app.set_current_track(-1);
        }
        self.refresh_tree();
        self.refresh_rows();
    }

    /// The folders missing files are looked for in.
    fn relink_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = self
            .folders
            .all()
            .into_iter()
            .map(Path::to_path_buf)
            .collect();
        roots.extend(settings::music_folders());
        roots
    }

    /// Where a missing track's file may be now (see [`relocate`]).
    fn relink_lookup(&self, t: &Track) -> Lookup {
        let index = self.relink_files.get_or_init(|| {
            let mut index = relocate::Index::new(audio_files(&self.relink_roots()));
            for t in self.library.iter().filter(|t| !t.info.missing) {
                if let Some(p) = file_path(t) {
                    index.know(p, &t.artist, &t.title, t.duration);
                }
            }
            index
        });
        match file_path(t) {
            Some(p) => index.find(p, &t.artist, &t.title, t.duration),
            None => Lookup::Nothing,
        }
    }

    /// A Collection Health area's Problem and Fix cells for `t`: what the
    /// listed check found, and what its first fix would do.
    pub(super) fn health_cells(&self, t: &Track) -> (String, String) {
        if matches!(self.current_shown(), Some(Contents::Ignored)) {
            let by: Vec<&str> = self
                .health
                .ignored_by(t.id)
                .into_iter()
                .map(|c| match c {
                    Check::NotMusic => "Kept as music",
                    c => c.title(),
                })
                .collect();
            return (by.join(", "), "Bring back".into());
        }
        let Some(check) = self.listed_check() else {
            return Default::default();
        };
        let short = |p: &Path| {
            let parts: Vec<String> = p
                .components()
                .rev()
                .take(2)
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            parts.into_iter().rev().collect::<Vec<_>>().join("/")
        };
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        match check {
            Check::Missing => match self.relink_lookup(t) {
                Lookup::Found(f) if f.sure && f.tier == relocate::Tier::Name => (
                    "File not found".into(),
                    format!("Relink to {}", short(&f.path)),
                ),
                Lookup::Found(f) if f.sure => (
                    "File not found".into(),
                    format!("Relink to {} ({})", short(&f.path), f.tier.why()),
                ),
                Lookup::Found(f) => (
                    "File not found".into(),
                    format!("Maybe {} ({}): check it", short(&f.path), f.tier.why()),
                ),
                Lookup::Several(n) => (
                    "File not found".into(),
                    format!("{n} files match: find the right one"),
                ),
                Lookup::Nothing => s(
                    "File not found",
                    "Not found in your folders: find it or remove it",
                ),
            },
            Check::LowBitrate => (
                self.health
                    .bitrate(t.id)
                    .map_or_else(|| "Low bitrate".into(), |b| format!("{b} kb/s")),
                "Find a better copy".into(),
            ),
            Check::NotMusic => s("Audiobook or podcast", "Remove from collection"),
            Check::Stray if self.folders.is_set() => {
                s("Outside the stage folders", "Move to In Prep")
            }
            Check::Stray => s("Outside the stage folders", "Remove from collection"),
            Check::Outside => s(
                "Outside your music folders",
                if self.folders.is_set() {
                    "Move to In Prep"
                } else {
                    "Remove from collection"
                },
            ),
            Check::Long => (
                format!("{} min", (t.duration / 60.0).round() as i64),
                format!("Tag as {MIX_TAG}"),
            ),
            Check::Duplicate => {
                let Some(g) = self.dupe_groups().iter().find(|g| g.tracks.contains(&t.id)) else {
                    return s("Copy", "");
                };
                if g.tracks[0] == t.id {
                    // The group's row: how it matched, and what differs.
                    let problem = match g.case {
                        Case::Versions => format!("{} versions", g.tracks.len()),
                        c => format!("{} · {} ({})", g.matched.title(), c.title(), g.tracks.len()),
                    };
                    let fix = match g.case {
                        Case::SameFile => "Identical files: keeps one".to_string(),
                        Case::Versions => g
                            .tracks
                            .iter()
                            .filter_map(|id| self.index_of.get(id))
                            .map(|&i| super::short_time(self.library[i].duration).to_string())
                            .collect::<Vec<_>>()
                            .join(" vs "),
                        _ => {
                            let others: Vec<String> =
                                g.tracks[1..].iter().map(|&id| self.copy_text(id)).collect();
                            format!("Keeps {} over {}", self.copy_text(t.id), others.join(", "))
                        }
                    };
                    (problem, fix)
                } else if g.case == Case::Versions {
                    (
                        format!("Version · {}", self.copy_text(t.id)),
                        Fix::LinkVersions.label().into(),
                    )
                } else {
                    let keep = self
                        .index_of
                        .get(&g.tracks[0])
                        .and_then(|&i| file_path(&self.library[i]))
                        .map(short)
                        .unwrap_or_default();
                    (
                        format!("Copy · {}", self.copy_text(t.id)),
                        format!("Merge into {keep}"),
                    )
                }
            }
            Check::MissingInfo => {
                let lacking: Vec<&str> = [
                    ("artist", &t.artist),
                    ("title", &t.title),
                    ("album", &t.album),
                    ("year", &t.info.year),
                    ("genre", &t.genre),
                ]
                .into_iter()
                .filter(|(_, v)| health::says_nothing(v))
                .map(|(what, _)| what)
                .collect();
                let problem = format!("No {}", lacking.join(", "));
                let named = file_path(t).and_then(health::from_file_name);
                let fix = match named {
                    Some((artist, title))
                        if health::says_nothing(&t.artist) || health::says_nothing(&t.title) =>
                    {
                        format!("{artist} – {title}, from the file name")
                    }
                    _ => "Look up online".into(),
                };
                (problem, fix)
            }
            Check::Junk => {
                let changes = self.health.junk(t);
                let fix = changes
                    .iter()
                    .map(|c| format!("{} → {}", c.old, c.new))
                    .collect::<Vec<_>>()
                    .join("; ");
                ("Junk in names".into(), fix)
            }
            Check::AlbumTags => (
                if t.album.trim().is_empty() {
                    "No album tag".into()
                } else {
                    format!("Album: {}", t.album)
                },
                self.health
                    .album_tag(t.id)
                    .map(|a| format!("Album: {a}"))
                    .unwrap_or_default(),
            ),
            Check::NoCover => s("No cover", "Look online for one"),
            Check::NotAnalyzed => s("No tempo or key", "Analyze"),
            Check::NoRating => s("No stars", "Rate it"),
        }
    }

    /// How far a check has come since it was first opened: tracks fixed,
    /// of those it found then.
    fn check_progress(&self, check: Check) -> (usize, usize) {
        let now = self.health_count(check);
        let mut starts = self.check_start.borrow_mut();
        let start = starts.entry(check).or_insert(now);
        *start = (*start).max(now);
        (*start - now, *start)
    }

    /// The check whose tracks are listed: a case of copies is Duplicates'.
    fn listed_check(&self) -> Option<Check> {
        match self.current_shown()? {
            Contents::Health(check) => check,
            Contents::Dupes(_) => Some(Check::Duplicate),
            _ => None,
        }
    }

    /// The fixes offered for the tracks listed: each case of copies has
    /// its own.
    fn offered(&self, check: Check) -> Vec<Fix> {
        match self.dupe_view() {
            Some(Some(Case::Versions)) => vec![Fix::LinkVersions, Fix::KeepBest],
            Some(Some(_)) => vec![Fix::KeepBest],
            _ => fixes(check, self.folders.is_set()),
        }
    }

    /// Copies are listed: all of them (`Some(None)`), or one case's.
    pub(super) fn dupe_view(&self) -> Option<Option<Case>> {
        match self.current_shown()? {
            Contents::Health(Some(Check::Duplicate)) => Some(None),
            Contents::Dupes(case) => Some(Some(case)),
            _ => None,
        }
    }

    /// Works out which tracks are copies of others, without reading their
    /// files, and forgets the groups worked out before.
    pub(super) fn refresh_dupes(&mut self) {
        self.dupe_groups = OnceCell::new();
        self.links = self
            .db
            .as_ref()
            .and_then(|db| db.all_links().ok())
            .unwrap_or_default();
        let everyone: HashSet<usize> = self.library.iter().map(|t| t.id).collect();
        let tracks: Vec<&Track> = self
            .library
            .iter()
            .filter(|t| self.in_collection(t) && self.health.finds(Check::Duplicate, t, &everyone))
            .map(|t| &**t)
            .collect();
        let links = &self.links;
        let alike: HashSet<(usize, usize)> = self.alike.iter().copied().collect();
        self.duplicates = dupes::copies(tracks, &self.alike)
            .iter()
            .flat_map(|c| {
                dupes::groups(
                    c,
                    &self.folders,
                    |_| FileFacts::default(),
                    |_, _| false,
                    |a, b| links.contains(&(a, b)),
                    |a, b| alike.contains(&(a.min(b), a.max(b))),
                )
            })
            .flat_map(|g| g.tracks)
            .collect();
        self.quarantined = self.count_quarantined();
    }

    /// The copies grouped by case, each with the copy to keep first. The
    /// first call after the library changes reads the copies' files.
    fn dupe_groups(&self) -> &[Group] {
        self.dupe_groups.get_or_init(|| {
            let tracks = self
                .library
                .iter()
                .filter(|t| self.in_collection(t) && self.duplicates.contains(&t.id))
                .map(|t| &**t);
            // Files iCloud has taken off this computer aren't read.
            fn here(t: &Track) -> Option<&Path> {
                file_path(t).filter(|p| !cover_scan::offloaded(p))
            }
            let facts = |t: &Track| {
                let f = here(t).map_or_else(FileFacts::default, |p| {
                    let info = tags::file_info(p);
                    FileFacts {
                        size: info.size,
                        lossless: dupes::lossless(&info.format, info.bit_depth),
                        bitrate: info.bitrate.unwrap_or(0),
                    }
                });
                self.dupe_facts.borrow_mut().insert(t.id, f);
                f
            };
            let same = |a: &Track, b: &Track| match (here(a), here(b)) {
                (Some(x), Some(y)) => dupes::same_bytes(x, y),
                _ => false,
            };
            let alike: HashSet<(usize, usize)> = self.alike.iter().copied().collect();
            dupes::copies(tracks, &self.alike)
                .iter()
                .flat_map(|c| {
                    dupes::groups(
                        c,
                        &self.folders,
                        facts,
                        same,
                        |a, b| self.links.contains(&(a, b)),
                        |a, b| alike.contains(&(a.min(b), a.max(b))),
                    )
                })
                .collect()
        })
    }

    /// The copies (indices into `library`) in groups of `case`, or in any
    /// group, group by group.
    pub(super) fn dupe_tracks(&self, case: Option<Case>) -> Vec<usize> {
        let mut seen = HashSet::new();
        self.dupe_groups()
            .iter()
            .filter(|g| case.is_none_or(|c| g.case == c))
            .flat_map(|g| &g.tracks)
            .filter(|&&id| seen.insert(id))
            .filter_map(|id| self.index_of.get(id).copied())
            .collect()
    }

    /// The rows of the Duplicates page (indices into `library`): each
    /// group's kept copy, followed by its other copies when the group is
    /// open.
    pub(super) fn dupe_rows(&self, case: Option<Case>) -> Vec<usize> {
        let mut seen = HashSet::new();
        self.dupe_groups()
            .iter()
            .filter(|g| case.is_none_or(|c| g.case == c))
            .flat_map(|g| {
                let open = self.all_copies || self.open_groups.contains(&g.tracks[0]);
                &g.tracks[..if open { g.tracks.len() } else { 1 }]
            })
            .filter(|&&id| seen.insert(id))
            .filter_map(|id| self.index_of.get(id).copied())
            .collect()
    }

    /// Opens or closes the group of the copy in `row`; with `row` below 0,
    /// every group.
    fn toggle_copies(&mut self, row: i32) {
        match usize::try_from(row)
            .ok()
            .and_then(|r| self.rows.get(r).copied().flatten())
        {
            None => {
                self.all_copies = !self.all_copies;
                self.open_groups.clear();
            }
            Some(i) => {
                let id = self.library[i].id;
                let Some(keep) = self
                    .dupe_groups()
                    .iter()
                    .find(|g| g.tracks.contains(&id))
                    .map(|g| g.tracks[0])
                else {
                    return;
                };
                if !self.open_groups.remove(&keep) {
                    self.open_groups.insert(keep);
                }
            }
        }
        self.refresh_rows_keeping_current();
    }

    /// A copy's file in a few words: "FLAC 1411 kb/s · 6:12".
    fn copy_text(&self, id: usize) -> String {
        let Some(t) = self.index_of.get(&id).map(|&i| &self.library[i]) else {
            return String::new();
        };
        let format = file_path(t)
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        let facts = self
            .dupe_facts
            .borrow()
            .get(&id)
            .copied()
            .unwrap_or_default();
        let mut parts = Vec::new();
        match (format.is_empty(), facts.bitrate) {
            (true, _) => {}
            (false, 0) => parts.push(format),
            (false, b) if facts.lossless => parts.push(format!("{format} {b} kb/s")),
            (false, b) => parts.push(format!("{format} {b}")),
        }
        if t.duration > 0.0 {
            parts.push(super::short_time(t.duration).to_string());
        }
        parts.join(" · ")
    }

    /// Ids of the copies the listed groups would merge away: in every easy
    /// group, or in `case`'s.
    pub(super) fn dupe_faded(&self, case: Option<Case>) -> HashSet<usize> {
        self.dupe_groups()
            .iter()
            .filter(|g| case.map_or(g.easy(), |c| g.case == c))
            .flat_map(|g| g.tracks[1..].iter().copied())
            .collect()
    }

    /// The groups of the cases `wanted` takes that hold any of these
    /// tracks (indices into `library`).
    fn picked_groups(&self, indices: &[usize], wanted: impl Fn(Case) -> bool) -> Vec<Group> {
        let ids: HashSet<usize> = indices.iter().map(|&i| self.library[i].id).collect();
        self.dupe_groups()
            .iter()
            .filter(|g| wanted(g.case) && g.tracks.iter().any(|t| ids.contains(t)))
            .cloned()
            .collect()
    }

    /// How many listed groups a fix for copies works on.
    fn fix_groups(&self, fix: Fix) -> usize {
        let case = self.dupe_view().flatten();
        self.dupe_groups()
            .iter()
            .filter(|g| match fix {
                Fix::LinkVersions => g.case == Case::Versions,
                _ => case.map_or(g.easy(), |c| g.case == c),
            })
            .count()
    }

    /// What merging `group` into `keep` takes out: the other copies, and
    /// for versions, each other version's own copies too.
    fn merge_plan(&self, group: &Group, keep: usize) -> (usize, Vec<usize>) {
        let mut others: Vec<usize> = group
            .tracks
            .iter()
            .copied()
            .filter(|&t| t != keep)
            .collect();
        if group.case == Case::Versions {
            let groups = self.dupe_groups();
            let copies: Vec<usize> = others
                .iter()
                .flat_map(|&o| {
                    groups
                        .iter()
                        .filter(move |g| g.case.easy() && g.tracks[0] == o)
                        .flat_map(|g| g.tracks[1..].iter().copied())
                })
                .collect();
            others.extend(copies);
        }
        (keep, others)
    }

    /// Keeps the copy at a row, merging the rest of its group into it.
    fn keep_this(&mut self, row: i32) {
        let Some(i) = usize::try_from(row)
            .ok()
            .and_then(|r| self.rows.get(r).copied().flatten())
        else {
            return;
        };
        let id = self.library[i].id;
        let view = self.dupe_view().flatten();
        let holds = |g: &&Group| g.tracks.contains(&id) && view.is_none_or(|c| g.case == c);
        let groups = self.dupe_groups();
        // On the list of all copies, its own recording's group comes first.
        let Some(group) = groups
            .iter()
            .filter(holds)
            .find(|g| g.case.easy())
            .or_else(|| groups.iter().find(holds))
            .cloned()
        else {
            return;
        };
        let plan = self.merge_plan(&group, id);
        self.health_status = self.merge(vec![plan]);
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Merges groups of copies, each into the copy kept (see
    /// [`opendj_library::Library::merge_copies`]), as one change the undo
    /// line takes back.
    fn merge(&mut self, plan: Vec<(usize, Vec<usize>)>) -> String {
        if plan.is_empty() {
            return "Nothing to merge here.".into();
        }
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let quarantine = dupes::quarantine_dir(&self.folders, &date);
        let moving = settings::move_files();
        let Some(db) = &mut self.db else {
            return String::new();
        };
        let (mut batch, mut groups, mut restore) = (None, 0, Vec::new());
        for (keep, others) in &plan {
            match db.merge_copies(*keep, others, quarantine.as_deref(), batch) {
                Ok(b) => {
                    batch = batch.or(b);
                    groups += 1;
                    restore.extend(others.iter().copied());
                }
                Err(e) => eprintln!("can't merge copies: {e}"),
            }
        }
        let mut failed = 0;
        if batch.is_some() && moving {
            match db.apply_moves() {
                Ok(r) => {
                    for (path, why) in &r.failed {
                        eprintln!("can't move {}: {why}", path.display());
                    }
                    failed = r.failed.len();
                }
                Err(e) => eprintln!("can't move files: {e}"),
            }
        }
        let n = restore.len();
        if n > 0 {
            self.undo = Some(Undo {
                batch,
                text: format!("Undo merging {}", copies(n)),
                restore,
                changes: None,
                relinked: Vec::new(),
            });
        }
        self.read_pending_moves();
        self.reload_library();
        let mut s = format!(
            "Kept one copy of {} track{}; {} merged into {}.",
            groups,
            if groups == 1 { "" } else { "s" },
            copies(n),
            if groups == 1 { "it" } else { "them" },
        );
        s += match (quarantine.is_some(), moving) {
            (false, _) => " Their files stay where they are: set your Play Ready folder in Settings › Collection to quarantine them.",
            (true, true) => " Their files are in _Quarantine.",
            (true, false) => " Their files go to _Quarantine once Move files is on in Settings › Collection.",
        };
        if failed > 0 {
            s += &format!(" {failed} couldn't be moved.");
        }
        s
    }

    /// Takes tracks (indices into `library`) out of the library and moves
    /// their files to today's quarantine folder, or queues the moves while
    /// Move files is off, as one change the undo line takes back.
    fn quarantine(&mut self, indices: &[usize]) -> String {
        let ids: Vec<usize> = indices
            .iter()
            .map(|&i| &self.library[i])
            .filter(|t| !is_browsed(t))
            .map(|t| t.id)
            .collect();
        self.quarantine_ids(ids, true)
    }

    /// Tracks taken out of the collection whose files stayed where they
    /// were, by id.
    fn removed_in_place(&self) -> Vec<usize> {
        let root = self.quarantine_root();
        self.db
            .as_ref()
            .and_then(|db| db.removed_tracks().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|r| !r.quarantined && r.path.is_file())
            .filter(|r| !root.as_ref().is_some_and(|q| r.path.starts_with(q)))
            .map(|r| r.id)
            .collect()
    }

    /// Moves the files of tracks already out of the collection, that stayed
    /// in your folders, to quarantine.
    fn quarantine_removed_files(&mut self, ids: &[usize]) {
        let in_place = self.removed_in_place();
        let ids: Vec<usize> = ids
            .iter()
            .copied()
            .filter(|id| in_place.contains(id))
            .collect();
        if ids.is_empty() {
            self.health_status = "Their files are in quarantine already.".into();
        } else {
            self.health_status = self.quarantine_ids(ids, false);
        }
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Takes tracks (by id) out of the collection, if `take_out`, and
    /// queues their files' moves to quarantine. Says what happened.
    fn quarantine_ids(&mut self, ids: Vec<usize>, take_out: bool) -> String {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let Some(quarantine) = dupes::quarantine_dir(&self.folders, &date) else {
            return "Set your Play Ready folder in Settings › Collection to quarantine tracks."
                .into();
        };
        let moving = settings::move_files();
        let Some(db) = &mut self.db else {
            return String::new();
        };
        let batch = match db.quarantine_tracks(&ids, Some(&quarantine)) {
            Ok(Some(b)) => b,
            Ok(None) => return "Nothing to quarantine here.".into(),
            Err(e) => return format!("Couldn't quarantine them: {e}"),
        };
        let mut failed = 0;
        if moving {
            match db.apply_moves() {
                Ok(r) => {
                    for (path, why) in &r.failed {
                        eprintln!("can't move {}: {why}", path.display());
                    }
                    failed = r.failed.len();
                }
                Err(e) => eprintln!("can't move files: {e}"),
            }
        }
        let n = ids.len();
        self.undo = Some(Undo {
            batch: Some(batch),
            text: format!("Undo moving {} to Quarantine", tracks(n)),
            // Tracks already out stay out: only their files go back.
            restore: if take_out { ids } else { Vec::new() },
            changes: None,
            relinked: Vec::new(),
        });
        self.read_pending_moves();
        self.reload_library();
        let mut s = if take_out {
            format!("{} taken out of your collection.", tracks(n))
        } else {
            format!("The files of {}:", tracks(n))
        };
        s += match (take_out, moving) {
            (true, true) => " Their files are in _Quarantine.",
            (true, false) => {
                " Their files go to _Quarantine once Move files is on in Settings › Collection."
            }
            (false, true) => " moved to _Quarantine.",
            (false, false) => {
                " they go to _Quarantine once Move files is on in Settings › Collection."
            }
        };
        if failed > 0 {
            s += &format!(" {failed} couldn't be moved.");
        }
        s
    }

    /// Links each listed group of versions, so they stay.
    fn link_versions(&mut self, indices: &[usize]) -> String {
        let groups = self.picked_groups(indices, |c| c == Case::Versions);
        let Some(db) = &self.db else {
            return String::new();
        };
        let mut n = 0;
        for g in &groups {
            for (i, &a) in g.tracks.iter().enumerate() {
                for &b in &g.tracks[i + 1..] {
                    if let Err(e) = db.link(a, b) {
                        eprintln!("can't link: {e}");
                    }
                }
            }
            n += g.tracks.len();
        }
        self.refresh_dupes();
        format!(
            "Linked {} as versions. Each shows the others on its Related tab.",
            tracks(n)
        )
    }

    /// The folder merged copies' files go to, by date.
    fn quarantine_root(&self) -> Option<PathBuf> {
        dupes::quarantine_root(&self.folders)
    }

    /// Files in the quarantine folder, and those waiting to go there.
    fn count_quarantined(&self) -> usize {
        let Some(root) = self.quarantine_root() else {
            return 0;
        };
        audio_files(std::slice::from_ref(&root)).len()
            + self
                .pending_moves
                .values()
                .filter(|to| to.starts_with(&root))
                .count()
    }

    /// Library tracks (indices into `library`) on rows `first` to `last`
    /// (every picked row when `first` is one of several picked), or on
    /// every row when `first` is negative.
    pub(super) fn picked_rows(&self, first: i32, last: i32) -> Vec<usize> {
        if first < 0 {
            return self.rows.iter().copied().flatten().collect();
        }
        self.chosen_tracks(first as usize, last.max(first) as usize)
    }

    /// Works out the checks' facts from the library as it is now, and looks
    /// for covers in files not looked at since they last changed.
    pub(super) fn refresh_health(&mut self) {
        let Some(db) = &self.db else {
            self.health = Facts::default();
            return;
        };
        let ignored = db.health_ignored().unwrap_or_default();
        let saved = db.covers().unwrap_or_default();
        let saved_sounds = db.sounds().unwrap_or_default();
        let tracks: Vec<&Track> = self
            .library
            .iter()
            .filter(|t| self.in_collection(t))
            .map(|t| &**t)
            .collect();
        let mut covers = HashMap::new();
        let mut bitrates = Vec::new();
        let mut look = Vec::new();
        for t in &tracks {
            let Some(path) = file_path(t) else { continue };
            if t.info.missing {
                continue;
            }
            match saved.get(&t.id) {
                // Looked at before bitrates were: look again.
                Some(found)
                    if found.mtime == cover_scan::mtime(path) && found.bitrate.is_some() =>
                {
                    covers.insert(t.id, found.cover);
                    bitrates.extend(found.bitrate.map(|b| (t.id, b)));
                }
                _ => look.push((t.id, path.to_path_buf(), t.album.clone())),
            }
        }
        let mut folders = settings::music_folders();
        folders.extend(self.folders.all().into_iter().map(Path::to_path_buf));
        folders.extend(self.mixed_sets.clone());
        let mut facts = Facts::new(
            tracks.iter().copied(),
            ignored,
            covers,
            self.folders.is_set(),
        )
        .with_folders(folders)
        .with_rules(settings::cleanup_rules());
        if let Some(albums) = &self.folders.albums {
            facts = facts.with_albums(albums, tracks.iter().copied());
        }
        for (id, kbps) in bitrates {
            facts.set_bitrate(id, kbps);
        }
        self.health = facts;
        self.removed = db.removed_count().unwrap_or(0);
        let music = |b: &Self, id: usize| {
            b.index_of.get(&id).is_some_and(|&i| {
                !b.health
                    .finds(Check::NotMusic, &b.library[i], &HashSet::new())
            })
        };
        for (id, path, album) in look {
            if music(self, id) && !cover_scan::offloaded(&path) {
                self.cover_scan.add(id, &path, &album);
            }
        }
        // Fingerprints: those made since the file last changed are kept,
        // the others are made again in the background.
        let mut sounds = HashMap::new();
        let mut listen = Vec::new();
        for t in self
            .library
            .iter()
            .filter(|t| self.in_collection(t) && !t.info.missing)
        {
            let Some(path) = file_path(t) else { continue };
            match saved_sounds.get(&t.id) {
                Some((mtime, print)) if *mtime == cover_scan::mtime(path) => {
                    if !print.is_empty() {
                        sounds.insert(t.id, print.clone());
                    }
                }
                _ => listen.push((t.id, path.to_path_buf())),
            }
        }
        for (id, path) in listen {
            if music(self, id) && !cover_scan::offloaded(&path) {
                self.sound_scan.add(id, &path);
            }
        }
        if sounds != self.sounds {
            self.sounds = sounds;
            self.alike = dupes::same_sound(&self.sounds);
        }
    }

    /// Takes in the fingerprints made since last time. Copies are looked
    /// for again every so often, and once the last file is done.
    pub fn poll_sounds(&mut self) {
        let found = self.sound_scan.take();
        if found.is_empty() {
            return;
        }
        if let Some(db) = &mut self.db {
            if let Err(e) = db.save_sounds(&found) {
                eprintln!("can't save fingerprints: {e}");
            }
        }
        for (id, _, print) in found {
            if !print.is_empty() {
                self.sounds.insert(id, print);
                self.sounds_new += 1;
            }
        }
        if self.sounds_new == 0 || (self.sound_scan.busy() && self.sounds_new < 50) {
            return;
        }
        self.sounds_new = 0;
        let alike = dupes::same_sound(&self.sounds);
        if alike == self.alike {
            return;
        }
        self.alike = alike;
        self.refresh_dupes();
        self.refresh_tree();
        if self.dupe_view().is_some() {
            self.refresh_rows_keeping_current();
        }
    }

    /// How many tracks `check` finds.
    pub(super) fn health_count(&self, check: Check) -> usize {
        self.library
            .iter()
            .filter(|t| self.in_collection(t) && self.health.finds(check, t, &self.duplicates))
            .count()
    }

    /// The tracks (indices into `library`) `check` finds.
    pub(super) fn health_tracks(&self, check: Check) -> Vec<usize> {
        (0..self.library.len())
            .filter(|&i| {
                let t = &self.library[i];
                self.in_collection(t) && self.health.finds(check, t, &self.duplicates)
            })
            .collect()
    }

    /// Takes in what the cover scan found since last time.
    pub fn poll_covers(&mut self) {
        let found = self.cover_scan.take();
        if found.is_empty() {
            return;
        }
        if let Some(db) = &mut self.db {
            if let Err(e) = db.save_covers(&found) {
                eprintln!("can't save covers: {e}");
            }
        }
        for (id, look) in found {
            self.health.set_cover(id, look.cover);
            if let Some(kbps) = look.bitrate {
                self.health.set_bitrate(id, kbps);
            }
        }
        self.refresh_tree();
        if matches!(
            self.current_shown(),
            Some(Contents::Health(Some(Check::NoCover | Check::LowBitrate)))
        ) {
            self.refresh_rows();
        }
    }

    /// Selects a check, or the Overview.
    fn review(&mut self, check: Option<Check>) {
        let Some(check) = check else {
            self.show_health(true);
            if let Some(n) = self
                .nodes
                .iter()
                .position(|n| matches!(n.contents, Contents::Health(None)))
            {
                self.health_status.clear();
                self.review_at = None;
                self.current_node = n;
                self.refresh_tree();
                self.refresh_rows();
            }
            return;
        };
        self.open_check(check);
    }

    /// Shows the page and bar as things are now.
    pub(super) fn refresh_health_ui(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let ui = app.global::<HealthUi>();
        let current = self.nodes.get(self.current_node).map(|n| &n.contents);
        let on = self.saved() && self.tree == super::COLLECTION && !self.history_tab;
        ui.set_overview(on && matches!(current, Some(Contents::Health(None))));
        let listed = self.listed_check().filter(|_| on);
        let check = listed.and_then(|c| Check::ALL.iter().position(|&x| x == c));
        // The Ignored node's bar is the card after the checks'.
        let ignored = on && matches!(current, Some(Contents::Ignored));
        let quarantine = on && matches!(current, Some(Contents::Quarantine));
        let check = check
            .or(ignored.then_some(Check::ALL.len()))
            .or(quarantine.then_some(Check::ALL.len() + 1));
        ui.set_check(check.map_or(-1, |c| c as i32));
        let case = self.dupe_view().flatten();
        let mut cards: Vec<HealthCard> = Check::ALL
            .iter()
            .map(|&c| {
                let offered = if Some(c) == listed {
                    self.offered(c)
                } else {
                    fixes(c, self.folders.is_set())
                };
                // The track menu has the stage lines already, and "Keep
                // this copy" instead of fixing every group.
                let menu: Vec<i32> = (0..offered.len() as i32)
                    .filter(|&j| offered[j as usize] != Fix::ToTriage)
                    .collect();
                // Fixes for copies count the groups they merge or link.
                let counts: Vec<i32> = if Some(c) == listed && c == Check::Duplicate {
                    offered.iter().map(|&f| self.fix_groups(f) as i32).collect()
                } else {
                    Vec::new()
                };
                let fixes: Vec<SharedString> =
                    offered.into_iter().map(|f| f.label().into()).collect();
                let case = case.filter(|_| c == Check::Duplicate);
                HealthCard {
                    title: c.title().into(),
                    about: case.map_or(c.about(), Case::about).into(),
                    count: case.map_or_else(
                        || self.health_count(c),
                        |case| self.dupe_tracks(Some(case)).len(),
                    ) as i32,
                    detail: self.health_detail(c).into(),
                    fixes: ModelRc::new(VecModel::from(fixes)),
                    counts: ModelRc::new(VecModel::from(counts)),
                    menu_fixes: ModelRc::new(VecModel::from(menu)),
                }
            })
            .collect();
        ui.set_view(self.health_view && self.tree == super::COLLECTION);
        let fixes: Vec<SharedString> = vec!["Bring back".into()];
        cards.push(HealthCard {
            title: "Ignored".into(),
            about: "Tracks you pressed I on in a review: each check leaves them alone. Bring back any that should be flagged again.".into(),
            count: self.ignored_tracks().len() as i32,
            detail: SharedString::new(),
            fixes: ModelRc::new(VecModel::from(fixes)),
            counts: ModelRc::default(),
            menu_fixes: ModelRc::new(VecModel::from(vec![0])),
        });
        let in_place = if self.shows_quarantine() {
            self.removed_in_place().len()
        } else {
            0
        };
        let qfixes = self.quarantine_fixes_with(in_place);
        let fixes: Vec<SharedString> = qfixes.iter().map(|f| f.label().into()).collect();
        let counts: Vec<i32> = qfixes
            .iter()
            .map(|f| match f {
                QuarantineFix::BringBack => self.removed as i32,
                QuarantineFix::Move => in_place as i32,
                QuarantineFix::ShowFolder => 0,
            })
            .collect();
        cards.push(HealthCard {
            title: "Quarantine".into(),
            about: "Tracks taken out of your collection: merged copies, quarantined and removed tracks. Bring back puts one back; a file in quarantine goes to In Prep. Files still in your folders can move to quarantine, out of the way.".into(),
            count: self.removed as i32,
            detail: SharedString::new(),
            fixes: ModelRc::new(VecModel::from(fixes)),
            counts: ModelRc::new(VecModel::from(counts)),
            menu_fixes: ModelRc::new(VecModel::from(vec![0])),
        });
        ui.set_cards(ModelRc::new(VecModel::from(cards)));
        ui.set_status(self.health_status.clone().into());
        ui.set_removed(self.removed as i32);
        ui.set_copies(on && self.dupe_view().is_some());
        ui.set_all_copies(self.all_copies);
        ui.set_quarantined(self.quarantined as i32);
        ui.set_lists(self.list_health_text().into());
        ui.set_quarantine_exists(self.quarantine_root().is_some_and(|d| d.is_dir()));
        ui.set_can_quarantine(self.saved() && self.quarantine_root().is_some());
        self.refresh_dashboard(&ui);
        self.refresh_health_page(&ui);
        self.refresh_review(&ui);
        self.refresh_cleanup_ui();
        self.refresh_lists_ui();
    }

    /// The Overview's numbers: the stages, how much is clean and what
    /// holds it back, and a tile per check.
    fn refresh_dashboard(&self, ui: &HealthUi) {
        let stages = [Stage::Inbox, Stage::Triage, Stage::Collection];
        let order = checks_in_order();
        let mut by_stage = [0usize; 3];
        let (mut total, mut clean) = (0, 0);
        // Tracks each check finds, and those it alone keeps from clean.
        let mut found = vec![0usize; order.len()];
        let mut only = vec![0usize; order.len()];
        for t in self.library.iter().filter(|t| self.in_collection(t)) {
            let finds: Vec<bool> = order
                .iter()
                .map(|&c| self.health.finds(c, t, &self.duplicates))
                .collect();
            let problems: Vec<usize> = order
                .iter()
                .zip(&finds)
                .enumerate()
                .filter(|(_, (c, &f))| f && c.area().problem())
                .map(|(i, _)| i)
                .collect();
            for (n, &f) in found.iter_mut().zip(&finds) {
                *n += usize::from(f);
            }
            if let [one] = problems[..] {
                only[one] += 1;
            }
            total += 1;
            clean += usize::from(problems.is_empty());
            if let Some(s) = stages.iter().position(|s| t.info.stage == s.name()) {
                by_stage[s] += 1;
            }
        }
        ui.set_total(total as i32);
        ui.set_clean(clean as i32);
        let now = percent(clean, total);
        ui.set_clean_percent(now);
        let staged: usize = by_stage.iter().sum();
        let stage_rows: Vec<HealthStage> = if self.folders.is_set() {
            stages
                .iter()
                .zip(by_stage)
                .map(|(stage, n)| HealthStage {
                    title: stage.title().into(),
                    count: n as i32,
                    percent: percent(n, staged).min(100) * i32::from(staged > 0),
                    note: SharedString::new(),
                })
                .collect()
        } else {
            Vec::new()
        };
        ui.set_stages(ModelRc::new(VecModel::from(stage_rows)));
        ui.set_staged(staged as i32);
        ui.set_wanted(self.store.wanted.len() as i32);
        // What holds the score back, the biggest first.
        let mut holds: Vec<HealthHold> = order
            .iter()
            .enumerate()
            .filter(|(i, c)| c.area().problem() && found[*i] > 0)
            .map(|(i, &c)| HealthHold {
                title: c.title().into(),
                count: found[i] as i32,
                gain: (percent(clean + only[i], total) - now).max(0),
                check: Check::ALL.iter().position(|&x| x == c).unwrap_or(0) as i32,
                state: check_state(c, found[i]) as i32,
            })
            .collect();
        holds.sort_by_key(|h| std::cmp::Reverse(h.count));
        ui.set_holds(ModelRc::new(VecModel::from(holds)));
        let counts: Vec<(Check, usize)> =
            order.iter().copied().zip(found.iter().copied()).collect();
        let start = start_check(&counts);
        let sections: Vec<HealthArea> = Area::ALL
            .iter()
            .map(|&a| {
                let lines: Vec<HealthLine> = counts
                    .iter()
                    .filter(|(c, _)| c.area() == a)
                    .map(|&(c, n)| HealthLine {
                        title: c.title().into(),
                        about: c.about().into(),
                        count: n as i32,
                        check: Check::ALL.iter().position(|&x| x == c).unwrap_or(0) as i32,
                        state: check_state(c, n) as i32,
                        start: start == Some(c),
                    })
                    .collect();
                HealthArea {
                    title: a.title().into(),
                    lines: ModelRc::new(VecModel::from(lines)),
                }
            })
            .collect();
        ui.set_areas(ModelRc::new(VecModel::from(sections)));
    }

    /// The check whose page is shown, while Collection Health is.
    pub(super) fn shown_check(&self) -> Option<Check> {
        self.listed_check().filter(|_| self.health_view)
    }

    /// The listed tracks (indices into `library`) in list order.
    fn listed(&self) -> Vec<usize> {
        self.rows.iter().flatten().copied().collect()
    }

    /// The first check after the one shown that still finds tracks.
    fn next_check(&self) -> Option<Check> {
        let now = self.shown_check()?;
        let order = checks_in_order();
        let from = order.iter().position(|&c| c == now)?;
        order[from + 1..]
            .iter()
            .copied()
            .find(|&c| self.health_count(c) > 0)
    }

    /// What Return does on the review card for the listed check, if
    /// anything.
    fn review_action(&self) -> Option<String> {
        if self.dupe_view().is_some() {
            return Some("Keep this copy".into());
        }
        let check = self.listed_check()?;
        let i = *self.listed().get(self.review_at.unwrap_or(0))?;
        self.first_fix(check, i).map(|f| f.label().to_string())
    }

    /// Shows the review card as things are now.
    fn refresh_review(&self, ui: &HealthUi) {
        let on = self.review_at.is_some() && self.shown_check().is_some();
        ui.set_reviewing(on);
        if !on {
            return;
        }
        let listed = self.listed();
        let at = self.review_at.unwrap_or(0);
        let next = self.next_check().map_or("", |c| c.title()).into();
        let Some(&i) = listed.get(at) else {
            ui.set_card(HealthReview {
                of: listed.len() as i32,
                done: true,
                next,
                ..Default::default()
            });
            return;
        };
        let t = &self.library[i];
        let (problem, fix) = self.health_cells(t);
        ui.set_card(HealthReview {
            at: at as i32 + 1,
            of: listed.len() as i32,
            check: self.listed_check().map_or("", |c| c.title()).into(),
            title: t.title.as_str().into(),
            artist: t.artist.as_str().into(),
            problem: problem.into(),
            fix: fix.into(),
            action: self.review_action().unwrap_or_default().into(),
            done: false,
            next,
        });
    }

    /// A key or button on the review card (see `ReviewCard` in the UI).
    fn review_key(&mut self, key: &str) {
        let listed = self.listed();
        let at = self.review_at.unwrap_or(0);
        let here = listed.get(at).copied();
        let row = |b: &Self| here.and_then(|i| b.rows.iter().position(|r| *r == Some(i)));
        match key {
            "start" => self.review_at = Some(0),
            "close" => self.review_at = None,
            "skip" => self.review_at = Some(at + 1),
            "next" => {
                if let Some(c) = self.next_check() {
                    self.open_check(c);
                    self.review_at = Some(0);
                }
            }
            "edit" => {
                if let Some(r) = row(self) {
                    self.show_info(r as i32);
                }
            }
            "use" | "ignore" => {
                let Some(i) = here else { return };
                if key == "ignore" {
                    let check = self.listed_check().unwrap_or(Check::Duplicate);
                    self.ignore(check, &[i]);
                } else if self.dupe_view().is_some() {
                    if let Some(r) = row(self) {
                        self.keep_this(r as i32);
                    }
                } else if let Some(fix) = self.listed_check().and_then(|c| self.first_fix(c, i)) {
                    self.fix(fix, &[i]);
                }
                // A fix that leaves the track listed (one that opens a
                // review table) moves on, so the card never sticks.
                if self.listed().get(at) == Some(&i) {
                    self.review_at = Some(at + 1);
                }
            }
            _ => return,
        }
        // The card's track is the current one, for Space to play it.
        let shown = self.listed().get(self.review_at.unwrap_or(0)).copied();
        let r = shown.and_then(|i| self.rows.iter().position(|r| *r == Some(i)));
        if let (Some(app), Some(r)) = (self.app.upgrade(), r) {
            app.set_current_track(r as i32);
        }
        self.refresh_health_ui();
    }

    /// The steps a check's page offers now, each with how many tracks (or
    /// groups) its button acts on; none of a fix not offered.
    fn page_steps(&self, check: Check) -> Vec<(Step, &'static str, &'static str, usize)> {
        let listed = self.rows.iter().flatten().count();
        let offered = self.offered(check);
        // What's left for a review once the sure fixes are made.
        let rest = match check {
            Check::Duplicate => self
                .dupe_groups()
                .iter()
                .filter(|g| !g.easy())
                .map(|g| g.tracks.len())
                .sum(),
            _ => listed.saturating_sub(self.sure_count(check)),
        };
        steps(check)
            .into_iter()
            .filter_map(|(step, title, about)| {
                let n = match step {
                    Step::Sure => self.sure_count(check),
                    Step::Review => rest,
                    // Duplicates' fixes work on their own kind of group,
                    // whichever Show tab is picked.
                    Step::Fix(f) if check == Check::Duplicate => self.fix_groups(f),
                    Step::Fix(f) if !offered.contains(&f) => return None,
                    Step::Fix(_) => listed,
                };
                Some((step, title, about, n))
            })
            .collect()
    }

    /// Runs step `i` of the shown check's page.
    fn run_step(&mut self, i: usize) {
        let Some(check) = self.listed_check() else {
            return;
        };
        let Some(&(step, ..)) = self.page_steps(check).get(i) else {
            return;
        };
        match step {
            Step::Sure => self.fix_sure(check),
            Step::Review => self.review_key("start"),
            Step::Fix(f) => {
                let all = self.picked_rows(-1, -1);
                self.fix(f, &all);
            }
        }
    }

    /// The shown check's page: its steps, tabs and progress.
    fn refresh_health_page(&self, ui: &HealthUi) {
        let check = self.shown_check();
        let steps: Vec<HealthStep> = check
            .map(|c| {
                self.page_steps(c)
                    .into_iter()
                    .map(|(step, title, about, n)| HealthStep {
                        title: title.into(),
                        about: about.into(),
                        label: match step {
                            Step::Sure if c == Check::NotAnalyzed => format!("Analyze {n}"),
                            Step::Sure if c == Check::Duplicate => format!("Merge {n}"),
                            Step::Sure if c == Check::Missing => format!("Relink {n}"),
                            Step::Sure => format!("Fix {n}"),
                            Step::Review => format!("Review {n}"),
                            Step::Fix(f) if n > 1 => format!("{} ({n})", f.label()),
                            Step::Fix(f) => f.label().to_string(),
                        }
                        .into(),
                        count: n as i32,
                    })
                    .collect()
            })
            .unwrap_or_default();
        // The first step with something to do is the one to do now.
        ui.set_step(
            steps
                .iter()
                .position(|s| s.count > 0)
                .map_or(-1, |i| i as i32),
        );
        ui.set_steps(ModelRc::new(VecModel::from(steps)));
        let tabs: Vec<HealthChip> = if check == Some(Check::Duplicate) {
            DUPE_TABS
                .iter()
                .map(|case| HealthChip {
                    title: case.map_or("All", Case::title).into(),
                    count: self.dupe_tracks(*case).len() as i32,
                })
                .collect()
        } else {
            Vec::new()
        };
        ui.set_chip(if tabs.is_empty() {
            -1
        } else {
            DUPE_TABS
                .iter()
                .position(|&c| c == self.dupe_tab)
                .unwrap_or(0) as i32
        });
        ui.set_chips(ModelRc::new(VecModel::from(tabs)));
        let (fixed, of) = check.map_or((0, 0), |c| self.check_progress(c));
        ui.set_fixed(fixed as i32);
        ui.set_fixed_of(of as i32);
    }

    /// A line on what a check found beyond its count.
    fn health_detail(&self, check: Check) -> String {
        match check {
            Check::MissingInfo => {
                // What the flagged tracks lack, the most often lacked first.
                let found = self.health_tracks(check);
                let lacking = |f: &dyn Fn(&Track) -> &str| {
                    found
                        .iter()
                        .filter(|&&i| health::says_nothing(f(&self.library[i])))
                        .count()
                };
                let mut counts = [
                    (
                        "artist or title",
                        lacking(&|t| {
                            if health::says_nothing(&t.artist) {
                                ""
                            } else {
                                &t.title
                            }
                        }),
                    ),
                    ("album", lacking(&|t| &t.album)),
                    ("year", lacking(&|t| &t.info.year)),
                    ("genre", lacking(&|t| &t.genre)),
                ];
                counts.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
                let parts: Vec<String> = counts
                    .iter()
                    .filter(|&&(_, n)| n > 0)
                    .map(|(what, n)| format!("{n} no {what}"))
                    .collect();
                if parts.is_empty() {
                    String::new()
                } else {
                    format!("{}.", parts.join(", "))
                }
            }
            Check::NoCover => {
                let unknown = self
                    .library
                    .iter()
                    .filter(|t| {
                        self.in_collection(t)
                            && !t.info.missing
                            && file_path(t).is_some()
                            && !self.health.cover_known(t.id)
                    })
                    .count();
                if unknown > 0 {
                    format!("Still looking in {}.", tracks(unknown))
                } else {
                    String::new()
                }
            }
            Check::Duplicate => {
                // Grouping reads the files, so only once it's looked at.
                let on_page = matches!(
                    self.nodes.get(self.current_node).map(|n| &n.contents),
                    Some(Contents::Health(None))
                ) || self.dupe_view().is_some();
                let groups = if on_page {
                    Some(self.dupe_groups())
                } else {
                    self.dupe_groups.get().map(Vec::as_slice)
                };
                let Some(groups) = groups.filter(|g| !g.is_empty()) else {
                    return String::new();
                };
                let count = |m: Match| groups.iter().filter(|g| g.matched == m).count();
                let (sure, likely, versions) = (
                    count(Match::Sure),
                    count(Match::Likely),
                    count(Match::Versions),
                );
                let mut s = format!("{sure} sure to merge at once");
                if likely > 0 {
                    s += &format!(", {likely} likely to look at");
                }
                if versions > 0 {
                    s += &format!(", {versions} of different lengths");
                }
                s + "."
            }
            Check::Stray if !self.folders.is_set() => {
                "Set your stage folders in Settings › Collection first.".into()
            }
            Check::Outside
                if settings::music_folders().is_empty() && self.folders.all().is_empty() =>
            {
                "Add a music folder in Settings › Collection first.".into()
            }
            _ => String::new(),
        }
    }

    /// Looks at the albums again for "Album tags differ", after album tags
    /// changed.
    pub(super) fn refresh_album_tags(&mut self) {
        let Some(albums) = self.folders.albums.clone() else {
            return;
        };
        let tracks: Vec<Arc<Track>> = self
            .library
            .iter()
            .filter(|t| self.in_collection(t))
            .cloned()
            .collect();
        self.health.set_albums(&albums, tracks.iter().map(|t| &**t));
    }

    /// Runs `fix` on library tracks (indices into `library`).
    fn fix(&mut self, fix: Fix, indices: &[usize]) {
        if indices.is_empty() || !self.saved() {
            return;
        }
        self.health_status = match fix {
            Fix::Remove => self.remove_tracks(indices),
            Fix::Relink => {
                let plan = self.relinks(indices, false);
                let left = indices.len() - plan.len();
                let mut status = if plan.is_empty() {
                    String::new()
                } else {
                    self.relink_sure(&plan) + " "
                };
                if left > 0 {
                    status += &format!(
                        "No one file found for {}: use Find file… or remove.",
                        tracks(left)
                    );
                }
                status.trim_end().to_string()
            }
            Fix::Locate => {
                self.locate(indices[0]);
                String::new()
            }
            Fix::MarkMusic => {
                self.ignore(Check::NotMusic, indices);
                format!("{} kept as music.", tracks(indices.len()))
            }
            Fix::ToTriage => {
                self.change_stage(indices, Stage::Triage, None);
                format!("{} moved to In Prep.", tracks(indices.len()))
            }
            Fix::FillFromName => self.review_file_names(indices),
            Fix::TagMix => self.tag_mix(indices),
            Fix::KeepBest => {
                let case = self.dupe_view().flatten();
                let plan = self
                    .picked_groups(indices, |c| case.map_or(c.easy(), |x| x == c))
                    .iter()
                    .map(|g| self.merge_plan(g, g.tracks[0]))
                    .collect();
                self.merge(plan)
            }
            Fix::LinkVersions => self.link_versions(indices),
            Fix::CleanUp => self.open_cleanup(indices),
            Fix::FindCovers => self.batch_covers(indices),
            Fix::LookUp => self.batch_lookup(indices),
            Fix::SameAlbum => self.review_album_tags(indices),
            Fix::ToWanted => self.missing_to_wanted(indices),
            Fix::FindInFolder => self.find_in_folder(indices),
        };
        // Removed tracks leave dead entries in lists.
        self.refresh_crates();
        self.refresh_tree();
        self.refresh_rows();
    }

    /// `check` leaves these tracks (indices into `library`) alone from now on.
    fn ignore(&mut self, check: Check, indices: &[usize]) {
        let ids: Vec<usize> = indices.iter().map(|&i| self.library[i].id).collect();
        if let Some(db) = &mut self.db {
            if let Err(e) = db.set_health_ignored(check, &ids, true) {
                eprintln!("can't save: {e}");
                return;
            }
        }
        self.health.set_ignored(check, &ids, true);
        self.refresh_dupes();
        self.health_status = format!("{} left out of {}.", tracks(ids.len()), check.title());
        self.refresh_tree();
        self.refresh_rows();
    }

    /// The tracks (indices into `library`) any check was told to leave
    /// alone.
    pub(super) fn ignored_tracks(&self) -> Vec<usize> {
        (0..self.library.len())
            .filter(|&i| {
                let t = &self.library[i];
                self.in_collection(t) && self.health.is_ignored(t.id)
            })
            .collect()
    }

    /// Every check looks at these tracks (indices into `library`) again.
    fn bring_back(&mut self, indices: &[usize]) {
        let ids: Vec<usize> = indices.iter().map(|&i| self.library[i].id).collect();
        let checks: HashSet<Check> = ids
            .iter()
            .flat_map(|&id| self.health.ignored_by(id))
            .collect();
        for check in checks {
            if let Some(db) = &mut self.db {
                if let Err(e) = db.set_health_ignored(check, &ids, false) {
                    eprintln!("can't save: {e}");
                    return;
                }
            }
            self.health.set_ignored(check, &ids, false);
        }
        self.refresh_dupes();
        self.health_status = format!("{} checked again.", tracks(ids.len()));
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Takes tracks out of the library, keeping their files.
    /// Puts missing tracks on the Wanted list, with a store search for each,
    /// and takes them out of the collection.
    fn missing_to_wanted(&mut self, indices: &[usize]) -> String {
        let Some(db) = &self.db else {
            return String::new();
        };
        let mut wanted = 0;
        for &i in indices {
            let t = &self.library[i];
            let words = format!("{} {}", t.artist, t.title);
            let url = format!(
                "https://bandcamp.com/search?q={}&item_type=t",
                opendj_plugins::encode(words.trim())
            );
            match db.add_wanted("Bandcamp", &t.artist, &t.title, &url) {
                Ok(_) => wanted += 1,
                Err(e) => eprintln!("can't add {} to Wanted: {e}", t.title),
            }
        }
        if let Ok(w) = db.wanted() {
            self.store.wanted = w;
        }
        let s = self.remove_tracks(indices);
        format!("{} on the Wanted list. {s}", tracks(wanted))
    }

    /// Asks for a folder and relinks the missing tracks whose files are
    /// found there for sure (see [`Self::poll_located`]).
    fn find_in_folder(&mut self, indices: &[usize]) -> String {
        let wanted: Vec<(usize, PathBuf, String, String, f64)> = indices
            .iter()
            .map(|&i| &self.library[i])
            .filter_map(|t| {
                Some((
                    t.id,
                    file_path(t)?.to_path_buf(),
                    t.artist.clone(),
                    t.title.clone(),
                    t.duration,
                ))
            })
            .collect();
        let tx = self.located.0.clone();
        crate::prefs::choose_folder("Look for the missing files in", move |dir| {
            let index = relocate::Index::new(audio_files(std::slice::from_ref(&dir)));
            for (id, path, artist, title, duration) in wanted {
                if let Lookup::Found(f) = index.find(&path, &artist, &title, duration) {
                    if f.sure {
                        let _ = tx.send((id, f.path));
                    }
                }
            }
        });
        String::new()
    }

    fn remove_tracks(&mut self, indices: &[usize]) -> String {
        let ids: Vec<usize> = indices.iter().map(|&i| self.library[i].id).collect();
        let Some(db) = &mut self.db else {
            return String::new();
        };
        match db.remove_tracks(&ids) {
            Ok(n) => {
                self.reload_library();
                format!(
                    "{} removed from your collection. The files are still there.",
                    tracks(n)
                )
            }
            Err(e) => format!("Couldn't remove them: {e}"),
        }
    }

    /// The Quarantine page's fixes, in its bar's order.
    fn quarantine_fixes(&self) -> Vec<QuarantineFix> {
        self.quarantine_fixes_with(self.removed_in_place().len())
    }

    fn quarantine_fixes_with(&self, in_place: usize) -> Vec<QuarantineFix> {
        let mut out = vec![QuarantineFix::BringBack];
        if in_place > 0 && self.quarantine_root().is_some() {
            out.push(QuarantineFix::Move);
        }
        if self.quarantine_root().is_some_and(|d| d.is_dir()) {
            out.push(QuarantineFix::ShowFolder);
        }
        out
    }

    /// Whether the Quarantine page is shown.
    pub(super) fn shows_quarantine(&self) -> bool {
        self.saved()
            && self.tree == super::COLLECTION
            && self
                .nodes
                .get(self.current_node)
                .is_some_and(|n| matches!(n.contents, Contents::Quarantine))
    }

    /// The tracks in rows `first` to `last` of the Quarantine page (every
    /// row when `first` is below 0), by id.
    fn picked_removed(&self, first: i32, last: i32) -> Vec<usize> {
        let rows: Vec<usize> = if first < 0 {
            (0..self.store.rows.len()).collect()
        } else {
            self.chosen_rows(first as usize, last.max(first) as usize)
        };
        rows.into_iter()
            .filter_map(|r| match self.store.rows.get(r) {
                Some(super::StoreRow::Removed(t)) => Some(t.id),
                _ => None,
            })
            .collect()
    }

    /// Brings tracks taken out back into the collection. One whose file
    /// went to quarantine goes to In Prep, to be looked at again.
    pub(super) fn bring_back_removed(&mut self, ids: &[usize]) {
        if ids.is_empty() {
            return;
        }
        let root = self.quarantine_root();
        let Some(db) = &mut self.db else { return };
        let moved: Vec<usize> = db
            .removed_tracks()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| ids.contains(&r.id))
            .filter(|r| root.as_ref().is_some_and(|q| r.path.starts_with(q)))
            .map(|r| r.id)
            .collect();
        self.health_status = match db.restore_tracks(Some(ids)) {
            Ok(n) => {
                self.reload_library();
                let indices: Vec<usize> = moved
                    .iter()
                    .filter_map(|id| self.library.iter().position(|t| t.id == *id))
                    .collect();
                if !indices.is_empty() {
                    self.change_stage(&indices, Stage::Triage, None);
                }
                match indices.len() {
                    0 => format!("{} back in your collection.", tracks(n)),
                    m => format!(
                        "{} back in your collection; {} out of quarantine into In Prep.",
                        tracks(n),
                        tracks(m)
                    ),
                }
            }
            Err(e) => format!("Couldn't bring them back: {e}"),
        };
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Brings back every track taken out of the library.
    fn restore_removed(&mut self) {
        let Some(db) = &mut self.db else { return };
        self.health_status = match db.restore_tracks(None) {
            Ok(n) => {
                self.reload_library();
                format!("{} back in your collection.", tracks(n))
            }
            Err(e) => format!("Couldn't bring them back: {e}"),
        };
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Reads the library again (after tracks left it or came back),
    /// staying on the node shown.
    pub(super) fn reload_library(&mut self) {
        // What it knows of the files found may have changed.
        self.relink_files = OnceCell::new();
        let Some(db) = &self.db else { return };
        let tracks = match db.all_tracks(Path::exists) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("can't read the library: {e}");
                return;
            }
        };
        let place = self
            .nodes
            .get(self.current_node)
            .map(|n| (n.name.clone(), n.depth, n.history));
        let status = std::mem::take(&mut self.health_status);
        // The side panel's lists (Automix) stay, by track id.
        let side: Vec<Vec<usize>> = self
            .side
            .iter()
            .map(|l| {
                l.iter()
                    .filter_map(|&i| self.library.get(i).map(|t| t.id))
                    .collect()
            })
            .collect();
        self.set_library(tracks.into_iter().map(Arc::new).collect());
        for (list, ids) in self.side.iter_mut().zip(side) {
            *list = ids
                .into_iter()
                .filter_map(|id| self.index_of.get(&id).copied())
                .collect();
        }
        self.refresh_side();
        self.health_status = status;
        if let Some((name, depth, history)) = place {
            if let Some(n) = self
                .nodes
                .iter()
                .position(|n| n.name == name && n.depth == depth && n.history == history)
            {
                if let Some(top) = self
                    .nodes
                    .iter()
                    .position(|n| matches!(n.contents, Contents::Health(None)))
                {
                    self.nodes[top].expanded |= matches!(
                        self.nodes[n].contents,
                        Contents::Health(_) | Contents::Dupes(_)
                    );
                }
                if matches!(self.nodes[n].contents, Contents::Dupes(_)) {
                    if let Some(d) = self.nodes.iter().position(|n| {
                        matches!(n.contents, Contents::Health(Some(Check::Duplicate)))
                    }) {
                        self.nodes[d].expanded = true;
                    }
                }
                self.current_node = n;
            }
        }
    }

    /// Missing tracks (indices into `library`) with a file found for
    /// them, and that file; only the sure ones with `sure`. A file two
    /// tracks would get is left for the DJ to pick.
    fn relinks(&self, indices: &[usize], sure: bool) -> Vec<(usize, PathBuf)> {
        let plan: Vec<(usize, PathBuf)> = indices
            .iter()
            .filter_map(|&i| match self.relink_lookup(&self.library[i]) {
                Lookup::Found(f) if f.sure || !sure => Some((i, f.path)),
                _ => None,
            })
            .collect();
        let mut taken: HashMap<&Path, usize> = HashMap::new();
        for (_, p) in &plan {
            *taken.entry(p).or_default() += 1;
        }
        plan.iter()
            .filter(|(_, p)| taken[p.as_path()] == 1)
            .cloned()
            .collect()
    }

    /// The relinks "Auto-fix" makes: the missing tracks found surely.
    fn sure_relinks(&self) -> Vec<(usize, PathBuf)> {
        self.relinks(&self.health_tracks(Check::Missing), true)
    }

    /// Asks for the file of a missing track (an index into `library`);
    /// [`Browser::poll_located`] relinks it.
    fn locate(&self, i: usize) {
        let id = self.library[i].id;
        let tx = self.located.0.clone();
        crate::prefs::choose_file("Find the track's file", move |path| {
            let _ = tx.send((id, path));
        });
    }

    /// Relinks missing tracks to the files picked for them.
    pub fn poll_located(&mut self) {
        let picked: Vec<(usize, PathBuf)> = self.located.1.try_iter().collect();
        let plan: Vec<(usize, PathBuf)> = picked
            .into_iter()
            .filter_map(|(id, p)| Some((*self.index_of.get(&id)?, p)))
            .collect();
        if plan.is_empty() || !self.saved() {
            return;
        }
        self.health_status = self.relink_sure(&plan);
        self.refresh_crates();
        self.refresh_tree();
        self.refresh_rows();
    }

    /// The fix the review card's ↵ makes for the track listed at `i` (an
    /// index into `library`): a missing file found is relinked, else
    /// asked for.
    fn first_fix(&self, check: Check, i: usize) -> Option<Fix> {
        if check == Check::Missing {
            return Some(match self.relink_lookup(&self.library[i]) {
                Lookup::Found(_) => Fix::Relink,
                _ => Fix::Locate,
            });
        }
        self.offered(check).first().copied()
    }

    /// What the clean-up rules take out of names that have junk in them,
    /// as fields to set (indices into `library`).
    fn sure_junk(&self) -> Vec<(usize, Field, String)> {
        self.health_tracks(Check::Junk)
            .into_iter()
            .flat_map(|i| {
                self.health
                    .junk(&self.library[i])
                    .into_iter()
                    .map(move |c| (i, c.field, c.new))
            })
            .collect()
    }

    /// How many tracks a check's sure fixes would fix: those with one
    /// clear answer (a file found once by name, an easy copy, junk the
    /// rules know). The rest are for review.
    pub(super) fn sure_count(&self, check: Check) -> usize {
        match check {
            Check::Missing => self.sure_relinks().len(),
            Check::Duplicate => self.dupe_faded(None).len(),
            Check::Junk => self
                .sure_junk()
                .iter()
                .map(|c| c.0)
                .collect::<HashSet<_>>()
                .len(),
            Check::NotAnalyzed => self.health_count(Check::NotAnalyzed),
            _ => 0,
        }
    }

    /// Makes a check's sure fixes, as one change the undo line takes back.
    fn fix_sure(&mut self, check: Check) {
        if !self.saved() {
            return;
        }
        self.health_status = match check {
            Check::Missing => {
                let plan = self.sure_relinks();
                self.relink_sure(&plan)
            }
            Check::Duplicate => {
                let plan = self
                    .dupe_groups()
                    .iter()
                    .filter(|g| g.easy())
                    .map(|g| self.merge_plan(g, g.tracks[0]))
                    .collect();
                self.merge(plan)
            }
            Check::Junk => {
                let changes = self.sure_junk();
                self.make_changes(changes, "clean-up")
            }
            Check::NotAnalyzed => {
                let picked: Vec<Arc<Track>> = self
                    .health_tracks(Check::NotAnalyzed)
                    .into_iter()
                    .map(|i| self.library[i].clone())
                    .collect();
                self.analysis.add(&picked);
                format!("Analyzing {} in the background.", tracks(picked.len()))
            }
            _ => return,
        };
        self.refresh_crates();
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Points missing tracks (indices into `library`) at the files found
    /// for them, as one change the undo line takes back.
    fn relink_sure(&mut self, plan: &[(usize, PathBuf)]) -> String {
        let Some(db) = &mut self.db else {
            return String::new();
        };
        let (mut relinked, mut taken) = (Vec::new(), 0);
        for (i, path) in plan {
            let t = &self.library[*i];
            let Some(old) = file_path(t).map(Path::to_path_buf) else {
                continue;
            };
            match db.relink(t.id, path) {
                Ok(()) => relinked.push((t.id, old)),
                // Another track has that file, with lists or cues of its own.
                Err(_) => taken += 1,
            }
        }
        self.relink_files = OnceCell::new();
        let n = relinked.len();
        if n > 0 {
            if let Err(e) = db.assign_stages(&self.folders) {
                eprintln!("can't give the found files their stage: {e}");
            }
            self.undo = Some(Undo {
                batch: None,
                text: format!("Undo finding {}", tracks(n)),
                restore: Vec::new(),
                changes: None,
                relinked,
            });
            self.reload_library();
        }
        let mut s = format!("Found {}.", tracks(n));
        if taken > 0 {
            s += &format!(" {taken} matched a track your collection has already: see Duplicates.");
        }
        s
    }

    /// Points tracks found elsewhere back at where they were before. The
    /// files found join the collection as tracks of their own again, as a
    /// scan had added them.
    pub(super) fn unlink(&mut self, back: &[(usize, PathBuf)]) {
        let found: Vec<Track> = back
            .iter()
            .filter_map(|(id, _)| file_path(&self.library[*self.index_of.get(id)?]))
            .filter(|p| p.exists())
            .map(|p| opendj_engine::scan::read_track(0, p))
            .collect();
        let Some(db) = &mut self.db else { return };
        let mut n = 0;
        for (id, old) in back {
            match db.relink(*id, old) {
                Ok(()) => n += 1,
                Err(e) => eprintln!("can't point the track back: {e}"),
            }
        }
        if let Err(e) = db.add_files(&found) {
            eprintln!("can't add the found files back: {e}");
        }
        if let Err(e) = db.assign_stages(&self.folders) {
            eprintln!("can't give the tracks their stage: {e}");
        }
        self.relink_files = OnceCell::new();
        self.reload_library();
        self.health_status = format!("{} missing again, as before.", tracks(n));
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Tags long tracks "mix", in the first tag group.
    fn tag_mix(&mut self, indices: &[usize]) -> String {
        let Some(group) = opendj_library::query::group_at(0) else {
            return "Make a tag group first, in Settings › Collection › Tags.".into();
        };
        let mut n = 0;
        for &i in indices {
            let mut t = (*self.library[i]).clone();
            if t.info.has_tag(group, MIX_TAG) {
                continue;
            }
            t.info.add_tag(group, MIX_TAG, false);
            if let Some(db) = &self.db {
                if let Err(e) = db.save_info(t.id, &t) {
                    eprintln!("can't save: {e}");
                    continue;
                }
            }
            self.health.set_ignored(Check::Long, &[t.id], true);
            if let Some(db) = &mut self.db {
                let _ = db.set_health_ignored(Check::Long, &[t.id], true);
            }
            self.indexed[i] = Indexed::new(&t);
            self.library[i] = Arc::new(t);
            n += 1;
        }
        format!("Tagged {} as \"{MIX_TAG}\".", tracks(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_check_has_a_node_and_fixes_fit() {
        let nodes = health_nodes();
        // Overview, a heading per section, every check, Ignored, Quarantine.
        assert_eq!(nodes.len(), 1 + Area::ALL.len() + Check::ALL.len() + 5);
        assert_eq!(nodes[0].name, "Overview");
        assert_eq!(nodes[1].name, "Files");
        assert_eq!(nodes[2].name, "Missing files");
        assert!(matches!(
            nodes.last().unwrap().contents,
            Contents::Quarantine
        ));
        assert_eq!(checks_in_order().len(), Check::ALL.len());
        assert_eq!(fixes(Check::Stray, false), [Fix::Remove]);
        assert_eq!(fixes(Check::Stray, true)[0], Fix::ToTriage);
        // Every fix a step names is one its check offers.
        // Duplicates' fixes are offered per group, not per check.
        for c in Check::ALL.into_iter().filter(|&c| c != Check::Duplicate) {
            for (step, ..) in steps(c) {
                if let Step::Fix(f) = step {
                    assert!(fixes(c, true).contains(&f), "{c:?} {f:?}");
                }
            }
        }
    }

    #[test]
    fn starts_with_the_first_problem_then_prep() {
        let counts = [
            (Check::Missing, 0),
            (Check::Long, 28),
            (Check::Junk, 5),
            (Check::NotAnalyzed, 9),
        ];
        assert_eq!(start_check(&counts), Some(Check::Long));
        assert_eq!(
            start_check(&[(Check::Long, 0), (Check::NotAnalyzed, 9)]),
            Some(Check::NotAnalyzed)
        );
        assert_eq!(start_check(&[(Check::Long, 0)]), None);
        assert_eq!(check_state(Check::Missing, 3), CheckState::Problem);
        assert_eq!(check_state(Check::NoRating, 3), CheckState::Goal);
        assert_eq!(check_state(Check::Junk, 0), CheckState::Clear);
        assert_eq!(percent(862, 1105), 78);
        assert_eq!(percent(0, 0), 100);
    }

    #[test]
    fn finds_files_by_name_below_folders() {
        let dir = std::env::temp_dir().join(format!("opendj-relink-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/Song.MP3"), b"x").unwrap();
        std::fs::write(dir.join("a/notes.txt"), b"x").unwrap();
        let found = audio_files(std::slice::from_ref(&dir));
        assert_eq!(found, [dir.join("a/b/Song.MP3")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
