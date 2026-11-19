//! The Mixed Set Editor (see `ui/mixed_set.slint`): a recorded set opened
//! from Mixed Sets as a timeline of its tracks and transitions, read from
//! the recorder's control log (or, without one, the cue sheet's
//! tracklist). It plays the recording back through the preview player
//! into the main mix, checks the mix for clipping, silence, level jumps
//! and the log's slips, makes quick fixes (trim, track level, cut,
//! loudness) that are kept in `<name>.edits.json` and rendered to a new
//! file on Save, exports the tracklist, YouTube chapters and a cue sheet,
//! and saves the transitions picked as pair mixes for Automix.
//!
//! The recording is never changed. Reading it for the waveform and the
//! checks, measuring loudness and saving each run on their own thread.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

use opendj_control::{ControlHandle, ControlRegistry};
use opendj_engine::analysis::WaveformMaker;
use opendj_engine::decode::Decoder;
use opendj_engine::preview::PREVIEW_GROUP;
use opendj_engine::set_chapters::{self, clock, Chapter, YoutubeNote};
use opendj_engine::set_checks::{self, Finding, Severity, Stretch, TrackGrid};
use opendj_engine::set_fixes::{self, Edits, Fix, Rendered, LOUDNESS_PRESETS};
use opendj_engine::set_render::{self, TransitionEdit};
use opendj_engine::set_timeline::{CueTrack, LaneKind, Recording, SetTimeline, Style};
use opendj_engine::Waveform;
use opendj_library::pair_mixes::PairMix;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{file_path, Browser, MIXED_SETS};
use crate::{
    files, AppWindow, FilesUi, MixedSetUi, SetBlock, SetFindingRow, SetFixRow, SetLane, SetSpan,
    SetTrackRow, SetTransitionRow,
};

/// Waveform slices a second: fine enough for a view of a few seconds.
const WAVE_RATE: f64 = 50.0;
/// The overview's size in pixels; it is stretched to its box.
const OVERVIEW_PX: (usize, usize) = (1600, 68);
/// The timeline waveform's height in pixels; stretched to its row.
const WAVE_PX_HIGH: usize = 240;
/// A transition plays from this many bars before it.
const TRANSITION_LEAD_BARS: f64 = 4.0;
/// A finding plays from this many bars before it.
const FINDING_LEAD_BARS: f64 = 2.0;
/// Transitions this long or longer are ticked to save as pair mixes.
const PAIR_MIN_BARS: f64 = 4.0;
/// Tick spacings for the ruler, in seconds.
const TICK_STEPS: [f64; 9] = [5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0];
/// The ruler aims for about this many ticks across the view.
const TICKS_IN_VIEW: f64 = 10.0;
/// A session's recording may start up to an hour before its first play.
const SESSION_LEAD: i64 = 3600;
/// The deck rows' size limit in the view.
const MIN_VIEW: f64 = 0.002;

/// What the editor's threads say.
enum Said {
    /// The waveform, and the audio checks' findings.
    Read {
        waveform: Waveform,
        findings: Vec<Finding>,
    },
    /// What the loudness preset would do.
    Measured(Rendered),
    /// The edited mix is written.
    Saved(PathBuf, Rendered),
    /// A transition is mixed again.
    Mixed(Fix),
    Failed(String),
}

/// The lengths a transition can be mixed again at, in bars.
const BAR_CHOICES: [f64; 5] = [2.0, 4.0, 8.0, 16.0, 32.0];
/// The curves it can be mixed again with, as the editor lists them.
const STYLE_CHOICES: [Style; 4] = [Style::Smooth, Style::BassSwap, Style::Filter, Style::Cut];

/// The preview player's controls, which play the set back.
struct Player {
    play: ControlHandle,
    position: ControlHandle,
    main: ControlHandle,
}

/// The set open in the editor.
struct Open {
    audio: PathBuf,
    title: String,
    artist: String,
    /// Seconds: the audio's length, else the log's.
    length: f64,
    timeline: Option<SetTimeline>,
    cue: Vec<CueTrack>,
    chapters: Vec<Chapter>,
    edits: Edits,
    /// Row on the timeline per deck (index = deck).
    row_of: Vec<i32>,
    show_decks: bool,
    /// Seconds.
    selection: Option<(f64, f64)>,
    /// Index into the blocks shown.
    block: Option<usize>,
    level_db: f64,
    transition: Option<usize>,
    /// Per transition: tick to save, the pair's ids, and its saved mix.
    pairs: Vec<Pair>,
    audio_findings: Vec<Finding>,
    log_findings: Vec<Finding>,
    findings: Vec<Finding>,
    checking: bool,
    measured: Option<Rendered>,
    measuring: bool,
    saving: bool,
    view_len: f32,
    said: Receiver<Said>,
    tell: Sender<Said>,
    /// Stops the threads once the editor closes.
    cancel: Arc<AtomicBool>,
    shown_position: (f32, bool),
    /// Seconds: where listening to a transition stops.
    listen_until: Option<f64>,
    /// A transition mixed again being played from its own file: where the
    /// file starts in the set and how long it is, in seconds, and the file.
    listen_mix: Option<(f64, f64, PathBuf)>,
    /// A transition being mixed again.
    mixing: bool,
    /// Why it couldn't be, or what to know about it.
    edit_note: String,
    /// The transition whose choices were last set in the card.
    shown_edit: std::cell::Cell<Option<usize>>,
    /// The track playing, as last shown in the Tracks tab.
    shown_track: i32,
    /// The recording's waveform, once read.
    waveform: Option<Waveform>,
    /// The view the timeline's waveform was last drawn for: start, length
    /// and width in pixels.
    drawn_view: (f32, f32, f32),
}

#[derive(Clone, Default)]
struct Pair {
    ticked: bool,
    ids: Option<(usize, usize)>,
    saved: String,
}

/// The editor's state on the browser.
pub(super) struct MixedSetEditor {
    player: Option<Player>,
    open: Option<Box<Open>>,
}

impl MixedSetEditor {
    pub(super) fn new(registry: &ControlRegistry) -> Self {
        let c = |k: &str| registry.get(PREVIEW_GROUP, k);
        let player = match (c("play"), c("playposition"), c("main")) {
            (Some(play), Some(position), Some(main)) => Some(Player {
                play,
                position,
                main,
            }),
            _ => None,
        };
        MixedSetEditor { player, open: None }
    }
}

impl Browser {
    pub(super) fn install_mixed_set(
        app: &AppWindow,
        with: &(impl Fn(&dyn Fn(&mut Browser)) + Clone + 'static),
    ) {
        let ui = app.global::<MixedSetUi>();
        ui.set_loudness_presets(strings(
            std::iter::once("Keep as recorded".to_string()).chain(
                LOUDNESS_PRESETS
                    .iter()
                    .map(|(name, lufs, _)| format!("{name} ({lufs} LUFS)")),
            ),
        ));
        let w = with.clone();
        app.global::<FilesUi>()
            .on_open_mixed_set(move |row| w(&|b| b.open_mixed_set_row(row as usize)));
        let w = with.clone();
        app.global::<crate::LibraryEdit>()
            .on_open_session_recording(move |row| w(&|b| b.open_session_recording(row as usize)));
        let w = with.clone();
        ui.on_close(move || w(&|b| b.close_mixed_set()));
        let w = with.clone();
        ui.on_play_pause(move || w(&|b| b.set_play_pause()));
        let w = with.clone();
        ui.on_seek(move |at| w(&|b| b.set_seek(at as f64, false)));
        let w = with.clone();
        ui.on_select_range(move |a, z| w(&|b| b.set_select_range(a as f64, z as f64)));
        let w = with.clone();
        ui.on_clear_range(move || {
            w(&|b| {
                if let Some(o) = &mut b.mixed_set.open {
                    if o.selection.take().is_some() {
                        b.show_mixed_set();
                    }
                }
            })
        });
        let w = with.clone();
        ui.on_zoom(move |f| w(&|b| b.set_zoom(f)));
        let w = with.clone();
        ui.on_select_block(move |i| w(&|b| b.set_select_block(i as usize)));
        let w = with.clone();
        ui.on_select_transition(move |i| w(&|b| b.set_select_transition(i as usize)));
        let w = with.clone();
        ui.on_select_track(move |i| w(&|b| b.set_select_track(i as usize)));
        let w = with.clone();
        ui.on_mix_transition(move || w(&|b| b.set_mix_transition()));
        let w = with.clone();
        ui.on_unmix_transition(move || w(&|b| b.set_unmix_transition()));
        let w = with.clone();
        ui.on_step_transition(move |d| w(&|b| b.set_step_transition(d)));
        let w = with.clone();
        ui.on_tick_pair(move |i, on| {
            w(&|b| {
                if let Some(p) = b
                    .mixed_set
                    .open
                    .as_mut()
                    .and_then(|o| o.pairs.get_mut(i as usize))
                {
                    p.ticked = on && p.ids.is_some();
                }
                b.show_mixed_set();
            })
        });
        let w = with.clone();
        ui.on_save_pairs(move || w(&|b| b.save_pair_mixes()));
        let w = with.clone();
        ui.on_select_finding(move |i| w(&|b| b.set_select_finding(i as usize)));
        let w = with.clone();
        ui.on_trim(move || w(&|b| b.set_trim()));
        let w = with.clone();
        ui.on_level(move |db| {
            w(&|b| {
                if let Some(o) = &mut b.mixed_set.open {
                    o.level_db = (db as f64).clamp(-12.0, 12.0);
                }
                b.show_mixed_set();
            })
        });
        let w = with.clone();
        ui.on_apply_level(move || w(&|b| b.set_apply_level()));
        let w = with.clone();
        ui.on_cut(move || w(&|b| b.set_cut()));
        let w = with.clone();
        ui.on_set_loudness(move |i| w(&|b| b.set_loudness(i)));
        let w = with.clone();
        ui.on_undo(move |i| {
            w(&|b| {
                if let Some(o) = &mut b.mixed_set.open {
                    if (i as usize) < o.edits.fixes.len() {
                        o.edits.fixes.remove(i as usize);
                    }
                }
                b.edits_changed();
            })
        });
        let w = with.clone();
        ui.on_save(move || w(&|b| b.save_edited_mix()));
        let w = with.clone();
        ui.on_export(move |i| w(&|b| b.export_set(i)));
    }

    /// Right-click › Open in Mixed Set Editor on a recording in Mixed Sets.
    fn open_mixed_set_row(&mut self, row: usize) {
        let picked = (self.tree == MIXED_SETS)
            .then(|| self.rows.get(row).copied().flatten())
            .flatten()
            .and_then(|i| {
                let t = &self.library[i];
                Some((
                    file_path(t)?.to_path_buf(),
                    t.title.clone(),
                    t.artist.clone(),
                ))
            });
        match picked {
            Some((path, title, artist)) => self.open_mixed_set(&path, &title, &artist),
            None => {
                if let Some(app) = self.app.upgrade() {
                    app.set_alert("Pick a recording in Mixed Sets first.".into());
                }
            }
        }
    }

    /// History › Sessions › Open in Mixed Set Editor: the recording that
    /// started closest to the session's first play, from an hour before it
    /// to its last.
    fn open_session_recording(&mut self, row: usize) {
        let Some(&node) = self.visible_nodes.get(row) else {
            return;
        };
        let super::Contents::Playlist(id) = self.nodes[node].contents else {
            return;
        };
        let times: Vec<i64> = self
            .db
            .as_ref()
            .and_then(|db| db.playlist_times(id).ok())
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect();
        let (Some(&first), Some(&last)) = (times.iter().min(), times.iter().max()) else {
            return;
        };
        let found = self
            .library
            .iter()
            .filter_map(|t| {
                let path = file_path(t)?;
                if let Some(dir) = &self.mixed_sets {
                    if !path.starts_with(dir) {
                        return None;
                    }
                }
                let started = recording_started(path)?;
                (first - SESSION_LEAD..=last)
                    .contains(&started)
                    .then(|| ((started - first).abs(), t))
            })
            .min_by_key(|(gap, _)| *gap)
            .and_then(|(_, t)| {
                Some((
                    file_path(t)?.to_path_buf(),
                    t.title.clone(),
                    t.artist.clone(),
                ))
            });
        match found {
            Some((path, title, artist)) => self.open_mixed_set(&path, &title, &artist),
            None => {
                if let Some(app) = self.app.upgrade() {
                    app.set_alert("No recording of this session in Mixed Sets.".into());
                }
            }
        }
    }

    /// Opens the recording whose first file is `audio` in the editor.
    pub(super) fn open_mixed_set(&mut self, audio: &Path, title: &str, artist: &str) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        self.close_mixed_set();
        let recording = Recording::open(audio);
        let (timeline, cue) = match recording {
            Recording::Logged(t) => (Some(*t), Vec::new()),
            Recording::CueOnly(c) => (None, c),
        };
        let audio_seconds = Decoder::open(audio).ok().and_then(|d| d.seconds());
        let length = audio_seconds
            .or(timeline.as_ref().map(|t| t.length))
            .unwrap_or(0.0)
            .max(1.0);
        let chapters = match &timeline {
            Some(t) => set_chapters::from_timeline(t),
            None => set_chapters::from_cue(&cue),
        };
        let row_of = match &timeline {
            Some(t) => {
                let decks = t.decks();
                let n = decks.iter().max().map_or(0, |d| d + 1);
                let mut rows = vec![0; n.max(1)];
                for (r, d) in decks.iter().enumerate() {
                    rows[*d] = r as i32;
                }
                rows
            }
            None => vec![0],
        };
        let (tell, said) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut open = Open {
            audio: audio.to_path_buf(),
            title: if title.trim().is_empty() {
                audio
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                title.trim().to_string()
            },
            artist: artist.trim().to_string(),
            length,
            timeline,
            cue,
            chapters,
            edits: Edits::load(audio),
            row_of,
            show_decks: true,
            listen_until: None,
            listen_mix: None,
            mixing: false,
            edit_note: String::new(),
            shown_edit: Default::default(),
            shown_track: -1,
            selection: None,
            block: None,
            level_db: 0.0,
            transition: None,
            pairs: Vec::new(),
            audio_findings: Vec::new(),
            log_findings: Vec::new(),
            findings: Vec::new(),
            checking: true,
            measured: None,
            measuring: false,
            saving: false,
            view_len: 1.0,
            said,
            tell,
            cancel,
            shown_position: (-1.0, false),
            waveform: None,
            drawn_view: (-1.0, -1.0, -1.0),
        };
        open.pairs = self.pairs_of(&open);
        if let Some(t) = &open.timeline {
            open.log_findings = set_checks::check_log(t, &self.grids_of(t));
            open.findings = open.log_findings.clone();
        }
        let stretches = match &open.timeline {
            Some(t) => set_checks::stretches(t),
            None => cue_stretches(&open.cue, length),
        };
        read_set(
            audio.to_path_buf(),
            stretches,
            open.tell.clone(),
            open.cancel.clone(),
        );
        let ui = app.global::<MixedSetUi>();
        ui.set_open(true);
        ui.set_loading(false);
        let blank = slint::Image::default();
        ui.set_wave_image(blank.clone());
        ui.set_overview_image(blank);
        ui.set_tab(3);
        ui.set_show_decks(true);
        ui.set_show_lanes(false);
        ui.set_view_start(0.0);
        ui.set_view_len(1.0);
        ui.set_title(open.title.as_str().into());
        ui.set_subtitle(subtitle(&open).into());
        ui.set_logged(open.timeline.is_some());
        let decks: Vec<i32> = match &open.timeline {
            Some(t) => t.decks().iter().map(|d| *d as i32).collect(),
            None => vec![0],
        };
        ui.set_deck_count(decks.len().max(1) as i32);
        ui.set_deck_rows(ModelRc::new(VecModel::from(decks)));
        ui.set_deck_rows(ModelRc::new(VecModel::from(
            (0..open.row_of.len())
                .filter(|d| {
                    open.row_of
                        .iter()
                        .filter(|r| **r == open.row_of[*d])
                        .count()
                        == 1
                        || *d == 0
                })
                .map(|d| d as i32)
                .collect::<Vec<_>>(),
        )));
        ui.set_loudness(
            open.edits
                .loudness()
                .and_then(|(lufs, _)| LOUDNESS_PRESETS.iter().position(|p| p.1 == lufs))
                .map_or(-1, |i| i as i32),
        );
        ui.set_status(
            if open.timeline.is_some() {
                "Reading the recording…"
            } else {
                "This recording has no control log: the tracklist comes from its cue sheet. Reading the recording…"
            }
            .into(),
        );
        ui.set_lanes(ModelRc::new(VecModel::from(lanes(&open))));
        self.mixed_set.open = Some(Box::new(open));
        self.show_mixed_set();
        self.measure_loudness();
        // Plays back through the preview player, into the main mix,
        // streamed from the file so a long set is never held in memory.
        self.engine.preview_stream(audio.to_path_buf(), 0.0, false);
        if let Some(p) = &self.mixed_set.player {
            p.main.set_bool(true);
        }
    }

    fn close_mixed_set(&mut self) {
        let Some(open) = self.mixed_set.open.take() else {
            return;
        };
        open.cancel.store(true, Ordering::Release);
        if let Some(p) = &self.mixed_set.player {
            p.play.set_bool(false);
            p.main.set_bool(false);
        }
        self.engine.preview(None, false);
        if let Some(app) = self.app.upgrade() {
            app.global::<MixedSetUi>().set_open(false);
        }
    }

    /// The decks' grids, for the log checks.
    fn grids_of(&self, t: &SetTimeline) -> Vec<Option<TrackGrid>> {
        t.tracks
            .iter()
            .map(|lt| {
                let i = *self.path_index.get(lt.path.as_ref()?)?;
                let track = &self.library[i];
                (track.bpm > 0.0).then(|| TrackGrid {
                    first_beat: track.first_beat,
                    bpm: track.bpm,
                    phrases: crate::automix_phrases::of(track)
                        .iter()
                        .map(|p| p.start)
                        .collect(),
                })
            })
            .collect()
    }

    /// Each transition's pair: whether to tick it and the mix saved for it.
    fn pairs_of(&self, open: &Open) -> Vec<Pair> {
        let Some(t) = &open.timeline else {
            return Vec::new();
        };
        let Some(db) = &self.db else {
            return vec![Pair::default(); t.transitions.len()];
        };
        let id = |track: usize| {
            t.tracks[track]
                .path
                .as_ref()
                .and_then(|p| db.track_id_at_path(p).ok().flatten())
        };
        t.transitions
            .iter()
            .map(|x| {
                let ids = id(x.from_track).zip(id(x.to_track));
                let saved = ids
                    .and_then(|(a, b)| db.pair_mix(a, b).ok().flatten())
                    .map(|m| format!("Saved {}", short_date(m.created)))
                    .unwrap_or_default();
                let long = x.bars.unwrap_or(0.0) >= PAIR_MIN_BARS;
                Pair {
                    ticked: ids.is_some()
                        && long
                        && x.style != opendj_engine::set_timeline::Style::Cut,
                    ids,
                    saved,
                }
            })
            .collect()
    }

    fn save_pair_mixes(&mut self) {
        let Some(open) = &mut self.mixed_set.open else {
            return;
        };
        let Some(t) = &open.timeline else { return };
        let Some(db) = &self.db else { return };
        let source = open
            .audio
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut saved = 0;
        let mut failed = None;
        for (x, pair) in t.transitions.iter().zip(open.pairs.iter_mut()) {
            let (Some((a, b)), true) = (pair.ids, pair.ticked) else {
                continue;
            };
            let mix = PairMix {
                out_track: a,
                in_track: b,
                out_seconds: x.out_point,
                in_seconds: x.in_point,
                bars: x.bars.unwrap_or(0.0),
                style: x.style.name().to_string(),
                source: source.clone(),
                created: 0,
            };
            match db.save_pair_mix(&mix) {
                Ok(true) => {
                    saved += 1;
                    pair.saved = "Saved today".into();
                }
                Ok(false) => {}
                Err(e) => failed = Some(e.to_string()),
            }
        }
        let status = match (saved, failed) {
            (_, Some(e)) => format!("Couldn't save the pair mixes: {e}"),
            (0, None) => "No pair mixes to save.".to_string(),
            (1, None) => {
                "Saved 1 pair mix: Automix mixes those two tracks this way next time.".into()
            }
            (n, None) => {
                format!("Saved {n} pair mixes: Automix mixes those tracks this way next time.")
            }
        };
        self.mixed_set_status(&status);
        self.show_mixed_set();
    }

    fn set_play_pause(&mut self) {
        let (Some(p), Some(_)) = (&self.mixed_set.player, &self.mixed_set.open) else {
            return;
        };
        if p.play.get_bool() {
            p.play.set_bool(false);
        } else if let Some((_, len, file)) = self
            .mixed_set
            .open
            .as_ref()
            .and_then(|o| o.listen_mix.clone())
        {
            let at = p.position.get();
            self.engine.preview_stream(file, at * len, true);
        } else {
            // Streamed again from where it is: after the end it starts over.
            self.set_seek(p.position.get(), true);
        }
    }

    /// Plays from `at` (a fraction of the set); `play` starts it too.
    fn set_seek(&mut self, at: f64, play: bool) {
        let (Some(p), Some(o)) = (&self.mixed_set.player, &mut self.mixed_set.open) else {
            return;
        };
        o.listen_until = None;
        o.listen_mix = None;
        let play = play || p.play.get_bool();
        let at = at.clamp(0.0, 1.0);
        // Shown there at once; the stream catches up a moment later.
        p.position.set(at);
        self.engine
            .preview_stream(o.audio.clone(), at * o.length, play);
    }

    fn set_select_range(&mut self, a: f64, z: f64) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let (a, z) = (a.min(z).clamp(0.0, 1.0), a.max(z).clamp(0.0, 1.0));
        o.selection = Some((a * o.length, z * o.length));
        self.show_mixed_set();
    }

    fn set_zoom(&mut self, factor: f32) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let ui = app.global::<MixedSetUi>();
        let len = ui.get_view_len();
        let new_len = (len * factor).clamp(MIN_VIEW as f32, 1.0);
        // Keeps the playhead in view when it is, else the middle.
        let start = ui.get_view_start();
        let pos = ui.get_position();
        let anchor = if (start..start + len).contains(&pos) {
            pos
        } else {
            start + len / 2.0
        };
        let new_start = (anchor - (anchor - start) * new_len / len).clamp(0.0, 1.0 - new_len);
        ui.set_view_len(new_len);
        ui.set_view_start(new_start);
        o.view_len = new_len;
        let (ticks, at) = ticks(o.length, new_len as f64);
        ui.set_ticks(strings(ticks));
        ui.set_tick_at(ModelRc::new(VecModel::from(at)));
    }

    fn set_select_block(&mut self, i: usize) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        o.block = Some(i);
        o.level_db = 0.0;
        self.show_mixed_set();
    }

    fn set_select_transition(&mut self, i: usize) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let Some(t) = &o.timeline else { return };
        let Some(x) = t.transitions.get(i) else {
            return;
        };
        o.transition = Some(i);
        o.edit_note.clear();
        // Mixed again: heard from its own file, a few bars either side.
        if let Some((start, end, file, _)) = o
            .edits
            .mixes()
            .into_iter()
            .find(|m| m.3 == i)
            .map(|(s, e, f, t)| (s, e, f.to_path_buf(), t))
        {
            self.show_mixed_set();
            self.set_seek(
                start / self.mixed_set.open.as_ref().map_or(1.0, |o| o.length),
                false,
            );
            self.engine.preview_stream(file.clone(), 0.0, true);
            if let Some(o) = &mut self.mixed_set.open {
                o.listen_mix = Some((start, end - start, file));
            }
            return;
        }
        let bar = bar_seconds(t, x.from_track, x.start);
        let at = (x.start - TRANSITION_LEAD_BARS * bar).max(0.0) / o.length;
        // Heard with a few bars either side, then it stops.
        let until = x.end + TRANSITION_LEAD_BARS * bar_seconds(t, x.to_track, x.end);
        self.show_mixed_set();
        self.set_seek(at, true);
        if let Some(o) = &mut self.mixed_set.open {
            o.listen_until = Some(until);
        }
    }

    /// Mixes the selected transition again with the length and curve
    /// picked, in the background.
    fn set_mix_transition(&mut self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let ui = app.global::<MixedSetUi>();
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let (Some(i), Some(t)) = (o.transition, &o.timeline) else {
            return;
        };
        if o.mixing {
            return;
        }
        let bars = BAR_CHOICES[(ui.get_edit_bars().max(0) as usize).min(BAR_CHOICES.len() - 1)];
        let style =
            STYLE_CHOICES[(ui.get_edit_style().max(0) as usize).min(STYLE_CHOICES.len() - 1)];
        let edit = TransitionEdit {
            transition: i,
            bars,
            style,
        };
        // Refused at once if it can't fit.
        if let Err(e) = set_render::transition_window(t, &edit) {
            o.edit_note = e;
            self.show_mixed_set();
            return;
        }
        o.mixing = true;
        o.edit_note = "Mixing it again from the set's log…".into();
        let (audio, timeline, tell) = (o.audio.clone(), t.clone(), o.tell.clone());
        let stamp = chrono::Local::now().format("%H%M%S");
        let dest = mix_path(&audio, i, &stamp.to_string());
        std::thread::spawn(move || {
            let said = match set_render::render_transition(&audio, &timeline, &edit, &dest) {
                Ok((start, end)) => Said::Mixed(Fix::Mix {
                    start,
                    end,
                    file: dest,
                    transition: i,
                    bars,
                    style: style.name().to_string(),
                }),
                Err(e) => Said::Failed(format!("Couldn't mix the transition again: {e}")),
            };
            let _ = tell.send(said);
        });
        self.show_mixed_set();
    }

    /// Back to the selected transition as it was played.
    fn set_unmix_transition(&mut self) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let Some(i) = o.transition else { return };
        let mut files = Vec::new();
        o.edits.fixes.retain(|f| match f {
            Fix::Mix {
                transition, file, ..
            } if *transition == i => {
                files.push(file.clone());
                false
            }
            _ => true,
        });
        o.listen_mix = None;
        o.edit_note.clear();
        for f in files {
            let _ = std::fs::remove_file(f);
        }
        self.edits_changed();
    }

    /// Jumps to the start of track `i` in the tracklist.
    fn set_select_track(&mut self, i: usize) {
        let Some(o) = &self.mixed_set.open else {
            return;
        };
        let Some(c) = o.chapters.get(i) else { return };
        let at = c.start / o.length.max(1e-9);
        self.set_seek(at, false);
    }

    fn set_step_transition(&mut self, step: i32) {
        let Some(o) = &self.mixed_set.open else {
            return;
        };
        let n = o.timeline.as_ref().map_or(0, |t| t.transitions.len()) as i32;
        if n == 0 {
            return;
        }
        let i = o.transition.map_or(0, |i| i as i32 + step).clamp(0, n - 1);
        self.set_select_transition(i as usize);
    }

    fn set_select_finding(&mut self, i: usize) {
        let Some(o) = &self.mixed_set.open else {
            return;
        };
        let Some(f) = o.findings.get(i) else { return };
        let bar = o.timeline.as_ref().map_or(2.0, |t| {
            t.clips
                .iter()
                .find(|c| c.start <= f.start && f.start < c.end)
                .map_or(2.0, |c| bar_seconds(t, c.track, f.start))
        });
        let at = (f.start - FINDING_LEAD_BARS * bar).max(0.0) / o.length;
        self.set_seek(at, true);
    }

    fn set_trim(&mut self) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let Some((start, end)) = o.selection.take() else {
            return;
        };
        o.edits.fixes.retain(|f| !matches!(f, Fix::Trim { .. }));
        o.edits.fixes.push(Fix::Trim {
            start,
            end,
            fade_in: set_fixes::TRIM_FADE_IN,
            fade_out: set_fixes::TRIM_FADE_OUT,
        });
        self.edits_changed();
    }

    fn set_cut(&mut self) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let Some((start, end)) = o.selection.take() else {
            return;
        };
        o.edits.fixes.push(Fix::Cut { start, end });
        self.edits_changed();
    }

    fn set_apply_level(&mut self) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let Some(block) = o.block.and_then(|i| blocks(o).get(i).cloned()) else {
            return;
        };
        if o.level_db == 0.0 {
            return;
        }
        let ease = o
            .timeline
            .as_ref()
            .and_then(|t| {
                let c = t
                    .clips
                    .iter()
                    .find(|c| c.start <= block.start && block.start < c.end)?;
                let bpm = t.tracks[c.track].bpm * c.speed;
                (bpm > 0.0).then(|| 60.0 / bpm)
            })
            .unwrap_or(0.5);
        o.edits.fixes.push(Fix::TrackLevel {
            start: block.start,
            end: block.end,
            db: o.level_db,
            ease,
            title: block.title.clone(),
        });
        o.level_db = 0.0;
        self.edits_changed();
    }

    fn set_loudness(&mut self, preset: i32) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        o.edits.fixes.retain(|f| !matches!(f, Fix::Loudness { .. }));
        if let Some((_, lufs, ceiling)) = usize::try_from(preset)
            .ok()
            .and_then(|i| LOUDNESS_PRESETS.get(i))
        {
            o.edits.fixes.push(Fix::Loudness {
                lufs: *lufs,
                ceiling: *ceiling,
            });
        }
        if let Some(app) = self.app.upgrade() {
            app.global::<MixedSetUi>().set_loudness(preset);
        }
        self.edits_changed();
    }

    /// Saves the fixes beside the recording and shows them.
    fn edits_changed(&mut self) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let saved = if o.edits.fixes.is_empty() {
            match std::fs::remove_file(set_fixes::edits_path(&o.audio)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else {
            o.edits.save(&o.audio)
        };
        if let Err(e) = saved {
            let text = format!("Couldn't keep the fixes: {e}");
            self.mixed_set_status(&text);
        }
        self.measure_loudness();
        self.show_mixed_set();
    }

    /// Measures what the loudness preset would do, after the other fixes.
    fn measure_loudness(&mut self) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        o.measured = None;
        o.measuring = true;
        let (audio, edits, tell, cancel) = (
            o.audio.clone(),
            o.edits.clone(),
            o.tell.clone(),
            o.cancel.clone(),
        );
        spawn("set loudness", move || {
            let said = match frames(&audio, cancel.clone()) {
                Ok((rate, frames)) => {
                    let r = set_fixes::measure(frames, rate as f64, &edits);
                    if cancel.load(Ordering::Acquire) {
                        return;
                    }
                    Said::Measured(r)
                }
                Err(e) => Said::Failed(format!("Couldn't measure the loudness: {e}")),
            };
            let _ = tell.send(said);
        });
    }

    fn save_edited_mix(&mut self) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        if o.saving || o.edits.fixes.is_empty() {
            return;
        }
        o.saving = true;
        let settings = crate::settings::recording();
        let (audio, edits, tell) = (o.audio.clone(), o.edits.clone(), o.tell.clone());
        let chapters = set_chapters::follow(&o.chapters, &o.edits, o.length);
        let title = o.title.clone();
        spawn("set save", move || {
            let dest = set_fixes::edited_path(&audio, settings.format);
            let said =
                match set_fixes::render(&audio, &dest, settings.format, settings.bits, &edits) {
                    Ok(r) => match write_edited_lists(&dest, &title, &chapters) {
                        Ok(()) => Said::Saved(dest, r),
                        Err(e) => {
                            Said::Failed(format!("Saved the mix, but not its tracklist: {e}"))
                        }
                    },
                    Err(e) => Said::Failed(format!("Couldn't save the edited mix: {e}")),
                };
            let _ = tell.send(said);
        });
        self.mixed_set_status("Saving the edited mix…");
        self.show_mixed_set();
    }

    /// Export ▾: 0 the Mixcloud tracklist, 1 YouTube chapters, 2 a cue
    /// sheet (written beside the recording, and shown), 3 Export for
    /// Mixcloud, from the edited mix once there is one.
    fn export_set(&mut self, what: i32) {
        let Some(o) = &self.mixed_set.open else {
            return;
        };
        let chapters = set_chapters::follow(&o.chapters, &o.edits, o.length);
        let kept = o.edits.map_time(o.length - 1e-6).unwrap_or(o.length);
        let edited = !o.edits.fixes.is_empty();
        let base = recording_stem(&o.audio);
        let format = crate::settings::recording().format;
        let edited_audio = set_fixes::edited_path(&o.audio, format);
        if what == 3 {
            let source = if edited && edited_audio.is_file() {
                edited_audio
            } else {
                o.audio.clone()
            };
            let (title, artist) = (o.title.clone(), o.artist.clone());
            if edited && source == o.audio {
                self.mixed_set_status("Save the edited mix first to export it with your fixes; making the MP3 from the recording as it is…");
            }
            self.mix_job_at(source, title, artist, super::mix_export::Job::Export);
            return;
        }
        if chapters.is_empty() {
            self.mixed_set_status("This set has no tracklist to export.");
            return;
        }
        let ending = if edited { " (edited)" } else { "" };
        let (path, text, note) = match what {
            0 => (
                with_name(
                    &o.audio,
                    &format!("{base}{ending} - Mixcloud tracklist.txt"),
                ),
                set_chapters::tracklist(&chapters),
                String::new(),
            ),
            1 => {
                let (text, notes) = set_chapters::youtube(&chapters, kept);
                let note = notes
                    .iter()
                    .map(|n| match n {
                        YoutubeNote::TooFew => {
                            " YouTube shows chapters only when there are three or more.".to_string()
                        }
                        YoutubeNote::Joined(1) => {
                            " One track under 10 seconds was joined to the one before.".to_string()
                        }
                        YoutubeNote::Joined(n) => {
                            format!(" {n} tracks under 10 seconds were joined to the one before.")
                        }
                    })
                    .collect::<String>();
                (
                    with_name(&o.audio, &format!("{base}{ending} - YouTube chapters.txt")),
                    text,
                    note,
                )
            }
            _ => {
                let audio = if edited { &edited_audio } else { &o.audio };
                let name = audio.file_name().unwrap_or_default().to_string_lossy();
                (
                    with_name(&o.audio, &format!("{base}{ending} - cue sheet.cue")),
                    set_chapters::cue_sheet(&chapters, &o.title, &name),
                    String::new(),
                )
            }
        };
        let status = match std::fs::write(&path, text) {
            Ok(()) => {
                files::reveal(&path);
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                format!("Wrote {name} beside the recording.{note}")
            }
            Err(e) => format!("Couldn't write {}: {e}", path.display()),
        };
        self.mixed_set_status(&status);
    }

    pub(super) fn mixed_set_status(&self, text: &str) {
        if self.mixed_set.open.is_none() {
            return;
        }
        if let Some(app) = self.app.upgrade() {
            app.global::<MixedSetUi>().set_status(text.into());
        }
    }

    /// Picks up the threads' news and the playhead. Called every frame.
    pub fn poll_mixed_set(&mut self, app: &AppWindow) {
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let ui = app.global::<MixedSetUi>();
        let mut changed = false;
        let mut mixed = None;
        loop {
            match o.said.try_recv() {
                Ok(Said::Read { waveform, findings }) => {
                    let (w, h) = OVERVIEW_PX;
                    ui.set_overview_image(crate::waves::draw(&waveform.overview(w), w, h, true));
                    o.waveform = Some(waveform);
                    o.drawn_view = (-1.0, -1.0, -1.0);
                    o.audio_findings = findings;
                    o.checking = false;
                    let mut all = o.audio_findings.clone();
                    all.extend(o.log_findings.iter().cloned());
                    all.sort_by(|a, b| a.start.total_cmp(&b.start));
                    o.findings = all;
                    ui.set_status(SharedString::new());
                    changed = true;
                }
                Ok(Said::Measured(r)) => {
                    o.measured = Some(r);
                    o.measuring = false;
                    changed = true;
                }
                Ok(Said::Saved(path, r)) => {
                    o.saving = false;
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let mut text = format!("Saved {name}, with its tracklist.");
                    if let Some(lufs) = r.after_lufs {
                        text += &format!(" It plays at {lufs:.1} LUFS.");
                    }
                    if r.squashed() {
                        text += " It needed heavy limiting to get there.";
                    }
                    ui.set_status(text.into());
                    files::reveal(&path);
                    let path = path.clone();
                    changed = true;
                    self.add_recordings(&[path]);
                    break;
                }
                Ok(Said::Mixed(fix)) => {
                    o.mixing = false;
                    o.edit_note =
                        "Kept with your fixes: Save edited mix writes it into a new file.".into();
                    let i = match &fix {
                        Fix::Mix { transition, .. } => *transition,
                        _ => 0,
                    };
                    o.edits.fixes.push(fix);
                    mixed = Some(i);
                    changed = true;
                }
                Ok(Said::Failed(e)) => {
                    if o.mixing {
                        o.mixing = false;
                        o.edit_note = e.clone();
                    }
                    o.saving = false;
                    o.measuring = false;
                    o.checking = false;
                    ui.set_status(e.into());
                    changed = true;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let Some(o) = &mut self.mixed_set.open else {
            return;
        };
        let show_decks = ui.get_show_decks();
        if show_decks != o.show_decks {
            o.show_decks = show_decks;
            o.block = None;
            changed = true;
        }
        // The timeline's waveform, drawn for what it shows at its width.
        let view = (ui.get_view_start(), ui.get_view_len(), ui.get_wave_px());
        if view != o.drawn_view && view.2 >= 1.0 {
            if let Some(w) = &o.waveform {
                o.drawn_view = view;
                let px = view.2.round() as usize;
                let from = view.0 as f64 * o.length;
                let to = (view.0 + view.1) as f64 * o.length;
                let slices = w.overview_of(from, to, px);
                ui.set_wave_image(crate::waves::draw(&slices, px, WAVE_PX_HIGH, true));
            }
        }
        if let Some(p) = &self.mixed_set.player {
            let at = match &o.listen_mix {
                // A transition mixed again plays from its own file.
                Some((start, len, _)) => ((start + p.position.get() * len) / o.length) as f32,
                None => p.position.get() as f32,
            };
            let playing = p.play.get_bool();
            // Only just past the end: until the new stream comes in, the
            // player still says where the old one was.
            if let Some(until) = o.listen_until {
                let now = at as f64 * o.length;
                if playing && (until..until + 2.0).contains(&now) {
                    p.play.set_bool(false);
                    o.listen_until = None;
                }
            }
            let now = o
                .chapters
                .iter()
                .rposition(|c| c.start <= at as f64 * o.length)
                .map_or(-1, |i| i as i32);
            if now != o.shown_track {
                o.shown_track = now;
                ui.set_playing_track(now);
            }
            if (at, playing) != o.shown_position {
                o.shown_position = (at, playing);
                ui.set_position(at);
                ui.set_playing(playing);
                ui.set_position_text(clock(at as f64 * o.length).into());
                // Follows the playhead once it runs off the view.
                let (start, len) = (ui.get_view_start(), ui.get_view_len());
                if playing && len < 1.0 && !(start..start + len).contains(&at) {
                    ui.set_view_start(at.clamp(0.0, 1.0 - len));
                }
            }
        }
        if let Some(i) = mixed {
            self.edits_changed();
            // Heard straight away.
            self.set_select_transition(i);
        } else if changed {
            self.show_mixed_set();
        }
    }

    /// Shows the open set: blocks, spans, transitions, findings and fixes.
    fn show_mixed_set(&mut self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let Some(o) = &self.mixed_set.open else {
            return;
        };
        let ui = app.global::<MixedSetUi>();
        let len = o.length;
        let frac = |s: f64| (s / len).clamp(0.0, 1.0) as f32;
        let shown = blocks(o);
        ui.set_blocks(ModelRc::new(VecModel::from(
            shown
                .iter()
                .enumerate()
                .map(|(i, b)| SetBlock {
                    start: frac(b.start),
                    end: frac(b.end),
                    title: b.title.as_str().into(),
                    artist: b.artist.as_str().into(),
                    deck: b.row,
                    selected: o.block == Some(i),
                })
                .collect::<Vec<_>>(),
        )));
        let mut spans = Vec::new();
        let span =
            |start: f64, end: f64, deck: i32, label: &str, kind: i32, selected: bool| SetSpan {
                start: frac(start),
                end: frac(end),
                deck,
                label: label.into(),
                kind,
                selected,
            };
        if let Some(t) = &o.timeline {
            let mixes = o.edits.mixes();
            for (i, x) in t.transitions.iter().enumerate() {
                let label = match mixed_again(&o.edits, i) {
                    Some((bars, style)) => format!("{style} · {bars} bars (new)"),
                    None => x.style.name().to_string(),
                };
                let end = mixes
                    .iter()
                    .find(|m| m.3 == i)
                    .and_then(|_| mixed_end(&o.edits, i, t))
                    .unwrap_or(x.end);
                spans.push(span(x.start, end, 0, &label, 0, o.transition == Some(i)));
            }
            let row = |d: usize| o.row_of.get(d).copied().unwrap_or(0);
            for l in &t.loops {
                let label = format!("Loop ×{}", l.repeats);
                spans.push(span(l.start, l.end, row(l.deck), &label, 1, false));
            }
            for m in &t.hard_moves {
                spans.push(span(m.start, m.end, row(m.deck), "Scratch", 2, false));
            }
        }
        for f in &o.edits.fixes {
            match f {
                Fix::Cut { start, end } => spans.push(span(*start, *end, 0, "Cut", 4, false)),
                Fix::Trim { start, end, .. } => {
                    if *start > 0.0 {
                        spans.push(span(0.0, *start, 0, "Trimmed", 5, false));
                    }
                    if *end < len {
                        spans.push(span(*end, len, 0, "Trimmed", 5, false));
                    }
                }
                _ => {}
            }
        }
        if let Some((a, z)) = o.selection {
            spans.push(span(a, z, 0, "", 3, false));
        }
        ui.set_spans(ModelRc::new(VecModel::from(spans)));
        let (ticks_text, ticks_at) = ticks(len, o.view_len as f64);
        ui.set_ticks(strings(ticks_text));
        ui.set_tick_at(ModelRc::new(VecModel::from(ticks_at)));

        // Transitions.
        let mut rows = Vec::new();
        if let Some(t) = &o.timeline {
            for (i, x) in t.transitions.iter().enumerate() {
                let pair = o.pairs.get(i).cloned().unwrap_or_default();
                rows.push(SetTransitionRow {
                    number: i as i32 + 1,
                    out_title: t.tracks[x.from_track].title.as_str().into(),
                    in_title: t.tracks[x.to_track].title.as_str().into(),
                    at: clock(x.start).into(),
                    length: transition_length(x).into(),
                    style: x.style.name().into(),
                    ticked: pair.ticked,
                    saved: pair.saved.into(),
                });
            }
            ui.set_pair_count(o.pairs.iter().filter(|p| p.ticked).count() as i32);
            match o.transition.and_then(|i| t.transitions.get(i)) {
                Some(x) => {
                    let name = |track: usize| {
                        let lt = &t.tracks[track];
                        if lt.artist.trim().is_empty() {
                            lt.title.clone()
                        } else {
                            format!("{} - {}", lt.artist, lt.title)
                        }
                    };
                    ui.set_selected_transition(o.transition.unwrap_or(0) as i32);
                    ui.set_detail_title(
                        format!(
                            "Transition {} of {}",
                            o.transition.unwrap_or(0) + 1,
                            t.transitions.len()
                        )
                        .into(),
                    );
                    ui.set_detail_out(
                        format!(
                            "{} (deck {}, from {})",
                            name(x.from_track),
                            x.from_deck + 1,
                            clock(x.out_point)
                        )
                        .into(),
                    );
                    ui.set_detail_in(
                        format!(
                            "{} (deck {}, from {})",
                            name(x.to_track),
                            x.to_deck + 1,
                            clock(x.in_point)
                        )
                        .into(),
                    );
                    ui.set_detail_at(clock(x.start).into());
                    ui.set_detail_length(transition_length(x).into());
                    ui.set_detail_style(x.style.name().into());
                    let i = o.transition.unwrap_or(0);
                    let (bars, style) = match mixed_again(&o.edits, i) {
                        Some((bars, style)) => {
                            ui.set_detail_mixed(
                                format!("Mixed again: {bars} bars, {style}").into(),
                            );
                            (
                                bars,
                                STYLE_CHOICES
                                    .iter()
                                    .position(|s| s.name() == style)
                                    .unwrap_or(0),
                            )
                        }
                        None => {
                            ui.set_detail_mixed(SharedString::new());
                            (
                                x.bars.unwrap_or(4.0),
                                STYLE_CHOICES
                                    .iter()
                                    .position(|s| *s == x.style)
                                    .unwrap_or(0),
                            )
                        }
                    };
                    if o.shown_edit.get() != Some(i) {
                        // The choices start at what it is now.
                        o.shown_edit.set(Some(i));
                        let nearest = BAR_CHOICES
                            .iter()
                            .enumerate()
                            .min_by(|a, b| {
                                (a.1.ln() - bars.max(1.0).ln())
                                    .abs()
                                    .total_cmp(&(b.1.ln() - bars.max(1.0).ln()).abs())
                            })
                            .map_or(1, |(k, _)| k);
                        ui.set_edit_bars(nearest as i32);
                        ui.set_edit_style(style as i32);
                    }
                    ui.set_mixing(o.mixing);
                    ui.set_edit_note(o.edit_note.as_str().into());
                    ui.set_detail_sync(
                        o.log_findings
                            .iter()
                            .find(|f| {
                                f.kind == set_checks::CheckKind::Drift
                                    && f.start < x.end
                                    && f.end > x.start
                            })
                            .map_or_else(|| "In time".to_string(), |f| f.text.clone())
                            .into(),
                    );
                }
                None => ui.set_selected_transition(-1),
            }
        } else {
            ui.set_pair_count(0);
            ui.set_selected_transition(-1);
        }
        ui.set_transitions(ModelRc::new(VecModel::from(rows)));
        ui.set_tracks(ModelRc::new(VecModel::from(
            o.chapters
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let end = o.chapters.get(i + 1).map_or(o.length, |n| n.start);
                    SetTrackRow {
                        number: i as i32 + 1,
                        title: c.title.as_str().into(),
                        artist: c.artist.as_str().into(),
                        at: clock(c.start).into(),
                        length: clock((end - c.start).max(0.0)).into(),
                    }
                })
                .collect::<Vec<_>>(),
        )));

        // Check my mix.
        ui.set_checking(o.checking);
        ui.set_findings(ModelRc::new(VecModel::from(
            o.findings
                .iter()
                .map(|f| SetFindingRow {
                    title: f.kind.name().into(),
                    text: f.text.as_str().into(),
                    at: clock(f.start).into(),
                    problem: f.severity == Severity::Problem,
                })
                .collect::<Vec<_>>(),
        )));

        // Fixes.
        ui.set_fixes(ModelRc::new(VecModel::from(
            o.edits.fixes.iter().map(fix_row).collect::<Vec<_>>(),
        )));
        ui.set_has_selection(o.selection.is_some());
        ui.set_selection_text(
            o.selection
                .map(|(a, z)| format!("{} to {}", clock(a), clock(z)))
                .unwrap_or_default()
                .into(),
        );
        match o.block.and_then(|i| shown.get(i)) {
            Some(b) => {
                ui.set_level_track(b.title.as_str().into());
                ui.set_level_db(o.level_db as f32);
                ui.set_level_note(
                    format!(
                        "{}, {} to {}. Raises or lowers everything heard then.",
                        b.title,
                        clock(b.start),
                        clock(b.end)
                    )
                    .into(),
                );
            }
            None => {
                ui.set_level_track(SharedString::new());
                ui.set_level_db(0.0);
            }
        }
        let preset = o.edits.loudness();
        ui.set_loudness_warning(
            preset.is_some() && o.measured.as_ref().is_some_and(|r| r.squashed()),
        );
        ui.set_loudness_text(loudness_text(o, preset).into());
        ui.set_saving(o.saving);
    }
}

/// A block on the timeline, in seconds.
#[derive(Clone)]
struct Block {
    start: f64,
    end: f64,
    title: String,
    artist: String,
    row: i32,
}

/// The tracks shown: each clip on its deck's row with Show decks on, else
/// the tracklist end to end.
fn blocks(o: &Open) -> Vec<Block> {
    if let (true, Some(t)) = (o.show_decks, &o.timeline) {
        return t
            .clips
            .iter()
            .filter(|c| !c.is_empty())
            .map(|c| Block {
                start: c.start,
                end: c.end,
                title: t.tracks[c.track].title.clone(),
                artist: t.tracks[c.track].artist.clone(),
                row: o.row_of.get(c.deck).copied().unwrap_or(0),
            })
            .collect();
    }
    o.chapters
        .iter()
        .enumerate()
        .map(|(i, c)| Block {
            start: c.start,
            end: o.chapters.get(i + 1).map_or(o.length, |n| n.start),
            title: c.title.clone(),
            artist: c.artist.clone(),
            row: 0,
        })
        .filter(|b| b.end > b.start)
        .collect()
}

/// Seconds per bar of `track` at `at` in the set (2 when unknown).
fn bar_seconds(t: &SetTimeline, track: usize, at: f64) -> f64 {
    let speed = t
        .clips
        .iter()
        .find(|c| c.track == track && c.start <= at && at < c.end.max(c.start + 1.0))
        .map_or(1.0, |c| c.speed);
    let bpm = t.tracks.get(track).map_or(0.0, |lt| lt.bpm) * speed;
    if bpm > 0.0 {
        4.0 * 60.0 / bpm
    } else {
        2.0
    }
}

/// How transition `i` was mixed again, if it was: bars and style name.
fn mixed_again(edits: &Edits, i: usize) -> Option<(f64, String)> {
    edits.fixes.iter().rev().find_map(|f| match f {
        Fix::Mix {
            transition,
            bars,
            style,
            ..
        } if *transition == i => Some((*bars, style.clone())),
        _ => None,
    })
}

/// Where transition `i`, mixed again, ends in the set.
fn mixed_end(edits: &Edits, i: usize, t: &SetTimeline) -> Option<f64> {
    let (bars, style) = mixed_again(edits, i)?;
    let x = t.transitions.get(i)?;
    if style == Style::Cut.name() {
        return Some(x.start + 0.05);
    }
    Some(x.start + bars * set_render::bar_seconds(t, x.to_track, x.start))
}

/// A new file for transition `i` mixed again, beside the recording.
fn mix_path(audio: &Path, i: usize, stamp: &str) -> PathBuf {
    let edits = set_fixes::edits_path(audio);
    let name = edits.file_name().unwrap_or_default().to_string_lossy();
    let stem = name.strip_suffix(".edits.json").unwrap_or(&name);
    edits.with_file_name(format!("{stem}.transition {} {stamp}.wav", i + 1))
}

fn transition_length(x: &opendj_engine::set_timeline::Transition) -> String {
    match x.bars {
        Some(b) if b >= 0.95 => format!("{} bars", b.round()),
        _ => format!("{:.0} s", x.len().max(0.0)),
    }
}

fn fix_row(f: &Fix) -> SetFixRow {
    let (title, text) = match f {
        Fix::Trim { start, end, .. } => (
            "Trim".to_string(),
            format!("Keep {} to {}", clock(*start), clock(*end)),
        ),
        Fix::TrackLevel {
            start,
            end,
            db,
            title,
            ..
        } => (
            "Track level".to_string(),
            format!(
                "{title}: {}{db} dB, {} to {}",
                if *db > 0.0 { "+" } else { "" },
                clock(*start),
                clock(*end)
            ),
        ),
        Fix::Cut { start, end } => (
            "Cut".to_string(),
            format!("{} to {}", clock(*start), clock(*end)),
        ),
        Fix::Loudness { lufs, ceiling } => (
            "Loudness".to_string(),
            format!("{lufs} LUFS, peaks under {ceiling} dBTP"),
        ),
        Fix::Mix {
            transition,
            bars,
            style,
            ..
        } => (
            format!("Transition {}", transition + 1),
            format!("Mixed again: {bars} bars, {style}"),
        ),
    };
    SetFixRow {
        title: title.into(),
        text: text.into(),
    }
}

fn loudness_text(o: &Open, preset: Option<(f64, f64)>) -> String {
    match (&o.measured, preset) {
        (None, _) if o.measuring => "Measuring the mix…".into(),
        (None, _) => String::new(),
        (Some(r), None) => match r.before_lufs {
            Some(l) => format!("The mix plays at {l:.1} LUFS."),
            None => String::new(),
        },
        (Some(r), Some((lufs, _))) => match r.before_lufs {
            Some(l) => format!(
                "Now {l:.1} LUFS; to {lufs} LUFS is {}{:.1} dB, with {:.1} dB of limiting.",
                if r.gain_db > 0.0 { "+" } else { "" },
                r.gain_db,
                r.limiting_db
            ),
            None => String::new(),
        },
    }
}

/// "Recorded 22:14 · 1:58:03 · 18 tracks · 17 transitions".
fn subtitle(o: &Open) -> String {
    let mut parts = Vec::new();
    if let Some(when) = o
        .timeline
        .as_ref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t.started).ok())
    {
        parts.push(format!(
            "Recorded {}",
            when.with_timezone(&chrono::Local).format("%-d %b %H:%M")
        ));
    }
    parts.push(clock(o.length));
    let tracks = o.chapters.len();
    parts.push(if tracks == 1 {
        "1 track".into()
    } else {
        format!("{tracks} tracks")
    });
    if let Some(t) = &o.timeline {
        parts.push(match t.transitions.len() {
            1 => "1 transition".into(),
            n => format!("{n} transitions"),
        });
    }
    parts.join(" · ")
}

/// The lanes that move, as paths over 1000 by 100.
fn lanes(o: &Open) -> Vec<SetLane> {
    let Some(t) = &o.timeline else {
        return Vec::new();
    };
    let len = o.length;
    t.lanes
        .iter()
        .filter(|l| {
            let first = l.points.first().map_or(0.0, |p| p.1);
            l.points.iter().any(|p| (p.1 - first).abs() > 1e-6)
        })
        .map(|l| {
            let (nominal_lo, nominal_hi) = match l.kind {
                LaneKind::Volume | LaneKind::Filter | LaneKind::FxSuper(_) => (0.0, 1.0),
                LaneKind::FxUnit(_) | LaneKind::FxEffect(..) => (0.0, 1.0),
                LaneKind::Crossfader => (-1.0, 1.0),
                LaneKind::EqLow | LaneKind::EqMid | LaneKind::EqHigh | LaneKind::Gain => (0.0, 2.0),
            };
            let lo = l.points.iter().map(|p| p.1).fold(nominal_lo, f64::min);
            let hi = l.points.iter().map(|p| p.1).fold(nominal_hi, f64::max);
            let y = |v: f64| 96.0 - 92.0 * (v - lo) / (hi - lo).max(1e-9);
            let x = |s: f64| (s / len * 1000.0).clamp(0.0, 1000.0);
            let mut path = String::new();
            let mut last = None;
            for (i, (s, v)) in l.points.iter().enumerate() {
                if i == 0 {
                    path += &format!("M {:.1} {:.1} ", x(*s), y(*v));
                } else {
                    // Holds the value until the next change.
                    path += &format!(
                        "L {:.1} {:.1} L {:.1} {:.1} ",
                        x(*s),
                        last.unwrap_or(y(*v)),
                        x(*s),
                        y(*v)
                    );
                }
                last = Some(y(*v));
            }
            path += &format!("L 1000 {:.1}", last.unwrap_or(96.0));
            let deck = l.deck.map(|d| o.row_of.get(d).copied().unwrap_or(0));
            let who = l
                .deck
                .map(|d| format!("Deck {} ", d + 1))
                .unwrap_or_default();
            let what = match l.kind {
                LaneKind::Volume => "volume".to_string(),
                LaneKind::EqLow => "low".into(),
                LaneKind::EqMid => "mid".into(),
                LaneKind::EqHigh => "high".into(),
                LaneKind::Filter => "filter".into(),
                LaneKind::Gain => "gain".into(),
                LaneKind::Crossfader => "Crossfader".into(),
                LaneKind::FxUnit(u) => format!("FX {}", u + 1),
                LaneKind::FxEffect(u, s) => format!("FX {} slot {}", u + 1, s + 1),
                LaneKind::FxSuper(u) => format!("FX {} knob", u + 1),
            };
            SetLane {
                deck: deck.unwrap_or(-1),
                name: format!("{who}{what}").into(),
                commands: path.into(),
            }
        })
        .collect()
}

/// The ruler's labels and where they sit (fractions of the set), spaced
/// for a view `view` of the set wide.
fn ticks(length: f64, view: f64) -> (Vec<String>, Vec<f32>) {
    let seen = (length * view).max(1.0);
    let step = TICK_STEPS
        .iter()
        .copied()
        .find(|s| seen / s <= TICKS_IN_VIEW)
        .unwrap_or(3600.0);
    let mut labels = Vec::new();
    let mut at = Vec::new();
    let mut s = 0.0;
    while s < length {
        labels.push(clock(s));
        at.push((s / length) as f32);
        s += step;
    }
    (labels, at)
}

/// Stretches for the level check from a cue sheet's start times.
fn cue_stretches(cue: &[CueTrack], length: f64) -> Vec<Stretch> {
    cue.iter()
        .enumerate()
        .map(|(i, c)| Stretch {
            start: c.start,
            end: cue.get(i + 1).map_or(length, |n| n.start),
            title: c.title.clone(),
        })
        .filter(|s| s.end > s.start)
        .collect()
}

/// The recording's frames as they decode, on a thread of their own, and
/// its sample rate. Decoding stops once `cancel` is set.
fn frames(
    audio: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(u32, impl Iterator<Item = [f32; 2]>), String> {
    let decoder = Decoder::open(audio)?;
    let rate = decoder.audio().sample_rate;
    let (tx, rx) = sync_channel::<Vec<[f32; 2]>>(16);
    spawn("set decode", move || {
        let _ = decoder.stream(|_, samples| {
            let (pairs, _) = samples.as_chunks::<2>();
            tx.send(pairs.to_vec()).is_ok() && !cancel.load(Ordering::Acquire)
        });
    });
    Ok((rate, rx.into_iter().flatten()))
}

/// Reads the set once for its waveform and the audio checks.
fn read_set(audio: PathBuf, stretches: Vec<Stretch>, tell: Sender<Said>, cancel: Arc<AtomicBool>) {
    spawn("set read", move || {
        let (rate, frames) = match frames(&audio, cancel.clone()) {
            Ok(f) => f,
            Err(e) => {
                let _ = tell.send(Said::Failed(format!("Couldn't read the recording: {e}")));
                return;
            }
        };
        let mut maker = WaveformMaker::new(rate, WAVE_RATE);
        let counted = frames.inspect(|f| maker.push(*f));
        let findings = set_checks::check_audio(counted, rate as f64, &stretches);
        if cancel.load(Ordering::Acquire) {
            return;
        }
        let _ = tell.send(Said::Read {
            waveform: maker.finish(),
            findings,
        });
    });
}

/// After Save: `<name> (edited).cue` and `.tracklist.txt` beside the
/// edited mix, so Export for Mixcloud finds its tracklist.
fn write_edited_lists(dest: &Path, title: &str, chapters: &[Chapter]) -> Result<(), String> {
    if chapters.is_empty() {
        return Ok(());
    }
    let stem = dest.file_stem().unwrap_or_default().to_string_lossy();
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    let cue = with_name(dest, &format!("{stem}.cue"));
    std::fs::write(&cue, set_chapters::cue_sheet(chapters, title, &name))
        .map_err(|e| format!("{}: {e}", cue.display()))?;
    let list = with_name(dest, &format!("{stem}.tracklist.txt"));
    std::fs::write(&list, set_chapters::tracklist(chapters))
        .map_err(|e| format!("{}: {e}", list.display()))
}

/// When a recording started, in Unix seconds: from its control log's
/// header, else from its name ("openDJ 2026-11-15 22.14", local time).
/// None for files that aren't openDJ recordings.
fn recording_started(audio: &Path) -> Option<i64> {
    use std::io::BufRead;
    let (log, _) = opendj_engine::set_timeline::log_paths(audio);
    if let Ok(f) = std::fs::File::open(&log) {
        let mut line = String::new();
        if std::io::BufReader::new(f).read_line(&mut line).is_ok() {
            let header: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
            if let Some(t) = header["started"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            {
                return Some(t.timestamp());
            }
        }
    }
    let stem = recording_stem(audio);
    let when = stem.strip_prefix("openDJ ")?.get(..16)?;
    let naive = chrono::NaiveDateTime::parse_from_str(when, "%Y-%m-%d %H.%M").ok()?;
    naive
        .and_local_timezone(chrono::Local)
        .earliest()
        .map(|t| t.timestamp())
}

/// The recording's name without its extension or " (part N)".
fn recording_stem(audio: &Path) -> String {
    let (log, _) = opendj_engine::set_timeline::log_paths(audio);
    let name = log.file_name().unwrap_or_default().to_string_lossy();
    name.strip_suffix(".controls.jsonl")
        .unwrap_or(&name)
        .to_string()
}

fn with_name(beside: &Path, name: &str) -> PathBuf {
    beside.with_file_name(name)
}

fn short_date(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%-d %b").to_string())
        .unwrap_or_default()
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) {
    if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(f) {
        eprintln!("could not start {name}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_spacing_follows_the_view() {
        let (labels, at) = ticks(3600.0, 1.0);
        assert_eq!(labels.first().map(String::as_str), Some("0:00"));
        assert_eq!(labels.len(), 6, "{labels:?}");
        assert!((at[1] - 600.0 / 3600.0).abs() < 1e-6);
        let (zoomed, _) = ticks(3600.0, 0.01);
        assert_eq!(zoomed[1], "0:05");
    }

    #[test]
    fn cue_tracks_become_stretches() {
        let cue = vec![
            CueTrack {
                title: "One".into(),
                performer: String::new(),
                start: 0.0,
            },
            CueTrack {
                title: "Two".into(),
                performer: String::new(),
                start: 100.0,
            },
        ];
        let s = cue_stretches(&cue, 250.0);
        assert_eq!((s[0].end, s[1].start, s[1].end), (100.0, 100.0, 250.0));
    }

    #[test]
    fn recordings_are_dated_by_their_log_or_name() {
        let dir = std::env::temp_dir().join(format!("opendj-set-started-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let logged = dir.join("openDJ 2026-11-15 22.14.wav");
        std::fs::write(
            dir.join("openDJ 2026-11-15 22.14.controls.jsonl"),
            "{\"opendj_recording\":1,\"started\":\"2026-11-16T06:14:00+00:00\"}\n",
        )
        .unwrap();
        assert_eq!(recording_started(&logged), Some(1_794_809_640));
        let named = dir.join("openDJ 2026-11-14 21.00.flac");
        assert!(recording_started(&named).is_some());
        assert_eq!(recording_started(&dir.join("Some Mix.mp3")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
