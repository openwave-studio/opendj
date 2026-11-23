//! The Background panel: what background work is doing (analysis, HQ
//! stems, word timing, Collection Health's sound and cover scans), shown as
//! the footer's ring and the panel it opens, with Pause, Stop and Do next;
//! and the track list's Stems column.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use opendj_engine::background::{self, Task, WhilePlaying};
use opendj_engine::{stems, Source, Track};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{columns, Browser};
use crate::{settings, AppWindow, TaskRow, TaskWaiting, TasksUi};

/// How often the panel is worked out again.
const EVERY: Duration = Duration::from_millis(250);
/// How long the ring shows its check after the last task finishes.
const DONE_FOR: Duration = Duration::from_secs(4);
/// Waiting tracks listed under a task, for Do next.
const NEXT_SHOWN: usize = 3;

/// Settings keys.
const PAUSED_KEY: &str = "background_paused";
const WHILE_PLAYING_KEY: &str = "background_while_playing";

/// What the panel showed last, and the HQ stems batch it is following.
#[derive(Default)]
pub(super) struct Shown {
    at: Option<Instant>,
    done_until: Option<Instant>,
    had_tasks: bool,
    rows: Vec<TaskRow>,
    stems: StemsBatch,
    /// The separations as last put in the Stems column.
    stems_cells: HashMap<PathBuf, SharedString>,
    /// Whether a file has HQ stems on disk, by track id, for the Stems
    /// column (cleared as separations finish).
    ready: RefCell<HashMap<usize, bool>>,
}

/// The separations asked for since the queue was last empty.
#[derive(Default)]
struct StemsBatch {
    paths: HashSet<PathBuf>,
    /// The one running and when it started.
    running: Option<(PathBuf, Instant)>,
    /// How long a separation took, on average, this batch.
    took: Option<Duration>,
}

/// "6 min left", or empty without a guess.
fn time_left(d: Option<Duration>) -> String {
    let Some(d) = d else {
        return String::new();
    };
    let mins = d.as_secs().div_ceil(60);
    match mins {
        0 | 1 => "1 min left".into(),
        m if m < 60 => format!("{m} min left"),
        m => format!("{} h {} min left", m / 60, m % 60),
    }
}

/// "Title · Artist".
fn label(t: &Track) -> String {
    if t.artist.trim().is_empty() {
        t.title.clone()
    } else {
        format!("{} · {}", t.title, t.artist)
    }
}

fn next_model(next: Vec<TaskWaiting>) -> ModelRc<TaskWaiting> {
    ModelRc::new(VecModel::from(next))
}

/// Pauses or resumes `tasks`, keeping the choice for the next start.
fn set_paused(tasks: &[Task], on: bool) {
    for &t in tasks {
        background::set_paused(t, on);
    }
    settings::set(PAUSED_KEY, (background::paused_bits() as u64).into());
}

impl Browser {
    pub(super) fn install_tasks(
        app: &AppWindow,
        with: &(impl Fn(&dyn Fn(&mut Browser)) + Clone + 'static),
    ) {
        // Pauses and the choice for while a deck plays carry over a restart.
        background::set_paused_bits(settings::number(PAUSED_KEY, 0.0) as u8);
        background::set_while_playing(WhilePlaying::from_index(settings::number(
            WHILE_PLAYING_KEY,
            0.0,
        ) as u8));
        let ui = app.global::<TasksUi>();
        ui.set_while_playing(background::while_playing() as i32);
        let w = with.clone();
        ui.on_pause(move |kind, on| {
            if let Some(t) = Task::from_index(kind as usize) {
                set_paused(&[t], on);
            }
            w(&|b| b.tasks_shown.at = None);
        });
        let w = with.clone();
        ui.on_pause_all(move |on| {
            set_paused(&Task::ALL, on);
            w(&|b| b.tasks_shown.at = None);
        });
        let w = with.clone();
        ui.on_stop(move |kind| {
            w(&|b| {
                match Task::from_index(kind as usize) {
                    Some(Task::Analyze) => {
                        b.analysis.cancel();
                        b.show_analysis();
                    }
                    Some(Task::Stems) => stems::stop_separations(),
                    _ => {}
                }
                b.tasks_shown.at = None;
            })
        });
        let w = with.clone();
        ui.on_do_next(move |kind, id| {
            w(&|b| {
                let id = id as usize;
                match Task::from_index(kind as usize) {
                    Some(Task::Analyze) => b.analysis.do_next(id),
                    Some(Task::Stems) => {
                        if let Some(Source::File(path)) = b
                            .index_of
                            .get(&id)
                            .and_then(|&i| b.library.get(i))
                            .map(|t| &t.source)
                        {
                            stems::separate_next(path);
                        }
                    }
                    _ => {}
                }
                b.tasks_shown.at = None;
            })
        });
        let app_weak = app.as_weak();
        ui.on_set_while_playing(move |choice| {
            let w = WhilePlaying::from_index(choice.clamp(0, 2) as u8);
            background::set_while_playing(w);
            settings::set(WHILE_PLAYING_KEY, (w as u64).into());
            if let Some(app) = app_weak.upgrade() {
                app.global::<TasksUi>().set_while_playing(w as i32);
            }
        });
    }

    /// Works out the panel again, a few times a second: called every frame.
    pub fn poll_tasks(&mut self) {
        let now = Instant::now();
        if self.tasks_shown.at.is_some_and(|at| now - at < EVERY) {
            return;
        }
        self.tasks_shown.at = Some(now);
        let rows = self.task_rows(now);
        self.show_stem_cells();
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let ui = app.global::<TasksUi>();
        let shown = &mut self.tasks_shown;
        if shown.had_tasks && rows.is_empty() {
            shown.done_until = Some(now + DONE_FOR);
        }
        shown.had_tasks = !rows.is_empty();
        let just_done = shown.done_until.is_some_and(|t| now < t);
        if ui.get_just_done() != just_done {
            ui.set_just_done(just_done);
        }
        let overall = if rows.is_empty() {
            1.0
        } else {
            rows.iter().map(|r| r.progress).sum::<f32>() / rows.len() as f32
        };
        let all_paused = !rows.is_empty() && rows.iter().all(|r| r.paused);
        ui.set_overall(overall);
        ui.set_all_paused(all_paused);
        if rows != shown.rows {
            ui.set_tasks(ModelRc::new(VecModel::from(rows.clone())));
            shown.rows = rows;
        }
    }

    /// A row for each kind of task with work to do.
    fn task_rows(&mut self, now: Instant) -> Vec<TaskRow> {
        let mut rows = Vec::new();
        let track =
            |b: &Browser, id: usize| b.index_of.get(&id).and_then(|&i| b.library.get(i)).cloned();
        // Analysis.
        if let Some((n, total)) = self.analysis.progress() {
            let running = self.analysis.current();
            let next = self.analysis.next_up(NEXT_SHOWN);
            let waiting = self.analysis.len() - usize::from(running.is_some());
            rows.push(TaskRow {
                kind: Task::Analyze as i32,
                name: "Analyze".into(),
                count: format!("{n} of {total}").into(),
                progress: (n.saturating_sub(1)) as f32 / total.max(1) as f32,
                left: time_left(self.analysis.time_left()).into(),
                now: running
                    .and_then(|id| track(self, id))
                    .map(|t| label(&t))
                    .unwrap_or_default()
                    .into(),
                paused: background::held(Task::Analyze),
                can_stop: true,
                more: waiting.saturating_sub(next.len()) as i32,
                next: next_model(
                    next.iter()
                        .map(|t| TaskWaiting {
                            id: t.id as i32,
                            title: label(t).into(),
                        })
                        .collect(),
                ),
            });
        }
        // HQ stems.
        if let Some(row) = self.stems_row(now) {
            rows.push(row);
        }
        // Word timing.
        if let Some((id, stage, progress)) = self.lyrics_task() {
            rows.push(TaskRow {
                kind: Task::Lyrics as i32,
                name: "Lyrics".into(),
                count: stage.into(),
                progress,
                left: SharedString::new(),
                now: track(self, id)
                    .map(|t| label(&t))
                    .unwrap_or_default()
                    .into(),
                paused: background::held(Task::Lyrics),
                can_stop: false,
                next: Default::default(),
                more: 0,
            });
        }
        // Collection Health's scans.
        for (task, name, progress) in [
            (Task::SoundCheck, "Sound check", self.sound_scan.progress()),
            (Task::Covers, "Covers", self.cover_scan.progress()),
        ] {
            if let Some((done, total)) = progress {
                rows.push(TaskRow {
                    kind: task as i32,
                    name: name.into(),
                    count: format!("{} of {total}", (done + 1).min(total)).into(),
                    progress: done as f32 / total.max(1) as f32,
                    left: SharedString::new(),
                    now: SharedString::new(),
                    paused: background::held(task),
                    can_stop: false,
                    next: Default::default(),
                    more: 0,
                });
            }
        }
        rows
    }

    /// The HQ stems row: the batch asked for since the queue was last
    /// empty, the one separating and how far, and the files waiting.
    fn stems_row(&mut self, now: Instant) -> Option<TaskRow> {
        let jobs = stems::separations();
        let batch = &mut self.tasks_shown.stems;
        let active: Vec<&PathBuf> = jobs
            .iter()
            .filter(|(_, s)| matches!(s, stems::Separation::Waiting | stems::Separation::Running))
            .map(|(p, _)| p)
            .collect();
        if active.is_empty() {
            *batch = StemsBatch::default();
            return None;
        }
        batch.paths.extend(active.iter().map(|p| (*p).clone()));
        // Asked for and since stopped: out of the batch.
        batch.paths.retain(|p| jobs.iter().any(|(q, _)| q == p));
        let finished = batch
            .paths
            .iter()
            .filter(|p| {
                jobs.iter().any(|(q, s)| {
                    q == *p && matches!(s, stems::Separation::Done | stems::Separation::Failed(_))
                })
            })
            .count();
        let total = batch.paths.len();
        let running = stems::separating();
        let fraction = running.as_ref().map_or(0.0, |(_, f)| *f);
        // Time each separation, for the time left.
        match (&running, &batch.running) {
            (Some((p, _)), Some((q, _))) if p == q => {}
            (r, before) => {
                if let Some((_, started)) = before {
                    let took = now - *started;
                    batch.took = Some(batch.took.map_or(took, |t| (t * 3 + took) / 4));
                }
                batch.running = r.as_ref().map(|(p, _)| (p.clone(), now));
            }
        }
        let waiting = stems::separations_waiting();
        let current_left = batch.running.as_ref().and_then(|(_, started)| {
            (fraction > 0.05).then(|| (now - *started).mul_f32((1.0 - fraction) / fraction))
        });
        let each = batch
            .took
            .or_else(|| current_left.map(|l| l.mul_f32(1.0 / (1.0 - fraction).max(0.05))));
        let left = match (current_left, each) {
            (Some(c), Some(e)) => Some(c + e * waiting.len() as u32),
            (None, Some(e)) => Some(e * (waiting.len() + usize::from(running.is_some())) as u32),
            _ => None,
        };
        let by_path = |p: &PathBuf| {
            self.library
                .iter()
                .find(|t| matches!(&t.source, Source::File(f) if f == p))
                .cloned()
        };
        let name_of = |p: &PathBuf| {
            by_path(p).map_or_else(
                || {
                    p.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                },
                |t| label(&t),
            )
        };
        let next: Vec<TaskWaiting> = waiting
            .iter()
            .take(NEXT_SHOWN)
            .filter_map(|p| {
                by_path(p).map(|t| TaskWaiting {
                    id: t.id as i32,
                    title: label(&t).into(),
                })
            })
            .collect();
        Some(TaskRow {
            kind: Task::Stems as i32,
            name: "HQ Stems".into(),
            count: format!("{} of {total}", (finished + 1).min(total)).into(),
            progress: (finished as f32 + fraction) / total.max(1) as f32,
            left: time_left(left).into(),
            now: running
                .as_ref()
                .map(|(p, _)| name_of(p))
                .unwrap_or_default()
                .into(),
            paused: background::held(Task::Stems),
            can_stop: true,
            more: waiting.len().saturating_sub(next.len()) as i32,
            next: next_model(next),
        })
    }

    /// What a track's Stems cell says: Ready, the percent separated,
    /// Waiting, Paused or Failed; empty when no HQ stems were asked for.
    pub(super) fn stems_cell(&self, t: &Track) -> SharedString {
        let Source::File(path) = &t.source else {
            return SharedString::new();
        };
        let paused = background::held(Task::Stems);
        match stems::separation(path) {
            Some(stems::Separation::Waiting) if paused => "Paused".into(),
            Some(stems::Separation::Waiting) => "Waiting".into(),
            Some(stems::Separation::Running) if paused => "Paused".into(),
            Some(stems::Separation::Running) => {
                let f = stems::separating()
                    .filter(|(p, _)| p == path)
                    .map_or(0.0, |(_, f)| f);
                format!("{:.0}%", f * 100.0).into()
            }
            Some(stems::Separation::Failed(_)) => "Failed".into(),
            Some(stems::Separation::Done) => "Ready".into(),
            None => {
                let ready = *self
                    .tasks_shown
                    .ready
                    .borrow_mut()
                    .entry(t.id)
                    .or_insert_with(|| stems::find(path).is_some());
                if ready {
                    "Ready".into()
                } else {
                    SharedString::new()
                }
            }
        }
    }

    /// Shows the separations' progress in the Stems column, touching only
    /// the rows of tracks whose cell changed.
    fn show_stem_cells(&mut self) {
        if !self.columns.ids().contains(&columns::STEMS) {
            return;
        }
        let mut cells = HashMap::new();
        let mut changed = HashSet::new();
        for (path, _) in stems::separations() {
            let Some(i) = self
                .library
                .iter()
                .position(|t| matches!(&t.source, Source::File(f) if *f == path))
            else {
                continue;
            };
            let cell = self.stems_cell(&self.library[i]);
            if self.tasks_shown.stems_cells.get(&path) != Some(&cell) {
                changed.insert(i);
            }
            cells.insert(path, cell);
        }
        if changed.is_empty() {
            return;
        }
        // A separation finished: look at the disk again for its track.
        for &i in &changed {
            self.tasks_shown
                .ready
                .borrow_mut()
                .remove(&self.library[i].id);
        }
        self.tasks_shown.stems_cells = cells;
        self.refresh_track_rows(&changed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_left_rounds_up_to_minutes() {
        assert_eq!(time_left(None), "");
        assert_eq!(time_left(Some(Duration::from_secs(20))), "1 min left");
        assert_eq!(time_left(Some(Duration::from_secs(61))), "2 min left");
        assert_eq!(
            time_left(Some(Duration::from_secs(3 * 3600 + 600))),
            "3 h 10 min left"
        );
    }
}
