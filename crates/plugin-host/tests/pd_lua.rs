//! A patch's Lua display, in this process (feature `pd`):
//! `cargo test -p opendj-plugin-host --features pd --test pd_lua`.
//! One test only: libpd is one engine per process.
#![cfg(feature = "pd")]

use std::path::PathBuf;

use opendj_plugin_host::effects::{Effect, Mouse};
use opendj_plugin_host::Transport;

fn patch(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pd")
        .join(name)
}

fn run(effect: &mut dyn Effect) -> f32 {
    let input = vec![0.5; 64];
    let (mut l, mut r) = (vec![0.0; 64], vec![0.0; 64]);
    let t = Transport {
        bpm: 120.0,
        beat: 0.0,
        playing: true,
    };
    effect.process([&input, &input], [&mut l, &mut r], &[], &t, &[]);
    l[63]
}

/// The pixel at (x, y) points, as 0RGB.
fn pixel(pixels: &[u32], width: u32, scale: u32, x: u32, y: u32) -> u32 {
    pixels[(y * scale * width + x * scale) as usize]
}

#[test]
fn a_lua_display_draws_and_takes_the_mouse() {
    let mut loaded = opendj_plugin_host::pd::load(&patch("lua-level.pd"), 48_000, 64, &[])
        .expect("the patch loads");
    let main = loaded
        .main
        .as_mut()
        .expect("a patch that draws has an editor");
    let drawn = main.drawn_editor().expect("drawn by the helper");
    assert_eq!(drawn.size(), (200.0, 40.0));
    assert!(drawn.changed());

    let (scale, w, h) = (2, 400, 80);
    let mut pixels = vec![0; (w * h) as usize];
    drawn.paint(&mut pixels, w, h, scale as f32);
    assert!(!drawn.changed());
    let lilac = 0xb69cff;
    assert_eq!(pixel(&pixels, w, scale, 150, 20), lilac, "full fader");
    assert_eq!(run(&mut *loaded.effect), 0.5);

    // Click at half way: the level halves, and the display shows it.
    drawn.mouse(100.0, 20.0, Mouse::Down);
    drawn.mouse(100.0, 20.0, Mouse::Up);
    assert!(drawn.changed());
    drawn.paint(&mut pixels, w, h, scale as f32);
    assert_eq!(pixel(&pixels, w, scale, 150, 20), 0x161819, "half fader");
    assert_eq!(pixel(&pixels, w, scale, 90, 30), lilac);
    let level = run(&mut *loaded.effect);
    assert!((level - 0.25).abs() < 1e-6, "{level}");
}
