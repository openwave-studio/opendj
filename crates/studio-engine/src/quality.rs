//! The checks every built-in device passes: it never allocates on the
//! audio thread, stays finite at its settings' extremes, lets its tail die
//! to true silence, leaves the sound alone at neutral settings, and glides
//! instead of clicking when a knob jumps. A device added to
//! [`crate::devices::catalog`] is checked with no more work.
//!
//! The benchmark (what each device costs per frame, against its budget)
//! is ignored by default; it only means something in a release build:
//!
//! ```text
//! cargo test --release -p opendj-studio-engine device_bench -- --ignored --nocapture
//! ```
//!
//! The knob audit (every setting of every device turned, fresh and while
//! it plays, listening for a change) prints a table; also ignored:
//!
//! ```text
//! cargo test --release -p opendj-studio-engine knob_audit -- --ignored --nocapture
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;

use crate::devices::{catalog, Device, Settings, Unit};

/// Counts the allocations made on each thread, so a test can check that a
/// stretch of audio-thread code made none (tests run on several threads).
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards to the system allocator; the counter is a plain
// thread-local `Cell` with const initialisation, which never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|a| a.set(a.get() + 1));
        // SAFETY: same contract as ours.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as ours.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.with(|a| a.set(a.get() + 1));
        // SAFETY: same contract as ours.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

const RATE: f64 = 48_000.0;
/// Frames per block, as the engine runs them.
const BLOCK: usize = 128;

/// White noise from a fixed seed, as a stand-in for music.
fn noise(frames: usize, amp: f32) -> Vec<[f32; 2]> {
    let mut seed = 0x1234_5678u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * amp
    };
    (0..frames).map(|_| [next(), next()]).collect()
}

fn sine(freq: f32, amp: f32, frames: usize) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|i| {
            let v = (i as f32 * freq * std::f32::consts::TAU / RATE as f32).sin() * amp;
            [v, v * 0.8]
        })
        .collect()
}

fn run(d: &mut Device, buf: &mut [[f32; 2]]) {
    for chunk in buf.chunks_mut(BLOCK) {
        d.process(chunk, 120.0);
    }
}

fn peak(buf: &[[f32; 2]]) -> f32 {
    buf.iter()
        .fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()))
}

/// The External Instrument plays a hardware synth, not the chain's sound;
/// it's only a gain here, and checked like any other device.
fn devices() -> Vec<(&'static str, Settings)> {
    catalog()
        .into_iter()
        .map(|(n, mut s)| {
            fill(&mut s);
            (n, s)
        })
        .collect()
}

/// A device from the catalog by name, fresh.
fn fresh(name: &str) -> Settings {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| s)
        .expect(name)
}

fn info(name: &str, s: Settings) -> crate::clip::DeviceInfo {
    crate::clip::DeviceInfo {
        name: name.into(),
        kind: "Built-in".into(),
        on: true,
        plugin: None,
        builtin: Some(s),
    }
}

/// Racks come empty from the catalog; checked with chains in them: an
/// effect rack with a reverb, a delay into a saturator and a utility; an
/// instrument rack split across the keys; a drum rack whose hats choke
/// each other.
fn fill(s: &mut Settings) {
    use crate::rack::{Branch, Kind};
    let Settings::Rack(r) = s else {
        if let Settings::Simpler(x) = s {
            x.sample = Some(test_sample());
            x.path = Some("test.wav".into());
        }
        return;
    };
    let simpler = || {
        let mut s = fresh("Simpler");
        fill(&mut s);
        info("Simpler", s)
    };
    match r.kind {
        Kind::Audio => {
            for chain in [vec!["Reverb"], vec!["Delay", "Saturator"], vec!["Utility"]] {
                r.branches.push(Branch {
                    devices: chain.into_iter().map(|n| info(n, fresh(n))).collect(),
                    ..Default::default()
                });
            }
        }
        Kind::Instrument => {
            for keys in [(0, 59), (60, 127), (48, 72)] {
                let mut devices = vec![simpler(), info("Auto Filter", fresh("Auto Filter"))];
                // The layer plays chords: a MIDI effect inside a chain.
                if keys == (48, 72) {
                    devices.insert(0, info("Chord", fresh("Chord")));
                }
                r.branches.push(Branch {
                    devices,
                    keys,
                    ..Default::default()
                });
            }
        }
        Kind::Drum => {
            for (note, choke) in [(36, 0), (38, 0), (42, 1), (46, 1), (48, 0), (50, 0)] {
                r.branches.push(Branch {
                    devices: vec![simpler()],
                    note,
                    choke,
                    ..Default::default()
                });
            }
        }
    }
}

#[test]
fn no_device_allocates_while_it_plays_or_its_knobs_turn() {
    let input = noise(BLOCK, 0.5);
    for (name, s) in devices() {
        let mut d = Device::new(&s, RATE);
        let params = s.params();
        // Warm up: the first block may touch memory set up lazily.
        d.process(&mut input.clone(), 120.0);
        let mut buf = input.clone();
        let before = allocations();
        for _ in 0..20 {
            buf.copy_from_slice(&input);
            d.process(&mut buf, 120.0);
        }
        for (i, p) in params.iter().enumerate() {
            for v in [p.min, p.max, (p.min + p.max) / 2.0] {
                d.set(i, v);
                buf.copy_from_slice(&input);
                d.process(&mut buf, 120.0);
            }
        }
        let made = allocations() - before;
        assert_eq!(made, 0, "{name} allocated {made} times on the audio thread");
    }
}

#[test]
fn every_device_stays_finite_at_its_extremes() {
    let input = noise(9_600, 1.0);
    for (name, s) in devices() {
        let params = s.params();
        // All knobs down, all up, and each one alone at its top.
        let mut corners: Vec<Vec<f32>> = vec![
            params.iter().map(|p| p.min).collect(),
            params.iter().map(|p| p.max).collect(),
        ];
        for i in 0..params.len() {
            corners.push(
                params
                    .iter()
                    .enumerate()
                    .map(|(j, p)| if i == j { p.max } else { p.value })
                    .collect(),
            );
        }
        for values in corners {
            let mut s = s.clone();
            for (i, v) in values.iter().enumerate() {
                s.set(i, *v);
            }
            let mut d = Device::new(&s, RATE);
            let mut buf = input.clone();
            run(&mut d, &mut buf);
            assert!(
                buf.iter().all(|f| f[0].is_finite() && f[1].is_finite()),
                "{name} {values:?} blew up"
            );
            // Every gain up can be very loud (eight EQ bands boosted at
            // once); what isn't allowed is a runaway that keeps growing.
            let (early, late) = (peak(&buf[2_400..4_800]), peak(&buf[7_200..]));
            assert!(
                late <= early * 4.0 + 1.0,
                "{name} {values:?} keeps growing: {early} then {late}"
            );
        }
    }
}

#[test]
fn every_tail_dies_to_true_silence() {
    // With flush-to-zero on, as on the audio thread, a tail decays to
    // exact zeros instead of lingering in denormals that cost CPU.
    opendj_engine::dsp::flush_denormals();
    for (name, s) in devices() {
        let mut d = Device::new(&s, RATE);
        let mut buf = noise(9_600, 0.5);
        run(&mut d, &mut buf);
        let mut last = [[1.0f32; 2]; BLOCK];
        for _ in 0..(60.0 * RATE) as usize / BLOCK {
            last = [[0.0; 2]; BLOCK];
            d.process(&mut last, 120.0);
        }
        for f in last {
            for v in f {
                assert!(v == 0.0 || v.is_normal(), "{name} left a denormal: {v:e}");
                assert!(v.abs() < 1e-4, "{name} still rings after a minute: {v}");
            }
        }
    }
}

/// Settings at which a device should give back exactly what it got
/// (delayed by its latency): (name, settings, input level).
fn neutral() -> Vec<(&'static str, Settings, f32)> {
    use crate::autofilter::AutoFilterSettings;
    use crate::devices::CompressorSettings;
    use crate::saturator::SaturatorSettings;
    let mut list = Vec::new();
    for (name, s) in catalog() {
        match &s {
            // Fresh Utility and EQ Eight (bands off) do nothing.
            Settings::Utility(_) | Settings::Eq8(_) | Settings::External(_) => {
                list.push((name, s, 0.5))
            }
            // Empty, and with one chain that does nothing.
            Settings::Rack(r) if !r.plays_notes() => {
                list.push((name, s.clone(), 0.5));
                let mut one = r.clone();
                one.branches.push(crate::rack::Branch {
                    devices: vec![info("Utility", fresh("Utility"))],
                    ..Default::default()
                });
                list.push((name, Settings::Rack(one), 0.5));
            }
            // Below the ceiling.
            Settings::Limiter(_) => list.push((name, s, 0.25)),
            Settings::Compressor(c) => list.push((
                name,
                Settings::Compressor(CompressorSettings {
                    threshold: 1.0,
                    knee_db: 0.0,
                    ..c.clone()
                }),
                0.1,
            )),
            Settings::Saturator(_) => list.push((
                name,
                Settings::Saturator(SaturatorSettings {
                    dry_wet: 0.0,
                    drive_db: 20.0,
                    ..Default::default()
                }),
                0.5,
            )),
            Settings::AutoFilter(_) => list.push((
                name,
                Settings::AutoFilter(AutoFilterSettings {
                    dry_wet: 0.0,
                    ..Default::default()
                }),
                0.5,
            )),
            _ => {}
        }
    }
    list
}

#[test]
fn neutral_settings_leave_the_sound_alone() {
    for (name, s, amp) in neutral() {
        let mut d = Device::new(&s, RATE);
        let input = noise(9_600, amp);
        let mut buf = input.clone();
        run(&mut d, &mut buf);
        let lag = d.latency();
        let worst = (lag..buf.len())
            .flat_map(|i| (0..2).map(move |c| (i, c)))
            .map(|(i, c)| (buf[i][c] - input[i - lag][c]).abs())
            .fold(0.0f32, f32::max);
        // 120 dB under the signal: nothing but rounding.
        assert!(
            worst <= amp * 1e-6,
            "{name} changed the sound by {:.1} dB",
            20.0 * (worst / amp).log10()
        );
    }
}

/// Devices whose knobs glide (built on [`crate::kit::Smooth`]).
fn gliding(s: &Settings) -> bool {
    matches!(
        s,
        Settings::Limiter(_) | Settings::Saturator(_) | Settings::AutoFilter(_)
    )
}

/// The biggest step between neighbouring frames.
fn steepest(buf: &[[f32; 2]]) -> f32 {
    buf.windows(2)
        .map(|w| (w[1][0] - w[0][0]).abs().max((w[1][1] - w[0][1]).abs()))
        .fold(0.0f32, f32::max)
}

#[test]
fn a_jumping_knob_glides_instead_of_clicking() {
    let frames = 24_000;
    let jump = 12_000;
    for (name, s) in devices().into_iter().filter(|(_, s)| gliding(s)) {
        for (i, p) in s.params().iter().enumerate() {
            if p.is_switch() || matches!(p.unit, Unit::Note) {
                continue;
            }
            for (from, to) in [(p.min, p.max), (p.max, p.min)] {
                let mut s = s.clone();
                s.set(i, from);
                let mut d = Device::new(&s, RATE);
                let mut buf = sine(110.0, 0.25, frames);
                for chunk in buf[..jump].chunks_mut(BLOCK) {
                    d.process(chunk, 120.0);
                }
                d.set(i, to);
                for chunk in buf[jump..].chunks_mut(BLOCK) {
                    d.process(chunk, 120.0);
                }
                // The steepest step while the knob moves, against the
                // steepest the sound has settled on either side of it.
                let around = steepest(&buf[jump..jump + 4_800]);
                let settled =
                    steepest(&buf[jump - 4_800..jump]).max(steepest(&buf[frames - 4_800..]));
                assert!(
                    around <= settled * 1.5 + 2e-3,
                    "{name} {} {from} -> {to} clicked: {around} vs {settled}",
                    p.name
                );
            }
        }
    }
}

/// What each device may cost, ns per frame at 48 kHz in a release build.
/// A frame lasts 20,833 ns, so 100 ns is half a percent of a core.
fn budget(name: &str) -> f64 {
    match name {
        "Utility" | "External Instrument" | "Gate" => 20.0,
        "EQ Eight" | "Compressor" | "Overdrive" | "Limiter" => 60.0,
        "Delay" | "Echo" | "Auto Filter" => 100.0,
        "Reverb" | "Roar" => 300.0,
        // Its curve runs four times per frame per side.
        "Saturator" => 400.0,
        "Hybrid Reverb" => 600.0,
        // Its chains (reverb, delay into saturator, utility) and the mix.
        "Audio Effect Rack" => 900.0,
        _ => 100.0,
    }
}

/// Nanoseconds per frame of a device over `input` (best of three).
fn cost(d: &mut Device, input: &[[f32; 2]]) -> f64 {
    let mut buf = vec![[0.0f32; 2]; BLOCK];
    (0..3)
        .map(|_| {
            let start = Instant::now();
            for chunk in input.chunks(BLOCK) {
                buf[..chunk.len()].copy_from_slice(chunk);
                d.process(black_box(&mut buf[..chunk.len()]), 120.0);
                black_box(&buf);
            }
            start.elapsed().as_nanos() as f64 / input.len() as f64
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn device_bench() {
    opendj_engine::dsp::flush_denormals();
    let input = noise(5 * RATE as usize, 0.5);
    println!();
    println!("openDJ Studio devices, 48 kHz stereo, ns per frame (budget)");
    let mut over = Vec::new();
    for (name, s) in devices() {
        let mut d = Device::new(&s, RATE);
        let ns = cost(&mut d, &input);
        let b = budget(name);
        println!(
            "{name:<20}{ns:>8.1}{:>8.0}{:>9.2}% core{}",
            b,
            ns / (1e9 / RATE) * 100.0,
            if ns > b { "  OVER" } else { "" }
        );
        if ns > b {
            over.push(name);
        }
    }
    if !cfg!(debug_assertions) {
        assert!(over.is_empty(), "over budget: {over:?}");
    }
}

// Instruments: the built-in ones that play the track's notes.

/// A one-second sample: a 220 Hz sine that starts and ends at full level,
/// the worst case for clicks.
fn test_sample() -> std::sync::Arc<opendj_engine::sampler::Sample> {
    let frames: Vec<[f32; 2]> = (0..RATE as usize)
        .map(|i| {
            let v = (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).cos() * 0.5;
            [v, v]
        })
        .collect();
    let audio = opendj_engine::track::Audio::from_frames(RATE as u32, frames);
    std::sync::Arc::new(opendj_engine::sampler::Sample::from_audio(&audio, RATE))
}

fn instruments() -> Vec<(&'static str, Settings)> {
    devices()
        .into_iter()
        .filter(|(_, s)| s.plays_notes())
        .collect()
}

/// Blocks of chords: every block a new chord on top of the last, then the
/// old one let go, so voices are stolen all the time.
fn chords(block: usize) -> Vec<opendj_plugin_host::shm::Midi> {
    use opendj_plugin_host::shm::Midi;
    let base = 36 + (block * 7 % 48) as u8;
    let old = 36 + ((block + 47) * 7 % 48) as u8;
    let mut m = Vec::new();
    for k in [0, 4, 7, 11, 14] {
        m.push(Midi::note_off(0, 0, old + k));
    }
    for (i, k) in [0u8, 4, 7, 11, 14].iter().enumerate() {
        m.push(Midi::note_on(i as u32 * 20, 0, base + k, 40 + i as u8 * 20));
    }
    m
}

#[test]
fn no_instrument_allocates_while_it_plays_or_its_knobs_turn() {
    for (name, s) in instruments() {
        let mut d = Device::new(&s, RATE);
        let params = s.params();
        let mut buf = vec![[0.0f32; 2]; BLOCK];
        d.play(&chords(0), &mut buf, 120.0);
        let notes: Vec<_> = (0..40).map(chords).collect();
        let before = allocations();
        for n in &notes {
            d.play(n, &mut buf, 120.0);
        }
        for (i, p) in params.iter().enumerate() {
            for v in [p.min, p.max, (p.min + p.max) / 2.0] {
                d.set(i, v);
                d.play(&notes[i % notes.len()], &mut buf, 120.0);
            }
        }
        let made = allocations() - before;
        assert_eq!(made, 0, "{name} allocated {made} times on the audio thread");
    }
}

#[test]
fn every_instrument_stays_finite_and_falls_silent() {
    opendj_engine::dsp::flush_denormals();
    for (name, s) in instruments() {
        let params = s.params();
        let corners: Vec<Vec<f32>> = vec![
            params.iter().map(|p| p.min).collect(),
            params.iter().map(|p| p.max).collect(),
            params.iter().map(|p| p.value).collect(),
        ];
        for values in corners {
            let mut s = s.clone();
            for (i, v) in values.iter().enumerate() {
                s.set(i, *v);
            }
            let mut d = Device::new(&s, RATE);
            let mut buf = vec![[0.0f32; 2]; BLOCK];
            for b in 0..200 {
                d.play(&chords(b), &mut buf, 120.0);
                assert!(
                    buf.iter().all(|f| f[0].is_finite() && f[1].is_finite()),
                    "{name} {values:?} blew up"
                );
            }
            // Everything off, then a minute for the longest release.
            let off = [opendj_plugin_host::shm::Midi {
                frame: 0,
                data: [0xB0, 123, 0],
            }];
            d.play(&off, &mut buf, 120.0);
            for _ in 0..(61.0 * RATE) as usize / BLOCK {
                d.play(&[], &mut buf, 120.0);
            }
            for f in buf {
                for v in f {
                    assert!(v == 0.0 || v.is_normal(), "{name} left a denormal: {v:e}");
                    assert!(v.abs() < 1e-4, "{name} {values:?} still sounds: {v}");
                }
            }
        }
    }
}

#[test]
fn stolen_and_ended_notes_fade_instead_of_clicking() {
    use opendj_plugin_host::shm::Midi;
    for (name, mut s) in instruments() {
        // Two voices, so every third note steals; a trimmed end. Inside
        // racks too.
        each_simpler(&mut s, &mut |x| {
            x.voices = 2;
            x.end = 0.3;
            x.mode = 1;
            x.attack_ms = 5.0;
        });
        let mut d = Device::new(&s, RATE);
        let mut out = Vec::new();
        let mut buf = vec![[0.0f32; 2]; BLOCK];
        for b in 0..600 {
            // Three notes close together (the third steals), then each
            // plays on to its trimmed end.
            let notes = match b % 100 {
                // The third lands where the first is at its peak (five
                // cycles in), the worst place to cut it.
                0 | 4 => vec![Midi::note_on(0, 0, [60, 64][b % 100 / 4], 127)],
                8 => vec![Midi::note_on(67, 0, 67, 127)],
                _ => vec![],
            };
            d.play(&notes, &mut buf, 120.0);
            out.extend_from_slice(&buf);
        }
        // A 220 Hz sine at its level moves at most this much per frame;
        // a cut is a step up to the whole level.
        let smooth = 0.5 * 2.0 * std::f32::consts::PI * 262.0 / RATE as f32 * 3.0;
        let worst = steepest(&out);
        assert!(
            worst < smooth,
            "{name} clicked: step {worst} (smooth {smooth})"
        );
    }
}

/// What each instrument may cost with eight notes held, filter on, ns per
/// frame at 48 kHz in a release build.
fn instrument_budget(name: &str) -> f64 {
    match name {
        "Simpler" => 250.0,
        // Three Simplers with Auto Filters, overlapping zones.
        "Instrument Rack" => 700.0,
        "Drum Rack" => 300.0,
        _ => 500.0,
    }
}

#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn instrument_bench() {
    use opendj_plugin_host::shm::Midi;
    opendj_engine::dsp::flush_denormals();
    println!();
    println!("openDJ Studio instruments, 8 notes, 48 kHz stereo, ns per frame (budget)");
    let mut over = Vec::new();
    for (name, mut s) in instruments() {
        if let Settings::Simpler(x) = &mut s {
            x.looping = true;
            x.filter = true;
            x.cutoff = 2_000.0;
        }
        let mut d = Device::new(&s, RATE);
        let chord: Vec<Midi> = [48u8, 52, 55, 59, 62, 64, 67, 71]
            .iter()
            .map(|&k| Midi::note_on(0, 0, k, 100))
            .collect();
        let mut buf = vec![[0.0f32; 2]; BLOCK];
        d.play(&chord, &mut buf, 120.0);
        let blocks = 5 * RATE as usize / BLOCK;
        let ns = (0..3)
            .map(|_| {
                let start = Instant::now();
                for _ in 0..blocks {
                    d.play(&[], black_box(&mut buf), 120.0);
                    black_box(&buf);
                }
                start.elapsed().as_nanos() as f64 / (blocks * BLOCK) as f64
            })
            .fold(f64::INFINITY, f64::min);
        let b = instrument_budget(name);
        println!(
            "{name:<20}{ns:>8.1}{:>8.0}{:>9.2}% core{}",
            b,
            ns / (1e9 / RATE) * 100.0,
            if ns > b { "  OVER" } else { "" }
        );
        if ns > b {
            over.push(name);
        }
    }
    if !cfg!(debug_assertions) {
        assert!(over.is_empty(), "over budget: {over:?}");
    }
}

fn each_simpler(s: &mut Settings, f: &mut dyn FnMut(&mut crate::simpler::SimplerSettings)) {
    match s {
        Settings::Simpler(x) => f(x),
        Settings::Rack(r) => {
            for b in r.branches.iter_mut() {
                for d in b.devices.iter_mut() {
                    if let Some(s) = d.builtin.as_mut() {
                        each_simpler(s, f);
                    }
                }
            }
        }
        _ => {}
    }
}

// Every knob does something: the audit behind "turning it changes the
// sound". Each setting of each device is moved to a handful of values,
// both on a fresh device and on a running one (as the panel does), and
// the sound (or, for a MIDI effect, the notes) must change. A knob that
// only matters with a switch flipped (a synced time with sync off) counts
// when flipping another setting first makes it heard.

/// Values a setting is tried at: its switch positions, else across its range.
fn tries(p: &crate::devices::Param) -> Vec<f32> {
    let steps: Vec<f32> = match p.unit {
        Unit::Toggle => vec![p.min, p.max],
        Unit::Choice(_) | Unit::Note | Unit::Count | Unit::Semitones | Unit::Key => {
            let n = (p.max - p.min).round() as usize;
            (0..=n).map(|k| p.min + k as f32).collect()
        }
        _ => [0.0, 0.1, 0.25, 0.5, 0.75, 1.0]
            .iter()
            .map(|n| p.at_norm(*n))
            .collect(),
    };
    steps
        .into_iter()
        .filter(|v| (v - p.value).abs() > 1e-6)
        .collect()
}

/// Music with dynamics: noise and a low sine, swelling from quiet to loud
/// every quarter second, so gates, compressors and limiters all move.
fn swell(frames: usize) -> Vec<[f32; 2]> {
    let n = noise(frames, 0.3);
    let s = sine(110.0, 0.4, frames);
    (0..frames)
        .map(|i| {
            let env = 0.03 + 0.97 * ((i % 12_000) as f32 / 12_000.0).powi(2);
            [(n[i][0] + s[i][0]) * env, (n[i][1] + s[i][1]) * env]
        })
        .collect()
}

const AUDIT_FRAMES: usize = 48_000;
/// Instruments play longer: held past the test sample's end (a loop), let
/// go (a release), and played again after the knob moves (a new note's
/// start).
const AUDIT_NOTES: usize = 120_064;

fn audit_frames(s: &Settings) -> usize {
    if s.plays_notes() {
        AUDIT_NOTES
    } else {
        AUDIT_FRAMES
    }
}

/// Every pad of the audit's drum rack and a chord across the key zones.
const AUDIT_KEYS: [u8; 9] = [36, 38, 42, 46, 48, 50, 60, 64, 67];

fn audit_notes(block: usize) -> Vec<opendj_plugin_host::shm::Midi> {
    use opendj_plugin_host::shm::Midi;
    let at = |f: usize| block == f / BLOCK;
    let mut m = Vec::new();
    for (i, k) in AUDIT_KEYS.iter().enumerate() {
        if at(0) || at(64_000) {
            m.push(Midi::note_on(0, 0, *k, 60 + i as u8 * 7));
        }
        if at(40_064) || at(100_096) {
            m.push(Midi::note_off(0, 0, *k));
        }
    }
    m
}

/// What the device makes: an effect from [`swell`], an instrument from
/// held chords. With `live`, the setting moves halfway through on the
/// running device.
fn render(s: &Settings, live: Option<(usize, f32)>) -> Vec<[f32; 2]> {
    let frames = audit_frames(s);
    let mut d = Device::new(s, RATE);
    // Loud enough that a limiter at 0 dB works.
    let input: Vec<[f32; 2]> = swell(frames)
        .iter()
        .map(|f| [f[0] * 2.0, f[1] * 2.0])
        .collect();
    let mut out = Vec::with_capacity(frames);
    for (b, chunk) in input.chunks(BLOCK).enumerate() {
        if b == frames / BLOCK / 2 {
            if let Some((i, v)) = live {
                d.set(i, v);
            }
        }
        let mut buf = chunk.to_vec();
        if s.plays_notes() {
            d.play(&audit_notes(b), &mut buf, 120.0);
        } else {
            d.process(&mut buf, 120.0);
        }
        out.extend_from_slice(&buf);
    }
    out
}

/// How different two renders are, against the louder one's level.
fn differs(a: &[[f32; 2]], b: &[[f32; 2]]) -> bool {
    let (mut diff, mut level) = (0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b) {
        for c in 0..2 {
            diff += ((x[c] - y[c]) as f64).powi(2);
            level += (x[c] as f64).powi(2).max((y[c] as f64).powi(2));
        }
    }
    diff > level.max(1e-9) * 1e-8
}

/// The notes a MIDI effect makes from a phrase: a chord, single notes at
/// different velocities, held and let go, over eight beats.
fn notes_out(s: &crate::midifx::MidiSettings, live: Option<(usize, f32)>) -> Vec<String> {
    use crate::midifx::{MidiFx, Note, NoteEvent};
    let mut fx = MidiFx::new(s);
    let mut phrase = Vec::new();
    for (beat, pitch, vel, len) in [
        (0.0, 60u8, 0.8f32, 1.0f64),
        (0.0, 64, 0.6, 1.0),
        (0.0, 67, 0.4, 1.0),
        (1.5, 62, 0.2, 0.25),
        (2.0, 65, 1.0, 1.5),
        (4.0, 61, 0.5, 2.0),
        (4.5, 70, 0.9, 0.5),
        (6.0, 55, 0.3, 1.0),
    ] {
        phrase.push(NoteEvent {
            beat,
            note: Note::On(pitch, vel),
        });
        phrase.push(NoteEvent {
            beat: beat + len,
            note: Note::Off(pitch),
        });
    }
    phrase.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    let mut out = Vec::with_capacity(4096);
    let mut all = Vec::new();
    let step = 0.125;
    for k in 0..(12.0 / step) as usize {
        let (lo, hi) = (k as f64 * step, (k + 1) as f64 * step);
        if k == (6.0 / step) as usize / 2 {
            if let Some((i, v)) = live {
                fx.set(i, v);
            }
        }
        let block: Vec<NoteEvent> = phrase
            .iter()
            .filter(|e| e.beat >= lo && e.beat < hi)
            .copied()
            .collect();
        out.clear();
        fx.run(&block, &mut out, hi, 120.0);
        all.extend(out.iter().map(|e| format!("{:.4} {:?}", e.beat, e.note)));
    }
    all
}

/// How a setting fared: heard on a fresh device, heard when turned live.
#[derive(Debug, Default, Clone, Copy)]
struct Heard {
    fresh: bool,
    live: bool,
    /// Only with another switch flipped first.
    conditional: bool,
}

fn audit_one(s: &Settings, i: usize) -> Heard {
    let p = &s.params()[i];
    let mut h = Heard::default();
    if let Settings::Midi(m) = s {
        let base = notes_out(m, None);
        for v in tries(p) {
            let mut m2 = m.clone();
            m2.set(i, v);
            h.fresh |= notes_out(&m2, None) != base;
            // Only the second half moved.
            let half = |x: Vec<String>| {
                x.into_iter()
                    .filter(|l| l.split(' ').next().unwrap().parse::<f64>().unwrap() >= 3.0)
                    .collect::<Vec<_>>()
            };
            h.live |= half(notes_out(m, Some((i, v)))) != half(base.clone());
            if h.fresh && h.live {
                break;
            }
        }
        return h;
    }
    let base = render(s, None);
    let second = audit_frames(s) / BLOCK / 2 * BLOCK;
    for v in tries(p) {
        let mut s2 = s.clone();
        s2.set(i, v);
        h.fresh |= differs(&render(&s2, None), &base);
        // 50 ms after the move, for a glide to land.
        let late = second + if s.plays_notes() { 0 } else { 2_400 };
        h.live |= differs(&render(s, Some((i, v)))[late..], &base[late..]);
        if h.fresh && h.live {
            break;
        }
    }
    h
}

/// Each setting, flipping each other switch (and stepping each choice)
/// first when it isn't heard at the defaults.
fn audit(s: &Settings) -> Vec<(String, Heard)> {
    let params = s.params();
    (0..params.len())
        .map(|i| {
            // A rack's unmapped macros move nothing, by design.
            if params[i].name.starts_with("Macro ") && params[i].name != "Macro 1" {
                return (params[i].name.clone(), Heard::default());
            }
            let mut h = audit_one(s, i);
            if !(h.fresh && h.live) {
                'ctx: for (j, q) in params.iter().enumerate() {
                    if j == i {
                        continue;
                    }
                    // Switches at each place; a knob low and high.
                    let values = if q.is_switch() || matches!(q.unit, Unit::Note) {
                        tries(q)
                    } else {
                        vec![q.at_norm(0.2), q.at_norm(0.8)]
                    };
                    for v in values {
                        let mut c = s.clone();
                        c.set(j, v);
                        let hc = audit_one(&c, i);
                        if hc.fresh || hc.live {
                            h.fresh |= hc.fresh;
                            h.live |= hc.live;
                            h.conditional = true;
                        }
                        if h.fresh && h.live {
                            break 'ctx;
                        }
                    }
                }
            }
            (params[i].name.clone(), h)
        })
        .collect()
}

/// A list's places that sound the same as an earlier place, at the
/// defaults: options that aren't built yet.
fn same_as(s: &Settings, i: usize) -> Vec<(String, String)> {
    let p = &s.params()[i];
    let Unit::Choice(names) = p.unit else {
        return Vec::new();
    };
    let mut heard: Vec<(usize, Vec<[f32; 2]>)> = Vec::new();
    let mut same = Vec::new();
    for k in 0..names.len() {
        let mut s2 = s.clone();
        s2.set(i, k as f32);
        // A MIDI effect's notes, as pitch and time.
        let out = match &s2 {
            Settings::Midi(m) => notes_out(m, None)
                .iter()
                .map(|l| {
                    let h = l
                        .bytes()
                        .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
                    [h as f32, 0.0]
                })
                .collect(),
            _ => render(&s2, None),
        };
        if let Some((j, _)) = heard.iter().find(|(_, o)| !differs(o, &out)) {
            same.push((names[k].to_string(), names[*j].to_string()));
        } else {
            heard.push((k, out));
        }
    }
    same
}

#[test]
#[ignore = "audit: run with -- --ignored --nocapture for the table"]
fn knob_audit() {
    let mut list = devices();
    // EQ Eight: every band on and boosted, so each band's knobs are heard.
    for (_, s) in list.iter_mut() {
        if let Settings::Eq8(e) = s {
            for b in e.bands.iter_mut() {
                b.on = true;
                b.gain_db = 6.0;
            }
        }
    }
    // Auto Filter with its LFO and envelope moving; Roar with every stage.
    for (_, s) in list.iter_mut() {
        match s {
            Settings::AutoFilter(a) => {
                a.lfo_amount = 0.5;
                a.env_amount = 0.5;
                a.morph = 0.3;
            }
            Settings::Roar(r) => {
                for st in r.stages.iter_mut() {
                    st.on = true;
                }
            }
            _ => {}
        }
    }
    // A rack's macros only move what's mapped to them: map the first one.
    for (_, s) in list.iter_mut() {
        if let Settings::Rack(r) = s {
            // Every pad heard: no choke groups.
            for b in r.branches.iter_mut() {
                b.choke = 0;
            }
            if let Some(b) = r.branches.first() {
                if let Some(d) = b.devices.iter().position(|d| d.builtin.is_some()) {
                    r.macros[0].targets.push(crate::rack::Target {
                        branch: 0,
                        device: d as _,
                        param: 0,
                        low: 0.0,
                        high: 1.0,
                    });
                }
            }
        }
    }
    println!();
    let mut dead = 0;
    for (name, s) in list {
        for (i, p) in s.params().iter().enumerate() {
            for (a, b) in same_as(&s, i) {
                println!("{name}\t{} = {a}\tSAME AS {b}", p.name);
            }
        }
        for (knob, h) in audit(&s) {
            if knob.starts_with("Macro ") && !knob.ends_with(" 1") {
                continue;
            }
            let verdict = match (h.fresh, h.live) {
                (true, true) if h.conditional => "ok (with a switch)",
                (true, true) => "ok",
                (true, false) => "DEAD LIVE (works only on load)",
                (false, true) => "LIVE ONLY",
                (false, false) => "DEAD",
            };
            if !(h.fresh && h.live) {
                dead += 1;
            }
            println!("{name}\t{knob}\t{verdict}");
        }
    }
    println!("{dead} settings not heard");
}
