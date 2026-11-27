//! The scrolling waveform of a deck playing a live source (an input, an
//! instrument plugin or the Groove Box): the deck's sound as it comes in,
//! drawn up to the play line, on the leading deck's beat grid.

use std::collections::VecDeque;
use std::time::Instant;

use opendj_engine::live_wave::LiveSlice;
use opendj_engine::track::{Track, Waveform, WaveformBin, WAVEFORM_RATE};
use opendj_engine::NUM_DECKS;
use opendj_waveform::{
    GridSection, Lanes, Mark, MarkKind, PhraseKind, GRID_SECTIONS, MAX_MARKS, MAX_PHRASES,
    NOTE_ROWS, SCOPE_POINTS,
};

use crate::smooth::SmoothClock;

/// Slices drawn: about 8 s, the half of the waveform behind the play line
/// with room to spare. A power of two, so each coarser level halves it.
pub const WINDOW: usize = 8192;

/// The bands carry less energy than the whole signal; lifted as a track's
/// waveform lifts them (see the analysis), so the colours match.
const BAND_GAIN: [f32; 4] = [1.0, 1.4, 2.0, 1.0];

/// A slice as a waveform bin: the square root of each level against full
/// scale, from 0 to 255, as a track's bins are against its loudest peak.
pub fn bin(s: &LiveSlice) -> WaveformBin {
    let q = |v: f32, k: usize| ((v * BAND_GAIN[k]).clamp(0.0, 1.0).sqrt() * 255.0).round() as u8;
    std::array::from_fn(|i| {
        if i < 4 {
            q(s.peak[i], i)
        } else {
            q(s.rms[i - 4], i - 4)
        }
    })
}

/// Notes played out past the Notes view's rows for this long (about two
/// bars at 120 BPM) move its window an octave that way.
const SLIDE_AFTER: f64 = 4.0;
/// Notes out there stop counting once none has landed for this long.
const OUT_FORGET: f64 = 2.0;
/// The window glides an octave in this long, never jumping.
const SLIDE_SECONDS: f64 = 0.3;
/// Held notes glow in Electric Lilac.
const LILAC: [f32; 4] = [0.714, 0.612, 1.0, 1.0];
/// Samples the scope spans: about 21 ms at 48 kHz, a few cycles of a note.
pub const SCOPE_SPAN: usize = 1024;

/// One deck's sound so far.
#[derive(Default)]
struct Trace {
    /// The latest bins, oldest first; at most a window and a little more.
    bins: VecDeque<WaveformBin>,
    /// Slices received since the deck went live.
    received: u64,
    clock: SmoothClock,
    notes: NoteRoll,
}

/// One note on the roll, in seconds on the deck's live clock.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Note {
    note: u8,
    start: f64,
    /// None while it is held.
    end: Option<f64>,
}

/// The notes an instrument deck plays, as they start and end, for its
/// Notes view: two octaves of fixed rows (so they never rescale), from a C,
/// that move an octave at a time when the playing stays out past them.
#[derive(Default)]
struct NoteRoll {
    held: u128,
    notes: VecDeque<Note>,
    /// The rows' lowest note, a C, once a note has played.
    bottom: Option<i32>,
    /// Where the rows were drawn from when they last moved, and when, as
    /// they glide to `bottom`.
    slide: Option<(f64, f64)>,
    /// Notes landing past the rows: since when, which way (1 up, -1
    /// down), and the latest.
    out: Option<(f64, i32, f64)>,
}

/// The rows a first note starts: from the C at or below the F# under it,
/// so the note sits in the upper octave or high in the lower one, clear
/// of the edges.
fn first_bottom(note: u8) -> i32 {
    (12 * (note as i32 - 6).div_euclid(12)).clamp(0, 96)
}

impl NoteRoll {
    /// Notes now held (bit `n` for MIDI note `n`) at `at` on the deck's clock.
    fn update(&mut self, held: u128, at: f64) {
        let released = self.held & !held;
        let pressed = held & !self.held;
        self.held = held;
        for n in self.notes.iter_mut().filter(|n| n.end.is_none()) {
            if released >> n.note & 1 == 1 {
                n.end = Some(at);
            }
        }
        for note in (0..128u8).filter(|&n| pressed >> n & 1 == 1) {
            self.notes.push_back(Note {
                note,
                start: at,
                end: None,
            });
        }
        self.follow(pressed, at);
        // Notes let go before the window are gone from it.
        let keep = at - WINDOW as f64 / WAVEFORM_RATE;
        while self
            .notes
            .front()
            .is_some_and(|n| n.end.is_some_and(|e| e < keep))
        {
            self.notes.pop_front();
        }
    }

    /// Moves the rows when the playing stays out past them: notes
    /// `pressed` at `at`, with those still held.
    fn follow(&mut self, pressed: u128, at: f64) {
        if self.bottom.is_none() {
            let Some(first) = (0..128u8).find(|&n| pressed >> n & 1 == 1) else {
                return;
            };
            self.bottom = Some(first_bottom(first));
        }
        let bottom = self.bottom.unwrap_or(0);
        let side = |n: u8| {
            let n = n as i32;
            if n < bottom {
                -1
            } else if n >= bottom + NOTE_ROWS as i32 {
                1
            } else {
                0
            }
        };
        let bits = |m: u128| (0..128u8).filter(move |&n| m >> n & 1 == 1);
        let landed_in = bits(pressed).any(|n| side(n) == 0);
        // Out there: a note just played or still held past the rows (the
        // last one if some went each way).
        let out = bits(self.held).map(side).filter(|&s| s != 0).last();
        match (out, self.out) {
            (Some(dir), Some((since, d, _))) if d == dir => self.out = Some((since, d, at)),
            (Some(dir), _) => self.out = Some((at, dir, at)),
            // Back inside the rows: they stay.
            (None, _) if landed_in => self.out = None,
            (None, Some((_, _, last))) if at - last > OUT_FORGET => self.out = None,
            (None, _) => {}
        }
        if let Some((since, dir, _)) = self.out {
            if at - since >= SLIDE_AFTER {
                self.slide = Some((self.base(at), at));
                self.bottom = Some((bottom + 12 * dir).clamp(0, 96));
                self.out = None;
            }
        }
    }

    /// The rows' lowest note as drawn at `at`: between whole notes while
    /// gliding an octave.
    fn base(&self, at: f64) -> f64 {
        let to = self.bottom.unwrap_or(48) as f64;
        match self.slide {
            Some((from, t0)) => {
                let u = ((at - t0) / SLIDE_SECONDS).clamp(0.0, 1.0);
                from + (to - from) * u * u * (3.0 - 2.0 * u)
            }
            None => to,
        }
    }

    /// The Notes view over a window that starts at `start` on the deck's
    /// clock, with the play line at `at`; none until a note has played.
    fn lanes(&self, start: f64, at: f64, color: [f32; 4]) -> Option<Lanes> {
        self.bottom?;
        let mut marks: Vec<Mark> = self
            .notes
            .iter()
            .filter(|n| n.end.unwrap_or(at) >= start)
            .map(|n| Mark {
                start: n.start - start,
                end: n.end.unwrap_or(at) - start,
                lane: n.note as u32,
                kind: if n.end.is_some() {
                    MarkKind::Played
                } else {
                    MarkKind::Held
                },
            })
            .collect();
        // The latest, if there are more than the shader draws.
        if marks.len() > MAX_MARKS {
            marks.drain(..marks.len() - MAX_MARKS);
        }
        Some(Lanes {
            count: NOTE_ROWS,
            tinted: 0,
            marks,
            trace: 0.0,
            color,
            lit: LILAC,
            notes: Some(self.base(at) as f32),
        })
    }
}

/// The scope a live instrument deck draws right of its play line, from the
/// latest samples `wave` (oldest first): the last [`SCOPE_SPAN`] samples up
/// to the latest rising zero crossing, so a steady note stands still, in
/// [`SCOPE_POINTS`] points, each the sample furthest from zero in its
/// stretch (keeping its sign), so a high note's peaks stay. None until
/// there are enough samples.
pub fn scope_points(wave: &[f32]) -> Option<[f32; SCOPE_POINTS]> {
    if wave.len() < SCOPE_SPAN {
        return None;
    }
    // The latest rising zero crossing with a whole span before it, else
    // the newest samples.
    let end = (SCOPE_SPAN..wave.len())
        .rev()
        .find(|&i| wave[i - 1] < 0.0 && wave[i] >= 0.0)
        .unwrap_or(wave.len());
    let span = &wave[end - SCOPE_SPAN..end];
    let per = SCOPE_SPAN / SCOPE_POINTS;
    Some(std::array::from_fn(|i| {
        span[i * per..(i + 1) * per]
            .iter()
            .copied()
            .fold(0.0f32, |a, v| if v.abs() > a.abs() { v } else { a })
    }))
}

/// What a live deck's scrolling waveform draws this frame.
pub struct Window {
    /// The waveform: [`WINDOW`] slices ending at the play line, and the
    /// coarser levels made from them.
    pub levels: Vec<Vec<WaveformBin>>,
    /// The play line, in seconds into the window.
    pub play_seconds: f64,
    /// The play line on the deck's live clock: seconds since it went live.
    pub clock: f64,
    /// What lies over the waveform: an instrument deck's Notes view, once
    /// it has played a note (the app swaps in other lanes or none).
    pub lanes: Option<Lanes>,
}

/// The decks' live sound, read from the engine on every frame.
pub struct LiveTraces {
    rx: [rtrb::Consumer<LiveSlice>; NUM_DECKS],
    decks: [Trace; NUM_DECKS],
}

impl LiveTraces {
    pub fn new(rx: [rtrb::Consumer<LiveSlice>; NUM_DECKS]) -> Self {
        Self {
            rx,
            decks: Default::default(),
        }
    }

    /// Takes what came in. `live[d]` says whether deck `d` plays live now;
    /// a deck that doesn't is cleared, so its next live part starts afresh.
    pub fn poll(&mut self, live: [bool; NUM_DECKS]) {
        for (d, rx) in self.rx.iter_mut().enumerate() {
            let trace = &mut self.decks[d];
            if !live[d] {
                while rx.pop().is_ok() {}
                if trace.received > 0 {
                    *trace = Trace::default();
                }
                continue;
            }
            while let Ok(s) = rx.pop() {
                trace.push(bin(&s));
            }
        }
    }

    /// Deck `d`'s waveform to draw at `now`, once anything has come in.
    /// `held`: the notes an instrument deck holds now (bit `n` for MIDI
    /// note `n`), for its Notes view in `color`; None on other decks.
    pub fn window(
        &mut self,
        d: usize,
        now: Instant,
        held: Option<u128>,
        color: [f32; 4],
    ) -> Option<Window> {
        self.decks[d].window(now, held, color)
    }
}

impl Trace {
    fn push(&mut self, b: WaveformBin) {
        self.bins.push_back(b);
        self.received += 1;
        // Room for the play line to lag what came in by a buffer or two.
        while self.bins.len() > WINDOW + 1024 {
            self.bins.pop_front();
        }
    }

    fn window(&mut self, now: Instant, held: Option<u128>, color: [f32; 4]) -> Option<Window> {
        if self.received == 0 {
            return None;
        }
        // Slices come a buffer at a time; the play line moves on smoothly
        // between them and draws only what it has passed.
        let came = self.received as f64 / WAVEFORM_RATE;
        let shown = self.clock.next(came, 1.0, now).min(came);
        let end = (shown * WAVEFORM_RATE).floor().max(0.0) as u64;
        // The bins up to the play line, the newest last.
        let kept = self.bins.len() as u64;
        let first_held = self.received - kept;
        let upto = (end.max(first_held) - first_held) as usize;
        let mut finest = vec![[0u8; 8]; WINDOW];
        let take = upto.min(WINDOW);
        for (i, b) in self.bins.range(upto - take..upto).enumerate() {
            finest[WINDOW - take + i] = *b;
        }
        let into = (shown * WAVEFORM_RATE - end as f64).max(0.0) / WAVEFORM_RATE;
        let play_seconds = WINDOW as f64 / WAVEFORM_RATE + into;
        let lanes = match held {
            Some(held) => {
                self.notes.update(held, shown);
                self.notes.lanes(shown - play_seconds, shown, color)
            }
            None => {
                self.notes = NoteRoll::default();
                None
            }
        };
        Some(Window {
            levels: Waveform::from_finest(WAVEFORM_RATE, finest).levels,
            play_seconds,
            clock: shown,
            lanes,
        })
    }
}

/// The leading deck's beat grid moved onto a live deck's window, so the live
/// part's beats line up with it. The leader is at beat `beat` (beat 0 a bar
/// line), at `bpm` as its grid says there, playing at `speed` times that;
/// the live deck's play line is `play_seconds` into its window.
pub fn leader_grid(
    bpm: f64,
    beat: f64,
    speed: f64,
    play_seconds: f64,
) -> [Option<GridSection>; GRID_SECTIONS] {
    let mut g = [None; GRID_SECTIONS];
    let bpm = bpm * speed;
    if bpm > 0.0 && speed > 0.0 {
        g[0] = Some(GridSection {
            seconds: play_seconds - beat * 60.0 / bpm,
            bpm,
            beat: 0.0,
        });
    }
    g
}

/// [`leader_grid`] for a leading track at `seconds`.
pub fn track_grid(
    leader: &Track,
    seconds: f64,
    speed: f64,
    play_seconds: f64,
) -> [Option<GridSection>; GRID_SECTIONS] {
    leader_grid(
        leader.bpm_at(seconds),
        leader.beat_at(seconds),
        speed,
        play_seconds,
    )
}

/// The leading track's phrases moved onto a live deck's window, as
/// [`track_grid`] moves its beats: each phrase starts as many beats from the
/// live play line as it is from the leader's position, at the leader's
/// tempo now. Phrases over before the window's left edge are left out, so
/// the ones in view are among the first [`MAX_PHRASES`] the shader draws.
pub fn track_phrases(
    leader: &Track,
    seconds: f64,
    speed: f64,
    play_seconds: f64,
    phrases: &[(f64, PhraseKind)],
) -> Vec<(f64, PhraseKind)> {
    let bpm = leader.bpm_at(seconds) * speed;
    if bpm <= 0.0 || speed <= 0.0 {
        return Vec::new();
    }
    let now = leader.beat_at(seconds);
    let moved: Vec<_> = phrases
        .iter()
        .map(|&(start, kind)| {
            let beats = leader.beat_at(start) - now;
            (play_seconds + beats * 60.0 / bpm, kind)
        })
        .collect();
    // The phrase under the window's left edge, and every one after it.
    let left = play_seconds - crate::VISIBLE_SECONDS;
    let first = moved.iter().rposition(|&(s, _)| s <= left).unwrap_or(0);
    moved.into_iter().skip(first).take(MAX_PHRASES).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn traces() -> (LiveTraces, [rtrb::Producer<LiveSlice>; NUM_DECKS]) {
        let mut tx = Vec::new();
        let mut rx = Vec::new();
        for _ in 0..NUM_DECKS {
            let (p, c) = rtrb::RingBuffer::new(WINDOW * 2);
            tx.push(p);
            rx.push(c);
        }
        (
            LiveTraces::new(rx.try_into().ok().unwrap()),
            tx.try_into().ok().unwrap(),
        )
    }

    fn loud() -> LiveSlice {
        LiveSlice {
            peak: [0.5; 4],
            rms: [0.25; 4],
        }
    }

    #[test]
    fn full_scale_is_the_top_of_the_waveform() {
        let full = LiveSlice {
            peak: [1.0; 4],
            rms: [1.0; 4],
        };
        assert_eq!(bin(&full), [255; 8]);
        assert_eq!(bin(&LiveSlice::default()), [0; 8]);
        // A quarter of full scale reads half way up, as on a track.
        assert_eq!(bin(&loud())[7], 128);
    }

    #[test]
    fn the_window_ends_at_the_play_line() {
        let (mut live, mut tx) = traces();
        for _ in 0..500 {
            tx[1].push(loud()).unwrap();
        }
        live.poll([false, true, false, false]);
        let w = live.window(1, Instant::now(), None, [1.0; 4]).unwrap();
        assert_eq!(w.levels[0].len(), WINDOW);
        assert!((w.play_seconds - WINDOW as f64 / WAVEFORM_RATE).abs() < 1e-9);
        // Half a second of sound at the end, silence before it.
        assert_ne!(w.levels[0][WINDOW - 1], [0; 8]);
        assert_ne!(w.levels[0][WINDOW - 500], [0; 8]);
        assert_eq!(w.levels[0][WINDOW - 501], [0; 8]);
        assert!(live.window(0, Instant::now(), None, [1.0; 4]).is_none());
    }

    #[test]
    fn a_deck_that_stops_playing_live_is_cleared() {
        let (mut live, mut tx) = traces();
        tx[0].push(loud()).unwrap();
        live.poll([true, false, false, false]);
        assert!(live.window(0, Instant::now(), None, [1.0; 4]).is_some());
        tx[0].push(loud()).unwrap();
        live.poll([false; NUM_DECKS]);
        assert!(live.window(0, Instant::now(), None, [1.0; 4]).is_none());
    }

    #[test]
    fn the_play_line_moves_on_smoothly_and_never_past_the_sound() {
        let (mut live, mut tx) = traces();
        let start = Instant::now();
        let mut last = 0.0;
        // Buffers of 10 ms, drawn at 60 frames a second.
        let mut sent = 0;
        for frame in 0..120 {
            let now = start + Duration::from_secs_f64(frame as f64 / 60.0);
            while (sent as f64) < (frame as f64 / 60.0) * 1000.0 {
                for _ in 0..10 {
                    tx[2].push(loud()).unwrap();
                }
                sent += 10;
            }
            live.poll([false, false, true, false]);
            if let Some(w) = live.window(2, now, None, [1.0; 4]) {
                // Whatever is drawn ends at the play line.
                let into = w.play_seconds - WINDOW as f64 / WAVEFORM_RATE;
                assert!((0.0..0.001).contains(&into), "frame {frame}: {into}");
                let shown = live.decks[2].received as f64; // slices
                assert!(shown >= last);
                last = shown;
            }
        }
    }

    #[test]
    fn the_leaders_beats_line_up_on_the_live_deck() {
        // The leader is on beat 20, a bar line: so is the live play line.
        let g = leader_grid(120.0, 20.0, 1.0, 8.192)[0].unwrap();
        assert_eq!(g.bpm, 120.0);
        let beats_at_play = (8.192 - g.seconds) * g.bpm / 60.0;
        assert!((beats_at_play - 20.0).abs() < 1e-9);
        // Sped up, the beats come faster.
        let fast = leader_grid(120.0, 20.0, 1.08, 8.192)[0].unwrap();
        assert!((fast.bpm - 129.6).abs() < 1e-9);
        // No tempo, no grid.
        assert!(leader_grid(0.0, 20.0, 1.0, 8.192)[0].is_none());
    }

    fn bit(n: u8) -> u128 {
        1u128 << n
    }

    #[test]
    fn notes_play_on_two_fixed_octaves() {
        let mut roll = NoteRoll::default();
        assert!(roll.lanes(0.0, 1.0, [1.0; 4]).is_none());
        // C4 at 1 s, E4 joins at 1.5 s, C4 let go at 2 s; drawn at 2.5 s.
        roll.update(bit(60), 1.0);
        roll.update(bit(60) | bit(64), 1.5);
        roll.update(bit(64), 2.0);
        let lanes = roll.lanes(0.0, 2.5, [1.0; 4]).unwrap();
        // C3 to B4, whatever is played: the rows never rescale.
        assert_eq!(lanes.count, NOTE_ROWS);
        assert_eq!(lanes.notes, Some(48.0));
        assert_eq!(lanes.trace, 0.0, "no waveform under the notes");
        let c = lanes.marks[0];
        assert_eq!(
            (c.start, c.end, c.lane, c.kind),
            (1.0, 2.0, 60, MarkKind::Played)
        );
        let e = lanes.marks[1];
        assert_eq!(
            (e.start, e.end, e.lane, e.kind),
            (1.5, 2.5, 64, MarkKind::Held)
        );
        // A note far above stays on the rows' top edge (the shader puts
        // it there); the rows stay.
        roll.update(bit(64) | bit(88), 3.0);
        let lanes = roll.lanes(0.0, 3.0, [1.0; 4]).unwrap();
        assert_eq!((lanes.count, lanes.notes), (NOTE_ROWS, Some(48.0)));
        assert!(lanes.marks.iter().any(|m| m.lane == 88));
    }

    #[test]
    fn the_first_note_sits_clear_of_the_edges() {
        assert_eq!(first_bottom(60), 48, "C4: C3 to B4");
        assert_eq!(first_bottom(64), 48, "E4: C3 to B4");
        assert_eq!(first_bottom(69), 60, "A4: C4 to B5");
        assert_eq!(first_bottom(2), 0);
        assert_eq!(first_bottom(127), 96);
    }

    #[test]
    fn playing_out_past_the_rows_slides_them_an_octave() {
        let mut roll = NoteRoll::default();
        roll.update(bit(60), 0.0);
        roll.update(0, 0.5);
        // A stray note above, then back inside: the rows stay.
        roll.update(bit(76), 1.0);
        roll.update(0, 1.2);
        roll.update(bit(62), 1.5);
        roll.update(0, 1.7);
        roll.update(0, 6.0);
        assert_eq!(roll.bottom, Some(48));
        // A melody up there, a note every half second for over 4 s.
        let mut t = 7.0;
        while t < 11.6 {
            roll.update(bit(74 + (t * 2.0) as u8 % 3), t);
            roll.update(0, t + 0.25);
            t += 0.5;
        }
        assert_eq!(roll.bottom, Some(60), "up an octave");
        // It glides there.
        let (from, t0) = roll.slide.unwrap();
        assert_eq!(from, 48.0);
        let mid = roll.base(t0 + SLIDE_SECONDS / 2.0);
        assert!(mid > 50.0 && mid < 58.0, "half way: {mid}");
        assert_eq!(roll.base(t0 + SLIDE_SECONDS), 60.0);
    }

    #[test]
    fn a_note_held_out_there_slides_them_too() {
        let mut roll = NoteRoll::default();
        roll.update(bit(60), 0.0);
        roll.update(0, 0.5);
        // C2 held below the rows.
        let mut t = 1.0;
        while t < 5.5 {
            roll.update(bit(36), t);
            t += 0.1;
        }
        assert_eq!(roll.bottom, Some(36), "down an octave");
    }

    #[test]
    fn stray_notes_are_forgotten() {
        let mut roll = NoteRoll::default();
        roll.update(bit(60), 0.0);
        roll.update(0, 0.5);
        // Two notes up there 3 s apart, with nothing between: no slide.
        roll.update(bit(80), 1.0);
        roll.update(0, 1.1);
        let mut t = 1.2;
        while t < 4.0 {
            roll.update(0, t);
            t += 0.1;
        }
        roll.update(bit(80), 4.0);
        roll.update(0, 4.1);
        roll.update(0, 6.0);
        assert_eq!(roll.bottom, Some(48));
    }

    #[test]
    fn notes_long_gone_are_dropped() {
        let mut roll = NoteRoll::default();
        roll.update(1 << 60, 1.0);
        roll.update(0, 1.2);
        roll.update(0, 20.0);
        assert!(roll.notes.is_empty());
        // The rows stay where they were.
        assert_eq!(roll.lanes(12.0, 20.0, [1.0; 4]).unwrap().marks.len(), 0);
    }

    #[test]
    fn the_scope_stands_still_on_a_steady_note() {
        assert!(scope_points(&[0.0; 100]).is_none());
        // A sine 64 samples a cycle: the scope ends on a rising crossing,
        // whatever the phase the samples end on.
        let sine = |phase: usize| -> Vec<f32> {
            (0..8192)
                .map(|i| ((i + phase) as f32 * std::f32::consts::TAU / 64.0).sin())
                .collect()
        };
        let a = scope_points(&sine(0)).unwrap();
        let b = scope_points(&sine(23)).unwrap();
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 0.15, "{x} vs {y}");
        }
        // Its peaks keep their height and sign.
        let top = a.iter().cloned().fold(f32::MIN, f32::max);
        let bottom = a.iter().cloned().fold(f32::MAX, f32::min);
        assert!(top > 0.95 && bottom < -0.95, "{top} {bottom}");
    }

    /// The leader's phrases land on the live deck the same beats away from
    /// its play line, at the leader's speed; long-gone ones are left out.
    #[test]
    fn moves_the_leaders_phrases_onto_the_live_window() {
        let mut t = (*opendj_engine::sample_library()[0]).clone();
        t.bpm = 120.0;
        t.first_beat = 0.0;
        t.grid = Default::default();
        t.duration = 600.0;
        let phrases = [
            (0.0, PhraseKind::Intro),
            (32.0, PhraseKind::Build),
            (64.0, PhraseKind::Drop),
            (96.0, PhraseKind::Outro),
        ];
        // The leader is 4 s (8 beats) before its drop; the live play line
        // is 8 s into its window.
        let moved = track_phrases(&t, 60.0, 1.0, 8.0, &phrases);
        assert_eq!(moved[0], (-20.0, PhraseKind::Build));
        assert_eq!(moved[1], (12.0, PhraseKind::Drop));
        assert_eq!(moved.len(), 3, "the intro, long over, is left out");
        // Played 10% faster, the 8 beats pass sooner.
        let fast = track_phrases(&t, 60.0, 1.1, 8.0, &phrases);
        assert!((fast[1].0 - (8.0 + 4.0 / 1.1)).abs() < 1e-9);
        assert!(track_phrases(&t, 60.0, 0.0, 8.0, &phrases).is_empty());
    }
}
