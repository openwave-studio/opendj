//! Push 2 as a control surface: what each pad, button and encoder does, and
//! what each light should show. No I/O here: messages come in as bytes and
//! go out as bytes, so all of it can be tested without the hardware.
//!
//! Numbers from Ableton's "Push 2 MIDI and Display Interface Manual".

use std::time::{Duration, Instant};

use crate::clip::{Sound, SCENES};
use crate::control::{Controls, Mix};
use crate::engine::SlotState;
use crate::scale::{LOWER_ROOTS, UPPER_ROOTS};
use crate::sound::PARAMS;
use crate::steps::{self, Layer, Stage};

mod device;
mod mode;
mod sequencer;
mod view;

pub use device::Focus;
pub use mode::{Menu, Mode};
pub use sequencer::RESOLUTIONS;
pub use view::View;

/// Ableton's SysEx header for Push 2.
pub const SYSEX: [u8; 6] = [0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01];

/// Takes Push 2 out of Live's hands: pads and buttons talk on the User port.
pub fn user_mode() -> Vec<u8> {
    sysex(&[0x0A, 0x01])
}

/// Gives Push 2 back to Live.
pub fn live_mode() -> Vec<u8> {
    sysex(&[0x0A, 0x00])
}

pub fn sysex(body: &[u8]) -> Vec<u8> {
    let mut m = SYSEX.to_vec();
    m.extend_from_slice(body);
    m.push(0xF7);
    m
}

// Pads: note 36 is the bottom left, 99 the top right.
pub const PAD_FIRST: u8 = 36;
pub const PAD_LAST: u8 = 99;

// Buttons (control changes).
pub const PLAY: u8 = 85;
pub const STOP: u8 = 29;
pub const SHIFT: u8 = 49;
pub const SESSION: u8 = 51;
pub const NOTE: u8 = 50;
pub const DEVICE: u8 = 110;
pub const MIX: u8 = 112;
pub const PAGE_LEFT: u8 = 62;
pub const PAGE_RIGHT: u8 = 63;
pub const LEFT: u8 = 44;
pub const RIGHT: u8 = 45;
pub const UP: u8 = 46;
pub const DOWN: u8 = 47;
pub const OCTAVE_DOWN: u8 = 54;
pub const OCTAVE_UP: u8 = 55;
pub const QUANTIZE: u8 = 116;
pub const SELECT: u8 = 48;
pub const DELETE: u8 = 118;
pub const LAYOUT: u8 = 31;
pub const SCALE: u8 = 58;
pub const ACCENT: u8 = 57;
pub const TAP_TEMPO: u8 = 3;
pub const METRONOME: u8 = 9;
pub const MUTE: u8 = 60;
pub const SOLO: u8 = 61;
pub const NEW: u8 = 87;
pub const DUPLICATE: u8 = 88;
pub const DOUBLE_LOOP: u8 = 117;
/// Scene buttons right of the pads, top (1/4) to bottom (1/32t).
pub const SCENE_FIRST: u8 = 36;
/// Buttons under the display, left to right.
pub const LOWER_FIRST: u8 = 20;
/// Buttons over the display, left to right.
pub const UPPER_FIRST: u8 = 102;
/// Mode over the display, left to right.
pub const ENCODER_FIRST: u8 = 71;
pub const MASTER_ENCODER: u8 = 79;
pub const TEMPO_ENCODER: u8 = 14;

// Colour palette entries this surface writes, clear of the white
// palette's usual values.
pub const fn track_color(t: usize) -> u8 {
    64 + (t % 8) as u8
}
pub const fn track_dim(t: usize) -> u8 {
    72 + (t % 8) as u8
}
pub const WHITE: u8 = 80;
pub const GREY: u8 = 81;
pub const AMBER: u8 = 82;
pub const GREEN: u8 = 83;
pub const RED: u8 = 84;
pub const RED_DIM: u8 = 85;
pub const SOFT: u8 = 86;

/// White-light brightness.
const W_OFF: u8 = 0;
const W_DIM: u8 = 16;
const W_ON: u8 = 127;

/// LED animation, as the MIDI channel that asks for it.
const PULSE_QUARTER: u8 = 9;
const BLINK_EIGHTH: u8 = 13;

/// The first track of the eight the pads and buttons show: the bank
/// holding the selected track.
pub fn bank(c: &Controls) -> usize {
    c.selected_track() / 8 * 8
}

/// SysEx messages writing this surface's palette, then reapplying it.
/// `track_colors` starts at the bank's first track.
pub fn palette(track_colors: &[u32]) -> Vec<Vec<u8>> {
    let rgb = |c: u32| ((c >> 16) as u8, (c >> 8) as u8, c as u8);
    let scale = |(r, g, b): (u8, u8, u8), k: f32| {
        let f = |x: u8| (x as f32 * k) as u8;
        (f(r), f(g), f(b))
    };
    let mut entries: Vec<(u8, (u8, u8, u8))> = Vec::new();
    for t in 0..8 {
        let c = rgb(track_colors.get(t).copied().unwrap_or(0x808080));
        entries.push((track_color(t), c));
        entries.push((track_dim(t), scale(c, 0.16)));
    }
    entries.extend([
        (WHITE, (230, 230, 230)),
        (GREY, (40, 40, 40)),
        (AMBER, (255, 160, 0)),
        (GREEN, (60, 255, 90)),
        (RED, (255, 40, 30)),
        (RED_DIM, (60, 8, 6)),
        (SOFT, (110, 110, 110)),
    ]);
    let mut out: Vec<Vec<u8>> = entries
        .into_iter()
        .map(|(i, (r, g, b))| {
            // The white LEDs' level for the entry: its brightness.
            let w = ((r as u32 + g as u32 + b as u32) / 3) as u8;
            sysex(&[
                0x03,
                i,
                r & 0x7F,
                r >> 7,
                g & 0x7F,
                g >> 7,
                b & 0x7F,
                b >> 7,
                w & 0x7F,
                w >> 7,
            ])
        })
        .collect();
    out.push(sysex(&[0x05]));
    out
}

/// What happened to a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Down,
    Up,
    /// Still held after [`LONG_PRESS`]; its Up comes later.
    Long,
}

/// How long a button is held before it counts as held rather than tapped.
pub const LONG_PRESS: Duration = Duration::from_millis(400);

/// One light: a colour, and optionally an animation towards a second one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Led {
    color: u8,
    /// MIDI channel of the animation, 0 for none.
    anim: u8,
    to: u8,
}

impl Led {
    const OFF: Led = Led {
        color: 0,
        anim: 0,
        to: 0,
    };

    fn solid(color: u8) -> Led {
        Led {
            color,
            anim: 0,
            to: 0,
        }
    }

    fn animate(color: u8, anim: u8, to: u8) -> Led {
        Led { color, anim, to }
    }
}

pub struct Surface {
    pub view: View,
    pub mode: Mode,
    shift: bool,
    /// Octaves up or down from each sound's home range in the Note view.
    octave: i32,
    /// Pitch held by each pad in the Note view, to end it on release.
    held: [Option<(usize, u8)>; 64],
    /// What each light was last told; None until first sent.
    pads: [Option<Led>; 64],
    buttons: [Option<Led>; 128],
    /// The encoder a finger rests on.
    touched: Option<usize>,
    pub layer: Layer,
    select: bool,
    /// Steps view pads held down, by pad index.
    down: u64,
    /// Held pads already used (for locks or a semitone): their release
    /// sets nothing.
    used: u64,
    /// When each button went down, while it is down.
    pressed: [Option<Instant>; 128],
    /// Buttons whose press has turned into a long press.
    long: u128,
    /// The view a held Session or Note button goes back to on release.
    back_view: Option<View>,
    /// The mode Scale goes back to.
    before_scales: Mode,
    /// Turns towards the next detent, for encoders that pick from a list.
    turns: [i32; 8],
    /// Drum sequencer: the sound (index into the drum pads), the page of
    /// 32 steps shown, the step length (index into [`RESOLUTIONS`]).
    pub drum: usize,
    pub page: usize,
    pub res: usize,
    /// Step pads whose press added a note: their release keeps it.
    added: u64,
    /// Accent: pads play and steps add at full velocity.
    pub accent: bool,
    /// Device mode: the device the encoders turn.
    focus: Focus,
    /// Stop Clip, Mute or Solo held, for the buttons under the screen to
    /// act on tracks; and whether they have while it is held.
    held_mod: Option<u8>,
    mod_used: bool,
    /// Duplicate is held: the clip picked to copy, and whether one was
    /// pasted while it was held.
    dup: Option<Option<(usize, usize)>>,
    dup_used: bool,
}

impl Default for Surface {
    fn default() -> Self {
        Surface::new()
    }
}

impl Surface {
    pub fn new() -> Surface {
        Surface {
            view: View::Session,
            mode: Mode::Device,
            shift: false,
            octave: 0,
            held: [None; 64],
            pads: [None; 64],
            buttons: [None; 128],
            touched: None,
            layer: Layer::Gate,
            select: false,
            down: 0,
            used: 0,
            pressed: [None; 128],
            long: 0,
            back_view: None,
            before_scales: Mode::Device,
            turns: [0; 8],
            drum: 0,
            page: 0,
            res: 4,
            added: 0,
            accent: false,
            focus: Focus::default(),
            held_mod: None,
            mod_used: false,
            dup: None,
            dup_used: false,
        }
    }

    /// The stages (columns) held down in Steps mode, lowest first.
    pub fn held_stages(&self) -> Vec<usize> {
        (0..8)
            .filter(|col| (0..8).any(|row| self.down & (1 << (row * 8 + col)) != 0))
            .collect()
    }

    /// The encoder (0..8) a finger rests on, if any.
    pub fn touched(&self) -> Option<usize> {
        self.touched
    }

    /// Forgets what the lights show, so the next [`Surface::lights`] sends
    /// them all (after a reconnect).
    pub fn reset_lights(&mut self) {
        self.pads = [None; 64];
        self.buttons = [None; 128];
    }

    /// Handles one message from Push.
    pub fn input(&mut self, msg: &[u8], c: &Controls) {
        let [status, data1, data2] = match msg {
            [s, a, b, ..] => [*s, *a, *b],
            _ => return,
        };
        match status & 0xF0 {
            0x90 | 0x80 if (PAD_FIRST..=PAD_LAST).contains(&data1) => {
                let velocity = if status & 0xF0 == 0x80 { 0 } else { data2 };
                self.pad((data1 - PAD_FIRST) as usize, velocity, c);
            }
            // Encoder touch: notes 0-7.
            0x90 | 0x80 if data1 < 8 => {
                let i = data1 as usize;
                if status & 0xF0 == 0x90 && data2 > 0 {
                    self.touched = Some(i);
                } else if self.touched == Some(i) {
                    self.touched = None;
                }
            }
            0xB0 => {
                if (ENCODER_FIRST..ENCODER_FIRST + 8).contains(&data1)
                    || data1 == MASTER_ENCODER
                    || data1 == TEMPO_ENCODER
                {
                    // Relative: 1..63 right, 127..64 left.
                    let delta = if data2 < 64 {
                        data2 as i32
                    } else {
                        data2 as i32 - 128
                    };
                    self.encoder(data1, delta, c);
                } else {
                    let i = (data1 & 0x7F) as usize;
                    if data2 > 0 {
                        self.pressed[i] = Some(Instant::now());
                        self.long &= !(1u128 << i);
                        self.button(data1, Press::Down, c);
                    } else {
                        self.pressed[i] = None;
                        self.button(data1, Press::Up, c);
                    }
                }
            }
            _ => {}
        }
    }

    /// Sends a Long press for buttons held past [`LONG_PRESS`] at `now`;
    /// called every few milliseconds.
    pub fn tick(&mut self, now: Instant, c: &Controls) {
        for i in 0..128 {
            let Some(at) = self.pressed[i] else { continue };
            if self.long & (1u128 << i) == 0 && now.duration_since(at) >= LONG_PRESS {
                self.long |= 1u128 << i;
                self.button(i as u8, Press::Long, c);
            }
        }
    }

    /// Whether the button's latest press became a long one.
    fn was_long(&self, cc: u8) -> bool {
        self.long & (1u128 << (cc & 0x7F)) != 0
    }

    fn button(&mut self, cc: u8, press: Press, c: &Controls) {
        let down = press == Press::Down;
        match (cc, press) {
            (SHIFT | SELECT | STOP | MUTE | SOLO | DUPLICATE, Press::Long) => return,
            // Stop Clip, Mute and Solo: held with a track's button, act on
            // that track; on their own, on the selected track. Stop Clip
            // with Shift stops every track.
            (STOP | MUTE | SOLO, Press::Down) => {
                self.held_mod = Some(cc);
                self.mod_used = false;
                return;
            }
            (STOP | MUTE | SOLO, _) => {
                if self.held_mod == Some(cc) {
                    self.held_mod = None;
                }
                if !self.mod_used {
                    let t = c.selected_track();
                    match cc {
                        STOP if self.shift => c.stop_all(),
                        STOP => c.stop(t),
                        MUTE => c.toggle_mute(t),
                        _ => c.toggle_solo(t, self.shift),
                    }
                }
                return;
            }
            // Duplicate: held, a clip's pad then another's copies it there;
            // on its own, the selected clip goes to the next empty slot.
            (DUPLICATE, Press::Down) => {
                self.dup = Some(None);
                self.dup_used = false;
                return;
            }
            (DUPLICATE, _) => {
                self.dup = None;
                if !self.dup_used {
                    self.duplicate_selected(c);
                }
                return;
            }
            (SHIFT, _) => {
                self.shift = down;
                return;
            }
            (SELECT, _) => {
                self.select = down;
                return;
            }
            // A held Session or Note button only peeks at its view.
            (SESSION | NOTE, Press::Up) => {
                if let Some(back) = self.back_view.take() {
                    if self.was_long(cc) {
                        self.set_view(back, c);
                    }
                }
                return;
            }
            // Scale: tap to open or close, hold to peek.
            (SCALE, Press::Down) => {
                if self.mode == Mode::Scales {
                    self.mode = self.before_scales;
                } else {
                    self.before_scales = self.mode;
                    self.mode = Mode::Scales;
                }
                return;
            }
            (SCALE, Press::Up) => {
                if self.was_long(cc) && self.mode == Mode::Scales {
                    self.mode = self.before_scales;
                }
                return;
            }
            _ => {}
        }
        if !down {
            return;
        }
        if self.mode == Mode::Scales && self.scale_button(cc, c) {
            return;
        }
        if let (Some(m), true) = (self.held_mod, (LOWER_FIRST..LOWER_FIRST + 8).contains(&cc)) {
            let t = bank(c) + (cc - LOWER_FIRST) as usize;
            if t < c.tracks.len() {
                match m {
                    STOP => c.stop(t),
                    MUTE => c.toggle_mute(t),
                    // Picking tracks while Solo is held solos them all.
                    _ => c.toggle_solo(t, self.shift || self.mod_used),
                }
            }
            self.mod_used = true;
            return;
        }
        if self.mode == Mode::Device && self.device_button(cc, c) {
            return;
        }
        let tracks = c.tracks.len();
        match cc {
            PLAY => c.toggle_play(),
            SESSION => {
                self.back_view = Some(self.view);
                self.set_view(View::Session, c);
            }
            // Again (or Layout): between the keyboard and the stages.
            NOTE | LAYOUT => {
                let next = match (cc, self.view) {
                    (_, View::Note) => View::Steps,
                    (LAYOUT, View::Session) => View::Session,
                    _ => View::Note,
                };
                if cc == NOTE {
                    self.back_view = Some(self.view);
                }
                self.set_view(next, c);
            }
            DELETE => {
                // Clears the held stages' locks.
                let held = self.held_stages();
                if !held.is_empty() {
                    let (t, s) = c.steps_slot();
                    c.edit_stages(t, s, |st| {
                        for &i in &held {
                            st.stages[i].locks = [None; PARAMS];
                        }
                    });
                    self.used |= self.down;
                }
            }
            DEVICE => self.mode = Mode::Device,
            MIX => {
                // Again: the next mixer control.
                self.mode = match self.mode {
                    Mode::Mix(m) => Mode::Mix(Mix::ALL[(m as usize + 1) % 4]),
                    _ => Mode::Mix(Mix::Volume),
                };
            }
            PAGE_LEFT | PAGE_RIGHT => {
                if let Mode::Mix(m) = self.mode {
                    let step = if cc == PAGE_RIGHT { 1 } else { 3 };
                    self.mode = Mode::Mix(Mix::ALL[(m as usize + step) % 4]);
                }
            }
            LEFT | RIGHT => {
                // With Shift: the next eight tracks.
                let (t, by) = (c.selected_track(), if self.shift { 8 } else { 1 });
                let next = if cc == RIGHT {
                    (t + by).min(tracks.saturating_sub(1))
                } else {
                    t.saturating_sub(by)
                };
                self.set_track(next, c);
            }
            UP | DOWN => {
                let next = match (c.selected_scene(), cc) {
                    (None, _) => 0,
                    (Some(s), DOWN) => (s + 1).min(SCENES - 1),
                    (Some(s), _) => s.saturating_sub(1),
                };
                c.select(c.selected_track(), Some(next));
            }
            OCTAVE_UP => self.octave = (self.octave + 1).min(3),
            OCTAVE_DOWN => self.octave = (self.octave - 1).max(-3),
            ACCENT => self.accent = !self.accent,
            METRONOME => c.toggle_metronome(),
            TAP_TEMPO => {
                c.tap_tempo(Instant::now());
            }
            // New: the selected track stops, on the next empty slot, ready
            // for a new clip.
            NEW => {
                let t = c.selected_track();
                let from = c.selected_scene().unwrap_or(0);
                if let Some(s) = c.next_empty(t, from).or_else(|| c.next_empty(t, 0)) {
                    c.stop(t);
                    c.select(t, Some(s));
                }
            }
            DOUBLE_LOOP => {
                let (t, s) = c.steps_slot();
                c.double_loop(t, s);
            }
            QUANTIZE => {
                c.next_quantize();
            }
            cc if (SCENE_FIRST..SCENE_FIRST + 8).contains(&cc) => {
                let scene = (cc - SCENE_FIRST) as usize;
                if self.view == View::Steps && !self.shift {
                    if let Some(&l) = Layer::ALL.get(scene) {
                        self.layer = l;
                    }
                } else if self.sequencing(c) && !self.shift {
                    // The step length; the page keeps its place in time.
                    let at = self.page as f64 * self.resolution();
                    self.res = scene;
                    self.page = (at / self.resolution()) as usize;
                } else if self.shift {
                    c.select(c.selected_track(), Some(scene));
                } else {
                    c.launch_scene(scene);
                }
            }
            cc if (LOWER_FIRST..LOWER_FIRST + 8).contains(&cc) => {
                let t = bank(c) + (cc - LOWER_FIRST) as usize;
                if t < tracks {
                    self.set_track(t, c);
                }
            }
            cc if (UPPER_FIRST..UPPER_FIRST + 8).contains(&cc) => {
                let t = bank(c) + (cc - UPPER_FIRST) as usize;
                if t < tracks {
                    c.stop(t);
                }
            }
            _ => {}
        }
    }

    fn set_view(&mut self, view: View, c: &Controls) {
        self.end_notes(c);
        self.down = 0;
        self.used = 0;
        self.added = 0;
        self.view = view;
    }

    fn set_track(&mut self, track: usize, c: &Controls) {
        self.end_notes(c);
        self.down = 0;
        self.used = 0;
        self.added = 0;
        self.page = 0;
        c.select_track(track);
    }

    /// Ends notes held from Note mode.
    pub fn end_notes(&mut self, c: &Controls) {
        for h in self.held.iter_mut() {
            if let Some((t, p)) = h.take() {
                c.note(t, p, 0.0);
            }
        }
    }

    /// The messages that bring every light up to date; only changes are
    /// sent.
    pub fn lights(&mut self, c: &Controls) -> Vec<[u8; 3]> {
        let mut out = Vec::new();
        let stages = match self.view {
            View::Steps => {
                let (t, s) = c.steps_slot();
                c.stages(t, s)
            }
            _ => None,
        };
        let notes = if self.sequencing(c) {
            let (t, s) = c.steps_slot();
            c.notes(t, s)
        } else {
            None
        };
        for index in 0..64 {
            let led = self.pad_led(index, c, stages.as_ref(), notes.as_ref());
            if self.pads[index] != Some(led) {
                self.pads[index] = Some(led);
                send(&mut out, 0x90, PAD_FIRST + index as u8, led);
            }
        }
        for (cc, led) in self.button_leds(c) {
            let slot = &mut self.buttons[cc as usize];
            if *slot != Some(led) {
                *slot = Some(led);
                send(&mut out, 0xB0, cc, led);
            }
        }
        out
    }

    fn button_leds(&self, c: &Controls) -> Vec<(u8, Led)> {
        let m = &c.monitor;
        let tracks = c.tracks.len();
        let white = |on: bool| Led::solid(if on { W_ON } else { W_DIM });
        let mut v = Vec::with_capacity(40);
        v.push((PLAY, Led::solid(if m.playing() { GREEN } else { GREY })));
        v.push((STOP, Led::solid(GREY)));
        let sequencing = self.sequencing(c);
        for s in 0..8 {
            if sequencing {
                let led = Led::solid(if s == self.res { WHITE } else { SOFT });
                v.push((SCENE_FIRST + s as u8, led));
                continue;
            }
            if self.view == View::Steps {
                let led = match Layer::ALL.get(s) {
                    Some(&l) if l == self.layer => Led::solid(WHITE),
                    Some(_) => Led::solid(SOFT),
                    None => Led::OFF,
                };
                v.push((SCENE_FIRST + s as u8, led));
                continue;
            }
            let playing = (0..tracks).any(|t| m.slot(t, s) == SlotState::Playing);
            let queued = (0..tracks).any(|t| m.slot(t, s) == SlotState::Queued);
            let any = (0..tracks).any(|t| c.has_clip(t, s));
            let led = if queued {
                Led::animate(GREY, BLINK_EIGHTH, GREEN)
            } else if playing {
                Led::solid(GREEN)
            } else if any {
                Led::solid(GREY)
            } else {
                Led::OFF
            };
            v.push((SCENE_FIRST + s as u8, led));
        }
        let selected = c.selected_track();
        let scales = self.mode == Mode::Scales || self.mode == Mode::Device;
        if self.mode == Mode::Scales {
            v.extend(self.scale_leds(c));
        }
        if self.mode == Mode::Device {
            v.extend(self.device_leds(c));
        }
        for t in bank(c)..bank(c) + 8 {
            if scales {
                break;
            }
            let lower = if t >= tracks {
                Led::OFF
            } else if t == selected {
                Led::solid(track_color(t))
            } else {
                Led::solid(track_dim(t))
            };
            v.push((LOWER_FIRST + (t % 8) as u8, lower));
            let playing = t < tracks && m.progress(t) >= 0.0;
            let upper = if !playing {
                Led::OFF
            } else if m.stopping(t) {
                Led::animate(RED_DIM, BLINK_EIGHTH, RED)
            } else {
                Led::solid(RED_DIM)
            };
            v.push((UPPER_FIRST + (t % 8) as u8, upper));
        }
        v.push((SESSION, white(self.view == View::Session)));
        v.push((NOTE, white(matches!(self.view, View::Note | View::Steps))));
        let steps = self.view == View::Steps;
        v.push((
            LAYOUT,
            Led::solid(if steps || self.view == View::Note {
                W_DIM
            } else {
                W_OFF
            }),
        ));
        v.push((
            SELECT,
            Led::solid(if self.select {
                W_ON
            } else if steps {
                W_DIM
            } else {
                W_OFF
            }),
        ));
        v.push((
            DELETE,
            Led::solid(if steps && self.down != 0 { W_ON } else { W_OFF }),
        ));
        v.push((DEVICE, white(self.mode == Mode::Device)));
        v.push((SCALE, white(self.mode == Mode::Scales)));
        v.push((ACCENT, white(self.accent)));
        v.push((MIX, white(matches!(self.mode, Mode::Mix(_)))));
        v.push((SHIFT, white(self.shift)));
        let pages = matches!(self.mode, Mode::Mix(_) | Mode::Device);
        for cc in [PAGE_LEFT, PAGE_RIGHT] {
            v.push((cc, Led::solid(if pages { W_DIM } else { W_OFF })));
        }
        for cc in [LEFT, RIGHT, UP, DOWN, QUANTIZE] {
            v.push((cc, Led::solid(W_DIM)));
        }
        let note = self.view == View::Note;
        for cc in [OCTAVE_DOWN, OCTAVE_UP] {
            v.push((cc, Led::solid(if note { W_DIM } else { W_OFF })));
        }
        let t = c.selected_track();
        v.push((METRONOME, white(c.metronome())));
        v.push((
            TAP_TEMPO,
            if m.playing() {
                Led::animate(W_DIM, PULSE_QUARTER, W_ON)
            } else {
                Led::solid(W_DIM)
            },
        ));
        v.push((MUTE, white(c.muted(t))));
        v.push((SOLO, white(c.soloed(t))));
        v.push((DUPLICATE, white(self.dup.is_some())));
        for cc in [NEW, DOUBLE_LOOP] {
            v.push((cc, Led::solid(W_DIM)));
        }
        // Stop Clip, Mute or Solo held: the buttons under the screen show
        // which tracks it would act on.
        if let Some(held) = self.held_mod {
            v.retain(|(cc, _)| !(LOWER_FIRST..LOWER_FIRST + 8).contains(cc));
            for t in bank(c)..bank(c) + 8 {
                let lit = t < tracks
                    && match held {
                        STOP => m.progress(t) >= 0.0,
                        MUTE => !c.muted(t),
                        _ => c.soloed(t),
                    };
                let led = match (t < tracks, lit, held) {
                    (false, _, _) => Led::OFF,
                    (_, true, STOP) => Led::solid(RED),
                    (_, true, _) => Led::solid(track_color(t)),
                    (_, false, _) => Led::solid(track_dim(t)),
                };
                v.push((LOWER_FIRST + (t % 8) as u8, led));
            }
        }
        v
    }

    /// Duplicate on its own: the selected clip (or the one the track
    /// plays) goes to the next empty slot below it, which is then selected.
    fn duplicate_selected(&mut self, c: &Controls) {
        let (t, s) = c.steps_slot();
        if !c.has_clip(t, s) {
            return;
        }
        if let Some(to) = c.next_empty(t, s + 1) {
            if c.copy_clip(t, s, t, to) {
                c.select(t, Some(to));
            }
        }
    }

    /// Messages turning every light off, for when the studio lets go.
    pub fn all_off() -> Vec<[u8; 3]> {
        let mut out = Vec::new();
        for n in PAD_FIRST..=PAD_LAST {
            out.push([0x90, n, 0]);
        }
        for cc in 0..128u8 {
            out.push([0xB0, cc, 0]);
        }
        out
    }
}

fn send(out: &mut Vec<[u8; 3]>, kind: u8, number: u8, led: Led) {
    // Channel 0 sets the colour and stops any animation; another channel
    // then starts one towards the second colour.
    out.push([kind, number, led.color]);
    if led.anim != 0 {
        out.push([kind | led.anim, number, led.to]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Command, Studio};

    fn controls() -> (Controls, rtrb::Consumer<Command>) {
        let set = crate::clip::demo_set();
        let (tx, rx) = rtrb::RingBuffer::new(64);
        let monitor = Studio::new(&set, 48_000.0, 120.0).1.monitor;
        (Controls::new(&set, &[0xff0000; 4], tx, monitor), rx)
    }

    #[test]
    fn device_mode_picks_devices_turns_macros_and_goes_into_chains() {
        use crate::clip::DeviceInfo;
        use crate::devices::{Settings, UtilitySettings};
        use crate::rack::{Kind, RackSettings};
        let utility = DeviceInfo {
            name: "Utility".into(),
            kind: "Built-in".into(),
            on: true,
            plugin: None,
            builtin: Some(Settings::Utility(UtilitySettings::default())),
        };
        let mut rack = RackSettings::new(Kind::Audio);
        rack.add_chain(Some(utility.clone()));
        let mut set = crate::clip::demo_set();
        set[0].devices = vec![
            utility,
            DeviceInfo {
                name: "Audio Effect Rack".into(),
                kind: "Built-in".into(),
                on: true,
                plugin: None,
                builtin: Some(Settings::Rack(rack)),
            },
        ];
        let (tx, mut rx) = rtrb::RingBuffer::new(64);
        let monitor = Studio::new(&set, 48_000.0, 120.0).1.monitor;
        let c = Controls::new(&set, &[0xff0000; 4], tx, monitor);
        let mut s = Surface::new();
        // Under the screen: the track's sound, then its two devices.
        let (_, lower) = s.menus(&c).expect("device menus");
        let names: Vec<&str> = lower.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(names, ["Drums", "Utility", "Audio Effect Rack"]);
        assert!(lower[0].on);
        // The rack: its macros on the encoders, its chain over the screen.
        s.input(&[0xB0, LOWER_FIRST + 2, 127], &c);
        let knobs = s.device_knobs(&c);
        assert_eq!(knobs[0].name, "Macro 1");
        let (upper, _) = s.menus(&c).unwrap();
        assert_eq!(upper[0].label, "Utility");
        s.input(&[0xB0, ENCODER_FIRST, 10], &c);
        assert!(matches!(
            rx.pop(),
            Ok(Command::DeviceParam {
                track: 0,
                device: 1,
                index: 0,
                ..
            })
        ));
        // Into the chain: the Utility's settings, as the rack's inner ones.
        s.input(&[0xB0, UPPER_FIRST, 127], &c);
        assert_eq!(s.device_knobs(&c)[0].name, "Gain");
        let (_, lower) = s.menus(&c).unwrap();
        assert_eq!(lower[0].label, "◀ Audio Effect Rack");
        s.input(&[0xB0, ENCODER_FIRST, 10], &c);
        let Ok(Command::DeviceParam {
            device: 1, index, ..
        }) = rx.pop()
        else {
            panic!("no inner change");
        };
        assert_eq!(crate::rack::split(index), Some((0, 0, 0)));
        // Back out, and a new track starts on its sound again.
        s.input(&[0xB0, LOWER_FIRST, 127], &c);
        assert_eq!(s.device_knobs(&c)[0].name, "Macro 1");
        c.select_track(1);
        assert_eq!(s.focus(&c).device, None);
    }

    #[test]
    fn mute_solo_metronome_tap_new_duplicate_and_double() {
        let mut set = crate::clip::demo_set();
        for t in &mut set {
            for slot in &mut t.slots[4..] {
                *slot = None;
            }
        }
        let (tx, _rx) = rtrb::RingBuffer::new(256);
        let monitor = Studio::new(&set, 48_000.0, 120.0).1.monitor;
        let c = Controls::new(&set, &[0xff0000; 4], tx, monitor);
        let mut s = Surface::new();
        let press = |s: &mut Surface, cc: u8, on: bool| {
            s.input(&[0xB0, cc, if on { 127 } else { 0 }], &c);
        };
        let tap = |s: &mut Surface, cc: u8| {
            s.input(&[0xB0, cc, 127], &c);
            s.input(&[0xB0, cc, 0], &c);
        };
        // Mute on its own: the selected track; held, the tracks picked.
        c.select_track(2);
        tap(&mut s, MUTE);
        assert!(c.muted(2));
        press(&mut s, MUTE, true);
        press(&mut s, LOWER_FIRST, true);
        press(&mut s, LOWER_FIRST + 2, true);
        press(&mut s, MUTE, false);
        assert!(c.muted(0) && !c.muted(2), "{:?}", (c.muted(0), c.muted(2)));
        // While held, the buttons under the screen show what sounds.
        press(&mut s, MUTE, true);
        let leds = s.button_leds(&c);
        let lower = |i: u8| {
            leds.iter()
                .find(|(cc, _)| *cc == LOWER_FIRST + i)
                .map(|l| l.1)
        };
        assert_eq!(lower(0), Some(Led::solid(track_dim(0))));
        assert_eq!(lower(1), Some(Led::solid(track_color(1))));
        press(&mut s, MUTE, false);

        // Solo on its own takes the other solos off; held, it adds them up.
        tap(&mut s, SOLO);
        assert!(c.soloed(2));
        press(&mut s, SOLO, true);
        press(&mut s, LOWER_FIRST + 1, true);
        press(&mut s, LOWER_FIRST + 3, true);
        press(&mut s, SOLO, false);
        assert!(!c.soloed(2) && c.soloed(1) && c.soloed(3));

        tap(&mut s, METRONOME);
        assert!(c.metronome());

        // Duplicate on its own: the selected clip, to the next empty slot.
        let t = (0..c.tracks.len())
            .find(|&t| c.has_clip(t, 0) && c.next_empty(t, 1).is_some())
            .expect("a track with a clip and room");
        let to = c.next_empty(t, 1).expect("room");
        c.select(t, Some(0));
        tap(&mut s, DUPLICATE);
        assert!(c.has_clip(t, to));
        assert_eq!(c.selected_scene(), Some(to));
        // Held: a clip's pad, then another's (scene 7 is the bottom row).
        let pad = |t: usize, scene: usize| 36 + ((7 - scene) * 8 + t) as u8;
        press(&mut s, DUPLICATE, true);
        s.input(&[0x90, pad(t, 0), 100], &c);
        s.input(&[0x90, pad(t, 7), 100], &c);
        press(&mut s, DUPLICATE, false);
        assert!(c.has_clip(t, 7));

        // Double: the selected clip's loop, twice as long.
        let beats = c.clip_info(t, 7).map(|i| i.1);
        c.select(t, Some(7));
        tap(&mut s, DOUBLE_LOOP);
        assert_eq!(c.clip_info(t, 7).map(|i| i.1), beats.map(|b| b * 2.0));

        // New: on the next empty slot.
        c.select(t, Some(0));
        tap(&mut s, NEW);
        let picked = c.selected_scene().expect("a slot");
        assert!(!c.has_clip(t, picked));

        // Tap Tempo doesn't set anything from one tap.
        tap(&mut s, TAP_TEMPO);
    }

    #[test]
    fn top_left_pad_launches_track_one_scene_one() {
        let (c, mut rx) = controls();
        let mut s = Surface::new();
        // Top-left pad: note 92.
        s.input(&[0x90, 92, 100], &c);
        assert!(matches!(
            rx.pop(),
            Ok(Command::Launch { track: 0, scene: 0 })
        ));
        // Bottom-right of the four tracks: note 39, scene 8.
        s.input(&[0x90, 39, 100], &c);
        assert!(matches!(
            rx.pop(),
            Ok(Command::Launch { track: 3, scene: 7 })
        ));
        // Columns past the last track do nothing.
        s.input(&[0x90, 40, 100], &c);
        assert!(rx.pop().is_err());
    }

    #[test]
    fn scene_buttons_and_stops() {
        let (c, mut rx) = controls();
        let mut s = Surface::new();
        s.input(&[0xB0, SCENE_FIRST + 2, 127], &c);
        assert!(matches!(rx.pop(), Ok(Command::Scene(2))));
        // Release does nothing.
        s.input(&[0xB0, SCENE_FIRST + 2, 0], &c);
        assert!(rx.pop().is_err());
        // Stop Clip held with track 2's button stops it, and nothing more
        // on release.
        s.input(&[0xB0, STOP, 127], &c);
        s.input(&[0xB0, LOWER_FIRST + 1, 127], &c);
        s.input(&[0xB0, STOP, 0], &c);
        assert!(matches!(rx.pop(), Ok(Command::Stop(1))));
        assert!(rx.pop().is_err());
        // On its own, the selected track; with Shift, all.
        s.input(&[0xB0, STOP, 127], &c);
        s.input(&[0xB0, STOP, 0], &c);
        assert!(matches!(rx.pop(), Ok(Command::Stop(0))));
        s.input(&[0xB0, SHIFT, 127], &c);
        s.input(&[0xB0, STOP, 127], &c);
        s.input(&[0xB0, STOP, 0], &c);
        assert!(matches!(rx.pop(), Ok(Command::StopAll)));
        s.input(&[0xB0, SHIFT, 0], &c);
        // In Mix mode the buttons over the screen stop their tracks.
        s.input(&[0xB0, MIX, 127], &c);
        s.input(&[0xB0, UPPER_FIRST + 1, 127], &c);
        assert!(matches!(rx.pop(), Ok(Command::Stop(1))));
    }

    #[test]
    fn encoders_turn_device_knobs_and_mixer() {
        let (c, mut rx) = controls();
        let mut s = Surface::new();
        c.select_track(1);
        assert_eq!(c.selected_track(), 1);
        let before = c.param(1, 0);
        s.input(&[0xB0, ENCODER_FIRST, 4], &c);
        assert!((c.param(1, 0) - before - 0.02).abs() < 1e-5);
        assert!(matches!(
            rx.pop(),
            Ok(Command::Param {
                track: 1,
                index: 0,
                ..
            })
        ));
        // Mix, then left turns on encoder 3: track 3's volume down.
        s.input(&[0xB0, MIX, 127], &c);
        let before = c.mix(2, Mix::Volume);
        s.input(&[0xB0, ENCODER_FIRST + 2, 126], &c);
        assert!((before - c.mix(2, Mix::Volume) - 0.01).abs() < 1e-5);
        assert!(matches!(rx.pop(), Ok(Command::Volume { track: 2, .. })));
        // Mix again: pan.
        s.input(&[0xB0, MIX, 127], &c);
        assert_eq!(s.mode, Mode::Mix(Mix::Pan));
    }

    #[test]
    fn note_mode_plays_and_releases() {
        let (c, mut rx) = controls();
        let mut s = Surface::new();
        // Drums are track 1: bottom-left pad is the kick.
        s.input(&[0xB0, NOTE, 127], &c);
        s.input(&[0x90, 36, 127], &c);
        assert!(matches!(
            rx.pop(),
            Ok(Command::Note {
                track: 0,
                pitch: 36,
                ..
            })
        ));
        s.input(&[0x80, 36, 0], &c);
        assert!(matches!(
            rx.pop(),
            Ok(Command::Note { track: 0, pitch: 36, velocity }) if velocity == 0.0
        ));
        // Bass: bottom-left is A1, the next pad up a fourth (D2).
        c.select_track(1);
        s.input(&[0x90, 44, 90], &c);
        assert!(matches!(
            rx.pop(),
            Ok(Command::Note {
                track: 1,
                pitch: 38,
                ..
            })
        ));
    }

    #[test]
    fn scales_mode_sets_the_key_the_pads_play() {
        let (c, mut rx) = controls();
        let mut s = Surface::new();
        let tap = |s: &mut Surface, cc: u8| {
            s.input(&[0xB0, cc, 127], &c);
            s.input(&[0xB0, cc, 0], &c);
        };
        tap(&mut s, SCALE);
        assert_eq!(s.mode, Mode::Scales);
        // Root C (second button over the screen), then Major (the previous
        // scale from Minor), then the 3rd-up layout with the third encoder.
        tap(&mut s, UPPER_FIRST + 1);
        tap(&mut s, UPPER_FIRST);
        s.input(&[0xB0, ENCODER_FIRST + 2, 6], &c);
        s.input(&[0xB0, ENCODER_FIRST + 2, 6], &c);
        let key = c.key();
        assert_eq!(key.name(), "C Major");
        assert_eq!(key.layout, crate::scale::Layout::ThirdUp);
        let (upper, lower) = s.menus(&c).expect("menus");
        assert!(upper[1].on && upper[1].label == "C");
        assert_eq!(lower[7].label, "Chromatic");
        // Bass in C major: bottom-left C1, a third up E1.
        tap(&mut s, LOWER_FIRST + 7);
        assert!(c.key().chromatic);
        tap(&mut s, LOWER_FIRST + 7);
        tap(&mut s, SCALE);
        assert_eq!(s.mode, Mode::Device);
        c.select_track(1);
        tap(&mut s, NOTE);
        while rx.pop().is_ok() {}
        s.input(&[0x90, 44, 90], &c);
        assert!(matches!(
            rx.pop(),
            Ok(Command::Note {
                track: 1,
                pitch: 28,
                ..
            })
        ));
    }

    #[test]
    fn drum_sequencer_adds_lengthens_edits_and_removes() {
        let (c, _rx) = controls();
        let mut s = Surface::new();
        // Drums, scene 3, emptied.
        c.edit_notes(0, 2, |clip| {
            clip.notes.clear();
            clip.beats = 4.0;
        });
        c.select(0, Some(2));
        s.input(&[0xB0, NOTE, 127], &c);
        s.input(&[0xB0, NOTE, 0], &c);
        assert!(s.sequencing(&c));
        // Snare (second sound pad), then step 5 (top row, fifth pad).
        s.input(&[0x90, PAD_FIRST + 1, 100], &c);
        s.input(&[0x80, PAD_FIRST + 1, 0], &c);
        let step = |i: u8| PAD_FIRST + 56 + i;
        s.input(&[0x90, step(4), 127], &c);
        s.input(&[0x80, step(4), 0], &c);
        let (beats, notes) = c.notes(0, 2).expect("note clip");
        assert_eq!(beats, 4.0);
        assert_eq!(notes.len(), 1);
        assert_eq!((notes[0].start, notes[0].pitch), (1.0, 38));
        // Hold step 1, tap step 3: one note over three steps.
        s.input(&[0x90, step(0), 100], &c);
        s.input(&[0x90, step(2), 100], &c);
        s.input(&[0x80, step(2), 0], &c);
        // Still held: the encoders edit its velocity.
        s.input(&[0xB0, ENCODER_FIRST, 127 - 19], &c);
        s.input(&[0x80, step(0), 0], &c);
        let (_, notes) = c.notes(0, 2).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].start, notes[0].length), (0.0, 0.75));
        assert!((notes[0].velocity - (100.0 / 127.0 - 0.1)).abs() < 1e-3);
        // The 1/8 scene button: step 3 now starts on beat 1, where the
        // snare is; tapping it removes it.
        s.input(&[0xB0, SCENE_FIRST + 2, 127], &c);
        s.input(&[0x90, step(2), 100], &c);
        s.input(&[0x80, step(2), 0], &c);
        let (_, notes) = c.notes(0, 2).unwrap();
        assert_eq!(notes.len(), 1);
        // Hold page 1, tap page 2: two pages of 32 eighths.
        let page = |i: u8| PAD_FIRST + 24 + 4 + i;
        s.input(&[0x90, page(0), 100], &c);
        s.input(&[0x90, page(1), 100], &c);
        assert_eq!(c.notes(0, 2).unwrap().0, 32.0);
        // Delete + snare pad clears the snare.
        s.input(&[0xB0, DELETE, 127], &c);
        s.input(&[0x90, PAD_FIRST + 1, 100], &c);
        assert!(c.notes(0, 2).unwrap().1.is_empty());
    }

    #[test]
    fn held_session_button_only_peeks() {
        let (c, _rx) = controls();
        let mut s = Surface::new();
        s.input(&[0xB0, NOTE, 127], &c);
        s.input(&[0xB0, NOTE, 0], &c);
        assert_eq!(s.view, View::Note);
        // Tapped: stays.
        s.input(&[0xB0, SESSION, 127], &c);
        s.input(&[0xB0, SESSION, 0], &c);
        assert_eq!(s.view, View::Session);
        s.input(&[0xB0, NOTE, 127], &c);
        s.input(&[0xB0, NOTE, 0], &c);
        // Held: back to Note on release.
        s.input(&[0xB0, SESSION, 127], &c);
        s.tick(Instant::now() + LONG_PRESS * 2, &c);
        assert_eq!(s.view, View::Session);
        s.input(&[0xB0, SESSION, 0], &c);
        assert_eq!(s.view, View::Note);
        // Scale held: a peek too.
        s.input(&[0xB0, SCALE, 127], &c);
        assert_eq!(s.mode, Mode::Scales);
        s.tick(Instant::now() + LONG_PRESS * 2, &c);
        s.input(&[0xB0, SCALE, 0], &c);
        assert_eq!(s.mode, Mode::Device);
    }

    #[test]
    fn lights_only_send_changes() {
        let (c, _rx) = controls();
        let mut s = Surface::new();
        let first = s.lights(&c);
        // Track 1 scene 1 holds a clip: its pad (92) is lit dim in the
        // track's colour.
        assert!(first.contains(&[0x90, 92, track_dim(0)]));
        assert!(first.contains(&[0xB0, PLAY, GREY]));
        assert!(s.lights(&c).is_empty());
        s.input(&[0xB0, NOTE, 127], &c);
        assert!(!s.lights(&c).is_empty());
    }

    #[test]
    fn palette_messages_are_valid_sysex() {
        let msgs = palette(&[0xff8a65, 0x4fc3f7]);
        assert_eq!(msgs.len(), 16 + 7 + 1);
        for m in &msgs {
            assert_eq!(&m[..6], &SYSEX);
            assert_eq!(*m.last().unwrap(), 0xF7);
            assert!(m[1..m.len() - 1].iter().all(|b| *b < 0x80));
        }
        // Entry 64, track 1: 0xff8a65 = 255, 138, 101.
        assert_eq!(
            &msgs[0][6..16],
            &[0x03, 64, 0x7F, 1, 0x0A, 1, 101, 0, 0x24, 1]
        );
        assert_eq!(user_mode(), vec![0xF0, 0, 0x21, 0x1D, 1, 1, 0x0A, 1, 0xF7]);
    }

    #[test]
    fn steps_mode_edits_and_locks() {
        let (c, _rx) = controls();
        let mut s = Surface::new();
        let press = |s: &mut Surface, cc: u8| {
            s.input(&[0xB0, cc, 127], &c);
            s.input(&[0xB0, cc, 0], &c);
        };
        let pad = |s: &mut Surface, row: u8, col: u8, down: bool| {
            s.input(
                &[0x90, PAD_FIRST + row * 8 + col, if down { 100 } else { 0 }],
                &c,
            );
        };
        // Lead (track 3), scene 8 is empty: Steps makes a clip there.
        c.select_track(3);
        s.input(&[0xB0, SHIFT, 127], &c);
        press(&mut s, SCENE_FIRST + 7);
        s.input(&[0xB0, SHIFT, 0], &c);
        press(&mut s, NOTE);
        press(&mut s, NOTE);
        assert_eq!(s.view, View::Steps);
        assert!(!c.has_clip(3, 7));
        press(&mut s, SCENE_FIRST + 3);
        assert_eq!(s.layer, Layer::Gate);
        pad(&mut s, 2, 0, true);
        pad(&mut s, 2, 0, false);
        let st = c.stages(3, 7).expect("stage clip");
        assert_eq!((st.stages[0].pulses, st.stages[0].gates), (3, 0b100));
        assert!(c.has_clip(3, 7));
        // Hold stage 2 and turn the first encoder: a lock, and the release
        // sets nothing.
        pad(&mut s, 0, 1, true);
        s.input(&[0xB0, ENCODER_FIRST, 10], &c);
        pad(&mut s, 0, 1, false);
        let st = c.stages(3, 7).unwrap();
        let base = c.param(3, 0);
        assert!((st.stages[1].locks[0].unwrap() - (base + 0.05)).abs() < 1e-4);
        assert_eq!(st.stages[1].gates, 0);
        // Hold and Delete clears it.
        pad(&mut s, 0, 1, true);
        press(&mut s, DELETE);
        pad(&mut s, 0, 1, false);
        assert!(!c.stages(3, 7).unwrap().stages[1].has_locks());
        // A note clip isn't touched.
        s.input(&[0xB0, SHIFT, 127], &c);
        press(&mut s, SCENE_FIRST + 3);
        s.input(&[0xB0, SHIFT, 0], &c);
        assert!(c.stages(3, 3).is_none());
        pad(&mut s, 0, 0, true);
        pad(&mut s, 0, 0, false);
        assert!(c.stages(3, 3).is_none());
        let lights = s.lights(&c);
        assert!(lights.contains(&[0x90, PAD_FIRST, 0]));
    }
}
