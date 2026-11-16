use super::*;

const RATE: f64 = 1000.0;

/// `seconds` of a constant level.
fn level(seconds: f64, v: f32) -> Vec<[f32; 2]> {
    vec![[v, v]; (seconds * RATE) as usize]
}

fn run(frames: &[[f32; 2]], edits: &Edits) -> (Vec<f32>, Rendered) {
    let mut out = Vec::new();
    let r = render_frames(|| frames.iter().copied(), RATE, edits, |f| out.push(f[0]));
    (out, r)
}

fn at(out: &[f32], t: f64) -> f32 {
    out[(t * RATE) as usize]
}

#[test]
fn trims_with_fades() {
    let edits = Edits {
        fixes: vec![Fix::Trim {
            start: 1.0,
            end: 9.0,
            fade_in: 1.0,
            fade_out: 2.0,
        }],
    };
    let (out, r) = run(&level(10.0, 0.5), &edits);
    assert_eq!(out.len(), 8000);
    assert_eq!(r.seconds, 8.0);
    assert!(out[0].abs() < 1e-3);
    assert!((at(&out, 0.5) - 0.25).abs() < 0.01, "{}", at(&out, 0.5));
    assert_eq!(at(&out, 4.0), 0.5);
    assert!((at(&out, 7.0) - 0.25).abs() < 0.01);
    assert!(out[7999].abs() < 1e-2);
}

#[test]
fn cuts_with_a_crossfade() {
    // 0.1 for 3 s, then 0.9 for 2 s (cut), then 0.3.
    let mut frames = level(3.0, 0.1);
    frames.extend(level(2.0, 0.9));
    frames.extend(level(5.0, 0.3));
    let edits = Edits {
        fixes: vec![Fix::Cut {
            start: 3.0,
            end: 5.0,
        }],
    };
    let (out, _) = run(&frames, &edits);
    // 2 s and the 20 ms crossfade shorter.
    assert_eq!(out.len(), 10_000 - 2000 - 20);
    assert!(out.iter().all(|v| *v < 0.5), "nothing of the cut stretch");
    assert_eq!(at(&out, 1.0), 0.1);
    assert_eq!(at(&out, 6.0), 0.3);
    // Through the join from 0.1 to 0.3.
    let mid = out[2980 + 10];
    assert!(mid > 0.1 && mid < 0.3 * 1.5, "{mid}");
    assert_eq!(edits.map_time(1.0), Some(1.0));
    assert_eq!(edits.map_time(4.0), None);
    assert!((edits.map_time(6.0).unwrap() - 3.98).abs() < 1e-9);
}

#[test]
fn track_level_eases_in_and_out() {
    let edits = Edits {
        fixes: vec![Fix::TrackLevel {
            start: 2.0,
            end: 4.0,
            db: 6.0,
            ease: 0.5,
            title: "B".into(),
        }],
    };
    let (out, _) = run(&level(6.0, 0.25), &edits);
    assert_eq!(out.len(), 6000);
    assert_eq!(at(&out, 1.0), 0.25);
    assert!((at(&out, 3.0) - 0.4988).abs() < 1e-3, "{}", at(&out, 3.0));
    // Halfway up at the edge given.
    let edge = at(&out, 2.0);
    assert!(edge > 0.3 && edge < 0.4, "{edge}");
    assert_eq!(at(&out, 5.0), 0.25);
}

fn sine(seconds: f64, v: f32, rate: f64) -> Vec<[f32; 2]> {
    (0..(seconds * rate) as usize)
        .map(|i| {
            let x = v * (i as f32 * 997.0 * std::f32::consts::TAU / rate as f32).sin();
            [x, x]
        })
        .collect()
}

#[test]
fn loudness_reaches_the_preset_under_the_ceiling() {
    let rate = 48_000.0;
    let frames = sine(6.0, 0.05, rate);
    let edits = Edits {
        fixes: vec![Fix::Loudness {
            lufs: -14.0,
            ceiling: -1.0,
        }],
    };
    let mut out = Vec::new();
    let r = render_frames(|| frames.iter().copied(), rate, &edits, |f| out.push(f));
    assert_eq!(out.len(), frames.len());
    let before = r.before_lufs.unwrap();
    assert!(before < -25.0, "{before}");
    assert!((r.gain_db - (-14.0 - before)).abs() < 1e-9);
    assert!((r.after_lufs.unwrap() + 14.0).abs() < 0.3, "{r:?}");
    let ceiling = 10f32.powf(-1.0 / 20.0);
    assert!(out.iter().all(|f| f[0].abs() <= ceiling + 1e-6));
    assert!(!r.squashed());

    // A quiet mix with loud peaks needs heavy limiting to get there.
    let mut peaky = frames.clone();
    for f in peaky.iter_mut().step_by(24_000) {
        *f = [0.95, 0.95];
    }
    let r = measure(peaky.iter().copied(), rate, &edits);
    assert!(r.squashed(), "{r:?}");
}

#[test]
fn edits_round_trip_as_json() {
    let edits = Edits {
        fixes: vec![
            Fix::Trim {
                start: 12.5,
                end: 3600.0,
                fade_in: 2.0,
                fade_out: 5.0,
            },
            Fix::TrackLevel {
                start: 300.0,
                end: 600.0,
                db: -2.5,
                ease: 0.48,
                title: "Track \"B\"".into(),
            },
            Fix::Cut {
                start: 1000.0,
                end: 1016.0,
            },
            Fix::Loudness {
                lufs: -14.0,
                ceiling: -1.0,
            },
        ],
    };
    assert_eq!(Edits::from_json(&edits.to_json()).unwrap(), edits);
    // A fix from a later version is skipped, not an error.
    let later = r#"{"opendj_edits": 1, "fixes": [{"fix": "stretch"}, {"fix": "cut", "start": 1, "end": 2}]}"#;
    assert_eq!(Edits::from_json(later).unwrap().fixes.len(), 1);
}

#[test]
fn files_sit_beside_the_recording() {
    let audio = Path::new("/rec/openDJ 2026-11-16 20.00 (part 2).wav");
    assert_eq!(
        edits_path(audio),
        Path::new("/rec/openDJ 2026-11-16 20.00.edits.json")
    );
    assert_eq!(
        edited_path(
            Path::new("/rec/openDJ 2026-11-16 20.00.flac"),
            RecordFormat::Flac
        ),
        Path::new("/rec/openDJ 2026-11-16 20.00 (edited).flac")
    );
}

#[test]
fn renders_a_file() {
    let dir = std::env::temp_dir().join(format!("opendj-set-fixes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let audio = dir.join("openDJ 2026-11-16 20.00.wav");
    let samples: Vec<i16> = (0..48_000 * 4)
        .map(|i| ((i as f32 / 20.0).sin() * 8000.0) as i16)
        .collect();
    crate::decode::tests::write_wav(&audio, 48_000, 1, &samples);
    let edits = Edits {
        fixes: vec![
            Fix::Cut {
                start: 1.0,
                end: 2.0,
            },
            Fix::Loudness {
                lufs: -14.0,
                ceiling: -1.0,
            },
        ],
    };
    edits.save(&audio).unwrap();
    assert_eq!(Edits::load(&audio), edits);
    let dest = edited_path(&audio, RecordFormat::Wav);
    let r = render(&audio, &dest, RecordFormat::Wav, 24, &edits).unwrap();
    let out = crate::decode::decode(&dest).unwrap();
    assert_eq!(out.sample_rate, 48_000);
    let expected = 48_000 * 3 - 960;
    assert_eq!(out.len(), expected);
    assert!((r.seconds - expected as f64 / 48_000.0).abs() < 1e-9);
    assert!((r.after_lufs.unwrap() + 14.0).abs() < 0.5, "{r:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}
