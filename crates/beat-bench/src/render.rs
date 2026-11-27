//! Test tracks rendered here, so every beat and bar 1 is known to the sample.
//!
//! Each track is drawn with the Groove Box's kits (see
//! [`opendj_engine::kits`]) plus a simple bass and pad, and aimed at
//! something that trips up beat grids in dance music: a long beatless
//! intro, a breakdown with no kick, swing, half time, a tempo that drifts
//! or changes, a fill before bar 1, a kick that skips beats, triplet
//! percussion, a rolling bass between the kicks. The reference beats are
//! the ones the track was rendered on, bars four beats long from bar 1.

use std::f32::consts::TAU;

use opendj_engine::kits;
use opendj_engine::track::Audio;

pub const RATE: u32 = 44_100;

/// A rendered test track and the beats it was rendered on.
pub struct Rendered {
    pub name: &'static str,
    pub about: &'static str,
    pub audio: Audio,
    pub beats: Vec<f64>,
    pub downbeats: Vec<f64>,
}

// Layers a section plays.
const KICK: u8 = 1;
const SNARE: u8 = 2;
const HATS: u8 = 4;
const PERC: u8 = 8;
const BASS: u8 = 16;
const PAD: u8 = 32;
const DRUMS: u8 = KICK | SNARE | HATS | PERC;
const ALL: u8 = DRUMS | BASS | PAD;

// Kit rows (see `kits::ROLES`), and the Dub kit's chord stab.
const R_KICK: usize = 0;
const R_SNARE: usize = 1;
const R_CLOSED: usize = 2;
const R_OPEN: usize = 3;
const R_CLAP: usize = 4;
const R_RIM: usize = 5;
const R_CRASH: usize = 7;
const R_RIDE: usize = 8;
const R_CHORD: usize = 9;

// Kits, in `kits::KITS` order.
const ANALOG: usize = 0;
const SYNTHETIC: usize = 1;
const BOOM: usize = 2;
const PUNCH: usize = 3;
const TIN: usize = 4;
const DUB: usize = 5;

#[derive(Clone, Copy)]
enum Tempo {
    Steady(f64),
    /// From the first BPM to the second, evenly across the track.
    Drift(f64, f64),
    /// The first BPM until the bar given, then the second.
    Change(f64, f64, usize),
}

#[derive(Clone, Copy)]
enum Bass {
    /// On the off-beat 8ths.
    Offbeat,
    /// The three 16ths after each beat (rolling techno, psytrance).
    Rolling,
    /// With the kick.
    OnKick,
    /// One long note a bar.
    Whole,
}

struct Scenario {
    name: &'static str,
    about: &'static str,
    kit: usize,
    tempo: Tempo,
    /// Steps per beat: 4 (16ths) or 3 (triplets).
    steps: usize,
    /// Where the second 16th of each 8th falls, as a share of the 8th:
    /// 0.5 is straight.
    swing: f64,
    kick: &'static str,
    snare: &'static str,
    snare_row: usize,
    hats: &'static str,
    hats_row: usize,
    open: &'static str,
    perc: &'static str,
    perc_row: usize,
    /// The percussion echoes (a dub delay, 3/16ths).
    echo: bool,
    bass: Bass,
    /// Beats of snare fill before bar 1.
    pickup: usize,
    sections: &'static [(usize, u8)],
}

const BASE: Scenario = Scenario {
    name: "",
    about: "",
    kit: PUNCH,
    tempo: Tempo::Steady(124.0),
    steps: 4,
    swing: 0.5,
    kick: "x...x...x...x...",
    snare: "....x.......x...",
    snare_row: R_CLAP,
    hats: "x.x.x.x.x.x.x.x.",
    hats_row: R_CLOSED,
    open: "..x...x...x...x.",
    perc: "",
    perc_row: R_RIM,
    echo: false,
    bass: Bass::Offbeat,
    pickup: 0,
    sections: &[
        (8, DRUMS),
        (16, ALL),
        (8, PAD | BASS | HATS),
        (16, ALL),
        (8, DRUMS),
    ],
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "house-124",
        about: "Four to the floor, clap on 2 and 4: the easy case",
        ..BASE
    },
    Scenario {
        name: "beatless-intro-128",
        about: "32 bars of pad before the first kick",
        tempo: Tempo::Steady(128.0),
        sections: &[(32, PAD), (24, ALL), (8, DRUMS)],
        ..BASE
    },
    Scenario {
        name: "breakdown-126",
        about: "A 16-bar breakdown with no drums, only bass notes and pad",
        tempo: Tempo::Steady(126.0),
        bass: Bass::Whole,
        sections: &[(16, ALL), (16, PAD | BASS), (16, ALL)],
        ..BASE
    },
    Scenario {
        name: "swing-house-122",
        about: "Swung 16ths (62 %) on the hats and percussion",
        tempo: Tempo::Steady(122.0),
        kit: ANALOG,
        swing: 0.62,
        hats: "xxxxxxxxxxxxxxxx",
        perc: ".x...x.x.x...x..",
        ..BASE
    },
    Scenario {
        name: "garage-2step-132",
        about: "2-step: the kick skips beats 2 and 4, swung hats",
        tempo: Tempo::Steady(132.0),
        kit: SYNTHETIC,
        swing: 0.6,
        kick: "x.........x.....",
        snare: "....x.......x...",
        snare_row: R_SNARE,
        hats: "x.xxx.x.x.xxx.x.",
        open: "",
        bass: Bass::OnKick,
        ..BASE
    },
    Scenario {
        name: "halftime-140",
        about: "Half-time feel at 140: kick on 1, snare on 3",
        tempo: Tempo::Steady(140.0),
        kit: BOOM,
        kick: "x.....x...x.....",
        snare: "........x.......",
        snare_row: R_SNARE,
        hats: "x.x.x.x.x.x.x.x.",
        open: "",
        bass: Bass::OnKick,
        ..BASE
    },
    Scenario {
        name: "dnb-174",
        about: "Drum and bass breakbeat at 174",
        tempo: Tempo::Steady(174.0),
        kit: SYNTHETIC,
        kick: "x.........x.....",
        snare: "....x.......x...",
        snare_row: R_SNARE,
        hats: "x.x.x.x.x.x.x.x.",
        open: "",
        bass: Bass::Whole,
        sections: &[(16, DRUMS | PAD), (32, ALL), (16, DRUMS)],
        ..BASE
    },
    Scenario {
        name: "downtempo-90",
        about: "Hip-hop beat at 90, swung hats",
        tempo: Tempo::Steady(90.0),
        kit: BOOM,
        swing: 0.58,
        kick: "x......x..x.....",
        snare: "....x.......x...",
        snare_row: R_SNARE,
        hats: "x.x.x.x.x.x.x.x.",
        open: "",
        bass: Bass::OnKick,
        sections: &[(4, PAD | HATS), (24, ALL), (4, DRUMS)],
        ..BASE
    },
    Scenario {
        name: "afro-12-8-118",
        about: "Four to the floor under triplet shakers and rims",
        tempo: Tempo::Steady(118.0),
        kit: TIN,
        steps: 3,
        kick: "x..x..x..x..",
        snare: "...x.....x..",
        hats: "xxxxxxxxxxxx",
        hats_row: 9,
        open: "",
        perc: ".x..xx.x..x.",
        ..BASE
    },
    Scenario {
        name: "rolling-techno-132",
        about: "Rolling bass on the 16ths between the kicks, ride on the off-beat",
        tempo: Tempo::Steady(132.0),
        kit: PUNCH,
        snare: "",
        open: "..x...x...x...x.",
        hats_row: R_CLOSED,
        perc: "..x...x...x...x.",
        perc_row: R_RIDE,
        bass: Bass::Rolling,
        ..BASE
    },
    Scenario {
        name: "psytrance-145",
        about: "Rolling bass between the kicks at 145",
        tempo: Tempo::Steady(145.0),
        kit: SYNTHETIC,
        snare: "",
        open: "..x...x...x...x.",
        bass: Bass::Rolling,
        ..BASE
    },
    Scenario {
        name: "dub-techno-118",
        about: "Soft kick, off-beat chord stabs with a dub echo",
        tempo: Tempo::Steady(118.0),
        kit: DUB,
        snare: "",
        hats: "..x...x...x...x.",
        hats_row: 10,
        open: "",
        perc: "..x...........x.",
        perc_row: R_CHORD,
        echo: true,
        bass: Bass::Whole,
        ..BASE
    },
    Scenario {
        name: "pickup-fill-126",
        about: "Three beats of snare fill before bar 1, crash on bar 1",
        tempo: Tempo::Steady(126.0),
        pickup: 3,
        sections: &[(16, ALL), (8, PAD | HATS), (16, ALL)],
        ..BASE
    },
    Scenario {
        name: "drift-120-123",
        about: "Played live: drifts slowly from 120 to 123 BPM",
        tempo: Tempo::Drift(120.0, 123.0),
        kit: ANALOG,
        snare_row: R_SNARE,
        sections: &[(8, DRUMS), (32, ALL), (8, DRUMS)],
        ..BASE
    },
    Scenario {
        name: "tempo-change-128-100",
        about: "An edit: 128 BPM, then 100 from bar 25",
        tempo: Tempo::Change(128.0, 100.0, 24),
        sections: &[(24, ALL), (24, ALL)],
        ..BASE
    },
];

/// The names of the rendered tracks, in order.
pub fn names() -> impl Iterator<Item = (&'static str, &'static str)> {
    SCENARIOS.iter().map(|s| (s.name, s.about))
}

/// Renders the tracks whose name contains `only` (all of them for "").
pub fn render_all(only: &str) -> Vec<Rendered> {
    let kits: Vec<Vec<Vec<f32>>> = kits::build(RATE as f64)
        .into_iter()
        .map(|k| {
            k.sounds
                .into_iter()
                .map(|s| {
                    let audio = s.sample.to_audio();
                    audio.decoded_chunks().flatten().map(|f| f[0]).collect()
                })
                .collect()
        })
        .collect();
    SCENARIOS
        .iter()
        .filter(|s| s.name.contains(only))
        .map(|s| render(s, &kits[s.kit]))
        .collect()
}

fn steps_of(pattern: &str) -> Vec<usize> {
    pattern
        .chars()
        .enumerate()
        .filter(|(_, c)| matches!(c, 'x' | 'X' | 'o'))
        .map(|(i, _)| i)
        .collect()
}

/// Bass notes per bar, in Hz.
const BASS_LINE: [f32; 4] = [55.0, 55.0, 65.41, 49.0];
/// A minor chord for the pad.
const PAD_CHORD: [f32; 3] = [220.0, 261.63, 329.63];

fn render(s: &Scenario, kit: &[Vec<f32>]) -> Rendered {
    let sr = RATE as f64;
    let bars: usize = s.sections.iter().map(|&(b, _)| b).sum();
    let total_beats = bars * 4;
    let bpm_at = |beat: usize| match s.tempo {
        Tempo::Steady(b) => b,
        Tempo::Drift(a, b) => a + (b - a) * beat as f64 / total_beats as f64,
        Tempo::Change(a, b, bar) => {
            if beat < bar * 4 {
                a
            } else {
                b
            }
        }
    };
    // Beat times from the pickup to one past the last bar.
    let lead_in = 0.25;
    let first = bpm_at(0);
    let bar1 = lead_in + s.pickup as f64 * 60.0 / first;
    let mut times = Vec::with_capacity(total_beats + 1);
    let mut t = bar1;
    for k in 0..=total_beats {
        times.push(t);
        t += 60.0 / bpm_at(k);
    }
    let end = times[total_beats] + 3.0;
    let mut out = vec![0.0f32; (end * sr) as usize];

    let at = |beat: usize, step: usize| -> f64 {
        let (t0, t1) = (times[beat], times[beat + 1]);
        let frac = if s.steps == 4 {
            let (pair, odd) = (step / 2, step % 2);
            (pair as f64 + if odd == 1 { s.swing } else { 0.0 }) * 0.5
        } else {
            step as f64 / s.steps as f64
        };
        t0 + frac * (t1 - t0)
    };
    let hit = |out: &mut [f32], sound: &[f32], t: f64, gain: f32| {
        let i0 = (t * sr).round() as usize;
        for (k, v) in sound.iter().enumerate() {
            if let Some(o) = out.get_mut(i0 + k) {
                *o += v * gain;
            }
        }
    };

    // The fill before bar 1, and a crash on it.
    for p in 0..s.pickup {
        let beat_len = 60.0 / first;
        for q in 0..4 {
            let t = lead_in + (p as f64 + q as f64 / 4.0) * beat_len;
            hit(&mut out, &kit[R_SNARE], t, 0.35 + 0.1 * q as f32);
        }
    }
    if s.pickup > 0 {
        hit(&mut out, &kit[R_CRASH], bar1, 0.4);
    }

    let row = |p: &str| steps_of(p);
    let (kick, snare, hats, open, perc) = (
        row(s.kick),
        row(s.snare),
        row(s.hats),
        row(s.open),
        row(s.perc),
    );
    let per_bar = s.steps * 4;
    let mut bar = 0;
    for &(len, layers) in s.sections {
        for b in bar..bar + len {
            let note = BASS_LINE[b % 4];
            for (steps, on, sound, gain) in [
                (&kick, KICK, R_KICK, 0.9),
                (&snare, SNARE, s.snare_row, 0.5),
                (&hats, HATS, s.hats_row, 0.22),
                (&open, HATS, R_OPEN, 0.2),
                (&perc, PERC, s.perc_row, 0.3),
            ] {
                if layers & on == 0 || sound >= kit.len() {
                    continue;
                }
                for &st in steps.iter() {
                    let st = st % per_bar;
                    let t = at(b * 4 + st / s.steps, st % s.steps);
                    hit(&mut out, &kit[sound], t, gain);
                    if s.echo && on == PERC {
                        let sixteenth = (times[b * 4 + 1] - times[b * 4]) / 4.0;
                        for e in 1..4 {
                            let te = t + 3.0 * sixteenth * e as f64;
                            hit(&mut out, &kit[sound], te, gain * 0.5f32.powi(e));
                        }
                    }
                }
            }
            if layers & BASS != 0 {
                let notes: Vec<(f64, f64)> = match s.bass {
                    Bass::Offbeat => (0..4).map(|q| (at(b * 4 + q, 2), 0.18)).collect(),
                    Bass::Rolling => (0..4)
                        .flat_map(|q| (1..4).map(move |st| (q, st)))
                        .map(|(q, st)| (at(b * 4 + q, st), 0.09))
                        .collect(),
                    Bass::OnKick => kick
                        .iter()
                        .map(|&st| (at(b * 4 + st / s.steps, st % s.steps), 0.3))
                        .collect(),
                    Bass::Whole => vec![(times[b * 4], times[b * 4 + 4] - times[b * 4])],
                };
                for (t, len) in notes {
                    bass_note(&mut out, t, len, note, sr);
                }
            }
        }
        if layers & PAD != 0 {
            pad(&mut out, times[bar * 4], times[(bar + len) * 4], sr);
        }
        bar += len;
    }

    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let frames = out.iter().map(|v| [v * 0.9 / peak; 2]).collect();
    // The fill's beats are beats too, before bar 1.
    let beats: Vec<f64> = (0..s.pickup)
        .map(|p| lead_in + p as f64 * 60.0 / first)
        .chain(times[..total_beats].iter().copied())
        .collect();
    let downbeats = times[..total_beats].iter().copied().step_by(4).collect();
    Rendered {
        name: s.name,
        about: s.about,
        audio: Audio::from_frames(RATE, frames),
        beats,
        downbeats,
    }
}

fn bass_note(out: &mut [f32], t: f64, len: f64, freq: f32, sr: f64) {
    let (i0, n) = ((t * sr) as usize, (len * sr) as usize);
    let srf = sr as f32;
    for k in 0..n {
        let x = k as f32 / srf;
        let env = (x / 0.004).min(1.0) * (-x * 3.0 / len as f32).exp();
        let ph = TAU * freq * x;
        if let Some(o) = out.get_mut(i0 + k) {
            *o += 0.35 * env * (ph.sin() + 0.3 * (2.0 * ph).sin());
        }
    }
}

fn pad(out: &mut [f32], from: f64, to: f64, sr: f64) {
    let (i0, i1) = ((from * sr) as usize, (to * sr) as usize);
    let fade = (0.08 * sr) as usize;
    for i in i0..i1.min(out.len()) {
        let x = i as f32 / sr as f32;
        let edge = ((i - i0).min(i1 - i) as f32 / fade as f32).min(1.0);
        let v: f32 = PAD_CHORD.iter().map(|f| (TAU * f * x).sin()).sum();
        out[i] += 0.06 * edge * v;
    }
}
