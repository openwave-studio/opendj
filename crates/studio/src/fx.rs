//! Live's time and character effects, built in: Delay and Echo, Reverb and
//! Hybrid Reverb, Gate, Overdrive and Roar. Delay, Echo, Reverb, Gate and
//! Overdrive follow the settings a set saves; Hybrid Reverb (its
//! algorithmic side, no convolution) and Roar (its shapers and filters in
//! series, no feedback or modulation) are close approximations. Every
//! buffer is allocated when the device is built, never on the audio thread.

use opendj_engine::dsp::Biquad;

use crate::devices::{db, from_db, Compressor, CompressorSettings};

/// The longest delay a Delay or Echo can hold.
const MAX_DELAY_SECONDS: f32 = 5.0;

/// A delay time: synced to the tempo, or free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Time {
    Beats(f64),
    Seconds(f32),
}

impl Time {
    fn seconds(self, tempo: f64) -> f32 {
        match self {
            Time::Beats(b) => (b * 60.0 / tempo.max(20.0)) as f32,
            Time::Seconds(s) => s,
        }
    }

    fn text(self) -> String {
        match self {
            Time::Seconds(s) => format!("{:.0} ms", s * 1000.0),
            Time::Beats(b) => {
                let whole = b / 4.0;
                for d in [1u32, 2, 4, 8, 16, 32, 64, 3, 6, 12, 24, 48] {
                    let n = whole * d as f64;
                    if n >= 0.5 && (n - n.round()).abs() < 1e-3 {
                        return format!("{}/{d}", n.round());
                    }
                }
                format!("{b:.2} beats")
            }
        }
    }
}

/// Live's Delay or Echo.
#[derive(Clone, Debug, PartialEq)]
pub struct DelaySettings {
    /// Echo rather than Delay (only the summary differs).
    pub echo: bool,
    pub time: [Time; 2],
    /// 0..1 (a little over 1 runs away, as in Live).
    pub feedback: f32,
    pub invert: bool,
    pub ping_pong: bool,
    /// High-pass and low-pass on the repeats, Hz.
    pub filter: Option<(f32, f32)>,
    pub input_db: f32,
    /// Linear gain on the repeats.
    pub output: f32,
    pub dry_wet: f32,
}

impl DelaySettings {
    pub fn summary(&self) -> String {
        let [l, r] = self.time;
        let mut parts = vec![if l == r {
            l.text()
        } else {
            format!("{} / {}", l.text(), r.text())
        }];
        parts.push(format!("{:.0}% feedback", self.feedback * 100.0));
        if self.ping_pong {
            parts.push("ping pong".into());
        }
        parts.push(format!("{:.0}% wet", self.dry_wet * 100.0));
        parts.join(" · ")
    }
}

/// One delay line, read at any distance up to its length.
struct Line {
    buf: Vec<f32>,
    pos: usize,
}

impl Line {
    fn new(len: usize) -> Line {
        Line {
            buf: vec![0.0; len.max(2)],
            pos: 0,
        }
    }

    /// What was written `d` samples ago (1 is the last write).
    fn tap(&self, d: usize) -> f32 {
        let n = self.buf.len();
        self.buf[(self.pos + n - d.clamp(1, n)) % n]
    }

    fn tap_frac(&self, d: f32) -> f32 {
        let i = d.max(1.0).floor();
        let fr = d.max(1.0) - i;
        let a = self.tap(i as usize);
        let b = self.tap(i as usize + 1);
        a + (b - a) * fr
    }

    fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % self.buf.len();
    }

    /// A Schroeder allpass the length of the line.
    fn allpass(&mut self, x: f32, g: f32) -> f32 {
        let d = self.tap(self.buf.len());
        let w = x + g * d;
        self.push(w);
        d - g * w
    }
}

/// Delay and Echo: two lines, filtered repeats, ping pong.
pub struct Delay {
    s: DelaySettings,
    lines: [Line; 2],
    rate: f32,
    /// Delay now, in samples; glides to a new tempo's time.
    time: [f32; 2],
    fresh: bool,
    filters: Option<[Biquad; 2]>,
    in_gain: f32,
}

impl Delay {
    pub fn new(s: &DelaySettings, rate: f64) -> Delay {
        let len = (rate as f32 * MAX_DELAY_SECONDS) as usize + 4;
        let filters = s.filter.map(|(hp, lp)| {
            let (mut h, mut l) = (Biquad::default(), Biquad::default());
            h.set_highpass(rate, hp.max(10.0) as f64, 0.707);
            l.set_lowpass(rate, lp.min(rate as f32 * 0.45) as f64, 0.707);
            [h, l]
        });
        Delay {
            s: s.clone(),
            lines: [Line::new(len), Line::new(len)],
            rate: rate as f32,
            time: [1.0; 2],
            fresh: true,
            filters,
            in_gain: from_db(s.input_db),
        }
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]], tempo: f64) {
        let max = self.lines[0].buf.len() as f32 - 2.0;
        let target = self
            .s
            .time
            .map(|t| (t.seconds(tempo) * self.rate).clamp(1.0, max));
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
            if let Some([h, l]) = &mut self.filters {
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
    pub low_cut: Option<f32>,
    pub high_cut: Option<f32>,
    /// 0 mono to 1 full (more widens).
    pub width: f32,
    pub dry_wet: f32,
}

impl ReverbSettings {
    pub fn summary(&self) -> String {
        let mut s = format!("{:.1} s · {:.0}% wet", self.decay_s, self.dry_wet * 100.0);
        if self.hybrid {
            s.push_str(" · approximation, no convolution");
        }
        s
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

/// A plate reverb (Dattorro's figure-eight tank) sized and timed from the
/// set's settings.
pub struct Reverb {
    s: ReverbSettings,
    predelay: Line,
    predelay_len: usize,
    cuts: [Option<Biquad>; 2],
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
}

impl Reverb {
    pub fn new(s: &ReverbSettings, rate: f64) -> Reverb {
        let size = 0.5 + s.size.clamp(0.0, 1.0);
        let scale = rate as f32 / 29_761.0 * size;
        let len = |n: usize| ((n as f32 * scale) as usize).max(2);
        let swing = EXCURSION * scale;
        let tank = TANK.map(|h| {
            [
                Line::new(len(h[0]) + swing as usize + 2),
                Line::new(len(h[1])),
                Line::new(len(h[2])),
                Line::new(len(h[3])),
            ]
        });
        // The tank's loop takes this long; the decay per half-loop that
        // makes 60 dB in the set's time (it's applied twice in each half).
        let loop_s: f32 = TANK.iter().flatten().sum::<usize>() as f32 * scale / rate as f32;
        let decay = 0.001f32.powf(loop_s / (4.0 * s.decay_s.max(0.1))).min(0.97);
        let cut = |f: Option<f32>, high: bool| {
            f.map(|f| {
                let mut b = Biquad::default();
                let f = f.clamp(20.0, rate as f32 * 0.45) as f64;
                if high {
                    b.set_lowpass(rate, f, 0.707);
                } else {
                    b.set_highpass(rate, f, 0.707);
                }
                b
            })
        };
        let predelay_len = (s.predelay_ms.max(0.0) * 0.001 * rate as f32) as usize;
        Reverb {
            s: s.clone(),
            predelay: Line::new(predelay_len + 2),
            predelay_len,
            cuts: [cut(s.low_cut, false), cut(s.high_cut, true)],
            input: 0.0,
            diffusers: DIFFUSERS.map(|n| Line::new(len(n))),
            tank,
            damped: [0.0; 2],
            cross: [0.0; 2],
            decay,
            damping: s.damping.clamp(0.0, 1.0) * 0.8,
            taps: TAPS.map(|side| side.map(len)),
            swing,
            lfo: 0.0,
            lfo_step: 1.0 / rate as f32,
        }
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        let wet = self.s.dry_wet.clamp(0.0, 1.0);
        let width = self.s.width.max(0.0);
        for f in buf.iter_mut() {
            let mut x = [(f[0] + f[1]) * 0.5; 2];
            for b in self.cuts.iter_mut().flatten() {
                x = b.process(x);
            }
            let x = if self.predelay_len > 0 {
                let d = self.predelay.tap(self.predelay_len);
                self.predelay.push(x[0]);
                d
            } else {
                x[0]
            };
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
                let base = ap1.buf.len() as f32 - self.swing - 2.0;
                let delayed = ap1.tap_frac(base + self.swing * m);
                let w = input + 0.7 * delayed;
                ap1.push(w);
                d1.push(delayed - 0.7 * w);
                let t = d1.tap(d1.buf.len());
                self.damped[half] += (t - self.damped[half]) * (1.0 - self.damping);
                let b = ap2.allpass(self.damped[half] * self.decay, 0.5);
                d2.push(b);
                self.cross[half] = d2.tap(d2.buf.len());
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
            let (mid, side) = ((l + r) * 0.5, (l - r) * 0.5 * width);
            let out = [(mid + side) * 0.6, (mid - side) * 0.6];
            *f = [
                f[0] * (1.0 - wet) + out[0] * wet,
                f[1] * (1.0 - wet) + out[1] * wet,
            ];
        }
    }
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

impl GateSettings {
    pub fn summary(&self) -> String {
        format!(
            "{:.1} dB · floor {:.0} dB{}",
            db(self.threshold),
            self.floor_db,
            if self.flip { " · flipped" } else { "" }
        )
    }
}

pub struct Gate {
    s: GateSettings,
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
        let coef = |ms: f32| (-1.0 / (ms.max(0.02) * 0.001 * rate as f32)).exp();
        let floor = if s.floor_db <= -80.0 {
            0.0
        } else {
            from_db(s.floor_db)
        };
        Gate {
            close_at: s.threshold * from_db(-s.return_db.max(0.0)),
            floor,
            attack: coef(s.attack_ms),
            release: coef(s.release_ms),
            hold: (s.hold_ms.max(0.0) * 0.001 * rate as f32) as usize,
            held: 0,
            open: false,
            level: 0.0,
            gain: if s.flip { 1.0 } else { floor },
            level_fall: coef(10.0),
            s: s.clone(),
        }
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
    /// 0..100.
    pub drive: f32,
    /// 0 dark to 100 bright.
    pub tone: f32,
    /// 0..1.
    pub dry_wet: f32,
}

impl OverdriveSettings {
    pub fn summary(&self) -> String {
        format!(
            "drive {:.0}% · tone {:.0}% · {:.0}% wet",
            self.drive,
            self.tone,
            self.dry_wet * 100.0
        )
    }
}

pub struct Overdrive {
    band: [Biquad; 2],
    tone: Biquad,
    drive: f32,
    wet: f32,
}

impl Overdrive {
    pub fn new(s: &OverdriveSettings, rate: f64) -> Overdrive {
        let half = 2f32.powf(s.bandwidth.clamp(0.5, 9.0) / 2.0);
        let nyq = rate as f32 * 0.45;
        let (mut hp, mut lp, mut tone) = (Biquad::default(), Biquad::default(), Biquad::default());
        hp.set_highpass(rate, (s.mid_freq / half).clamp(20.0, nyq) as f64, 0.707);
        lp.set_lowpass(rate, (s.mid_freq * half).clamp(20.0, nyq) as f64, 0.707);
        // Tone sweeps a low-pass from 500 Hz to 16 kHz.
        let tone_hz = 500.0 * 2f32.powf(s.tone.clamp(0.0, 100.0) / 100.0 * 5.0);
        tone.set_lowpass(rate, tone_hz.min(nyq) as f64, 0.707);
        Overdrive {
            band: [hp, lp],
            tone,
            drive: from_db(s.drive.clamp(0.0, 100.0) * 0.4),
            wet: s.dry_wet.clamp(0.0, 1.0),
        }
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        for f in buf.iter_mut() {
            let [h, l] = &mut self.band;
            let x = l.process(h.process(*f));
            let y = self.tone.process(x.map(|v| (v * self.drive).tanh()));
            *f = [
                f[0] * (1.0 - self.wet) + y[0] * self.wet,
                f[1] * (1.0 - self.wet) + y[1] * self.wet,
            ];
        }
    }
}

/// Roar, approximated: its shaper stages in series, each with its
/// low-pass, then its compressor.
#[derive(Clone, Debug, PartialEq)]
pub struct RoarSettings {
    pub input_db: f32,
    pub stages: Vec<RoarStage>,
    /// 0..1.
    pub compression: f32,
    pub output_db: f32,
    pub dry_wet: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoarStage {
    /// 0..1.
    pub amount: f32,
    /// -1..1.
    pub bias: f32,
    pub lowpass: Option<f32>,
}

impl RoarSettings {
    pub fn summary(&self) -> String {
        format!(
            "{} stage{} · approximation",
            self.stages.len(),
            if self.stages.len() == 1 { "" } else { "s" }
        )
    }
}

pub struct Roar {
    stages: Vec<(RoarStage, f32, f32, Option<Biquad>)>,
    input: f32,
    output: f32,
    wet: f32,
    comp: Option<Compressor>,
}

impl Roar {
    pub fn new(s: &RoarSettings, rate: f64) -> Roar {
        let stages = s
            .stages
            .iter()
            .map(|st| {
                let drive = from_db(st.amount.clamp(0.0, 1.0) * 36.0);
                let bias = st.bias.clamp(-1.0, 1.0) * 0.5;
                let lp = st.lowpass.map(|f| {
                    let mut b = Biquad::default();
                    b.set_lowpass(rate, f.clamp(20.0, rate as f32 * 0.45) as f64, 0.707);
                    b
                });
                (*st, drive, bias, lp)
            })
            .collect();
        let c = s.compression.clamp(0.0, 1.0);
        let comp = (c > 0.0).then(|| {
            Compressor::new(
                &CompressorSettings {
                    threshold: from_db(-24.0 * c),
                    ratio: 1.0 + 5.0 * c,
                    attack_ms: 3.0,
                    release_ms: 80.0,
                    makeup_db: 0.0,
                    knee_db: 6.0,
                    dry_wet: 1.0,
                },
                rate,
            )
        });
        Roar {
            stages,
            input: from_db(s.input_db),
            output: from_db(s.output_db),
            wet: s.dry_wet.clamp(0.0, 1.0),
            comp,
        }
    }

    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        let dry_wet = self.wet;
        for f in buf.iter_mut() {
            let dry = *f;
            let mut x = [f[0] * self.input, f[1] * self.input];
            for (_, drive, bias, lp) in self.stages.iter_mut() {
                let off = bias.tanh();
                x = x.map(|v| (v * *drive + *bias).tanh() - off);
                if let Some(lp) = lp {
                    x = lp.process(x);
                }
            }
            *f = x;
            if let Some(c) = &mut self.comp {
                c.process(std::slice::from_mut(f));
            }
            *f = [
                dry[0] * (1.0 - dry_wet) + f[0] * self.output * dry_wet,
                dry[1] * (1.0 - dry_wet) + f[1] * self.output * dry_wet,
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
            echo: false,
            time: [Time::Beats(0.5); 2],
            feedback: 0.5,
            invert: false,
            ping_pong: true,
            filter: None,
            input_db: 0.0,
            output: 1.0,
            dry_wet: 1.0,
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
    fn reverb_tail_lasts_about_its_decay_time() {
        let tail = |decay_s| {
            let s = ReverbSettings {
                hybrid: false,
                predelay_ms: 10.0,
                decay_s,
                size: 0.5,
                damping: 0.3,
                low_cut: None,
                high_cut: None,
                width: 1.0,
                dry_wet: 1.0,
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
    fn gate_shuts_quiet_sound_and_lets_loud_through() {
        let s = GateSettings {
            threshold: from_db(-30.0),
            attack_ms: 0.1,
            hold_ms: 5.0,
            release_ms: 5.0,
            return_db: 3.0,
            floor_db: -40.0,
            flip: false,
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
                drive: 100.0,
                tone: 50.0,
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
                input_db: 0.0,
                stages: vec![RoarStage {
                    amount: 0.5,
                    bias: -0.3,
                    lowpass: Some(8_000.0),
                }],
                compression: 0.25,
                output_db: 0.0,
                dry_wet: 1.0,
            },
            48_000.0,
        );
        let mut b = sine;
        r.process(&mut b);
        let peak = b.iter().fold(0f32, |m, f| m.max(f[0].abs()));
        assert!(peak > 0.1 && peak < 2.0, "{peak}");
    }
}
