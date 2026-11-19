use super::*;
use crate::beats::BeatGrid;
use crate::track::Audio;
use serde_json::json;

const SR: u32 = 22_050;
const SET: f64 = 40.0;

/// A steady tone, 120 BPM on the grid.
fn tone(hz: f64, seconds: f64) -> Arc<Track> {
    let n = (seconds * SR as f64) as usize;
    let frames = (0..n)
        .map(|i| {
            let v = (std::f64::consts::TAU * hz * i as f64 / SR as f64).sin() as f32 * 0.4;
            [v, v]
        })
        .collect();
    let mut t = (*crate::synth::sample_library()[0]).clone();
    t.duration = seconds;
    t.audio = Some(Arc::new(Audio::from_frames(SR, frames)));
    t.set_grid(BeatGrid::constant(120.0, 0.0));
    Arc::new(t)
}

/// What the DJ did: (seconds, control, value).
fn script() -> Vec<(f64, &'static str, f64)> {
    let mut s = vec![(0.0, "[Channel1],play", 1.0), (8.0, "[Channel2],play", 1.0)];
    for k in 0..=40 {
        let f = k as f64 / 40.0;
        s.push((8.0 + 4.0 * f, "[Channel2],volume", f));
        s.push((12.0 + 4.0 * f, "[Channel1],volume", 1.0 - f));
    }
    s.push((17.0, "[Channel1],play", 0.0));
    // A dip later on, well after the transition.
    s.push((30.0, "[Channel2],volume", 0.2));
    s.push((31.0, "[Channel2],volume", 1.0));
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    s
}

/// Plays the script through the engine as a live set would be recorded,
/// and writes its control log.
fn record(a: &Arc<Track>, b: &Arc<Track>) -> (Vec<[f32; 2]>, SetTimeline) {
    let reg = ControlRegistry::new();
    let (mut engine, mut input) = Engine::new(&reg, SR as f64);
    reg.get("[Master]", "gain").unwrap().set(1.0);
    input
        .send(Command::Load {
            deck: 0,
            track: a.clone(),
        })
        .ok()
        .unwrap();
    input
        .send(Command::Load {
            deck: 1,
            track: b.clone(),
        })
        .ok()
        .unwrap();
    engine.process(&mut [0.0; 2], 2);
    reg.get("[Channel1]", "volume").unwrap().set(1.0);
    reg.get("[Channel2]", "volume").unwrap().set(0.0);

    let mut log = vec![
        json!({"opendj_recording": 1, "started": "2026-11-19T20:00:00-08:00", "sample_rate": SR, "audio": "set.wav"}),
        json!({"t": 0.0, "snapshot": {
            "[Channel1],volume": 1.0, "[Channel2],volume": 0.0,
            "[Channel1],play": 0.0, "[Channel2],play": 0.0,
            "[Master],gain": 1.0, "[Master],crossfader": 0.0,
        }}),
        json!({"t": 0.0, "load": {"deck": 1, "title": "A", "artist": "", "path": "a.wav", "bpm": 120.0, "key": ""}}),
        json!({"t": 0.0, "load": {"deck": 2, "title": "B", "artist": "", "path": "b.wav", "bpm": 120.0, "key": ""}}),
    ];
    let script = script();
    let mut out = Vec::new();
    let mut buf = vec![0.0f32; BLOCK * 2];
    let frames = (SET * SR as f64) as usize;
    let mut i = 0;
    let mut frame = 0;
    while frame < frames {
        while i < script.len() && (script[i].0 * SR as f64).round() as usize <= frame {
            let (at, name, v) = script[i];
            let (g, item) = split(name).unwrap();
            reg.get(g, item).unwrap().set(v);
            log.push(json!({"t": at, "c": name, "v": v}));
            if item == "play" && v > 0.5 {
                let deck = if g == "[Channel1]" { 1 } else { 2 };
                let s = reg.get(g, "playposition").unwrap().get()
                    * if deck == 1 { a.duration } else { b.duration };
                log.push(json!({"t": at, "pos": {"deck": deck, "s": s}}));
            }
            i += 1;
        }
        let next = script.get(i).map_or(usize::MAX, |x| {
            ((x.0 * SR as f64).round() as usize).max(frame + 1)
        });
        let n = BLOCK.min(frames - frame).min(next - frame);
        engine.process(&mut buf[..n * 2], 2);
        out.extend_from_slice(buf[..n * 2].as_chunks::<2>().0);
        frame += n;
    }
    log.push(json!({"t": SET, "end": true}));
    let text: String = log.iter().map(|l| format!("{l}\n")).collect();
    (out, SetTimeline::parse(&text).unwrap())
}

/// How strongly `hz` sounds in `frames` (Goertzel, left channel).
fn level(frames: &[[f32; 2]], hz: f64) -> f64 {
    let w = std::f64::consts::TAU * hz / SR as f64;
    let (mut s1, mut s2) = (0.0, 0.0);
    for f in frames {
        let s = f[0] as f64 + 2.0 * w.cos() * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    (s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2).sqrt() / frames.len() as f64
}

fn at(frames: &[[f32; 2]], from: f64, seconds: f64) -> &[[f32; 2]] {
    let i = (from * SR as f64) as usize;
    &frames[i..i + (seconds * SR as f64) as usize]
}

fn tracks(a: Arc<Track>, b: Arc<Track>) -> impl FnMut(&LoadedTrack) -> Result<Arc<Track>, String> {
    move |t| Ok(if t.title == "A" { a.clone() } else { b.clone() })
}

#[test]
fn an_unedited_stretch_renders_as_recorded() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let (recording, timeline) = record(&a, &b);
    assert_eq!(timeline.transitions.len(), 1, "{:?}", timeline.transitions);
    let window = Window {
        from: 6.0,
        to: 20.0,
        ..Default::default()
    };
    let render = render(&timeline, &window, SR, &mut tracks(a, b)).unwrap();
    assert_eq!(render.len(), (14.0 * SR as f64).ceil() as usize);
    let rec = at(&recording, 6.0, 14.0);
    let (mut diff, mut power) = (0.0f64, 0.0f64);
    for (r, o) in render.iter().zip(rec) {
        diff += ((r[0] - o[0]) as f64).powi(2);
        power += (o[0] as f64).powi(2);
    }
    // The same engine, the same moves on the same frames: the same audio.
    let db = 10.0 * (diff / power).log10();
    assert!(db < -60.0, "residual {db:.1} dB");
}

#[test]
fn a_longer_transition_keeps_the_outgoing_track_playing() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let (mut recording, timeline) = record(&a, &b);
    let x = timeline.transitions[0].clone();
    let edit = TransitionEdit {
        transition: 0,
        bars: 8.0,
        style: Style::Smooth,
    };
    let window = transition_window(&timeline, &edit).unwrap();
    let e = x.start + 16.0;
    assert!(window.from < x.start && window.to > e, "{window:?}");
    let render = render(&timeline, &window, SR, &mut tracks(a, b)).unwrap();
    splice(&mut recording, &render, window.from, SR);

    // Recorded, the outgoing track stopped at 17 s; now it fades out to e.
    let mid = at(&recording, x.start + 11.0, 0.5);
    assert!(level(mid, 220.0) > 0.03, "{}", level(mid, 220.0));
    assert!(level(mid, 330.0) > 0.1);
    let after = at(&recording, e + 1.0, 0.5);
    assert!(level(after, 220.0) < 0.002, "{}", level(after, 220.0));
    // Before the incoming track comes in, as recorded.
    let before = at(&recording, window.from + 0.5, 0.5);
    assert!(level(before, 220.0) > 0.15 && level(before, 330.0) < 0.002);
}

#[test]
fn a_bass_swap_brings_the_bass_in_on_the_middle_bar() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let (_, timeline) = record(&a, &b);
    let x = timeline.transitions[0].clone();
    let edit = TransitionEdit {
        transition: 0,
        bars: 4.0,
        style: Style::BassSwap,
    };
    let window = transition_window(&timeline, &edit).unwrap();
    let low_in = "[EqualizerRack1_[Channel2]_Effect1],parameter1";
    let swaps: Vec<_> = window.changes.iter().filter(|c| c.1 == low_in).collect();
    assert_eq!(swaps.first().map(|c| (c.0, c.2)), Some((x.start, 0.0)));
    assert!(swaps
        .iter()
        .any(|c| (c.0 - (x.start + 4.0)).abs() < 1e-9 && c.2 > 0.5));
    let render = render(&timeline, &window, SR, &mut tracks(a, b)).unwrap();
    assert_eq!(
        render.len(),
        ((window.to - window.from) * SR as f64).round() as usize
    );
}

#[test]
fn too_long_a_transition_is_refused() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let (_, timeline) = record(&a, &b);
    let edit = TransitionEdit {
        transition: 0,
        bars: 32.0,
        style: Style::BassSwap,
    };
    let err = transition_window(&timeline, &edit).unwrap_err();
    assert!(err.contains("at most"), "{err}");
}

#[test]
fn splices_with_a_crossfade() {
    let mut rec = vec![[1.0f32; 2]; 1000];
    let render = vec![[0.0f32; 2]; 400];
    splice(&mut rec, &render, 300.0 / 100.0, 100);
    assert_eq!(rec[299], [1.0; 2]);
    assert!(rec[300][0] > 0.9, "the seam fades: {}", rec[300][0]);
    assert_eq!(rec[500], [0.0; 2]);
    assert_eq!(rec[700], [1.0; 2]);
}

/// A tone on a grid at `bpm`.
fn tone_at(hz: f64, seconds: f64, bpm: f64) -> Arc<Track> {
    let mut t = (*tone(hz, seconds)).clone();
    t.set_grid(BeatGrid::constant(bpm, 0.0));
    Arc::new(t)
}

fn swap_b_for(title: &str, bpm: f64) -> SetEdits {
    SetEdits {
        swaps: vec![Swap {
            slot: 1,
            track: LoadedTrack {
                title: title.into(),
                path: Some("c.wav".into()),
                bpm,
                ..Default::default()
            },
        }],
        ..Default::default()
    }
}

#[test]
fn a_swapped_track_plays_in_its_place_at_the_sets_tempo() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let c = tone_at(400.0, 45.0, 150.0);
    let (mut recording, timeline) = record(&a, &b);
    assert_eq!(
        slot_track(&timeline, 1),
        Some(timeline.transitions[0].to_track)
    );
    let edits = swap_b_for("C", 150.0);
    let mut tracks = |t: &LoadedTrack| -> Result<Arc<Track>, String> {
        Ok(match t.title.as_str() {
            "A" => a.clone(),
            "B" => b.clone(),
            _ => c.clone(),
        })
    };
    let (window, render) =
        render_target(&timeline, &edits, Target::Swap(1), SR, &mut tracks).unwrap();
    assert!(window.from < timeline.transitions[0].start, "{window:?}");
    assert!((window.to - timeline.length).abs() < 1e-6, "{window:?}");
    splice(&mut recording, &render, window.from, SR);

    // Slowed from 150 to the set's 120 BPM: 400 Hz sounds at 320 Hz.
    let later = at(&recording, 25.0, 0.5);
    assert!(level(later, 320.0) > 0.1, "{}", level(later, 320.0));
    assert!(level(later, 330.0) < 0.01, "{}", level(later, 330.0));
    // The outgoing track and the recorded fade are as they were.
    let before = at(&recording, 4.0, 0.5);
    assert!(level(before, 220.0) > 0.15 && level(before, 320.0) < 0.002);
    let during = at(&recording, 14.0, 0.5);
    assert!(level(during, 220.0) > 0.03 && level(during, 320.0) > 0.1);
}

#[test]
fn a_replacement_too_short_for_the_stretch_is_refused() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let c = tone_at(400.0, 10.0, 120.0);
    let (_, timeline) = record(&a, &b);
    let mut tracks = |t: &LoadedTrack| -> Result<Arc<Track>, String> {
        Ok(match t.title.as_str() {
            "A" => a.clone(),
            "B" => b.clone(),
            _ => c.clone(),
        })
    };
    let err = check_edit(
        &timeline,
        &swap_b_for("C", 120.0),
        Target::Swap(1),
        &mut tracks,
    )
    .unwrap_err();
    assert!(err.contains("0:10 long"), "{err}");
}

#[test]
fn a_transition_next_to_a_swap_is_mixed_with_the_replacement() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let c = tone_at(400.0, 45.0, 120.0);
    let (_, timeline) = record(&a, &b);
    let mut edits = swap_b_for("C", 120.0);
    edits.transitions.push(TransitionEdit {
        transition: 0,
        bars: 4.0,
        style: Style::Smooth,
    });
    let mut tracks = |t: &LoadedTrack| -> Result<Arc<Track>, String> {
        Ok(match t.title.as_str() {
            "A" => a.clone(),
            "B" => b.clone(),
            _ => c.clone(),
        })
    };
    let (window, render) =
        render_target(&timeline, &edits, Target::Transition(0), SR, &mut tracks).unwrap();
    let after = at(&render, window.to - window.from - 1.0, 0.5);
    assert!(level(after, 400.0) > 0.1, "{}", level(after, 400.0));
    assert!(level(after, 330.0) < 0.01);
}

#[test]
fn moves_after_an_edited_transition_play_as_recorded() {
    let (a, b) = (tone(220.0, 45.0), tone(330.0, 45.0));
    let c = tone_at(400.0, 45.0, 120.0);
    let (_, timeline) = record(&a, &b);
    let mut edits = swap_b_for("C", 120.0);
    edits.transitions.push(TransitionEdit {
        transition: 0,
        bars: 2.0,
        style: Style::Smooth,
    });
    let mut tracks = |t: &LoadedTrack| -> Result<Arc<Track>, String> {
        Ok(match t.title.as_str() {
            "A" => a.clone(),
            "B" => b.clone(),
            _ => c.clone(),
        })
    };
    let (window, render) =
        render_target(&timeline, &edits, Target::Swap(1), SR, &mut tracks).unwrap();
    let at_set = |t: f64| at(&render, t - window.from, 0.5);
    // Shorter than recorded: the outgoing track is gone at 13 s, though
    // the recorded fade ran to 16 s.
    assert!(
        level(at_set(13.5), 220.0) < 0.002,
        "{}",
        level(at_set(13.5), 220.0)
    );
    // The dip at 30 s is the log's, kept.
    let full = level(at_set(25.0), 400.0);
    let dipped = level(at_set(30.2), 400.0);
    assert!(dipped < full * 0.3, "{dipped} vs {full}");
}
