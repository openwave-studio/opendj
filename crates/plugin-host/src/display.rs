//! A plugin display openDJ shows on the deck: a second shared file, next to
//! the helper's audio block (`<block>.display`), holding a picture the
//! helper draws and the mouse events openDJ sends back.
//!
//! Only helpers that draw their own display (a Pd patch's Lua displays)
//! make the file. Who writes what:
//!
//! - The helper creates it, writes the display's size in points
//!   (`content_w`, `content_h`), and sets `magic` last.
//! - openDJ writes the size in pixels it shows the display at (`view_w`,
//!   `view_h`; 0 while hidden). The helper draws at that size.
//! - The helper makes `frame_seq` odd while it writes the pixels, even
//!   once they are whole: openDJ copies them only while it stays the same
//!   even number.
//! - openDJ queues mouse events, in points, in `mouse` and bumps
//!   `mouse_in`; the helper takes them and moves `mouse_out` up.

use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use memmap2::MmapMut;

/// "openDJ D" in ASCII.
pub const MAGIC: u64 = 0x6f70_656e_444a_2044;
/// The largest picture, in pixels.
pub const MAX_W: u32 = 2048;
pub const MAX_H: u32 = 1200;
/// Mouse events that can wait.
const QUEUE: usize = 64;

#[repr(C)]
struct Header {
    magic: AtomicU64,
    content_w: AtomicU32,
    content_h: AtomicU32,
    view_w: AtomicU32,
    view_h: AtomicU32,
    /// The size the pixels were drawn at.
    drawn_w: AtomicU32,
    drawn_h: AtomicU32,
    frame_seq: AtomicU32,
    mouse_in: AtomicU32,
    mouse_out: AtomicU32,
    _pad: AtomicU32,
    mouse: [AtomicU64; QUEUE],
}

pub use crate::effects::Mouse;

/// The helper's own display file, which the helper binary sets from its
/// audio block's path before loading the plugin.
static HELPER_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The helper: where its display file goes (see [`path_for`]).
pub fn set_helper_path(path: PathBuf) {
    let _ = HELPER_PATH.set(path);
}

pub fn helper_path() -> Option<&'static Path> {
    HELPER_PATH.get().map(PathBuf::as_path)
}

/// The display file of the helper whose audio block is at `shm`.
pub fn path_for(shm: &Path) -> PathBuf {
    let mut name = shm.as_os_str().to_owned();
    name.push(".display");
    PathBuf::from(name)
}

fn size() -> usize {
    std::mem::size_of::<Header>() + (MAX_W * MAX_H * 4) as usize
}

/// One display file, from either side.
pub struct Display {
    map: MmapMut,
}

// Plain memory; the protocol above says who writes what.
unsafe impl Send for Display {}
unsafe impl Sync for Display {}

/// A mouse event packed into 64 bits: kind, then x and y in 1/16 points.
fn pack(x: f32, y: f32, what: Mouse) -> u64 {
    let kind = match what {
        Mouse::Down => 0u64,
        Mouse::Up => 1,
        Mouse::Move => 2,
    };
    let fixed = |v: f32| {
        ((v * 16.0).clamp(-(1 << 29) as f32, (1 << 29) as f32) as i32 as u32 & 0x3fff_ffff) as u64
    };
    kind << 60 | fixed(x) << 30 | fixed(y)
}

fn unpack(v: u64) -> (f32, f32, Mouse) {
    let what = match v >> 60 {
        0 => Mouse::Down,
        1 => Mouse::Up,
        _ => Mouse::Move,
    };
    // Back from 30-bit two's complement.
    let signed = |b: u64| (((b as u32) << 2) as i32 >> 2) as f32 / 16.0;
    (signed(v >> 30 & 0x3fff_ffff), signed(v & 0x3fff_ffff), what)
}

impl Display {
    /// Makes the file (the helper's side), for a display `w` by `h` points.
    pub fn create(path: &Path, w: f32, h: f32) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(size() as u64)?;
        // SAFETY: our own new file, which nothing resizes.
        let map = unsafe { MmapMut::map_mut(&file)? };
        let d = Self { map };
        let h_ = d.header();
        h_.content_w.store(w.ceil() as u32, Ordering::Relaxed);
        h_.content_h.store(h.ceil() as u32, Ordering::Relaxed);
        h_.magic.store(MAGIC, Ordering::Release);
        Ok(d)
    }

    /// Maps a file a helper made (openDJ's side); `None` while there is
    /// none (yet).
    pub fn open(path: &Path) -> Option<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path).ok()?;
        if (file.metadata().ok()?.len() as usize) < size() {
            return None;
        }
        // SAFETY: the helper's file, mapped whole; nothing resizes it.
        let map = unsafe { MmapMut::map_mut(&file).ok()? };
        let d = Self { map };
        (d.header().magic.load(Ordering::Acquire) == MAGIC).then_some(d)
    }

    fn header(&self) -> &Header {
        // SAFETY: page-aligned, at least a Header long, all atomics.
        unsafe { &*(self.map.as_ptr() as *const Header) }
    }

    fn pixels(&self) -> *mut u8 {
        // SAFETY: the pixels follow the header, inside the mapping.
        unsafe { (self.map.as_ptr() as *mut u8).add(std::mem::size_of::<Header>()) }
    }

    /// The display's size, in points.
    pub fn content(&self) -> (u32, u32) {
        let h = self.header();
        (
            h.content_w.load(Ordering::Relaxed),
            h.content_h.load(Ordering::Relaxed),
        )
    }

    pub fn set_content(&self, w: f32, h: f32) {
        let hd = self.header();
        hd.content_w.store(w.ceil() as u32, Ordering::Relaxed);
        hd.content_h.store(h.ceil() as u32, Ordering::Relaxed);
    }

    /// openDJ: shows the display at `w` by `h` pixels (0 hides it).
    pub fn set_view(&self, w: u32, h: u32) {
        let hd = self.header();
        hd.view_w.store(w.min(MAX_W), Ordering::Relaxed);
        hd.view_h.store(h.min(MAX_H), Ordering::Relaxed);
    }

    /// The helper: the size openDJ shows it at, in pixels.
    pub fn view(&self) -> (u32, u32) {
        let h = self.header();
        (
            h.view_w.load(Ordering::Relaxed),
            h.view_h.load(Ordering::Relaxed),
        )
    }

    /// The helper: writes a picture `w` by `h` pixels, RGBA.
    pub fn write_frame(&self, w: u32, h: u32, rgba: &[u8]) {
        let (w, h) = (w.min(MAX_W), h.min(MAX_H));
        let n = (w * h * 4) as usize;
        if rgba.len() < n {
            return;
        }
        let hd = self.header();
        let seq = hd.frame_seq.load(Ordering::Relaxed);
        hd.frame_seq.store(seq | 1, Ordering::Release);
        std::sync::atomic::fence(Ordering::Release);
        // SAFETY: n bytes fit (w, h within the maximum).
        unsafe { std::ptr::copy_nonoverlapping(rgba.as_ptr(), self.pixels(), n) };
        hd.drawn_w.store(w, Ordering::Relaxed);
        hd.drawn_h.store(h, Ordering::Relaxed);
        hd.frame_seq.store((seq | 1) + 1, Ordering::Release);
    }

    /// The number of the last whole picture (changes with each new one).
    pub fn frame_seq(&self) -> u32 {
        self.header().frame_seq.load(Ordering::Acquire)
    }

    /// openDJ: copies the last whole picture into `out` (RGBA), returning its
    /// size and number; `None` while the helper is writing one or has none.
    pub fn read_frame(&self, out: &mut Vec<u8>) -> Option<(u32, u32, u32)> {
        let hd = self.header();
        let seq = hd.frame_seq.load(Ordering::Acquire);
        if seq == 0 || seq & 1 == 1 {
            return None;
        }
        let (w, h) = (
            hd.drawn_w.load(Ordering::Relaxed),
            hd.drawn_h.load(Ordering::Relaxed),
        );
        let n = (w.min(MAX_W) * h.min(MAX_H) * 4) as usize;
        out.resize(n, 0);
        // SAFETY: n bytes are inside the mapping.
        unsafe { std::ptr::copy_nonoverlapping(self.pixels(), out.as_mut_ptr(), n) };
        std::sync::atomic::fence(Ordering::Acquire);
        (hd.frame_seq.load(Ordering::Relaxed) == seq).then_some((w, h, seq))
    }

    /// openDJ: a mouse event at (x, y) points on the display. Dropped when
    /// the queue is full.
    pub fn send_mouse(&self, x: f32, y: f32, what: Mouse) {
        let hd = self.header();
        let i = hd.mouse_in.load(Ordering::Relaxed);
        if i.wrapping_sub(hd.mouse_out.load(Ordering::Acquire)) as usize >= QUEUE {
            return;
        }
        hd.mouse[i as usize % QUEUE].store(pack(x, y, what), Ordering::Relaxed);
        hd.mouse_in.store(i.wrapping_add(1), Ordering::Release);
    }

    /// The helper: the mouse events openDJ sent since the last call.
    pub fn take_mouse(&self, mut each: impl FnMut(f32, f32, Mouse)) {
        let hd = self.header();
        let end = hd.mouse_in.load(Ordering::Acquire);
        let mut i = hd.mouse_out.load(Ordering::Relaxed);
        while i != end {
            let (x, y, what) = unpack(hd.mouse[i as usize % QUEUE].load(Ordering::Relaxed));
            each(x, y, what);
            i = i.wrapping_add(1);
        }
        hd.mouse_out.store(end, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_events_keep_their_place() {
        for (x, y, m) in [
            (0.0, 0.0, Mouse::Down),
            (812.5, 33.25, Mouse::Move),
            (-4.0, 1000.0, Mouse::Up),
        ] {
            assert_eq!(unpack(pack(x, y, m)), (x, y, m));
        }
    }

    #[test]
    fn a_frame_and_the_mouse_cross_over() {
        let path = std::env::temp_dir().join(format!("opendj-display-test-{}", std::process::id()));
        let helper = Display::create(&path, 200.0, 100.0).unwrap();
        let app = Display::open(&path).expect("made");
        assert_eq!(app.content(), (200, 100));
        let mut out = Vec::new();
        assert!(app.read_frame(&mut out).is_none(), "nothing drawn yet");
        app.set_view(4, 2);
        assert_eq!(helper.view(), (4, 2));
        helper.write_frame(4, 2, &[7; 32]);
        let (w, h, _) = app.read_frame(&mut out).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(out, [7; 32]);
        app.send_mouse(10.0, 20.0, Mouse::Down);
        app.send_mouse(11.0, 20.0, Mouse::Up);
        let mut got = Vec::new();
        helper.take_mouse(|x, y, m| got.push((x, y, m)));
        assert_eq!(got, [(10.0, 20.0, Mouse::Down), (11.0, 20.0, Mouse::Up)]);
        helper.take_mouse(|_, _, _| panic!("taken already"));
        let _ = std::fs::remove_file(&path);
    }
}
