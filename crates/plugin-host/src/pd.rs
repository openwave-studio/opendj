//! Pure Data patches as effects and instruments (experimental, feature
//! `pd`): the helper runs the patch in libpd, Pd's engine as a library.
//! How a patch talks to openDJ is in [`crate::pd_patch`].
//!
//! pd-lua is built in too: a patch's `.pd_lua` objects run, and those that
//! draw (a `paint` method) show in the helper's editor window, drawn by
//! [`crate::pd_display`], with the mouse going back to them.
//!
//! One patch per helper process, like every other plugin: a patch that
//! crashes or hangs costs its own slot.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once};
use std::time::Duration;

use libpd_sys as pd;

use crate::client::Transport;
use crate::effects::{arrange, DrawnEditor, Effect, Info, Loaded, MainThread, Mouse, Param};
use crate::pd_display::{self, displays, Display};
use crate::pd_patch::{param_id, read_patch, BEAT, BPM, PLAYING};
use crate::shm::{Midi, CHANNELS};

/// Frames Pd processes at a time (one "tick").
const TICK: usize = 64;

static INIT: Once = Once::new();

/// Pd runs on the audio thread, and on the main thread while the mouse
/// works a Lua display: one at a time.
static PD: Mutex<()> = Mutex::new(());

fn pd_lock() -> MutexGuard<'static, ()> {
    PD.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "C" fn print_line(line: *const c_char) {
    if !line.is_null() {
        // SAFETY: libpd hands a NUL-terminated line.
        let text = unsafe { CStr::from_ptr(line) }.to_string_lossy();
        eprintln!("pd: {}", text.trim_end());
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
            // cyclone and ELSE, built in.
            opendj_pd_libs::setup();
            // pd-lua, which reads its own Lua half from a file.
            match lua_datadir().and_then(|d| c(&d.to_string_lossy())) {
                Ok(dir) => opendj_pd_libs::setup_lua(dir.as_ptr(), pd_display::draw_call),
                Err(e) => eprintln!("pd: no Lua objects: {e}"),
            }
        }
    });
}

/// A folder holding `pdlua/pd.lua` and `pdx.lua`, for pd-lua to read at
/// start (shared by helpers; each file is replaced whole when it differs).
fn lua_datadir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("opendj-pdlua-0.12.23");
    let sub = dir.join("pdlua");
    std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
    for (name, text) in [
        ("pd.lua", opendj_pd_libs::PD_LUA),
        ("pdx.lua", opendj_pd_libs::PDX_LUA),
    ] {
        let file = sub.join(name);
        if std::fs::read_to_string(&file).ok().as_deref() != Some(text) {
            let part = sub.join(format!("{name}.{}", std::process::id()));
            std::fs::write(&part, text).map_err(|e| e.to_string())?;
            std::fs::rename(&part, &file).map_err(|e| e.to_string())?;
        }
    }
    Ok(dir)
}

/// Adds a Lua display pd-lua found to [`displays`].
unsafe extern "C" fn found_display(
    _ctx: *mut c_void,
    obj: *mut c_void,
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
) {
    displays().list.push(Display::new(
        obj as usize,
        x as f32,
        y as f32,
        w as f32,
        h as f32,
    ));
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
    let _pd = pd_lock();
    let canvas;
    unsafe {
        if pd::libpd_init_audio(CHANNELS as i32, CHANNELS as i32, sample_rate as i32) != 0 {
            return Err("Pd couldn't start its audio".into());
        }
        pd::libpd_add_to_search_path(dir_c.as_ptr());
        canvas = pd::libpd_openfile(base_c.as_ptr(), dir_c.as_ptr());
        if canvas.is_null() {
            return Err(format!("Pd couldn't open {}", file.display()));
        }
        // DSP on: [; pd dsp 1(
        pd::libpd_start_message(1);
        pd::libpd_add_float(1.0);
        let (pd_c, dsp_c) = (c("pd")?, c("dsp")?);
        pd::libpd_finish_message(pd_c.as_ptr(), dsp_c.as_ptr());
    }
    // The Lua objects that draw: route their drawing here, then have them
    // paint once (their first paint, at load, went nowhere).
    // SAFETY: the patch just opened, on this thread, with Pd locked.
    let objs: Vec<usize> = unsafe {
        opendj_pd_libs::lua_displays(canvas, std::ptr::null_mut(), found_display);
        let objs: Vec<usize> = displays().list.iter().map(|d| d.obj).collect();
        for &o in &objs {
            opendj_pd_libs::lua_repaint(o as *mut c_void);
        }
        objs
    };
    drop(_pd);
    let main: Option<Box<dyn MainThread>> = if objs.is_empty() {
        None
    } else {
        Some(Box::new(PdMain::default()))
    };
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
        main,
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
        let _pd = pd_lock();
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

/// The patch's main-thread side: its Lua displays, as the editor window.
#[derive(Default)]
struct PdMain {
    /// The [`pd_display::Displays::version`] last painted.
    painted: Option<u64>,
    /// The display a press started on, which gets the drag and the release.
    pressed: Option<usize>,
}

impl PdMain {
    /// Sends a mouse event to a Lua object. `kind`: 0 down, 1 up, 2 move,
    /// 3 drag.
    fn send(obj: usize, x: f32, y: f32, kind: i32) {
        let _pd = pd_lock();
        // SAFETY: the object is one of the patch's (patches never close in
        // a helper), and Pd is locked to this thread.
        unsafe {
            pd::libpd_set_instance(pd::libpd_main_instance());
            opendj_pd_libs::lua_mouse(obj as *mut c_void, x.round() as i32, y.round() as i32, kind);
        }
    }
}

impl MainThread for PdMain {
    fn idle(&mut self) {}

    fn next_idle(&self) -> Option<Duration> {
        // A display may repaint from the audio thread at any time.
        Some(Duration::from_millis(16))
    }

    fn drawn_editor(&mut self) -> Option<&mut dyn DrawnEditor> {
        Some(self)
    }
}

impl DrawnEditor for PdMain {
    fn size(&mut self) -> (f32, f32) {
        pd_display::bounds(&displays().list).map_or((200.0, 100.0), |b| (b.2, b.3))
    }

    fn changed(&mut self) -> bool {
        self.painted != Some(displays().version)
    }

    fn paint(&mut self, pixels: &mut [u32], width: u32, height: u32, scale: f32) {
        let Some(mut pixmap) = tiny_skia::Pixmap::new(width, height) else {
            return;
        };
        // A copy, so Pd (which adds to the list as it paints) never waits
        // on the drawing.
        let list = {
            let all = displays();
            self.painted = Some(all.version);
            all.list.clone()
        };
        pd_display::paint(&list, &mut pixmap, scale);
        pd_display::to_window(&pixmap, pixels);
    }

    fn mouse(&mut self, x: f32, y: f32, what: Mouse) {
        let target = {
            let all = displays();
            match (what, self.pressed) {
                (Mouse::Down, _) => pd_display::hit(&all.list, x, y).map(|(o, x, y)| (o, x, y, 0)),
                (Mouse::Move, Some(o)) => {
                    pd_display::local(&all.list, o, x, y).map(|(x, y)| (o, x, y, 3))
                }
                (Mouse::Up, Some(o)) => {
                    pd_display::local(&all.list, o, x, y).map(|(x, y)| (o, x, y, 1))
                }
                (Mouse::Move, None) => {
                    pd_display::hit(&all.list, x, y).map(|(o, x, y)| (o, x, y, 2))
                }
                (Mouse::Up, None) => None,
            }
        };
        match what {
            Mouse::Down => self.pressed = target.map(|t| t.0),
            Mouse::Up => self.pressed = None,
            Mouse::Move => {}
        }
        if let Some((obj, x, y, kind)) = target {
            Self::send(obj, x, y, kind);
        }
    }
}
