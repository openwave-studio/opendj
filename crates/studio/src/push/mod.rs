//! Ableton Push 2 as the studio's controller, with no Live running: pads
//! launch clips or play the selected instrument, the encoders work the
//! device and mixer, and the screen shows what they turn.
//!
//! Push has two MIDI ports. Live uses the first; other software uses the
//! User port, after switching Push to User mode with a SysEx message. The
//! screen is a separate USB interface. A thread looks for Push every two
//! seconds, so it can be plugged in at any time.

pub mod display;
pub mod surface;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use crate::control::Controls;
use surface::Surface;

/// The User port among MIDI port names: macOS calls it "Ableton Push 2
/// User Port", Windows "MIDIIN2 (Ableton Push 2)", Linux the second of the
/// device's ports.
pub fn user_port(names: &[String]) -> Option<usize> {
    let push: Vec<usize> = (0..names.len())
        .filter(|&i| names[i].to_lowercase().contains("push 2"))
        .collect();
    let lower = |i: usize| names[i].to_lowercase();
    push.iter()
        .copied()
        .find(|&i| {
            lower(i).contains("user")
                || lower(i).contains("midiin2")
                || lower(i).contains("midiout2")
        })
        .or_else(|| {
            let not_live: Vec<usize> = push
                .iter()
                .copied()
                .filter(|&i| !lower(i).contains("live"))
                .collect();
            match not_live.len() {
                1 if push.len() > 1 => Some(not_live[0]),
                _ => push.get(1).copied(),
            }
        })
}

/// A running Push connection; stops and hands Push back to Live when
/// dropped.
pub struct Push {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Push {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What the studio's status line says about Push.
pub type Status = Arc<std::sync::Mutex<String>>;

/// Starts looking for Push 2.
pub fn start(controls: Arc<Controls>, status: Status) -> Push {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::Builder::new()
        .name("push2".into())
        .spawn(move || run(controls, flag, status))
        .ok();
    Push { stop, thread }
}

struct Connection {
    _input: MidiInputConnection<()>,
    output: MidiOutputConnection,
    messages: mpsc::Receiver<Vec<u8>>,
}

fn connect() -> Option<Connection> {
    let input = MidiInput::new("openDJ Studio Push in").ok()?;
    let output = MidiOutput::new("openDJ Studio Push out").ok()?;
    let in_ports = input.ports();
    let out_ports = output.ports();
    let in_names: Vec<String> = in_ports
        .iter()
        .map(|p| input.port_name(p).unwrap_or_default())
        .collect();
    let out_names: Vec<String> = out_ports
        .iter()
        .map(|p| output.port_name(p).unwrap_or_default())
        .collect();
    let in_port = in_ports.get(user_port(&in_names)?)?.clone();
    let out_port = out_ports.get(user_port(&out_names)?)?.clone();
    let (tx, messages) = mpsc::channel();
    let input = input
        .connect(
            &in_port,
            "push2-user",
            move |_, msg, _| {
                let _ = tx.send(msg.to_vec());
            },
            (),
        )
        .ok()?;
    let output = output.connect(&out_port, "push2-user").ok()?;
    Some(Connection {
        _input: input,
        output,
        messages,
    })
}

/// Sends MIDI clock to Push so its blinking and pulsing lights keep time:
/// from the transport while it plays, from the tempo while it doesn't.
struct ClockOut {
    ticks: f64,
    sent: i64,
    last: Instant,
    was_playing: bool,
}

impl ClockOut {
    fn new() -> ClockOut {
        ClockOut {
            ticks: 0.0,
            sent: 0,
            last: Instant::now(),
            was_playing: false,
        }
    }

    fn tick(&mut self, c: &Controls, out: &mut MidiOutputConnection) {
        let m = &c.monitor;
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        let playing = m.playing();
        if playing {
            let ticks = m.beat() * 24.0;
            if !self.was_playing || ticks < self.ticks - 1.0 {
                // Started (or jumped back): restart the lights' phase.
                let _ = out.send(&[0xFA]);
                self.sent = ticks.floor() as i64;
            }
            self.ticks = ticks;
        } else {
            self.ticks += elapsed * m.tempo().max(1.0) / 60.0 * 24.0;
        }
        self.was_playing = playing;
        let due = self.ticks.floor() as i64;
        // Never a flood after a stall.
        let n = (due - self.sent).clamp(0, 24);
        for _ in 0..n {
            let _ = out.send(&[0xF8]);
        }
        self.sent = due;
    }
}

fn set_status(status: &Status, s: &str) {
    if let Ok(mut st) = status.lock() {
        if *st != s {
            *st = s.to_string();
        }
    }
}

fn run(c: Arc<Controls>, stop: Arc<AtomicBool>, status: Status) {
    let colors: Vec<u32> = c.tracks.iter().map(|t| t.color).collect();
    let mut surface = Surface::new();
    let mut conn: Option<Connection> = None;
    // The eight tracks whose colours the palette holds.
    let mut bank = 0;
    let mut screen: Option<display::Screen> = None;
    let mut clock = ClockOut::new();
    let mut last_try = Instant::now() - Duration::from_secs(10);
    let mut last_frame = Instant::now() - Duration::from_secs(10);
    let mut last_screen_try = Instant::now() - Duration::from_secs(10);
    // Slint components stay on the thread that made them, so the screen's
    // renderer is made here.
    let mut renderer = match display::Renderer::new() {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("{e}");
            None
        }
    };
    while !stop.load(Ordering::Relaxed) {
        if conn.is_none() && last_try.elapsed() > Duration::from_secs(2) {
            last_try = Instant::now();
            if let Some(mut found) = connect() {
                let _ = found.output.send(&surface::user_mode());
                std::thread::sleep(Duration::from_millis(20));
                bank = surface::bank(&c);
                for m in surface::palette(&colors[bank.min(colors.len())..]) {
                    let _ = found.output.send(&m);
                    std::thread::sleep(Duration::from_millis(2));
                }
                surface.reset_lights();
                conn = Some(found);
                set_status(&status, "Push 2 connected");
            }
        }
        if screen.is_none() && conn.is_some() && last_screen_try.elapsed() > Duration::from_secs(2)
        {
            last_screen_try = Instant::now();
            screen = display::Screen::open();
            set_status(
                &status,
                if screen.is_some() {
                    "Push 2 connected"
                } else {
                    "Push 2 connected (its screen is in use or unavailable)"
                },
            );
        }
        let mut lost = false;
        if let Some(k) = conn.as_mut() {
            loop {
                match k.messages.try_recv() {
                    Ok(msg) => surface.input(&msg, &c),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        lost = true;
                        break;
                    }
                }
            }
            if surface::bank(&c) != bank {
                // Another eight tracks: their colours into the palette.
                bank = surface::bank(&c);
                for m in surface::palette(&colors[bank.min(colors.len())..]) {
                    let _ = k.output.send(&m);
                }
            }
            surface.tick(Instant::now(), &c);
            clock.tick(&c, &mut k.output);
            for m in surface.lights(&c) {
                if k.output.send(&m).is_err() {
                    lost = true;
                    break;
                }
            }
        }
        if let (Some(s), Some(r)) = (screen.as_ref(), renderer.as_mut()) {
            // Runs the screen's animations up to 60 times a second, and
            // sends at least twice a second (the screen goes dark after
            // two seconds without a frame).
            if last_frame.elapsed() > Duration::from_millis(16) {
                r.update(&c, &surface);
                let changed = r.draw();
                if changed || last_frame.elapsed() > Duration::from_millis(500) {
                    last_frame = Instant::now();
                    if s.send(r.frame()).is_err() {
                        screen = None;
                    }
                }
            }
        }
        if lost {
            conn = None;
            screen = None;
            set_status(&status, "Push 2 unplugged");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // Let go: notes off, lights off, Push back to Live.
    surface.end_notes(&c);
    if let Some(mut k) = conn {
        for m in Surface::all_off() {
            let _ = k.output.send(&m);
        }
        let _ = k.output.send(&surface::live_mode());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_the_user_port() {
        let mac = names(&[
            "IAC Bus 1",
            "Ableton Push 2 Live Port",
            "Ableton Push 2 User Port",
        ]);
        assert_eq!(user_port(&mac), Some(2));
        let win = names(&["Ableton Push 2", "MIDIIN2 (Ableton Push 2)", "loopMIDI"]);
        assert_eq!(user_port(&win), Some(1));
        let linux = names(&[
            "Midi Through:Midi Through Port-0 14:0",
            "Ableton Push 2:Ableton Push 2 MIDI 1 20:0",
            "Ableton Push 2:Ableton Push 2 MIDI 2 20:1",
        ]);
        assert_eq!(user_port(&linux), Some(2));
        assert_eq!(user_port(&names(&["Launchpad X"])), None);
    }
}
