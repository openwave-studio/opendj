//! A rack in the device row: which chain is open beside it, map mode, and
//! edits to its chains (devices dropped in, moved, removed; chains made and
//! taken out; knobs mapped to macros). Each edit changes the rack's
//! settings and swaps the rack for the edited one.

use std::cell::Cell;

use crate::clip::DeviceInfo;
use crate::control::{self, Controls};
use crate::devices::Settings;
use crate::plugins::{Plugins, State};
use crate::rack::{self, RackSettings};
use crate::{DeviceData, KnobData};
use slint::{Model, ModelRc};

thread_local! {
    /// The chain open beside a rack: track, the rack's place, the chain.
    static OPEN: Cell<Option<(usize, usize, usize)>> = const { Cell::new(None) };
    /// Map mode: track, the rack's place, the macro being mapped.
    static MAPPING: Cell<Option<(usize, usize, usize)>> = const { Cell::new(None) };
}

/// The rack at `at` on `track`, if it is one, with the sounds its
/// plug-ins have now (an edit restarts them).
fn rack_at(
    c: &Controls,
    p: &Plugins,
    track: usize,
    at: usize,
) -> Option<(DeviceInfo, RackSettings)> {
    let mut info = c.devices(track).get(at)?.clone();
    if !matches!(&info.builtin, Some(Settings::Rack(_))) {
        return None;
    }
    p.keep_sounds(track, at, &mut info);
    match &info.builtin {
        Some(Settings::Rack(r)) => {
            let r = r.clone();
            Some((info, r))
        }
        _ => None,
    }
}

/// The chain open on `track`: the one picked, while its rack is still
/// there, else the first chain of the track's first rack.
pub fn open_chain(track: usize, devices: &[DeviceInfo]) -> Option<(usize, usize)> {
    let rack_of = |d: &DeviceInfo| match &d.builtin {
        Some(Settings::Rack(r)) => Some(r.branches.len()),
        _ => None,
    };
    if let Some((t, at, b)) = OPEN.get() {
        if t == track && devices.get(at).and_then(rack_of).is_some_and(|n| b < n) {
            return Some((at, b));
        }
    }
    devices
        .iter()
        .position(|d| rack_of(d).is_some_and(|n| n > 0))
        .map(|at| (at, 0))
}

/// The macro being mapped on the rack at `at`, if map mode is on there.
pub fn mapping(track: usize, at: usize) -> Option<usize> {
    MAPPING
        .get()
        .and_then(|(t, a, m)| (t == track && a == at).then_some(m))
}

/// Renumbers a panel's knobs as settings of the rack (chain `b`, device
/// `d`), marking those a macro follows.
fn inner_knobs(
    m: &ModelRc<KnobData>,
    old: Option<&ModelRc<KnobData>>,
    r: &RackSettings,
    b: usize,
    d: usize,
) -> ModelRc<KnobData> {
    let list: Vec<KnobData> = m
        .iter()
        .map(|mut k| {
            let p = k.index.max(0) as usize;
            k.mapped = r.mapped(b, d, p).map_or(0, |m| m as i32 + 1);
            k.index = rack::inner(b, d, p) as i32;
            k
        })
        .collect();
    crate::reuse(old, list)
}

/// The devices of chain `b` of rack `r`, as the device row shows them.
pub fn inner_rows(
    r: &RackSettings,
    b: usize,
    old: Option<&ModelRc<DeviceData>>,
    state: &dyn Fn(usize) -> Option<State>,
) -> Vec<DeviceData> {
    let Some(chain) = r.branches.get(b) else {
        return Vec::new();
    };
    chain
        .devices
        .iter()
        .enumerate()
        .map(|(d, info)| {
            let before = old.and_then(|o| o.row_data(d));
            let before = before.as_ref();
            let state = state(d);
            let note = match &state {
                Some(State::Running) => "Playing".to_string(),
                Some(s) => s.text(),
                None if info.builtin.is_some() => String::new(),
                None => "Live device: no built-in match yet".to_string(),
            };
            let mut row = crate::device_row(
                DeviceData {
                    name: info.name.as_str().into(),
                    kind: info.kind.as_str().into(),
                    note: note.into(),
                    can_open: state == Some(State::Running),
                    on: info.on,
                    instrument: control::is_instrument(info),
                    midi: control::is_midi_effect(info),
                    on_index: rack::inner(b, d, rack::ON) as i32,
                    ..DeviceData::default()
                },
                info.builtin.as_ref(),
                before,
            );
            row.params = inner_knobs(&row.params, before.map(|o| &o.params), r, b, d);
            row.knobs = inner_knobs(&row.knobs, before.map(|o| &o.knobs), r, b, d);
            row.switches = inner_knobs(&row.switches, before.map(|o| &o.switches), r, b, d);
            row
        })
        .collect()
}

/// Puts the edited rack back in its place and opens chain `open`.
fn put_back(
    p: &Plugins,
    track: usize,
    at: usize,
    mut info: DeviceInfo,
    r: RackSettings,
    open: Option<usize>,
) {
    let chains = r.branches.len();
    info.builtin = Some(Settings::Rack(r));
    if p.replace(track, at, info) {
        let b = open.unwrap_or(0).min(chains.saturating_sub(1));
        OPEN.set(Some((track, at, b)));
    }
}

pub fn select_chain(c: &Controls, track: usize, at: usize, b: usize) {
    OPEN.set(Some((track, at, b)));
    c.bump();
}

pub fn remove_chain(c: &Controls, p: &Plugins, track: usize, at: usize, b: usize) {
    if let Some((info, mut r)) = rack_at(c, p, track, at) {
        if r.remove_chain(b).is_some() {
            put_back(p, track, at, info, r, Some(b.saturating_sub(1)));
        }
    }
}

pub fn remove_inner(c: &Controls, p: &Plugins, track: usize, at: usize, b: usize, d: usize) {
    if let Some((info, mut r)) = rack_at(c, p, track, at) {
        if r.remove(b, d).is_some() {
            put_back(p, track, at, info, r, Some(b));
        }
    }
}

/// Map mode on or off for the rack at `at`; it starts on the first macro.
pub fn toggle_map(c: &Controls, track: usize, at: usize) {
    MAPPING.set(match MAPPING.get() {
        Some((t, a, _)) if t == track && a == at => None,
        _ => Some((track, at, 0)),
    });
    c.bump();
}

pub fn pick_macro(c: &Controls, track: usize, at: usize, m: usize) {
    if mapping(track, at).is_some() && m < rack::MACROS {
        MAPPING.set(Some((track, at, m)));
        c.bump();
    }
}

/// A knob inside the rack picked in map mode: it follows the macro being
/// mapped, or stops following it.
pub fn map_knob(c: &Controls, p: &Plugins, track: usize, at: usize, index: usize) {
    let (Some(m), Some((b, d, param))) = (mapping(track, at), rack::split(index)) else {
        return;
    };
    if let Some((info, mut r)) = rack_at(c, p, track, at) {
        r.toggle_map(m, b, d, param);
        put_back(p, track, at, info, r, Some(b));
    }
}

/// What was dragged into a rack.
pub enum Source {
    /// A new device (from the browser, or a Simpler made from a sample).
    New(Box<DeviceInfo>),
    /// A device on a track's chain: the track and its place.
    Chain(usize, usize),
    /// A device inside a rack: track, the rack's place, chain, place.
    Inner(usize, usize, usize, usize),
}

/// Where a device is let go in a rack: the track, the rack's place, the
/// chain (None for a new one) and the place in it (None for its end).
pub struct Into {
    pub track: usize,
    pub at: usize,
    pub chain: Option<usize>,
    pub place: Option<usize>,
}

/// A device let go into a rack. `copy` leaves a device where it came from.
pub fn drop_in(c: &Controls, p: &Plugins, into: Into, from: Source, copy: bool) {
    let Into {
        track,
        at,
        chain: to,
        place,
    } = into;
    let Some((info, mut r)) = rack_at(c, p, track, at) else {
        return;
    };
    let mut at = at;
    match from {
        Source::Inner(t, a, b, d) if t == track && a == at => {
            // Along its own rack.
            let Some(device) = r.branches.get(b).and_then(|x| x.devices.get(d)).cloned() else {
                return;
            };
            let place = place.unwrap_or(usize::MAX);
            let open = match to {
                Some(to) if to == b && !copy => {
                    let place = if place > d { place - 1 } else { place };
                    if !r.move_device(b, d, place) {
                        return;
                    }
                    Some(b)
                }
                _ => {
                    if !copy {
                        r.remove(b, d);
                    }
                    match to {
                        Some(to) => r.insert(to, place, device).map(|_| to),
                        None => r.add_chain(Some(device)),
                    }
                }
            };
            if open.is_some() {
                put_back(p, track, at, info, r, open);
            }
        }
        Source::Inner(t, a, b, d) => {
            // From another rack: copied (taking it out of there too is a
            // later step).
            let device = rack_at(c, p, t, a)
                .and_then(|(_, other)| other.branches.get(b)?.devices.get(d).cloned());
            if let Some(device) = device {
                if let Some(open) = add(&mut r, to, place, device) {
                    put_back(p, track, at, info, r, Some(open));
                }
            }
        }
        Source::Chain(t, d) => {
            if t == track && d == at {
                return;
            }
            let Some(mut device) = c.devices(t).get(d).cloned() else {
                return;
            };
            p.keep_sounds(t, d, &mut device);
            let Some(open) = add(&mut r, to, place, device) else {
                return;
            };
            if !copy {
                p.remove(t, d);
                if t == track && d < at {
                    at -= 1;
                }
            }
            put_back(p, track, at, info, r, Some(open));
        }
        Source::New(device) => {
            if let Some(open) = add(&mut r, to, place, *device) {
                put_back(p, track, at, info, r, Some(open));
            }
        }
    }
}

/// Puts `device` in chain `to` (or a new chain). Returns the chain.
fn add(
    r: &mut RackSettings,
    to: Option<usize>,
    place: Option<usize>,
    device: DeviceInfo,
) -> Option<usize> {
    match to {
        Some(to) => r
            .insert(to, place.unwrap_or(usize::MAX), device)
            .map(|_| to),
        None => r.add_chain(Some(device)),
    }
}

/// A device inside a rack dragged out onto a track's chain: it leaves the
/// rack and goes where it was let go (or stays, with `copy`).
pub fn drag_out(
    c: &Controls,
    p: &Plugins,
    (track, at, b, d): (usize, usize, usize, usize),
    to_track: usize,
    place: Option<usize>,
    copy: bool,
) -> bool {
    let Some((info, mut r)) = rack_at(c, p, track, at) else {
        return false;
    };
    let Some(device) = r.branches.get(b).and_then(|x| x.devices.get(d)).cloned() else {
        return false;
    };
    // Off the track the rack is on: the rack's place may move once the
    // device lands, so the rack goes back first.
    if !copy {
        r.remove(b, d);
        put_back(p, track, at, info, r, Some(b));
    }
    p.drop_new(to_track, place, device).is_some()
}

/// Setting `i` of a device: for a rack, one inside it when `i` is an
/// inner index (its on switch included).
pub fn param(s: &Settings, i: usize) -> Option<crate::devices::Param> {
    match (s, rack::split(i)) {
        (Settings::Rack(r), Some((b, d, p))) => {
            let dev = r.branches.get(b)?.devices.get(d)?;
            if p == rack::ON {
                return Some(crate::devices::Param::toggle("On", dev.on));
            }
            dev.builtin.as_ref()?.params().into_iter().nth(p)
        }
        _ => s.params().into_iter().nth(i),
    }
}
