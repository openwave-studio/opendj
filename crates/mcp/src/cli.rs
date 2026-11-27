//! `opendj <command>`: the library tools from a terminal or a script.
//!
//! Each command turns its words into one tool call. `--json` prints the
//! tool's result as it is; otherwise a short text form.

use std::io::Write;

use serde_json::{json, Value};

use crate::tools::{self, Ctx};
use crate::Config;

/// The commands the app binary hands to [`run`].
pub const COMMANDS: [&str; 7] = ["search", "track", "lists", "list", "rate", "tag", "mcp"];

const HELP: &str = "openDJ library commands (they never touch the decks)

  opendj search <words or fields> [--limit N]
  opendj track <id>
  opendj lists [--history]
  opendj list <list>                       tracks in a list (id or name)
  opendj list create <name> [--crate] [--in <folder>]
  opendj list add <list> <ids…> | --where <search>
  opendj rate <0-5> <ids…> | --where <search>
  opendj tag add|remove <group> <tag> <ids…> | --where <search>
  opendj mcp                               MCP server on stdin/stdout for AI assistants

  --json       print the result as JSON
  --dry-run    say what a change would do without making it

Searches use the browser's syntax: bpm:120-124 key:8A genre:house rating:4+";

/// Runs a command; returns the process exit code.
pub fn run(args: &[String], config: &Config) -> i32 {
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    let Some(command) = args.first().map(String::as_str) else {
        let _ = writeln!(out, "{HELP}");
        return 0;
    };
    if matches!(command, "help" | "--help" | "-h") {
        let _ = writeln!(out, "{HELP}");
        return 0;
    }
    let mut cx = match crate::open(config) {
        Ok(cx) => cx,
        Err(e) => {
            let _ = writeln!(err, "{e}");
            return 1;
        }
    };
    if command == "mcp" {
        let stdin = std::io::stdin().lock();
        return match crate::server::serve(&mut cx, stdin, out) {
            Ok(()) => 0,
            Err(e) => {
                let _ = writeln!(err, "{e}");
                1
            }
        };
    }
    let words = Words::new(&args[1..]);
    let json_out = words.flag("--json");
    match execute(&mut cx, command, words) {
        Ok((tool, v)) => {
            let _ = if json_out {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&v).unwrap_or_default()
                )
            } else {
                writeln!(out, "{}", text(tool, &v))
            };
            0
        }
        Err(e) => {
            let _ = writeln!(err, "{e}");
            1
        }
    }
}

/// A command's words, with its `--flags` picked out.
struct Words {
    plain: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

/// Flags that take the next word as their value.
const VALUED: [&str; 3] = ["--limit", "--where", "--in"];

impl Words {
    fn new(args: &[String]) -> Self {
        let mut plain = Vec::new();
        let mut flags = Vec::new();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            if VALUED.contains(&a.as_str()) {
                flags.push((a.clone(), it.next().cloned()));
            } else if a.starts_with("--") {
                flags.push((a.clone(), None));
            } else {
                plain.push(a.clone());
            }
        }
        Words { plain, flags }
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|(f, _)| f == name)
    }

    fn value(&self, name: &str) -> Result<Option<&str>, String> {
        match self.flags.iter().find(|(f, _)| f == name) {
            None => Ok(None),
            Some((_, Some(v))) => Ok(Some(v)),
            Some((_, None)) => Err(format!("{name} needs a value")),
        }
    }

    fn unknown_flag(&self, known: &[&str]) -> Result<(), String> {
        match self
            .flags
            .iter()
            .find(|(f, _)| f != "--json" && !known.contains(&f.as_str()))
        {
            Some((f, _)) => Err(format!("Unknown option {f}")),
            None => Ok(()),
        }
    }
}

/// The tool a command calls, and its result.
fn execute(cx: &mut Ctx, command: &str, w: Words) -> Result<(&'static str, Value), String> {
    let p = &w.plain;
    let dry = w.flag("--dry-run");
    let (tool, args) = match command {
        "search" => {
            w.unknown_flag(&["--limit"])?;
            let mut a = json!({"query": p.join(" ")});
            if let Some(n) = w.value("--limit")? {
                a["limit"] = json!(number(n)?);
            }
            ("search_tracks", a)
        }
        "track" => {
            w.unknown_flag(&[])?;
            let [id] = p.as_slice() else {
                return Err("Usage: opendj track <id>".into());
            };
            ("get_track", json!({"id": number(id)?}))
        }
        "lists" => {
            w.unknown_flag(&["--history"])?;
            (
                "list_lists",
                json!({"include_history": w.flag("--history")}),
            )
        }
        "list" => match p.first().map(String::as_str) {
            Some("create") if p.len() == 2 => {
                w.unknown_flag(&["--crate", "--in"])?;
                let mut a = json!({
                    "name": p[1],
                    "kind": if w.flag("--crate") { "crate" } else { "list" },
                });
                if let Some(f) = w.value("--in")? {
                    a["parent"] = list_value(f);
                }
                ("create_list", a)
            }
            Some("add") if p.len() >= 2 => {
                w.unknown_flag(&["--where", "--dry-run"])?;
                let ids = track_ids(cx, &p[2..], &w)?;
                (
                    "add_to_list",
                    json!({"list": list_value(&p[1]), "track_ids": ids, "dry_run": dry}),
                )
            }
            Some("create" | "add") => {
                return Err(
                    "Usage: opendj list create <name> | opendj list add <list> <ids…>".into(),
                )
            }
            Some(_) if p.len() == 1 => {
                w.unknown_flag(&["--limit"])?;
                let mut a = json!({"list": list_value(&p[0])});
                if let Some(n) = w.value("--limit")? {
                    a["limit"] = json!(number(n)?);
                }
                ("get_list", a)
            }
            _ => return Err("Usage: opendj list <list> (quote a name with spaces)".into()),
        },
        "rate" => {
            w.unknown_flag(&["--where", "--dry-run"])?;
            let Some(rating) = p.first() else {
                return Err("Usage: opendj rate <0-5> <ids…>".into());
            };
            let ids = track_ids(cx, &p[1..], &w)?;
            (
                "set_rating",
                json!({"rating": number(rating)?, "track_ids": ids, "dry_run": dry}),
            )
        }
        "tag" => {
            w.unknown_flag(&["--where", "--dry-run"])?;
            let (Some(verb), Some(group), Some(tag)) = (p.first(), p.get(1), p.get(2)) else {
                return Err("Usage: opendj tag add|remove <group> <tag> <ids…>".into());
            };
            let remove = match verb.as_str() {
                "add" => false,
                "remove" => true,
                _ => return Err("Usage: opendj tag add|remove <group> <tag> <ids…>".into()),
            };
            let ids = track_ids(cx, &p[3..], &w)?;
            (
                "tag_tracks",
                json!({"group": group, "tag": tag, "remove": remove, "track_ids": ids, "dry_run": dry}),
            )
        }
        _ => return Err(format!("Unknown command {command}; try opendj help")),
    };
    Ok((tool, tools::call(cx, tool, &args)?))
}

/// The tracks a write names: ids, or `--where` with a search.
fn track_ids(cx: &Ctx, ids: &[String], w: &Words) -> Result<Vec<i64>, String> {
    match w.value("--where")? {
        Some(_) if !ids.is_empty() => Err("Give track ids or --where, not both".into()),
        Some(q) => {
            let found = tools::search(cx, q)?;
            if found.len() > tools::MAX_TRACKS {
                return Err(format!(
                    "That search finds {} tracks; at most {} at a time",
                    found.len(),
                    tools::MAX_TRACKS
                ));
            }
            Ok(found.iter().map(|t| t.id as i64).collect())
        }
        None => ids.iter().map(|s| number(s)).collect(),
    }
}

fn number(s: &str) -> Result<i64, String> {
    s.parse().map_err(|_| format!("{s} is not a number"))
}

/// A list named on the command line: its id when it's a number.
fn list_value(s: &str) -> Value {
    s.parse::<i64>().map_or_else(|_| json!(s), |n| json!(n))
}

/// A result as a person reads it.
fn text(tool: &str, v: &Value) -> String {
    let s = |v: &Value, k: &str| v[k].as_str().unwrap_or("").to_string();
    let track_line = |t: &Value| {
        let mut name = format!("{} - {}", s(t, "artist"), s(t, "title"));
        if !s(t, "remix").is_empty() {
            name.push_str(&format!(" ({})", s(t, "remix")));
        }
        let bpm = t["bpm"]
            .as_f64()
            .map(|b| format!("{b:.1}"))
            .unwrap_or_default();
        let stars = "★".repeat(t["rating"].as_u64().unwrap_or(0) as usize);
        format!("{:>6}  {name}  {bpm}  {}  {stars}", t["id"], s(t, "key"))
    };
    let tracks = |v: &Value| {
        let list: Vec<String> = v["tracks"]
            .as_array()
            .map(|a| a.iter().map(track_line).collect())
            .unwrap_or_default();
        let shown = list.len() as u64;
        let total = v["total"].as_u64().unwrap_or(shown);
        let mut out = list.join("\n");
        if total > shown {
            out.push_str(&format!("\n… {shown} of {total} shown (--limit)"));
        } else if total == 0 {
            out.push_str("No tracks.");
        }
        out
    };
    let dry = |v: &Value| {
        if v["dry_run"].as_bool() == Some(true) {
            " (dry run, nothing changed)"
        } else {
            ""
        }
    };
    match tool {
        "search_tracks" => tracks(v),
        "get_list" => format!("{} ({})\n{}", s(v, "name"), s(v, "kind"), tracks(v)),
        "get_track" => {
            let mut out = track_line(v);
            for (k, label) in [
                ("album", "Album"),
                ("genre", "Genre"),
                ("label", "Label"),
                ("year", "Year"),
                ("grouping", "Grouping"),
                ("comment", "Comment"),
                ("file", "File"),
            ] {
                if !s(v, k).is_empty() {
                    out.push_str(&format!("\n  {label}: {}", s(v, k)));
                }
            }
            let tags: Vec<String> = v["tags"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|t| format!("{}: {}", s(t, "group"), s(t, "tag")))
                        .collect()
                })
                .unwrap_or_default();
            if !tags.is_empty() {
                out.push_str(&format!("\n  Tags: {}", tags.join(", ")));
            }
            let lists: Vec<String> = v["lists"]
                .as_array()
                .map(|a| a.iter().map(|l| s(l, "name")).collect())
                .unwrap_or_default();
            if !lists.is_empty() {
                out.push_str(&format!("\n  In: {}", lists.join(", ")));
            }
            out.push_str(&format!(
                "\n  Played {} times, {} hot cues",
                v["play_count"], v["hot_cues"]
            ));
            out
        }
        "list_lists" => v["lists"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|l| {
                        let n = l["tracks"]
                            .as_u64()
                            .map(|n| format!("  {n} tracks"))
                            .unwrap_or_default();
                        format!("{:>6}  {} ({}){n}", l["id"], s(l, "name"), s(l, "kind"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        "create_list" => format!("Made {} {} ({})", s(v, "kind"), s(v, "name"), v["id"]),
        "add_to_list" => format!(
            "Added {}, {} already in the list{}",
            v["added"],
            v["already_in"],
            dry(v)
        ),
        "set_rating" | "tag_tracks" => format!(
            "Changed {}, {} already so{}",
            v["changed"],
            v["unchanged"],
            dry(v)
        ),
        _ => v.to_string(),
    }
}
