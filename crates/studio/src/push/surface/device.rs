//! Device mode, as Push does it in Live: the buttons under the screen pick
//! a device on the selected track, the eight encoders turn its settings a
//! page at a time (picked with the buttons over the screen, or Page left
//! and right). On a rack the encoders turn its eight macros and the
//! buttons over the screen go into its chains; the first button under the
//! screen comes back out.

use super::*;
use crate::devices::{Param, Settings};
use crate::rack;

/// Which device the encoders turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Focus {
    pub track: usize,
    /// On the track: a device in its chain, or None for its own sound.
    pub device: Option<usize>,
    /// Inside the rack at `device`: the chain, and the device in it.
    pub inner: Option<(usize, usize)>,
    /// The page of eight settings.
    pub page: usize,
}

/// One encoder's setting: what the screen shows, and where a turn goes.
pub struct Knob {
    pub name: String,
    pub text: String,
    /// Where it sits, 0..1.
    pub value: f32,
    pub bipolar: bool,
    param: Option<Param>,
    target: Target,
}

enum Target {
    /// The track's own sound, knob `i`.
    Sound(usize),
    /// Setting `index` of the device at `device` on the track (an inner
    /// index for a device inside a rack).
    Device { device: usize, index: usize },
}

impl Surface {
    /// Device mode's focus, back on the track's sound when the track
    /// changed or what it pointed at went away.
    pub fn focus(&self, c: &Controls) -> Focus {
        let t = c.selected_track();
        let mut f = self.focus;
        if f.track != t {
            return Focus {
                track: t,
                ..Focus::default()
            };
        }
        let devices = c.devices(t);
        match f.device {
            Some(d) if d >= devices.len() => {
                return Focus {
                    track: t,
                    ..Focus::default()
                }
            }
            Some(d) => {
                if let Some((b, x)) = f.inner {
                    let ok = matches!(&devices[d].builtin, Some(Settings::Rack(r))
                        if r.branches.get(b).is_some_and(|ch| x < ch.devices.len()));
                    if !ok {
                        f.inner = None;
                        f.page = 0;
                    }
                }
            }
            None => f.inner = None,
        }
        f
    }

    /// Whether the track has a sound of its own to show first.
    fn has_sound(c: &Controls, t: usize) -> bool {
        c.tracks.get(t).is_some_and(|i| i.sound != Sound::Audio)
    }

    /// The settings the encoders turn now, eight at most.
    pub fn device_knobs(&self, c: &Controls) -> Vec<Knob> {
        let f = self.focus(c);
        let t = f.track;
        let Some(d) = f.device else {
            if !Self::has_sound(c, t) {
                return Vec::new();
            }
            return (0..PARAMS)
                .map(|i| Knob {
                    name: c.param_name(t, i).into(),
                    text: c.param_text(t, i),
                    value: c.param(t, i),
                    bipolar: false,
                    param: None,
                    target: Target::Sound(i),
                })
                .collect();
        };
        let Some(s) = c.builtin(t, d) else {
            return Vec::new();
        };
        let (params, index): (Vec<Param>, Box<dyn Fn(usize) -> usize>) = match (&s, f.inner) {
            (Settings::Rack(r), Some((b, x))) => {
                let Some(inner) = r.branches.get(b).and_then(|ch| ch.devices.get(x)) else {
                    return Vec::new();
                };
                let Some(is) = inner.builtin.as_ref() else {
                    return Vec::new();
                };
                (is.params(), Box::new(move |p| rack::inner(b, x, p)))
            }
            // A rack's first page is its macros.
            (s, _) => (s.params(), Box::new(|p| p)),
        };
        params
            .into_iter()
            .enumerate()
            .skip(f.page * 8)
            .take(8)
            .map(|(p, param)| Knob {
                name: param.name.clone(),
                text: param.text(),
                value: param.norm(),
                bipolar: param.bipolar,
                param: Some(param),
                target: Target::Device {
                    device: d,
                    index: index(p),
                },
            })
            .collect()
    }

    /// Encoder `i` turned by `step` (a share of its whole sweep).
    pub(super) fn device_encoder(&mut self, i: usize, step: f32, c: &Controls) {
        let knobs = self.device_knobs(c);
        let Some(k) = knobs.get(i) else { return };
        let t = self.focus(c).track;
        match k.target {
            Target::Sound(i) => c.set_param(t, i, c.param(t, i) + step),
            Target::Device { device, index } => {
                let Some(p) = &k.param else { return };
                let next = if p.is_switch() {
                    // A detent's worth of turning steps a list.
                    self.turns[i] += (step.signum()) as i32;
                    if self.turns[i].abs() < 6 {
                        return;
                    }
                    let by = self.turns[i].signum() as f32;
                    self.turns[i] = 0;
                    (p.value + by).clamp(p.min, p.max)
                } else {
                    p.at_norm(p.norm() + step)
                };
                c.set_device_param(t, device, index, next);
            }
        }
    }

    /// The pages of the focused device: (label, shown now) for each.
    fn pages(&self, c: &Controls) -> Vec<(String, bool)> {
        let f = self.focus(c);
        let Some(d) = f.device else {
            return Vec::new();
        };
        let Some(s) = c.builtin(f.track, d) else {
            return Vec::new();
        };
        let n = match (&s, f.inner) {
            (Settings::Rack(r), Some((b, x))) => r
                .branches
                .get(b)
                .and_then(|ch| ch.devices.get(x))
                .and_then(|i| i.builtin.as_ref())
                .map_or(0, |s| s.params().len()),
            (s, _) => s.params().len(),
        };
        let pages = n.div_ceil(8).min(8);
        (0..pages)
            .map(|p| (format!("Page {}", p + 1), p == f.page))
            .collect()
    }

    /// Labels for the buttons over (pages, or a rack's chains) and under
    /// (the devices) the screen in Device mode.
    pub fn device_menus(&self, c: &Controls) -> (Vec<Menu>, Vec<Menu>) {
        let f = self.focus(c);
        let t = f.track;
        let devices = c.devices(t);
        let item = |label: String, on: bool| Menu { label, on };
        let rack = f.device.and_then(|d| match &devices[d].builtin {
            Some(Settings::Rack(r)) => Some(r.clone()),
            _ => None,
        });
        let lower: Vec<Menu> = match (f.device, f.inner, &rack) {
            (Some(d), Some((b, x)), Some(r)) => {
                let mut v = vec![item(format!("◀ {}", devices[d].name), false)];
                if let Some(ch) = r.branches.get(b) {
                    v.extend(
                        ch.devices
                            .iter()
                            .take(7)
                            .enumerate()
                            .map(|(i, dev)| item(dev.name.clone(), i == x)),
                    );
                }
                v
            }
            _ => {
                let mut v = Vec::new();
                if Self::has_sound(c, t) {
                    v.push(item(c.tracks[t].name.clone(), f.device.is_none()));
                }
                let room = 8 - v.len();
                v.extend(
                    devices
                        .iter()
                        .take(room)
                        .enumerate()
                        .map(|(i, dev)| item(dev.name.clone(), f.device == Some(i))),
                );
                v
            }
        };
        let upper: Vec<Menu> = match (&rack, f.inner) {
            // A rack: its chains, to go into.
            (Some(r), None) => r
                .branches
                .iter()
                .take(8)
                .map(|b| item(b.name.clone(), false))
                .collect(),
            _ => self
                .pages(c)
                .into_iter()
                .map(|(l, on)| item(l, on))
                .collect(),
        };
        (upper, lower)
    }

    /// A button over or under the screen in Device mode; false when it
    /// isn't one of its own.
    pub(super) fn device_button(&mut self, cc: u8, c: &Controls) -> bool {
        let upper = (UPPER_FIRST..UPPER_FIRST + 8).contains(&cc);
        let lower = (LOWER_FIRST..LOWER_FIRST + 8).contains(&cc);
        let mut f = self.focus(c);
        let devices = c.devices(f.track);
        let rack = f.device.and_then(|d| match &devices[d].builtin {
            Some(Settings::Rack(r)) => Some(r.clone()),
            _ => None,
        });
        if cc == PAGE_LEFT || cc == PAGE_RIGHT {
            let pages = self.pages(c).len().max(1);
            f.page = if cc == PAGE_RIGHT {
                (f.page + 1).min(pages - 1)
            } else {
                f.page.saturating_sub(1)
            };
            self.focus = f;
            return true;
        }
        if !upper && !lower {
            return false;
        }
        let i = (cc - if upper { UPPER_FIRST } else { LOWER_FIRST }) as usize;
        match (upper, f.inner, &rack) {
            // Into a rack's chain, on its first device.
            (true, None, Some(r)) => {
                if r.branches.get(i).is_some_and(|b| !b.devices.is_empty()) {
                    f.inner = Some((i, 0));
                    f.page = 0;
                }
            }
            (true, _, _) => {
                if i < self.pages(c).len() {
                    f.page = i;
                }
            }
            // Out of the chain, back on the rack.
            (false, Some(_), Some(_)) if i == 0 => {
                f.inner = None;
                f.page = 0;
            }
            (false, Some((b, _)), Some(r)) => {
                if r.branches.get(b).is_some_and(|ch| i - 1 < ch.devices.len()) {
                    f.inner = Some((b, i - 1));
                    f.page = 0;
                }
            }
            (false, _, _) => {
                let first = usize::from(Self::has_sound(c, f.track));
                let pick = if i < first {
                    Some(None)
                } else {
                    (i - first < devices.len()).then_some(Some(i - first))
                };
                if let Some(d) = pick {
                    f = Focus {
                        track: f.track,
                        device: d,
                        inner: None,
                        page: 0,
                    };
                }
            }
        }
        self.focus = f;
        true
    }

    /// The buttons over and under the screen in Device mode.
    pub(super) fn device_leds(&self, c: &Controls) -> Vec<(u8, Led)> {
        let (upper, lower) = self.device_menus(c);
        let mut v = Vec::new();
        let t = c.selected_track();
        for i in 0..8 {
            let led = |m: Option<&Menu>, on: u8| match m {
                Some(m) if m.on => Led::solid(on),
                Some(_) => Led::solid(SOFT),
                None => Led::OFF,
            };
            v.push((UPPER_FIRST + i as u8, led(upper.get(i), WHITE)));
            v.push((LOWER_FIRST + i as u8, led(lower.get(i), track_color(t))));
        }
        v
    }

    /// What the screen calls the focused device.
    pub fn device_title(&self, c: &Controls) -> String {
        let f = self.focus(c);
        let track = c.tracks.get(f.track).map_or("", |t| t.name.as_str());
        let devices = c.devices(f.track);
        match f.device.and_then(|d| devices.get(d)) {
            None => format!("Device · {track}"),
            Some(dev) => match (&dev.builtin, f.inner) {
                (Some(Settings::Rack(r)), Some((b, x))) => {
                    let ch = r.branches.get(b);
                    format!(
                        "{track} · {} · {} · {}",
                        dev.name,
                        ch.map_or("", |c| c.name.as_str()),
                        ch.and_then(|c| c.devices.get(x))
                            .map_or("", |d| d.name.as_str())
                    )
                }
                _ => format!("{track} · {}", dev.name),
            },
        }
    }
}
