//! The Remix deck's scenes: what the SCENE knob plays as it turns right
//! (Build Up) or left (Break Down). A scene is two effects in series whose
//! parameters move together on curves as the knob turns from centre to
//! the end, as the RMX-1000's Scene FX do. TIME picks the echoes' length
//! (1/4, 1/2 or 1 beat) and COLOR moves the filters down or up an octave.

use super::{Kind, MAX_PARAMETERS};

/// The Build Up scenes, by `scene_up`.
pub const SCENES_UP: [&str; 5] = [
    "Rise Echo",
    "Crush Echo",
    "Spiral Up",
    "Noise Riser",
    "Roll Up",
];
/// The Break Down scenes, by `scene_down`.
pub const SCENES_DOWN: [&str; 5] = [
    "Dub Down",
    "Wash Out",
    "Filter Down",
    "Gate Down",
    "Crush Down",
];

/// Where a scene is: `amount` 0 at centre to 1 at the end, `time` and
/// `color` 0..1 (0.5 the middle).
#[derive(Clone, Copy, Debug)]
pub(super) struct Look {
    pub(super) amount: f32,
    pub(super) time: f32,
    pub(super) color: f32,
}

impl Look {
    /// A frequency moved by COLOR: an octave down to an octave up.
    fn hz(&self, hz: f32) -> f32 {
        hz * 2f32.powf(2.0 * self.color - 1.0)
    }

    /// An echo's Time parameter (powers of two from 1/16 to 16 beats) for
    /// TIME: 1/4, 1/2 or 1 beat.
    fn echo_time(&self) -> f32 {
        0.25 + 0.125 * (2.0 * self.time).round()
    }

    /// The same, for effects whose time runs 1/4 to 1 beat over 0..1.
    fn short_time(&self) -> f32 {
        (2.0 * self.time).round() / 2.0
    }
}

/// An effect's parameters and how much of it is heard, for a [`Look`].
pub(super) type Params = fn(&Look) -> ([f32; MAX_PARAMETERS], f32);

pub(super) struct Step {
    pub(super) kind: Kind,
    pub(super) params: Params,
}

pub(super) struct Scene {
    pub(super) steps: [Step; 2],
}

/// Filter parameters: a low-pass at `lp` Hz and a high-pass at `hp` Hz,
/// with resonance `res` (the Filter effect's normalized controls).
fn filter(lp: f32, hp: f32, res: f32) -> [f32; MAX_PARAMETERS] {
    let lp = (lp / 20.0).max(1.0).ln() / 1000f32.ln();
    let hp = (hp / 20.0).max(1.0).ln() / 500f32.ln();
    [lp.clamp(0.0, 1.0), res, hp.clamp(0.0, 1.0), 0.0]
}

/// From `a` to `b` exponentially (for frequencies) as `t` goes 0..1.
fn sweep(a: f32, b: f32, t: f32) -> f32 {
    a * (b / a).powf(t)
}

/// Full wet over the first part of the turn, so the knob's start eases in.
fn ease(a: f32, over: f32) -> f32 {
    (a / over).min(1.0)
}

// --- Build Up ---------------------------------------------------------------

fn rise_filter(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // A band-pass rising from 400 Hz to 6 kHz.
    let c = l.hz(sweep(400.0, 6_000.0, l.amount));
    (filter(c * 2.0, c / 2.0, 0.35), ease(l.amount, 0.15))
}

fn rise_echo(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    let a = l.amount;
    ([l.echo_time(), 0.3 + 0.5 * a, 0.3, 0.7 * a], 1.0)
}

fn crush(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // 16 bits down to about 4, and fewer samples kept.
    let a = l.amount;
    ([1.0 - 0.85 * a, 1.0 - 0.6 * a, 0.0, 0.0], ease(a, 0.3))
}

fn crush_echo(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    let a = l.amount;
    ([l.echo_time(), 0.2 + 0.5 * a, 0.0, a], 1.0)
}

fn thin_out(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // A high-pass rising to 800 Hz.
    let hp = l.hz(sweep(20.0, 800.0, l.amount));
    (filter(20_000.0, hp, 0.3), ease(l.amount, 0.1))
}

fn spiral(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    let a = l.amount;
    ([l.short_time(), 0.3 + 0.6 * a, a, 0.0], 1.0)
}

fn riser_filter(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    let hp = l.hz(sweep(20.0, 2_000.0, l.amount));
    (filter(20_000.0, hp, 0.3), ease(l.amount, 0.1))
}

fn noise(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // Louder, and its band higher, as it rises.
    let a = l.amount;
    (
        [a, ((0.1 + 0.9 * a) * (0.5 + l.color)).min(1.0), 0.4, 0.0],
        1.0,
    )
}

fn roll_filter(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    let hp = l.hz(sweep(20.0, 1_000.0, l.amount));
    (filter(20_000.0, hp, 0.3), ease(l.amount, 0.1))
}

fn roll(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // 1/2 beat slices at the start down to 1/16 at the end.
    let a = l.amount;
    ([0.625 + 0.375 * a, 0.0, 0.0, 0.0], ease(a, 0.25))
}

// --- Break Down -------------------------------------------------------------

fn close_down(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // A low-pass closing from 20 kHz to 300 Hz.
    let lp = l.hz(sweep(20_000.0, 300.0, l.amount));
    (filter(lp, 20.0, 0.3), ease(l.amount, 0.1))
}

fn dub(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // Tape Echo: Time, Feedback, Wow, Send.
    let a = l.amount;
    ([l.echo_time(), 0.4 + 0.45 * a, 0.6, a], 1.0)
}

fn wash_filter(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    let hp = l.hz(sweep(20.0, 600.0, l.amount));
    (filter(20_000.0, hp, 0.2), ease(l.amount, 0.1))
}

fn wash(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // Hall: Decay, Pre-delay, Damping, Send.
    let a = l.amount;
    ([0.4 + 0.55 * a, 0.3, 0.4, a], 1.0)
}

fn ladder(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // Ladder: Cutoff, Resonance, Drive.
    let a = l.amount;
    let cutoff = (1.0 - 0.85 * a) * (0.8 + 0.4 * l.color);
    ([cutoff.min(1.0), 0.3 + 0.4 * a, 0.2, 0.0], ease(a, 0.1))
}

fn gate(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // Gate: Length (1/4 beat), Depth, Width.
    let a = l.amount;
    ([0.25, a, 0.6 - 0.3 * a, 0.0], 1.0)
}

fn short_gate(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // 1/8 beat chops, closing tighter.
    let a = l.amount;
    ([0.125, a, 0.5 - 0.35 * a, 0.0], 1.0)
}

fn soft_close(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    let lp = l.hz(sweep(20_000.0, 1_200.0, l.amount));
    (filter(lp, 20.0, 0.2), ease(l.amount, 0.1))
}

fn crush_down(l: &Look) -> ([f32; MAX_PARAMETERS], f32) {
    // Fewer and fewer samples kept, at 8 bits.
    let a = l.amount;
    ([0.4, 1.0 - 0.9 * a, 0.0, 0.0], ease(a, 0.3))
}

const fn step(kind: Kind, params: Params) -> Step {
    Step { kind, params }
}

pub(super) const UP: [Scene; 5] = [
    Scene {
        steps: [step(Kind::Filter, rise_filter), step(Kind::Echo, rise_echo)],
    },
    Scene {
        steps: [step(Kind::Bitcrusher, crush), step(Kind::Echo, crush_echo)],
    },
    Scene {
        steps: [step(Kind::Filter, thin_out), step(Kind::Spiral, spiral)],
    },
    Scene {
        steps: [step(Kind::Filter, riser_filter), step(Kind::Noise, noise)],
    },
    Scene {
        steps: [step(Kind::Filter, roll_filter), step(Kind::Roll, roll)],
    },
];

pub(super) const DOWN: [Scene; 5] = [
    Scene {
        steps: [step(Kind::Filter, close_down), step(Kind::TapeEcho, dub)],
    },
    Scene {
        steps: [step(Kind::Filter, wash_filter), step(Kind::Hall, wash)],
    },
    Scene {
        steps: [step(Kind::Ladder, ladder), step(Kind::Gate, gate)],
    },
    Scene {
        steps: [step(Kind::Gate, short_gate), step(Kind::Filter, soft_close)],
    },
    Scene {
        steps: [
            step(Kind::Bitcrusher, crush_down),
            step(Kind::Filter, soft_close),
        ],
    },
];
