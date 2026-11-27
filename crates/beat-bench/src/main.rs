//! Scores openDJ's beat grids against tracks whose beats are known.
//!
//!     cargo run --release -p beat-bench -- [--dir DIR] [--no-rendered]
//!         [--only NAME] [--standard | --neural] [--write DIR] [--csv]
//!
//! Two kinds of tracks:
//!
//! - rendered here (see [`render`]): dance-music test tracks drawn with the
//!   Groove Box's kits, each aimed at something that trips grids up, so
//!   every beat and bar 1 is known exactly. On by default.
//! - `--dir DIR`: audio files beside a `.beats` file of the same name, one
//!   beat a line as `seconds [position]`, position 1 a downbeat (the format
//!   of the GTZAN and Ballroom annotations; `fetch-gtzan.sh` downloads 30
//!   GTZAN excerpts and their beats). Nothing here ships their audio.
//!
//! Each track is analysed as the app does it: the standard (DSP) grid, then
//! with Beat This! refining it (models from `OPENDJ_MODELS`, else the app's
//! model folder; missing ones are downloaded with `curl` from the
//! `stem-models` release, as the app does). For each grid it prints:
//!
//! - tempo: the main BPM, and whether it is right (within 4 %), at half or
//!   double, or wrong;
//! - beats and bars: F-measure of the grid's beats and of its bar lines
//!   (beat 0 and every fourth beat after it) within 70 ms of the reference;
//! - bar 1: how many beats the grid's bars sit from the true ones (0 is
//!   right), the most common offset over the track;
//! - continuity (CMLt, AMLt): how much of the track the grid stays locked
//!   on, at the right level and at any related level;
//! - with Beat This!, the network's own beat and downbeat F-measure before
//!   the grid is fitted through them, to tell its misses from the fit's.
//!
//! `--write DIR` saves the rendered tracks as WAV with their `.beats`, to
//! listen to them or load them in another app.

mod metrics;
mod render;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use opendj_engine::analysis::{self, Analysis};
use opendj_engine::beats::BeatGrid;
use opendj_engine::track::Audio;
use opendj_engine::{beatthis, decode};

use metrics::{TempoVerdict, TOLERANCE};

struct Options {
    dir: Option<PathBuf>,
    rendered: bool,
    only: String,
    standard: bool,
    neural: bool,
    write: Option<PathBuf>,
    csv: bool,
}

fn options() -> Result<Options, String> {
    let mut o = Options {
        dir: None,
        rendered: true,
        only: String::new(),
        standard: true,
        neural: true,
        write: None,
        csv: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--dir" => o.dir = Some(value()?.into()),
            "--no-rendered" => o.rendered = false,
            "--only" => o.only = value()?,
            "--standard" => o.neural = false,
            "--neural" => o.standard = false,
            "--write" => o.write = Some(value()?.into()),
            "--csv" => o.csv = true,
            "--list" => {
                for (name, about) in render::names() {
                    println!("{name:24} {about}");
                }
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument {a}")),
        }
    }
    Ok(o)
}

struct Case {
    set: &'static str,
    name: String,
    about: &'static str,
    audio: Audio,
    beats: Vec<f64>,
    downbeats: Vec<f64>,
}

/// How one grid scored on one track.
struct Score {
    bpm: f64,
    tempo: TempoVerdict,
    beat_f: f64,
    bar_f: Option<f64>,
    /// The most common distance, in beats, from the grid's bar lines to the
    /// true ones, and the share of bars at it.
    bar_offset: Option<(i64, f64)>,
    cmlt: f64,
    amlt: f64,
    unsure: bool,
    /// Beat This!'s own beats and downbeats, before the fit.
    raw: Option<(f64, Option<f64>)>,
}

fn grid_beats(grid: &BeatGrid, duration: f64) -> (Vec<f64>, Vec<f64>) {
    if grid.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let (from, to) = (
        grid.beat_at(0.0).ceil() as i64,
        grid.beat_at(duration).floor() as i64,
    );
    let beats: Vec<(i64, f64)> = (from..=to)
        .map(|k| (k, grid.seconds_at(k as f64)))
        .collect();
    let bars = beats
        .iter()
        .filter(|(k, _)| k.rem_euclid(4) == 0)
        .map(|b| b.1)
        .collect();
    (beats.into_iter().map(|b| b.1).collect(), bars)
}

fn score(case: &Case, a: &Analysis) -> Score {
    let duration = case.audio.duration();
    let (beats, bars) = grid_beats(&a.grid, duration);
    let ref_bpm = metrics::tempo(&case.beats);
    let (cmlt, amlt) = metrics::continuity(&case.beats, &beats).unwrap_or((0.0, 0.0));
    let bar_offset = (!case.downbeats.is_empty() && !a.grid.is_empty()).then(|| {
        let mut count = [0usize; 4];
        for &d in &case.downbeats {
            count[(a.grid.beat_at(d).round() as i64).rem_euclid(4) as usize] += 1;
        }
        let (k, n) = count.iter().enumerate().max_by_key(|c| c.1).unwrap();
        // Beats from the true bar 1 to the grid's: 3 reads as one early.
        let k = if k == 3 { -1 } else { k as i64 };
        (k, *n as f64 / case.downbeats.len() as f64)
    });
    let raw = a.neural_beats.then(|| {
        (
            metrics::f_measure(&case.beats, &a.beats, TOLERANCE).unwrap_or(0.0),
            metrics::f_measure(&case.downbeats, &a.downbeats, TOLERANCE),
        )
    });
    Score {
        bpm: a.bpm,
        tempo: metrics::tempo_verdict(ref_bpm, a.bpm),
        beat_f: metrics::f_measure(&case.beats, &beats, TOLERANCE).unwrap_or(0.0),
        bar_f: metrics::f_measure(&case.downbeats, &bars, TOLERANCE),
        bar_offset,
        cmlt,
        amlt,
        unsure: a.grid_unsure,
        raw,
    }
}

fn load_dir(dir: &Path, only: &str) -> Vec<Case> {
    let mut paths: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(d) => d.flatten().map(|e| e.path()).collect(),
        Err(e) => {
            eprintln!("{}: {e}", dir.display());
            return Vec::new();
        }
    };
    paths.sort();
    let mut cases = Vec::new();
    for p in paths {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "beats" || p.is_dir() {
            continue;
        }
        let name = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let Ok(text) = fs::read_to_string(p.with_extension("beats")) else {
            continue;
        };
        if !name.contains(only) {
            continue;
        }
        let (mut beats, mut downbeats) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let mut cols = line.split_whitespace();
            let Some(t) = cols.next().and_then(|t| t.parse::<f64>().ok()) else {
                continue;
            };
            beats.push(t);
            if cols.next().and_then(|c| c.parse::<f64>().ok()) == Some(1.0) {
                downbeats.push(t);
            }
        }
        match decode::decode(&p) {
            Ok(audio) => cases.push(Case {
                set: "dir",
                name,
                about: "",
                audio,
                beats,
                downbeats,
            }),
            Err(e) => eprintln!("{}: {e}", p.display()),
        }
    }
    cases
}

fn write_rendered(dir: &Path, case: &Case) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let frames: Vec<[f32; 2]> = case.audio.decoded_chunks().flatten().copied().collect();
    let mut wav = Vec::with_capacity(44 + frames.len() * 4);
    let data = (frames.len() * 4) as u32;
    let sr = case.audio.sample_rate;
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&sr.to_le_bytes());
    wav.extend_from_slice(&(sr * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    for f in &frames {
        for v in f {
            wav.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
    }
    fs::write(dir.join(format!("{}.wav", case.name)), wav)?;
    let mut beats = fs::File::create(dir.join(format!("{}.beats", case.name)))?;
    let mut n = 0;
    for &b in &case.beats {
        let pos = if case.downbeats.iter().any(|d| (d - b).abs() < 1e-9) {
            n = 1;
            1
        } else {
            n = n % 4 + 1;
            n
        };
        writeln!(beats, "{b:.6}\t{pos}")?;
    }
    Ok(())
}

fn fmt_opt(v: Option<f64>) -> String {
    v.map_or("-".into(), |v| format!("{v:.2}"))
}

fn verdict(v: TempoVerdict) -> String {
    match v {
        TempoVerdict::Right => "ok".into(),
        TempoVerdict::Related(r) => r.into(),
        TempoVerdict::Wrong => "WRONG".into(),
        TempoVerdict::None => "none".into(),
    }
}

fn offset(o: Option<(i64, f64)>) -> String {
    match o {
        None => "-".into(),
        Some((0, share)) => format!("ok {:.0}%", share * 100.0),
        Some((k, share)) => format!("{k:+} {:.0}%", share * 100.0),
    }
}

/// Where the app downloads its models from (see `crates/app/src/update.rs`).
const MODELS_URL: &str = "https://github.com/openwave-studio/opendj/releases/download/stem-models";

/// Lets the engine download Beat This!'s models with `curl`, checking each
/// file's SHA-256.
fn fetch_models_with_curl() {
    opendj_engine::separation::set_fetch(Box::new(|name, hash, to| {
        eprintln!("downloading {name}");
        let url = format!("{MODELS_URL}/{name}");
        let ok = Command::new("curl")
            .args(["-fsSL", "-o"])
            .arg(to)
            .arg(&url)
            .status()
            .map_err(|e| format!("curl: {e}"))?
            .success();
        if !ok {
            return Err(format!("couldn't download {url}"));
        }
        let sum = ["sha256sum", "shasum -a 256"]
            .iter()
            .find_map(|cmd| {
                let mut parts = cmd.split(' ');
                let out = Command::new(parts.next()?)
                    .args(parts)
                    .arg(to)
                    .output()
                    .ok()?;
                out.status
                    .success()
                    .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
            })
            .ok_or("no sha256sum or shasum to check the download")?;
        if sum.split_whitespace().next() != Some(hash) {
            return Err(format!("{name} doesn't match its SHA-256"));
        }
        Ok(())
    }));
}

fn main() {
    fetch_models_with_curl();
    let o = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let mut cases = Vec::new();
    if o.rendered {
        for r in render::render_all(&o.only) {
            cases.push(Case {
                set: "rendered",
                name: r.name.to_string(),
                about: r.about,
                audio: r.audio,
                beats: r.beats,
                downbeats: r.downbeats,
            });
        }
    }
    if let Some(dir) = &o.write {
        for c in cases.iter().filter(|c| c.set == "rendered") {
            if let Err(e) = write_rendered(dir, c) {
                eprintln!("{}: {e}", dir.display());
            }
        }
    }
    if let Some(dir) = &o.dir {
        cases.extend(load_dir(dir, &o.only));
    }
    if cases.is_empty() {
        eprintln!("no tracks to score");
        std::process::exit(1);
    }

    let modes: Vec<&str> = [("Standard", o.standard), ("Beat This!", o.neural)]
        .into_iter()
        .filter(|m| m.1)
        .map(|m| m.0)
        .collect();
    if o.csv {
        println!("set,track,ref_bpm,mode,bpm,tempo,beat_f,bar_f,bar_offset,bar_share,cmlt,amlt,unsure,net_beat_f,net_downbeat_f");
    }
    let mut totals: Vec<(String, Vec<Score>)> = Vec::new();
    for case in &cases {
        let ref_bpm = metrics::tempo(&case.beats);
        if !o.csv {
            println!(
                "{} ({}, {:.1} BPM, {:.0} s) {}",
                case.name,
                case.set,
                ref_bpm,
                case.audio.duration(),
                case.about
            );
        }
        let base = analysis::analyse(&case.audio, None);
        for &mode in &modes {
            let mut a = base.clone();
            if mode == "Beat This!" {
                if let Err(e) = beatthis::refine(&mut a, &case.audio, None) {
                    eprintln!("  Beat This!: {e}");
                    continue;
                }
            }
            let s = score(case, &a);
            if o.csv {
                let (net_b, net_d) = s.raw.map_or((None, None), |r| (Some(r.0), r.1));
                println!(
                    "{},{},{:.2},{},{:.2},{},{:.3},{},{},{},{:.3},{:.3},{},{},{}",
                    case.set,
                    case.name,
                    ref_bpm,
                    mode,
                    s.bpm,
                    verdict(s.tempo),
                    s.beat_f,
                    s.bar_f.map_or(String::new(), |v| format!("{v:.3}")),
                    s.bar_offset.map_or(String::new(), |b| b.0.to_string()),
                    s.bar_offset
                        .map_or(String::new(), |b| format!("{:.2}", b.1)),
                    s.cmlt,
                    s.amlt,
                    s.unsure,
                    net_b.map_or(String::new(), |v| format!("{v:.3}")),
                    net_d.map_or(String::new(), |v| format!("{v:.3}")),
                );
            } else {
                let raw = s.raw.map_or(String::new(), |(b, d)| {
                    format!("  net beats {b:.2} downbeats {}", fmt_opt(d))
                });
                println!(
                    "  {mode:10} {:7.2} BPM {:6}  beats {:.2}  bars {}  bar 1 {:8}  CMLt {:.2} AMLt {:.2}{}{raw}",
                    s.bpm,
                    verdict(s.tempo),
                    s.beat_f,
                    fmt_opt(s.bar_f),
                    offset(s.bar_offset),
                    s.cmlt,
                    s.amlt,
                    if s.unsure { "  unsure" } else { "" },
                );
            }
            let key = format!("{} / {mode}", case.set);
            match totals.iter_mut().find(|t| t.0 == key) {
                Some(t) => t.1.push(s),
                None => totals.push((key, vec![s])),
            }
        }
    }
    if o.csv {
        return;
    }
    println!();
    for (key, scores) in &totals {
        let n = scores.len() as f64;
        let mean = |f: &dyn Fn(&Score) -> Option<f64>| {
            let v: Vec<f64> = scores.iter().filter_map(f).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        let right = scores
            .iter()
            .filter(|s| s.tempo == TempoVerdict::Right)
            .count();
        let related = scores
            .iter()
            .filter(|s| matches!(s.tempo, TempoVerdict::Related(_)))
            .count();
        let bar_ok = scores
            .iter()
            .filter(|s| matches!(s.bar_offset, Some((0, _))))
            .count();
        println!(
            "{key}: {} tracks, tempo right {right} (+{related} half/double), bar 1 right {bar_ok}, beats {}, bars {}, CMLt {}, AMLt {}",
            n,
            fmt_opt(mean(&|s| Some(s.beat_f))),
            fmt_opt(mean(&|s| s.bar_f)),
            fmt_opt(mean(&|s| Some(s.cmlt))),
            fmt_opt(mean(&|s| Some(s.amlt))),
        );
    }
}
