//! One openDJ at a time. A second one would fight the first for the audio
//! interface and the controllers: an ASIO driver serves one program, and a
//! controller's lights and motor answer whoever wrote last.
//!
//! Starting openDJ again while it runs brings the running window forward.
//! One still quitting is waited for. One left running without a window
//! (stuck letting go of a driver) is named, with the choice to end it.
//!
//! Windows only for now: macOS opens the running app instead of a second
//! copy by itself.

/// Whether this process should go on starting. Call it first, before
/// anything opens the log, a device or the library.
#[cfg(windows)]
pub fn check() -> bool {
    windows::check()
}

#[cfg(not(windows))]
pub fn check() -> bool {
    true
}

#[cfg(windows)]
mod windows {
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, INVALID_HANDLE_VALUE,
        LPARAM, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        CreateMutexW, GetCurrentProcessId, OpenProcess, TerminateProcess, WaitForSingleObject,
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        MessageBoxW, SetForegroundWindow, ShowWindow, IDYES, MB_DEFBUTTON2, MB_ICONWARNING,
        MB_SETFOREGROUND, MB_YESNO, SW_RESTORE,
    };

    /// Held by every running openDJ in this Windows session.
    const MUTEX: &str = "Local\\openDJ.single-instance";
    /// How long one without a window gets to show one, or to finish
    /// quitting (its own quit gives up after 10 s), before it counts as
    /// stuck.
    const WAIT: Duration = Duration::from_secs(12);
    /// How long ending a stuck one may take.
    const END_WAIT_MS: u32 = 5000;

    pub fn check() -> bool {
        if !another_holds_mutex() {
            return true;
        }
        let deadline = Instant::now() + WAIT;
        loop {
            let others = other_instances();
            // The mutex outlived its process for a moment, or the other
            // one finished quitting while this one waited.
            if others.is_empty() {
                return true;
            }
            if let Some(window) = others.iter().find_map(|&pid| visible_window(pid)) {
                bring_forward(window);
                return false;
            }
            if Instant::now() >= deadline {
                return ask_to_end(&others);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Opens the session's openDJ mutex, which stays open for the life of
    /// this process; whether another process had it open already.
    fn another_holds_mutex() -> bool {
        let name = wide(MUTEX);
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives
        // the call; null security attributes mean defaults. The handle is
        // never closed: closing it would let a later start think this one
        // isn't running. It closes with the process.
        unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        // SAFETY: reads this thread's last error, set by the call above.
        unsafe { GetLastError() == ERROR_ALREADY_EXISTS }
    }

    /// Other processes running this program (by file name, so a build
    /// elsewhere counts too: it would hold the same devices).
    fn other_instances() -> Vec<u32> {
        let Some(name) = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        else {
            return Vec::new();
        };
        // SAFETY: a plain system call.
        let me = unsafe { GetCurrentProcessId() };
        let mut found = Vec::new();
        // SAFETY: a snapshot of all processes, closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return found;
        }
        // SAFETY: PROCESSENTRY32W is plain data; all zeroes is valid.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        // SAFETY: `entry` is sized as the API requires, and the snapshot
        // is open.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
            if exe == name && entry.th32ProcessID != me {
                found.push(entry.th32ProcessID);
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        close(snapshot);
        found
    }

    /// A shown, titled top-level window of process `pid`: the main window,
    /// or the mini player while the main window is hidden.
    fn visible_window(pid: u32) -> Option<HWND> {
        struct Search {
            pid: u32,
            found: Option<HWND>,
        }
        unsafe extern "system" fn each(window: HWND, data: LPARAM) -> windows_sys::core::BOOL {
            // SAFETY: `data` is the `Search` passed to EnumWindows below,
            // alive for the whole enumeration.
            let search = unsafe { &mut *(data as *mut Search) };
            let mut pid = 0;
            // SAFETY: `window` comes from EnumWindows; `pid` is writable.
            unsafe { GetWindowThreadProcessId(window, &mut pid) };
            // SAFETY: plain queries on a window handle from EnumWindows.
            let shown = unsafe { IsWindowVisible(window) != 0 && GetWindowTextLengthW(window) > 0 };
            if pid == search.pid && shown {
                search.found = Some(window);
                return 0;
            }
            1
        }
        let mut search = Search { pid, found: None };
        // SAFETY: `each` only reads `search` through the pointer, during
        // this call.
        unsafe { EnumWindows(Some(each), &mut search as *mut Search as LPARAM) };
        search.found
    }

    fn bring_forward(window: HWND) {
        // SAFETY: plain calls on a window handle; a window that has gone
        // meanwhile makes them fail harmlessly. A process just started by
        // the person may hand the foreground on.
        unsafe {
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            SetForegroundWindow(window);
        }
    }

    /// The other openDJ has had no window for [`WAIT`]: offers to end it.
    /// Whether this one should start.
    fn ask_to_end(others: &[u32]) -> bool {
        if !ask(
            "Another openDJ is still running, without a window. It may be stuck \
             closing, and it keeps your audio interface and controllers busy.\n\n\
             End it and start openDJ?",
        ) {
            return false;
        }
        if others.iter().all(|&pid| end(pid)) {
            return true;
        }
        ask(
            "Windows couldn't end the other openDJ: it is stuck inside a device \
             driver, and only restarting Windows clears it.\n\n\
             Start openDJ anyway? Devices the other one holds may not work until \
             Windows restarts.",
        )
    }

    /// Ends process `pid` and waits for it; whether it ended.
    fn end(pid: u32) -> bool {
        // SAFETY: a plain system call; the handle is closed below.
        let process = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid) };
        if process.is_null() {
            // Gone already, or not ours to end.
            return other_instances().iter().all(|&p| p != pid);
        }
        // SAFETY: `process` was opened with the rights these need.
        let ended = unsafe {
            TerminateProcess(process, 1);
            WaitForSingleObject(process, END_WAIT_MS) == WAIT_OBJECT_0
        };
        close(process);
        ended
    }

    /// A Yes/No question, No the default; whether Yes was picked.
    fn ask(text: &str) -> bool {
        let (text, title) = (wide(text), wide("openDJ"));
        // SAFETY: both strings are NUL-terminated and outlive the call; no
        // owner window.
        let answer = unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND,
            )
        };
        answer == IDYES
    }

    fn close(handle: HANDLE) {
        // SAFETY: an open handle this module owns, closed once.
        unsafe { CloseHandle(handle) };
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}
