//! The Remix deck: a deck kind that plays another deck (or the mic or an
//! aux input) through three band lanes, each with an ISOLATE knob and a
//! LEVER, after the RMX-style remix units.
//!
//! Its input is the source's channel after its EQ and before its fader. In
//! Send mode the source keeps playing on its own channel and the Remix deck
//! plays only what its lanes add (each lane's output minus its band), so an
//! idle Remix deck adds exactly nothing. Turning bands down (ISOLATE left,
//! Solo) needs Insert: in Send the source's own channel still plays them,
//! and playing a band upside down would not cancel it (the crossovers shift
//! its phase), so they do nothing there. In Insert mode the source's channel goes quiet
//! and the Remix deck plays the processed source, bit-identical while idle.
//!
//! The source is split as the channel isolator splits it (Linkwitz-Riley
//! crossovers, so the bands sum flat and a killed band is gone). While
//! nothing is on the deck plays the source itself; it fades over to the
//! bands' sum when something comes on and back once all is idle again.

use opendj_control::{Behavior, ControlHandle, ControlRegistry};

use super::{fade_step, glide, link, Ctx, Instance, Kind, MANIFESTS, MAX_PARAMETERS, SILENCE};
use crate::dsp::{Isolator, EQ_HIGH_HZ, EQ_LOW_HZ};
use crate::inputs::{AUXILIARIES, MICROPHONE};
use crate::NUM_DECKS;

/// The Isolate effects, by `isolate_effect`. The knob turned right adds
/// the effect to its band, more as it turns.
pub const ISOLATE_EFFECTS: [&str; 4] = ["Echo", "Reverb", "Drive", "Filter"];
/// The Lever effects, by `lever_effect`.
pub const LEVER_EFFECTS: [&str; 5] = ["Echo", "Reverb", "Repeat", "Reverse", "Solo"];

/// The band names in control keys, low to high.
pub const BANDS: [&str; 3] = ["low", "mid", "high"];

/// `[RemixN],mode`.
pub const SEND: f64 = 0.0;
pub const INSERT: f64 = 1.0;

/// ISOLATE within this of centre is off, so a detented knob is exactly off.
const DEAD_ZONE: f64 = 0.02;

/// `[RemixN],source`: 1-4 a deck, 5 the mic, 6-13 the aux inputs.
pub const SOURCE_MIC: usize = NUM_DECKS + 1;
pub const SOURCES: usize = SOURCE_MIC + AUXILIARIES.len();

pub fn remix_group(deck: usize) -> String {
    format!("[Remix{}]", deck + 1)
}

/// Where a Remix deck's `source` comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Deck(usize),
    Input(usize),
}

/// `source` as a deck or an input, if it names one.
pub fn source(v: f64) -> Option<Source> {
    let n = v.round();
    if n < 1.0 || n > SOURCES as f64 {
        return None;
    }
    let n = n as usize;
    Some(if n <= NUM_DECKS {
        Source::Deck(n - 1)
    } else if n == SOURCE_MIC {
        Source::Input(MICROPHONE)
    } else {
        Source::Input(AUXILIARIES[n - SOURCE_MIC - 1])
    })
}

/// A deck's `[RemixN]` controls.
pub struct RemixControls {
    /// The deck is the Remix deck. One deck at a time.
    pub enabled: ControlHandle,
    pub source: ControlHandle,
    pub mode: ControlHandle,
    /// -1..1 per band: left lowers the band to silence, right adds the
    /// Isolate effect.
    pub isolate: [ControlHandle; 3],
    /// The Lever effect's depth per band, 0..1.
    pub lever: [ControlHandle; 3],
    /// The lever is down (held or latched; the mappings and the remote
    /// decide which).
    pub lever_on: [ControlHandle; 3],
    pub isolate_effect: ControlHandle,
    pub lever_effect: ControlHandle,
}

impl RemixControls {
    pub fn new(reg: &ControlRegistry, deck: usize) -> Self {
        let g = remix_group(deck);
        let g = g.as_str();
        // By default it listens to the deck beside it: 1 and 2, 3 and 4.
        let partner = (deck ^ 1) + 1;
        Self {
            enabled: reg.toggle(g, "enabled"),
            source: reg.create(
                g,
                "source",
                partner as f64,
                1.0,
                SOURCES as f64,
                Behavior::Free,
            ),
            mode: reg.create(g, "mode", SEND, SEND, INSERT, Behavior::Free),
            isolate: std::array::from_fn(|b| {
                reg.create(
                    g,
                    &format!("isolate_{}", BANDS[b]),
                    0.0,
                    -1.0,
                    1.0,
                    Behavior::CenteredOnDefault,
                )
            }),
            lever: std::array::from_fn(|b| {
                reg.create(
                    g,
                    &format!("lever_{}", BANDS[b]),
                    1.0,
                    0.0,
                    1.0,
                    Behavior::Linear,
                )
            }),
            lever_on: std::array::from_fn(|b| reg.toggle(g, &format!("lever_{}_on", BANDS[b]))),
            isolate_effect: reg.create(
                g,
                "isolate_effect",
                0.0,
                0.0,
                (ISOLATE_EFFECTS.len() - 1) as f64,
                Behavior::Free,
            ),
            lever_effect: reg.create(
                g,
                "lever_effect",
                0.0,
                0.0,
                (LEVER_EFFECTS.len() - 1) as f64,
                Behavior::Free,
            ),
        }
    }

    /// The settings a Remix deck keeps when it moves to another deck.
    fn settings(&self) -> impl Iterator<Item = &ControlHandle> {
        [
            &self.source,
            &self.mode,
            &self.isolate_effect,
            &self.lever_effect,
        ]
        .into_iter()
        .chain(&self.isolate)
        .chain(&self.lever)
        .chain(&self.lever_on)
    }

    /// Takes on `from`'s settings, when the Remix deck moves here.
    pub fn take_settings(&self, from: &RemixControls) {
        for (to, from) in self.settings().zip(from.settings()) {
            to.set(from.get());
        }
    }
}

/// The Isolate effect `isolate_effect` picks.
fn isolate_kind(v: f64) -> Kind {
    match v.round() as i64 {
        1 => Kind::Reverb,
        2 => Kind::Drive,
        3 => Kind::Filter,
        _ => Kind::Echo,
    }
}

/// What `lever_effect` picks: an effect, or Solo (`None`).
fn lever_kind(v: f64) -> Option<Kind> {
    match v.round() as i64 {
        1 => Some(Kind::Reverb),
        2 => Some(Kind::Roll),
        3 => Some(Kind::RevRoll),
        4 => None,
        _ => Some(Kind::Echo),
    }
}

/// Effects whose sound rings on after their input stops: they keep playing
/// fully once let go, their send closed, until they fall silent.
fn rings(kind: Kind) -> bool {
    matches!(kind, Kind::Echo | Kind::Reverb)
}

/// An effect's parameters at its defaults with `meta` applied as on a slot.
fn parameters(kind: Kind) -> [f32; MAX_PARAMETERS] {
    let defaults = MANIFESTS[kind as usize].defaults;
    std::array::from_fn(|i| defaults.get(i).copied().unwrap_or(0.0) as f32)
}

fn with_meta(kind: Kind, meta: f64) -> [f32; MAX_PARAMETERS] {
    let mut p = parameters(kind);
    for (v, l) in p.iter_mut().zip(link(kind, meta)) {
        if let Some(l) = l {
            *v = l as f32;
        }
    }
    p
}

/// The normalized Filter parameters for a resonant band-pass around `hz`.
fn band_pass(hz: f64) -> [f32; MAX_PARAMETERS] {
    let lp = ((hz * 1.5) / 20.0).ln() / 1000f64.ln();
    let hp = ((hz / 1.5) / 20.0).ln() / 500f64.ln();
    [
        lp.clamp(0.0, 1.0) as f32,
        0.5,
        hp.clamp(0.0, 1.0) as f32,
        0.0,
    ]
}

/// One effect in a lane: an instance that wakes when turned on and sleeps
/// once it is off and quiet.
struct Slot {
    inst: Instance,
    kind: Option<Kind>,
    on: bool,
    awake: bool,
    /// How much of the effect is heard, gliding to `mix_target`.
    mix: f32,
    mix_target: f32,
    /// Samples the effect has been off and silent.
    quiet: f32,
    sample_rate: f32,
}

impl Slot {
    fn new(sample_rate: f32) -> Self {
        Self {
            inst: Instance::with_memory(sample_rate),
            kind: None,
            on: false,
            awake: false,
            mix: 0.0,
            mix_target: 0.0,
            quiet: 0.0,
            sample_rate,
        }
    }

    /// For the next buffer: the effect, on or off, how much of it is heard
    /// while on, and its parameters.
    fn set(&mut self, kind: Kind, on: bool, wet: f32, p: &[f32; MAX_PARAMETERS], ctx: Ctx) {
        if self.kind != Some(kind) {
            self.kind = Some(kind);
            self.inst.kind = Some(kind);
            self.awake = false;
            self.mix = 0.0;
        }
        if on && (!self.on || !self.awake) {
            // Repeats start from what plays when the lever goes down.
            if !self.awake || matches!(kind, Kind::Roll | Kind::RevRoll) {
                self.inst.reset();
            }
            if !self.awake {
                self.mix = 0.0;
            }
            self.awake = true;
            self.quiet = 0.0;
        }
        self.on = on;
        self.mix_target = match (on, rings(kind)) {
            (true, _) => wet,
            (false, true) => 1.0,
            (false, false) => 0.0,
        };
        if self.awake {
            self.inst.prepare(p, ctx);
        }
    }

    #[inline]
    fn process(&mut self, x: [f32; 2], step: f32) -> [f32; 2] {
        if !self.awake {
            return x;
        }
        let d = self.mix_target - self.mix;
        self.mix += d.clamp(-step, step);
        let y = self.inst.process(x);
        let out = [
            x[0] + (y[0] - x[0]) * self.mix,
            x[1] + (y[1] - x[1]) * self.mix,
        ];
        if !self.on {
            let left = (y[0] - x[0]).abs().max((y[1] - x[1]).abs()) * self.mix;
            if left < SILENCE {
                self.quiet += 1.0;
            } else {
                self.quiet = 0.0;
            }
            if self.mix == 0.0 || self.quiet > super::SLEEP_AFTER * self.sample_rate {
                self.awake = false;
                self.mix = 0.0;
            }
        }
        out
    }
}

/// One band's lane: the ISOLATE gain or effect, then the lever.
struct Lane {
    gain: f32,
    gain_target: f32,
    /// The other lanes' levers on Solo turn this one down to here.
    solo: f32,
    solo_target: f32,
    isolate: Slot,
    lever: Slot,
}

/// The Remix deck's sound. There is one: only one deck is the Remix deck at
/// a time.
pub struct RemixDeck {
    sample_rate: f32,
    split: Isolator,
    /// How much of the bands' sum plays instead of the source, gliding
    /// to 1 while anything is on and back to 0 once all is idle.
    blend: f32,
    lanes: [Lane; 3],
    step: f32,
    insert: bool,
}

/// ISOLATE turned left: the band's gain, silent at the end.
fn kill_gain(v: f64) -> f32 {
    if v < -DEAD_ZONE {
        let t = (1.0 + v).clamp(0.0, 1.0) as f32;
        t * t
    } else {
        1.0
    }
}

impl RemixDeck {
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate as f32;
        Self {
            sample_rate: sr,
            split: Isolator::new(sample_rate),
            blend: 0.0,
            lanes: std::array::from_fn(|_| Lane {
                gain: 1.0,
                gain_target: 1.0,
                solo: 1.0,
                solo_target: 1.0,
                isolate: Slot::new(sr),
                lever: Slot::new(sr),
            }),
            step: fade_step(sr),
            insert: false,
        }
    }

    /// In Insert mode the source's own channel is quiet.
    pub fn insert(&self) -> bool {
        self.insert
    }

    /// Reads the controls for the next buffer, at the source's tempo:
    /// `beat_seconds` long beats, at `beat` in its grid if it has one.
    pub fn prepare(&mut self, c: &RemixControls, beat_seconds: f64, beat: Option<f64>) {
        let ctx = Ctx {
            sample_rate: self.sample_rate,
            beat_seconds: beat_seconds as f32,
            beat,
        };
        self.insert = c.mode.get() >= 0.5;
        let iso_kind = isolate_kind(c.isolate_effect.get());
        let lever_kind = lever_kind(c.lever_effect.get());
        let solo: [Option<f32>; 3] = std::array::from_fn(|b| {
            (lever_kind.is_none() && c.lever_on[b].get_bool())
                .then(|| c.lever[b].get().clamp(0.0, 1.0) as f32)
        });
        // The deepest solo lever turns the other bands down.
        let solo_depth = solo.iter().flatten().fold(0.0f32, |a, &d| a.max(d));
        let edges = [
            (20.0, EQ_LOW_HZ),
            (EQ_LOW_HZ, EQ_HIGH_HZ),
            (EQ_HIGH_HZ, 16_000.0),
        ];
        for (b, lane) in self.lanes.iter_mut().enumerate() {
            let v = c.isolate[b].get().clamp(-1.0, 1.0);
            lane.gain_target = if self.insert { kill_gain(v) } else { 1.0 };
            let on = v > DEAD_ZONE;
            let depth = if on { v } else { 0.0 };
            let (p, wet) = match iso_kind {
                Kind::Filter => {
                    // A band-pass sweeping up through the band.
                    let (lo, hi) = edges[b];
                    let hz = lo * (hi / lo).powf(depth);
                    (band_pass(hz), (4.0 * depth).min(1.0) as f32)
                }
                Kind::Drive => (with_meta(iso_kind, depth), (4.0 * depth).min(1.0) as f32),
                // Echo and Reverb: the knob opens their send.
                _ => (with_meta(iso_kind, depth), 1.0),
            };
            lane.isolate.set(iso_kind, on, wet, &p, ctx);

            lane.solo_target = if !self.insert || solo[b].is_some() || solo_depth == 0.0 {
                1.0
            } else {
                1.0 - solo_depth
            };
            let depth = c.lever[b].get().clamp(0.0, 1.0);
            match lever_kind {
                Some(kind) => {
                    let on = c.lever_on[b].get_bool() && depth > 0.0;
                    let (p, wet) = if rings(kind) {
                        (with_meta(kind, if on { depth } else { 0.0 }), 1.0)
                    } else {
                        (parameters(kind), depth as f32)
                    };
                    lane.lever.set(kind, on, wet, &p, ctx);
                }
                None => {
                    // Solo has no effect of its own: what was on rings out.
                    if let Some(kind) = lane.lever.kind {
                        let p = if rings(kind) {
                            with_meta(kind, 0.0)
                        } else {
                            parameters(kind)
                        };
                        lane.lever.set(kind, false, 0.0, &p, ctx);
                    }
                }
            }
        }
    }

    /// The source's frame `x` through the lanes, and what the lanes
    /// changed in it.
    #[inline]
    fn process(&mut self, x: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let bands = self.split.split(x);
        let mut y = [0.0f32; 2];
        let mut change = [0.0f32; 2];
        let mut busy = false;
        for (lane, band) in self.lanes.iter_mut().zip(bands) {
            glide(&mut lane.gain, lane.gain_target);
            let d = lane.solo_target - lane.solo;
            lane.solo += d.clamp(-self.step, self.step);
            let g = lane.gain * lane.solo;
            busy |= g != 1.0 || lane.isolate.awake || lane.lever.awake;
            let mut s = [band[0] * g, band[1] * g];
            s = lane.isolate.process(s, self.step);
            s = lane.lever.process(s, self.step);
            y[0] += s[0];
            y[1] += s[1];
            change[0] += s[0] - band[0];
            change[1] += s[1] - band[1];
        }
        let target = if busy { 1.0 } else { 0.0 };
        let d = target - self.blend;
        self.blend += d.clamp(-self.step, self.step);
        if self.blend == 0.0 {
            // Idle: the source itself, bit for bit.
            return (x, change);
        }
        let y = [
            x[0] + (y[0] - x[0]) * self.blend,
            x[1] + (y[1] - x[1]) * self.blend,
        ];
        (y, change)
    }

    /// What the Remix deck plays for the source frame `x`: the processed
    /// source in Insert mode, only the change in Send mode.
    #[inline]
    pub fn play(&mut self, x: [f32; 2]) -> [f32; 2] {
        let (y, change) = self.process(x);
        if self.insert {
            y
        } else {
            change
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn sine(hz: f64, n: usize) -> impl Iterator<Item = [f32; 2]> {
        (0..n).map(move |i| {
            let v = (0.5 * (std::f64::consts::TAU * hz * i as f64 / SR).sin()) as f32;
            [v, v]
        })
    }

    /// Two tones, a bass and a hi-hat-like high.
    fn music(n: usize) -> Vec<[f32; 2]> {
        sine(80.0, n)
            .zip(sine(6000.0, n))
            .map(|(a, b)| [a[0] + b[0], a[1] + b[1]])
            .collect()
    }

    fn setup() -> (std::sync::Arc<ControlRegistry>, RemixControls, RemixDeck) {
        let reg = ControlRegistry::new();
        let c = RemixControls::new(&reg, 1);
        (reg, c, RemixDeck::new(SR))
    }

    fn run(deck: &mut RemixDeck, c: &RemixControls, x: &[[f32; 2]]) -> Vec<[f32; 2]> {
        let mut out = Vec::with_capacity(x.len());
        for block in x.chunks(512) {
            deck.prepare(c, 0.5, Some(0.0));
            out.extend(block.iter().map(|&f| deck.play(f)));
        }
        out
    }

    /// Level of the `hz` tone in `x` (left channel).
    fn level_at(x: &[[f32; 2]], hz: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, f) in x.iter().enumerate() {
            let w = std::f64::consts::TAU * hz * i as f64 / SR;
            re += f[0] as f64 * w.cos();
            im += f[0] as f64 * w.sin();
        }
        2.0 * (re * re + im * im).sqrt() / x.len() as f64
    }

    #[test]
    fn sources_name_decks_the_mic_and_aux_inputs() {
        assert_eq!(source(1.0), Some(Source::Deck(0)));
        assert_eq!(source(4.0), Some(Source::Deck(3)));
        assert_eq!(source(5.0), Some(Source::Input(MICROPHONE)));
        assert_eq!(source(6.0), Some(Source::Input(AUXILIARIES[0])));
        assert_eq!(source(SOURCES as f64), Some(Source::Input(AUXILIARIES[7])));
        assert_eq!(source(0.0), None);
        assert_eq!(source(SOURCES as f64 + 1.0), None);
        // Deck 2's Remix deck listens to deck 1 by default.
        let (_reg, c, _) = setup();
        assert_eq!(source(c.source.get()), Some(Source::Deck(0)));
    }

    #[test]
    fn an_idle_remix_deck_adds_nothing_or_passes_its_source_bit_identical() {
        let (_reg, c, mut deck) = setup();
        let x = music(24_000);
        let send = run(&mut deck, &c, &x);
        assert!(send.iter().all(|f| *f == [0.0, 0.0]), "Send adds nothing");
        c.mode.set(INSERT);
        let insert = run(&mut deck, &c, &x);
        assert_eq!(insert, x, "Insert passes the source through unchanged");
    }

    #[test]
    fn isolate_left_silences_its_band_only() {
        let (_reg, c, mut deck) = setup();
        c.mode.set(INSERT);
        c.isolate[0].set(-1.0);
        let x = music(48_000);
        let y = run(&mut deck, &c, &x);
        let tail = &y[24_000..];
        assert!(level_at(tail, 80.0) < 0.01, "the bass is gone");
        let high = level_at(tail, 6000.0);
        assert!((high - 0.5).abs() < 0.02, "the highs are unchanged: {high}");
    }

    #[test]
    fn in_send_mode_turning_a_band_down_does_nothing() {
        // The source's own channel still plays it: use Insert for that.
        let (_reg, c, mut deck) = setup();
        c.isolate[0].set(-1.0);
        c.lever_effect.set(4.0);
        c.lever_on[2].set_bool(true);
        let y = run(&mut deck, &c, &music(24_000));
        assert!(y.iter().all(|f| *f == [0.0, 0.0]));
    }

    #[test]
    fn a_lever_echo_on_the_highs_rings_out_after_it_is_let_go() {
        let (_reg, c, mut deck) = setup();
        c.lever_on[2].set_bool(true);
        let x = music(48_000);
        let y = run(&mut deck, &c, &x[..24_000]);
        // Only the highs echo: the Remix deck plays no bass.
        assert!(level_at(&y[12_000..], 80.0) < 0.01);
        assert!(level_at(&y[12_000..], 6000.0) > 0.1, "the highs echo");
        c.lever_on[2].set_bool(false);
        let silence = vec![[0.0f32; 2]; 12_000];
        let tail = run(&mut deck, &c, &silence);
        let ring = tail[..6_000].iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!(ring > 0.05, "the echo rings on: {ring}");
    }

    #[test]
    fn a_solo_lever_turns_the_other_bands_down() {
        let (_reg, c, mut deck) = setup();
        c.mode.set(INSERT);
        c.lever_effect.set(4.0);
        c.lever_on[2].set_bool(true);
        let x = music(48_000);
        let y = run(&mut deck, &c, &x);
        let tail = &y[24_000..];
        assert!(level_at(tail, 80.0) < 0.01, "the bass is muted");
        assert!((level_at(tail, 6000.0) - 0.5).abs() < 0.02);
    }

    #[test]
    fn isolate_right_adds_its_effect_and_goes_back_to_dry() {
        let (_reg, c, mut deck) = setup();
        c.mode.set(INSERT);
        c.isolate_effect.set(2.0);
        c.isolate[0].set(1.0);
        let x = music(48_000);
        let y = run(&mut deck, &c, &x);
        let changed = y[24_000..]
            .iter()
            .zip(&x[24_000..])
            .map(|(a, b)| (a[0] - b[0]).abs())
            .fold(0.0, f32::max);
        assert!(changed > 0.05, "Drive changes the bass: {changed}");
        // Back to centre: after the fade it is the source again, exactly.
        c.isolate[0].set(0.0);
        let y = run(&mut deck, &c, &x);
        assert_eq!(&y[4_096..], &x[4_096..]);
    }
}
