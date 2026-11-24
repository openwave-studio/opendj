//! The studio's own devices, matching Live's so a set's chains sound as
//! they do in Live: Utility, EQ Eight and Compressor, with the settings the
//! set saved. Each runs on the audio thread in the track's chain, in order
//! with the plug-ins; all memory is set up before the audio starts.

use opendj_engine::dsp::Biquad;

/// A device's settings, as a set saves them (Live's own units).
#[derive(Clone, Debug, PartialEq)]
pub enum Settings {
    Utility(UtilitySettings),
    Eq8(Eq8Settings),
    Compressor(CompressorSettings),
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
        }
    }
}

fn db(gain: f32) -> f32 {
    20.0 * gain.max(1e-6).log10()
}

fn from_db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub on: bool,
    pub shape: Shape,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

/// Live's EQ Eight.
#[derive(Clone, Debug, PartialEq)]
pub struct Eq8Settings {
    pub bands: Vec<Band>,
    /// Output gain, dB.
    pub gain_db: f32,
    /// How much of each band's gain applies (Live's Scale, 1 = 100%).
    pub scale: f32,
}

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

/// A device as the audio thread runs it.
pub enum Device {
    Utility(UtilitySettings),
    Eq8(Box<Eq8>),
    Compressor(Compressor),
}

impl Device {
    pub fn new(s: &Settings, rate: f64) -> Device {
        match s {
            Settings::Utility(u) => Device::Utility(u.clone()),
            Settings::Eq8(e) => Device::Eq8(Box::new(Eq8::new(e, rate))),
            Settings::Compressor(c) => Device::Compressor(Compressor::new(c, rate)),
        }
    }

    /// Processes `buf` in place.
    pub fn process(&mut self, buf: &mut [[f32; 2]]) {
        match self {
            Device::Utility(u) => utility(u, buf),
            Device::Eq8(e) => e.process(buf),
            Device::Compressor(c) => c.process(buf),
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

/// EQ Eight: each band one biquad, or two for the 48 dB cuts.
pub struct Eq8 {
    filters: Vec<Biquad>,
    gain: f32,
}

impl Eq8 {
    fn new(s: &Eq8Settings, rate: f64) -> Eq8 {
        let mut filters = Vec::new();
        for b in s.bands.iter().filter(|b| b.on) {
            let (f, q, g) = (
                b.freq as f64,
                b.q.max(0.1) as f64,
                (b.gain_db * s.scale) as f64,
            );
            let mut one = Biquad::default();
            match b.shape {
                Shape::LowCut12 => one.set_highpass(rate, f, q),
                Shape::HighCut12 => one.set_lowpass(rate, f, q),
                Shape::LowCut48 | Shape::HighCut48 => {
                    // Four poles: two Butterworth stages, the first with the
                    // band's resonance.
                    let low = b.shape == Shape::LowCut48;
                    let mut second = Biquad::default();
                    if low {
                        one.set_highpass(rate, f, q * 0.541 / 0.707);
                        second.set_highpass(rate, f, 1.307);
                    } else {
                        one.set_lowpass(rate, f, q * 0.541 / 0.707);
                        second.set_lowpass(rate, f, 1.307);
                    }
                    filters.push(second);
                }
                Shape::LowShelf => one.set_low_shelf(rate, f, q, g),
                Shape::HighShelf => one.set_high_shelf(rate, f, q, g),
                Shape::Bell => one.set_peaking(rate, f, q, g),
                // A deep, narrow cut.
                Shape::Notch => one.set_peaking(rate, f, q.max(2.0), -60.0),
            }
            filters.push(one);
        }
        Eq8 {
            filters,
            gain: from_db(s.gain_db),
        }
    }

    fn process(&mut self, buf: &mut [[f32; 2]]) {
        for f in buf.iter_mut() {
            let mut x = *f;
            for b in self.filters.iter_mut() {
                x = b.process(x);
            }
            *f = [x[0] * self.gain, x[1] * self.gain];
        }
    }
}

/// A peak compressor with a soft knee, linked stereo.
pub struct Compressor {
    s: CompressorSettings,
    threshold_db: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    /// Gain reduction now, dB (0 or less).
    reduction: f32,
}

impl Compressor {
    fn new(s: &CompressorSettings, rate: f64) -> Compressor {
        let coef = |ms: f32| (-1.0 / (ms.max(0.01) * 0.001 * rate as f32)).exp();
        Compressor {
            threshold_db: db(s.threshold),
            attack: coef(s.attack_ms),
            release: coef(s.release_ms),
            makeup: from_db(s.makeup_db),
            reduction: 0.0,
            s: s.clone(),
        }
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

    fn process(&mut self, buf: &mut [[f32; 2]]) {
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
        d.process(&mut buf);
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
            bands: vec![
                band(Shape::LowCut48, 300.0, 0.0),
                band(Shape::Bell, 3000.0, 6.0),
            ],
            gain_db: 0.0,
            scale: 1.0,
        };
        let mut d = Device::new(&Settings::Eq8(s), 48_000.0);
        let mut low = sine(50.0, 0.5, 48_000);
        d.process(&mut low);
        assert!(peak(&low[24_000..]) < 0.01, "{}", peak(&low[24_000..]));
        let mut d = Device::new(
            &Settings::Eq8(Eq8Settings {
                bands: vec![band(Shape::Bell, 3000.0, 6.0)],
                gain_db: 0.0,
                scale: 1.0,
            }),
            48_000.0,
        );
        let mut mid = sine(3000.0, 0.25, 48_000);
        d.process(&mut mid);
        let p = peak(&mid[24_000..]);
        assert!((p - 0.5).abs() < 0.02, "{p}");
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
        let mut d = Device::new(&Settings::Compressor(s), 48_000.0);
        // 0 dB in: 20 dB over, so 15 dB down.
        let mut buf = sine(100.0, 1.0, 48_000);
        d.process(&mut buf);
        let p = db(peak(&buf[24_000..]));
        assert!((p + 15.0).abs() < 1.5, "{p} dB");
        // Quiet sound passes.
        let mut quiet = sine(100.0, 0.05, 48_000);
        let mut d = Device::new(
            &Settings::Compressor(CompressorSettings {
                threshold: from_db(-20.0),
                ratio: 4.0,
                attack_ms: 1.0,
                release_ms: 50.0,
                makeup_db: 0.0,
                knee_db: 0.0,
                dry_wet: 1.0,
            }),
            48_000.0,
        );
        d.process(&mut quiet);
        assert!((peak(&quiet[24_000..]) - 0.05).abs() < 0.002);
    }
}
