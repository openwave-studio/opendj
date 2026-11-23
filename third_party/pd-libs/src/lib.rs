//! The cyclone and ELSE Pure Data libraries, compiled into the program that
//! links this crate (openDJ's plugin helper). Call [`setup`] once, after
//! `libpd_init`, on the thread that runs Pd.

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
