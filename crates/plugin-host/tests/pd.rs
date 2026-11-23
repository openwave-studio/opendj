//! Runs Pure Data patches through the helper (feature `pd`):
//! `cargo test -p opendj-plugin-host --features pd --test pd`.
#![cfg(feature = "pd")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use opendj_plugin_host::{spawn, Connection, Helper, Options, Outcome, Plugin, Transport};

const DEADLINE: Duration = Duration::from_millis(500);

fn helper() -> PathBuf {
    env!("CARGO_BIN_EXE_opendj-plugin-host").into()
}

fn patch(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pd")
        .join(name)
}

fn start(name: &str) -> (Connection, Helper) {
    spawn(&Options {
        picks: Vec::new(),
        helper: helper(),
        plugin: Plugin::Pd { file: patch(name) },
        sample_rate: 48_000,
        max_frames: 1024,
        title: None,
        state: None,
    })
    .expect("the patch loads")
}

fn transport() -> Transport {
    Transport {
        bpm: 120.0,
        beat: 0.0,
        playing: true,
    }
}

/// Runs a block of `frames` of a steady 0.5 (left) and -0.25 (right).
fn run(conn: &mut Connection, frames: usize) -> (Outcome, Vec<f32>, Vec<f32>) {
    let l = vec![0.5; frames];
    let r = vec![-0.25; frames];
    let mut ol = vec![0.0; frames];
    let mut or = vec![0.0; frames];
    let out = conn.process([&l, &r], [&mut ol, &mut or], transport(), DEADLINE);
    (out, ol, or)
}

#[test]
fn a_patch_describes_its_parameters() {
    let (_conn, helper) = start("beat-echo.pd");
    let info = helper.info();
    assert_eq!(info.name, "beat-echo");
    let names: Vec<&str> = info.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["beats", "feedback", "mix"]);
    assert_eq!(info.params[0].min, 0.125);
    assert_eq!(info.params[0].max, 2.0);
    assert_eq!(info.params[0].default, 0.75);
    assert_eq!(info.latency, 0);
}

#[test]
fn the_gain_knob_changes_the_sound() {
    let (mut conn, _helper) = start("gain.pd");
    let (out, l, r) = run(&mut conn, 256);
    assert_eq!(out, Outcome::Wet);
    assert!((l[255] - 0.5).abs() < 1e-6, "{}", l[255]);
    assert!((r[255] + 0.25).abs() < 1e-6, "{}", r[255]);
    conn.set_param(0, 0.5);
    let (_, l, r) = run(&mut conn, 256);
    assert!((l[255] - 0.25).abs() < 1e-6, "{}", l[255]);
    assert!((r[255] + 0.125).abs() < 1e-6, "{}", r[255]);
}

#[test]
fn blocks_that_arent_whole_ticks_come_out_one_tick_late() {
    let (mut conn, _helper) = start("gain.pd");
    // 100 frames isn't a multiple of Pd's 64: the first 64 are silence.
    let (_, l, _) = run(&mut conn, 100);
    assert!(l[..64].iter().all(|&x| x == 0.0));
    assert!((l[64] - 0.5).abs() < 1e-6, "{}", l[64]);
    for _ in 0..5 {
        let (_, l, r) = run(&mut conn, 100);
        assert!(l.iter().all(|&x| (x - 0.5).abs() < 1e-6));
        assert!(r.iter().all(|&x| (x + 0.25).abs() < 1e-6));
    }
}

#[test]
fn the_echo_repeats_on_the_beat() {
    // 120 BPM, 0.75 beats: 375 ms, 18 000 frames at 48 kHz.
    let (mut conn, _helper) = start("beat-echo.pd");
    let frames = 256;
    let mut left = Vec::new();
    for block in 0..120 {
        let l: Vec<f32> = (0..frames)
            .map(|i| if block == 0 && i == 0 { 1.0 } else { 0.0 })
            .collect();
        let r = l.clone();
        let mut ol = vec![0.0; frames];
        let mut or = vec![0.0; frames];
        conn.process([&l, &r], [&mut ol, &mut or], transport(), DEADLINE);
        left.extend(ol);
    }
    let peak = (1000..left.len())
        .max_by(|&a, &b| left[a].abs().total_cmp(&left[b].abs()))
        .unwrap();
    // The delay time glides in over a few ms after load, so allow a little.
    assert!(
        (peak as i64 - 18_000).abs() < 400,
        "the first echo is at frame {peak}"
    );
}

#[test]
fn cyclone_and_else_objects_are_built_in() {
    // Left: [cyclone/clip~ -1 1.25]. Right: [lores~] (cyclone, plain name)
    // into [else/trunc~] into [*~ 0.5]. A missing object breaks its chain
    // and leaves silence.
    let (mut conn, helper) = start("libs-check.pd");
    assert_eq!(helper.info().params[0].name, "level");
    let frames = 256;
    let l = vec![1.75; frames];
    let r = vec![1.75; frames];
    let (mut ol, mut or) = (vec![0.0; frames], vec![0.0; frames]);
    // lores~ settles over a few blocks.
    for _ in 0..40 {
        conn.process([&l, &r], [&mut ol, &mut or], transport(), DEADLINE);
    }
    assert!((ol[255] - 1.25).abs() < 1e-6, "clip~: {}", ol[255]);
    assert!(
        (or[255] - 0.5).abs() < 1e-6,
        "lores~ and trunc~: {}",
        or[255]
    );
}

/// Not a pass/fail test: what a Pd patch costs through the helper.
/// `cargo test --release -p opendj-plugin-host --features pd --test pd -- --ignored --nocapture`
#[test]
#[ignore]
fn pd_bench() {
    for name in ["gain.pd", "beat-echo.pd"] {
        let (mut conn, _helper) = start(name);
        let frames = 256;
        for _ in 0..200 {
            run(&mut conn, frames);
        }
        let mut times: Vec<Duration> = (0..2000)
            .map(|_| {
                let t = Instant::now();
                run(&mut conn, frames);
                t.elapsed()
            })
            .collect();
        times.sort();
        println!(
            "{name}: {frames} frames (5.3 ms at 48 kHz) round trip median {:?}, p99 {:?}",
            times[times.len() / 2],
            times[times.len() * 99 / 100]
        );
    }
}

#[test]
fn a_lua_display_shows_on_the_deck_and_takes_clicks() {
    use opendj_plugin_host::display::{Display, Mouse};
    let (mut conn, helper) = start("lua-level.pd");
    let path = helper.display_path();
    let wait_for = |what: &str, mut ok: Box<dyn FnMut() -> bool + '_>| {
        let t = Instant::now();
        while !ok() {
            assert!(t.elapsed() < Duration::from_secs(5), "no {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let mut deck = None;
    wait_for(
        "display file",
        Box::new(|| {
            deck = Display::open(&path);
            deck.is_some()
        }),
    );
    let deck = deck.unwrap();
    assert_eq!(deck.content(), (200, 40));
    // Shown at twice its size.
    deck.set_view(400, 80);
    let mut px = Vec::new();
    let mut frame = (0, 0, 0);
    wait_for(
        "first picture",
        Box::new(|| deck.read_frame(&mut px).map(|f| frame = f).is_some()),
    );
    assert_eq!((frame.0, frame.1), (400, 80));
    let at = |px: &[u8], x: usize, y: usize| {
        let i = (y * 400 + x) * 4;
        [px[i], px[i + 1], px[i + 2]]
    };
    assert_eq!(at(&px, 300, 40), [0xb6, 0x9c, 0xff], "full fader");
    // Click half way, in the display's own points.
    deck.send_mouse(100.0, 20.0, Mouse::Down);
    deck.send_mouse(100.0, 20.0, Mouse::Up);
    let first = frame.2;
    wait_for(
        "new picture",
        Box::new(|| deck.read_frame(&mut px).is_some_and(|f| f.2 != first)),
    );
    assert_eq!(at(&px, 300, 40), [0x16, 0x18, 0x19], "half fader");
    let (_, l, _) = run(&mut conn, 256);
    assert!((l[255] - 0.25).abs() < 1e-6, "{}", l[255]);
}
