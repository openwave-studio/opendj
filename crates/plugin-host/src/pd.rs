//! Pure Data patches as effects and instruments (experimental, feature
//! `pd`): the helper runs the patch in libpd, Pd's engine as a library.
//! How a patch talks to openDJ is in [`crate::pd_patch`].
//!
//! One patch per helper process, like every other plugin: a patch that
//! crashes or hangs costs its own slot.

use std::ffi::{c_char, CStr, CString};
use std::path::Path;
use std::sync::Once;

use libpd_sys as pd;

use crate::client::Transport;
use crate::effects::{arrange, Effect, Info, Loaded, Param};
use crate::pd_patch::{param_id, read_patch, BEAT, BPM, PLAYING};
use crate::shm::{Midi, CHANNELS};

/// Frames Pd processes at a time (one "tick").
const TICK: usize = 64;

static INIT: Once = Once::new();

unsafe extern "C" fn print_line(line: *const c_char) {
    if !line.is_null() {
        // SAFETY: libpd hands a NUL-terminated line.
        let text = unsafe { CStr::from_ptr(line) }.to_string_lossy();
        eprint!("pd: {text}");
    }
}

/// Starts libpd once per process, its prints going to stderr.
fn init() {
    INIT.call_once(|| {
        // SAFETY: plain libpd set-up calls, once, before any other.
        unsafe {
            pd::libpd_init();
            // Hooks belong to the instance, so they come after it exists.
            pd::libpd_set_concatenated_printhook(Some(print_line));
            pd::libpd_set_printhook(Some(pd::libpd_print_concatenator));
        }
    });
}

fn c(text: &str) -> Result<CString, String> {
    CString::new(text).map_err(|_| format!("{text:?} has a NUL in it"))
}

/// Loads the patch `file` for openDJ, with knobs in `picks` order.
pub fn load(
    file: &Path,
    sample_rate: u32,
    max_frames: usize,
    picks: &[(usize, u32)],
) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let patch = read_patch(&text);
    let dir = file.parent().unwrap_or(Path::new("."));
    let base = file
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the patch's file name isn't text")?;
    let (dir_c, base_c) = (c(&dir.to_string_lossy())?, c(base)?);
    init();
    // SAFETY: libpd is set up; this helper has one patch and calls libpd
    // from one thread at a time (here, then the audio thread).
    unsafe {
        if pd::libpd_init_audio(CHANNELS as i32, CHANNELS as i32, sample_rate as i32) != 0 {
            return Err("Pd couldn't start its audio".into());
        }
        pd::libpd_add_to_search_path(dir_c.as_ptr());
        if pd::libpd_openfile(base_c.as_ptr(), dir_c.as_ptr()).is_null() {
            return Err(format!("Pd couldn't open {}", file.display()));
        }
        // DSP on: [; pd dsp 1(
        pd::libpd_start_message(1);
        pd::libpd_add_float(1.0);
        let (pd_c, dsp_c) = (c("pd")?, c("dsp")?);
        pd::libpd_finish_message(pd_c.as_ptr(), dsp_c.as_ptr());
    }
    let mut params: Vec<(CString, Param)> = Vec::new();
    for p in &patch.params {
        params.push((
            c(&p.receive)?,
            Param {
                id: param_id(&p.receive),
                name: p.name.clone(),
                min: p.min,
                max: p.max,
                default: p.default,
                value: p.default,
            },
        ));
    }
    let all = arrange(&mut params, &[], picks);
    let (receives, params): (Vec<CString>, Vec<Param>) = params.into_iter().unzip();
    let name = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Pd patch".into());
    Ok(Loaded {
        effect: Box::new(PdEffect::new(receives, max_frames)?),
        main: None,
        info: Info {
            name,
            params,
            all,
            pages: Vec::new(),
            latency: 0,
        },
    })
}

/// The patch, on the helper's audio thread.
pub struct PdEffect {
    receives: Vec<CString>,
    /// The values last sent, so only changes go to Pd.
    sent: Vec<f32>,
    transport: [CString; 3],
    last_bpm: f64,
    last_playing: Option<bool>,
    /// Interleaved input and output for a whole block.
    input: Vec<f32>,
    output: Vec<f32>,
    /// Set once a block arrives that isn't whole ticks: from then on audio
    /// goes through `fifo`, one tick (64 frames, about 1.3 ms) late.
    fifo: Option<Fifo>,
    /// Whether the audio thread was pointed at the patch's Pd instance.
    on_this_thread: bool,
}

/// Turns blocks of any size into whole ticks, a tick late.
struct Fifo {
    tick_in: [f32; TICK * CHANNELS],
    tick_out: [f32; TICK * CHANNELS],
    filled: usize,
    /// Processed frames waiting to go out (interleaved ring).
    ring: [f32; 2 * TICK * CHANNELS],
    read: usize,
    queued: usize,
}

impl Fifo {
    fn new() -> Self {
        Self {
            tick_in: [0.0; TICK * CHANNELS],
            tick_out: [0.0; TICK * CHANNELS],
            filled: 0,
            ring: [0.0; 2 * TICK * CHANNELS],
            read: 0,
            // A tick of silence first, so a frame is always ready.
            queued: TICK,
        }
    }
}

impl PdEffect {
    fn new(receives: Vec<CString>, max_frames: usize) -> Result<Self, String> {
        let sent = vec![f32::NAN; receives.len()];
        Ok(Self {
            receives,
            sent,
            transport: [c(BPM)?, c(BEAT)?, c(PLAYING)?],
            last_bpm: f64::NAN,
            last_playing: None,
            input: vec![0.0; max_frames * CHANNELS],
            output: vec![0.0; max_frames * CHANNELS],
            fifo: None,
            on_this_thread: false,
        })
    }

    fn send_params(&mut self, params: &[f32]) {
        for (i, (recv, sent)) in self.receives.iter().zip(&mut self.sent).enumerate() {
            let Some(&v) = params.get(i) else { break };
            if v.to_bits() != sent.to_bits() {
                *sent = v;
                // SAFETY: a NUL-terminated name, on the one thread using Pd.
                unsafe { pd::libpd_float(recv.as_ptr(), v) };
            }
        }
    }

    fn send_transport(&mut self, t: &Transport) {
        let [bpm, beat, playing] = &self.transport;
        // SAFETY: as in `send_params`.
        unsafe {
            if t.bpm != self.last_bpm {
                self.last_bpm = t.bpm;
                pd::libpd_float(bpm.as_ptr(), t.bpm as f32);
            }
            if self.last_playing != Some(t.playing) {
                self.last_playing = Some(t.playing);
                pd::libpd_float(playing.as_ptr(), if t.playing { 1.0 } else { 0.0 });
            }
            pd::libpd_float(beat.as_ptr(), t.beat as f32);
        }
    }

    fn send_midi(events: &[Midi]) {
        for e in events {
            let [status, a, b] = e.data;
            let ch = (status & 15) as i32;
            // SAFETY: as in `send_params`.
            unsafe {
                match status & 0xf0 {
                    0x90 => pd::libpd_noteon(ch, a as i32, b as i32),
                    0x80 => pd::libpd_noteon(ch, a as i32, 0),
                    0xb0 => pd::libpd_controlchange(ch, a as i32, b as i32),
                    0xe0 => pd::libpd_pitchbend(ch, ((b as i32) << 7 | a as i32) - 8192),
                    _ => 0,
                };
            }
        }
    }
}

impl Effect for PdEffect {
    fn process(
        &mut self,
        input: [&[f32]; CHANNELS],
        output: [&mut [f32]; CHANNELS],
        params: &[f32],
        transport: &Transport,
        events: &[Midi],
    ) {
        if !self.on_this_thread {
            // libpd keeps its current instance per thread: point the audio
            // thread at the one the patch was opened in.
            // SAFETY: the main instance exists (`init` ran in `load`).
            unsafe { pd::libpd_set_instance(pd::libpd_main_instance()) };
            self.on_this_thread = true;
        }
        // The tempo first: a patch may divide by it when a knob moves.
        self.send_transport(transport);
        self.send_params(params);
        // Notes land at the block's start (not sample-exact yet).
        Self::send_midi(events);
        let [il, ir] = input;
        let [ol, or] = output;
        let n = ol.len().min(self.input.len() / CHANNELS);
        if self.fifo.is_none() && n % TICK == 0 {
            for f in 0..n {
                self.input[2 * f] = il[f];
                self.input[2 * f + 1] = ir[f];
            }
            // SAFETY: both buffers hold n frames of 2 channels.
            unsafe {
                pd::libpd_process_float(
                    (n / TICK) as i32,
                    self.input.as_ptr(),
                    self.output.as_mut_ptr(),
                );
            }
            for f in 0..n {
                ol[f] = self.output[2 * f];
                or[f] = self.output[2 * f + 1];
            }
            return;
        }
        let q = self.fifo.get_or_insert_with(Fifo::new);
        let ring_frames = q.ring.len() / CHANNELS;
        for f in 0..n {
            q.tick_in[2 * q.filled] = il[f];
            q.tick_in[2 * q.filled + 1] = ir[f];
            q.filled += 1;
            if q.filled == TICK {
                // SAFETY: both hold one tick of 2 channels.
                unsafe { pd::libpd_process_float(1, q.tick_in.as_ptr(), q.tick_out.as_mut_ptr()) };
                for t in 0..TICK {
                    let w = (q.read + q.queued + t) % ring_frames;
                    q.ring[2 * w] = q.tick_out[2 * t];
                    q.ring[2 * w + 1] = q.tick_out[2 * t + 1];
                }
                q.queued += TICK;
                q.filled = 0;
            }
            ol[f] = q.ring[2 * q.read];
            or[f] = q.ring[2 * q.read + 1];
            q.ring[2 * q.read] = 0.0;
            q.ring[2 * q.read + 1] = 0.0;
            q.read = (q.read + 1) % ring_frames;
            q.queued -= 1;
        }
    }
}
