//! The Jam source (formerly Instruments) in the library rail (`ui/instruments.slint`):
//! one column (a Presets / Instruments switch, search, a star, filter menus
//! of chips and the rows they leave) and the rows again as a wide list,
//! dragged onto a deck to play through its channel. Dropping is handled on
//! the screen (`InstrumentsUi.drop`), which sets the deck's controls; the
//! engine does the rest (`[ChannelN],live_input`, `[GrooveN],enabled`).

use std::cell::RefCell;
use std::collections::HashSet;

use slint::{ComponentHandle, DataTransfer, ModelRc, SharedString, VecModel};

use opendj_engine::{inputs, remix};

use crate::audio_plugins::Found;
use crate::presets::{self, Preset, GROOVE};
use crate::tree_navigation::Direction;
use crate::{
    settings, AppWindow, InstrumentsUi, JamChip, JamFacet, JamLine, LiveItem, MixerStripInfo,
    Preferences, RemixUi,
};
use crate::{BrowserFocus, BrowserPane};

/// What dragging an item carries: `opendj-live:<kind>:<index>`.
const LIVE_PAYLOAD: &str = "opendj-live:";

/// Kinds of item, as `LiveItem.kind`.
const KIT: i32 = 1;
const PLUGIN: i32 = 2;
const MIC: i32 = 3;
// 4 was an aux input on its own, before the Mixer.
/// A preset, by its place in `crate::presets::all`.
const PRESET: i32 = 5;
/// The Looper (`[LooperN],enabled`).
const LOOPER: i32 = 6;
/// The Mixer: the aux inputs as strips, summed into one deck.
const MIXER: i32 = 7;
/// The Remix deck (`[RemixN],enabled`): another deck played through
/// band effects and scenes.
const REMIX: i32 = 8;
const LAST_KIND: i32 = 8;

fn payload(kind: i32, index: i32) -> String {
    format!("{LIVE_PAYLOAD}{kind}:{index}")
}

/// The item a drag's text names, as `kind * 1000 + index`.
fn parse(text: &str) -> Option<i32> {
    let (kind, index) = text.strip_prefix(LIVE_PAYLOAD)?.split_once(':')?;
    let (kind, index): (i32, i32) = (kind.parse().ok()?, index.parse().ok()?);
    ((1..=LAST_KIND).contains(&kind) && (0..1000).contains(&index)).then_some(kind * 1000 + index)
}

/// What the tree and the list were built from, kept to rebuild them when a
/// node is opened or selected, the search changes, or plugins and inputs do.
#[derive(Default)]
struct State {
    /// The ticked instrument plugins: picker name, maker, format, kind.
    plugins: Vec<Plugin>,
    /// Each input's device and channels, by the engine's input index.
    routes: Vec<String>,
    /// The side the column shows: [`PRESETS`] or [`INSTRUMENTS`].
    view: i32,
    search: String,
    /// The presets, as `crate::presets::all` lists them.
    presets: Vec<Preset>,
    /// The values picked in each of the side's filter menus.
    picks: Vec<HashSet<String>>,
    /// The filter menu open, if any: one at a time.
    open_facet: Option<usize>,
    /// Starred items, by [`recent_key`], kept in settings.
    starred: HashSet<String>,
    /// Only starred items show.
    starred_only: bool,
    /// What went onto a deck lately, newest first (see [`recent_key`]).
    recent: Vec<String>,
}

/// How many items Recently played keeps.
const RECENT: usize = 20;

/// What an item is, kept across restarts for Recently played: `groove`,
/// `plugin:<key>`, `preset:<path>` or `input:<engine input>`.
fn recent_key(s: &State, kind: i32, index: i32) -> Option<String> {
    let i = usize::try_from(index).ok()?;
    match kind {
        KIT => Some("groove".into()),
        LOOPER => Some("looper".into()),
        REMIX => Some("remix".into()),
        PLUGIN => s.plugins.get(i).map(|p| format!("plugin:{}", p.key)),
        PRESET => s
            .presets
            .get(i)
            .map(|p| format!("preset:{}", p.path.display())),
        MIC | MIXER => INPUTS
            .iter()
            .find(|(k, x, _, _)| *k == kind && *x == index)
            .map(|(_, _, input, _)| format!("input:{input}")),
        _ => None,
    }
}

/// Puts `key` first in Recently played.
fn remember(recent: &mut Vec<String>, key: String) {
    recent.retain(|k| *k != key);
    recent.insert(0, key);
    recent.truncate(RECENT);
}

struct Plugin {
    key: String,
    name: String,
    maker: String,
    format: &'static str,
    kind: &'static str,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        starred: crate::settings::load()[STARRED]
            .as_array()
            .map(|a| a.iter().filter_map(|k| k.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        recent: crate::settings::load()["instruments_recent"]
            .as_array()
            .map(|a| a.iter().filter_map(|k| k.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        ..State::default()
    });
}

/// Where the Mixer's line sits in `State::routes`: one past the engine's
/// inputs, so a deck's `live_input` less one finds it as it finds an input's.
const MIXER_ROUTE: usize = inputs::MIXER - 1;

/// The microphone and the Mixer, with where their lines sit in
/// `State::routes` (the engine's input index for the microphone) and their
/// names. The aux inputs play as the Mixer's strips, not one by one.
const INPUTS: [(i32, i32, usize, &str); 2] =
    [(MIC, 0, 0, "Microphone"), (MIXER, 0, MIXER_ROUTE, "Mixer")];

/// ← in the column goes back to the rail.
/// ↑ and ↓ move through its rows (see `browser::navigation`).
pub(crate) fn navigate_column(app: &AppWindow, code: i32) {
    if let Some(Direction::Left) = Direction::from_code(code) {
        app.global::<BrowserFocus>()
            .invoke_focus(BrowserPane::Sources);
    }
}

/// Changes the state, then shows it.
fn update(app: &slint::Weak<AppWindow>, change: impl FnOnce(&mut State)) {
    STATE.with_borrow_mut(change);
    if let Some(app) = app.upgrade() {
        app.global::<InstrumentsUi>().set_current_item(-1);
        refresh(&app);
    }
}

pub fn install(app: &AppWindow) {
    // The Remix deck's effect and scene names, by its controls' values.
    let names = |n: &[&str]| -> ModelRc<SharedString> {
        ModelRc::new(VecModel::from(
            n.iter().map(|&n| SharedString::from(n)).collect::<Vec<_>>(),
        ))
    };
    let remix_ui = app.global::<RemixUi>();
    remix_ui.set_isolate_effects(names(&remix::ISOLATE_EFFECTS));
    remix_ui.set_lever_effects(names(&remix::LEVER_EFFECTS));
    remix_ui.set_scenes_up(names(&remix::SCENES_UP));
    remix_ui.set_scenes_down(names(&remix::SCENES_DOWN));
    remix_ui.set_pad_names(names(&remix::PAD_NAMES));
    remix_ui.set_roll_rates(names(&remix::ROLL_RATES));
    let kits: Vec<&str> = remix::KITS.iter().map(|k| k.name).collect();
    remix_ui.set_kits(names(&kits));
    let ui = app.global::<InstrumentsUi>();
    ui.set_audio_page(crate::prefs::AUDIO_PAGE);
    ui.set_plugins_page(crate::prefs::PLUGINS_PAGE);
    ui.on_payload(|kind, index| DataTransfer::from(SharedString::from(payload(kind, index))));
    ui.on_item(|data| data.plain_text().ok().and_then(|t| parse(&t)).unwrap_or(-1));
    let weak = app.as_weak();
    ui.on_open_settings(move |page, tab| {
        if let Some(app) = weak.upgrade() {
            app.global::<Preferences>().invoke_show_tab(page, tab);
        }
    });
    let weak = app.as_weak();
    ui.on_played(move |item| {
        STATE.with_borrow_mut(|s| {
            if let Some(key) = recent_key(s, item / 1000, item % 1000) {
                remember(&mut s.recent, key);
                crate::settings::set("instruments_recent", serde_json::json!(s.recent));
            }
        });
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    ui.on_search(move |text| update(&weak, |s| s.search = text.to_string()));
    let weak = app.as_weak();
    ui.on_set_view(move |view| {
        update(&weak, |s| {
            if s.view != view {
                s.view = view;
                s.picks.clear();
                s.open_facet = None;
            }
        })
    });
    let weak = app.as_weak();
    ui.on_toggle_starred_only(move || update(&weak, |s| s.starred_only = !s.starred_only));
    let weak = app.as_weak();
    ui.on_toggle_facet(move |facet| {
        update(&weak, |s| {
            let f = facet as usize;
            s.open_facet = (s.open_facet != Some(f)).then_some(f);
        })
    });
    let weak = app.as_weak();
    ui.on_toggle_value(move |facet, value| {
        update(&weak, |s| {
            s.picks.resize(facet_names(s.view).len(), HashSet::new());
            if let Some(p) = s.picks.get_mut(facet as usize) {
                if !p.remove(value.as_str()) {
                    p.insert(value.to_string());
                }
            }
        })
    });
    let weak = app.as_weak();
    ui.on_clear_filters(move || update(&weak, |s| s.picks.clear()));
    let weak = app.as_weak();
    ui.on_toggle_star(move |kind, index| {
        update(&weak, |s| {
            if let Some(key) = recent_key(s, kind, index) {
                if !s.starred.remove(&key) {
                    s.starred.insert(key);
                }
                let mut keys: Vec<&String> = s.starred.iter().collect();
                keys.sort();
                crate::settings::set(STARRED, serde_json::json!(keys));
            }
        })
    });
    // The instrument deck's keyboard size, kept.
    ui.set_keyboard_size(
        crate::settings::load()[KEYBOARD_SIZE]
            .as_i64()
            .unwrap_or(0)
            .clamp(0, 2) as i32,
    );
    ui.on_keyboard_size_changed(|size| {
        crate::settings::set(KEYBOARD_SIZE, serde_json::json!(size));
    });
    let weak = app.as_weak();
    ui.on_set_color(move |instrument, color| {
        let key = STATE.with_borrow(|s| {
            s.plugins
                .get((instrument - 1).max(0) as usize)
                .map(|p| p.key.clone())
        });
        if let Some(key) = key {
            let mut picked = crate::settings::load()[COLORS].clone();
            if !picked.is_object() {
                picked = serde_json::json!({});
            }
            if color < 0 {
                picked.as_object_mut().map(|m| m.remove(&key));
            } else {
                picked[&key] = serde_json::json!(color);
            }
            crate::settings::set(COLORS, picked);
        }
        if let Some(app) = weak.upgrade() {
            show_colors(&app);
        }
    });
    refresh(app);
}

/// A pitch bend message for `value` -1 (down) to 1 (up), 0 the middle.
pub fn bend_message(value: f32) -> [u8; 3] {
    let v = ((value.clamp(-1.0, 1.0) + 1.0) * 8191.5).round() as u16;
    let v = v.min(16383);
    [0xe0, (v & 0x7f) as u8, (v >> 7) as u8]
}

/// A mod wheel (CC 1) message for `value` 0 to 1.
pub fn modwheel_message(value: f32) -> [u8; 3] {
    [0xb0, 1, (value.clamp(0.0, 1.0) * 127.0).round() as u8]
}

/// Settings: the instrument deck's keyboard size, and the colors picked
/// for instruments, by plugin key.
const KEYBOARD_SIZE: &str = "instrument_keyboard";
/// Settings: the starred presets and instruments, by [`recent_key`].
const STARRED: &str = "jam_starred";
const COLORS: &str = "instrument_colors";

/// How many colors the Mixer strips' palette has; the last is grey, which
/// an instrument only gets when picked.
const PALETTE: u32 = 16;

/// An instrument's own color, from its key: the same every run, spread
/// over the palette but its grey.
fn own_color(key: &str) -> i32 {
    // FNV-1a, so the pick doesn't change between Rust versions.
    let hash = key.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    });
    (hash % (PALETTE - 1)) as i32
}

/// Each ticked plugin's color: the DJ's pick, else its own.
fn show_colors(app: &AppWindow) {
    let picked = crate::settings::load()[COLORS].clone();
    let colors: Vec<i32> = STATE.with_borrow(|s| {
        s.plugins
            .iter()
            .map(|p| {
                picked[&p.key].as_i64().map_or_else(
                    || own_color(&p.key),
                    |c| c.clamp(0, PALETTE as i64 - 1) as i32,
                )
            })
            .collect()
    });
    app.global::<InstrumentsUi>()
        .set_plugin_colors(ModelRc::new(VecModel::from(colors)));
}

/// Lists the ticked instrument plugins, in the Instrument input's order,
/// under the names its picker shows.
pub fn show_plugins(app: &AppWindow, instruments: &[Found], names: &[String]) {
    STATE.with_borrow_mut(|s| {
        s.plugins = instruments
            .iter()
            .zip(names)
            .map(|(f, name)| Plugin {
                key: f.key.clone(),
                name: name.clone(),
                maker: f.vendor.clone(),
                format: f.format,
                kind: plugin_kind(&f.features),
            })
            .collect();
    });
    app.global::<InstrumentsUi>()
        .set_plugin_names(ModelRc::new(VecModel::from(
            names
                .iter()
                .map(|n| SharedString::from(n.as_str()))
                .collect::<Vec<_>>(),
        )));
    app.global::<InstrumentsUi>()
        .set_plugin_formats(ModelRc::new(VecModel::from(
            instruments
                .iter()
                .map(|f| SharedString::from(f.format))
                .collect::<Vec<_>>(),
        )));
    show_colors(app);
    crate::instrument_art::show_plugins(
        app,
        instruments
            .iter()
            .map(|f| crate::instrument_art::Plugin {
                key: f.key.clone(),
                name: f.name.clone(),
                maker: f.vendor.clone(),
            })
            .collect(),
    );
    presets::set_plugins(
        app,
        instruments
            .iter()
            .zip(names)
            .map(|(f, name)| presets::Instrument {
                key: f.key.clone(),
                name: name.clone(),
                plugin_name: f.name.clone(),
                maker: f.vendor.clone(),
            })
            .collect(),
    );
    refresh(app);
}

/// Lists the presets again, after one was saved or renamed.
pub fn presets_changed(app: &AppWindow) {
    refresh(app);
}

/// What an instrument plugin says it is, in a word.
fn plugin_kind(features: &[String]) -> &'static str {
    let has = |word: &str| features.iter().any(|f| f.to_lowercase().contains(word));
    if has("drum") {
        "Drums"
    } else if has("sampler") {
        "Sampler"
    } else if has("synth") {
        "Synth"
    } else {
        "Instrument"
    }
}

fn maker(p: &Plugin) -> &str {
    if p.maker.is_empty() {
        "Other"
    } else {
        &p.maker
    }
}

/// The inputs listed: the microphone and the Mixer.
fn live_inputs() -> impl Iterator<Item = &'static (i32, i32, usize, &'static str)> {
    INPUTS.iter()
}

/// The column's two sides: every preset, or one row per instrument.
const PRESETS: i32 = 0;
const INSTRUMENTS: i32 = 1;

/// Each side's filter menus, in order. Genre and Style join Presets once
/// presets carry them (`redesign-jam-browser` step 2).
const PRESET_FACETS: [&str; 4] = ["Sound type", "Character", "Instrument", "Maker"];
const INSTRUMENT_FACETS: [&str; 2] = ["Kind", "Maker"];

fn facet_names(view: i32) -> &'static [&'static str] {
    if view == INSTRUMENTS {
        &INSTRUMENT_FACETS
    } else {
        &PRESET_FACETS
    }
}

/// A row's colour square, as `LiveItem.tint`, and its Kind: what plays.
const TINT_SYNTH: i32 = 0;
const TINT_DRUMS: i32 = 1;
const TINT_KEYS: i32 = 2;
const TINT_TOOLS: i32 = 3;
const TINT_PD: i32 = 4;
const TINT_INPUTS: i32 = 5;
const KINDS: [&str; 6] = [
    "Synths",
    "Drums",
    "Keys and samplers",
    "Tools",
    "Pd patches",
    "Inputs",
];

fn plugin_tint(p: &Plugin) -> i32 {
    if p.format == "Pd" {
        TINT_PD
    } else {
        match p.kind {
            "Drums" => TINT_DRUMS,
            "Sampler" => TINT_KEYS,
            _ => TINT_SYNTH,
        }
    }
}

/// A row, and its values in each of its side's filter menus.
struct Entry {
    item: LiveItem,
    facets: Vec<Vec<String>>,
}

/// Everything the side shown can list, before search and filters.
fn entries(s: &State) -> Vec<Entry> {
    let built_in = |kind: i32, name: &str, what: &str, tint: i32| Entry {
        item: LiveItem {
            kind,
            index: 0,
            name: name.into(),
            r#type: what.into(),
            maker: "openDJ".into(),
            format: "Built in".into(),
            tint,
            ..LiveItem::default()
        },
        facets: vec![vec![KINDS[tint as usize].into()], vec!["openDJ".into()]],
    };
    let plugin_by_key = |key: &str| s.plugins.iter().find(|p| p.key == key);
    let mut out: Vec<Entry> = if s.view == INSTRUMENTS {
        let mut out = vec![
            built_in(KIT, "Groove Box", "Drum machine", TINT_DRUMS),
            built_in(LOOPER, "Looper", "Looper", TINT_TOOLS),
            built_in(REMIX, "Remix", "Remix effects", TINT_TOOLS),
        ];
        out.extend(s.plugins.iter().enumerate().map(|(i, p)| Entry {
            item: LiveItem {
                kind: PLUGIN,
                index: i as i32,
                name: p.name.as_str().into(),
                r#type: p.kind.into(),
                maker: p.maker.as_str().into(),
                format: p.format.into(),
                tint: plugin_tint(p),
                ..LiveItem::default()
            },
            facets: vec![
                vec![KINDS[plugin_tint(p) as usize].into()],
                vec![maker(p).into()],
            ],
        }));
        out.extend(live_inputs().map(|&(kind, index, input, name)| {
            let route = s.routes.get(input).cloned().unwrap_or_default();
            Entry {
                item: LiveItem {
                    kind,
                    index,
                    name: name.into(),
                    r#type: if kind == MIC { "Microphone" } else { "Mixer" }.into(),
                    problem: match (route.is_empty(), kind) {
                        (false, _) => "",
                        (true, MIC) => "No input picked",
                        (true, _) => "No inputs picked",
                    }
                    .into(),
                    maker: route.into(),
                    tint: TINT_INPUTS,
                    ..LiveItem::default()
                },
                facets: vec![vec![KINDS[TINT_INPUTS as usize].into()], Vec::new()],
            }
        }));
        out
    } else {
        s.presets
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let plugin = plugin_by_key(&p.instrument);
                let (maker_name, tint) = if p.instrument == GROOVE {
                    ("openDJ", TINT_DRUMS)
                } else {
                    plugin.map_or(("", TINT_SYNTH), |pl| (maker(pl), plugin_tint(pl)))
                };
                let tagged = |set: &[&str]| -> Vec<String> {
                    p.tags
                        .iter()
                        .filter(|t| set.contains(&t.as_str()))
                        .cloned()
                        .collect()
                };
                Entry {
                    item: LiveItem {
                        kind: PRESET,
                        index: i as i32,
                        name: p.name.as_str().into(),
                        r#type: if p.from_plugin {
                            "Plugin preset"
                        } else {
                            "Preset"
                        }
                        .into(),
                        maker: p.instrument_name.as_str().into(),
                        tint,
                        ..LiveItem::default()
                    },
                    facets: vec![
                        tagged(&presets::TYPES),
                        tagged(&presets::CHARACTERS),
                        vec![p.instrument_name.clone()],
                        if maker_name.is_empty() {
                            Vec::new()
                        } else {
                            vec![maker_name.to_string()]
                        },
                    ],
                }
            })
            .collect()
    };
    for e in &mut out {
        e.item.starred =
            recent_key(s, e.item.kind, e.item.index).is_some_and(|k| s.starred.contains(&k));
    }
    out
}

/// The order a menu's values come in: the tag lists' own for tags and
/// kinds, name order for the rest.
fn value_order(view: i32, facet: usize) -> &'static [&'static str] {
    match (view, facet) {
        (PRESETS, 0) => &presets::TYPES,
        (PRESETS, 1) => &presets::CHARACTERS,
        (INSTRUMENTS, 0) => &KINDS,
        _ => &[],
    }
}

/// One menu: its values with how many rows have each, given the search,
/// the star and every other menu's picks, and whether each is picked.
struct Facet {
    name: &'static str,
    values: Vec<(String, usize, bool)>,
}

/// The rows to list and the menus over them. Picks in one menu are any-of;
/// menus narrow each other.
fn filtered(s: &State) -> (Vec<LiveItem>, Vec<Facet>) {
    let all = entries(s);
    let names = facet_names(s.view);
    let pick = |f: usize| s.picks.get(f).filter(|p| !p.is_empty());
    let words: Vec<String> = s.search.split_whitespace().map(str::to_lowercase).collect();
    let base = |e: &Entry| {
        if s.starred_only && !e.item.starred {
            return false;
        }
        let text = format!(
            "{} {} {} {}",
            e.item.name, e.item.r#type, e.item.maker, e.item.format
        )
        .to_lowercase();
        words.iter().all(|w| text.contains(w.as_str()))
    };
    let fits = |e: &Entry, skip: Option<usize>| {
        (0..names.len()).all(|f| {
            Some(f) == skip || pick(f).is_none_or(|p| e.facets[f].iter().any(|v| p.contains(v)))
        })
    };
    let mut rows: Vec<LiveItem> = all
        .iter()
        .filter(|e| base(e) && fits(e, None))
        .map(|e| e.item.clone())
        .collect();
    if s.view == PRESETS {
        rows.sort_by_cached_key(|r| (r.name.to_lowercase(), r.maker.to_lowercase()));
    }
    let facets = names
        .iter()
        .enumerate()
        .map(|(f, &name)| {
            let mut counts: Vec<(String, usize)> = Vec::new();
            for e in all.iter().filter(|e| base(e) && fits(e, Some(f))) {
                let mut seen: Vec<&String> = Vec::new();
                for v in &e.facets[f] {
                    if seen.contains(&v) {
                        continue;
                    }
                    seen.push(v);
                    match counts.iter_mut().find(|(x, _)| x == v) {
                        Some((_, n)) => *n += 1,
                        None => counts.push((v.clone(), 1)),
                    }
                }
            }
            // A pick that matches nothing now stays, to be unpicked.
            for p in s.picks.get(f).into_iter().flatten() {
                if !counts.iter().any(|(x, _)| x == p) {
                    counts.push((p.clone(), 0));
                }
            }
            let order = value_order(s.view, f);
            counts.sort_by_cached_key(|(v, _)| {
                (
                    order.iter().position(|o| o == v).unwrap_or(order.len()),
                    v.to_lowercase(),
                )
            });
            let picked = |v: &String| s.picks.get(f).is_some_and(|p| p.contains(v));
            Facet {
                name,
                values: counts
                    .into_iter()
                    .map(|(v, n)| {
                        let on = picked(&v);
                        (v, n, on)
                    })
                    .collect(),
            }
        })
        .collect();
    (rows, facets)
}

/// How wide the column's chips may run before the next line: the column
/// less its padding.
const CHIP_LINE: f32 = 232.0;

/// A chip's width as the column draws it: 11 px text, 8 px each side, 4 px
/// apart. An estimate, so a line breaks a little early rather than late.
fn chip_width(text: &str) -> f32 {
    text.chars().count() as f32 * 6.2 + 16.0 + 4.0
}

/// Breaks a menu's chips into lines that fit the column.
fn chip_lines(values: &[(String, usize, bool)]) -> Vec<JamLine> {
    let mut lines: Vec<Vec<JamChip>> = Vec::new();
    let mut width = CHIP_LINE;
    for (name, count, on) in values {
        let w = chip_width(name);
        if width + w > CHIP_LINE {
            lines.push(Vec::new());
            width = 0.0;
        }
        width += w;
        if let Some(line) = lines.last_mut() {
            line.push(JamChip {
                name: name.as_str().into(),
                count: *count as i32,
                on: *on,
            });
        }
    }
    lines
        .into_iter()
        .map(|chips| JamLine {
            chips: ModelRc::new(VecModel::from(chips)),
        })
        .collect()
}

/// Shows the column and the list again from the state.
fn refresh(app: &AppWindow) {
    let ui = app.global::<InstrumentsUi>();
    let found = presets::all();
    STATE.with_borrow_mut(|s| {
        s.presets = found;
        s.picks.resize(facet_names(s.view).len(), HashSet::new());
        let (rows, facets) = filtered(s);
        let picked = s.picks.iter().any(|p| !p.is_empty());
        let (empty, page, tab) = if !rows.is_empty() {
            ("", -1, 0)
        } else if picked {
            ("Nothing has every pick. Clear some filters.", -1, 0)
        } else if !s.search.is_empty() {
            ("Nothing matches the search.", -1, 0)
        } else if s.starred_only {
            ("Nothing starred yet. Click ☆ on a row.", -1, 0)
        } else if s.view == PRESETS {
            (
                "No presets yet. Save one from an instrument deck, or tick instruments in Settings › Plugins…",
                7,
                1,
            )
        } else {
            ("", -1, 0)
        };
        let count = rows.len();
        let facets: Vec<JamFacet> = facets
            .into_iter()
            .enumerate()
            .map(|(f, facet)| JamFacet {
                name: facet.name.into(),
                open: s.open_facet == Some(f),
                picked: facet.values.iter().filter(|(_, _, on)| *on).count() as i32,
                picks: ModelRc::new(VecModel::from(
                    facet
                        .values
                        .iter()
                        .filter(|(_, _, on)| *on)
                        .map(|(name, count, on)| JamChip {
                            name: name.as_str().into(),
                            count: *count as i32,
                            on: *on,
                        })
                        .collect::<Vec<_>>(),
                )),
                lines: ModelRc::new(VecModel::from(chip_lines(&facet.values))),
            })
            .collect();
        ui.set_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_facets(ModelRc::new(VecModel::from(facets)));
        ui.set_view(s.view);
        ui.set_starred_only(s.starred_only);
        ui.set_count_text(
            match (s.view, count) {
                (PRESETS, 1) => "1 preset".to_string(),
                (PRESETS, n) => format!("{n} presets"),
                (_, 1) => "1 instrument".to_string(),
                (_, n) => format!("{n} instruments"),
            }
            .into(),
        );
        ui.set_filtered(picked);
        ui.set_empty_text(empty.into());
        ui.set_empty_page(page);
        ui.set_empty_tab(tab);
    });
}

/// Shows on the inputs' tiles and live decks where each input comes from,
/// from the engine's input states ("M4 · In 3 · 44.1 kHz"): the device and
/// channels only, or nothing for an input that isn't recording. The
/// Mixer's line, after the inputs', sums up its strips (see [`mixer_route`]).
///
/// `clock` is the engine's status (the main output's device first): a
/// strip on another interface runs on its own clock.
pub fn show_routes(app: &AppWindow, status: &[String], clock: &str) {
    let mut routes: Vec<String> = status.iter().map(|s| route(s)).collect();
    let mixer = mixer_route(&routes);
    routes.resize(MIXER_ROUTE, String::new());
    routes.push(mixer);
    let shown: Vec<SharedString> = routes.iter().map(|r| r.as_str().into()).collect();
    let ui = app.global::<InstrumentsUi>();
    ui.set_routes(ModelRc::new(VecModel::from(shown)));
    ui.set_strips(ModelRc::new(VecModel::from(strips(
        status,
        &settings::mixer_names(),
        &settings::mixer_colors(),
        clock_device(clock),
    ))));
    ui.set_mixer_groove(
        inputs::AUXILIARIES
            .iter()
            .any(|&i| status.get(i).is_some_and(|s| s == inputs::GROOVE_BOX)),
    );
    STATE.with_borrow_mut(|s| s.routes = routes);
    refresh(app);
}

/// The Mixer's channels from the engine's input states: each aux input
/// that has a device, working or not, under the name given it ("Ch N"
/// until renamed), marked when its interface isn't `clock`'s (the main
/// output's device, "" when there is none).
fn strips(status: &[String], names: &[String], colors: &[i32], clock: &str) -> Vec<MixerStripInfo> {
    inputs::AUXILIARIES
        .iter()
        .enumerate()
        .filter_map(|(n, &i)| {
            let status = status.get(i).filter(|s| !s.is_empty())?;
            let route = route(status);
            let mut parts = route.split(" · ");
            let device = parts.next().unwrap_or_default();
            let channels = parts.next().map(channels).unwrap_or_default();
            // The Mixer deck's own Groove Box: no interface, no clock.
            let groove = device == inputs::GROOVE_BOX;
            Some(MixerStripInfo {
                number: n as i32 + 1,
                name: names
                    .get(n)
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map_or_else(
                        || {
                            if groove {
                                "Groove".to_string()
                            } else {
                                format!("Ch {}", n + 1)
                            }
                        },
                        String::from,
                    )
                    .into(),
                channels: channels.as_str().into(),
                stereo: channels.contains('/'),
                own_clock: !groove && !route.is_empty() && !clock.is_empty() && device != clock,
                group: format!("[Auxiliary{}]", n + 1).into(),
                badge: (n + 1).to_string().into(),
                on_key: "main_mix".into(),
                aux: true,
                color: colors.get(n).copied().unwrap_or(-1),
                view: if groove { 2 } else { 0 },
                problem: if route.is_empty() {
                    status.as_str()
                } else {
                    ""
                }
                .into(),
            })
        })
        .collect()
}

/// A strip's channels as its input box shows them: "In 3" is "3", "In 1 &
/// 2" is "1/2".
fn channels(label: &str) -> String {
    label.trim_start_matches("In ").replace(" & ", "/")
}

/// The Mixer's line from the inputs' routes: the interfaces its strips
/// come from and how many there are ("M4 · 3 inputs"), or "" with none.
fn mixer_route(routes: &[String]) -> String {
    let mut devices: Vec<&str> = Vec::new();
    let mut count = 0;
    for route in inputs::AUXILIARIES.iter().filter_map(|&i| routes.get(i)) {
        if let Some(device) = route.split(" · ").next().filter(|_| !route.is_empty()) {
            count += 1;
            if !devices.contains(&device) {
                devices.push(device);
            }
        }
    }
    match count {
        0 => String::new(),
        1 => format!("{} · 1 input", devices[0]),
        n => format!("{} · {n} inputs", devices.join(", ")),
    }
}

/// The main output's device from the engine's status ("M4 · 48 kHz · …"),
/// or "" when it isn't playing ("No audio output", "M4 went away").
pub fn clock_device(status: &str) -> &str {
    match status.split_once(" · ") {
        Some((device, rest)) if rest.contains(" kHz") => device,
        _ => "",
    }
}

/// "M4 · In 3" from an open input's state; "" from anything else (not set
/// up, or an error).
fn route(status: &str) -> String {
    if status == inputs::GROOVE_BOX {
        return status.to_string();
    }
    let parts: Vec<&str> = status.split(" · ").collect();
    match parts.iter().position(|p| p.ends_with(" kHz")) {
        Some(k) if k > 0 => parts[..k].join(" · "),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn bend_and_mod_wheel_are_midi() {
        assert_eq!(bend_message(0.0), [0xe0, 0x00, 0x40]);
        assert_eq!(bend_message(-1.0), [0xe0, 0, 0]);
        assert_eq!(bend_message(1.0), [0xe0, 0x7f, 0x7f]);
        assert_eq!(modwheel_message(1.0), [0xb0, 1, 127]);
        assert_eq!(modwheel_message(0.0), [0xb0, 1, 0]);
    }

    #[test]
    fn an_instrument_keeps_its_own_color_and_never_the_grey() {
        let key = "clap:/x/Acid V.clap#com.arturia.acid";
        assert_eq!(own_color(key), own_color(key));
        for k in ["a", "b", "au:aumu", "vst3:/y.vst3#1", key] {
            assert!((0..PALETTE as i32 - 1).contains(&own_color(k)));
        }
    }

    #[test]
    fn payloads_read_back_as_their_items() {
        assert_eq!(parse(&payload(1, 2)), Some(1002));
        assert_eq!(parse(&payload(2, 0)), Some(2000));
        assert_eq!(parse(&payload(MIXER, 0)), Some(7000));
        assert_eq!(parse(&payload(REMIX, 0)), Some(8000));
        assert_eq!(parse("opendj-row:3"), None);
        assert_eq!(parse("opendj-live:9:0"), None);
        assert_eq!(parse("opendj-live:1:x"), None);
    }

    fn state(view: i32) -> State {
        let plugin = |name: &str, maker: &str, kind: &'static str| Plugin {
            key: format!("clap:{name}"),
            name: name.into(),
            maker: maker.into(),
            format: "CLAP",
            kind,
        };
        let preset = |name: &str, instrument: &str, instrument_name: &str, tags: &[&str]| Preset {
            name: name.into(),
            instrument: instrument.into(),
            instrument_name: instrument_name.into(),
            path: format!("/p/{name}.json").into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            from_plugin: false,
            reference: String::new(),
        };
        State {
            plugins: vec![
                plugin("Acid V", "Arturia", "Synth"),
                plugin("Vital", "Vital Audio", "Synth"),
                plugin("Jun-6 V", "arturia", "Synth"),
                plugin("Drumlab", "", "Drums"),
            ],
            routes: vec![
                "".into(),
                "M4 · In 3".into(),
                "".into(),
                "M4 · In 4".into(),
                "".into(),
            ],
            presets: vec![
                preset("Four Floor", GROOVE, "Groove Box", &["Drums"]),
                preset(
                    "Ambient Ebow",
                    "clap:Jun-6 V",
                    "Jun-6 V",
                    &["Pad", "Evolving"],
                ),
                preset("Warm Glass", "clap:Vital", "Vital", &["Pad", "Warm"]),
                Preset {
                    from_plugin: true,
                    ..preset("Factory Brass", "clap:Jun-6 V", "Jun-6 V", &["Brass"])
                },
            ],
            view,
            ..State::default()
        }
    }

    fn names(s: &State) -> Vec<String> {
        filtered(s).0.iter().map(|r| r.name.to_string()).collect()
    }

    fn facet<'a>(facets: &'a [Facet], name: &str) -> &'a Facet {
        facets.iter().find(|f| f.name == name).unwrap()
    }

    fn pick(s: &mut State, facet: &str, value: &str) {
        let f = facet_names(s.view)
            .iter()
            .position(|n| *n == facet)
            .unwrap();
        s.picks.resize(facet_names(s.view).len(), HashSet::new());
        s.picks[f].insert(value.into());
    }

    #[test]
    fn instruments_list_built_in_plugins_and_inputs() {
        let mut s = state(INSTRUMENTS);
        assert_eq!(
            names(&s),
            [
                "Groove Box",
                "Looper",
                "Remix",
                "Acid V",
                "Vital",
                "Jun-6 V",
                "Drumlab",
                "Microphone",
                "Mixer"
            ]
        );
        let rows = filtered(&s).0;
        assert_eq!(rows[7].problem, "No input picked");
        assert_eq!(rows[8].problem, "No inputs picked");
        assert_eq!(rows[6].tint, TINT_DRUMS);
        // Makers match whatever their case; "Other" for none.
        let (_, facets) = filtered(&s);
        let makers: Vec<&str> = facet(&facets, "Maker")
            .values
            .iter()
            .map(|(v, _, _)| v.as_str())
            .collect();
        assert_eq!(
            makers,
            ["Arturia", "arturia", "openDJ", "Other", "Vital Audio"]
        );
        pick(&mut s, "Kind", "Drums");
        assert_eq!(names(&s), ["Groove Box", "Drumlab"]);
        s.picks.clear();
        s.search = "vital".into();
        assert_eq!(names(&s), ["Vital"]);
    }

    #[test]
    fn presets_list_every_instruments_presets_by_name() {
        let s = state(PRESETS);
        assert_eq!(
            names(&s),
            ["Ambient Ebow", "Factory Brass", "Four Floor", "Warm Glass"]
        );
        let rows = filtered(&s).0;
        assert_eq!(rows[0].maker, "Jun-6 V");
        assert_eq!(rows[2].tint, TINT_DRUMS);
        assert_eq!(parse(&payload(PRESET, 1)), Some(5001));
    }

    #[test]
    fn filter_menus_narrow_each_other_and_keep_their_tag_order() {
        let mut s = state(PRESETS);
        let (_, facets) = filtered(&s);
        let types: Vec<(&str, usize)> = facet(&facets, "Sound type")
            .values
            .iter()
            .map(|(v, n, _)| (v.as_str(), *n))
            .collect();
        // TYPES' order, not name order.
        assert_eq!(types, [("Pad", 2), ("Brass", 1), ("Drums", 1)]);
        pick(&mut s, "Sound type", "Pad");
        assert_eq!(names(&s), ["Ambient Ebow", "Warm Glass"]);
        // Two picks in one menu: either.
        pick(&mut s, "Sound type", "Drums");
        assert_eq!(names(&s), ["Ambient Ebow", "Four Floor", "Warm Glass"]);
        // Another menu narrows, and its own counts follow the first menu.
        pick(&mut s, "Character", "Warm");
        assert_eq!(names(&s), ["Warm Glass"]);
        let (_, facets) = filtered(&s);
        let instruments: Vec<(&str, usize)> = facet(&facets, "Instrument")
            .values
            .iter()
            .map(|(v, n, _)| (v.as_str(), *n))
            .collect();
        assert_eq!(instruments, [("Vital", 1)]);
        // The Sound type menu counts as if nothing were picked in it.
        let pad = facet(&facets, "Sound type")
            .values
            .iter()
            .find(|(v, _, _)| v == "Pad")
            .unwrap();
        assert_eq!((pad.1, pad.2), (1, true));
        // A pick that matches nothing stays, to be taken off.
        let drums = facet(&facets, "Sound type")
            .values
            .iter()
            .find(|(v, _, _)| v == "Drums")
            .unwrap();
        assert_eq!((drums.1, drums.2), (0, true));
    }

    #[test]
    fn stars_narrow_the_list() {
        let mut s = state(PRESETS);
        s.starred.insert(recent_key(&s, PRESET, 2).unwrap());
        let rows = filtered(&s).0;
        assert!(
            rows.iter()
                .find(|r| r.name == "Warm Glass")
                .unwrap()
                .starred
        );
        s.starred_only = true;
        assert_eq!(names(&s), ["Warm Glass"]);
    }

    #[test]
    fn chips_break_into_lines_that_fit() {
        let values: Vec<(String, usize, bool)> = presets::TYPES
            .iter()
            .chain(presets::CHARACTERS.iter())
            .map(|v| (v.to_string(), 1, false))
            .collect();
        let lines = chip_lines(&values);
        assert!(lines.len() > 1);
        for line in &lines {
            let width: f32 = line.chips.iter().map(|c| chip_width(&c.name)).sum();
            assert!(width <= CHIP_LINE);
        }
        assert_eq!(
            lines.iter().map(|l| l.chips.row_count()).sum::<usize>(),
            values.len()
        );
    }

    #[test]
    fn the_mixer_sums_up_its_strips_interfaces() {
        let mut routes = vec![String::new(); inputs::NUM_INPUTS];
        assert_eq!(mixer_route(&routes), "");
        routes[inputs::AUXILIARY] = "M4 · In 3".into();
        assert_eq!(mixer_route(&routes), "M4 · 1 input");
        routes[inputs::AUXILIARY_3] = "M4 · In 4".into();
        // The mic's and the instrument's routes aren't strips.
        routes[inputs::MICROPHONE] = "MacBook Pro Microphone · In 1".into();
        routes[inputs::INSTRUMENT] = "Plugin".into();
        routes[inputs::AUXILIARIES[7]] = "iPhone · In 1".into();
        assert_eq!(mixer_route(&routes), "M4, iPhone · 3 inputs");
        // Its line sits where a deck's `live_input` less one finds it.
        let mut s = state(INSTRUMENTS);
        s.routes = routes;
        let mixer = mixer_route(&s.routes);
        s.routes.resize(MIXER_ROUTE, String::new());
        s.routes.push(mixer);
        assert_eq!(s.routes.len(), inputs::MIXER);
        let rows = filtered(&s).0;
        let row = rows.iter().find(|r| r.name == "Mixer").unwrap();
        assert_eq!(
            (row.maker.as_str(), row.problem.as_str()),
            ("M4, iPhone · 3 inputs", "")
        );
    }

    #[test]
    fn strips_are_the_aux_inputs_with_a_device() {
        let mut status = vec![String::new(); inputs::NUM_INPUTS];
        status[inputs::MICROPHONE] = "MacBook Pro Microphone · In 1 · 48 kHz".into();
        status[inputs::AUXILIARY] = "M4 · In 1 & 2 · 48 kHz".into();
        status[inputs::AUXILIARIES[4]] = "M4 · In 3 · 48 kHz".into();
        status[inputs::AUXILIARIES[7]] = "iPhone is not connected".into();
        status[inputs::AUXILIARIES[2]] = "iPhone · In 1 · 48 kHz".into();
        status[inputs::AUXILIARIES[5]] = inputs::GROOVE_BOX.into();
        let mut names = vec![String::new(); 8];
        names[4] = "Vocals".into();
        let clock = clock_device("M4 · 48 kHz · 256 samples (5.3 ms)");
        let shown: Vec<(i32, String, String, bool, String)> = strips(&status, &names, &[], clock)
            .into_iter()
            .map(|s| {
                let (name, channels, problem) =
                    (s.name.into(), s.channels.into(), s.problem.into());
                (s.number, name, channels, s.own_clock, problem)
            })
            .collect();
        assert_eq!(
            shown,
            [
                (1, "Ch 1".into(), "1/2".into(), false, "".into()),
                (3, "Ch 3".into(), "1".into(), true, "".into()),
                (5, "Vocals".into(), "3".into(), false, "".into()),
                // The Mixer deck's Groove Box: no channels, no clock.
                (6, "Groove".into(), "".into(), false, "".into()),
                (
                    8,
                    "Ch 8".into(),
                    "".into(),
                    false,
                    "iPhone is not connected".into()
                ),
            ]
        );
        // No main output: nothing to drift from.
        assert!(
            strips(&status, &names, &[], clock_device("No audio output"))
                .iter()
                .all(|s| !s.own_clock)
        );
    }

    #[test]
    fn recently_played_keeps_what_went_on_a_deck_newest_first() {
        let mut s = state(INSTRUMENTS);
        for (kind, index) in [(PLUGIN, 1), (KIT, 0), (MIC, 0), (PLUGIN, 1)] {
            let key = recent_key(&s, kind, index).unwrap();
            remember(&mut s.recent, key);
        }
        assert_eq!(s.recent, ["plugin:clap:Vital", "input:0", "groove"]);
    }

    #[test]
    fn routes_keep_the_device_and_channels() {
        assert_eq!(route("M4 · In 3 · 44.1 kHz"), "M4 · In 3");
        assert_eq!(
            route("M4 · In 1 & 2 · 48 kHz · silent so far"),
            "M4 · In 1 & 2"
        );
        assert_eq!(route(""), "");
        assert_eq!(route("M4 unavailable"), "");
        assert_eq!(route("M4 can't record In 3 at 44.1 kHz"), "");
    }
}
