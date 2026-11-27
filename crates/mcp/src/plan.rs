//! Set planning: putting tracks in mixing order, and what could come next,
//! with the same paths as Automix's Order menu (see
//! [`opendj_engine::mix_order`]).

use opendj_engine::key::{self, Key};
use opendj_engine::mix_order::{self, bpm_jump, key_steps, Item, Order};
use opendj_engine::Track;
use opendj_library::PlaylistKind;
use serde_json::{json, Map, Value};

use crate::tools::{
    brief, clean, db, find_list, ids_arg, int_arg, list_arg, named_tracks, r1, schema, search, Ctx,
    Result, Tool, MAX_TRACKS, NAME_LEN,
};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "plan_set",
            title: "Plan a set",
            description: "Puts tracks (a list, or track ids) in mixing order: flow (key and tempo together, the smoothest mixes), harmonic (key first), bpm_up or bpm_down. Each track comes with the key steps and tempo change from the one before. With save_as, saves the order as a new list.",
            schema: || {
                schema(
                    json!({
                        "list": list_arg(),
                        "track_ids": ids_arg(),
                        "order": {"type": "string", "enum": ["flow", "harmonic", "bpm_up", "bpm_down"], "description": "Default flow."},
                        "first": {"type": "integer", "description": "Track id to start with; it must be among the tracks."},
                        "save_as": {"type": "string", "description": "Name for a new list with this order."}
                    }),
                    &[],
                )
            },
            read_only: false,
        },
        Tool {
            name: "suggest_next",
            title: "Suggest the next track",
            description: "Tracks that mix well after a given one, best first: close in key (Camelot steps) and tempo (half and double time count). Picks from the whole library, a search, or a list.",
            schema: || {
                schema(
                    json!({
                        "track_id": {"type": "integer"},
                        "query": {"type": "string", "description": "Only tracks matching this search."},
                        "list": list_arg(),
                        "limit": {"type": "integer", "minimum": 1, "maximum": 50, "description": "Default 10."}
                    }),
                    &["track_id"],
                )
            },
            read_only: true,
        },
    ]
}

fn item(t: &Track) -> Item {
    Item {
        bpm: t.bpm,
        key: Key::parse(&t.key),
    }
}

/// The tracks of a list in order, refusing what holds no tracks of its own.
fn list_tracks(cx: &Ctx, arg: &Value) -> Result<Vec<Track>> {
    let list = find_list(cx, arg)?;
    match (list.kind, &list.query) {
        (PlaylistKind::Smart, Some(q)) => search(cx, q),
        (PlaylistKind::Folder | PlaylistKind::Set, _) => Err(format!(
            "{} holds lists, not tracks",
            clean(&list.name, NAME_LEN)
        )),
        _ => {
            let all = cx.by_id()?;
            Ok(cx
                .lib
                .playlist_tracks(list.id)
                .map_err(db)?
                .into_iter()
                .filter_map(|id| all.get(&id).cloned())
                .collect())
        }
    }
}

/// How a mix from `a` into `b` looks: Camelot steps and tempo change.
fn transition(a: &Track, b: &Track) -> Value {
    let steps = match (Key::parse(&a.key), Key::parse(&b.key)) {
        (Some(x), Some(y)) => json!(key_steps(x, y, key::mixing())),
        _ => Value::Null,
    };
    json!({
        "key_steps": steps,
        "bpm_change_pct": bpm_jump(a.bpm, b.bpm).map(r1),
    })
}

pub fn plan_set(cx: &mut Ctx, a: &Map<String, Value>) -> Result<Value> {
    let mut tracks = match (a.get("list"), a.get("track_ids")) {
        (Some(l), None) => list_tracks(cx, l)?,
        (None, Some(_)) => named_tracks(cx, a)?,
        _ => return Err("Give a list or track_ids".into()),
    };
    if tracks.len() > MAX_TRACKS {
        return Err(format!(
            "At most {MAX_TRACKS} tracks; this has {}",
            tracks.len()
        ));
    }
    // A track twice in a list is planned once.
    let mut seen = std::collections::HashSet::new();
    tracks.retain(|t| seen.insert(t.id));
    let order = match a.get("order").and_then(Value::as_str).unwrap_or("flow") {
        "flow" => Order::Flow,
        "harmonic" => Order::Harmonic,
        "bpm_up" => Order::BpmUp,
        "bpm_down" => Order::BpmDown,
        o => {
            return Err(format!(
                "`order` is flow, harmonic, bpm_up or bpm_down, not {o}"
            ))
        }
    };
    let mut fixed = 0;
    if a.contains_key("first") {
        let first = int_arg(a, "first")? as usize;
        let at = tracks
            .iter()
            .position(|t| t.id == first)
            .ok_or_else(|| format!("Track {first} isn't among the tracks"))?;
        let t = tracks.remove(at);
        tracks.insert(0, t);
        fixed = 1;
    }
    let items: Vec<Item> = tracks.iter().map(item).collect();
    let mut no_random = || 0;
    let ordered: Vec<&Track> =
        mix_order::order(&items, fixed, order, key::mixing(), &mut no_random)
            .into_iter()
            .map(|i| &tracks[i])
            .collect();
    let rows: Vec<Value> = ordered
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut v = brief(t);
            if i > 0 {
                v["from_previous"] = transition(ordered[i - 1], t);
            }
            v
        })
        .collect();
    let minutes = ordered.iter().map(|t| t.duration).sum::<f64>() / 60.0;
    let mut out = json!({"tracks": rows, "total_minutes": r1(minutes)});
    if let Some(name) = a.get("save_as").and_then(Value::as_str) {
        let name = clean(name.trim(), NAME_LEN);
        if name.is_empty() {
            return Err("`save_as` is empty".into());
        }
        let id = cx
            .lib
            .create_playlist(&name, None, PlaylistKind::List)
            .map_err(db)?;
        let ids: Vec<usize> = ordered.iter().map(|t| t.id).collect();
        cx.lib.add_to_playlist(id, &ids).map_err(db)?;
        cx.changed();
        out["saved_as"] = json!({"id": id, "name": name});
    }
    Ok(out)
}

pub fn suggest_next(cx: &mut Ctx, a: &Map<String, Value>) -> Result<Value> {
    let id = int_arg(a, "track_id")? as usize;
    let from = cx
        .by_id()?
        .remove(&id)
        .ok_or_else(|| format!("There's no track {id}"))?;
    let limit = match a.get("limit") {
        None | Some(Value::Null) => 10,
        Some(v) => v.as_i64().ok_or("`limit` is a number")?.clamp(1, 50) as usize,
    };
    let pool = match (a.get("list"), a.get("query").and_then(Value::as_str)) {
        (Some(l), _) => list_tracks(cx, l)?,
        (None, Some(q)) => search(cx, q)?,
        (None, None) => cx.tracks()?,
    };
    let here = item(&from);
    let mixing = key::mixing();
    let mut scored: Vec<(f64, &Track)> = pool
        .iter()
        .filter(|t| t.id != from.id && t.bpm > 0.0)
        .map(|t| {
            let there = item(t);
            let key = match (here.key, there.key) {
                (Some(x), Some(y)) => key_steps(x, y, mixing) as f64,
                _ => 4.0,
            };
            let tempo = bpm_jump(here.bpm, there.bpm).map_or(3.0, |j| (j.abs() / 6.0).min(5.0));
            (key + tempo, t)
        })
        .collect();
    scored.sort_by(|x, y| x.0.total_cmp(&y.0));
    let rows: Vec<Value> = scored
        .iter()
        .take(limit)
        .map(|(_, t)| {
            let mut v = brief(t);
            v["from_previous"] = transition(&from, t);
            v
        })
        .collect();
    Ok(json!({"after": brief(&from), "suggestions": rows}))
}
