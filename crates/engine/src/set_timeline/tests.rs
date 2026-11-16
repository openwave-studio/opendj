use serde_json::json;

use super::*;

/// Writes a control log line by line, as the recorder does.
struct Log {
    lines: Vec<String>,
    end: f64,
}

impl Log {
    /// Decks 1 and 2 with their faders as given, nothing loaded.
    fn new(snapshot: serde_json::Value) -> Log {
        let mut log = Log {
            lines: Vec::new(),
            end: 0.0,
        };
        log.push(json!({
            "opendj_recording": 1,
            "started": "2026-11-16T20:00:00+00:00",
            "sample_rate": 48000.0,
            "audio": "openDJ 2026-11-16 20.00.wav",
        }));
        log.push(json!({ "t": 0.0, "snapshot": snapshot }));
        log
    }

    fn push(&mut self, v: serde_json::Value) {
        self.lines.push(v.to_string());
    }

    fn c(&mut self, t: f64, name: &str, v: f64) -> &mut Self {
        self.push(json!({ "t": t, "c": name, "v": v }));
        self
    }

    fn load(&mut self, t: f64, deck: usize, title: &str, bpm: f64) -> &mut Self {
        self.push(json!({ "t": t, "load": {
            "deck": deck, "artist": "Artist", "title": title,
            "path": format!("/music/{title}.mp3"), "bpm": bpm, "key": "8A",
        }}));
        self
    }

    fn pos(&mut self, t: f64, deck: usize, s: f64) -> &mut Self {
        self.push(json!({ "t": t, "pos": { "deck": deck, "s": s } }));
        self
    }

    /// `pos` lines once a second for a deck playing at normal speed from
    /// `s` at `from` until `to`.
    fn playing(&mut self, deck: usize, from: f64, to: f64, s: f64) -> &mut Self {
        let mut t = from;
        while t < to {
            self.pos(t, deck, s + (t - from));
            t += 1.0;
        }
        self
    }

    fn end(&mut self, t: f64) -> SetTimeline {
        self.end = t;
        self.push(json!({ "t": t, "end": true }));
        SetTimeline::parse(&self.lines.join("\n")).unwrap()
    }
}

fn two_decks() -> Log {
    Log::new(json!({
        "[Channel1],volume": 1.0,
        "[Channel2],volume": 0.0,
        "[Channel1],play": 0.0,
        "[Channel2],play": 0.0,
        "[Master],crossfader": 0.0,
        "[EqualizerRack1_[Channel1]_Effect1],parameter1": 1.0,
        "[EqualizerRack1_[Channel2]_Effect1],parameter1": 1.0,
    }))
}

/// Deck 1 plays A from 1 s; deck 2 starts B at 20 s, faded in at 30 s
/// with its bass out; the basses swap at 40 s; deck 1 is faded out at 50 s
/// and stopped at 52 s.
fn bass_swap() -> SetTimeline {
    let mut log = two_decks();
    log.load(0.5, 1, "A", 120.0).pos(0.5, 1, 0.0);
    log.c(1.0, "[Channel1],play", 1.0);
    log.playing(1, 1.0, 52.0, 0.0);
    log.load(10.0, 2, "B", 120.0).pos(10.0, 2, 16.0);
    log.c(20.0, "[Channel2],play", 1.0);
    log.playing(2, 20.0, 80.0, 16.0);
    log.c(29.0, "[EqualizerRack1_[Channel2]_Effect1],parameter1", 0.0);
    log.c(30.0, "[Channel2],volume", 1.0);
    log.c(40.0, "[EqualizerRack1_[Channel1]_Effect1],parameter1", 0.0);
    log.c(40.0, "[EqualizerRack1_[Channel2]_Effect1],parameter1", 1.0);
    log.c(50.0, "[Channel1],volume", 0.0);
    log.c(52.0, "[Channel1],play", 0.0);
    log.end(80.0)
}

#[test]
fn reads_tracks_and_clips() {
    let t = bass_swap();
    assert_eq!(t.length, 80.0);
    assert_eq!(t.audio, "openDJ 2026-11-16 20.00.wav");
    assert_eq!(t.tracks.len(), 2);
    assert_eq!(t.tracks[1].title, "B");
    assert_eq!(t.tracks[1].path.as_deref(), Some(Path::new("/music/B.mp3")));
    assert_eq!(t.decks(), vec![0, 1]);
    assert_eq!(t.clips.len(), 2);
    let a = &t.clips[0];
    assert_eq!((a.deck, a.track, a.start, a.end), (0, 0, 1.0, 52.0));
    assert!(a.track_start.abs() < 1e-9);
    let b = &t.clips[1];
    assert_eq!((b.deck, b.track, b.start, b.end), (1, 1, 20.0, 80.0));
    assert!((b.track_start - 16.0).abs() < 1e-9);
}

#[test]
fn finds_a_bass_swap() {
    let t = bass_swap();
    assert_eq!(t.transitions.len(), 1, "{:?}", t.transitions);
    let x = &t.transitions[0];
    assert_eq!(
        (x.from_deck, x.to_deck, x.from_track, x.to_track),
        (0, 1, 0, 1)
    );
    assert_eq!((x.start, x.end), (30.0, 50.0));
    // 20 s at 120 BPM is 10 bars.
    assert!((x.bars.unwrap() - 10.0).abs() < 1e-9);
    assert_eq!(x.style, Style::BassSwap);
    // A had played 29 s; B started at 16 s and had played 10.
    assert!((x.out_point - 29.0).abs() < 1e-9, "{}", x.out_point);
    assert!((x.in_point - 26.0).abs() < 1e-9, "{}", x.in_point);
}

#[test]
fn a_quick_swap_is_a_cut() {
    let mut log = two_decks();
    log.load(0.0, 1, "A", 124.0).c(0.0, "[Channel1],play", 1.0);
    log.playing(1, 0.0, 40.0, 0.0);
    log.load(5.0, 2, "B", 124.0).c(20.0, "[Channel2],play", 1.0);
    log.playing(2, 20.0, 60.0, 0.0);
    log.c(30.0, "[Channel2],volume", 1.0);
    log.c(31.0, "[Channel1],volume", 0.0);
    let t = log.end(60.0);
    assert_eq!(t.transitions.len(), 1);
    assert_eq!(t.transitions[0].style, Style::Cut);
}

#[test]
fn the_crossfader_counts() {
    // Both faders up the whole time; the crossfader moves from deck 1's
    // side to deck 2's between 30 and 46 s.
    let mut log = Log::new(json!({
        "[Channel1],volume": 1.0,
        "[Channel2],volume": 1.0,
        "[Master],crossfader": -1.0,
    }));
    log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
    log.playing(1, 0.0, 60.0, 0.0);
    log.load(5.0, 2, "B", 120.0).c(20.0, "[Channel2],play", 1.0);
    log.playing(2, 20.0, 80.0, 0.0);
    for i in 0..=16 {
        log.c(
            30.0 + i as f64,
            "[Master],crossfader",
            -1.0 + i as f64 / 8.0,
        );
    }
    let t = log.end(80.0);
    assert_eq!(t.transitions.len(), 1, "{:?}", t.transitions);
    let x = &t.transitions[0];
    assert_eq!(x.style, Style::Smooth);
    assert!(x.start > 30.0 && x.start < 32.0, "{}", x.start);
    assert!(x.end > 44.0 && x.end <= 46.0, "{}", x.end);
}

#[test]
fn finds_a_filter_and_an_echo_out() {
    let filter = {
        let mut log = two_decks();
        log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
        log.playing(1, 0.0, 50.0, 0.0);
        log.load(5.0, 2, "B", 120.0).c(20.0, "[Channel2],play", 1.0);
        log.playing(2, 20.0, 80.0, 0.0);
        log.c(30.0, "[Channel2],volume", 1.0);
        log.c(35.0, "[QuickEffectRack1_[Channel1]],super1", 0.7);
        log.c(40.0, "[QuickEffectRack1_[Channel1]],super1", 0.95);
        log.c(46.0, "[Channel1],volume", 0.0);
        log.end(80.0)
    };
    assert_eq!(filter.transitions[0].style, Style::Filter);

    let echo = {
        let mut log = two_decks();
        log.c(
            0.0,
            "[EffectRack1_EffectUnit1],group_[Channel1]_enable",
            1.0,
        );
        log.c(0.0, "[EffectRack1_EffectUnit1_Effect1],loaded_effect", 1.0);
        log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
        log.playing(1, 0.0, 50.0, 0.0);
        log.load(5.0, 2, "B", 120.0).c(20.0, "[Channel2],play", 1.0);
        log.playing(2, 20.0, 80.0, 0.0);
        log.c(30.0, "[Channel2],volume", 1.0);
        log.c(44.0, "[EffectRack1_EffectUnit1_Effect1],enabled", 1.0);
        log.c(46.0, "[Channel1],volume", 0.0);
        log.c(50.0, "[EffectRack1_EffectUnit1_Effect1],enabled", 0.0);
        log.end(80.0)
    };
    assert_eq!(echo.transitions[0].style, Style::EchoOut);
    assert!(echo
        .lanes
        .iter()
        .any(|l| l.kind == LaneKind::FxEffect(0, 0) && l.points == [(44.0, 1.0), (50.0, 0.0)]));
}

#[test]
fn a_hot_cue_splits_the_clip() {
    let mut log = two_decks();
    log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
    log.playing(1, 0.0, 10.5, 0.0);
    // Jump back to 2 s into the track at 10.5 s.
    log.pos(10.5, 1, 2.0);
    log.playing(1, 11.5, 20.0, 3.0);
    let t = log.end(20.0);
    assert_eq!(t.clips.len(), 2, "{:?}", t.clips);
    assert_eq!((t.clips[0].start, t.clips[0].end), (0.0, 10.5));
    assert_eq!((t.clips[1].start, t.clips[1].track_start), (10.5, 2.0));
    assert!(t.hard_moves.is_empty());
}

#[test]
fn loops_count_their_repeats() {
    let mut log = two_decks();
    log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
    log.playing(1, 0.0, 10.0, 0.0);
    // A 2 s loop from 10 to 12 s in the track, round three times.
    log.c(10.0, "[Channel1],loop_start_position", 10.0);
    log.c(10.0, "[Channel1],loop_end_position", 12.0);
    log.c(10.0, "[Channel1],loop_enabled", 1.0);
    log.pos(10.0, 1, 10.0);
    for i in 1..=3 {
        let t = 10.0 + 2.0 * i as f64;
        log.pos(t - 1.0, 1, 11.0);
        log.pos(t, 1, 10.0);
    }
    log.c(17.0, "[Channel1],loop_enabled", 0.0);
    log.playing(1, 17.0, 25.0, 11.0);
    let t = log.end(25.0);
    assert_eq!(t.loops.len(), 1);
    let l = &t.loops[0];
    assert_eq!((l.deck, l.track, l.start, l.end), (0, 0, 10.0, 17.0));
    assert_eq!((l.loop_start, l.loop_end, l.repeats), (10.0, 12.0, 3));
    assert_eq!(t.clips.len(), 1, "a loop doesn't split the clip");
}

#[test]
fn a_scratch_is_one_hard_move_inside_its_clip() {
    let mut log = two_decks();
    log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
    log.playing(1, 0.0, 10.0, 0.0);
    log.c(10.0, "[Channel1],scratch2_enable", 1.0);
    // Back and forth around 10 s in the track.
    for (i, s) in [9.5, 10.2, 9.4, 10.3, 9.6].iter().enumerate() {
        log.pos(10.2 + 0.3 * i as f64, 1, *s);
    }
    log.c(11.5, "[Channel1],scratch2_enable", 0.0);
    log.pos(11.5, 1, 10.0);
    log.playing(1, 12.5, 20.0, 11.0);
    let t = log.end(20.0);
    assert_eq!(t.hard_moves.len(), 1, "{:?}", t.hard_moves);
    let m = &t.hard_moves[0];
    assert_eq!((m.deck, m.track), (0, Some(0)));
    assert!(m.start <= 10.2 && m.end >= 11.4, "{m:?}");
    assert_eq!(t.clips.len(), 1, "{:?}", t.clips);
    assert_eq!((t.clips[0].start, t.clips[0].end), (0.0, 20.0));
}

#[test]
fn tempo_changes_move_the_place() {
    let mut log = Log::new(json!({ "[Channel1],rateRange": 0.08 }));
    log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
    log.playing(1, 0.0, 10.0, 0.0);
    // +4% from 10 s.
    log.c(10.0, "[Channel1],rate", 0.5);
    for i in 0..10 {
        log.pos(10.0 + i as f64, 1, 10.0 + i as f64 * 1.04);
    }
    let t = log.end(20.0);
    assert_eq!(t.clips.len(), 1, "{:?}", t.clips);
}

#[test]
fn reads_a_log_cut_short() {
    let mut log = two_decks();
    log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
    log.playing(1, 0.0, 10.0, 0.0);
    log.lines.push("{\"t\": 10.0, \"c\": \"[Chan".into());
    let t = SetTimeline::parse(&log.lines.join("\n")).unwrap();
    assert_eq!(t.clips.len(), 1);
    assert_eq!(t.clips[0].end, 9.0);
    assert!(SetTimeline::parse("").is_err());
    assert!(SetTimeline::parse("{\"t\": 0}").is_err());
}

#[test]
fn value_at_reads_the_log() {
    let t = bass_swap();
    assert_eq!(t.value_at("[Channel2],volume", 29.0), Some(0.0));
    assert_eq!(t.value_at("[Channel2],volume", 30.0), Some(1.0));
    assert_eq!(t.value_at("[Nope],x", 30.0), None);
    let volume = t
        .lanes
        .iter()
        .find(|l| l.kind == LaneKind::Volume && l.deck == Some(1))
        .unwrap();
    assert_eq!(volume.points, vec![(0.0, 0.0), (30.0, 1.0)]);
}

#[test]
fn finds_the_log_and_cue_beside_the_audio() {
    let (log, cue) = log_paths(Path::new("/rec/openDJ 2026-11-16 20.00 (part 2).wav"));
    assert_eq!(
        log,
        Path::new("/rec/openDJ 2026-11-16 20.00.controls.jsonl")
    );
    assert_eq!(cue, Path::new("/rec/openDJ 2026-11-16 20.00.cue"));
    let (log, _) = log_paths(Path::new("/rec/openDJ 2026-11-16 20.00.flac"));
    assert_eq!(
        log,
        Path::new("/rec/openDJ 2026-11-16 20.00.controls.jsonl")
    );
}

const CUE: &str = r#"REM DATE 2026-11-16 20:00
PERFORMER "openDJ"
TITLE "openDJ 2026-11-16 20.00"
FILE "openDJ 2026-11-16 20.00.wav" WAVE
  TRACK 01 AUDIO
    TITLE "First"
    PERFORMER "Someone"
    INDEX 01 00:01:00
  TRACK 02 AUDIO
    TITLE "Second"
    PERFORMER "Else"
    INDEX 01 03:30:38
"#;

#[test]
fn reads_a_cue_sheet() {
    let tracks = read_cue(CUE);
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "First");
    assert_eq!(tracks[0].performer, "Someone");
    assert_eq!(tracks[0].start, 1.0);
    assert!((tracks[1].start - (210.0 + 38.0 / 75.0)).abs() < 1e-9);
}

#[test]
fn opens_with_the_cue_sheet_when_there_is_no_log() {
    let dir = std::env::temp_dir().join(format!("opendj-set-timeline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let audio = dir.join("openDJ 2026-11-16 20.00.wav");
    std::fs::write(dir.join("openDJ 2026-11-16 20.00.cue"), CUE).unwrap();
    match Recording::open(&audio) {
        Recording::CueOnly(tracks) => assert_eq!(tracks.len(), 2),
        other => panic!("{other:?}"),
    }
    let mut log = two_decks();
    log.load(0.0, 1, "A", 120.0).c(0.0, "[Channel1],play", 1.0);
    log.end(5.0);
    std::fs::write(
        dir.join("openDJ 2026-11-16 20.00.controls.jsonl"),
        log.lines.join("\n"),
    )
    .unwrap();
    match Recording::open(&audio) {
        Recording::Logged(t) => assert_eq!(t.tracks.len(), 1),
        other => panic!("{other:?}"),
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
