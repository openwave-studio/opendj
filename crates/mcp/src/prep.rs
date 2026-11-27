//! Prep work: analysis and high-quality stems for a batch of tracks, and a
//! look at what the collection is missing.
//!
//! Analysis and stems never run while a deck plays, in this process or in
//! a running openDJ (see [`background::playing_anywhere`]). The command
//! line waits for the decks to stop; an assistant gets told what was done
//! and why it stopped, rather than a call that hangs for a whole set.
//! Stems also take the heavy-work lock, so the app and a command never
//! separate at the same time, and a separation in progress gives up as
//! soon as a deck plays.

use std::collections::HashSet;
use std::time::Duration;

use opendj_engine::{background, loader, separation, stems, Source, Track};
use opendj_library::query;
use serde_json::{json, Map, Value};

use crate::tools::{
    bool_arg, brief, db, dry_run_arg, ids_arg, named_tracks, schema, Ctx, Job, Result, Tool,
};

/// The most tracks one stems call separates: each takes minutes.
pub const STEMS_MAX: usize = 20;

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "analyze_tracks",
            title: "Analyze tracks",
            description: "Finds tempo, beat grid, key and phrases for tracks, as openDJ does in the background. Tracks already analysed are skipped unless fresh is true. Never runs while a deck plays: if one starts, the call stops and says how far it got. Reports progress; can be cancelled.",
            schema: || {
                schema(
                    json!({
                        "track_ids": ids_arg(),
                        "fresh": {"type": "boolean", "description": "Analyse again even when already analysed (a locked grid stays)."}
                    }),
                    &["track_ids"],
                )
            },
            read_only: false,
        },
        Tool {
            name: "separate_stems",
            title: "Separate stems",
            description: "Makes high-quality stems for up to 20 tracks (minutes each), kept in openDJ's cache for the decks. Tracks with stems already are skipped. Never runs while a deck plays: if one starts, the call stops and says how far it got. Waits while openDJ itself separates. Reports progress; can be cancelled.",
            schema: || {
                schema(
                    json!({"track_ids": {"type": "array", "items": {"type": "integer"}, "maxItems": STEMS_MAX}, "dry_run": dry_run_arg()}),
                    &["track_ids"],
                )
            },
            read_only: false,
        },
        Tool {
            name: "health_report",
            title: "Collection health",
            description: "What the collection is missing: files not found, tracks not analysed, no genre, no rating, and duplicates, with counts and up to 10 example tracks each.",
            schema: || schema(json!({}), &[]),
            read_only: true,
        },
    ]
}

/// Whether a track still needs its tempo, key or grid found.
fn needs_analysis(t: &Track) -> bool {
    !t.info.missing && (t.bpm <= 0.0 || t.key.is_empty() || t.grid.is_empty())
}

/// Before each track: false when the batch must stop (cancelled, or a deck
/// plays and the job doesn't wait), else true once the decks are quiet.
fn clear_to_go(job: &Job, done: usize, total: usize) -> std::result::Result<(), &'static str> {
    loop {
        if (job.cancelled)() {
            return Err("cancelled");
        }
        if !background::playing_anywhere() {
            return Ok(());
        }
        if !job.wait_while_playing {
            return Err("a deck is playing");
        }
        (job.progress)(done as f64, total as f64, "Waiting: a deck is playing");
        std::thread::sleep(Duration::from_secs(1));
    }
}

pub fn analyze(cx: &mut Ctx, a: &Map<String, Value>, job: &Job) -> Result<Value> {
    let tracks = named_tracks(cx, a)?;
    let fresh = bool_arg(a, "fresh");
    let todo: Vec<&Track> = tracks
        .iter()
        .filter(|t| !t.info.missing && (fresh || needs_analysis(t)))
        .collect();
    let total = todo.len();
    let mut analysed = 0;
    let mut failed = Vec::new();
    let mut stopped = None;
    // Below the decks and the UI of a running openDJ.
    background::lower_priority();
    for (i, t) in todo.iter().enumerate() {
        if let Err(why) = clear_to_go(job, i, total) {
            stopped = Some(why);
            break;
        }
        (job.progress)(i as f64, total as f64, &label(t));
        match loader::analyse_file(t, fresh) {
            Ok(found) => {
                cx.lib.save_analysis(t.id, &found).map_err(db)?;
                analysed += 1;
            }
            Err(e) => failed.push(json!({"id": t.id, "error": crate::tools::clean(&e, 200)})),
        }
    }
    if analysed > 0 {
        cx.changed();
    }
    Ok(json!({
        "analysed": analysed,
        "already_analysed": tracks.len() - total,
        "failed": failed,
        "stopped": stopped,
        "left": total - analysed - failed.len(),
    }))
}

pub fn stems(cx: &mut Ctx, a: &Map<String, Value>, job: &Job) -> Result<Value> {
    let asked = a
        .get("track_ids")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if asked > STEMS_MAX {
        return Err(format!(
            "At most {STEMS_MAX} tracks per call (each takes minutes); this names {asked}"
        ));
    }
    let tracks = named_tracks(cx, a)?;
    let todo: Vec<(&Track, &std::path::Path)> = tracks
        .iter()
        .filter_map(|t| match &t.source {
            Source::File(p) if !t.info.missing && stems::find(p).is_none() => {
                Some((t, p.as_path()))
            }
            _ => None,
        })
        .collect();
    let total = todo.len();
    if bool_arg(a, "dry_run") {
        return Ok(
            json!({"would_separate": total, "have_stems": tracks.len() - total, "dry_run": true}),
        );
    }
    let mut separated = 0;
    let mut failed = Vec::new();
    let mut stopped = None;
    let mut i = 0;
    while i < total {
        let (t, path) = todo[i];
        if let Err(why) = clear_to_go(job, i, total) {
            stopped = Some(why);
            break;
        }
        let name = label(t);
        // Waits here while openDJ itself separates.
        (job.progress)(i as f64, total as f64, &name);
        let lock = background::heavy_lock();
        // A deck may have started while it waited.
        if background::playing_anywhere() || (job.cancelled)() {
            drop(lock);
            continue;
        }
        let keep_going = || !(job.cancelled)() && !background::playing_anywhere();
        let mut progress = |done: usize, of: usize| {
            let part = done as f64 / of.max(1) as f64;
            (job.progress)(i as f64 + part, total as f64, &name);
        };
        let result = separation::separate_hq_watched(path, &[], &mut progress, &keep_going);
        drop(lock);
        if !keep_going() {
            // Stopped part way: this track starts again once the decks stop
            // (the command line), or is left for later.
            continue;
        }
        match result {
            Ok(_) => separated += 1,
            Err(e) => failed.push(json!({"id": t.id, "error": crate::tools::clean(&e, 200)})),
        }
        i += 1;
    }
    Ok(json!({
        "separated": separated,
        "have_stems": tracks.len() - total,
        "failed": failed,
        "stopped": stopped,
        "left": total - separated - failed.len(),
    }))
}

pub fn health(cx: &Ctx) -> Result<Value> {
    let all = cx.lib.all_tracks(cx.exists_fn()).map_err(db)?;
    let dupes = query::duplicates(all.iter().filter(|t| !t.info.missing));
    let group = |name: &str, pick: &dyn Fn(&Track) -> bool| {
        let found: Vec<&Track> = all.iter().filter(|t| pick(t)).collect();
        json!({
            "check": name,
            "tracks": found.len(),
            "examples": found.iter().take(10).map(|t| brief(t)).collect::<Vec<_>>(),
        })
    };
    let seen: HashSet<usize> = dupes;
    Ok(json!({
        "tracks": all.len(),
        "checks": [
            group("file not found", &|t| t.info.missing),
            group("not analysed", &|t| needs_analysis(t)),
            group("no genre", &|t| !t.info.missing && t.genre.trim().is_empty()),
            group("no rating", &|t| !t.info.missing && t.rating == 0),
            group("duplicates", &|t| seen.contains(&t.id)),
        ],
    }))
}

fn label(t: &Track) -> String {
    crate::tools::clean(&format!("{} - {}", t.artist, t.title), 120)
}
