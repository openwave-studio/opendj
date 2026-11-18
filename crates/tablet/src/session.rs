//! The iPadOS audio session.
//!
//! cpal plays through the session but leaves its category alone. The
//! default category stops at the screen lock and follows the ringer
//! switch, so this sets Playback; and it asks for every output channel the
//! route has, so a controller with its own sound card (the DDJ-FLX4 has
//! four) gets the master on 1-2 and the headphones on 3-4.

/// Sets the session up, before any audio starts.
#[cfg(target_os = "ios")]
pub fn start() {
    use objc2_avf_audio::{AVAudioSession, AVAudioSessionCategoryPlayback};
    // SAFETY: the shared session is a process-wide singleton, and these
    // calls only configure it, on the main thread.
    unsafe {
        let session = AVAudioSession::sharedInstance();
        if let Some(playback) = AVAudioSessionCategoryPlayback {
            if let Err(e) = session.setCategory_error(playback) {
                eprintln!("audio session category: {}", e.localizedDescription());
            }
        }
        // About 5 ms at 48 kHz: short enough to scratch and cue by ear.
        let _ = session.setPreferredIOBufferDuration_error(0.005);
        if let Err(e) = session.setActive_error(true) {
            eprintln!("audio session: {}", e.localizedDescription());
        }
    }
}

/// The output channels the route has now, after asking for all of them.
/// Called again whenever the route may have changed (a controller plugged
/// in or out), as the preferred count only holds for the route it was set on.
#[cfg(target_os = "ios")]
pub fn channels() -> u16 {
    use objc2_avf_audio::AVAudioSession;
    // SAFETY: as in `start`.
    unsafe {
        let session = AVAudioSession::sharedInstance();
        let most = session.maximumOutputNumberOfChannels();
        if most > session.outputNumberOfChannels() {
            let _ = session.setPreferredOutputNumberOfChannels_error(most);
        }
        session.outputNumberOfChannels().clamp(0, u16::MAX as isize) as u16
    }
}

/// On the desktop the system's sound settings apply as they are.
#[cfg(not(target_os = "ios"))]
pub fn start() {}

/// On the desktop: `OPENDJ_CHANNELS` to try the four-channel routing, else
/// the master only.
#[cfg(not(target_os = "ios"))]
pub fn channels() -> u16 {
    std::env::var("OPENDJ_CHANNELS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(2)
}
