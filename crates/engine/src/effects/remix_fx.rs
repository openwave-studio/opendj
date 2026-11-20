//! The Remix effects that also play in any slot: Tape Echo, Ducker and
//! Rhythm (Solo needs no state: see [`super::Instance::process_bands`]).
//! Written for openDJ after the RMX-style remix units; no code is copied.

use std::f32::consts::TAU;

use super::{beats, glide, lock_to_beat, soft, tap, Ctx, Echo, MAX_PARAMETERS};
use crate::dsp::Svf;

// ---------------------------------------------------------------------------
// Tape Echo

/// Wow: the repeats' pitch drifts this much (a share of the delay) at most,
/// about once every couple of seconds; flutter is faster and smaller.
const WOW_DEPTH: f32 = 0.004;
const WOW_HZ: f32 = 0.55;
const FLUTTER_DEPTH: f32 = 0.0007;
const FLUTTER_HZ: f32 = 5.3;

/// An echo with the character of a tape loop: each repeat is pushed into
/// gentle saturation, loses its lowest lows and a little top, and its pitch
/// wobbles with the tape's wow and flutter.
pub(super) struct TapeEcho {
    pub(super) echo: Echo,
    /// The delay `echo` would have without wow, set each buffer.
    base: f32,
    wow: f32,
    wow_target: f32,
    phase: [f32; 2],
    step: [f32; 2],
    hp: Svf,
    sample_rate: f32,
}

impl TapeEcho {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            echo: Echo::new(sample_rate),
            base: 1.0,
            wow: 0.0,
            wow_target: 0.0,
            phase: [0.0; 2],
            step: [WOW_HZ / sample_rate, FLUTTER_HZ / sample_rate],
            hp: Svf::default(),
            sample_rate,
        }
    }

    pub(super) fn reset(&mut self, mem: &mut [f32]) {
        self.echo.reset(mem);
        self.hp = Svf::default();
        self.phase = [0.0; 2];
        self.wow = self.wow_target;
    }

    pub(super) fn prepare(&mut self, p: &[f32; MAX_PARAMETERS], ctx: Ctx) {
        // Time, Feedback, Wow, Send.
        self.echo.prepare(&[p[0], p[1], 0.0, p[3]], ctx);
        self.base = self.echo.target;
        self.wow_target = p[2];
        self.hp.set(self.sample_rate, 90.0, 0.7);
    }

    #[inline]
    pub(super) fn process(&mut self, mem: &mut [f32], x: [f32; 2]) -> [f32; 2] {
        glide(&mut self.wow, self.wow_target);
        for (ph, st) in self.phase.iter_mut().zip(self.step) {
            *ph += st;
            if *ph >= 1.0 {
                *ph -= 1.0;
            }
        }
        let drift =
            (TAU * self.phase[0]).sin() * WOW_DEPTH + (TAU * self.phase[1]).sin() * FLUTTER_DEPTH;
        let most = (self.echo.len - 2) as f32;
        self.echo.target = (self.base * (1.0 + drift * self.wow)).clamp(1.0, most);
        let hp = &mut self.hp;
        self.echo.process_with(mem, x, |v| {
            let v = hp.process(v)[2];
            // Driven into the tape a little harder than it plays back.
            [soft(1.6 * v[0]) * 0.7, soft(1.6 * v[1]) * 0.7]
        })
    }
}

// ---------------------------------------------------------------------------
// Ducker

/// A beat-synced volume envelope, as a sidechained compressor pumps: the
/// sound dips on every beat and swells back up over `recover` of a beat.
pub(super) struct Ducker {
    phase: f32,
    step: f32,
    unlocked: bool,
    depth: f32,
    depth_target: f32,
    /// The share of a beat the swell back takes.
    recover: f32,
    gain: f32,
    /// The fastest the gain falls per frame: each dip takes about 3 ms, so
    /// it doesn't click.
    fall: f32,
}

impl Default for Ducker {
    fn default() -> Self {
        Self {
            phase: 0.0,
            step: 0.0,
            unlocked: true,
            depth: 0.0,
            depth_target: 0.0,
            recover: 0.5,
            gain: 1.0,
            fall: 1.0,
        }
    }
}

impl Ducker {
    pub(super) fn prepare(&mut self, p: &[f32; MAX_PARAMETERS], ctx: Ctx) {
        // Depth; Time: the swell takes 1/4, 1/2 or a whole beat.
        self.step = 1.0 / (ctx.beat_seconds * ctx.sample_rate);
        lock_to_beat(
            &mut self.phase,
            &mut self.step,
            &mut self.unlocked,
            ctx.beat,
            1.0,
        );
        self.depth_target = p[0];
        self.recover = beats(p[1], -2, 0);
        self.fall = 1.0 / (0.003 * ctx.sample_rate);
    }

    #[inline]
    pub(super) fn process(&mut self, x: [f32; 2]) -> [f32; 2] {
        glide(&mut self.depth, self.depth_target);
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let t = (self.phase / self.recover).min(1.0);
        // Back up fast at first, then easing in: (1 - t)^2.
        let target = 1.0 - self.depth * (1.0 - t) * (1.0 - t);
        self.gain = if target < self.gain {
            (self.gain - self.fall).max(target)
        } else {
            target
        };
        [x[0] * self.gain, x[1] * self.gain]
    }
}

// ---------------------------------------------------------------------------
// Rhythm

/// Slices per cycle.
const SLICES: usize = 16;

/// Rhythm's patterns: for each slice of the cycle, how many slices back it
/// plays from. 0 plays the music as it is; 1 repeats the slice before.
const PATTERNS: [[u8; SLICES]; 4] = [
    // Stutter: every other pair repeats.
    [0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1, 0, 3, 2, 1],
    // Skip: hits pulled back on the offbeats.
    [0, 1, 0, 1, 0, 0, 0, 2, 0, 1, 0, 1, 2, 2, 3, 3],
    // Build: longer and longer holds.
    [0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 3, 3, 4, 4, 4, 4],
    // Shuffle: slices out of order.
    [0, 2, 0, 2, 1, 0, 3, 0, 0, 2, 0, 2, 1, 3, 2, 1],
];

/// The most a pattern reaches back, in slices.
const MOST_BACK: u8 = 4;

/// Cuts the music into slices on the beat and replays them in a fixed
/// rearranged order: each slice plays from the same place or a few slices
/// back, so the groove turns into a new rhythm while staying in time.
/// Each jump crossfades over 3 ms.
pub(super) struct Rhythm {
    len: usize,
    write: usize,
    phase: f32,
    step: f32,
    unlocked: bool,
    slice_frames: f32,
    pattern: usize,
    slice: usize,
    /// The delay (frames) of this slice and of the one before, faded across.
    delay: f32,
    before: f32,
    fade: f32,
    fade_step: f32,
}

impl Rhythm {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            len: 1,
            write: 0,
            phase: 0.0,
            step: 0.0,
            unlocked: true,
            slice_frames: 1.0,
            pattern: 0,
            slice: 0,
            delay: 0.0,
            before: 0.0,
            fade: 1.0,
            fade_step: 1.0 / (0.003 * sample_rate),
        }
    }

    pub(super) fn reset(&mut self, mem: &mut [f32]) {
        self.len = mem.len() / 2;
        mem[..2 * self.len].fill(0.0);
        self.write = 0;
        self.unlocked = true;
        self.slice = 0;
        self.delay = 0.0;
        self.before = 0.0;
        self.fade = 1.0;
    }

    pub(super) fn prepare(&mut self, p: &[f32; MAX_PARAMETERS], ctx: Ctx) {
        // Rate: slices of 1/16, 1/8 or 1/4 beat; Pattern.
        let slice_beats = beats(p[0], -4, -2);
        let cycle = slice_beats * SLICES as f32;
        // As long as the memory lets it reach back.
        let most = (self.len as f32 - 4.0) / MOST_BACK as f32;
        self.slice_frames = (slice_beats * ctx.beat_seconds * ctx.sample_rate).clamp(1.0, most);
        self.step = 1.0 / (self.slice_frames * SLICES as f32);
        lock_to_beat(
            &mut self.phase,
            &mut self.step,
            &mut self.unlocked,
            ctx.beat,
            cycle,
        );
        self.pattern =
            ((p[1] * (PATTERNS.len() - 1) as f32).round() as usize).min(PATTERNS.len() - 1);
    }

    #[inline]
    pub(super) fn process(&mut self, mem: &mut [f32], x: [f32; 2]) -> [f32; 2] {
        let (l, r) = mem[..2 * self.len].split_at_mut(self.len);
        l[self.write] = x[0];
        r[self.write] = x[1];
        self.write += 1;
        if self.write == self.len {
            self.write = 0;
        }
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let slice = ((self.phase * SLICES as f32) as usize).min(SLICES - 1);
        if slice != self.slice {
            self.slice = slice;
            self.before = self.delay;
            self.delay = PATTERNS[self.pattern][slice] as f32 * self.slice_frames;
            self.fade = 0.0;
        }
        // One frame back is the frame just written.
        let read = |delay: f32, l: &[f32], r: &[f32], write: usize| {
            [tap(l, write, delay + 1.0), tap(r, write, delay + 1.0)]
        };
        let now = read(self.delay, l, r, self.write);
        if self.fade >= 1.0 {
            return now;
        }
        let was = read(self.before, l, r, self.write);
        let f = self.fade;
        self.fade = (self.fade + self.fade_step).min(1.0);
        [
            was[0] + (now[0] - was[0]) * f,
            was[1] + (now[1] - was[1]) * f,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn ctx(beat: Option<f64>) -> Ctx {
        Ctx {
            sample_rate: SR,
            beat_seconds: 0.5,
            beat,
        }
    }

    #[test]
    fn the_ducker_dips_on_the_beat_and_swells_back() {
        let mut d = Ducker::default();
        d.prepare(&[1.0, 0.5, 0.0, 0.0], ctx(Some(0.0)));
        // Let the depth glide in.
        let gains: Vec<f32> = (0..96_000).map(|_| d.process([1.0, 1.0])[0]).collect();
        let beat = 24_000;
        let last = &gains[2 * beat..3 * beat];
        let low = last.iter().cloned().fold(1.0, f32::min);
        assert!(low < 0.05, "dips to silence at full depth: {low}");
        // Half a beat in, it is back up.
        assert!(last[beat / 2 + 100] > 0.99);
        // And the dip comes right on the beat (within the 3 ms fall).
        let at = last.iter().position(|&g| g == low).unwrap();
        assert!(at < 200 || at > beat - 10, "{at}");
    }

    #[test]
    fn rhythm_plays_slices_from_a_few_slices_back() {
        let mut mem = vec![0.0f32; 2 * 96_000].into_boxed_slice();
        let mut rh = Rhythm::new(SR);
        rh.reset(&mut mem);
        rh.prepare(&[0.5, 0.0, 0.0, 0.0], ctx(Some(0.0)));
        // A ramp shows where each output frame came from.
        let n = 48_000;
        let out: Vec<f32> = (0..n)
            .map(|i| rh.process(&mut mem, [i as f32, i as f32])[0])
            .collect();
        // 1/8 beat slices: 3000 frames. Slice 2 of the stutter pattern
        // plays one slice back, slice 0 as it is.
        let slice = 3000;
        let mid = |k: usize| k * slice + slice / 2;
        assert!((out[mid(1)] - mid(1) as f32).abs() < 2.0);
        assert!((out[mid(2)] - (mid(2) - slice) as f32).abs() < 2.0);
        assert!((out[mid(6)] - (mid(6) - 2 * slice) as f32).abs() < 2.0);
    }

    #[test]
    fn tape_echo_repeats_with_a_drifting_pitch() {
        let mut mem = vec![0.0f32; 2 * 96_100].into_boxed_slice();
        let mut te = TapeEcho::new(SR);
        te.reset(&mut mem);
        te.prepare(&[0.375, 0.5, 1.0, 1.0], ctx(None));
        let mut y = Vec::new();
        for i in 0..48_000 {
            let v = if i < 480 { 0.5 } else { 0.0 };
            y.push(te.process(&mut mem, [v, v])[0]);
        }
        // The first repeat comes about 1/2 beat (12000 frames) later.
        let peak = (11_000..13_500).map(|i| y[i].abs()).fold(0.0, f32::max);
        assert!(peak > 0.1, "{peak}");
        assert!(y.iter().all(|v| v.abs() < 1.0));
    }
}
