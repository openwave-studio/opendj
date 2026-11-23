//! Finding lyrics on LRCLIB (lrclib.net), a free community database of
//! synced lyrics with no key. On by default, and switched off in Settings ›
//! Collection › Health; asked when the DJ presses Find in the Lyrics tab,
//! and for each track the DJ asks to analyse that has no lyrics yet (see
//! [`closest`]), one request at a time, waiting as long as LRCLIB's
//! `Retry-After` asks, with openDJ's User-Agent as LRCLIB asks. Nothing is
//! ever sent to LRCLIB but the track's names and length.

use std::path::{Path, PathBuf};
use std::time::Duration;

use opendj_engine::lyrics;
use opendj_plugins::{encode, USER_AGENT};
use serde_json::Value;

use crate::settings;

/// The switch in Settings › Collection › Health: on at first.
pub const ENABLED: &str = "lyrics_lookup";

/// The switch for finding lyrics when tracks are analysed: on at first,
/// and only asked when [`ENABLED`] is on too.
pub const ON_ANALYZE: &str = "lyrics_on_analyze";

/// Lengths within this many seconds match; LRCLIB matches the same way.
const SAME_LENGTH: f64 = 2.0;

const API: &str = "https://lrclib.net/api";

pub fn enabled() -> bool {
    settings::flag(ENABLED, true)
}

/// Whether analysing a track also finds its lyrics.
pub fn on_analyze() -> bool {
    enabled() && settings::flag(ON_ANALYZE, true)
}

/// Where openDJ keeps lyrics for tracks whose folder can't be written.
pub fn store() -> Option<PathBuf> {
    settings::config_dir().map(|d| d.join("lyrics"))
}

/// One record LRCLIB has.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Seconds.
    pub duration: f64,
    pub instrumental: bool,
    /// LRC text, timed by line (sometimes by word); empty when none.
    pub synced: String,
    pub plain: String,
}

impl Found {
    /// The lyrics to save: the synced ones when there are any.
    pub fn text(&self) -> &str {
        if self.synced.trim().is_empty() {
            &self.plain
        } else {
            &self.synced
        }
    }

    pub fn is_synced(&self) -> bool {
        !self.synced.trim().is_empty()
    }

    /// Whether its length is close to `seconds`, so its timing fits.
    pub fn fits(&self, seconds: f64) -> bool {
        seconds <= 0.0 || (self.duration - seconds).abs() <= SAME_LENGTH
    }
}

/// What a track is looked up by.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
}

fn record(v: &Value) -> Option<Found> {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let found = Found {
        title: s("trackName"),
        artist: s("artistName"),
        album: s("albumName"),
        duration: v["duration"].as_f64().unwrap_or(0.0),
        instrumental: v["instrumental"].as_bool().unwrap_or(false),
        synced: s("syncedLyrics"),
        plain: s("plainLyrics"),
    };
    (!found.title.is_empty()).then_some(found)
}

/// Best first: synced lyrics of the right length, then plain of the right
/// length, then the rest by how close their length is. Same lyrics only once.
fn rank(mut found: Vec<Found>, duration: f64) -> Vec<Found> {
    let mut seen = std::collections::HashSet::new();
    found.retain(|f| seen.insert((f.text().to_string(), f.instrumental)));
    found.sort_by(|a, b| {
        let key = |f: &Found| {
            (
                !f.fits(duration),
                f.instrumental,
                !f.is_synced(),
                f.text().trim().is_empty(),
            )
        };
        key(a).cmp(&key(b)).then(
            (a.duration - duration)
                .abs()
                .total_cmp(&(b.duration - duration).abs()),
        )
    });
    found.truncate(12);
    found
}

/// The lyrics to save for a track `duration` seconds long without asking:
/// the best of what [`find`] ranked. Lyrics of another length are taken
/// only as plain words, as their timing wouldn't fit; instrumental records
/// give nothing.
pub fn closest(found: &[Found], duration: f64) -> Option<String> {
    let best = found
        .iter()
        .find(|f| !f.instrumental && !f.text().trim().is_empty())?;
    if best.fits(duration) {
        return Some(best.text().to_string());
    }
    let plain = found
        .iter()
        .filter(|f| !f.instrumental && !f.plain.trim().is_empty())
        .min_by(|a, b| {
            (a.duration - duration)
                .abs()
                .total_cmp(&(b.duration - duration).abs())
        })?;
    Some(plain.plain.clone())
}

/// What became of looking up lyrics for a track being analysed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fetched {
    /// The closest lyrics were saved for it.
    Saved,
    /// It has lyrics already, or nothing to look it up by.
    Skipped,
    /// LRCLIB had nothing to use.
    Missed,
    /// LRCLIB couldn't be reached, or the lyrics couldn't be saved.
    Failed,
}

/// Finds and saves the closest lyrics for the file at `path`, unless it has
/// lyrics already (beside it, in its tags or in openDJ's own folder). Runs
/// on the caller's thread.
pub fn fetch_closest(path: &Path, q: &Query) -> Fetched {
    let store = store();
    if q.title.trim().is_empty() || lyrics::find_with(path, store.as_deref()).is_some() {
        return Fetched::Skipped;
    }
    let found = match find(q) {
        Ok(found) => found,
        Err(Miss::None) => return Fetched::Missed,
        Err(Miss::Offline) => return Fetched::Failed,
    };
    let Some(text) = closest(&found, q.duration) else {
        return Fetched::Missed;
    };
    match lyrics::save(path, store.as_deref(), &text) {
        Ok(_) => Fetched::Saved,
        Err(e) => {
            eprintln!("can't save the lyrics found for {}: {e}", q.title);
            Fetched::Failed
        }
    }
}

/// Why a request gave nothing back.
enum Failed {
    /// LRCLIB answered that it has no such thing.
    NotThere,
    /// LRCLIB couldn't be reached, or kept saying it was busy.
    Unreachable,
}

/// How many times a request asked to wait is tried again.
const RETRIES: u32 = 2;
/// The longest a `Retry-After` is waited for.
const MOST_WAIT: u64 = 60;

/// Asks LRCLIB for JSON, waiting and asking again when it answers 429 with
/// a `Retry-After`.
fn get_json(url: &str) -> Result<Value, Failed> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .user_agent(USER_AGENT)
        .http_status_as_error(false)
        .build()
        .into();
    for _ in 0..=RETRIES {
        let mut r = agent
            .get(url)
            .header("Accept", "application/json")
            .call()
            .map_err(|_| Failed::Unreachable)?;
        match r.status().as_u16() {
            200 => return r.body_mut().read_json().map_err(|_| Failed::Unreachable),
            404 => return Err(Failed::NotThere),
            429 | 503 => {
                let wait = r
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .unwrap_or(5);
                std::thread::sleep(Duration::from_secs(wait.min(MOST_WAIT)));
            }
            _ => return Err(Failed::Unreachable),
        }
    }
    Err(Failed::Unreachable)
}

/// Why nothing came back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Miss {
    /// LRCLIB has nothing for the track.
    None,
    /// LRCLIB couldn't be reached.
    Offline,
}

/// Looks the track up: the exact match first (artist, title, album and
/// length), then a search by artist and title. Runs on the caller's thread.
pub fn find(q: &Query) -> Result<Vec<Found>, Miss> {
    let mut found = Vec::new();
    let mut reached = false;
    if q.duration > 0.0 {
        let mut url = format!(
            "{API}/get?artist_name={}&track_name={}&duration={}",
            encode(&q.artist),
            encode(&q.title),
            q.duration.round() as u64
        );
        if !q.album.trim().is_empty() {
            url.push_str(&format!("&album_name={}", encode(&q.album)));
        }
        match get_json(&url) {
            Ok(v) => {
                reached = true;
                found.extend(record(&v));
            }
            Err(Failed::NotThere) => reached = true,
            Err(Failed::Unreachable) => {}
        }
    }
    let mut url = format!("{API}/search?track_name={}", encode(&q.title));
    if !q.artist.trim().is_empty() {
        url.push_str(&format!("&artist_name={}", encode(&q.artist)));
    }
    match get_json(&url) {
        Ok(Value::Array(list)) => {
            reached = true;
            found.extend(list.iter().filter_map(record));
        }
        Ok(_) | Err(Failed::NotThere) => reached = true,
        Err(Failed::Unreachable) => {}
    }
    if found.is_empty() {
        return Err(if reached { Miss::None } else { Miss::Offline });
    }
    Ok(rank(found, q.duration))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn found(duration: f64, synced: &str, plain: &str) -> Found {
        Found {
            title: "Song".into(),
            artist: "Artist".into(),
            album: String::new(),
            duration,
            instrumental: false,
            synced: synced.into(),
            plain: plain.into(),
        }
    }

    #[test]
    fn reads_a_record() {
        let v = json!({
            "id": 3396226, "trackName": "I Want to Live", "artistName": "Borislav Slavov",
            "albumName": "Baldur's Gate 3", "duration": 233.0, "instrumental": false,
            "plainLyrics": "I feel your breath", "syncedLyrics": "[00:17.12] I feel your breath"
        });
        let f = record(&v).unwrap();
        assert_eq!(f.title, "I Want to Live");
        assert_eq!(f.duration, 233.0);
        assert!(f.is_synced());
        assert_eq!(f.text(), "[00:17.12] I feel your breath");
        let v = json!({ "trackName": "Quiet", "instrumental": true, "plainLyrics": null, "syncedLyrics": null });
        let f = record(&v).unwrap();
        assert!(f.instrumental);
        assert_eq!(f.text(), "");
        assert!(record(&json!({ "code": 404 })).is_none());
    }

    #[test]
    fn ranks_by_fit_then_timing() {
        let list = vec![
            found(200.0, "", "plain right length"),
            found(180.0, "[00:01.00]radio edit", ""),
            found(201.0, "[00:01.00]album", ""),
            found(201.5, "[00:01.00]album", ""),
        ];
        let ranked = rank(list, 200.0);
        assert_eq!(ranked.len(), 3, "the same lyrics only once");
        assert_eq!(ranked[0].synced, "[00:01.00]album");
        assert_eq!(ranked[1].plain, "plain right length");
        assert_eq!(ranked[2].synced, "[00:01.00]radio edit");
        assert!(!ranked[2].fits(200.0));
    }

    #[test]
    fn closest_takes_the_best_that_fits() {
        let ranked = rank(
            vec![
                found(200.0, "", "plain right length"),
                found(201.0, "[00:01.00]album", "album words"),
            ],
            200.0,
        );
        assert_eq!(closest(&ranked, 200.0).as_deref(), Some("[00:01.00]album"));
    }

    #[test]
    fn closest_takes_only_the_words_of_another_length() {
        let ranked = rank(
            vec![
                found(180.0, "[00:01.00]radio edit", "radio words"),
                found(240.0, "[00:01.00]extended", "extended words"),
            ],
            200.0,
        );
        assert_eq!(closest(&ranked, 200.0).as_deref(), Some("radio words"));
        let ranked = rank(vec![found(180.0, "[00:01.00]radio edit", "")], 200.0);
        assert_eq!(closest(&ranked, 200.0), None);
    }

    #[test]
    fn closest_skips_instrumentals() {
        let mut quiet = found(200.0, "", "");
        quiet.instrumental = true;
        assert_eq!(closest(&[quiet.clone()], 200.0), None);
        let ranked = rank(vec![quiet, found(200.0, "", "words")], 200.0);
        assert_eq!(closest(&ranked, 200.0).as_deref(), Some("words"));
    }
}
