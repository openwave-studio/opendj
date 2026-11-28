//! Page faults and audio dropouts between two frame log lines, to tell
//! whether the OS paging memory back in (from its compressor or from swap)
//! lines up with audio dropouts or UI stalls.

use std::sync::Arc;

use opendj_control::{ControlHandle, ControlRegistry};

/// The process's page faults so far: all of them, and those read from disk
/// (swap or a mapped file) where the OS tells them apart.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Faults {
    pub all: u64,
    pub from_disk: Option<u64>,
}

/// The process's page faults so far, or `None` if the OS won't say.
pub fn faults() -> Option<Faults> {
    #[cfg(unix)]
    {
        // SAFETY: `rusage` is plain old data, so all zeros is a valid value;
        // getrusage only writes into the struct we pass, a live local.
        let mut u: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) } != 0 {
            return None;
        }
        let (minor, major) = (u.ru_minflt.max(0) as u64, u.ru_majflt.max(0) as u64);
        Some(Faults {
            all: minor + major,
            from_disk: Some(major),
        })
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        // SAFETY: the counters are plain old data; the call fills the struct
        // we pass, whose size we give.
        let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, size) } == 0 {
            return None;
        }
        Some(Faults {
            all: c.PageFaultCount as u64,
            from_disk: None,
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Counts since the last report.
pub struct Paging {
    last: Option<Faults>,
    reg: Arc<ControlRegistry>,
    /// Looked up on first use: the audio outputs make it when they open.
    dropouts: Option<ControlHandle>,
    last_dropouts: f64,
}

impl Paging {
    pub fn new(reg: &Arc<ControlRegistry>) -> Self {
        let mut p = Self {
            last: faults(),
            reg: reg.clone(),
            dropouts: None,
            last_dropouts: 0.0,
        };
        p.last_dropouts = p.dropouts();
        p
    }

    fn dropouts(&mut self) -> f64 {
        if self.dropouts.is_none() {
            self.dropouts = self.reg.get("[App]", "audio_latency_overload_count");
        }
        self.dropouts.as_ref().map_or(0.0, |d| d.get())
    }

    /// "page faults +N (M from disk), audio dropouts +K" since the last call.
    pub fn report(&mut self) -> String {
        let now = faults();
        let faults = describe(self.last, now);
        self.last = now.or(self.last);
        let dropouts = self.dropouts();
        let new = (dropouts - self.last_dropouts).max(0.0) as u64;
        self.last_dropouts = dropouts;
        format!("{faults}, audio dropouts +{new}")
    }
}

fn describe(before: Option<Faults>, now: Option<Faults>) -> String {
    let (Some(b), Some(n)) = (before, now) else {
        return "page faults unknown".to_string();
    };
    let all = n.all.saturating_sub(b.all);
    match (b.from_disk, n.from_disk) {
        (Some(b), Some(n)) => format!("page faults +{all} ({} from disk)", n.saturating_sub(b)),
        _ => format!("page faults +{all}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_has_page_faults_on_this_os() {
        let f = faults().expect("page fault counts");
        assert!(f.all > 0);
    }

    #[test]
    fn reports_the_faults_since_last_time() {
        let at = |all, disk| {
            Some(Faults {
                all,
                from_disk: disk,
            })
        };
        assert_eq!(
            describe(at(10, Some(1)), at(25, Some(3))),
            "page faults +15 (2 from disk)"
        );
        assert_eq!(describe(at(10, None), at(12, None)), "page faults +2");
        assert_eq!(describe(None, at(12, None)), "page faults unknown");
    }

    #[test]
    fn counts_dropouts_since_last_time() {
        let reg = ControlRegistry::new();
        let d = reg.indicator("[App]", "audio_latency_overload_count", f64::MAX);
        d.set(2.0);
        let mut p = Paging::new(&reg);
        d.set(5.0);
        let first = p.report();
        assert!(first.ends_with("audio dropouts +3"), "{first}");
        assert!(p.report().ends_with("audio dropouts +0"));
    }
}
