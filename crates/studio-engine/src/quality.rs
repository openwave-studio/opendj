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
                r.branches.push(Branch {
                    devices: vec![simpler(), info("Auto Filter", fresh("Auto Filter"))],
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
