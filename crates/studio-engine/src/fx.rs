//! Live's time and character effects, built in: Delay and Echo, Reverb and
//! Hybrid Reverb, Gate, Overdrive and Roar. Delay, Echo, Reverb, Gate and
//! Overdrive follow the settings a set saves; Hybrid Reverb (its
//! algorithmic side, no convolution) and Roar (its shapers and filters in
//! series, no feedback or modulation) are close approximations. Every
//! buffer is allocated when the device is built, never on the audio thread;
//! a setting changed from the panel retunes the device in place.

use opendj_engine::dsp::{Biquad, Lr4};

use crate::devices::{db, from_db, Compressor, CompressorSettings, Param, Unit};

/// The longest delay a Delay or Echo can hold.
const MAX_DELAY_SECONDS: f32 = 5.0;

/// The synced times a delay's knob steps through, in beats.
pub const NOTES: [f64; 16] = [
    0.0625,
    0.125,
    1.0 / 6.0,
    0.25,
    0.375,
    1.0 / 3.0,
    0.5,
    0.75,
    2.0 / 3.0,
    1.0,
    1.5,
    4.0 / 3.0,
    2.0,
    3.0,
    4.0,
    8.0,
];
pub const NOTE_NAMES: [&str; 16] = [
    "1/64", "1/32", "1/16T", "1/16", "1/16D", "1/8T", "1/8", "1/8D", "1/4T", "1/4", "1/4D", "1/2T",
    "1/2", "1/2D", "1 bar", "2 bars",
];

/// The closest of [`NOTES`] to `beats`.
pub fn nearest_note(beats: f64) -> usize {
    let mut best = 0;
    for (i, n) in NOTES.iter().enumerate() {
        if (n.ln() - beats.max(1e-3).ln()).abs() < (NOTES[best].ln() - beats.max(1e-3).ln()).abs() {
            best = i;
        }
    }
    best
}

/// A synced time as a fraction of a bar (3/16), or its note name.
fn beats_text(b: f64) -> String {
    if let Some(i) = NOTES.iter().position(|n| (n - b).abs() < 1e-6) {
        return NOTE_NAMES[i].into();
    }
    let whole = b / 4.0;
    for d in [1u32, 2, 4, 8, 16, 32, 64, 3, 6, 12, 24, 48] {
        let n = whole * d as f64;
        if n >= 0.5 && (n - n.round()).abs() < 1e-3 {
            return format!("{}/{d}", n.round());
        }
    }
    format!("{b:.2} beats")
}

fn on(v: f32) -> bool {
    v >= 0.5
}

/// Live's Delay or Echo.
#[derive(Clone, Debug, PartialEq)]
pub struct DelaySettings {
    /// Echo rather than Delay (only the summary differs).
    pub echo: bool,
    /// Times follow the tempo.
    pub sync: bool,
    /// Synced times, left and right, in beats.
    pub beats: [f64; 2],
    /// Free times, left and right.
    pub seconds: [f32; 2],
    /// The right side follows the left.
    pub link: bool,
    /// 0..1 (a little over 1 runs away, as in Live).
    pub feedback: f32,
    pub invert: bool,
    pub ping_pong: bool,
    /// High-pass and low-pass on the repeats.
    pub filter_on: bool,
    pub high_pass: f32,
    pub low_pass: f32,
    pub input_db: f32,
    /// Linear gain on the repeats.
    pub output: f32,
    pub dry_wet: f32,
}

impl Default for DelaySettings {
    fn default() -> Self {
        DelaySettings {
            echo: false,
            sync: true,
            beats: [0.75, 1.0],
            seconds: [0.25; 2],
            link: false,
            feedback: 0.5,
            invert: false,
            ping_pong: false,
            filter_on: true,
            high_pass: 60.0,
            low_pass: 8000.0,
            input_db: 0.0,
            output: 1.0,
            dry_wet: 0.3,
        }
    }
}

impl DelaySettings {
    fn side(&self, c: usize) -> usize {
        if self.link {
            0
        } else {
            c
        }
    }

    fn seconds_at(&self, c: usize, tempo: f64) -> f32 {
        let c = self.side(c);
        if self.sync {
            (self.beats[c] * 60.0 / tempo.max(20.0)) as f32
        } else {
            self.seconds[c]
        }
    }

    fn time_text(&self, c: usize) -> String {
        let c = self.side(c);
        if self.sync {
            beats_text(self.beats[c])
        } else {
            format!("{:.0} ms", self.seconds[c] * 1000.0)
        }
    }

    /// The filter as Live's Delay shows it: centre Hz, width in octaves.
    pub fn band(&self) -> (f32, f32) {
        let (h, l) = (self.high_pass.max(1.0), self.low_pass.max(1.0));
        ((h * l).sqrt(), (l / h).log2().max(0.0))
    }

    pub fn set_band(&mut self, freq: f32, width: f32) {
        let half = 2f32.powf(width / 2.0);
        self.high_pass = freq / half;
        self.low_pass = freq * half;
    }

    pub fn summary(&self) -> String {
        let (l, r) = (self.time_text(0), self.time_text(1));
        let mut parts = vec![if l == r { l } else { format!("{l} / {r}") }];
        parts.push(format!("{:.0}% feedback", self.feedback * 100.0));
        if self.ping_pong {
            parts.push("ping pong".into());
        }
        parts.push(format!("{:.0}% wet", self.dry_wet * 100.0));
        parts.join(" · ")
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        let time = |name: &str, c: usize| {
            let c = self.side(c);
            if self.sync {
                Param::notes(name, self.beats[c])
            } else {
                Param::knob(name, self.seconds[c] * 1000.0, 1.0, 5000.0, Unit::Ms).log()
            }
        };
        vec![
            Param::toggle("Sync", self.sync),
            Param::toggle("Link", self.link),
            time("Left", 0),
            time("Right", 1),
            Param::knob("Feedback", self.feedback, 0.0, 1.2, Unit::Percent),
            Param::toggle("Ping Pong", self.ping_pong),
            Param::toggle("Filter", self.filter_on),
            // Live's Delay shows its filter as a band; Echo as two cuts.
            if self.echo {
                Param::knob("High-pass", self.high_pass, 20.0, 20_000.0, Unit::Hz).log()
            } else {
                Param::knob("Freq", self.band().0, 50.0, 18_000.0, Unit::Hz).log()
            },
            if self.echo {
                Param::knob("Low-pass", self.low_pass, 20.0, 20_000.0, Unit::Hz).log()
            } else {
                Param::knob("Width", self.band().1, 0.5, 9.0, Unit::Octaves)
            },
            Param::knob("Dry/Wet", self.dry_wet, 0.0, 1.0, Unit::Percent),
        ]
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.sync = on(v),
            1 => {
                self.link = on(v);
                if self.link {
                    self.beats[1] = self.beats[0];
                    self.seconds[1] = self.seconds[0];
                }
            }
            2 | 3 => {
                let sides = if self.link { 0..2 } else { i - 2..i - 1 };
                for c in sides {
                    if self.sync {
                        self.beats[c] = NOTES[(v.round().max(0.0) as usize).min(NOTES.len() - 1)];
                    } else {
                        self.seconds[c] = (v / 1000.0).clamp(0.001, MAX_DELAY_SECONDS);
                    }
                }
            }
            4 => self.feedback = v.clamp(0.0, 1.2),
            5 => self.ping_pong = on(v),
            6 => self.filter_on = on(v),
            7 if self.echo => self.high_pass = v.clamp(20.0, 20_000.0),
            8 if self.echo => self.low_pass = v.clamp(20.0, 20_000.0),
            7 => self.set_band(v.clamp(50.0, 18_000.0), self.band().1),
            8 => self.set_band(self.band().0, v.clamp(0.5, 9.0)),
            9 => self.dry_wet = v.clamp(0.0, 1.0),
            _ => {}
        }
    }
}

/// One delay line, read at any distance up to its capacity; `len` is the
/// distance its allpass and default reads use, so it can change size
/// without allocating.
struct Line {
    buf: Vec<f32>,
    pos: usize,
    len: usize,
}

impl Line {
    fn new(capacity: usize) -> Line {
        Line {
            buf: vec![0.0; capacity.max(2)],
            pos: 0,
            len: capacity.max(2),
        }
    }

    fn set_len(&mut self, len: usize) {
        self.len = len.clamp(1, self.buf.len());
    }

    /// What was written `d` samples ago (1 is the last write).
    fn tap(&self, d: usize) -> f32 {
        let n = self.buf.len();
        self.buf[(self.pos + n - d.clamp(1, n)) % n]
    }

    fn tap_frac(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, self.buf.len() as f32 - 1.0);
        let i = d.floor();
        let fr = d - i;
        let a = self.tap(i as usize);
        let b = self.tap(i as usize + 1);
        a + (b - a) * fr
    }

    fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % self.buf.len();
    }

    /// A Schroeder allpass `len` long.
    fn allpass(&mut self, x: f32, g: f32) -> f32 {
        let d = self.tap(self.len);
        let w = x + g * d;
        self.push(w);
        d - g * w
    }
}

/// Delay and Echo: two lines, filtered repeats, ping pong.
pub struct Delay {
    s: DelaySettings,
    lines: [Line; 2],
    rate: f64,
    /// Delay now, in samples; glides to a new time.
    time: [f32; 2],
    fresh: bool,
    filters: [Biquad; 2],
    in_gain: f32,
}

impl Delay {
    pub fn new(s: &DelaySettings, rate: f64) -> Delay {
        let len = (rate as f32 * MAX_DELAY_SECONDS) as usize + 4;
        let mut d = Delay {
            s: s.clone(),
            lines: [Line::new(len), Line::new(len)],
            rate,
            time: [1.0; 2],
            fresh: true,
            filters: [Biquad::default(); 2],
            in_gain: 1.0,
        };
        d.retune();
        d
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.retune();
    }

    fn retune(&mut self) {
        let [h, l] = &mut self.filters;
        h.set_highpass(self.rate, self.s.high_pass.max(10.0) as f64, 0.707);
        l.set_lowpass(
            self.rate,
            (self.s.low_pass as f64).min(self.rate * 0.45),
            0.707,
        );
        self.in_gain = from_db(self.s.input_db);
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]], tempo: f64) {
        let max = self.lines[0].buf.len() as f32 - 2.0;
        let rate = self.rate as f32;
        let target = [0, 1].map(|c| (self.s.seconds_at(c, tempo) * rate).clamp(1.0, max));
        if self.fresh {
            self.time = target;
            self.fresh = false;
        }
        let wet = self.s.dry_wet.clamp(0.0, 1.0);
        let fb = self.s.feedback.clamp(0.0, 1.2) * if self.s.invert { -1.0 } else { 1.0 };
        for f in buf.iter_mut() {
            for (t, want) in self.time.iter_mut().zip(target) {
                *t += (want - *t) * 0.0005;
            }
            let mut d = [
                self.lines[0].tap_frac(self.time[0]),
                self.lines[1].tap_frac(self.time[1]),
            ];
            if self.s.filter_on {
                let [h, l] = &mut self.filters;
                d = l.process(h.process(d));
            }
            let x = [f[0] * self.in_gain, f[1] * self.in_gain];
            let back = [d[0] * fb, d[1] * fb];
            // Repeats are soft-limited so feedback over 100% swells but
            // never blows up.
            let (wl, wr) = if self.s.ping_pong {
                ((x[0] + x[1]) * 0.5 + back[1], back[0])
            } else {
                (x[0] + back[0], x[1] + back[1])
            };
            self.lines[0].push(soft(wl));
            self.lines[1].push(soft(wr));
            let o = self.s.output;
            *f = [
                f[0] * (1.0 - wet) + d[0] * o * wet,
                f[1] * (1.0 - wet) + d[1] * o * wet,
            ];
        }
    }
}

/// Clean below about 1, rounding off above.
fn soft(x: f32) -> f32 {
    if x.abs() < 1.0 {
        x
    } else {
        x.signum() * (1.0 + (x.abs() - 1.0).tanh())
    }
}

/// Live's Reverb, or Hybrid Reverb's algorithmic side.
#[derive(Clone, Debug, PartialEq)]
pub struct ReverbSettings {
    /// Hybrid Reverb (only the summary differs).
    pub hybrid: bool,
    pub predelay_ms: f32,
    /// Time to fall 60 dB.
    pub decay_s: f32,
    /// 0 small to 1 large.
    pub size: f32,
    /// 0 bright to 1 dark.
    pub damping: f32,
    pub low_cut_on: bool,
    pub low_cut: f32,
    pub high_cut_on: bool,
    pub high_cut: f32,
    /// 0 mono to 1 full (more widens).
    pub width: f32,
    pub dry_wet: f32,
    /// Hybrid Reverb's impulse response, once found and decoded.
    pub ir: Option<Ir>,
    /// Convolution (0) to algorithm (1).
    pub blend: f32,
    /// How the two sides connect (see [`HYBRID_ROUTINGS`]).
    pub routing: u8,
}

/// Hybrid Reverb's routings, in Live's order.
pub const HYBRID_ROUTINGS: &[&str] = &["Serial", "Parallel", "Algorithm", "Convolution"];

/// An impulse response: where it's from, and its frames at the engine's
/// rate (empty until it's loaded).
#[derive(Clone)]
pub struct Ir {
    pub name: String,
    pub path: std::path::PathBuf,
    pub frames: std::sync::Arc<Vec<[f32; 2]>>,
}

impl std::fmt::Debug for Ir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ir({}, {} frames)", self.name, self.frames.len())
    }
}

impl PartialEq for Ir {
    fn eq(&self, other: &Ir) -> bool {
        self.path == other.path && std::sync::Arc::ptr_eq(&self.frames, &other.frames)
    }
}

impl Default for ReverbSettings {
    fn default() -> Self {
        ReverbSettings {
            hybrid: false,
            predelay_ms: 10.0,
            decay_s: 2.0,
            size: 0.5,
            damping: 0.4,
            low_cut_on: false,
            low_cut: 100.0,
            high_cut_on: false,
            high_cut: 8000.0,
            width: 1.0,
            dry_wet: 0.3,
            ir: None,
            blend: 0.5,
            routing: 1,
        }
    }
}

/// The longest pre-delay.
const MAX_PREDELAY_MS: f32 = 250.0;

impl ReverbSettings {
    pub fn summary(&self) -> String {
        let mut s = format!("{:.1} s · {:.0}% wet", self.decay_s, self.dry_wet * 100.0);
        if self.hybrid {
            match &self.ir {
                Some(ir) if !ir.frames.is_empty() => {
                    s.push_str(&format!(" · {}", ir.name));
                }
                _ => s.push_str(" · no convolution (its room wasn't found)"),
            }
        }
        s
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        let mut p = vec![
            Param::knob(
                "Pre-delay",
                self.predelay_ms,
                0.5,
                MAX_PREDELAY_MS,
                Unit::Ms,
            )
            .log(),
            Param::knob("Decay", self.decay_s, 0.2, 60.0, Unit::Seconds).log(),
            Param::knob("Size", self.size, 0.0, 1.0, Unit::Percent),
            Param::knob("Damping", self.damping, 0.0, 1.0, Unit::Percent),
            Param::knob("Width", self.width, 0.0, 1.2, Unit::Percent),
            Param::toggle("Low Cut", self.low_cut_on),
            Param::knob("Low", self.low_cut, 20.0, 2000.0, Unit::Hz).log(),
            Param::toggle("High Cut", self.high_cut_on),
            Param::knob("High", self.high_cut, 500.0, 20_000.0, Unit::Hz).log(),
            Param::knob("Dry/Wet", self.dry_wet, 0.0, 1.0, Unit::Percent),
        ];
        if self.hybrid {
            p.push(Param::knob("Blend", self.blend, 0.0, 1.0, Unit::Percent));
            p.push(Param::choice(
                "Routing",
                self.routing as usize,
                HYBRID_ROUTINGS,
            ));
        }
        p
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        match i {
            10 => self.blend = v.clamp(0.0, 1.0),
            11 => self.routing = v.round().clamp(0.0, 3.0) as u8,
            0 => self.predelay_ms = v.clamp(0.0, MAX_PREDELAY_MS),
            1 => self.decay_s = v.clamp(0.1, 60.0),
            2 => self.size = v.clamp(0.0, 1.0),
            3 => self.damping = v.clamp(0.0, 1.0),
            4 => self.width = v.max(0.0),
            5 => self.low_cut_on = on(v),
            6 => self.low_cut = v.clamp(20.0, 20_000.0),
            7 => self.high_cut_on = on(v),
            8 => self.high_cut = v.clamp(20.0, 20_000.0),
            9 => self.dry_wet = v.clamp(0.0, 1.0),
            _ => {}
        }
    }
}

/// Dattorro's delay lengths, at 29 761 Hz.
const DIFFUSERS: [usize; 4] = [142, 107, 379, 277];
const TANK: [[usize; 4]; 2] = [[672, 4453, 1800, 3720], [908, 4217, 2656, 3163]];
/// Where the outputs tap the tank (Dattorro's table 2).
const TAPS: [[usize; 7]; 2] = [
    [266, 2974, 1913, 1996, 1990, 187, 1066],
    [353, 3627, 1228, 2673, 2111, 335, 121],
];
/// How far the tank's first allpass swings, at 29 761 Hz.
const EXCURSION: f32 = 16.0;
/// Size 0..1 scales the tank by this range.
const SIZE_SCALE: (f32, f32) = (0.5, 1.5);

/// A plate reverb (Dattorro's figure-eight tank) sized and timed from the
/// set's settings. Its lines are allocated for the largest size, so the
/// size knob only changes how far they read.
pub struct Reverb {
    s: ReverbSettings,
    rate: f64,
    predelay: Line,
    predelay_len: usize,
    cuts: [Biquad; 2],
    input: f32,
    diffusers: [Line; 4],
    tank: [[Line; 4]; 2],
    damped: [f32; 2],
    cross: [f32; 2],
    decay: f32,
    damping: f32,
    taps: [[usize; 7]; 2],
    swing: f32,
    lfo: f32,
    lfo_step: f32,
    /// Hybrid Reverb's convolution side, when its response was found.
    conv: Option<Box<crate::conv::Convolver>>,
}

impl Reverb {
    pub fn new(s: &ReverbSettings, rate: f64) -> Reverb {
        let most = rate as f32 / 29_761.0 * SIZE_SCALE.1;
        let cap = |n: usize| (n as f32 * most) as usize + 4;
        let swing = EXCURSION * most;
        let mut r = Reverb {
            s: s.clone(),
            rate,
            predelay: Line::new((MAX_PREDELAY_MS * 0.001 * rate as f32) as usize + 2),
            predelay_len: 0,
            cuts: [Biquad::default(); 2],
            input: 0.0,
            diffusers: DIFFUSERS.map(|n| Line::new(cap(n))),
            tank: TANK.map(|h| {
                [
                    Line::new(cap(h[0]) + swing as usize + 2),
                    Line::new(cap(h[1])),
                    Line::new(cap(h[2])),
                    Line::new(cap(h[3])),
                ]
            }),
            damped: [0.0; 2],
            cross: [0.0; 2],
            decay: 0.5,
            damping: 0.0,
            taps: [[1; 7]; 2],
            swing: 0.0,
            lfo: 0.0,
            lfo_step: 1.0 / rate as f32,
            conv: s
                .ir
                .as_ref()
                .filter(|ir| !ir.frames.is_empty())
                .map(|ir| Box::new(crate::conv::Convolver::new(&ir.frames, rate))),
        };
        r.retune();
        r
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.retune();
    }

    fn retune(&mut self) {
        let s = &self.s;
        let rate = self.rate as f32;
        let size = SIZE_SCALE.0 + (SIZE_SCALE.1 - SIZE_SCALE.0) * s.size.clamp(0.0, 1.0);
        let scale = rate / 29_761.0 * size;
        let len = |n: usize| ((n as f32 * scale) as usize).max(2);
        self.swing = EXCURSION * scale;
        for (d, n) in self.diffusers.iter_mut().zip(DIFFUSERS) {
            d.set_len(len(n));
        }
        for (half, tuning) in self.tank.iter_mut().zip(TANK) {
            half[0].set_len(len(tuning[0]) + self.swing as usize + 2);
            for (l, n) in half[1..].iter_mut().zip(&tuning[1..]) {
                l.set_len(len(*n));
            }
        }
        self.taps = TAPS.map(|side| side.map(len));
        // The tank's loop takes this long; the decay per pass that makes
        // 60 dB in the set's time (it's applied twice in each half).
        let loop_s: f32 = TANK.iter().flatten().sum::<usize>() as f32 * scale / rate;
        self.decay = 0.001f32.powf(loop_s / (4.0 * s.decay_s.max(0.1))).min(0.97);
        self.damping = s.damping.clamp(0.0, 1.0) * 0.8;
        self.predelay_len = (s.predelay_ms.max(0.0) * 0.001 * rate) as usize;
        let nyq = self.rate * 0.45;
        let [lo, hi] = &mut self.cuts;
        lo.set_highpass(self.rate, (s.low_cut as f64).clamp(20.0, nyq), 0.707);
        hi.set_lowpass(self.rate, (s.high_cut as f64).clamp(20.0, nyq), 0.707);
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        let wet = self.s.dry_wet.clamp(0.0, 1.0);
        let width = self.s.width.max(0.0);
        for f in buf.iter_mut() {
            let mut x = [(f[0] + f[1]) * 0.5; 2];
            if self.s.low_cut_on {
                x = self.cuts[0].process(x);
            }
            if self.s.high_cut_on {
                x = self.cuts[1].process(x);
            }
            self.predelay.push(x[0]);
            let pre = self.predelay.tap(self.predelay_len + 1);
            let blend = self.s.blend.clamp(0.0, 1.0);
            let [l, r] = match (&mut self.conv, self.s.routing) {
                (None, _) | (_, 2) => self.plate(pre),
                (Some(c), routing) => {
                    // The convolution starts early by its latency, so the
                    // room lines up with the pre-delay.
                    let early = (self.predelay_len + 1).saturating_sub(c.latency()).max(1);
                    let room = c.process(self.predelay.tap(early));
                    match routing {
                        3 => room,
                        0 => {
                            let algo = self.plate((room[0] + room[1]) * 0.5);
                            mix2(room, algo, blend)
                        }
                        _ => {
                            let algo = self.plate(pre);
                            mix2(room, algo, blend)
                        }
                    }
                }
            };
            let (mid, side) = ((l + r) * 0.5, (l - r) * 0.5 * width);
            let out = [mid + side, mid - side];
            *f = [
                f[0] * (1.0 - wet) + out[0] * wet,
                f[1] * (1.0 - wet) + out[1] * wet,
            ];
        }
    }

    /// The plate (Dattorro's tank) for one pre-delayed input sample.
    fn plate(&mut self, x: f32) -> [f32; 2] {
        self.input += (x - self.input) * 0.9995;
        let mut v = self.input;
        for (d, g) in self.diffusers.iter_mut().zip([0.75, 0.75, 0.625, 0.625]) {
            v = d.allpass(v, g);
        }
        self.lfo += self.lfo_step;
        if self.lfo >= 1.0 {
            self.lfo -= 1.0;
        }
        let (sin, cos) = (self.lfo * std::f32::consts::TAU).sin_cos();
        for half in 0..2 {
            let m = if half == 0 { sin } else { cos };
            let input = v + self.cross[1 - half] * self.decay;
            let [ap1, d1, ap2, d2] = &mut self.tank[half];
            let base = ap1.len as f32 - self.swing - 2.0;
            let delayed = ap1.tap_frac(base + self.swing * m);
            let w = input + 0.7 * delayed;
            ap1.push(w);
            d1.push(delayed - 0.7 * w);
            let t = d1.tap(d1.len);
            self.damped[half] += (t - self.damped[half]) * (1.0 - self.damping);
            let b = ap2.allpass(self.damped[half] * self.decay, 0.5);
            d2.push(b);
            self.cross[half] = d2.tap(d2.len);
        }
        let t = &self.taps;
        let [_, l2, l3, l4] = &self.tank[0];
        let [_, r2, r3, r4] = &self.tank[1];
        let l = r2.tap(t[0][0]) + r2.tap(t[0][1]) - r3.tap(t[0][2]) + r4.tap(t[0][3])
            - l2.tap(t[0][4])
            - l3.tap(t[0][5])
            - l4.tap(t[0][6]);
        let r = l2.tap(t[1][0]) + l2.tap(t[1][1]) - l3.tap(t[1][2]) + l4.tap(t[1][3])
            - r2.tap(t[1][4])
            - r3.tap(t[1][5])
            - r4.tap(t[1][6]);
        [l * 0.6, r * 0.6]
    }
}

/// Convolution (blend 0) to algorithm (1).
fn mix2(a: [f32; 2], b: [f32; 2], blend: f32) -> [f32; 2] {
    [
        a[0] * (1.0 - blend) + b[0] * blend,
        a[1] * (1.0 - blend) + b[1] * blend,
    ]
}

/// Live's Gate.
#[derive(Clone, Debug, PartialEq)]
pub struct GateSettings {
    /// Linear level.
    pub threshold: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    /// How far under the threshold it closes again (hysteresis), dB.
    pub return_db: f32,
    /// How far a closed gate turns it down, dB (Live's Floor).
    pub floor_db: f32,
    /// Lets through only what's under the threshold.
    pub flip: bool,
}

impl Default for GateSettings {
    fn default() -> Self {
        GateSettings {
            threshold: from_db(-30.0),
            attack_ms: 0.1,
            hold_ms: 10.0,
            release_ms: 15.0,
            return_db: 3.0,
            floor_db: -40.0,
            flip: false,
        }
    }
}

impl GateSettings {
    pub fn summary(&self) -> String {
        format!(
            "{:.1} dB · floor {:.0} dB{}",
            db(self.threshold),
            self.floor_db,
            if self.flip { " · flipped" } else { "" }
        )
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        vec![
            Param::knob("Threshold", db(self.threshold), -70.0, 0.0, Unit::Db),
            Param::knob("Return", self.return_db, 0.0, 24.0, Unit::Db),
            Param::knob("Attack", self.attack_ms, 0.02, 150.0, Unit::Ms).log(),
            Param::knob("Hold", self.hold_ms, 0.1, 1000.0, Unit::Ms).log(),
            Param::knob("Release", self.release_ms, 1.0, 3000.0, Unit::Ms).log(),
            Param::knob("Floor", self.floor_db, -80.0, 0.0, Unit::Db),
            Param::toggle("Flip", self.flip),
        ]
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.threshold = from_db(v),
            1 => self.return_db = v.max(0.0),
            2 => self.attack_ms = v.max(0.02),
            3 => self.hold_ms = v.max(0.0),
            4 => self.release_ms = v.max(1.0),
            5 => self.floor_db = v.min(0.0),
            6 => self.flip = on(v),
            _ => {}
        }
    }
}

pub struct Gate {
    s: GateSettings,
    rate: f32,
    close_at: f32,
    floor: f32,
    attack: f32,
    release: f32,
    hold: usize,
    held: usize,
    open: bool,
    level: f32,
    gain: f32,
    level_fall: f32,
}

impl Gate {
    pub fn new(s: &GateSettings, rate: f64) -> Gate {
        let mut g = Gate {
            s: s.clone(),
            rate: rate as f32,
            close_at: 0.0,
            floor: 0.0,
            attack: 0.0,
            release: 0.0,
            hold: 0,
            held: 0,
            open: false,
            level: 0.0,
            gain: 0.0,
            level_fall: 0.0,
        };
        g.retune();
        g.gain = if s.flip { 1.0 } else { g.floor };
        g
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.retune();
    }

    fn retune(&mut self) {
        let rate = self.rate;
        let coef = |ms: f32| (-1.0 / (ms.max(0.02) * 0.001 * rate)).exp();
        let s = &self.s;
        self.floor = if s.floor_db <= -79.9 {
            0.0
        } else {
            from_db(s.floor_db)
        };
        self.close_at = s.threshold * from_db(-s.return_db.max(0.0));
        self.attack = coef(s.attack_ms);
        self.release = coef(s.release_ms);
        self.hold = (s.hold_ms.max(0.0) * 0.001 * rate) as usize;
        self.level_fall = coef(10.0);
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        for f in buf.iter_mut() {
            let peak = f[0].abs().max(f[1].abs());
            self.level = peak.max(self.level * self.level_fall);
            if self.level >= self.s.threshold {
                self.open = true;
                self.held = self.hold;
            } else if self.open && self.level < self.close_at {
                if self.held == 0 {
                    self.open = false;
                } else {
                    self.held -= 1;
                }
            }
            let pass = self.open != self.s.flip;
            let (want, c) = if pass {
                (1.0, self.attack)
            } else {
                (self.floor, self.release)
            };
            self.gain = want + (self.gain - want) * c;
            *f = [f[0] * self.gain, f[1] * self.gain];
        }
    }
}

/// Live's Overdrive.
#[derive(Clone, Debug, PartialEq)]
pub struct OverdriveSettings {
    /// The band that's driven: centre Hz and width in octaves.
    pub mid_freq: f32,
    pub bandwidth: f32,
    /// 0..1.
    pub drive: f32,
    /// 0 dark to 1 bright.
    pub tone: f32,
    /// 0..1.
    pub dry_wet: f32,
}

impl Default for OverdriveSettings {
    fn default() -> Self {
        OverdriveSettings {
            mid_freq: 1600.0,
            bandwidth: 3.0,
            drive: 0.5,
            tone: 0.5,
            dry_wet: 1.0,
        }
    }
}

impl OverdriveSettings {
    pub fn summary(&self) -> String {
        format!(
            "drive {:.0}% · tone {:.0}% · {:.0}% wet",
            self.drive * 100.0,
            self.tone * 100.0,
            self.dry_wet * 100.0
        )
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        vec![
            Param::knob("Filter", self.mid_freq, 50.0, 20_000.0, Unit::Hz).log(),
            Param::knob("Width", self.bandwidth, 0.5, 9.0, Unit::Octaves),
            Param::knob("Drive", self.drive, 0.0, 1.0, Unit::Percent),
            Param::knob("Tone", self.tone, 0.0, 1.0, Unit::Percent),
            Param::knob("Dry/Wet", self.dry_wet, 0.0, 1.0, Unit::Percent),
        ]
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.mid_freq = v.clamp(20.0, 20_000.0),
            1 => self.bandwidth = v.clamp(0.5, 9.0),
            2 => self.drive = v.clamp(0.0, 1.0),
            3 => self.tone = v.clamp(0.0, 1.0),
            4 => self.dry_wet = v.clamp(0.0, 1.0),
            _ => {}
        }
    }
}

pub struct Overdrive {
    s: OverdriveSettings,
    rate: f64,
    band: [Biquad; 2],
    tone: Biquad,
    drive: f32,
}

impl Overdrive {
    pub fn new(s: &OverdriveSettings, rate: f64) -> Overdrive {
        let mut o = Overdrive {
            s: s.clone(),
            rate,
            band: [Biquad::default(); 2],
            tone: Biquad::default(),
            drive: 1.0,
        };
        o.retune();
        o
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.retune();
    }

    fn retune(&mut self) {
        let s = &self.s;
        let half = 2f32.powf(s.bandwidth.clamp(0.5, 9.0) / 2.0);
        let nyq = self.rate as f32 * 0.45;
        let [hp, lp] = &mut self.band;
        hp.set_highpass(
            self.rate,
            (s.mid_freq / half).clamp(20.0, nyq) as f64,
            0.707,
        );
        lp.set_lowpass(
            self.rate,
            (s.mid_freq * half).clamp(20.0, nyq) as f64,
            0.707,
        );
        // Tone sweeps a low-pass from 500 Hz to 16 kHz.
        let tone_hz = 500.0 * 2f32.powf(s.tone.clamp(0.0, 1.0) * 5.0);
        self.tone
            .set_lowpass(self.rate, tone_hz.min(nyq) as f64, 0.707);
        self.drive = from_db(s.drive.clamp(0.0, 1.0) * 40.0);
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        let wet = self.s.dry_wet.clamp(0.0, 1.0);
        for f in buf.iter_mut() {
            let [h, l] = &mut self.band;
            let x = l.process(h.process(*f));
            let y = self.tone.process(x.map(|v| (v * self.drive).tanh()));
            *f = [
                f[0] * (1.0 - wet) + y[0] * wet,
                f[1] * (1.0 - wet) + y[1] * wet,
            ];
        }
    }
}

/// Roar, approximated: its three shaper stages in series, each with its
/// low-pass, then its compressor.
#[derive(Clone, Debug, PartialEq)]
pub struct RoarSettings {
    pub input_db: f32,
    /// How the stages connect, in Live's order (see [`ROUTINGS`]).
    pub routing: u8,
    /// Multi Band's crossovers, Hz.
    pub low_mid: f32,
    pub mid_high: f32,
    pub stages: [RoarStage; 3],
    /// 0..1.
    pub compression: f32,
    pub output_db: f32,
    pub dry_wet: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoarStage {
    pub on: bool,
    /// 0..1.
    pub amount: f32,
    /// -1..1.
    pub bias: f32,
    /// Low-pass, Hz; at [`ROAR_OPEN`] or over it's open.
    pub lowpass: f32,
    /// Live's shaper type, an index into [`ROAR_SHAPES`].
    pub shape: u8,
}

/// Roar's Tri Fold shaper type.
pub const ROAR_TRI_FOLD: u8 = 9;

/// One Roar shaper curve, by its index in [`ROAR_SHAPES`].
fn roar_shape(shape: u8, x: f32) -> f32 {
    use std::f32::consts::FRAC_PI_2;
    match shape {
        0 => x.clamp(-FRAC_PI_2, FRAC_PI_2).sin(),
        1 => x.clamp(-1.0, 1.0),
        5 => x.max(0.0).tanh(),
        6 => x.abs().tanh(),
        ROAR_TRI_FOLD => tri_fold(x),
        _ => x.tanh(),
    }
}

/// Folds back into -1..1 like a triangle wave.
fn tri_fold(x: f32) -> f32 {
    1.0 - 4.0 * (((x + 1.0) / 4.0).rem_euclid(1.0) - 0.5).abs()
}

/// A Roar stage's low-pass this high is left out.
pub const ROAR_OPEN: f32 = 19_000.0;

impl Default for RoarStage {
    fn default() -> Self {
        RoarStage {
            on: false,
            amount: 0.0,
            bias: 0.0,
            lowpass: 20_000.0,
            shape: 0,
        }
    }
}

impl Default for RoarSettings {
    fn default() -> Self {
        RoarSettings {
            input_db: 0.0,
            routing: 1,
            low_mid: 200.0,
            mid_high: 2000.0,
            stages: [
                RoarStage {
                    on: true,
                    amount: 0.3,
                    ..RoarStage::default()
                },
                RoarStage::default(),
                RoarStage::default(),
            ],
            compression: 0.0,
            output_db: 0.0,
            dry_wet: 1.0,
        }
    }
}

const STAGE_PARAMS: usize = 4;

/// Roar's routings, in Live's order; Feedback and Delay play as Serial here.
pub const ROUTINGS: &[&str] = &[
    "Single",
    "Serial",
    "Parallel",
    "Multi Band",
    "Mid Side",
    "Feedback",
    "Delay",
];

/// Roar's shaper types, in Live's order. Soft Sine, Hard Clip, the two
/// rectifiers and Tri Fold are their own curves here; the rest saturate.
pub const ROAR_SHAPES: &[&str] = &[
    "Soft Sine",
    "Hard Clip",
    "Bit Crusher",
    "Diode Clipper",
    "Tube Preamp",
    "Half Wave Rectifier",
    "Full Wave Rectifier",
    "Polynomial",
    "Fractal",
    "Tri Fold",
    "Noise Injection",
    "Shards",
];

impl RoarSettings {
    pub fn summary(&self) -> String {
        let shapes: Vec<&str> = self
            .stages
            .iter()
            .filter(|s| s.on)
            .map(|s| *ROAR_SHAPES.get(s.shape as usize).unwrap_or(&"Soft Sine"))
            .collect();
        format!(
            "{} · {} · approximation",
            ROUTINGS.get(self.routing as usize).unwrap_or(&"Serial"),
            if shapes.is_empty() {
                "no stages".to_string()
            } else {
                shapes.join(", ")
            }
        )
    }

    pub(crate) fn params(&self) -> Vec<Param> {
        let mut p = vec![Param::knob("Input", self.input_db, -24.0, 24.0, Unit::Db).bipolar()];
        for (i, s) in self.stages.iter().enumerate() {
            let n = i + 1;
            p.push(Param::toggle(format!("Stage {n}"), s.on));
            p.push(Param::knob(
                format!("Drive {n}"),
                s.amount,
                0.0,
                1.0,
                Unit::Percent,
            ));
            p.push(Param::knob(format!("Bias {n}"), s.bias, -1.0, 1.0, Unit::Plain).bipolar());
            p.push(Param::knob(format!("Tone {n}"), s.lowpass, 20.0, 20_000.0, Unit::Hz).log());
        }
        p.push(Param::knob(
            "Compress",
            self.compression,
            0.0,
            1.0,
            Unit::Percent,
        ));
        p.push(Param::knob("Output", self.output_db, -24.0, 24.0, Unit::Db).bipolar());
        p.push(Param::knob(
            "Dry/Wet",
            self.dry_wet,
            0.0,
            1.0,
            Unit::Percent,
        ));
        p.push(Param::choice("Routing", self.routing as usize, ROUTINGS));
        p.push(Param::knob("Low/Mid", self.low_mid, 50.0, 1000.0, Unit::Hz).log());
        p.push(Param::knob("Mid/High", self.mid_high, 500.0, 10_000.0, Unit::Hz).log());
        p
    }

    pub(crate) fn set(&mut self, i: usize, v: f32) {
        if i == 0 {
            self.input_db = v;
            return;
        }
        if let Some(s) = self.stages.get_mut((i - 1) / STAGE_PARAMS) {
            match (i - 1) % STAGE_PARAMS {
                0 => s.on = on(v),
                1 => s.amount = v.clamp(0.0, 1.0),
                2 => s.bias = v.clamp(-1.0, 1.0),
                _ => s.lowpass = v.clamp(20.0, 20_000.0),
            }
            return;
        }
        match i - 1 - 3 * STAGE_PARAMS {
            0 => self.compression = v.clamp(0.0, 1.0),
            1 => self.output_db = v,
            2 => self.dry_wet = v.clamp(0.0, 1.0),
            3 => self.routing = v.round().clamp(0.0, 5.0) as u8,
            4 => self.low_mid = v.clamp(20.0, 5000.0),
            5 => self.mid_high = v.clamp(100.0, 20_000.0),
            _ => {}
        }
    }
}

pub struct Roar {
    s: RoarSettings,
    rate: f64,
    /// Per stage: drive, bias, its low-pass.
    stages: [(f32, f32, Biquad); 3],
    input: f32,
    output: f32,
    comp: Compressor,
    /// Multi Band's split: low, the rest, then mid and high from the rest.
    split: [Lr4; 4],
}

impl Roar {
    pub fn new(s: &RoarSettings, rate: f64) -> Roar {
        let mut r = Roar {
            s: s.clone(),
            rate,
            stages: [(1.0, 0.0, Biquad::default()); 3],
            input: 1.0,
            output: 1.0,
            comp: Compressor::new(
                &CompressorSettings {
                    threshold: 1.0,
                    ratio: 1.0,
                    attack_ms: 3.0,
                    release_ms: 80.0,
                    makeup_db: 0.0,
                    knee_db: 6.0,
                    dry_wet: 1.0,
                },
                rate,
            ),
            split: [
                Lr4::lowpass(rate, 200.0),
                Lr4::highpass(rate, 200.0),
                Lr4::lowpass(rate, 2000.0),
                Lr4::highpass(rate, 2000.0),
            ],
        };
        r.retune();
        r
    }

    pub fn set(&mut self, i: usize, v: f32) {
        self.s.set(i, v);
        self.retune();
    }

    fn retune(&mut self) {
        let nyq = self.rate as f32 * 0.45;
        for (st, (drive, bias, lp)) in self.s.stages.iter().zip(self.stages.iter_mut()) {
            *drive = from_db(st.amount.clamp(0.0, 1.0) * 36.0);
            *bias = st.bias.clamp(-1.0, 1.0) * 0.5;
            lp.set_lowpass(self.rate, st.lowpass.clamp(20.0, nyq) as f64, 0.707);
        }
        let c = self.s.compression.clamp(0.0, 1.0);
        self.comp.s.threshold = from_db(-24.0 * c);
        self.comp.s.ratio = 1.0 + 5.0 * c;
        self.comp.retune();
        self.input = from_db(self.s.input_db);
        self.output = from_db(self.s.output_db);
        let (lo, hi) = (self.s.low_mid as f64, self.s.mid_high as f64);
        let [a, b, c, d] = &mut self.split;
        a.set_lowpass(self.rate, lo);
        b.set_highpass(self.rate, lo);
        c.set_lowpass(self.rate, hi);
        d.set_highpass(self.rate, hi);
    }

    /// One stage's shaper and low-pass, if it's on.
    fn stage(&mut self, i: usize, x: [f32; 2]) -> [f32; 2] {
        let st = self.s.stages[i];
        if !st.on {
            return x;
        }
        let (drive, bias, lp) = &mut self.stages[i];
        let off = roar_shape(st.shape, *bias);
        let y = x.map(|v| roar_shape(st.shape, v * *drive + *bias) - off);
        if st.lowpass < ROAR_OPEN {
            lp.process(y)
        } else {
            y
        }
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        let wet = self.s.dry_wet.clamp(0.0, 1.0);
        let compress = self.s.compression > 0.0;
        for f in buf.iter_mut() {
            let dry = *f;
            let x = [f[0] * self.input, f[1] * self.input];
            let x = match self.s.routing {
                0 => self.stage(0, x),
                2 => {
                    let n = self.s.stages.iter().filter(|s| s.on).count().max(1) as f32;
                    let mut sum = [0.0; 2];
                    for i in 0..3 {
                        if self.s.stages[i].on {
                            let y = self.stage(i, x);
                            sum = [sum[0] + y[0] / n, sum[1] + y[1] / n];
                        }
                    }
                    sum
                }
                3 => {
                    let low = self.split[0].process(x);
                    let rest = self.split[1].process(x);
                    let mid = self.split[2].process(rest);
                    let high = self.split[3].process(rest);
                    let [l, m, h] = [self.stage(0, low), self.stage(1, mid), self.stage(2, high)];
                    [l[0] + m[0] + h[0], l[1] + m[1] + h[1]]
                }
                4 => {
                    let (mid, side) = ((x[0] + x[1]) * 0.5, (x[0] - x[1]) * 0.5);
                    let m = self.stage(0, [mid; 2])[0];
                    let sd = self.stage(1, [side; 2])[0];
                    [m + sd, m - sd]
                }
                _ => {
                    let mut y = x;
                    for i in 0..3 {
                        y = self.stage(i, y);
                    }
                    y
                }
            };
            *f = x;
            if compress {
                self.comp.process(std::slice::from_mut(f));
            }
            *f = [
                dry[0] * (1.0 - wet) + f[0] * self.output * wet,
                dry[1] * (1.0 - wet) + f[1] * self.output * wet,
            ];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(n: usize) -> Vec<[f32; 2]> {
        let mut v = vec![[0.0; 2]; n];
        v[0] = [1.0, 1.0];
        v
    }

    fn energy(buf: &[[f32; 2]]) -> f32 {
        buf.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum()
    }

    #[test]
    fn synced_delay_repeats_on_the_beat_and_ping_pongs() {
        let s = DelaySettings {
            sync: true,
            beats: [0.5; 2],
            feedback: 0.5,
            ping_pong: true,
            filter_on: false,
            dry_wet: 1.0,
            ..DelaySettings::default()
        };
        assert_eq!(s.summary(), "1/8 · 50% feedback · ping pong · 100% wet");
        let mut d = Delay::new(&s, 48_000.0);
        let mut buf = click(48_000);
        d.process(&mut buf, 120.0);
        // An eighth at 120 is 250 ms: first repeat left, then right.
        assert!((buf[12_000][0] - 1.0).abs() < 1e-3, "{:?}", buf[12_000]);
        assert!(buf[12_000][1].abs() < 1e-3);
        assert!((buf[24_000][1] - 0.5).abs() < 1e-3, "{:?}", buf[24_000]);
        assert!(buf[100][0].abs() < 1e-6, "fully wet: no dry click");
    }

    #[test]
    fn a_linked_delay_moves_both_sides() {
        let mut s = DelaySettings {
            link: true,
            ..DelaySettings::default()
        };
        s.set(3, 9.0);
        assert_eq!(s.beats, [1.0, 1.0]);
        assert_eq!(s.summary().split(" · ").next(), Some("1/4"));
    }

    #[test]
    fn reverb_tail_lasts_about_its_decay_time() {
        let tail = |decay_s| {
            let s = ReverbSettings {
                decay_s,
                damping: 0.3,
                dry_wet: 1.0,
                ..ReverbSettings::default()
            };
            let mut r = Reverb::new(&s, 48_000.0);
            let mut buf = click(48_000 * 3);
            r.process(&mut buf);
            let early = energy(&buf[..24_000]);
            let late = energy(&buf[48_000 * 2..]);
            assert!(early > 0.0 && late.is_finite());
            late / early
        };
        let short = tail(0.5);
        let long = tail(4.0);
        assert!(short < 1e-6, "{short}");
        assert!(long > 1e-4, "{long}");
    }

    #[test]
    fn reverb_size_changes_without_losing_its_tail() {
        let s = ReverbSettings {
            decay_s: 4.0,
            dry_wet: 1.0,
            ..ReverbSettings::default()
        };
        let mut r = Reverb::new(&s, 48_000.0);
        let mut buf = click(24_000);
        r.process(&mut buf);
        r.set(2, 1.0);
        let mut after = vec![[0.0; 2]; 24_000];
        r.process(&mut after);
        let e = energy(&after);
        assert!(e > 1e-4 && e.is_finite(), "{e}");
    }

    #[test]
    fn hybrid_plays_its_room_on_time() {
        // A room that's one echo 1000 samples in.
        let mut frames = vec![[0.0; 2]; 2000];
        frames[1000] = [0.5, 0.25];
        let s = ReverbSettings {
            hybrid: true,
            predelay_ms: 10.0,
            dry_wet: 1.0,
            routing: 3,
            ir: Some(Ir {
                name: "Test Room".into(),
                path: "test.aif".into(),
                frames: std::sync::Arc::new(frames),
            }),
            ..ReverbSettings::default()
        };
        assert!(s.summary().ends_with("Test Room"));
        let mut r = Reverb::new(&s, 48_000.0);
        let mut buf = click(4000);
        r.process(&mut buf);
        // 10 ms of pre-delay (480 samples) plus the room's echo, the convolution's
        // latency hidden inside the pre-delay.
        let (at, peak) = buf
            .iter()
            .enumerate()
            .max_by(|a, b| a.1[0].abs().total_cmp(&b.1[0].abs()))
            .unwrap();
        assert_eq!(at, 1480, "{peak:?}");
        assert!((peak[0] - 0.5).abs() < 0.01 && (peak[1] - 0.25).abs() < 0.01);
    }

    #[test]
    fn gate_shuts_quiet_sound_and_lets_loud_through() {
        let s = GateSettings {
            threshold: from_db(-30.0),
            attack_ms: 0.1,
            hold_ms: 5.0,
            release_ms: 5.0,
            ..GateSettings::default()
        };
        let mut g = Gate::new(&s, 48_000.0);
        let mut quiet = vec![[0.01, 0.01]; 4_800];
        g.process(&mut quiet);
        assert!(quiet[4_799][0] < 0.01 * 0.011);
        let mut loud = vec![[0.5, 0.5]; 4_800];
        g.process(&mut loud);
        assert!((loud[4_799][0] - 0.5).abs() < 1e-3);
    }

    #[test]
    fn overdrive_and_roar_stay_bounded() {
        let sine: Vec<[f32; 2]> = (0..48_000).map(|i| [(i as f32 * 0.05).sin(); 2]).collect();
        let mut o = Overdrive::new(
            &OverdriveSettings {
                mid_freq: 1600.0,
                bandwidth: 3.3,
                drive: 1.0,
                tone: 0.5,
                dry_wet: 1.0,
            },
            48_000.0,
        );
        let mut a = sine.clone();
        o.process(&mut a);
        let peak = a.iter().fold(0f32, |m, f| m.max(f[0].abs()));
        assert!(peak > 0.1 && peak < 2.0, "{peak}");
        let mut r = Roar::new(
            &RoarSettings {
                stages: [
                    RoarStage {
                        on: true,
                        amount: 0.5,
                        bias: -0.3,
                        lowpass: 8_000.0,
                        shape: ROAR_TRI_FOLD,
                    },
                    RoarStage::default(),
                    RoarStage::default(),
                ],
                compression: 0.25,
                ..RoarSettings::default()
            },
            48_000.0,
        );
        let mut b = sine;
        r.process(&mut b);
        let peak = b.iter().fold(0f32, |m, f| m.max(f[0].abs()));
        assert!(peak > 0.1 && peak < 2.0, "{peak}");
    }
}
