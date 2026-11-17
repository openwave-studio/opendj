//! Tracks played in this History session: dimmed with a lilac check in the
//! track list, a short "Played 23:41" note on a deck that loads one, Mark
//! as Not Played, and Start new session over History's Sessions list.
//!
//! A play counts once a track has been heard in the master for
//! [`super::PLAYED_AFTER`] seconds: its channel fader up and the crossfader
//! open on its side. A deck heard only in the headphones never counts.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::TimeZone;
use opendj_control::{ControlHandle, ControlRegistry};
use opendj_engine::{deck_group, NUM_DECKS};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{history, Browser};
use crate::{broadcast, wall_clock, DeckActions, LibraryEdit};

/// How long a deck shows "Played 23:41" after loading a played track.
const NOTE_FOR: Duration = Duration::from_secs(3);

/// How often to look whether the session ran out (an hour with nothing
/// played, or 6:00), in seconds of [`Browser::keep`] time.
const EXPIRY_CHECK: f64 = 10.0;

pub struct Played {
    /// Each deck's channel fader and crossfader side.
    faders: [(ControlHandle, ControlHandle); NUM_DECKS],
    crossfader: Option<ControlHandle>,
    crossfader_on: Option<ControlHandle>,
    /// A hardware mixer does the mixing: openDJ can't tell what is heard,
    /// so a playing deck counts.
    external: Option<ControlHandle>,
    /// When each track was first played in this session (Unix seconds),
    /// by track id.
    pub at: HashMap<usize, i64>,
    /// Until when each deck shows its note, and for which track.
    notes: [Option<(Instant, usize)>; NUM_DECKS],
    since_check: f64,
    twenty_four: bool,
}

impl Played {
    pub fn new(registry: &ControlRegistry) -> Self {
        Self {
            faders: std::array::from_fn(|d| {
                let g = deck_group(d);
                (
                    registry.get(&g, "volume").expect("volume control"),
                    registry
                        .get(&g, "orientation")
                        .expect("orientation control"),
                )
            }),
            crossfader: registry.get("[Master]", "crossfader"),
            crossfader_on: registry.get("[Mixer]", "crossfader_enabled"),
            external: registry.get("[Master]", "external_mixer"),
            at: HashMap::new(),
            notes: [None; NUM_DECKS],
            since_check: 0.0,
            twenty_four: wall_clock::system_uses_24_hour(),
        }
    }

    /// Whether a deck is heard in the master mix (playing is checked by
    /// the caller).
    pub fn heard(&self, deck: usize) -> bool {
        if self.external.as_ref().is_some_and(|e| e.get_bool()) {
            return true;
        }
        let xfader = if self.crossfader_on.as_ref().is_none_or(|c| c.get() > 0.5) {
            self.crossfader.as_ref().map_or(0.0, |c| c.get())
        } else {
            0.0
        };
        let (volume, orientation) = &self.faders[deck];
        broadcast::heard(broadcast::master_level(
            volume.get(),
            orientation.get(),
            xfader,
        ))
    }

    /// When a track was played in this session, as the list shows it
    /// ("23:41" or "11:41 PM"); empty when it wasn't.
    pub fn time_of(&self, id: usize) -> String {
        self.at
            .get(&id)
            .map(|&at| clock_time(at, self.twenty_four))
            .unwrap_or_default()
    }
}

/// A Unix time as the local time of day.
fn clock_time(at: i64, twenty_four: bool) -> String {
    chrono::Local
        .timestamp_opt(at, 0)
        .single()
        .map(|t| wall_clock::format(t.time(), twenty_four))
        .unwrap_or_default()
}

impl Browser {
    pub(super) fn install_played(
        edit: &LibraryEdit,
        with: &(impl Fn(&dyn Fn(&mut Browser)) + Clone + 'static),
    ) {
        let w = with.clone();
        edit.on_start_new_session(move || w(&|b| b.start_new_session()));
        let w = with.clone();
        edit.on_mark_not_played(move |first, last| {
            w(&|b| b.mark_not_played(first.max(0) as usize, last.max(0) as usize))
        });
        let w = with.clone();
        edit.on_any_played(move |first, last| {
            let any = std::cell::Cell::new(false);
            w(&|b| {
                let (first, last) = (first.max(0) as usize, last.max(0) as usize);
                any.set((first..=last.max(first)).any(|r| {
                    b.rows
                        .get(r)
                        .copied()
                        .flatten()
                        .is_some_and(|i| b.played.at.contains_key(&b.library[i].id))
                }));
            });
            any.get()
        });
    }

    /// Carries on in the last History session when the app starts within
    /// the hour (and before 6:00), so its played marks survive a restart.
    pub(super) fn resume_session(&mut self) {
        let Some(db) = &self.db else { return };
        let Ok(Some((session, last))) = db.last_session() else {
            return;
        };
        if history::new_set(last, history::now()) {
            return;
        }
        self.session = Some(session);
        self.last_play = Some(last);
        self.played.at = db
            .session_plays(session)
            .unwrap_or_default()
            .into_iter()
            .collect();
    }

    /// Notes a play in this session's marks.
    pub(super) fn mark_played(&mut self, id: usize, at: i64) {
        self.played.at.entry(id).or_insert(at);
    }

    /// Ends this session: the marks clear and the next track played starts
    /// a new one.
    pub(super) fn start_new_session(&mut self) {
        if let (Some(db), Some(s)) = (&self.db, self.session) {
            if let Err(e) = db.end_session(s) {
                eprintln!("can't end the session: {e}");
            }
        }
        self.session = None;
        self.last_play = None;
        self.played.at.clear();
        self.played.notes = [None; NUM_DECKS];
        self.show_notes();
        self.refresh_rows_keeping_current();
    }

    /// Takes the tracks in rows `first..=last` out of this session.
    fn mark_not_played(&mut self, first: usize, last: usize) {
        let ids: Vec<usize> = (first..=last.max(first))
            .filter_map(|r| self.rows.get(r).copied().flatten())
            .map(|i| self.library[i].id)
            .filter(|id| self.played.at.contains_key(id))
            .collect();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.played.at.remove(id);
            if let (Some(db), Some(s)) = (&self.db, self.session) {
                if let Err(e) = db.unrecord_play(s, *id) {
                    eprintln!("can't mark the track as not played: {e}");
                }
            }
        }
        for n in &mut self.played.notes {
            if n.is_some_and(|(_, id)| ids.contains(&id)) {
                *n = None;
            }
        }
        self.show_notes();
        self.refresh_rows_keeping_current();
    }

    /// A track arrived on `deck`: a short note if it was played already.
    pub(super) fn note_load(&mut self, deck: usize, id: usize) {
        self.played.notes[deck] = self
            .played
            .at
            .contains_key(&id)
            .then(|| (Instant::now() + NOTE_FOR, id));
        self.show_notes();
    }

    /// Takes down notes whose time is up, and ends the session once it ran
    /// out. Called from [`Browser::keep`].
    pub(super) fn keep_played(&mut self, dt: f64) {
        let now = Instant::now();
        if self
            .played
            .notes
            .iter()
            .any(|n| n.is_some_and(|(t, _)| t <= now))
        {
            for n in &mut self.played.notes {
                if n.is_some_and(|(t, _)| t <= now) {
                    *n = None;
                }
            }
            self.show_notes();
        }
        self.played.since_check += dt;
        if self.played.since_check >= EXPIRY_CHECK {
            self.played.since_check = 0.0;
            if self
                .last_play
                .is_some_and(|last| history::new_set(last, history::now()))
                && !self.played.at.is_empty()
            {
                self.start_new_session();
            }
        }
    }

    fn show_notes(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let notes: Vec<SharedString> = (0..NUM_DECKS)
            .map(
                |d| match self.played.notes[d].and_then(|(_, id)| self.played.at.get(&id)) {
                    Some(&at) => {
                        format!("Played {}", clock_time(at, self.played.twenty_four)).into()
                    }
                    None => SharedString::new(),
                },
            )
            .collect();
        app.global::<DeckActions>()
            .set_played_notes(ModelRc::new(VecModel::from(notes)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_times_follow_the_hour_setting() {
        let at = chrono::Local
            .with_ymd_and_hms(2026, 11, 15, 23, 41, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(clock_time(at, true), "23:41");
        assert_eq!(clock_time(at, false), "11:41 PM");
    }
}
