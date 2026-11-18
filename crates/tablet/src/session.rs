//! The iPadOS audio session, set up before audio starts.
//!
//! cpal plays through the session but leaves its category alone. The
//! default category stops at the screen lock and follows the ringer
//! switch, so this sets Playback; and it asks for every output channel the
//! route has, so a controller with its own sound card (the DDJ-FLX4 has
//! four) gets the master on 1-2 and the headphones on 3-4.

/// Sets the session up; returns the output channels the route now has.
#[cfg(target_os = "ios")]
pub fn start() -> u16 {
    use objc2_avf_audio::{AVAudioSession, AVAudioSessionCategoryPlayback};
    // SAFETY: the shared session is a process-wide singleton, and these
    // calls only configure it; they are made on the main thread before
    // any audio starts.
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
        // Only once active does the session know the route's channels.
        let most = session.maximumOutputNumberOfChannels();
        if most > 2 {
            let _ = session.setPreferredOutputNumberOfChannels_error(most);
        }
        session.outputNumberOfChannels().clamp(0, u16::MAX as isize) as u16
    }
}

/// On the desktop the system's sound settings apply as they are.
#[cfg(not(target_os = "ios"))]
pub fn start() -> u16 {
    2
}
