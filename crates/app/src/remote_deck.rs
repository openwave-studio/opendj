//! The phone remote's Deck page, on the app's side: the deck controls a
//! phone or iPad may set, the decks' state it is sent, and the waveforms it
//! draws.
//!
//! Three kinds of state go out, each as often as it changes:
//!
//! - `decks`: buttons, knobs and faders of all four decks, ten times a second.
//! - `deck_pos`: where each deck is and how fast it moves, twenty times a
//!   second, stamped with the app's clock; the page moves its waveform on its
//!   own clock in between, steered by the stamps rather than by when each
//!   update happened to arrive over Wi-Fi.
//! - `wave_look`: the waveform colour scheme picked in the app, so the page
//!   draws the same colours.
//! - `track1` to `track4`: the track on each deck (title, cover, beats, hot
//!   cue names), only when it changes, with the id of its waveform, which the
//!   page fetches once from `/wave/<id>`. While a track is still being
//!   analysed its waveform grows; it is sent again at most every
//!   [`GROWING_EVERY`], since encoding it is the heaviest thing here.
//!
//! A phone may only set the controls in [`DECK_KEYS`] and the EQ and filter
//! knobs: the things on the Deck page, nothing that edits the collection.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendj_control::{ControlHandle, ControlRegistry};
use opendj_engine::key::Key;
use opendj_engine::{deck_group, eq_group, quick_effect_group, Track, NUM_DECKS, NUM_HOTCUES};
use opendj_remote::Server;
use opendj_waveform::{Style, BAND_PALETTES, RGB_PALETTES};
use serde_json::{json, Value};

use crate::browser::Browser;

const DECKS_EVERY: Duration = Duration::from_millis(100);
const POS_EVERY: Duration = Duration::from_millis(50);
const GROWING_EVERY: Duration = Duration::from_secs(2);

/// The deck controls a phone may set, by their Mixxx names.
const DECK_KEYS: &[&str] = &[
    "play",
    "cue_default",
    "sync_enabled",
    "keylock",
    "quantize",
    "slip_enabled",
    "reverse",
    "rate",
    "rate_temp_up",
    "rate_temp_down",
    "pfl",
    "volume",
    "pregain",
    "loop_in",
    "loop_out",
    "reloop_toggle",
    "beatloop_activate",
    "beatloop_size",
    "loop_halve",
    "loop_double",
    "beatjump_forward",
    "beatjump_backward",
    "beatjump_size",
    "pitch_up",
    "pitch_down",
    "reset_key",
    "sync_key",
];

/// Knobs a phone sets by a short name: the EQ's three bands and the filter.
const KNOBS: [(&str, Knob); 4] = [
    ("eq_low", Knob::Eq(1)),
    ("eq_mid", Knob::Eq(2)),
    ("eq_high", Knob::Eq(3)),
    ("filter", Knob::Filter),
];

#[derive(Clone, Copy)]
enum Knob {
    Eq(usize),
    Filter,
}

/// Every control of one deck the page shows or sets.
struct Deck {
    keys: HashMap<String, ControlHandle>,
    playposition: ControlHandle,
    bpm: ControlHandle,
    rate_range: ControlHandle,
    loop_enabled: ControlHandle,
    loop_start: ControlHandle,
    loop_end: ControlHandle,
    vu: ControlHandle,
    /// The key the deck plays in now (shifted or sped up), as Mixxx's number.
    key: ControlHandle,
    /// Per hot cue: status (0 empty, 1 set, 2 held), position in seconds,
    /// and its type (1 cue, 4 loop).
    hotcues: Vec<(ControlHandle, ControlHandle, ControlHandle)>,
}

pub struct DeckRemote {
    decks: Vec<Deck>,
    last_decks: Instant,
    last_pos: Instant,
    /// The track and waveform length each deck's `track` topic was sent for.
    sent: [Option<(usize, usize)>; NUM_DECKS],
    /// When each deck's track was last sent, to hold back a growing waveform.
    sent_at: [Option<Instant>; NUM_DECKS],
    /// Encoded waveforms of the tracks on the decks, by wave id.
    waves: [Option<(String, Arc<Vec<u8>>)>; NUM_DECKS],
    /// The app's clock for the `deck_pos` stamps.
    start: Instant,
}

impl DeckRemote {
    pub fn new(reg: &ControlRegistry) -> Self {
        let decks = (0..NUM_DECKS)
            .map(|d| {
                let g = deck_group(d);
                let get = |group: &str, key: &str| {
                    reg.get(group, key)
                        .unwrap_or_else(|| panic!("engine control {group},{key}"))
                };
                let mut keys: HashMap<String, ControlHandle> = DECK_KEYS
                    .iter()
                    .map(|k| (k.to_string(), get(&g, k)))
                    .collect();
                for n in 1..=NUM_HOTCUES {
                    for what in ["activate", "clear"] {
                        let k = format!("hotcue_{n}_{what}");
                        keys.insert(k.clone(), get(&g, &k));
                    }
                }
                for (name, knob) in KNOBS {
                    let h = match knob {
                        Knob::Eq(n) => get(&eq_group(d), &format!("parameter{n}")),
                        Knob::Filter => get(&quick_effect_group(d), "super1"),
                    };
                    keys.insert(name.to_string(), h);
                }
                Deck {
                    keys,
                    playposition: get(&g, "playposition"),
                    bpm: get(&g, "bpm"),
                    rate_range: get(&g, "rateRange"),
                    loop_enabled: get(&g, "loop_enabled"),
                    loop_start: get(&g, "loop_start_position"),
                    loop_end: get(&g, "loop_end_position"),
                    vu: get(&g, "vu_meter"),
                    key: get(&g, "key"),
                    hotcues: (1..=NUM_HOTCUES)
                        .map(|n| {
                            (
                                get(&g, &format!("hotcue_{n}_status")),
                                get(&g, &format!("hotcue_{n}_position")),
                                get(&g, &format!("hotcue_{n}_type")),
                            )
                        })
                        .collect(),
                }
            })
            .collect();
        Self {
            decks,
            last_decks: Instant::now(),
            last_pos: Instant::now(),
            sent: Default::default(),
            sent_at: Default::default(),
            waves: Default::default(),
            start: Instant::now(),
        }
    }

    /// Carries out a Deck page request; `None` when `method` isn't one.
    pub fn handle(
        &mut self,
        method: &str,
        params: &Value,
        browser: &mut Browser,
    ) -> Option<Result<Value, String>> {
        let deck = || {
            params["deck"]
                .as_u64()
                .map(|d| d as usize)
                .filter(|&d| d < NUM_DECKS)
                .ok_or_else(|| "No such deck".to_string())
        };
        Some(match method {
            "deck.set" => deck().and_then(|d| {
                let key = params["key"].as_str().unwrap_or("");
                let value = params["value"].as_f64().ok_or("Missing value")?;
                let h = self.decks[d]
                    .keys
                    .get(key)
                    .ok_or_else(|| format!("The remote can't set {key}"))?;
                h.set(value);
                // Show the change on the page straight away.
                self.last_decks = Instant::now() - DECKS_EVERY;
                Ok(Value::Bool(true))
            }),
            "deck.load" => deck().and_then(|d| {
                let id = params["id"].as_u64().ok_or("Missing track")? as usize;
                // A deck that plays to the room asks once more; one that's
                // playing with its fader down is being cued, so it loads.
                let heard = self.decks[d].keys["play"].get_bool()
                    && self.decks[d].keys["volume"].get() > 0.0;
                if heard && !params["force"].as_bool().unwrap_or(false) {
                    return Err("That deck is playing.".into());
                }
                if browser.load_id_onto(d, id) {
                    Ok(Value::Bool(true))
                } else {
                    Err("That track can't be loaded.".into())
                }
            }),
            _ => return None,
        })
    }

    /// Pushes what changed: deck state, positions, and the track on each
    /// deck with its waveform.
    pub fn publish(
        &mut self,
        server: &Server,
        tracks: &[Option<Arc<Track>>; NUM_DECKS],
        camelot: bool,
        art: &dyn Fn(&Track) -> Option<String>,
    ) {
        let now = Instant::now();
        let mut waves_changed = false;
        for (d, track) in tracks.iter().enumerate() {
            let sent = track
                .as_ref()
                .map(|t| (t.id, t.waveform.levels.first().map_or(0, Vec::len)));
            if sent == self.sent[d] {
                continue;
            }
            // A track still being analysed is sent again as its waveform
            // grows, but not every frame.
            let same_track = sent.map(|s| s.0) == self.sent[d].map(|s| s.0);
            if same_track
                && self.sent_at[d].is_some_and(|at| now.duration_since(at) < GROWING_EVERY)
            {
                continue;
            }
            self.sent[d] = sent;
            self.sent_at[d] = Some(now);
            let wave = track.as_deref().and_then(|t| {
                let bytes = encode_wave(t)?;
                Some((wave_id(t, d), Arc::new(bytes)))
            });
            let mut json = track_json(
                track.as_deref(),
                wave.as_ref().map(|w| w.0.as_str()),
                camelot,
            );
            if let Some(t) = track.as_deref() {
                json["art"] = json!(art(t));
            }
            server.publish(&format!("track{}", d + 1), json);
            self.waves[d] = wave;
            waves_changed = true;
        }
        if waves_changed {
            server.set_waves(self.waves.iter().flatten().cloned().collect());
        }
        if now.duration_since(self.last_decks) >= DECKS_EVERY {
            self.last_decks = now;
            server.publish(
                "decks",
                Value::Array(self.decks.iter().map(|d| deck_json(d, camelot)).collect()),
            );
            server.publish("wave_look", wave_look());
        }
        if now.duration_since(self.last_pos) >= POS_EVERY {
            self.last_pos = now;
            let pos: Vec<Value> = self
                .decks
                .iter()
                .zip(tracks)
                .map(|(d, t)| {
                    let Some(t) = t else {
                        return Value::Null;
                    };
                    let k = |key: &str| d.keys[key].get();
                    // How fast the track moves: its tempo now over its own.
                    let mut speed = if t.bpm > 0.0 {
                        d.bpm.get() / t.bpm
                    } else {
                        1.0
                    };
                    if k("reverse") > 0.0 {
                        speed = -speed;
                    }
                    json!([
                        round3(d.playposition.get().clamp(0.0, 1.0) * t.duration),
                        round4(speed),
                        k("play") > 0.0,
                        round3(d.vu.get()),
                    ])
                })
                .collect();
            let at = now.duration_since(self.start).as_secs_f64() * 1000.0;
            server.publish("deck_pos", json!({"at": at.round(), "decks": pos}));
        }
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

/// One deck's buttons, knobs and loop for the page.
/// The waveform colours the app draws with: RGB colours each slice by the
/// balance of its bands, 3-Band layers them. Stems has no stems to show
/// over the network, so it draws as RGB.
fn wave_look() -> Value {
    let hex = |c: [f32; 4]| {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
    };
    let (style, colors) = match crate::wave_look::style() {
        Style::Bands => (
            "bands",
            BAND_PALETTES[crate::wave_look::palette_index().min(BAND_PALETTES.len() - 1)],
        ),
        _ => {
            let p = crate::wave_look::palette();
            let rgb = if matches!(crate::wave_look::style(), Style::Rgb) {
                [p.low, p.mid, p.high]
            } else {
                RGB_PALETTES[0]
            };
            ("rgb", rgb)
        }
    };
    json!({
        "style": style,
        "colors": colors.map(hex),
        "half": crate::wave_look::half(),
    })
}

fn deck_json(d: &Deck, camelot: bool) -> Value {
    let k = |key: &str| d.keys[key].get();
    let key = Key::from_mixxx_number(d.key.get()).map(|k| k.label(camelot));
    json!({
        "key": key,
        "play": k("play") > 0.0,
        "sync": k("sync_enabled") > 0.0,
        "keylock": k("keylock") > 0.0,
        "quantize": k("quantize") > 0.0,
        "slip": k("slip_enabled") > 0.0,
        "reverse": k("reverse") > 0.0,
        "pfl": k("pfl") > 0.0,
        "rate": round4(k("rate")),
        "range": round4(d.rate_range.get()),
        "bpm": (d.bpm.get() * 100.0).round() / 100.0,
        "volume": round3(k("volume")),
        "pregain": round3(k("pregain")),
        "eq": [round3(k("eq_low")), round3(k("eq_mid")), round3(k("eq_high"))],
        "filter": round3(k("filter")),
        "loop": {
            "on": d.loop_enabled.get_bool(),
            "start": round3(d.loop_start.get()),
            "end": round3(d.loop_end.get()),
            "size": k("beatloop_size"),
        },
        "jump": k("beatjump_size"),
        "hotcues": d.hotcues.iter().map(|(status, pos, kind)| {
            if status.get() <= 0.0 {
                Value::Null
            } else {
                json!({"at": round3(pos.get()), "held": status.get() >= 2.0, "loop": kind.get() == 4.0})
            }
        }).collect::<Vec<_>>(),
    })
}

/// The track on a deck for the page: what it is, its beats (and which one
/// starts the first bar), and its hot cues' names and colours.
fn track_json(t: Option<&Track>, wave: Option<&str>, camelot: bool) -> Value {
    let Some(t) = t else {
        return Value::Null;
    };
    let mut beats = Vec::new();
    let mut downbeat = 0;
    if !t.grid.is_empty() && t.duration > 0.0 {
        let first = t.grid.beat_at(0.0).ceil();
        let last = t.grid.beat_at(t.duration).floor();
        let mut b = first;
        while b <= last && beats.len() < 20_000 {
            if beats.is_empty() {
                downbeat = (4 - (b as i64).rem_euclid(4)) % 4;
            }
            beats.push(round3(t.grid.seconds_at(b)));
            b += 1.0;
        }
    }
    let cues: Vec<Value> = t
        .info
        .hotcues
        .iter()
        .filter(|c| c.number < NUM_HOTCUES)
        .map(|c| json!({"n": c.number, "name": c.name, "color": c.color.map(|c| format!("#{c:06x}"))}))
        .collect();
    json!({
        "id": t.id,
        "title": t.title,
        "artist": t.artist,
        "key": crate::settings::show_key(&t.key, camelot),
        "bpm": t.bpm,
        "duration": round3(t.duration),
        "wave": wave,
        "beats": beats,
        "downbeat": downbeat,
        "cues": cues,
    })
}

/// A waveform id the page asks `/wave/` for: hex, new for each track and
/// for each pass of its analysis.
fn wave_id(t: &Track, deck: usize) -> String {
    let len = t.waveform.levels.first().map_or(0, Vec::len);
    format!("{:x}{:x}{:x}", deck + 1, t.id, len)
}

/// The track's waveform as the page reads it: `ODW2`, slices a second
/// (f32), the slice count (u32), four spare bytes, then each slice as
/// analysed at full resolution: the peaks of the low, mid and high bands and
/// of the whole signal, then their RMS (each 0 to 255, the square root of
/// the level), so the page can draw it as the app does.
pub fn encode_wave(t: &Track) -> Option<Vec<u8>> {
    let w = &t.waveform;
    let bins = w.levels.first().filter(|l| !l.is_empty())?;
    let mut out = Vec::with_capacity(16 + bins.len() * 8);
    out.extend_from_slice(b"ODW2");
    out.extend_from_slice(&(w.bins_per_second as f32).to_le_bytes());
    out.extend_from_slice(&(bins.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    for b in bins {
        out.extend_from_slice(b);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendj_engine::{Source, Waveform};

    fn blank() -> Track {
        Track {
            id: 0,
            artist: String::new(),
            title: String::new(),
            album: String::new(),
            genre: String::new(),
            rating: 0,
            bpm: 0.0,
            key: String::new(),
            duration: 0.0,
            first_beat: 0.0,
            grid: Default::default(),
            source: Source::Synth { seed: 0 },
            waveform: Arc::new(Waveform::default()),
            audio: None,
            cover: None,
            info: Default::default(),
        }
    }

    #[test]
    fn waveforms_encode_low_mid_high_peaks() {
        let t = Track {
            waveform: Arc::new(Waveform::from_finest(
                1000.0,
                vec![[10, 20, 30, 40, 1, 2, 3, 4], [50, 60, 70, 80, 5, 6, 7, 8]],
            )),
            ..blank()
        };
        let bytes = encode_wave(&t).unwrap();
        assert_eq!(&bytes[..4], b"ODW2");
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1000.0);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
        assert_eq!(&bytes[16..24], &[10, 20, 30, 40, 1, 2, 3, 4]);
        assert_eq!(bytes.len(), 16 + 2 * 8);
        assert!(encode_wave(&blank()).is_none());
    }

    #[test]
    fn tracks_list_beats_from_the_first_bar() {
        let t = Track {
            duration: 10.0,
            bpm: 120.0,
            grid: opendj_engine::BeatGrid::constant(120.0, 0.25),
            ..blank()
        };
        let v = track_json(Some(&t), Some("1a"), true);
        let beats = v["beats"].as_array().unwrap();
        assert_eq!(beats[0], json!(0.25));
        assert_eq!(beats[1], json!(0.75));
        assert_eq!(beats.len(), 20);
        assert_eq!(v["downbeat"], json!(0));
        assert_eq!(v["wave"], json!("1a"));
        assert_eq!(track_json(None, None, true), Value::Null);
    }
}
