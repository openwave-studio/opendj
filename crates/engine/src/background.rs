//! Work done behind the DJ's back, such as analysing a batch of tracks, runs
//! at low priority and on few cores, so the audio and the UI always come
//! first. While a deck plays it goes gentler still: see [`playing`] and
//! [`WhilePlaying`]. Each kind of [`Task`] can be paused by the DJ; work
//! waits at its pause points with [`wait_while_held`].

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Once;
use std::time::Duration;

static PLAYING: AtomicBool = AtomicBool::new(false);
/// One bit per [`Task`], set while the DJ has it paused.
static PAUSED: AtomicU8 = AtomicU8::new(0);
static WHILE_PLAYING: AtomicU8 = AtomicU8::new(WhilePlaying::SlowDown as u8);

/// The kinds of background work the Background panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Task {
    /// Tempo, grid, key and phrases.
    Analyze = 0,
    /// High-quality stem separation.
    Stems = 1,
    /// Whisper word timing and transcribing.
    Lyrics = 2,
    /// Collection Health's audio fingerprints.
    SoundCheck = 3,
    /// Collection Health's cover and bitrate scan.
    Covers = 4,
}

impl Task {
    pub const ALL: [Task; 5] = [
        Task::Analyze,
        Task::Stems,
        Task::Lyrics,
        Task::SoundCheck,
        Task::Covers,
    ];

    pub fn from_index(i: usize) -> Option<Task> {
        Self::ALL.get(i).copied()
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// What background work does while a deck plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhilePlaying {
    /// One core, resting between tracks (the default).
    SlowDown = 0,
    /// Waits until the decks stop.
    Pause = 1,
    /// Carries on as when nothing plays.
    KeepGoing = 2,
}

impl WhilePlaying {
    pub fn from_index(i: u8) -> WhilePlaying {
        match i {
            1 => WhilePlaying::Pause,
            2 => WhilePlaying::KeepGoing,
            _ => WhilePlaying::SlowDown,
        }
    }
}

/// Pauses or resumes one kind of task.
pub fn set_paused(task: Task, on: bool) {
    if on {
        PAUSED.fetch_or(task.bit(), Ordering::Relaxed);
    } else {
        PAUSED.fetch_and(!task.bit(), Ordering::Relaxed);
    }
}

/// Whether the DJ has paused this kind of task.
pub fn paused(task: Task) -> bool {
    PAUSED.load(Ordering::Relaxed) & task.bit() != 0
}

/// The paused tasks as bits, for keeping in settings.
pub fn paused_bits() -> u8 {
    PAUSED.load(Ordering::Relaxed)
}

pub fn set_paused_bits(bits: u8) {
    PAUSED.store(bits, Ordering::Relaxed);
}

pub fn set_while_playing(w: WhilePlaying) {
    WHILE_PLAYING.store(w as u8, Ordering::Relaxed);
}

pub fn while_playing() -> WhilePlaying {
    WhilePlaying::from_index(WHILE_PLAYING.load(Ordering::Relaxed))
}

/// Whether background work should go gently now: a deck plays and the DJ
/// hasn't chosen Keep going.
pub fn slow_down() -> bool {
    playing() && while_playing() != WhilePlaying::KeepGoing
}

/// Whether a task should wait at its next pause point: paused by the DJ,
/// or a deck plays with While a deck plays set to Pause.
pub fn held(task: Task) -> bool {
    paused(task) || (playing() && while_playing() == WhilePlaying::Pause)
}

/// Waits at a pause point while `task` is [`held`], or until `keep_going`
/// says to give up (a stop). Returns whether it waited.
pub fn wait_while_held(task: Task, keep_going: impl Fn() -> bool) -> bool {
    let mut waited = false;
    while held(task) && keep_going() {
        waited = true;
        std::thread::sleep(Duration::from_millis(100));
    }
    waited
}

thread_local! {
    /// Whether this thread runs at low priority (see [`lower_priority`]).
    static LOW: Cell<bool> = const { Cell::new(false) };
}

/// How many threads Beat This! (on rten's thread pool) runs on: a quarter
/// of the cores, between 2 and 4, rather than every fast core.
pub fn beat_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |n| n.get() / 4)
        .clamp(2, 4)
}

/// Sizes rten's thread pool (Beat This! runs on it) with [`beat_threads`],
/// unless `RTEN_NUM_THREADS` is set already. rten reads the variable once,
/// when the pool is first used, so this is called first thing in `main`,
/// before any other thread starts.
pub fn configure() {
    if std::env::var_os("RTEN_NUM_THREADS").is_none() {
        std::env::set_var("RTEN_NUM_THREADS", beat_threads().to_string());
    }
}

/// Runs the calling thread at low priority: the utility QoS class on macOS
/// (it may move to the efficiency cores), nice 10 on Linux, below normal on
/// Windows. Threads it starts afterwards should call it too (see
/// [`is_low`]): macOS doesn't pass the class on to them.
pub fn lower_priority() {
    LOW.with(|l| l.set(true));
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    // SAFETY: only changes the calling thread's scheduling.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
    #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
    // SAFETY: as above; on Linux the nice value belongs to each thread.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 10);
    }
    #[cfg(windows)]
    // SAFETY: as above.
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

/// Whether the calling thread was put at low priority with
/// [`lower_priority`].
pub fn is_low() -> bool {
    LOW.with(Cell::get)
}

/// Puts each of rten's threads (Beat This! runs on them) at low priority,
/// once. Called from a low-priority thread: Beat This! on a deck's track
/// then waits a little more too, which the DSP grid covers meanwhile.
pub fn lower_rten_priority() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        rten::thread_pool().run(|| {
            rayon::broadcast(|_| lower_priority());
        });
    });
}

/// Tells background work whether a deck is playing; the UI calls it as
/// the decks start and stop.
pub fn set_playing(on: bool) {
    PLAYING.store(on, Ordering::Relaxed);
}

/// Whether a deck is playing: background work then takes one core and
/// rests between tracks.
pub fn playing() -> bool {
    PLAYING.load(Ordering::Relaxed)
}

/// How many threads background work may share a step out to, from `most`:
/// one while a deck plays (unless the DJ chose Keep going).
pub fn threads(most: usize) -> usize {
    if is_low() && slow_down() {
        1
    } else {
        most.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_priority_is_per_thread() {
        std::thread::spawn(|| {
            lower_priority();
            assert!(is_low());
            assert_eq!(threads(4), 4);
        })
        .join()
        .unwrap();
        assert!(!is_low());
    }

    #[test]
    fn tasks_pause_on_their_own() {
        set_paused(Task::Stems, true);
        assert!(paused(Task::Stems));
        assert!(!paused(Task::Analyze));
        assert!(held(Task::Stems));
        // A stop ends the wait at once.
        assert!(!wait_while_held(Task::Stems, || false));
        set_paused(Task::Stems, false);
        assert!(!held(Task::Stems));
        assert_eq!(WhilePlaying::from_index(2), WhilePlaying::KeepGoing);
        assert_eq!(WhilePlaying::from_index(9), WhilePlaying::SlowDown);
    }

    #[test]
    fn beat_this_takes_a_few_cores() {
        assert!((2..=4).contains(&beat_threads()));
    }
}
