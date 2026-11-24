//! The audio devices to choose from, found once and kept.
//!
//! Listing the ASIO devices loads every installed ASIO driver in turn
//! (cpal 0.18): seconds with a few installed for hardware that isn't
//! plugged in, and loading the driver of an interface that plays through
//! Windows Audio can cut that stream off. So the devices are scanned once
//! at start-up, before any output opens ([`scan_devices`]), and the
//! settings and menus read what was found. Windows Audio is looked at
//! again, quickly, when Settings › Audio opens; ASIO only when the DJ asks
//! (Refresh devices).
//!
//! ASIO allows one driver loaded at a time in a process, so whatever loads
//! one (a scan, a lookup, opening a stream) holds [`asio_guard`]. The ASIO
//! devices found are kept: a stream opens from a kept device by loading
//! only its own driver, where looking it up again loads every driver listed
//! before it, and stops at the first one while another is in use.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};

use super::{device_name, hosts, is_asio, no_audio, OutputDevice};

/// How much of the devices to scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceScan {
    /// The system's own audio devices (Windows Audio on Windows): quick,
    /// and no ASIO driver is loaded. The ASIO devices stay as last found.
    System,
    /// Every audio system's, ASIO drivers included.
    All,
}

/// What one audio system's scan found.
#[derive(Clone, Default)]
struct Found {
    outputs: Vec<OutputDevice>,
    inputs: Vec<OutputDevice>,
}

#[derive(Default)]
struct Lists {
    /// The system's own devices.
    system: Found,
    /// The ASIO drivers' devices, named with their "ASIO: " prefix.
    asio: Found,
    default_output: Option<String>,
    /// When the system's devices were last scanned; `None` before the
    /// first scan.
    scanned_at: Option<Instant>,
    /// The ASIO devices found, by their listed name, to open streams from.
    kept: HashMap<String, cpal::Device>,
    /// The last ASIO scan went through every driver (openDJ held none):
    /// an ASIO device it didn't find, and no lookup found since, is
    /// missing (see [`known_missing`]).
    asio_complete: bool,
}

fn lists() -> MutexGuard<'static, Lists> {
    static LISTS: OnceLock<Mutex<Lists>> = OnceLock::new();
    LISTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Goes up by one each time the lists change.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// A scan runs on its own thread (see [`rescan_devices`]).
static SCANNING: AtomicBool = AtomicBool::new(false);
/// Refresh devices was asked for while a quick scan ran: a full one
/// follows it.
static SCAN_ALL_NEXT: AtomicBool = AtomicBool::new(false);
/// One of openDJ's streams holds an ASIO driver. An ASIO scan then stops
/// at the first other driver, so the ASIO devices it doesn't reach are
/// kept as they were.
static ASIO_HELD: AtomicBool = AtomicBool::new(false);
/// A quick scan this soon after the last one is skipped: switching back
/// and forth between Settings pages doesn't scan each time.
const FRESH: Duration = Duration::from_secs(3);

/// Scans the devices now, on this thread. At start-up, call it before the
/// engine opens its outputs.
pub fn scan_devices(what: DeviceScan) {
    if no_audio() {
        lists().scanned_at = Some(Instant::now());
        return;
    }
    let started = Instant::now();
    let host = cpal::default_host();
    let system = found(&host, "", None);
    let default_output = host.default_output_device().map(|d| device_name(&d));
    let system_time = started.elapsed();
    let asio = match what {
        DeviceScan::All => scan_asio(),
        DeviceScan::System => None,
    };
    let held = ASIO_HELD.load(Ordering::Acquire);
    eprintln!(
        "audio devices: {} outputs, {} inputs in {} ms{}",
        system.outputs.len(),
        system.inputs.len(),
        system_time.as_millis(),
        match &asio {
            Some((f, _, time)) => format!(
                "; ASIO: {} outputs, {} inputs in {} ms{}",
                f.outputs.len(),
                f.inputs.len(),
                time.as_millis(),
                if held { " (a driver is in use)" } else { "" }
            ),
            None => String::new(),
        }
    );
    let mut l = lists();
    l.system = system;
    l.default_output = default_output;
    l.scanned_at = Some(Instant::now());
    if let Some((asio, kept, _)) = asio {
        if held {
            merge(&mut l.asio.outputs, asio.outputs);
            merge(&mut l.asio.inputs, asio.inputs);
            l.kept.extend(kept);
        } else {
            l.asio = asio;
            l.kept = kept.into_iter().collect();
            l.asio_complete = true;
        }
    }
    drop(l);
    GENERATION.fetch_add(1, Ordering::Release);
}

/// Scans the devices on a thread of its own, so the caller never waits;
/// [`devices_generation`] goes up when it's done. A quick scan
/// ([`DeviceScan::System`]) is skipped when the last one was moments ago
/// or one is under way.
pub fn rescan_devices(what: DeviceScan) {
    if SCANNING.swap(true, Ordering::AcqRel) {
        if what == DeviceScan::All {
            SCAN_ALL_NEXT.store(true, Ordering::Release);
        }
        return;
    }
    let fresh = lists().scanned_at.is_some_and(|t| t.elapsed() < FRESH);
    if what == DeviceScan::System && fresh {
        SCANNING.store(false, Ordering::Release);
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("opendj-device-scan".into())
        .spawn(move || {
            scan_devices(what);
            while SCAN_ALL_NEXT.swap(false, Ordering::AcqRel) {
                scan_devices(DeviceScan::All);
            }
            SCANNING.store(false, Ordering::Release);
        });
    if let Err(e) = spawned {
        eprintln!("audio devices: can't scan: {e}");
        SCANNING.store(false, Ordering::Release);
    }
}

/// Goes up by one each time the device lists change: a scan finished, or
/// an ASIO device opened with other channels than were listed.
pub fn devices_generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// Whether a scan started by [`rescan_devices`] is under way.
pub fn devices_scanning() -> bool {
    SCANNING.load(Ordering::Acquire)
}

/// The output devices found, the system's first, then ASIO's.
pub(super) fn outputs() -> Vec<OutputDevice> {
    let l = scanned();
    l.system
        .outputs
        .iter()
        .chain(&l.asio.outputs)
        .cloned()
        .collect()
}

/// The input devices found, the system's first, then ASIO's.
pub(super) fn inputs() -> Vec<OutputDevice> {
    let l = scanned();
    l.system
        .inputs
        .iter()
        .chain(&l.asio.inputs)
        .cloned()
        .collect()
}

/// The system's default output device when last scanned.
pub(super) fn default_output() -> Option<String> {
    scanned().default_output.clone()
}

/// The lists, scanned first if they never were.
fn scanned() -> MutexGuard<'static, Lists> {
    if lists().scanned_at.is_none() {
        scan_devices(DeviceScan::All);
    }
    lists()
}

/// The ASIO device listed as `name` as last found, to open a stream from
/// without listing the drivers again.
pub(super) fn kept(name: &str) -> Option<cpal::Device> {
    lists().kept.get(name).cloned()
}

/// Keeps the ASIO device listed as `name`, just found, and lists it with
/// its channels.
pub(super) fn keep(name: &str, device: &cpal::Device) {
    if !is_asio(name) {
        return;
    }
    let outputs = max_channels(device.supported_output_configs());
    let inputs = max_channels(device.supported_input_configs());
    let mut guard = lists();
    let l = &mut *guard;
    l.kept.insert(name.to_string(), device.clone());
    let mut changed = false;
    for (list, channels) in [(&mut l.asio.outputs, outputs), (&mut l.asio.inputs, inputs)] {
        let Some(channels) = channels else { continue };
        let entry = OutputDevice {
            name: name.to_string(),
            channels,
        };
        if !list.contains(&entry) {
            merge(list, vec![entry]);
            changed = true;
        }
    }
    drop(guard);
    if changed {
        GENERATION.fetch_add(1, Ordering::Release);
    }
}

/// Forgets the ASIO device listed as `name`: it's gone, or wouldn't open.
/// It's looked up again next time.
pub(super) fn forget(name: &str) {
    lists().kept.remove(name);
}

/// Whether the ASIO device listed as `name` is known to be missing: the
/// last ASIO scan went through every driver without finding it, and no
/// lookup has found it since. Opening it then looks no further: that
/// would load every driver again, seconds each time, for a device that
/// isn't there. Looking for a lost device to come back ([`super::look`])
/// still loads them.
pub(super) fn known_missing(name: &str) -> bool {
    let l = lists();
    is_asio(name) && l.asio_complete && !l.kept.contains_key(name)
}

/// Says whether one of openDJ's streams holds an ASIO driver.
pub(super) fn set_asio_held(held: bool) {
    ASIO_HELD.store(held, Ordering::Release);
}

/// The devices `host` lists, named with `prefix`, each asked once for its
/// outputs and inputs; with `kept`, the devices themselves too.
fn found(
    host: &cpal::Host,
    prefix: &str,
    mut kept: Option<&mut Vec<(String, cpal::Device)>>,
) -> Found {
    let mut f = Found::default();
    let Ok(devices) = host.devices() else {
        return f;
    };
    for d in devices {
        let name = format!("{prefix}{}", device_name(&d));
        let outputs = max_channels(d.supported_output_configs());
        let inputs = max_channels(d.supported_input_configs());
        if let Some(channels) = outputs {
            f.outputs.push(OutputDevice {
                name: name.clone(),
                channels,
            });
        }
        if let Some(channels) = inputs {
            f.inputs.push(OutputDevice {
                name: name.clone(),
                channels,
            });
        }
        if let Some(kept) = kept.as_mut() {
            if outputs.is_some() || inputs.is_some() {
                kept.push((name, d));
            }
        }
    }
    f.outputs.dedup_by(|a, b| a.name == b.name);
    f.inputs.dedup_by(|a, b| a.name == b.name);
    f
}

/// The most channels of any of a device's configs; `None` if it has none
/// that way (an output-only device's inputs).
fn max_channels<I: Iterator<Item = cpal::SupportedStreamConfigRange>, E>(
    configs: Result<I, E>,
) -> Option<u16> {
    configs.ok()?.map(|c| c.channels()).max()
}

/// The ASIO drivers' devices, the devices themselves and how long it took;
/// `None` where there is no ASIO.
#[allow(clippy::type_complexity)]
fn scan_asio() -> Option<(Found, Vec<(String, cpal::Device)>, Duration)> {
    let (prefix, host) = hosts().into_iter().find(|(p, _)| !p.is_empty())?;
    let _asio = asio_guard();
    let started = Instant::now();
    let mut kept = Vec::new();
    let found = found(&host, prefix, Some(&mut kept));
    Some((found, kept, started.elapsed()))
}

/// Puts `new` into `list`: entries of the same name are replaced, others
/// added at the end.
fn merge(list: &mut Vec<OutputDevice>, new: Vec<OutputDevice>) {
    for d in new {
        match list.iter_mut().find(|x| x.name == d.name) {
            Some(x) => *x = d,
            None => list.push(d),
        }
    }
}

static ASIO: Mutex<()> = Mutex::new(());

thread_local! {
    /// How many [`AsioGuard`]s this thread holds: a guard taken inside
    /// another (opening the outputs looks a device up) doesn't wait.
    static ASIO_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Held while ASIO drivers may be loaded: see [`asio_guard`].
pub(super) struct AsioGuard(#[allow(dead_code)] Option<MutexGuard<'static, ()>>);

impl Drop for AsioGuard {
    fn drop(&mut self) {
        ASIO_DEPTH.with(|d| d.set(d.get() - 1));
    }
}

/// Waits until no other thread may be loading an ASIO driver, and keeps
/// them from it until dropped. A thread may take it again while it has it.
pub(super) fn asio_guard() -> AsioGuard {
    let outer = ASIO_DEPTH.with(|d| d.get() == 0);
    let guard = outer.then(|| ASIO.lock().unwrap_or_else(PoisonError::into_inner));
    ASIO_DEPTH.with(|d| d.set(d.get() + 1));
    AsioGuard(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(name: &str, channels: u16) -> OutputDevice {
        OutputDevice {
            name: name.into(),
            channels,
        }
    }

    #[test]
    fn merging_replaces_by_name_and_adds_the_rest() {
        let mut list = vec![d("ASIO: A", 2), d("ASIO: B", 4)];
        merge(&mut list, vec![d("ASIO: B", 8), d("ASIO: C", 2)]);
        assert_eq!(
            list,
            vec![d("ASIO: A", 2), d("ASIO: B", 8), d("ASIO: C", 2)]
        );
    }

    // One test: tests run in parallel, and the guard is shared.
    #[test]
    fn the_asio_guard_is_taken_again_on_its_thread_but_not_on_others() {
        let outer = asio_guard();
        // Doesn't wait for itself.
        let inner = asio_guard();
        assert_eq!(ASIO_DEPTH.with(Cell::get), 2);
        drop(inner);
        // Still held by `outer`: another thread can't have it.
        let other = std::thread::spawn(|| ASIO.try_lock().is_err());
        assert!(other.join().unwrap());
        drop(outer);
        assert_eq!(ASIO_DEPTH.with(Cell::get), 0);
    }
}
