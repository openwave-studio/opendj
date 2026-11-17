use super::*;
use crate::set_timeline::tests::{bass_swap, two_decks};

const RATE: f64 = 8000.0;

fn sine(seconds: f64, level: f32) -> impl Iterator<Item = [f32; 2]> {
    (0..(seconds * RATE) as usize).map(move |i| {
        let x = level * (i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin();
        [x, x]
    })
}

fn kinds(f: &[Finding]) -> Vec<CheckKind> {
    f.iter().map(|f| f.kind).collect()
}

#[test]
fn finds_clipping_silence_and_a_level_jump() {
    // A at -6 dBFS for 10 s, 3 s of silence, then B at 1.0 for 12 s.
    let frames = sine(10.0, 0.5).chain(sine(3.0, 0.0)).chain(sine(12.0, 1.0));
    let stretches = [
        Stretch {
            start: 0.0,
            end: 10.0,
            title: "A".into(),
        },
        Stretch {
            start: 13.0,
            end: 25.0,
            title: "B".into(),
        },
    ];
    let f = check_audio(frames, RATE, &stretches);
    assert_eq!(
        kinds(&f),
        [
            CheckKind::Silence,
            CheckKind::Clipping,
            CheckKind::LevelJump
        ],
        "{f:?}"
    );
    let silence = &f[0];
    assert!((silence.start - 10.0).abs() < 0.11 && (silence.end - 13.0).abs() < 0.11);
    let clip = &f[1];
    assert!(
        (clip.start - 13.0).abs() < 0.11 && (clip.end - 25.0).abs() < 0.11,
        "{clip:?}"
    );
    assert_eq!(f[2].start, 13.0);
    assert!(
        f[2].text.starts_with("B is 6.0 dB louder than A"),
        "{}",
        f[2].text
    );
}

#[test]
fn silence_before_the_set_starts_is_fine() {
    let frames = sine(5.0, 0.0).chain(sine(5.0, 0.5));
    assert!(check_audio(frames, RATE, &[]).is_empty());
    // Short gaps are fine too.
    let frames = sine(5.0, 0.5).chain(sine(1.5, 0.0)).chain(sine(5.0, 0.5));
    assert!(check_audio(frames, RATE, &[]).is_empty());
}

fn grid(first_beat: f64, phrases: &[f64]) -> Option<TrackGrid> {
    Some(TrackGrid {
        first_beat,
        bpm: 120.0,
        phrases: phrases.to_vec(),
    })
}

#[test]
fn finds_beats_drifting_apart() {
    let t = bass_swap();
    assert!(check_log(&t, &[grid(0.0, &[]), grid(0.0, &[])]).is_empty());
    // B's grid 100 ms later: its beats land 100 ms off A's.
    let f = check_log(&t, &[grid(0.0, &[]), grid(0.1, &[])]);
    assert_eq!(kinds(&f), [CheckKind::Drift], "{f:?}");
    assert_eq!(f[0].start, 30.0);
    assert!(f[0].text.contains("100 ms"), "{}", f[0].text);
    // Without grids nothing is checked.
    assert!(check_log(&t, &[None, None]).is_empty());
}

#[test]
fn finds_an_effect_left_on() {
    let log = |off: f64| {
        let mut log = two_decks();
        log.c(
            0.0,
            "[EffectRack1_EffectUnit1],group_[Channel1]_enable",
            1.0,
        );
        log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
        log.playing(1, 0.0, 60.0, 0.0);
        log.c(10.0, "[EffectRack1_EffectUnit1_Effect1],enabled", 1.0);
        log.c(off, "[EffectRack1_EffectUnit1_Effect1],enabled", 0.0);
        log.end(60.0)
    };
    // A phrase change at 16 s; 16 beats later is 24 s.
    let grids = [grid(0.0, &[0.0, 16.0, 32.0])];
    let f = check_log(&log(50.0), &grids);
    assert_eq!(kinds(&f), [CheckKind::FxLeftOn], "{f:?}");
    assert_eq!((f[0].start, f[0].end), (10.0, 50.0));
    assert!(check_log(&log(20.0), &grids).is_empty());
}

#[test]
fn finds_a_jump_mid_phrase() {
    let log = |to: f64| {
        let mut log = two_decks();
        log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
        log.playing(1, 0.0, 10.5, 0.0);
        log.pos(10.5, 1, to);
        log.playing(1, 11.5, 20.0, to + 1.0);
        log.end(20.0)
    };
    let grids = [grid(0.0, &[0.0, 16.0, 32.0])];
    let f = check_log(&log(2.0), &grids);
    assert_eq!(kinds(&f), [CheckKind::MidPhraseJump], "{f:?}");
    assert_eq!(f[0].start, 10.5);
    assert!(check_log(&log(16.0), &grids).is_empty());
}

#[test]
fn stretches_run_between_transitions() {
    let t = bass_swap();
    let s = stretches(&t);
    assert_eq!(s.len(), 2, "{s:?}");
    assert_eq!(
        (s[0].start, s[0].end, s[0].title.as_str()),
        (0.0, 30.0, "A")
    );
    assert_eq!(
        (s[1].start, s[1].end, s[1].title.as_str()),
        (50.0, 80.0, "B")
    );
}
