//! The cyclone, ELSE and pd-lua Pure Data libraries, compiled into the program that
//! links this crate (openDJ's plugin helper). Call [`setup`] once, after
//! `libpd_init`, on the thread that runs Pd. pd-lua comes with them
//! ([`setup_lua`]).

use std::ffi::{c_char, c_int, c_void};

extern "C" {
    fn opendj_pd_libs_setup();
}

/// Registers every cyclone and ELSE object with libpd's current instance.
///
/// # Safety
/// libpd must be initialised and its instance current on this thread.
pub unsafe fn setup() {
    // SAFETY: as the caller promised.
    unsafe { opendj_pd_libs_setup() }
}

/// One drawing call from a Lua object (see `pdlua_glue.c`).
pub type DrawFn = unsafe extern "C" fn(
    obj: *mut c_void,
    layer: c_int,
    what: *const c_char,
    nums: *const f64,
    n: c_int,
    text: *const c_char,
);

/// Called for each Lua object that draws: (ctx, object, x, y, width, height).
pub type FoundFn = unsafe extern "C" fn(*mut c_void, *mut c_void, c_int, c_int, c_int, c_int);

extern "C" {
    fn opendj_pdlua_setup(datadir: *const c_char, draw: DrawFn);
    fn opendj_pdlua_displays(canvas: *mut c_void, ctx: *mut c_void, found: FoundFn);
    fn opendj_pdlua_repaint(obj: *mut c_void);
    fn opendj_pdlua_mouse(obj: *mut c_void, x: c_int, y: c_int, kind: c_int);
    fn opendj_pd_object_texts(
        canvas: *mut c_void,
        ctx: *mut c_void,
        each: unsafe extern "C" fn(*mut c_void, c_int, *const c_char, c_int),
    );
}

/// Each object box on `canvas` (a patch libpd opened): its index among
/// everything on the canvas, in patch-file order, and its text as Pd would
/// save it ("class args"), which a Lua object's `self:set_args` changes.
///
/// # Safety
///
/// `canvas` is a patch libpd opened and hasn't closed, and Pd is used by no
/// other thread meanwhile.
pub unsafe fn object_texts(canvas: *mut c_void) -> Vec<(usize, String)> {
    unsafe extern "C" fn each(ctx: *mut c_void, index: c_int, text: *const c_char, len: c_int) {
        // SAFETY: `ctx` is the Vec below; `text` holds `len` bytes.
        let out = unsafe { &mut *(ctx as *mut Vec<(usize, String)>) };
        let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, len.max(0) as usize) };
        let text = String::from_utf8_lossy(bytes)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        out.push((index.max(0) as usize, text));
    }
    let mut out: Vec<(usize, String)> = Vec::new();
    // SAFETY: as the caller promised; `each` only runs during the call.
    unsafe { opendj_pd_object_texts(canvas, &mut out as *mut _ as *mut c_void, each) };
    out
}

/// pd-lua's own Lua code, which it reads from `<datadir>/pdlua/pd.lua`.
pub const PD_LUA: &str = include_str!("../pdlua/pd.lua");

/// pd-lua's `pdx` module, which `pd.lua` loads from beside itself.
pub const PDX_LUA: &str = include_str!("../pdlua/pdx.lua");

/// Starts pd-lua, so patches can use `.pd_lua` objects. `datadir` holds
/// `pdlua/pd.lua` ([`PD_LUA`]) and `pdlua/pdx.lua` ([`PDX_LUA`]); `draw` gets what Lua objects draw.
///
/// # Safety
/// As [`setup`]; `datadir` is NUL-terminated.
pub unsafe fn setup_lua(datadir: *const c_char, draw: DrawFn) {
    // SAFETY: as the caller promised.
    unsafe { opendj_pdlua_setup(datadir, draw) }
}

/// Finds the Lua objects that draw on `canvas` (a patch libpd opened),
/// routing their drawing to the `draw` given to [`setup_lua`].
///
/// # Safety
/// `canvas` is a live canvas of the current instance; Pd is not running on
/// another thread meanwhile.
pub unsafe fn lua_displays(canvas: *mut c_void, ctx: *mut c_void, found: FoundFn) {
    // SAFETY: as the caller promised.
    unsafe { opendj_pdlua_displays(canvas, ctx, found) }
}

/// Asks a Lua object to paint again.
///
/// # Safety
/// `obj` came from [`lua_displays`] and lives; Pd is not running elsewhere.
pub unsafe fn lua_repaint(obj: *mut c_void) {
    // SAFETY: as the caller promised.
    unsafe { opendj_pdlua_repaint(obj) }
}

/// A mouse event for a Lua object, in its own coordinates. `kind`: 0 down,
/// 1 up, 2 move, 3 drag.
///
/// # Safety
/// As [`lua_repaint`].
pub unsafe fn lua_mouse(obj: *mut c_void, x: i32, y: i32, kind: i32) {
    // SAFETY: as the caller promised.
    unsafe { opendj_pdlua_mouse(obj, x, y, kind) }
}
