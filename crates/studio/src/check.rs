//! `--check`, `--render` and `--link-check`: the engine run offline, with
//! numbers for launch timing, CPU and allocations on the audio path.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::clip::{self, SCENES};
use crate::engine::{Command, Studio, QUANTUM};

/// Counts allocations, so the check can show the audio path makes none.
struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const RATE: f64 = 48_000.0;
const TEMPO: f64 = 124.0;

fn frames_per_beat() -> f64 {
    RATE * 60.0 / TEMPO
}

/// Runs until the clock reaches `beat`.
fn run_to(studio: &mut Studio, out: &mut [[f32; 2]], beat: f64) {
    while studio.clock.beat_now() < beat {
        studio.process(out);
    }
}

/// Launch timing at one block size: the error, in frames, of each launch
/// against its bar.
fn timing(block: usize) -> Vec<i64> {
    let set = clip::demo_set();
    let (mut s, mut r) = Studio::new(&set, RATE, TEMPO);
    s.engine.keep_log(100_000);
    let mut out = vec![[0.0; 2]; block];
    let mut want = Vec::new();
    r.commands.push(Command::Scene(2)).ok();
    want.push((0.0, 2u8));
    // Launches pressed mid-bar: each should land on the next bar.
    for (press, scene) in [(5.5, 3usize), (13.3, 4), (22.9, 5), (30.01, 2)] {
        run_to(&mut s, &mut out, press);
        r.commands.push(Command::Scene(scene)).ok();
        want.push(((press / QUANTUM).ceil() * QUANTUM, scene as u8));
    }
    run_to(&mut s, &mut out, 40.0);
    let fpb = frames_per_beat();
    want.iter()
        .map(|&(beat, scene)| {
            let expect = (beat * fpb).round() as i64;
            // The first drum hit of the launched scene's clip.
            let got = s
                .engine
                .log()
                .iter()
                .find(|e| {
                    e.on && e.track == 0 && e.scene == scene && e.frame as i64 >= expect - 1000
                })
                .map_or(i64::MAX, |e| e.frame as i64);
            got - expect
        })
        .collect()
}

pub fn run() -> i32 {
    println!("openDJ Studio check: {RATE} Hz, {TEMPO} BPM, demo set (4 tracks x {SCENES} scenes)");
    println!();
    println!(
        "Launch timing (scenes launched mid-bar; first drum hit of the new clip against the bar):"
    );
    let mut worst = 0;
    for block in [64, 256, 1024] {
        let errors = timing(block);
        worst = errors.iter().map(|e| e.abs()).max().unwrap_or(0).max(worst);
        println!("  block {block:>4}: errors in frames {errors:?}");
    }
    println!(
        "  worst: {worst} frame(s) = {:.3} ms",
        worst as f64 / RATE * 1000.0
    );
    println!();

    // CPU: every track playing, 60 s of audio in 256-frame blocks.
    let set = clip::demo_set();
    let (mut s, mut r) = Studio::new(&set, RATE, TEMPO);
    let block = 256;
    let mut out = vec![[0.0; 2]; block];
    r.commands.push(Command::Scene(5)).ok();
    s.process(&mut out);
    let blocks = (60.0 * RATE) as usize / block;
    let mut times = Vec::with_capacity(blocks);
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for i in 0..blocks {
        if i % 1000 == 500 {
            // Launches and stops while measuring, as a performer would.
            let scene = 2 + (i / 1000) % 4;
            r.commands.push(Command::Scene(scene)).ok();
        }
        let t = Instant::now();
        s.process(&mut out);
        times.push(t.elapsed());
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    times.sort();
    let budget = Duration::from_secs_f64(block as f64 / RATE);
    let mean = times.iter().sum::<Duration>() / times.len() as u32;
    let p99 = times[times.len() * 99 / 100];
    let max = *times.last().unwrap();
    let pct = |d: Duration| d.as_secs_f64() / budget.as_secs_f64() * 100.0;
    println!(
        "CPU, all four tracks playing, {block}-frame blocks ({:.2} ms budget), 60 s of audio:",
        budget.as_secs_f64() * 1000.0
    );
    println!(
        "  mean {:>6.1} us ({:.2}% of one core)",
        mean.as_secs_f64() * 1e6,
        pct(mean)
    );
    println!(
        "  p99  {:>6.1} us ({:.2}%)",
        p99.as_secs_f64() * 1e6,
        pct(p99)
    );
    println!(
        "  max  {:>6.1} us ({:.2}%)",
        max.as_secs_f64() * 1e6,
        pct(max)
    );
    println!("  allocations in the audio path: {allocations}");
    let ok = worst <= 1 && allocations == 0;
    println!();
    println!("{}", if ok { "OK" } else { "FAILED" });
    if ok {
        0
    } else {
        1
    }
}

/// Writes 32 bars of the demo set going through scenes 1-8, four bars each.
pub fn render(path: &str) -> i32 {
    let set = clip::demo_set();
    let (mut s, mut r) = Studio::new(&set, RATE, TEMPO);
    let mut out = vec![[0.0; 2]; 256];
    let mut audio: Vec<[f32; 2]> = Vec::new();
    for scene in 0..SCENES {
        r.commands.push(Command::Scene(scene)).ok();
        let until = (scene as f64 + 1.0) * 4.0 * QUANTUM - 1.0;
        while s.clock.beat_now() < until {
            s.process(&mut out);
            audio.extend_from_slice(&out);
        }
    }
    r.commands.push(Command::StopAll).ok();
    for _ in 0..(RATE as usize * 3 / 256) {
        s.process(&mut out);
        audio.extend_from_slice(&out);
    }
    match write_wav(path, &audio) {
        Ok(()) => {
            println!("wrote {path}: {:.1} s", audio.len() as f64 / RATE);
            0
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            1
        }
    }
}

fn write_wav(path: &str, audio: &[[f32; 2]]) -> std::io::Result<()> {
    let data = (audio.len() * 4) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&(RATE as u32).to_le_bytes());
    b.extend_from_slice(&(RATE as u32 * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for f in audio {
        for c in f {
            let v = (c.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, b)
}

/// Joins a Link session with a second Link instance standing in for
/// openDJ, launches a clip and checks it lands on a bar of the session.
pub fn link() -> i32 {
    use rusty_link::{AblLink, SessionState};

    let other = AblLink::new(128.0);
    other.enable(true);
    let set = clip::demo_set();
    let (mut s, mut r) = Studio::new(&set, RATE, TEMPO);
    let mut lc = crate::link::LinkClock::new(TEMPO, RATE);
    let handle = lc.handle();
    handle.enable(true);
    lc.set_latency(0.0);
    s.link = Some(lc);
    s.engine.keep_log(10_000);
    let block = 256usize;
    let mut out = vec![[0.0; 2]; block];
    let dur = Duration::from_secs_f64(block as f64 / RATE);
    let mut next = Instant::now();
    let mut blocks: Vec<(u64, i64)> = Vec::new();
    let mut frame = 0u64;
    let mut launched = false;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(8) {
        if !launched && handle.num_peers() > 0 && started.elapsed() > Duration::from_millis(1500) {
            r.commands.push(Command::Launch { track: 0, scene: 2 }).ok();
            launched = true;
        }
        s.process(&mut out);
        let heard = s.link.as_ref().map_or(0, |l| l.heard_at());
        blocks.push((frame, heard));
        frame += block as u64;
        next += dur;
        if let Some(w) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    let peers = handle.num_peers();
    println!("Link peers seen by the studio: {peers}");
    println!(
        "Session tempo in the studio: {:.2} BPM (other app: 128)",
        r.monitor.tempo()
    );
    if peers == 0 {
        println!("No peer found (no multicast on this machine?): nothing to compare.");
        return 1;
    }
    let Some(first) = s.engine.log().iter().find(|e| e.on).copied() else {
        println!("Nothing played.");
        return 1;
    };
    let (bf, heard) = blocks
        .iter()
        .rev()
        .find(|(f, _)| *f <= first.frame)
        .copied()
        .unwrap_or((0, 0));
    let t = heard + ((first.frame - bf) as f64 / RATE * 1e6) as i64;
    let mut st = SessionState::new();
    other.capture_app_session_state(&mut st);
    let beat = st.beat_at_time(t, QUANTUM);
    let off = beat - (beat / QUANTUM).round() * QUANTUM;
    let ms = off * 60.0 / st.tempo() * 1000.0;
    println!(
        "First note of the launched clip, in the other app's session: beat {beat:.4} ({ms:+.3} ms from its bar)"
    );
    handle.enable(false);
    other.enable(false);
    if ms.abs() < 1.0 && (r.monitor.tempo() - 128.0).abs() < 0.01 {
        println!("OK");
        0
    } else {
        println!("FAILED");
        1
    }
}

/// Draws what Push 2's screen shows for the demo set, a clip playing on
/// each track, into a PNG: with the instrument knobs, with the mixer, in
/// Scales mode, in Steps mode holding a stage with a lock, and in the drum
/// sequencer holding a step.
pub fn push_preview(path: &str) -> i32 {
    use crate::control::{Controls, Mix};
    use crate::push::display::{self, HEIGHT, WIDTH};
    use crate::push::surface::{Mode, Surface, NOTE, PAD_FIRST};

    let set = clip::demo_set();
    let (mut s, r) = Studio::new(&set, RATE, TEMPO);
    let colors = [0xff8a65, 0x4fc3f7, 0xb69cff, 0xf48fb1];
    let c = Controls::new(&set, &colors, r.commands, r.monitor.clone());
    c.launch_scene(3);
    c.select(1, Some(3));
    c.set_mix(1, Mix::Pan, 0.35);
    c.set_mix(2, Mix::Volume, 0.8);
    let mut out = vec![[0.0; 2]; 256];
    while s.clock.beat_now() < 9.5 {
        s.process(&mut out);
    }
    let mut renderer = match display::Renderer::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut surface = Surface::new();
    let mut frames = Vec::new();
    let settle = |renderer: &mut display::Renderer, surface: &Surface| {
        // Let the animations (and the page-name toast) finish.
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(1200) {
            renderer.update(&c, surface);
            renderer.draw();
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        renderer.frame().rgb()
    };
    for mode in [Mode::Device, Mode::Mix(Mix::Volume), Mode::Scales] {
        surface.mode = mode;
        frames.push(settle(&mut renderer, &surface));
    }
    // Steps mode on the bass's stage clip, holding its locked stage.
    surface.mode = Mode::Device;
    c.select(1, Some(0));
    for m in [
        [0xB0, NOTE, 127],
        [0xB0, NOTE, 127],
        [0x90, PAD_FIRST + 5, 100],
    ] {
        surface.input(&m, &c);
    }
    frames.push(settle(&mut renderer, &surface));
    // The drum sequencer, holding the first kick.
    surface.input(&[0x80, PAD_FIRST + 5, 0], &c);
    c.select(0, Some(0));
    surface.input(&[0xB0, NOTE, 127], &c);
    surface.input(&[0x90, PAD_FIRST + 56, 100], &c);
    frames.push(settle(&mut renderer, &surface));
    let gap = 12;
    let height = HEIGHT * frames.len() + gap * (frames.len() - 1);
    let mut rgb = vec![40u8; WIDTH * height * 3];
    for (i, f) in frames.iter().enumerate() {
        let start = i * (HEIGHT + gap) * WIDTH * 3;
        rgb[start..start + f.len()].copy_from_slice(f);
    }
    let result = std::fs::File::create(path)
        .map_err(|e| e.to_string())
        .and_then(|file| {
            let mut enc =
                png::Encoder::new(std::io::BufWriter::new(file), WIDTH as u32, height as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().map_err(|e| e.to_string())?;
            w.write_image_data(&rgb).map_err(|e| e.to_string())
        });
    match result {
        Ok(()) => {
            println!("wrote {path}");
            0
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            1
        }
    }
}
