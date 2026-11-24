//! The studio's own devices, matching Live's so a set's chains sound as
//! they do in Live: Utility, EQ Eight and Compressor here, and the time and
//! character effects in [`crate::fx`], with the settings the set saved.
//! Each runs on the audio thread in the track's chain, in order with the
//! plug-ins; all memory is set up before the audio starts, and a setting
//! changed from its panel retunes the running device in place.

use opendj_engine::dsp::Biquad;

use crate::fx;

/// How a setting reads and moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Db,
    /// A fraction shown as a percentage.
    Percent,
    Hz,
    Ms,
    Seconds,
    Ratio,
    Octaves,
    /// An index into [`fx::NOTES`].
    Note,
    /// -1 left to 1 right.
    Pan,
    Plain,
    Toggle,
    Choice(&'static [&'static str]),
}

/// One setting as a device's panel shows it, in Live's units.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// Moves on a log scale (frequencies, times).
    pub log: bool,
    pub unit: Unit,
    /// Its knob fills from the middle.
    pub bipolar: bool,
}

impl Param {
    pub fn knob(name: impl Into<String>, value: f32, min: f32, max: f32, unit: Unit) -> Param {
        Param {
            name: name.into(),
            value,
            min,
            max,
            log: false,
            unit,
            bipolar: false,
        }
    }

    pub fn log(mut self) -> Param {
        self.log = true;
        self
    }

    pub fn bipolar(mut self) -> Param {
        self.bipolar = true;
        self
    }

    pub fn toggle(name: impl Into<String>, on: bool) -> Param {
        Param::knob(name, on as u8 as f32, 0.0, 1.0, Unit::Toggle)
    }

    pub fn choice(name: impl Into<String>, i: usize, names: &'static [&'static str]) -> Param {
        let max = names.len().saturating_sub(1) as f32;
        Param::knob(name, i as f32, 0.0, max, Unit::Choice(names))
    }

    /// A synced time, as the nearest of [`fx::NOTES`].
    pub fn notes(name: impl Into<String>, beats: f64) -> Param {
        let max = (fx::NOTES.len() - 1) as f32;
        Param::knob(name, fx::nearest_note(beats) as f32, 0.0, max, Unit::Note)
    }

    /// A switch (on/off or a list) rather than a knob.
    pub fn is_switch(&self) -> bool {
        matches!(self.unit, Unit::Toggle | Unit::Choice(_))
    }

    /// The value as a knob's position, 0..1.
    pub fn norm(&self) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }
        let v = self.value.clamp(self.min, self.max);
        if self.log && self.min > 0.0 {
            (v / self.min).ln() / (self.max / self.min).ln()
        } else {
            (v - self.min) / (self.max - self.min)
        }
    }

    /// The value at a knob's position.
    pub fn at_norm(&self, n: f32) -> f32 {
        let n = n.clamp(0.0, 1.0);
        let v = if self.log && self.min > 0.0 {
            self.min * (self.max / self.min).powf(n)
        } else {
            self.min + n * (self.max - self.min)
        };
        match self.unit {
            Unit::Note | Unit::Toggle | Unit::Choice(_) => v.round(),
            _ => v,
        }
    }

    /// The value a click on a switch moves to.
    pub fn next(&self) -> f32 {
        match self.unit {
            Unit::Toggle => 1.0 - self.value.round(),
            _ => {
                let v = self.value.round() + 1.0;
                if v > self.max {
                    self.min
                } else {
                    v
                }
            }
        }
    }

    pub fn text(&self) -> String {
        let v = self.value;
        match self.unit {
            Unit::Db if self.min <= -70.0 && v <= self.min + 0.05 => "-inf dB".into(),
            Unit::Db => format!("{v:.1} dB"),
            Unit::Percent => format!("{:.0} %", v * 100.0),
            Unit::Hz if v >= 1000.0 => format!("{:.2} kHz", v / 1000.0),
            Unit::Hz => format!("{v:.0} Hz"),
            Unit::Ms if v < 10.0 => format!("{v:.2} ms"),
            Unit::Ms if v < 100.0 => format!("{v:.1} ms"),
            Unit::Ms => format!("{v:.0} ms"),
            Unit::Seconds => format!("{v:.2} s"),
            Unit::Ratio => format!("{v:.1} : 1"),
            Unit::Octaves => format!("{v:.1} oct"),
            Unit::Note => fx::NOTE_NAMES[(v.round() as usize).min(fx::NOTES.len() - 1)].into(),
            Unit::Pan if v.abs() < 0.005 => "C".into(),
            Unit::Pan => format!("{:.0}{}", v.abs() * 50.0, if v < 0.0 { "L" } else { "R" }),
            Unit::Plain => format!("{v:.2}"),
            Unit::Toggle => if v >= 0.5 { "On" } else { "Off" }.into(),
            Unit::Choice(names) => names
                .get(v.round().max(0.0) as usize)
                .copied()
                .unwrap_or("")
                .into(),
        }
    }
}

/// A device's settings, as a set saves them (Live's own units).
#[derive(Clone, Debug, PartialEq)]
pub enum Settings {
    Utility(UtilitySettings),
    Eq8(Eq8Settings),
    Compressor(CompressorSettings),
    Delay(fx::DelaySettings),
    Reverb(fx::ReverbSettings),
    Gate(fx::GateSettings),
    Overdrive(fx::OverdriveSettings),
    Roar(fx::RoarSettings),
}

impl Settings {
    /// What the device row says about it.
    pub fn summary(&self) -> String {
        match self {
            Settings::Utility(u) => {
                let mut parts = vec![format!("{:+.1} dB", db(u.gain))];
                if (u.width - 1.0).abs() > 1e-3 {
                    parts.push(format!("width {:.0}%", u.width * 100.0));
                }
                if u.mono {
                    parts.push("mono".into());
                }
                if u.mute {
                    parts.push("muted".into());
                }
                parts.join(" · ")
            }
            Settings::Eq8(e) => {
                let on = e.bands.iter().filter(|b| b.on).count();
                format!("{on} bands")
            }
            Settings::Compressor(c) => format!(
                "{:.1} dB · {:.1}:1 · {:.0}/{:.0} ms",
                db(c.threshold),
                c.ratio,
                c.attack_ms,
                c.release_ms
            ),
            Settings::Delay(d) => d.summary(),
            Settings::Reverb(r) => r.summary(),
            Settings::Gate(g) => g.summary(),
            Settings::Overdrive(o) => o.summary(),
            Settings::Roar(r) => r.summary(),
        }
    }

    /// The settings its panel shows, in order; [`Settings::set`] takes the
    /// same indices.
    pub fn params(&self) -> Vec<Param> {
        match self {
            Settings::Utility(u) => u.params(),
            Settings::Eq8(e) => e.params(),
            Settings::Compressor(c) => c.params(),
            Settings::Delay(d) => d.params(),
            Settings::Reverb(r) => r.params(),
            Settings::Gate(g) => g.params(),
            Settings::Overdrive(o) => o.params(),
            Settings::Roar(r) => r.params(),
        }
    }

    /// The order its panel lays out its knobs (by setting) and how many
    /// across, where it isn't simply in order.
    pub fn knob_layout(&self) -> Option<(Vec<usize>, usize)> {
        match self {
            // Roar: a column per stage, input and the crossovers on the
            // left, the output section on the right.
            Settings::Roar(_) => {
                Some((vec![0, 2, 3, 4, 13, 17, 6, 7, 8, 14, 18, 10, 11, 12, 15], 5))
            }
            _ => None,
        }
    }

    /// Changes one setting (see [`Settings::params`]). Never allocates, so
    /// the audio thread runs it too.
    pub fn set(&mut self, i: usize, v: f32) {
        match self {
            Settings::Utility(u) => u.set(i, v),
            Settings::Eq8(e) => e.set(i, v),
            Settings::Compressor(c) => c.set(i, v),
            Settings::Delay(d) => d.set(i, v),
            Settings::Reverb(r) => r.set(i, v),
            Settings::Gate(g) => g.set(i, v),
            Settings::Overdrive(o) => o.set(i, v),
            Settings::Roar(r) => r.set(i, v),
        }
    }
}

/// Points across a device's response graph, from [`EQ_FREQ`]'s low end to
/// its high end on a log scale.
pub const GRAPH_POINTS: usize = 120;

/// The frequency at a place across the graph, 0..1.
pub fn graph_freq(n: f32) -> f32 {
    EQ_FREQ.0 * (EQ_FREQ.1 / EQ_FREQ.0).powf(n)
}

/// Where a frequency sits across the graph, 0..1.
pub fn graph_place(freq: f32) -> f32 {
    (freq.max(EQ_FREQ.0) / EQ_FREQ.0).ln() / (EQ_FREQ.1 / EQ_FREQ.0).ln()
}

/// A high-pass then a low-pass, as a response in dB at each graph point.
fn cuts_curve(high_pass: Option<f32>, low_pass: Option<f32>) -> Vec<f32> {
    let rate = 48_000.0;
    let mut f = Vec::new();
    if let Some(h) = high_pass {
        let mut b = Biquad::default();
        b.set_highpass(rate, h as f64, 0.707);
        f.push(b);
    }
    if let Some(l) = low_pass {
        let mut b = Biquad::default();
        b.set_lowpass(rate, (l as f64).min(rate * 0.45), 0.707);
        f.push(b);
    }
    (0..GRAPH_POINTS)
        .map(|i| {
            let hz = graph_freq(i as f32 / (GRAPH_POINTS - 1) as f32) as f64;
            f.iter().map(|b| b.magnitude_db(rate, hz)).sum::<f64>() as f32
        })
        .collect()
}

impl Settings {
    /// The device's frequency response for its panel's graph, with the dB
    /// at its top and bottom edges; None for a device with no graph.
    pub fn curve(&self) -> Option<(Vec<f32>, f32, f32)> {
        match self {
            Settings::Eq8(e) => Some((
                (0..GRAPH_POINTS)
                    .map(|i| {
                        let hz = graph_freq(i as f32 / (GRAPH_POINTS - 1) as f32);
                        e.response(48_000.0, hz as f64)
                    })
                    .collect(),
                EQ_GAIN,
                -EQ_GAIN,
            )),
            Settings::Delay(d) => Some((
                cuts_curve(
                    d.filter_on.then_some(d.high_pass),
                    d.filter_on.then_some(d.low_pass),
                ),
                6.0,
                -30.0,
            )),
            Settings::Overdrive(o) => {
                let half = 2f32.powf(o.bandwidth / 2.0);
                Some((
                    cuts_curve(Some(o.mid_freq / half), Some(o.mid_freq * half)),
                    6.0,
                    -30.0,
                ))
            }
            Settings::Reverb(r) => Some((
                cuts_curve(
                    r.low_cut_on.then_some(r.low_cut),
                    r.high_cut_on.then_some(r.high_cut),
                ),
                6.0,
                -30.0,
            )),
            _ => None,
        }
    }
}

pub(crate) fn db(gain: f32) -> f32 {
    20.0 * gain.max(1e-6).log10()
}

pub(crate) fn from_db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn on(v: f32) -> bool {
    v >= 0.5
}

/// Live's Utility.
#[derive(Clone, Debug, PartialEq)]
pub struct UtilitySettings {
    /// Linear gain.
    pub gain: f32,
    /// 0 (mono) to 4 (400%); 1 leaves it.
    pub width: f32,
    pub mono: bool,
    /// -1 (left) to 1 (right).
    pub balance: f32,
    pub mute: bool,
    pub invert: [bool; 2],
    /// 0 left only, 1 stereo, 2 right only, 3 swapped.
    pub channels: u8,
}

impl Default for UtilitySettings {
    fn default() -> Self {
        UtilitySettings {
            gain: 1.0,
            width: 1.0,
            mono: false,
            balance: 0.0,
            mute: false,
            invert: [false; 2],
            channels: 1,
        }
    }
}

const CHANNELS: &[&str] = &["Left", "Stereo", "Right", "Swap"];

impl UtilitySettings {
    fn params(&self) -> Vec<Param> {
        vec![
            Param::knob("Gain", db(self.gain), -35.0, 35.0, Unit::Db).bipolar(),
            Param::knob("Width", self.width, 0.0, 4.0, Unit::Percent),
            Param::knob("Balance", self.balance, -1.0, 1.0, Unit::Pan).bipolar(),
            Param::toggle("Mono", self.mono),
            Param::toggle("Mute", self.mute),
            Param::toggle("Invert L", self.invert[0]),
            Param::toggle("Invert R", self.invert[1]),
            Param::choice("Channels", self.channels as usize, CHANNELS),
        ]
    }

    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.gain = from_db(v),
            1 => self.width = v.max(0.0),
            2 => self.balance = v.clamp(-1.0, 1.0),
            3 => self.mono = on(v),
            4 => self.mute = on(v),
            5 => self.invert[0] = on(v),
            6 => self.invert[1] = on(v),
            7 => self.channels = v.round().clamp(0.0, 3.0) as u8,
            _ => {}
        }
    }
}

/// An EQ Eight band's shape, in Live's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    LowCut48,
    LowCut12,
    LowShelf,
    Bell,
    Notch,
    HighShelf,
    HighCut12,
    HighCut48,
}

pub const SHAPES: &[&str] = &[
    "Low Cut 48",
    "Low Cut 12",
    "Low Shelf",
    "Bell",
    "Notch",
    "High Shelf",
    "High Cut 12",
    "High Cut 48",
];

impl Shape {
    pub fn from_live(mode: u8) -> Shape {
        match mode {
            0 => Shape::LowCut48,
            1 => Shape::LowCut12,
            2 => Shape::LowShelf,
            4 => Shape::Notch,
            5 => Shape::HighShelf,
            6 => Shape::HighCut12,
            7 => Shape::HighCut48,
            _ => Shape::Bell,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Whether its gain does anything.
    pub fn has_gain(self) -> bool {
        matches!(self, Shape::LowShelf | Shape::Bell | Shape::HighShelf)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub on: bool,
    pub shape: Shape,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

impl Default for Band {
    fn default() -> Self {
        Band {
            on: false,
            shape: Shape::Bell,
            freq: 1000.0,
            gain_db: 0.0,
            q: 0.71,
        }
    }
}

/// Live's EQ Eight.
#[derive(Clone, Debug, PartialEq)]
pub struct Eq8Settings {
    pub bands: [Band; 8],
    /// Output gain, dB.
    pub gain_db: f32,
    /// How much of each band's gain applies (Live's Scale, 1 = 100%).
    pub scale: f32,
}

/// Settings each EQ Eight band has, in [`Eq8Settings::params`] order.
pub const BAND_PARAMS: usize = 5;
pub const EQ_FREQ: (f32, f32) = (10.0, 22_000.0);
pub const EQ_GAIN: f32 = 15.0;

impl Eq8Settings {
    fn params(&self) -> Vec<Param> {
        let mut p = Vec::with_capacity(8 * BAND_PARAMS + 2);
        for (i, b) in self.bands.iter().enumerate() {
            let n = i + 1;
            p.push(Param::toggle(format!("Band {n}"), b.on));
            p.push(Param::choice(format!("Shape {n}"), b.shape.index(), SHAPES));
            p.push(Param::knob("Freq", b.freq, EQ_FREQ.0, EQ_FREQ.1, Unit::Hz).log());
            p.push(Param::knob("Gain", b.gain_db, -EQ_GAIN, EQ_GAIN, Unit::Db).bipolar());
            p.push(Param::knob("Q", b.q, 0.1, 18.0, Unit::Plain).log());
        }
        p.push(Param::knob("Output", self.gain_db, -12.0, 12.0, Unit::Db).bipolar());
        p.push(Param::knob("Scale", self.scale, -2.0, 2.0, Unit::Percent).bipolar());
        p
    }

    fn set(&mut self, i: usize, v: f32) {
        if let Some(b) = self.bands.get_mut(i / BAND_PARAMS) {
            match i % BAND_PARAMS {
                0 => b.on = on(v),
                1 => b.shape = SHAPE_ORDER[(v.round().max(0.0) as usize).min(7)],
                2 => b.freq = v.clamp(EQ_FREQ.0, EQ_FREQ.1),
                3 => b.gain_db = v.clamp(-EQ_GAIN, EQ_GAIN),
                _ => b.q = v.clamp(0.1, 18.0),
            }
            return;
        }
        match i - 8 * BAND_PARAMS {
            0 => self.gain_db = v,
            1 => self.scale = v,
            _ => {}
        }
    }

    /// The whole EQ's response, dB, at `freq`.
    pub fn response(&self, rate: f64, freq: f64) -> f32 {
        let mut total = self.gain_db as f64;
        for b in self.bands.iter().filter(|b| b.on) {
            let (n, f) = band_filters(b, self.scale, rate);
            total += f[..n]
                .iter()
                .map(|f| f.magnitude_db(rate, freq))
                .sum::<f64>();
        }
        total as f32
    }
}

const SHAPE_ORDER: [Shape; 8] = [
    Shape::LowCut48,
    Shape::LowCut12,
    Shape::LowShelf,
    Shape::Bell,
    Shape::Notch,
    Shape::HighShelf,
    Shape::HighCut12,
    Shape::HighCut48,
];

/// Live's Compressor (peak, feed-forward).
#[derive(Clone, Debug, PartialEq)]
pub struct CompressorSettings {
    /// Linear level.
    pub threshold: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    /// Make-up, dB.
    pub makeup_db: f32,
    pub knee_db: f32,
    /// 0..1.
    pub dry_wet: f32,
}

impl CompressorSettings {
    fn params(&self) -> Vec<Param> {
        vec![
            Param::knob("Threshold", db(self.threshold), -60.0, 0.0, Unit::Db),
            Param::knob("Ratio", self.ratio, 1.0, 100.0, Unit::Ratio).log(),
            Param::knob("Attack", self.attack_ms, 0.01, 1000.0, Unit::Ms).log(),
            Param::knob("Release", self.release_ms, 1.0, 3000.0, Unit::Ms).log(),
            Param::knob("Knee", self.knee_db, 0.0, 18.0, Unit::Db),
            Param::knob("Makeup", self.makeup_db, -36.0, 36.0, Unit::Db).bipolar(),
            Param::knob("Dry/Wet", self.dry_wet, 0.0, 1.0, Unit::Percent),
        ]
    }

    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.threshold = from_db(v),
            1 => self.ratio = v.max(1.0),
            2 => self.attack_ms = v.max(0.01),
            3 => self.release_ms = v.max(1.0),
            4 => self.knee_db = v.max(0.0),
            5 => self.makeup_db = v,
            6 => self.dry_wet = v.clamp(0.0, 1.0),
            _ => {}
        }
    }
}

/// A device as the audio thread runs it.
pub enum Device {
    Utility(UtilitySettings),
    Eq8(Box<Eq8>),
    Compressor(Compressor),
    Delay(Box<fx::Delay>),
    Reverb(Box<fx::Reverb>),
    Gate(fx::Gate),
    Overdrive(Box<fx::Overdrive>),
    Roar(Box<fx::Roar>),
}

impl Device {
    pub fn new(s: &Settings, rate: f64) -> Device {
        match s {
            Settings::Utility(u) => Device::Utility(u.clone()),
            Settings::Eq8(e) => Device::Eq8(Box::new(Eq8::new(e, rate))),
            Settings::Compressor(c) => Device::Compressor(Compressor::new(c, rate)),
            Settings::Delay(d) => Device::Delay(Box::new(fx::Delay::new(d, rate))),
            Settings::Reverb(r) => Device::Reverb(Box::new(fx::Reverb::new(r, rate))),
            Settings::Gate(g) => Device::Gate(fx::Gate::new(g, rate)),
            Settings::Overdrive(o) => Device::Overdrive(Box::new(fx::Overdrive::new(o, rate))),
            Settings::Roar(r) => Device::Roar(Box::new(fx::Roar::new(r, rate))),
        }
    }

    /// Changes one setting while it runs, keeping its sound going (a
    /// reverb's tail, a delay's repeats). Never allocates.
    pub fn set(&mut self, i: usize, v: f32) {
        match self {
            Device::Utility(u) => u.set(i, v),
            Device::Eq8(e) => {
                e.s.set(i, v);
                e.retune();
            }
            Device::Compressor(c) => {
                c.s.set(i, v);
                c.retune();
            }
            Device::Delay(d) => d.set(i, v),
            Device::Reverb(r) => r.set(i, v),
            Device::Gate(g) => g.set(i, v),
            Device::Overdrive(o) => o.set(i, v),
            Device::Roar(r) => r.set(i, v),
        }
    }

    /// Processes `buf` in place; `tempo` times synced delays.
    pub fn process(&mut self, buf: &mut [[f32; 2]], tempo: f64) {
        match self {
            Device::Utility(u) => utility(u, buf),
            Device::Eq8(e) => e.process(buf),
            Device::Compressor(c) => c.process(buf),
            Device::Delay(d) => d.process(buf, tempo),
            Device::Reverb(r) => r.process(buf),
            Device::Gate(g) => g.process(buf),
            Device::Overdrive(o) => o.process(buf),
            Device::Roar(r) => r.process(buf),
        }
    }
}

fn utility(u: &UtilitySettings, buf: &mut [[f32; 2]]) {
    if u.mute {
        buf.fill([0.0; 2]);
        return;
    }
    let width = if u.mono { 0.0 } else { u.width };
    let (l_gain, r_gain) = ((1.0 - u.balance).min(1.0), (1.0 + u.balance).min(1.0));
    let sign = |i: bool| if i { -1.0 } else { 1.0 };
    let (sl, sr) = (sign(u.invert[0]), sign(u.invert[1]));
    for f in buf.iter_mut() {
        let [mut l, mut r] = match u.channels {
            0 => [f[0], f[0]],
            2 => [f[1], f[1]],
            3 => [f[1], f[0]],
            _ => *f,
        };
        l *= sl;
        r *= sr;
        let mid = (l + r) * 0.5;
        let side = (l - r) * 0.5 * width;
        *f = [
            (mid + side) * u.gain * l_gain,
            (mid - side) * u.gain * r_gain,
        ];
    }
}

/// A band's filters: one biquad, or two for the 48 dB cuts.
fn band_filters(b: &Band, scale: f32, rate: f64) -> (usize, [Biquad; 2]) {
    let mut f = [Biquad::default(); 2];
    let n = tune_band(&mut f, b, scale, rate);
    (n, f)
}

/// Sets `f` for a band (keeping their state) and says how many it uses.
fn tune_band(f: &mut [Biquad; 2], b: &Band, scale: f32, rate: f64) -> usize {
    let (freq, q, g) = (
        b.freq as f64,
        b.q.max(0.1) as f64,
        (b.gain_db * scale) as f64,
    );
    let [one, second] = f;
    match b.shape {
        Shape::LowCut12 => one.set_highpass(rate, freq, q),
        Shape::HighCut12 => one.set_lowpass(rate, freq, q),
        Shape::LowCut48 | Shape::HighCut48 => {
            // Four poles: two Butterworth stages, the first with the
            // band's resonance.
            if b.shape == Shape::LowCut48 {
                one.set_highpass(rate, freq, q * 0.541 / 0.707);
                second.set_highpass(rate, freq, 1.307);
            } else {
                one.set_lowpass(rate, freq, q * 0.541 / 0.707);
                second.set_lowpass(rate, freq, 1.307);
            }
            return 2;
        }
        Shape::LowShelf => one.set_low_shelf(rate, freq, q, g),
        Shape::HighShelf => one.set_high_shelf(rate, freq, q, g),
        Shape::Bell => one.set_peaking(rate, freq, q, g),
        // A deep, narrow cut.
        Shape::Notch => one.set_peaking(rate, freq, q.max(2.0), -60.0),
    }
    1
}

/// EQ Eight: each band one biquad, or two for the 48 dB cuts.
pub struct Eq8 {
    s: Eq8Settings,
    rate: f64,
    bands: [(usize, [Biquad; 2]); 8],
    gain: f32,
}

impl Eq8 {
    fn new(s: &Eq8Settings, rate: f64) -> Eq8 {
        let mut e = Eq8 {
            s: s.clone(),
            rate,
            bands: [(0, [Biquad::default(); 2]); 8],
            gain: 1.0,
        };
        e.retune();
        e
    }

    fn retune(&mut self) {
        for (b, (n, f)) in self.s.bands.iter().zip(self.bands.iter_mut()) {
            *n = if b.on {
                tune_band(f, b, self.s.scale, self.rate)
            } else {
                0
            };
        }
        self.gain = from_db(self.s.gain_db);
    }

    fn process(&mut self, buf: &mut [[f32; 2]]) {
        for f in buf.iter_mut() {
            let mut x = *f;
            for (n, b) in self.bands.iter_mut() {
                for b in b[..*n].iter_mut() {
                    x = b.process(x);
                }
            }
            *f = [x[0] * self.gain, x[1] * self.gain];
        }
    }
}

/// A peak compressor with a soft knee, linked stereo.
pub struct Compressor {
    pub(crate) s: CompressorSettings,
    rate: f64,
    threshold_db: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    /// Gain reduction now, dB (0 or less).
    reduction: f32,
}

impl Compressor {
    pub(crate) fn new(s: &CompressorSettings, rate: f64) -> Compressor {
        let mut c = Compressor {
            s: s.clone(),
            rate,
            threshold_db: 0.0,
            attack: 0.0,
            release: 0.0,
            makeup: 1.0,
            reduction: 0.0,
        };
        c.retune();
        c
    }

    pub(crate) fn retune(&mut self) {
        let rate = self.rate as f32;
        let coef = |ms: f32| (-1.0 / (ms.max(0.01) * 0.001 * rate)).exp();
        self.threshold_db = db(self.s.threshold);
        self.attack = coef(self.s.attack_ms);
        self.release = coef(self.s.release_ms);
        self.makeup = from_db(self.s.makeup_db);
    }

    /// Gain reduction (dB, 0 or less) for a level `x` dB.
    fn curve(&self, x: f32) -> f32 {
        let (t, k, r) = (
            self.threshold_db,
            self.s.knee_db.max(0.0),
            self.s.ratio.max(1.0),
        );
        let over = x - t;
        let slope = 1.0 / r - 1.0;
        if 2.0 * over < -k {
            0.0
        } else if k > 0.0 && 2.0 * over.abs() <= k {
            slope * (over + k / 2.0).powi(2) / (2.0 * k)
        } else {
            slope * over
        }
    }

    pub(crate) fn process(&mut self, buf: &mut [[f32; 2]]) {
        let wet = self.s.dry_wet.clamp(0.0, 1.0);
        for f in buf.iter_mut() {
            let level = f[0].abs().max(f[1].abs());
            let want = self.curve(db(level));
            // Falling gain is the attack; rising, the release.
            let c = if want < self.reduction {
                self.attack
            } else {
                self.release
            };
            self.reduction = want + (self.reduction - want) * c;
            let g = from_db(self.reduction) * self.makeup;
            *f = [f[0] * (1.0 - wet + wet * g), f[1] * (1.0 - wet + wet * g)];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32, n: usize) -> Vec<[f32; 2]> {
        (0..n)
            .map(|i| {
                let v = (i as f32 * freq * std::f32::consts::TAU / 48_000.0).sin() * amp;
                [v, v]
            })
            .collect()
    }

    fn peak(buf: &[[f32; 2]]) -> f32 {
        buf.iter()
            .fold(0.0, |m, f| m.max(f[0].abs()).max(f[1].abs()))
    }

    fn bands(list: &[Band]) -> [Band; 8] {
        let mut b = [Band::default(); 8];
        b[..list.len()].copy_from_slice(list);
        b
    }

    #[test]
    fn utility_gains_and_narrows() {
        let mut d = Device::new(
            &Settings::Utility(UtilitySettings {
                gain: 2.0,
                mono: true,
                ..UtilitySettings::default()
            }),
            48_000.0,
        );
        let mut buf = vec![[0.5, -0.5], [0.25, 0.25]];
        d.process(&mut buf, 120.0);
        assert_eq!(buf, vec![[0.0, 0.0], [0.5, 0.5]]);
    }

    #[test]
    fn eq_cuts_below_its_low_cut() {
        let band = |shape, freq, gain_db| Band {
            on: true,
            shape,
            freq,
            gain_db,
            q: 0.71,
        };
        let s = Eq8Settings {
            bands: bands(&[
                band(Shape::LowCut48, 300.0, 0.0),
                band(Shape::Bell, 3000.0, 6.0),
            ]),
            gain_db: 0.0,
            scale: 1.0,
        };
        assert!(s.response(48_000.0, 50.0) < -40.0);
        assert!((s.response(48_000.0, 3000.0) - 6.0).abs() < 0.2);
        let mut d = Device::new(&Settings::Eq8(s), 48_000.0);
        let mut low = sine(50.0, 0.5, 48_000);
        d.process(&mut low, 120.0);
        assert!(peak(&low[24_000..]) < 0.01, "{}", peak(&low[24_000..]));
        let mut d = Device::new(
            &Settings::Eq8(Eq8Settings {
                bands: bands(&[band(Shape::Bell, 3000.0, 6.0)]),
                gain_db: 0.0,
                scale: 1.0,
            }),
            48_000.0,
        );
        let mut mid = sine(3000.0, 0.25, 48_000);
        d.process(&mut mid, 120.0);
        let p = peak(&mid[24_000..]);
        assert!((p - 0.5).abs() < 0.02, "{p}");
        // Turning the band's gain down from its panel takes effect.
        d.set(3, 0.0);
        let mut mid = sine(3000.0, 0.25, 48_000);
        d.process(&mut mid, 120.0);
        let p = peak(&mid[24_000..]);
        assert!((p - 0.25).abs() < 0.01, "{p}");
    }

    #[test]
    fn compressor_pulls_loud_sound_down() {
        let s = CompressorSettings {
            threshold: from_db(-20.0),
            ratio: 4.0,
            attack_ms: 1.0,
            release_ms: 50.0,
            makeup_db: 0.0,
            knee_db: 0.0,
            dry_wet: 1.0,
        };
        let mut d = Device::new(&Settings::Compressor(s.clone()), 48_000.0);
        // 0 dB in: 20 dB over, so 15 dB down.
        let mut buf = sine(100.0, 1.0, 48_000);
        d.process(&mut buf, 120.0);
        let p = db(peak(&buf[24_000..]));
        assert!((p + 15.0).abs() < 1.5, "{p} dB");
        // Quiet sound passes.
        let mut quiet = sine(100.0, 0.05, 48_000);
        let mut d = Device::new(&Settings::Compressor(s), 48_000.0);
        d.process(&mut quiet, 120.0);
        assert!((peak(&quiet[24_000..]) - 0.05).abs() < 0.002);
    }

    /// Every device's settings read back as they were set, and a knob's
    /// position maps to its value and back.
    #[test]
    fn every_setting_round_trips() {
        let all = [
            Settings::Utility(UtilitySettings::default()),
            Settings::Eq8(Eq8Settings {
                bands: [Band::default(); 8],
                gain_db: 0.0,
                scale: 1.0,
            }),
            Settings::Compressor(CompressorSettings {
                threshold: 0.5,
                ratio: 2.0,
                attack_ms: 10.0,
                release_ms: 100.0,
                makeup_db: 0.0,
                knee_db: 6.0,
                dry_wet: 1.0,
            }),
            Settings::Delay(fx::DelaySettings::default()),
            Settings::Reverb(fx::ReverbSettings::default()),
            Settings::Reverb(fx::ReverbSettings {
                hybrid: true,
                ..fx::ReverbSettings::default()
            }),
            Settings::Gate(fx::GateSettings::default()),
            Settings::Overdrive(fx::OverdriveSettings::default()),
            Settings::Roar(fx::RoarSettings::default()),
        ];
        for s in all {
            let params = s.params();
            for (i, p) in params.iter().enumerate() {
                let want = p.at_norm(0.3);
                let mut changed = s.clone();
                changed.set(i, want);
                let got = &changed.params()[i];
                assert!(
                    (got.value - want).abs() <= 1e-3 * want.abs().max(1.0),
                    "{} {}: set {want}, got {}",
                    s.summary(),
                    p.name,
                    got.value
                );
                assert!(
                    (p.at_norm(p.norm()) - p.value).abs() <= 1e-3 * p.value.abs().max(1.0)
                        || p.is_switch()
                        || p.unit == Unit::Note
                );
                // The running device takes it without trouble.
                let mut d = Device::new(&s, 48_000.0);
                d.set(i, want);
                let mut buf = sine(440.0, 0.5, 256);
                d.process(&mut buf, 120.0);
                assert!(buf.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
            }
        }
    }
}
