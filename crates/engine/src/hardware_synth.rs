//! A hardware synth the instrument deck plays instead of a plugin: every
//! note the deck makes (keys, pads, Scale/Chord/Strum, Arp, HOLD, the step
//! sequencer) goes to the synth's MIDI port and channel, the macro knobs
//! send its control changes, and its sound comes back on an audio input
//! (the `Instrument` input route, see [`crate::output::InputRole`]), which
//! plays as the instrument's sound.
//!
//! The audio thread only queues each message with the host time it is due
//! (the moment its buffer is heard, plus its frame); a thread of its own
//! ([`start`]) sends it then, sleeping until just before and spinning on
//! the clock as MIDI clock out does, so notes land within a fraction of a
//! millisecond of their place.
//!
//! The synth's sound comes back late: MIDI out, the synth, the interface's
//! input and the engine's queue. `[Instrument1],hardware_latency` (ms) is
//! that delay: the step sequencer and the arpeggiator work that far ahead of
//! the beat, so their notes go out early and are heard on it. Played keys go
//! out at once.
//!
//! MIDI clock and Start/Stop go to the synth too, following the deck the
//! instrument plays to (its transport, as the arpeggiator follows) rather
//! than the master deck, early by the same latency, so the synth's own
//! sequencer and arpeggiator start on that deck's next bar and keep its
//! tempo. `[Instrument1],hardware_clock` turns that off.
//!
//! Controls: `[Instrument1],hardware` (1 while a hardware synth is loaded),
//! `[Instrument1],hardware_clock` (clock and Start/Stop to it, on),
//! `[Instrument1],hardware_latency` (0 to [`MAX_LATENCY_MS`]) and
//! `[Instrument1],hardware_open` (its MIDI port is open).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use opendj_control::{Behavior, ControlHandle, ControlRegistry};
use opendj_plugin_host::shm::Midi;
use rusty_link::AblLink;

use crate::inputs::INSTRUMENT_PARAMETERS;
use crate::midi_clock::{BeatClock, Generator, Message};

pub const GROUP: &str = "[Instrument1]";

/// The longest hardware latency taken, in ms (as openDJ Studio's External
/// Instrument).
pub const MAX_LATENCY_MS: f64 = 200.0;

/// The latency a synth starts with: USB MIDI, a synth and an interface
/// input at a small buffer.
pub const DEFAULT_LATENCY_MS: f64 = 10.0;

/// A knob that sends no control change.
pub const NO_CC: u8 = 255;

/// A knob's value before it was first sent.
const UNSENT: u16 = u16::MAX;

/// How many messages wait between the audio thread and the MIDI thread.
const QUEUE: usize = 4096;

/// Sleeps until this long (µs) before a message is due, then spins.
const SPIN: f64 = 1500.0;

/// Longest sleep while nothing is due (µs): messages are queued about a
/// buffer before they are due, so this is short.
const IDLE: f64 = 1000.0;

/// A closed port is tried again this often (µs): a synth plugged in later.
const RETRY: f64 = 2e6;

/// What the app loads: where the synth is and which control change each
/// knob sends.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HardwareConfig {
    /// The MIDI output's name, as the system lists it; a port whose name
    /// holds this (any case) is taken when none matches exactly.
    pub port: String,
    /// 0 to 15 (MIDI channel 1 to 16).
    pub channel: u8,
    /// The control change each of `[Instrument1],parameter1` to
    /// `parameter32` sends, none for a knob that sends nothing.
    pub ccs: Vec<Option<u8>>,
    /// For a knob with 14-bit resolution, the control change its low 7 bits
    /// go on (usually its CC + 32), sent after the high ones.
    pub lsbs: Vec<Option<u8>>,
    /// Each knob's name, as the deck shows it.
    pub names: Vec<String>,
}

/// The audio thread's copy of a [`HardwareConfig`]: no heap, so it goes
/// through a queue by value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
    pub(crate) channel: u8,
    pub(crate) ccs: [u8; INSTRUMENT_PARAMETERS],
    pub(crate) lsbs: [u8; INSTRUMENT_PARAMETERS],
}

impl Slot {
    #[cfg(test)]
    pub(crate) fn for_test(c: &HardwareConfig) -> Self {
        Self::from_config(c)
    }

    fn from_config(c: &HardwareConfig) -> Self {
        let mut ccs = [NO_CC; INSTRUMENT_PARAMETERS];
        for (k, cc) in ccs.iter_mut().zip(&c.ccs) {
            *k = cc.filter(|&n| n < 128).unwrap_or(NO_CC);
        }
        let mut lsbs = [NO_CC; INSTRUMENT_PARAMETERS];
        for (k, cc) in lsbs.iter_mut().zip(&c.lsbs) {
            *k = cc.filter(|&n| n < 128).unwrap_or(NO_CC);
        }
        Self {
            channel: c.channel & 15,
            ccs,
            lsbs,
        }
    }
}

/// A MIDI message and the host time (µs, Link's clock) it is due.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Timed {
    pub(crate) at: f64,
    pub(crate) len: u8,
    pub(crate) data: [u8; 3],
}

impl Timed {
    fn bytes(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }
}

/// How many bytes a channel message with `status` takes.
fn message_len(status: u8) -> u8 {
    match status & 0xF0 {
        0xC0 | 0xD0 => 2,
        _ => 3,
    }
}

/// The effect rack's half: turns the instrument's notes and knobs into
/// timed messages for the MIDI thread.
pub(crate) struct Out {
    slot: Option<Slot>,
    /// Each knob's last value sent, 0 to 16383 (14 bits); [`UNSENT`]
    /// before the first.
    sent: [u16; INSTRUMENT_PARAMETERS],
    tx: rtrb::Producer<Timed>,
    slots: rtrb::Consumer<Option<Slot>>,
    latency: ControlHandle,
    /// The instrument's deck's beat as each buffer is heard, for the clock.
    clock: Arc<BeatClock>,
}

/// The MIDI thread's half of [`Out`]'s queues, taken once.
pub struct HardwareLink {
    pub(crate) rx: rtrb::Consumer<Timed>,
    pub(crate) slots: rtrb::Producer<Option<Slot>>,
    pub(crate) clock: Arc<BeatClock>,
    latency: ControlHandle,
}

impl Out {
    pub(crate) fn new(reg: &ControlRegistry) -> (Self, HardwareLink) {
        let (tx, rx) = rtrb::RingBuffer::new(QUEUE);
        let (slots_tx, slots) = rtrb::RingBuffer::new(16);
        let latency = reg.create(
            GROUP,
            "hardware_latency",
            DEFAULT_LATENCY_MS,
            0.0,
            MAX_LATENCY_MS,
            Behavior::Linear,
        );
        reg.create(GROUP, "hardware_clock", 1.0, 0.0, 1.0, Behavior::Toggle);
        let clock = Arc::new(BeatClock::default());
        (
            Self {
                slot: None,
                sent: [UNSENT; INSTRUMENT_PARAMETERS],
                tx,
                slots,
                latency: latency.clone(),
                clock: clock.clone(),
            },
            HardwareLink {
                rx,
                slots: slots_tx,
                clock,
                latency,
            },
        )
    }

    /// Takes a synth loaded or unloaded since the last buffer.
    pub(crate) fn take_changes(&mut self) {
        while let Ok(slot) = self.slots.pop() {
            if slot.map(|s| s.channel) != self.slot.map(|s| s.channel) || slot.is_none() {
                self.sent = [UNSENT; INSTRUMENT_PARAMETERS];
            }
            self.slot = slot;
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.slot.is_some()
    }

    /// How far ahead of the beat the sequencer and arpeggiator work, in
    /// seconds: the hardware latency while a synth is loaded.
    pub(crate) fn ahead(&self) -> f64 {
        if self.slot.is_none() {
            return 0.0;
        }
        self.latency.get().clamp(0.0, MAX_LATENCY_MS) / 1000.0
    }

    /// Publishes the instrument's deck as this buffer is heard (`heard_at`,
    /// µs): its beat, and its tempo while it plays (0 stopped), for the
    /// synth's clock.
    pub(crate) fn publish(&self, heard_at: i64, beat: f64, tempo: f64) {
        if self.slot.is_some() {
            self.clock.publish(heard_at, beat, tempo);
        }
    }

    /// Queues this buffer's knobs and `notes` (frames into the buffer, at
    /// `sample_rate`), which is heard at host time `heard_at` (µs).
    pub(crate) fn send(&mut self, heard_at: i64, sample_rate: f32, params: &[f32], notes: &[Midi]) {
        let Some(slot) = self.slot else { return };
        let base = heard_at as f64;
        let ch = slot.channel & 15;
        for (k, &cc) in slot.ccs.iter().enumerate() {
            if cc == NO_CC {
                continue;
            }
            let Some(&p) = params.get(k) else { break };
            let lsb = slot.lsbs[k];
            let p = p.clamp(0.0, 1.0);
            // 7-bit knobs keep their value in the high bits, so both kinds
            // compare the same way.
            let v = if lsb == NO_CC {
                ((p * 127.0).round() as u16) << 7
            } else {
                (p * 16383.0).round() as u16
            };
            if self.sent[k] == v {
                continue;
            }
            let msb = Timed {
                at: base,
                len: 3,
                data: [0xB0 | ch, cc, (v >> 7) as u8],
            };
            if self.tx.push(msb).is_err() {
                continue;
            }
            if lsb != NO_CC {
                let low = Timed {
                    at: base,
                    len: 3,
                    data: [0xB0 | ch, lsb, (v & 127) as u8],
                };
                let _ = self.tx.push(low);
            }
            self.sent[k] = v;
        }
        let us_per_frame = 1e6 / f64::from(sample_rate.max(1.0));
        for n in notes {
            let status = n.data[0];
            // System messages aren't the synth's to take from the deck.
            if !(0x80..0xF0).contains(&status) {
                continue;
            }
            let msg = Timed {
                at: base + f64::from(n.frame) * us_per_frame,
                len: message_len(status),
                data: [(status & 0xF0) | ch, n.data[1] & 127, n.data[2] & 127],
            };
            let _ = self.tx.push(msg);
        }
    }
}

struct Shared {
    config: Mutex<Option<HardwareConfig>>,
    generation: AtomicU64,
    stop: AtomicBool,
    /// Messages the app sends at once (a program change), in order.
    now: Mutex<Vec<Vec<u8>>>,
    /// The audio thread's queue of loaded synths.
    slots: Mutex<rtrb::Producer<Option<Slot>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// Loads and unloads the hardware synth; clones share the one MIDI thread.
#[derive(Clone)]
pub struct HardwareSynth {
    shared: Arc<Shared>,
    loaded: ControlHandle,
    choice: ControlHandle,
    /// `[Instrument1],num_parameters`: how many knobs the synth's has.
    count: ControlHandle,
}

impl HardwareSynth {
    /// Plays `config`'s synth on the instrument deck from now on, or none.
    /// Loading one unloads the instrument plugin; choosing a plugin
    /// (`[Instrument1],instrument`) unloads the synth.
    pub fn set(&self, config: Option<HardwareConfig>) {
        let slot = config.as_ref().map(Slot::from_config);
        if config.is_some() {
            self.choice.set(0.0);
        }
        self.loaded.set_bool(config.is_some());
        let knobs = config
            .as_ref()
            .map_or(0, |c| c.ccs.len().min(INSTRUMENT_PARAMETERS));
        self.count.set(knobs as f64);
        *self.shared.config.lock().unwrap() = config;
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
        let _ = self.shared.slots.lock().unwrap().push(slot);
    }

    /// Picks the synth's preset `program` (0 to 127), after Bank Select
    /// `bank` when it has banks. Nothing without a synth loaded.
    pub fn program(&self, bank: Option<u8>, program: u8) {
        let Some(ch) = self.config().map(|c| c.channel & 15) else {
            return;
        };
        let mut now = self.shared.now.lock().unwrap();
        if let Some(b) = bank {
            now.push(vec![0xB0 | ch, 0, b & 127]);
            now.push(vec![0xB0 | ch, 32, 0]);
        }
        now.push(vec![0xC0 | ch, program & 127]);
    }

    /// The loaded synth's knob names, empty without one.
    pub fn names(&self) -> Vec<String> {
        self.config().map(|c| c.names).unwrap_or_default()
    }

    /// The synth loaded now.
    pub fn config(&self) -> Option<HardwareConfig> {
        self.shared.config.lock().unwrap().clone()
    }

    /// The MIDI outputs there are now, by name.
    pub fn ports() -> Vec<String> {
        let Ok(out) = midir::MidiOutput::new("openDJ") else {
            return Vec::new();
        };
        out.ports()
            .iter()
            .filter_map(|p| out.port_name(p).ok())
            .collect()
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Where a message goes; the MIDI thread's port, or a test's list.
trait Sink: Send {
    fn send(&mut self, bytes: &[u8]) -> bool;
}

impl Sink for midir::MidiOutputConnection {
    fn send(&mut self, bytes: &[u8]) -> bool {
        midir::MidiOutputConnection::send(self, bytes).is_ok()
    }
}

/// The port named `wanted` among `names`: the same name, else the first
/// holding it, any case.
fn find_port(names: &[String], wanted: &str) -> Option<usize> {
    if wanted.is_empty() {
        return None;
    }
    names.iter().position(|n| n == wanted).or_else(|| {
        let w = wanted.to_lowercase();
        names.iter().position(|n| n.to_lowercase().contains(&w))
    })
}

fn open(port: &str) -> Option<Box<dyn Sink>> {
    let out = midir::MidiOutput::new("openDJ").ok()?;
    let ports = out.ports();
    let names: Vec<String> = ports
        .iter()
        .map(|p| out.port_name(p).unwrap_or_default())
        .collect();
    let i = find_port(&names, port)?;
    match out.connect(&ports[i], "openDJ instrument") {
        Ok(c) => Some(Box::new(c)),
        Err(e) => {
            eprintln!("{port}: cannot open MIDI output for the hardware synth: {e}");
            None
        }
    }
}

/// All notes off and sound off on `channel`, so nothing hangs when a synth
/// is unloaded or moves.
fn silence(sink: &mut dyn Sink, channel: u8) {
    let ch = channel & 15;
    sink.send(&[0xB0 | ch, 123, 0]);
    sink.send(&[0xB0 | ch, 64, 0]);
}

/// The MIDI thread's work apart from ports and the clock, for tests.
#[derive(Default)]
struct Sender {
    queue: VecDeque<Timed>,
}

impl Sender {
    /// Takes in what the audio thread queued, in time order.
    fn take(&mut self, rx: &mut rtrb::Consumer<Timed>) {
        while let Ok(m) = rx.pop() {
            let at = self.queue.partition_point(|q| q.at <= m.at);
            self.queue.insert(at, m);
        }
    }

    /// Sends everything due within [`SPIN`] of now, each at its time.
    fn send_due(&mut self, sink: Option<&mut Box<dyn Sink>>, clock: &dyn Fn() -> f64) {
        let mut sink = sink;
        while let Some(m) = self.queue.front() {
            if m.at - clock() > SPIN {
                return;
            }
            while clock() < m.at {
                std::hint::spin_loop();
            }
            let m = self.queue.pop_front().expect("queued");
            if let Some(s) = sink.as_deref_mut() {
                s.send(m.bytes());
            }
        }
    }

    /// How long to sleep (µs) before there's something to do.
    fn idle_for(&self, now: f64) -> f64 {
        match self.queue.front() {
            Some(m) => (m.at - SPIN - now).clamp(0.0, IDLE),
            None => IDLE,
        }
    }
}

/// Starts the MIDI thread, which runs until the registry is dropped.
pub(crate) fn start(
    link: HardwareLink,
    clock: Arc<AblLink>,
    reg: &Arc<ControlRegistry>,
) -> HardwareSynth {
    let HardwareLink {
        rx,
        slots,
        clock: deck_clock,
        latency,
    } = link;
    let shared = Arc::new(Shared {
        config: Mutex::new(None),
        generation: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        now: Mutex::new(Vec::new()),
        slots: Mutex::new(slots),
        thread: Mutex::new(None),
    });
    let synth = HardwareSynth {
        shared: shared.clone(),
        loaded: reg.indicator(GROUP, "hardware", 1.0),
        choice: reg
            .get(GROUP, "instrument")
            .expect("[Instrument1],instrument exists: the rack makes it"),
        count: reg
            .get(GROUP, "num_parameters")
            .expect("[Instrument1],num_parameters exists: the input makes it"),
    };
    let open_shown = reg.indicator(GROUP, "hardware_open", 1.0);
    let clock_on = reg
        .get(GROUP, "hardware_clock")
        .expect("[Instrument1],hardware_clock exists: the rack makes it");
    let weak = Arc::downgrade(reg);
    let s = synth.clone();
    let spawned = std::thread::Builder::new()
        .name("hardware-synth".into())
        .spawn(move || {
            let c = Clocking {
                deck: deck_clock,
                on: clock_on,
                latency,
            };
            run(rx, clock, weak, Arc::downgrade(&s.shared), s, open_shown, c)
        });
    match spawned {
        Ok(t) => *shared.thread.lock().unwrap() = Some(t),
        Err(e) => eprintln!("could not start the hardware synth's MIDI thread: {e}"),
    }
    synth
}

/// What the MIDI thread needs for the synth's clock.
struct Clocking {
    deck: Arc<BeatClock>,
    on: ControlHandle,
    latency: ControlHandle,
}

/// Clock messages are worked out this far (µs) ahead of when they go out.
const CLOCK_LOOKAHEAD: f64 = 3000.0;

fn clock_message(t: f64, m: Message) -> Timed {
    let b = m.bytes();
    let mut data = [0; 3];
    data[..b.len()].copy_from_slice(&b);
    Timed {
        at: t,
        len: b.len() as u8,
        data,
    }
}

fn run(
    mut rx: rtrb::Consumer<Timed>,
    link: Arc<AblLink>,
    reg: Weak<ControlRegistry>,
    shared: Weak<Shared>,
    synth: HardwareSynth,
    open_shown: ControlHandle,
    clocking: Clocking,
) {
    let mut generator = Generator::default();
    let mut ticks = Vec::new();
    let HardwareSynth { loaded, choice, .. } = synth;
    let clock = move || link.clock_micros() as f64;
    let mut sender = Sender::default();
    let mut sink: Option<Box<dyn Sink>> = None;
    let mut current: Option<HardwareConfig> = None;
    let mut generation = u64::MAX;
    let mut last_try = f64::MIN;
    loop {
        let Some(sh) = shared.upgrade() else { break };
        if reg.strong_count() == 0 || sh.stop.load(Ordering::Relaxed) {
            break;
        }
        // A plugin chosen for the instrument unloads the synth.
        if current.is_some() && choice.get() > 0.5 {
            *sh.config.lock().unwrap() = None;
            sh.generation.fetch_add(1, Ordering::Relaxed);
            let _ = sh.slots.lock().unwrap().push(None);
            loaded.set_bool(false);
        }
        let now = clock();
        let g = sh.generation.load(Ordering::Relaxed);
        if g != generation {
            let wanted = sh.config.lock().unwrap().clone();
            if let (Some(old), Some(s)) = (&current, sink.as_mut()) {
                if generator.running() {
                    s.send(&Message::Stop.bytes());
                }
                silence(s.as_mut(), old.channel);
            }
            generator = Generator::default();
            let same_port = matches!((&current, &wanted), (Some(a), Some(b)) if a.port == b.port);
            if !same_port {
                sink = None;
            }
            current = wanted;
            generation = g;
            last_try = f64::MIN;
        }
        if sink.is_none() && current.is_some() && now - last_try > RETRY {
            sink = current.as_ref().and_then(|c| open(&c.port));
            last_try = now;
        }
        if current.is_none() {
            sink = None;
        }
        open_shown.set_bool(sink.is_some());
        let now_msgs = std::mem::take(&mut *sh.now.lock().unwrap());
        if let Some(s) = sink.as_mut() {
            for m in &now_msgs {
                s.send(m);
            }
        }
        drop(sh);
        sender.take(&mut rx);
        if current.is_none() {
            sender.queue.clear();
        }
        // The synth's clock follows the instrument's deck, early by the
        // latency; Stop goes out when it is turned off or unloaded.
        let early = clocking.latency.get().clamp(0.0, MAX_LATENCY_MS) * 1000.0;
        let enabled = current.is_some() && clocking.on.get_bool();
        let now = clock();
        ticks.clear();
        generator.poll(
            clocking.deck.read(),
            enabled,
            false,
            now + early,
            now + early + CLOCK_LOOKAHEAD,
            &mut ticks,
        );
        for &(t, m) in &ticks {
            let msg = clock_message(t - early, m);
            let at = sender.queue.partition_point(|q| q.at <= msg.at);
            sender.queue.insert(at, msg);
        }
        sender.send_due(sink.as_mut(), &clock);
        let wait = sender.idle_for(clock());
        if wait > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(wait / 1e6));
        }
    }
    if let (Some(c), Some(s)) = (&current, sink.as_mut()) {
        if generator.running() {
            s.send(&Message::Stop.bytes());
        }
        silence(s.as_mut(), c.channel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out() -> (Out, HardwareLink) {
        let reg = ControlRegistry::new();
        Out::new(&reg)
    }

    fn drain(link: &mut HardwareLink) -> Vec<Timed> {
        let mut v = Vec::new();
        while let Ok(m) = link.rx.pop() {
            v.push(m);
        }
        v
    }

    fn slot(channel: u8, ccs: &[Option<u8>]) -> Option<Slot> {
        Some(Slot::from_config(&HardwareConfig {
            port: "Synth".into(),
            channel,
            ccs: ccs.to_vec(),
            lsbs: Vec::new(),
            ..Default::default()
        }))
    }

    #[test]
    fn fine_knobs_send_14_bits_high_then_low() {
        let (mut o, mut link) = out();
        let config = HardwareConfig {
            port: "Messenger".into(),
            channel: 0,
            ccs: vec![Some(19)],
            lsbs: vec![Some(51)],
            ..Default::default()
        };
        link.slots.push(Some(Slot::from_config(&config))).unwrap();
        o.take_changes();
        o.send(0, 48_000.0, &[0.5], &[]);
        let sent = drain(&mut link);
        // 0.5 of 16383 is 8192: high 64, low 0.
        assert_eq!(sent[0].bytes(), [0xB0, 19, 64]);
        assert_eq!(sent[1].bytes(), [0xB0, 51, 0]);
        // A move too small for 7 bits still goes out on 14.
        o.send(0, 48_000.0, &[0.5 + 3.0 / 16383.0], &[]);
        let sent = drain(&mut link);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].bytes(), [0xB0, 51, 3]);
    }

    #[test]
    fn notes_go_out_on_the_synths_channel_at_their_frame() {
        let (mut o, mut link) = out();
        link.slots.push(slot(3, &[])).unwrap();
        o.take_changes();
        let notes = [Midi::note_on(0, 0, 60, 100), Midi::note_off(480, 0, 60)];
        o.send(1_000_000, 48_000.0, &[], &notes);
        let sent = drain(&mut link);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].bytes(), [0x93, 60, 100]);
        assert_eq!(sent[0].at, 1_000_000.0);
        assert_eq!(sent[1].bytes()[0] & 0xF0, 0x80);
        assert_eq!(sent[1].bytes()[0] & 15, 3);
        assert!((sent[1].at - 1_010_000.0).abs() < 1e-6);
    }

    #[test]
    fn knobs_send_their_cc_only_when_they_move() {
        let (mut o, mut link) = out();
        link.slots
            .push(slot(0, &[Some(74), None, Some(71)]))
            .unwrap();
        o.take_changes();
        o.send(0, 48_000.0, &[0.5, 1.0, 0.0], &[]);
        let sent = drain(&mut link);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].bytes(), [0xB0, 74, 64]);
        assert_eq!(sent[1].bytes(), [0xB0, 71, 0]);
        o.send(100, 48_000.0, &[0.5, 0.2, 0.0], &[]);
        assert!(drain(&mut link).is_empty());
        o.send(200, 48_000.0, &[0.6, 0.2, 0.0], &[]);
        assert_eq!(drain(&mut link)[0].bytes(), [0xB0, 74, 76]);
    }

    #[test]
    fn nothing_goes_out_without_a_synth() {
        let (mut o, mut link) = out();
        o.send(0, 48_000.0, &[1.0], &[Midi::note_on(0, 0, 60, 100)]);
        assert!(drain(&mut link).is_empty());
        assert_eq!(o.ahead(), 0.0);
    }

    #[test]
    fn the_latency_is_how_far_ahead_notes_are_worked_out() {
        let (mut o, mut link) = out();
        link.slots.push(slot(0, &[])).unwrap();
        o.take_changes();
        o.latency.set(12.0);
        assert!((o.ahead() - 0.012).abs() < 1e-9);
        link.slots.push(None).unwrap();
        o.take_changes();
        assert_eq!(o.ahead(), 0.0);
    }

    #[test]
    fn the_clock_follows_the_instruments_deck_only_with_a_synth() {
        let (mut o, mut link) = out();
        o.publish(1000, 4.0, 120.0);
        assert!(link.clock.read().is_none());
        link.slots.push(slot(0, &[])).unwrap();
        o.take_changes();
        o.publish(2000, 8.0, 124.0);
        let snap = link.clock.read().unwrap();
        assert_eq!((snap.heard_at, snap.beat, snap.tempo), (2000.0, 8.0, 124.0));
        assert_eq!(clock_message(5.0, Message::Start).bytes(), [0xFA]);
        assert_eq!(clock_message(5.0, Message::Clock).bytes(), [0xF8]);
    }

    #[test]
    fn ports_match_by_name_then_by_part_of_it() {
        let names = vec![
            "IAC Driver Bus 1".to_string(),
            "MiniFreak MIDI".to_string(),
            "Moog Messenger".to_string(),
        ];
        assert_eq!(find_port(&names, "Moog Messenger"), Some(2));
        assert_eq!(find_port(&names, "minifreak"), Some(1));
        assert_eq!(find_port(&names, "Prophet"), None);
        assert_eq!(find_port(&names, ""), None);
    }

    type Log = Arc<Mutex<Vec<(f64, Vec<u8>)>>>;
    struct List(Log, Arc<Mutex<f64>>);
    impl Sink for List {
        fn send(&mut self, bytes: &[u8]) -> bool {
            let t = *self.1.lock().unwrap();
            self.0.lock().unwrap().push((t, bytes.to_vec()));
            true
        }
    }

    #[test]
    fn the_sender_sends_in_time_order_once_due() {
        let (mut tx, mut rx) = rtrb::RingBuffer::new(8);
        let msg = |at: f64, key: u8| Timed {
            at,
            len: 3,
            data: [0x90, key, 100],
        };
        tx.push(msg(5000.0, 2)).unwrap();
        tx.push(msg(2000.0, 1)).unwrap();
        let mut s = Sender::default();
        s.take(&mut rx);
        let log = Arc::new(Mutex::new(Vec::new()));
        let now = Arc::new(Mutex::new(0.0));
        let mut sink: Option<Box<dyn Sink>> = Some(Box::new(List(log.clone(), now.clone())));
        let t = now.clone();
        let clock = move || {
            // Each look at the clock moves it on 100 µs.
            let mut g = t.lock().unwrap();
            *g += 100.0;
            *g
        };
        s.send_due(sink.as_mut(), &clock);
        assert!(log.lock().unwrap().is_empty(), "nothing due yet");
        *now.lock().unwrap() = 1000.0;
        s.send_due(sink.as_mut(), &clock);
        let sent = log.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, [0x90, 1, 100]);
        assert!(sent[0].0 >= 2000.0);
        *now.lock().unwrap() = 4000.0;
        s.send_due(sink.as_mut(), &clock);
        assert_eq!(log.lock().unwrap().len(), 2);
    }
}
