//! Registering with Native Instruments' host agent.
//!
//! Some NI keyboards only act as a host-driven controller while a host is
//! registered with `NIHostIntegrationAgent`, the background service NI's
//! installer adds. The Komplete Kontrol M32 is one: with no host, its knobs
//! fall back to the firmware's MIDI mode ("Template 1": absolute CCs that stop
//! at 0 and 127, and a readout drawn over ours). Once a host registers and
//! takes the keyboard, the knobs arrive as endless counters in HID report
//! 0x01, which the mapping reads.
//!
//! So when such a keyboard is plugged in and the agent is running, openDJ
//! registers with it the way NI's own software does, and stays registered for
//! as long as the keyboard's mapping runs. Without the agent nothing changes:
//! the mapping keeps reading the MIDI-mode knobs. The agent is polled now and
//! then, so starting it later still works.
//!
//! The message layout comes from Encdr (github.com/RufusIbiza/Encdr, by
//! RufusIbiza), which documented the agent's protocol for NI controllers.
//! This is openDJ's own code; only the protocol facts are shared.
//!
//! macOS only for now: the agent there speaks over `CFMessagePort`s. Windows
//! (named pipes, same messages) comes later.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Native Instruments' USB vendor id.
const NI: u16 = 0x17cc;

/// Keyboards that need a host registered with the agent before their knobs
/// work over HID.
const NEEDS_AGENT: &[u16] = &[
    0x1860, // Komplete Kontrol M32
];

/// Whether this device only reaches its HID mode through the agent.
pub(crate) fn needs_agent(vendor: u16, product: u16) -> bool {
    vendor == NI && NEEDS_AGENT.contains(&product)
}

/// Held while a keyboard's mapping runs; dropping it unregisters.
pub(crate) struct Registration {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Registration {
    /// Keep openDJ registered with the agent for this keyboard, from a thread
    /// of its own (connecting can take a few seconds). `port` names the
    /// keyboard in the log.
    pub(crate) fn start(product: u16, serial: String, port: String) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        #[cfg(target_os = "macos")]
        let thread = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name(format!("ni agent {port}"))
                .spawn(move || mac::stay_registered(product, &serial, &port, &stop))
                .ok()
        };
        #[cfg(not(target_os = "macos"))]
        let thread = {
            let _ = (product, serial, port);
            None
        };
        Self { stop, thread }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The agent's messages: little-endian 32-bit words, sometimes followed by a
/// NUL-terminated string. Built and read here so they're tested everywhere.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod wire {
    /// The agent's well-known ports, in the order to try them.
    pub const AGENT_PORTS: &[&str] = &[
        "com.native-instruments.NIHostIntegrationAgent",
        "NIHWMainHandler",
    ];

    /// "true": the agent's yes, which also starts every accepted reply.
    pub const YES: u32 = 0x7472_7565;
    /// Who we say we are when connecting.
    const CLIENT: [u32; 2] = [0x4e69_4d32, 0x7072_6d79];

    const CONNECT_DEVICE: u32 = 0x0344_7500;
    const CONNECT_UNIT: u32 = 0x0344_4900;
    const NOTIFY_PORT_READY: u32 = 0x0340_4300;
    const ASK_DEVICE_STATE: u32 = 0x0344_7143;

    const UNIT_ON: u32 = 0x0344_4e2b;
    const UNIT_OFF: u32 = 0x0344_4e2d;
    const CONTROL: u32 = 0x0343_4e00;

    /// The name the agent shows for this host.
    const HOST_NAME: &str = "openDJ";

    fn pack(words: &[u32], text: Option<&str>) -> Vec<u8> {
        let mut out: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        if let Some(t) = text {
            out.extend_from_slice(t.as_bytes());
            out.push(0);
        }
        out
    }

    fn word(d: &[u8], i: usize) -> Option<u32> {
        let b = d.get(i * 4..i * 4 + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// `len` bytes from byte `at`, as text without the trailing NULs.
    fn text_at(d: &[u8], at: usize, len: usize) -> Option<String> {
        let raw = d.get(at..at.checked_add(len)?)?;
        Some(
            String::from_utf8_lossy(raw)
                .trim_end_matches('\0')
                .to_string(),
        )
    }

    /// Register for every unit of this product.
    pub fn connect_device(product: u16) -> Vec<u8> {
        pack(
            &[CONNECT_DEVICE, product as u32, CLIENT[0], CLIENT[1], 0],
            None,
        )
    }

    /// Register as a host of one unit. The length counts the NUL.
    pub fn connect_unit(product: u16, serial: &str) -> Vec<u8> {
        let len = serial.len() as u32 + 1;
        pack(
            &[CONNECT_UNIT, product as u32, CLIENT[0], CLIENT[1], len],
            Some(serial),
        )
    }

    /// Tell the agent our notification port is listening.
    pub fn notify_port_ready(name: &str) -> Vec<u8> {
        pack(&[NOTIFY_PORT_READY, YES, 0, name.len() as u32], Some(name))
    }

    pub fn ask_device_state() -> Vec<u8> {
        pack(&[ASK_DEVICE_STATE], None)
    }

    /// What a host sends once it has a unit, before it takes the hardware.
    /// The agent answers some of these; nothing in the answers is needed.
    pub fn unit_setup(product: u16) -> Vec<Vec<u8>> {
        let name_len = HOST_NAME.len() as u32 + 1;
        vec![
            pack(&[0x0356_6775, 0x4b65_7973], None),
            pack(&[0x0356_6775, 0x524b_6579], None),
            pack(&[0x0343_6753], None),
            pack(
                &[0x0349_734e, 0x7000_1006, 0xf6b2_4000, name_len],
                Some(HOST_NAME),
            ),
            pack(&[0x0344_674d, product as u32], None),
            pack(&[0x0343_4300, 0x7374_7274], None),
            pack(&[0x0349_7354, 0], None),
            pack(&[0x0343_6746], None),
            pack(&[0x0349_7374, YES], None),
            pack(&[0x0344_5200, 2], None),
            pack(&[0x0344_4100, 3, 0, 0x04e2_97b0], None),
        ]
    }

    /// The ports a connect reply hands out: (request, notification).
    /// Layout: YES, length, name, length, name.
    pub fn ports(reply: &[u8]) -> Option<(String, String)> {
        if word(reply, 0)? != YES {
            return None;
        }
        let req_len = word(reply, 1)? as usize;
        let request = text_at(reply, 8, req_len)?;
        let at = 8usize.checked_add(req_len)?;
        let b = reply.get(at..at + 4)?;
        let note_len = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
        let notification = text_at(reply, at + 4, note_len)?;
        Some((request, notification))
    }

    /// A notification from the agent.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Note {
        /// A unit came or went, with its serial number.
        Unit {
            present: bool,
            serial: String,
        },
        /// This host got the hardware (`true`) or another host took it.
        Control(bool),
        Other,
    }

    pub fn note(d: &[u8]) -> Note {
        let parsed = match word(d, 0) {
            Some(w @ (UNIT_ON | UNIT_OFF)) => word(d, 3)
                .and_then(|len| text_at(d, 16, len as usize))
                .map(|serial| Note::Unit {
                    present: w == UNIT_ON,
                    serial,
                }),
            Some(CONTROL) => word(d, 1).map(|w| Note::Control(w == YES)),
            _ => None,
        };
        parsed.unwrap_or(Note::Other)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn connects_as_a_host() {
            assert_eq!(
                connect_device(0x1860),
                [
                    0x00, 0x75, 0x44, 0x03, 0x60, 0x18, 0, 0, 0x32, 0x4d, 0x69, 0x4e, 0x79, 0x6d,
                    0x72, 0x70, 0, 0, 0, 0
                ]
            );
            let m = connect_unit(0x1860, "ABC123");
            assert_eq!(&m[..4], [0x00, 0x49, 0x44, 0x03]);
            assert_eq!(word(&m, 4), Some(7), "the length counts the NUL");
            assert_eq!(&m[20..], b"ABC123\0");
            let m = notify_port_ready("NIHWS18600001Notification");
            assert_eq!(word(&m, 1), Some(YES));
            assert_eq!(word(&m, 3), Some(25));
            assert_eq!(m.len(), 16 + 25 + 1);
        }

        #[test]
        fn names_the_host() {
            let setup = unit_setup(0x1860);
            assert_eq!(setup.len(), 11);
            assert_eq!(word(&setup[3], 3), Some(7));
            assert_eq!(&setup[3][16..], b"openDJ\0");
            assert_eq!(word(&setup[4], 1), Some(0x1860));
        }

        #[test]
        fn reads_the_ports_handed_out() {
            let mut reply = pack(&[YES, 21], Some("NIHWS18600001Request"));
            reply.extend(pack(&[26], Some("NIHWS18600001Notification")));
            assert_eq!(
                ports(&reply),
                Some((
                    "NIHWS18600001Request".into(),
                    "NIHWS18600001Notification".into()
                ))
            );
            assert_eq!(ports(&[0x64, 0x69, 0x63, 0x65]), None, "refused");
            assert_eq!(ports(&reply[..20]), None, "cut short");
        }

        #[test]
        fn reads_notifications() {
            let on = pack(&[UNIT_ON, 0, 0x1860, 7], Some("ABC123"));
            assert_eq!(
                note(&on),
                Note::Unit {
                    present: true,
                    serial: "ABC123".into()
                }
            );
            let off = pack(&[UNIT_OFF, 0, 0x1860, 7], Some("ABC123"));
            assert_eq!(
                note(&off),
                Note::Unit {
                    present: false,
                    serial: "ABC123".into()
                }
            );
            assert_eq!(
                note(&pack(&[UNIT_ON, 0, 0, 9], None)),
                Note::Other,
                "serial cut off"
            );
            assert_eq!(note(&pack(&[CONTROL, YES], None)), Note::Control(true));
            assert_eq!(note(&pack(&[CONTROL, 0], None)), Note::Control(false));
            assert_eq!(note(&pack(&[CONTROL], None)), Note::Other);
            assert_eq!(note(&pack(&[0x0344_4e00, 1], None)), Note::Other);
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    //! The agent on macOS: each port is a `CFMessagePort`. A request carries
    //! one message and comes back with the agent's answer; notifications
    //! arrive on a port of ours, served from its own run loop.

    use super::wire::{self, Note};
    use core_foundation_sys::base::{kCFAllocatorDefault, Boolean, CFRelease};
    use core_foundation_sys::data::{CFDataCreate, CFDataGetBytePtr, CFDataGetLength, CFDataRef};
    use core_foundation_sys::messageport::{
        kCFMessagePortSuccess, CFMessagePortContext, CFMessagePortCreateLocal,
        CFMessagePortCreateRemote, CFMessagePortCreateRunLoopSource, CFMessagePortInvalidate,
        CFMessagePortIsValid, CFMessagePortRef, CFMessagePortSendRequest,
    };
    use core_foundation_sys::runloop::{
        kCFRunLoopDefaultMode, CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRemoveSource,
        CFRunLoopRunInMode,
    };
    use core_foundation_sys::string::{
        kCFStringEncodingUTF8, CFStringCreateWithBytes, CFStringRef,
    };
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    /// How often to look for the agent again when it isn't there.
    const RETRY: Duration = Duration::from_secs(5);
    /// How long to wait for the agent to report the keyboard.
    const FIND_UNIT: Duration = Duration::from_secs(3);

    /// Registered for as long as `stop` is clear, connecting again whenever
    /// the agent goes away or starts later.
    pub(super) fn stay_registered(product: u16, serial: &str, port: &str, stop: &AtomicBool) {
        let mut said_missing = false;
        while !stop.load(Ordering::Relaxed) {
            match Host::connect(product, serial) {
                Ok((host, notes)) => {
                    eprintln!(
                        "{port}: registered with NI's agent as host of {}",
                        host.serial
                    );
                    said_missing = false;
                    host.serve(port, &notes, stop);
                    if !stop.load(Ordering::Relaxed) {
                        eprintln!("{port}: NI's agent went away");
                    }
                }
                Err(e) => {
                    if !said_missing {
                        eprintln!(
                            "{port}: not registered with NI's agent ({e}); knobs stay in MIDI mode"
                        );
                        said_missing = true;
                    }
                }
            }
            let until = Instant::now() + RETRY;
            while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    fn cf_string(s: &str) -> CFStringRef {
        // SAFETY: the bytes are valid UTF-8 for the given length.
        unsafe {
            CFStringCreateWithBytes(
                kCFAllocatorDefault,
                s.as_ptr(),
                s.len() as isize,
                kCFStringEncodingUTF8,
                0,
            )
        }
    }

    fn bytes_of(data: CFDataRef) -> Vec<u8> {
        if data.is_null() {
            return Vec::new();
        }
        // SAFETY: `data` is a live CFData; its bytes are copied before return.
        unsafe {
            let len = CFDataGetLength(data).max(0) as usize;
            let ptr = CFDataGetBytePtr(data);
            if ptr.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(ptr, len).to_vec()
            }
        }
    }

    /// One of the agent's ports.
    struct Remote(CFMessagePortRef);

    impl Remote {
        fn open(name: &str) -> Option<Self> {
            let cf = cf_string(name);
            if cf.is_null() {
                return None;
            }
            // The agent opens a port it handed out a moment after saying so.
            let mut port = std::ptr::null_mut();
            for _ in 0..10 {
                // SAFETY: `cf` is a valid CFString.
                port = unsafe { CFMessagePortCreateRemote(kCFAllocatorDefault, cf) };
                if !port.is_null() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // SAFETY: we own `cf`.
            unsafe { CFRelease(cf as *const c_void) };
            (!port.is_null()).then_some(Self(port))
        }

        /// Send a message, and the agent's answer (empty when none).
        fn send(&self, message: &[u8], wait: Duration) -> Result<Vec<u8>, String> {
            // SAFETY: `self.0` is a live remote port; data and reply are
            // released after their bytes are copied.
            unsafe {
                let data = CFDataCreate(
                    kCFAllocatorDefault,
                    message.as_ptr(),
                    message.len() as isize,
                );
                if data.is_null() {
                    return Err("out of memory".into());
                }
                let mut reply: CFDataRef = std::ptr::null();
                let secs = wait.as_secs_f64();
                let status = CFMessagePortSendRequest(
                    self.0,
                    0,
                    data,
                    secs,
                    secs,
                    kCFRunLoopDefaultMode,
                    &mut reply,
                );
                CFRelease(data as *const c_void);
                let bytes = bytes_of(reply);
                if !reply.is_null() {
                    CFRelease(reply as *const c_void);
                }
                if status == kCFMessagePortSuccess {
                    Ok(bytes)
                } else {
                    Err(format!("no answer (status {status})"))
                }
            }
        }

        fn alive(&self) -> bool {
            // SAFETY: `self.0` is a live port reference.
            unsafe { CFMessagePortIsValid(self.0) != 0 }
        }
    }

    // SAFETY: CFMessagePorts may be used from any thread; each Remote is used
    // by one thread at a time.
    unsafe impl Send for Remote {}

    impl Drop for Remote {
        fn drop(&mut self) {
            // SAFETY: we own this reference.
            unsafe {
                CFMessagePortInvalidate(self.0);
                CFRelease(self.0 as *const c_void);
            }
        }
    }

    /// Our notification port, served on its own thread until dropped.
    struct Listener {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    /// Called on the listener's run loop with each notification. The data
    /// is only valid during the call, so it's copied.
    unsafe extern "C" fn on_note(
        _: CFMessagePortRef,
        _: i32,
        data: CFDataRef,
        info: *mut c_void,
    ) -> CFDataRef {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: `info` is the listener thread's Sender, alive while the port is.
            let notes = unsafe { &*(info as *const Sender<Vec<u8>>) };
            let _ = notes.send(bytes_of(data));
        }));
        std::ptr::null()
    }

    impl Listener {
        fn serve(name: &str, notes: Sender<Vec<u8>>) -> Result<Self, String> {
            let stop = Arc::new(AtomicBool::new(false));
            let (ready_tx, ready_rx) = mpsc::channel();
            let name = name.to_string();
            let thread = {
                let stop = stop.clone();
                std::thread::Builder::new()
                    .name("ni agent notes".into())
                    .spawn(move || {
                        let notes = Box::new(notes);
                        let context = CFMessagePortContext {
                            version: 0,
                            info: &*notes as *const Sender<Vec<u8>> as *mut c_void,
                            retain: None,
                            release: None,
                            copyDescription: None,
                        };
                        let cf = cf_string(&name);
                        let mut free_info: Boolean = 0;
                        // SAFETY: `context.info` outlives the port: `notes`
                        // is dropped after the port is invalidated below.
                        let local = unsafe {
                            CFMessagePortCreateLocal(
                                kCFAllocatorDefault,
                                cf,
                                Some(on_note),
                                &context,
                                &mut free_info,
                            )
                        };
                        if !cf.is_null() {
                            // SAFETY: we own `cf`.
                            unsafe { CFRelease(cf as *const c_void) };
                        }
                        if local.is_null() {
                            let _ = ready_tx.send(Err(format!("cannot listen on {name}")));
                            return;
                        }
                        // SAFETY: `local` is a live local port, served on
                        // this thread's run loop until `stop` is set.
                        unsafe {
                            let source =
                                CFMessagePortCreateRunLoopSource(kCFAllocatorDefault, local, 0);
                            let run_loop = CFRunLoopGetCurrent();
                            if !source.is_null() {
                                CFRunLoopAddSource(run_loop, source, kCFRunLoopDefaultMode);
                                let _ = ready_tx.send(Ok(()));
                                while !stop.load(Ordering::Relaxed) {
                                    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.2, 0);
                                }
                                CFRunLoopRemoveSource(run_loop, source, kCFRunLoopDefaultMode);
                                CFRelease(source as *const c_void);
                            } else {
                                let _ = ready_tx.send(Err(format!("cannot serve {name}")));
                            }
                            CFMessagePortInvalidate(local);
                            CFRelease(local as *const c_void);
                        }
                        drop(notes);
                    })
                    .map_err(|e| e.to_string())?
            };
            let listener = Self {
                stop,
                thread: Some(thread),
            };
            match ready_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Ok(())) => Ok(listener),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("notification port didn't start".into()),
            }
        }
    }

    /// One registration: the agent's request port for it, and our
    /// notification port.
    struct Link {
        request: Remote,
        _listener: Listener,
    }

    /// Ask the agent's well-known port for a registration, then set up both
    /// ports it hands out.
    fn link(agent: &str, hello: &[u8], notes: &Sender<Vec<u8>>) -> Result<Link, String> {
        let bootstrap = Remote::open(agent).ok_or("agent not running")?;
        let reply = bootstrap.send(hello, Duration::from_secs(2))?;
        drop(bootstrap);
        let (request, notification) = wire::ports(&reply).ok_or("the agent refused")?;
        let listener = Listener::serve(&notification, notes.clone())?;
        let request = Remote::open(&request).ok_or("the agent's port didn't open")?;
        let _ = request.send(
            &wire::notify_port_ready(&notification),
            Duration::from_millis(500),
        );
        Ok(Link {
            request,
            _listener: listener,
        })
    }

    /// Registered as host of one keyboard.
    struct Host {
        serial: String,
        _device: Link,
        unit: Link,
    }

    impl Host {
        fn connect(product: u16, serial: &str) -> Result<(Self, Receiver<Vec<u8>>), String> {
            let mut last = String::from("agent not running");
            for agent in wire::AGENT_PORTS {
                match Self::connect_via(agent, product, serial) {
                    Ok(done) => return Ok(done),
                    Err(e) => last = e,
                }
            }
            Err(last)
        }

        fn connect_via(
            agent: &str,
            product: u16,
            serial: &str,
        ) -> Result<(Self, Receiver<Vec<u8>>), String> {
            let (tx, rx) = mpsc::channel();
            let device = link(agent, &wire::connect_device(product), &tx)?;
            let _ = device
                .request
                .send(&wire::ask_device_state(), Duration::from_millis(500));
            let serial = find_unit(&rx, serial)?;
            let unit = link(agent, &wire::connect_unit(product, &serial), &tx)?;
            for m in wire::unit_setup(product) {
                let _ = unit.request.send(&m, Duration::from_millis(40));
            }
            Ok((
                Self {
                    serial,
                    _device: device,
                    unit,
                },
                rx,
            ))
        }

        /// Hold the registration until `stop` or the agent goes away,
        /// logging who has the keyboard.
        fn serve(&self, port: &str, notes: &Receiver<Vec<u8>>, stop: &AtomicBool) {
            while !stop.load(Ordering::Relaxed) && self.unit.request.alive() {
                match notes.recv_timeout(Duration::from_millis(250)) {
                    Ok(d) => match wire::note(&d) {
                        Note::Control(true) => {
                            eprintln!("{port}: openDJ has the keyboard (HID knobs)")
                        }
                        Note::Control(false) => eprintln!("{port}: another host took the keyboard"),
                        Note::Unit {
                            present: false,
                            serial,
                        } if serial == self.serial => {
                            eprintln!("{port}: NI's agent lost the keyboard");
                        }
                        _ => {}
                    },
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        }
    }

    /// The serial the agent knows the keyboard by. USB and the agent may
    /// spell it differently, so take the first unit reported when none
    /// matches within a moment.
    fn find_unit(notes: &Receiver<Vec<u8>>, wanted: &str) -> Result<String, String> {
        let mut deadline = Instant::now() + FIND_UNIT;
        let mut first = None;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match notes.recv_timeout(left) {
                Ok(d) => {
                    if let Note::Unit {
                        present: true,
                        serial,
                    } = wire::note(&d)
                    {
                        if serial.eq_ignore_ascii_case(wanted) {
                            return Ok(serial);
                        }
                        if first.is_none() {
                            first = Some(serial);
                            deadline = deadline.min(Instant::now() + Duration::from_millis(1500));
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        first.ok_or_else(|| "the agent doesn't see the keyboard".into())
    }
}
