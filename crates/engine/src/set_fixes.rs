//! Quick fixes to a recorded set: trim, a track's level, a cut, and
//! loudness for where the mix is going. The recording is never changed:
//! the fixes are a list saved beside it (`<name>.edits.json`), and Save
//! renders them into a new file, `<name> (edited).wav` or `.flac`.
//!
//! Everything happens in the recording's own time: a track's level is a
//! gain over its stretch, eased at both ends; a cut takes out a stretch
//! with a short equal-power crossfade at the join; a trim keeps a stretch
//! with a fade at each end. Loudness is measured on the result of those and
//! reached with one gain, then the true-peak limiter at the ceiling.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::dsp::Limiter;
use crate::loudness::{LoudnessMeter, TruePeak};
use crate::recorder::{audio_sink, RecordFormat};

/// The crossfade at a cut's join, in seconds.
pub const CUT_CROSSFADE: f64 = 0.02;
/// Default fades for a trim, in seconds.
pub const TRIM_FADE_IN: f64 = 2.0;
pub const TRIM_FADE_OUT: f64 = 5.0;
/// Loudness presets: (name, LUFS, true-peak ceiling in dBTP).
pub const LOUDNESS_PRESETS: [(&str, f64, f64); 3] = [
    ("Streaming and Mixcloud", -14.0, -1.0),
    ("Apple Music and podcasts", -16.0, -1.0),
    ("Club and radio", -9.0, -1.0),
];
/// More limiting than this, in dB, is worth a warning: the mix will sound
/// squashed.
pub const LIMITING_WARNING: f64 = 3.0;
/// Frames written at a time.
const WRITE_BLOCK: usize = 4096;

/// One fix.
#[derive(Clone, Debug, PartialEq)]
pub enum Fix {
    /// Keep `start..end` (seconds into the recording), fading in and out.
    Trim {
        start: f64,
        end: f64,
        fade_in: f64,
        fade_out: f64,
    },
    /// `db` over `start..end`, eased in and out over `ease` seconds (a
    /// beat). The recording is the whole mix, so this moves whatever else
    /// plays there too.
    TrackLevel {
        start: f64,
        end: f64,
        db: f64,
        ease: f64,
        /// The track, for the list of fixes.
        title: String,
    },
    /// Take out `start..end`.
    Cut { start: f64, end: f64 },
    /// Bring the whole mix to `lufs`, true peaks under `ceiling` dBTP.
    Loudness { lufs: f64, ceiling: f64 },
}

impl Fix {
    fn to_json(&self) -> Value {
        match self {
            Fix::Trim {
                start,
                end,
                fade_in,
                fade_out,
            } => json!({ "fix": "trim", "start": start, "end": end,
                         "fade_in": fade_in, "fade_out": fade_out }),
            Fix::TrackLevel {
                start,
                end,
                db,
                ease,
                title,
            } => json!({ "fix": "track_level", "start": start, "end": end,
                         "db": db, "ease": ease, "title": title }),
            Fix::Cut { start, end } => json!({ "fix": "cut", "start": start, "end": end }),
            Fix::Loudness { lufs, ceiling } => {
                json!({ "fix": "loudness", "lufs": lufs, "ceiling": ceiling })
            }
        }
    }

    fn from_json(v: &Value) -> Option<Fix> {
        let f = |k: &str| v[k].as_f64();
        Some(match v["fix"].as_str()? {
            "trim" => Fix::Trim {
                start: f("start")?,
                end: f("end")?,
                fade_in: f("fade_in").unwrap_or(TRIM_FADE_IN),
                fade_out: f("fade_out").unwrap_or(TRIM_FADE_OUT),
            },
            "track_level" => Fix::TrackLevel {
                start: f("start")?,
                end: f("end")?,
                db: f("db")?,
                ease: f("ease").unwrap_or(0.5),
                title: v["title"].as_str().unwrap_or_default().to_string(),
            },
            "cut" => Fix::Cut {
                start: f("start")?,
                end: f("end")?,
            },
            "loudness" => Fix::Loudness {
                lufs: f("lufs")?,
                ceiling: f("ceiling").unwrap_or(-1.0),
            },
            _ => return None,
        })
    }
}

/// The fixes to a recording, in the order they were made (undo takes the
/// last off).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edits {
    pub fixes: Vec<Fix>,
}

impl Edits {
    pub fn to_json(&self) -> String {
        let v = json!({
            "opendj_edits": 1,
            "fixes": self.fixes.iter().map(Fix::to_json).collect::<Vec<_>>(),
        });
        serde_json::to_string_pretty(&v).unwrap_or_default()
    }

    /// Fixes this version doesn't know are skipped.
    pub fn from_json(text: &str) -> Result<Edits, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let fixes = v["fixes"]
            .as_array()
            .ok_or("no fixes")?
            .iter()
            .filter_map(Fix::from_json)
            .collect();
        Ok(Edits { fixes })
    }

    /// The fixes saved beside the recording whose first file is `audio`;
    /// none when there are none yet.
    pub fn load(audio: &Path) -> Edits {
        std::fs::read_to_string(edits_path(audio))
            .ok()
            .and_then(|t| Edits::from_json(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, audio: &Path) -> std::io::Result<()> {
        std::fs::write(edits_path(audio), self.to_json())
    }

    /// The last trim, if any.
    fn trim(&self) -> Option<(f64, f64, f64, f64)> {
        self.fixes.iter().rev().find_map(|f| match *f {
            Fix::Trim {
                start,
                end,
                fade_in,
                fade_out,
            } => Some((start, end, fade_in, fade_out)),
            _ => None,
        })
    }

    /// The last loudness preset, if any: (LUFS, ceiling dBTP).
    pub fn loudness(&self) -> Option<(f64, f64)> {
        self.fixes.iter().rev().find_map(|f| match *f {
            Fix::Loudness { lufs, ceiling } => Some((lufs, ceiling)),
            _ => None,
        })
    }

    /// Where a time in the recording lands in the edited file (None when
    /// it was cut or trimmed off), for moving the tracklist and chapters.
    pub fn map_time(&self, t: f64) -> Option<f64> {
        let removed = self.removed(f64::INFINITY);
        let mut shift = 0.0;
        for r in &removed {
            if t >= r.start && t < r.end {
                return None;
            }
            if t >= r.end {
                shift += r.end - r.start + if r.crossfade { CUT_CROSSFADE } else { 0.0 };
            }
        }
        Some((t - shift).max(0.0))
    }

    /// The first moment at or after `t` that the fixes keep (None when
    /// everything from `t` on is trimmed off).
    pub fn next_kept(&self, t: f64) -> Option<f64> {
        match self
            .removed(f64::INFINITY)
            .iter()
            .find(|r| t >= r.start && t < r.end)
        {
            Some(r) if r.end.is_finite() => Some(r.end),
            Some(_) => None,
            None => Some(t),
        }
    }

    /// The stretches taken out, in order, joined where they overlap.
    fn removed(&self, length: f64) -> Vec<Removed> {
        let mut out: Vec<Removed> = self
            .fixes
            .iter()
            .filter_map(|f| match *f {
                Fix::Cut { start, end } if end > start => Some(Removed {
                    start,
                    end,
                    crossfade: true,
                }),
                _ => None,
            })
            .collect();
        if let Some((start, end, _, _)) = self.trim() {
            if start > 0.0 {
                out.push(Removed {
                    start: 0.0,
                    end: start,
                    crossfade: false,
                });
            }
            if end < length {
                out.push(Removed {
                    start: end,
                    end: f64::INFINITY,
                    crossfade: false,
                });
            }
        }
        out.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut joined: Vec<Removed> = Vec::new();
        for r in out {
            match joined.last_mut() {
                Some(last) if r.start <= last.end => {
                    last.end = last.end.max(r.end);
                    last.crossfade &= r.crossfade;
                }
                _ => joined.push(r),
            }
        }
        joined
    }

    /// The gain at `t` from track levels and the trim's fades.
    fn gain(&self, t: f64) -> f64 {
        let mut g = 1.0;
        for f in &self.fixes {
            if let Fix::TrackLevel {
                start,
                end,
                db,
                ease,
                ..
            } = *f
            {
                if t < start - ease / 2.0 || t > end + ease / 2.0 {
                    continue;
                }
                // Eased from half a beat before to half a beat after each
                // end, so the change sits on the edge given.
                let ease = ease.max(1e-3);
                let k_in = ((t - (start - ease / 2.0)) / ease).clamp(0.0, 1.0);
                let k_out = (((end + ease / 2.0) - t) / ease).clamp(0.0, 1.0);
                let k = smooth(k_in.min(k_out));
                g *= 10f64.powf(db * k / 20.0);
            }
        }
        if let Some((start, end, fade_in, fade_out)) = self.trim() {
            if fade_in > 0.0 && t < start + fade_in {
                g *= smooth(((t - start) / fade_in).clamp(0.0, 1.0));
            }
            if fade_out > 0.0 && t > end - fade_out {
                g *= smooth(((end - t) / fade_out).clamp(0.0, 1.0));
            }
        }
        g
    }
}

/// An S-curve from 0 to 1.
fn smooth(k: f64) -> f64 {
    k * k * (3.0 - 2.0 * k)
}

#[derive(Clone, Copy, Debug)]
struct Removed {
    start: f64,
    end: f64,
    crossfade: bool,
}

/// `<name>.edits.json` beside the recording whose first file is `audio`.
pub fn edits_path(audio: &Path) -> PathBuf {
    let (log, _) = crate::set_timeline::log_paths(audio);
    let name = log.file_name().unwrap_or_default().to_string_lossy();
    let stem = name
        .strip_suffix(".controls.jsonl")
        .unwrap_or(&name)
        .to_string();
    audio.with_file_name(format!("{stem}.edits.json"))
}

/// `<name> (edited).wav` (or `.flac`) beside the recording.
pub fn edited_path(audio: &Path, format: RecordFormat) -> PathBuf {
    let (log, _) = crate::set_timeline::log_paths(audio);
    let name = log.file_name().unwrap_or_default().to_string_lossy();
    let stem = name
        .strip_suffix(".controls.jsonl")
        .unwrap_or(&name)
        .to_string();
    audio.with_file_name(format!("{stem} (edited).{}", format.name()))
}

/// Applies the gains, cuts and trim to `frames` (at `rate`), passing each
/// frame of the result to `out`.
fn apply(
    frames: impl IntoIterator<Item = [f32; 2]>,
    rate: f64,
    edits: &Edits,
    mut out: impl FnMut([f32; 2]),
) {
    let removed = edits.removed(f64::INFINITY);
    let w = ((CUT_CROSSFADE * rate).round() as usize).max(1);
    // The last `w` frames kept, held back for a crossfade at a cut.
    let mut held: VecDeque<[f32; 2]> = VecDeque::with_capacity(w + 1);
    let mut next = 0;
    // A crossfade under way: frames of it done.
    let mut fading: Option<usize> = None;
    let mut entered = false;
    for (i, x) in frames.into_iter().enumerate() {
        let t = i as f64 / rate;
        while next < removed.len() && t >= removed[next].end {
            if entered && removed[next].crossfade {
                fading = Some(0);
            }
            entered = false;
            next += 1;
        }
        if next < removed.len() && t >= removed[next].start {
            entered = true;
            continue;
        }
        let g = edits.gain(t) as f32;
        let y = x.map(|v| v * g);
        match fading {
            Some(k) if k < w && !held.is_empty() => {
                let a = (k as f64 + 0.5) / w as f64 * std::f64::consts::FRAC_PI_2;
                let (go, gi) = (a.cos() as f32, a.sin() as f32);
                let o = held.pop_front().unwrap();
                out([o[0] * go + y[0] * gi, o[1] * go + y[1] * gi]);
                fading = Some(k + 1);
            }
            _ => {
                if fading.is_some() {
                    // What wasn't used of the tail (a cut right after another).
                    while let Some(o) = held.pop_front() {
                        out(o);
                    }
                    fading = None;
                }
                held.push_back(y);
                if held.len() > w {
                    out(held.pop_front().unwrap());
                }
            }
        }
    }
    for o in held {
        out(o);
    }
}

/// What a render did, for the Fixes panel's before and after.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rendered {
    /// Loudness of the mix after trims, levels and cuts, before the preset.
    pub before_lufs: Option<f64>,
    /// Loudness of the file written.
    pub after_lufs: Option<f64>,
    /// The gain the preset applied, in dB.
    pub gain_db: f64,
    /// How far the limiter had to pull the loudest peak down, in dB.
    pub limiting_db: f64,
    /// Seconds written.
    pub seconds: f64,
}

impl Rendered {
    /// Whether the limiting is heavy enough to warn about.
    pub fn squashed(&self) -> bool {
        self.limiting_db > LIMITING_WARNING
    }
}

/// Measures what a loudness preset would do: the mix's loudness after the
/// other fixes, the gain to the target, and the limiting that would take.
pub fn measure(frames: impl IntoIterator<Item = [f32; 2]>, rate: f64, edits: &Edits) -> Rendered {
    let mut meter = LoudnessMeter::new(rate);
    let mut tp = TruePeak::new();
    let mut peak = 0f32;
    let mut n = 0usize;
    apply(frames, rate, edits, |f| {
        meter.push(f);
        peak = peak.max(tp.push(f));
        n += 1;
    });
    let before = meter.finish().map(|l| l.lufs as f64);
    let mut r = Rendered {
        before_lufs: before,
        after_lufs: before,
        seconds: n as f64 / rate,
        ..Default::default()
    };
    if let (Some((lufs, ceiling)), Some(before)) = (edits.loudness(), before) {
        r.gain_db = lufs - before;
        let peak_db = 20.0 * (peak.max(1e-9) as f64).log10();
        r.limiting_db = (peak_db + r.gain_db - ceiling).max(0.0);
        r.after_lufs = Some(lufs);
    }
    r
}

/// Renders the fixes: `frames` gives the recording from the start each
/// time it's called (twice when there is a loudness preset: once to
/// measure). Every frame of the result goes to `out`.
pub fn render_frames<I: IntoIterator<Item = [f32; 2]>>(
    mut frames: impl FnMut() -> I,
    rate: f64,
    edits: &Edits,
    mut out: impl FnMut([f32; 2]),
) -> Rendered {
    let Some((_, ceiling)) = edits.loudness() else {
        let mut meter = LoudnessMeter::new(rate);
        let mut n = 0usize;
        apply(frames(), rate, edits, |f| {
            meter.push(f);
            n += 1;
            out(f);
        });
        let lufs = meter.finish().map(|l| l.lufs as f64);
        return Rendered {
            before_lufs: lufs,
            after_lufs: lufs,
            seconds: n as f64 / rate,
            ..Default::default()
        };
    };
    let planned = measure(frames(), rate, edits);
    let gain = 10f64.powf(planned.gain_db / 20.0) as f32;
    let mut limiter = Limiter::new(rate, 10f64.powf(ceiling / 20.0) as f32);
    let latency = limiter.latency();
    let mut meter = LoudnessMeter::new(rate);
    let mut skipped = 0usize;
    let mut n = 0usize;
    let mut emit = |f: [f32; 2], out: &mut dyn FnMut([f32; 2])| {
        if skipped < latency {
            skipped += 1;
            return;
        }
        meter.push(f);
        n += 1;
        out(f);
    };
    let mut written = 0usize;
    apply(frames(), rate, edits, |f| {
        written += 1;
        let y = limiter.process(f.map(|v| v * gain));
        emit(y, &mut out);
    });
    for _ in 0..latency.min(written) {
        let y = limiter.process([0.0; 2]);
        emit(y, &mut out);
    }
    Rendered {
        after_lufs: meter.finish().map(|l| l.lufs as f64),
        seconds: n as f64 / rate,
        ..planned
    }
}

/// Renders the fixes to the recording at `audio` (its first file) into
/// `dest`, without holding the set in memory.
pub fn render(
    audio: &Path,
    dest: &Path,
    format: RecordFormat,
    bits: u16,
    edits: &Edits,
) -> Result<Rendered, String> {
    let err = |e: std::io::Error| format!("{}: {e}", dest.display());
    // The rate, from a first look at the file.
    let mut rate = 0u32;
    // Stopped after the first packet, which "cancels" the decode.
    let _ = crate::decode::Decoder::open(audio)?.stream(|r, _| {
        rate = r;
        false
    });
    if rate == 0 {
        return Err(format!("{}: no audio decoded", audio.display()));
    }
    let rate = rate as f64;
    let mut sink = audio_sink(dest, format, bits, rate).map_err(err)?;
    let mut buf: Vec<f32> = Vec::with_capacity(WRITE_BLOCK * 2);
    let mut failed: Option<std::io::Error> = None;
    let workers = std::cell::RefCell::new(Vec::new());
    let rendered = render_frames(
        || {
            // Each pass streams the file again, a block at a time from a
            // decoding thread, so the set is never held whole.
            let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<[f32; 2]>>(4);
            let path = audio.to_path_buf();
            workers.borrow_mut().push(std::thread::spawn(move || {
                let mut chunk = Vec::with_capacity(WRITE_BLOCK);
                let r = crate::decode::Decoder::open(&path).and_then(|d| {
                    d.stream(|_, samples| {
                        chunk.extend_from_slice(samples.as_chunks::<2>().0);
                        if chunk.len() >= WRITE_BLOCK {
                            let full =
                                std::mem::replace(&mut chunk, Vec::with_capacity(WRITE_BLOCK));
                            // The renderer stopped reading: stop decoding.
                            return tx.send(full).is_ok();
                        }
                        true
                    })
                });
                let _ = tx.send(chunk);
                r
            }));
            rx.into_iter().flatten()
        },
        rate,
        edits,
        |f| {
            buf.extend_from_slice(&f);
            if buf.len() >= WRITE_BLOCK * 2 {
                if let Err(e) = sink.write(&buf) {
                    failed.get_or_insert(e);
                }
                buf.clear();
            }
        },
    );
    for w in workers.into_inner() {
        match w.join() {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(format!("{}: decoding failed", audio.display())),
        }
    }
    if !buf.is_empty() {
        sink.write(&buf).map_err(err)?;
    }
    if let Some(e) = failed {
        return Err(err(e));
    }
    sink.finish().map_err(err)?;
    Ok(rendered)
}

#[cfg(test)]
mod tests;
