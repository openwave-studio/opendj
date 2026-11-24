//! Runs Mixxx's HID mappings for the Traktor Kontrol range against the real
//! engine controls, with a stand-in device in place of the hardware.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendj_control::ControlRegistry;
use opendj_controllers::hid::{self, Io};
use opendj_controllers::{default_mappings_dir, Controller, Mapping};

/// A device that answers every report request with zeros and keeps what it
/// is sent. Input reports are as long as the S4 MK3's longest; feature
/// reports (calibration data, on the S2 MK1 and Z1) are 32 bytes, as theirs.
#[derive(Default)]
struct FakeDevice {
    feature_sent: RefCell<Vec<Vec<u8>>>,
}

impl Io for FakeDevice {
    fn write(&self, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn send_feature(&self, report: &[u8]) -> Result<(), String> {
        self.feature_sent.borrow_mut().push(report.to_vec());
        Ok(())
    }
    fn get_input(&self, _: u8) -> Option<Vec<u8>> {
        Some(vec![0; REPORT - 1])
    }
    fn get_feature(&self, _: u8) -> Option<Vec<u8>> {
        Some(vec![0; 32])
    }
}

/// The length of the input reports fed in, with the ID.
const REPORT: usize = 128;

fn load(file: &str) -> (Arc<ControlRegistry>, Controller, Rc<FakeDevice>) {
    let reg = ControlRegistry::new();
    let _engine = opendj_engine::engine::Engine::new(&reg, 44_100.0);
    let path = default_mappings_dir().unwrap().join(file);
    let mapping = Mapping::load(&path).unwrap();
    assert!(mapping.is_hid(), "{file}");
    let device = Rc::new(FakeDevice::default());
    let c = Controller::new_hid("test", mapping, reg.clone(), device.clone())
        .unwrap_or_else(|e| panic!("{file}: {e}"));
    (reg, c, device)
}

fn settle(c: &mut Controller, ms: u64) {
    let start = Instant::now();
    for i in 0..=ms {
        c.poll(start + Duration::from_millis(i));
    }
}

#[test]
fn every_bundled_hid_mapping_loads_and_takes_reports() {
    let dir = default_mappings_dir().unwrap();
    let mappings = hid::list_mappings(&dir);
    assert!(mappings.len() >= 9, "{}", mappings.len());
    let mut failures = Vec::new();
    for (name, path, _) in mappings {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let (_reg, mut c, _device) = load(&file);
        settle(&mut c, 50);
        let _ = c.take_reports();
        // Every report ID with all bits clear, then all set, as if every
        // button were pressed and every fader at the top.
        for id in 0..=0x05u8 {
            for fill in [0x00u8, 0xFF, 0x00] {
                let mut report = vec![fill; REPORT];
                report[0] = id;
                c.receive_report(&report);
                c.poll(Instant::now());
            }
        }
        settle(&mut c, 50);
        let _ = c.take_reports();
        c.shutdown();
        // Mixxx's F1 script calls a shutdown function it doesn't have.
        let errors: Vec<String> = c
            .errors()
            .into_iter()
            .filter(|e| !e.starts_with("KontrolF1.shutdown(): not a function"))
            .collect();
        if !errors.is_empty() {
            failures.push(format!("{name}: {errors:#?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn get(reg: &ControlRegistry, group: &str, key: &str) -> f64 {
    reg.get(group, key)
        .unwrap_or_else(|| panic!("{group},{key} missing"))
        .get()
}

/// The S2 MK3's play button and channel fader, as its reports carry them,
/// reach the decks, and its lights go out as output reports.
#[test]
fn traktor_kontrol_s2_mk3() {
    let (reg, mut c, _device) = load("Traktor Kontrol S2 MK3.hid.xml");
    settle(&mut c, 50);
    let reports = c.take_reports();
    assert!(!reports.is_empty(), "init lights the controller");
    assert!(
        reports.iter().all(|r| r.len() > 1),
        "each report has an ID and data: {reports:x?}"
    );

    // Buttons: report 0x01. The parser ignores the first report it sees.
    let mut buttons = vec![0u8; 22];
    buttons[0] = 0x01;
    c.receive_report(&buttons);
    let idle = buttons.clone();
    // PLAY on deck 1: bit 0x08 of byte 2, counting the ID as byte 0.
    buttons[0x02] |= 0x08;
    c.receive_report(&buttons);
    c.receive_report(&idle);
    settle(&mut c, 30);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0, "PLAY starts deck 1");
    let _ = c.take_reports();
    // The engine sets the indicator while playing; do it by hand here.
    reg.get("[Channel1]", "play_indicator").unwrap().set(1.0);
    settle(&mut c, 30);
    let lights = c.take_reports();
    assert!(!lights.is_empty(), "the PLAY light follows");

    // Faders and knobs: report 0x02, 16-bit little-endian values.
    let mut analog = vec![0u8; 40];
    analog[0] = 0x02;
    c.receive_report(&analog);
    analog[0x03] = 0xFF;
    analog[0x04] = 0x7F;
    c.receive_report(&analog);
    settle(&mut c, 30);
    let v = get(&reg, "[Channel1]", "volume");
    assert!(v > 0.3, "deck 1's channel fader moved the volume: {v}");
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// Presses and lets go of one button: bit `mask` of byte `byte` (the ID is
/// byte 0) in a report like `idle`.
fn tap(c: &mut Controller, idle: &[u8], byte: usize, mask: u8) {
    let mut pressed = idle.to_vec();
    pressed[byte] |= mask;
    c.receive_report(&pressed);
    c.receive_report(idle);
    settle(c, 10);
}

/// Traktor's layers on the S2 MK3: SAMPLES plays a stem track's stems
/// (pads 1-4 mute them), REV is flux reverse while held.
#[test]
fn traktor_kontrol_s2_mk3_plays_stems_as_traktor() {
    let (reg, mut c, _device) = load("Traktor Kontrol S2 MK3.hid.xml");
    settle(&mut c, 50);
    reg.get("[Channel1]", "stem_count").unwrap().set(4.0);
    let mut idle = vec![0u8; 22];
    idle[0] = 0x01;
    c.receive_report(&idle);
    c.receive_report(&idle);
    // SAMPLES on deck 1, then pad 1: stem 1 mutes.
    tap(&mut c, &idle, 0x01, 0x80);
    tap(&mut c, &idle, 0x02, 0x10);
    assert_eq!(
        get(&reg, "[Channel1_Stem1]", "mute"),
        1.0,
        "pad 1 mutes stem 1"
    );
    tap(&mut c, &idle, 0x02, 0x10);
    assert_eq!(get(&reg, "[Channel1_Stem1]", "mute"), 0.0, "and back");
    // REV held: reverse roll, as Traktor's flux reverse.
    let mut rev = idle.clone();
    rev[0x01] |= 0x01;
    c.receive_report(&rev);
    settle(&mut c, 10);
    assert_eq!(get(&reg, "[Channel1]", "reverseroll"), 1.0);
    c.receive_report(&idle);
    settle(&mut c, 10);
    assert_eq!(get(&reg, "[Channel1]", "reverseroll"), 0.0);
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// Traktor's layers on the S3: SAMPLES plays a stem track's stems, SHIFT +
/// SYNC makes the deck the tempo leader, a tap on GRID toggles quantize.
#[test]
fn traktor_kontrol_s3_plays_stems_as_traktor() {
    let (reg, mut c, _device) = load("Traktor Kontrol S3.hid.xml");
    settle(&mut c, 50);
    reg.get("[Channel1]", "stem_count").unwrap().set(4.0);
    let mut idle = vec![0u8; 24];
    idle[0] = 0x01;
    c.receive_report(&idle);
    c.receive_report(&idle);
    tap(&mut c, &idle, 0x02, 0x40);
    tap(&mut c, &idle, 0x03, 0x02);
    assert_eq!(
        get(&reg, "[Channel1_Stem1]", "mute"),
        1.0,
        "pad 1 mutes stem 1"
    );
    // SHIFT + SYNC.
    let mut shifted = idle.clone();
    shifted[0x01] |= 0x01;
    c.receive_report(&shifted);
    tap(&mut c, &shifted, 0x02, 0x08);
    c.receive_report(&idle);
    settle(&mut c, 10);
    assert!(
        get(&reg, "[Channel1]", "sync_leader") > 0.0,
        "SHIFT + SYNC: leader"
    );
    let quantize = get(&reg, "[Channel1]", "quantize");
    tap(&mut c, &idle, 0x01, 0x80);
    assert_ne!(get(&reg, "[Channel1]", "quantize"), quantize, "GRID tap");
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// The Maschine Mikro MK3 drives the Groove Box: pads reach
/// `[GrooveN],padK`, buttons pick the pad mode, and the lights and screen
/// go out as reports 0x80 and 0xe0.
#[test]
fn maschine_mikro_mk3_drives_the_groove_box() {
    let (reg, mut c, _device) = load("Native Instruments Maschine Mikro MK3.hid.xml");
    reg.get("[Groove2]", "enabled").unwrap().set(1.0);
    settle(&mut c, 60);
    let reports = c.take_reports();
    let screen: Vec<&Vec<u8>> = reports.iter().filter(|r| r[0] == 0xe0).collect();
    // "No Groove Box" from init, then deck 2's: two halves each.
    assert_eq!(screen.len(), 4, "{}", reports.len());
    assert!(screen.iter().all(|r| r.len() == 1 + 8 + 256));
    let lights = reports.iter().find(|r| r[0] == 0x80).expect("lights");
    assert_eq!(lights.len(), 81);

    // Pad 4 (bottom row, so report index 15) pressed fairly hard, then let
    // go: the deck 2 Groove Box.
    c.receive_report(&[0x02, 15, 0x10 | 0x0b, 0xb8, 0, 0, 0]);
    c.poll(Instant::now());
    let v = get(&reg, "[Groove2]", "pad4");
    assert!((v - 1.0).abs() < 1e-9, "3000 of 4095 is full: {v}");
    c.receive_report(&[0x02, 15, 0x30, 0x00, 0, 0, 0]);
    c.poll(Instant::now());
    assert_eq!(get(&reg, "[Groove2]", "pad4"), 0.0);

    // PAD MODE (button 27: byte 4, bit 3) plays the pads; PATTERN (32)
    // picks grooves.
    let mut buttons = vec![0u8; 16];
    buttons[0] = 0x01;
    let idle = buttons.clone();
    c.receive_report(&idle);
    buttons[4] |= 1 << 3;
    c.receive_report(&buttons);
    c.receive_report(&idle);
    assert_eq!(get(&reg, "[Groove2]", "pad_mode"), 1.0);
    let mut pattern = idle.clone();
    pattern[5] |= 1;
    c.receive_report(&pattern);
    c.receive_report(&idle);
    assert_eq!(get(&reg, "[Groove2]", "pad_mode"), 2.0);

    // RIGHT (9: byte 2, bit 1) moves to row 2; MUTE (38: byte 5, bit 6)
    // tapped mutes it.
    let mut right = idle.clone();
    right[2] |= 1 << 1;
    c.receive_report(&right);
    c.receive_report(&idle);
    assert_eq!(get(&reg, "[Groove2]", "row_selected"), 2.0);
    let mut mute = idle.clone();
    mute[5] |= 1 << 6;
    c.receive_report(&mute);
    c.receive_report(&idle);
    assert_eq!(get(&reg, "[Groove2]", "row2_mute"), 1.0);

    // A pad colour from the engine lights that pad: row 1's colour on pad 1,
    // bottom left (report index 12).
    reg.get("[Groove2]", "pad1_color").unwrap().set(1.0);
    // The first settle ran its clock 60 ms ahead; the timer is due after.
    settle(&mut c, 200);
    let reports = c.take_reports();
    let lights = reports
        .iter()
        .rev()
        .find(|r| r[0] == 0x80)
        .expect("lights again");
    assert_eq!(lights[1 + 39 + 12], 4 + 3, "red, bright");
    assert!(c.errors().is_empty(), "{:#?}", c.errors());
}

/// The kit and sound names the Mikro's screen shows are the engine's.
#[test]
fn maschine_mikro_mk3_knows_the_kits() {
    let path = default_mappings_dir()
        .unwrap()
        .join("Native-Instruments-Maschine-Mikro-MK3.js");
    let script = std::fs::read_to_string(path).unwrap();
    for kit in opendj_engine::kits::KITS.iter() {
        let sounds: Vec<String> = kit.sounds().map(|s| format!("\"{s}\"")).collect();
        let line = format!(
            "{{name: \"{}\", sounds: [{}]}}",
            kit.name,
            sounds.join(", ")
        );
        assert!(script.contains(&line), "missing {line}");
    }
}

/// The DDJ-FLX10's jog screens: the HID mapping says it runs, the MIDI
/// mapping switches the screens over with SysEx, and the HID mapping then
/// streams each loaded deck's state and loads its waveform.
#[test]
fn ddj_flx10_jog_screens_show_the_waveform() {
    // Ten seconds of red (bass only), at full height.
    opendj_controllers::waves::set_source(Some(Arc::new(|deck, per_second| {
        if deck == 0 {
            vec![[255, 0, 0, 255]; (10.0 * per_second) as usize]
        } else {
            Vec::new()
        }
    })));
    // A 300-byte "JPEG" for deck 1's art: three reports.
    opendj_controllers::waves::set_cover_source(Some(Arc::new(|deck, size| {
        assert_eq!(size, 240);
        if deck == 0 {
            (0..300).map(|i| i as u8).collect()
        } else {
            Vec::new()
        }
    })));
    let reg = ControlRegistry::new();
    let _engine = opendj_engine::engine::Engine::new(&reg, 44_100.0);
    let dir = default_mappings_dir().unwrap();
    let mut midi = Controller::new(
        "DDJ-FLX10",
        Mapping::load(&dir.join("Pioneer-DDJ-FLX10.midi.xml")).unwrap(),
        reg.clone(),
    )
    .unwrap();
    let mut screens = Controller::new_hid(
        "Pioneer DDJ-FLX10 jog screens",
        Mapping::load(&dir.join("Pioneer-DDJ-FLX10-screens.hid.xml")).unwrap(),
        reg.clone(),
        Rc::new(FakeDevice::default()),
    )
    .unwrap();
    let set = |key: &str, v: f64| reg.get("[Channel1]", key).unwrap().force(v);
    set("duration", 10.0);
    set("playposition", 0.5);
    set("bpm", 128.5);
    set("key", 22.0); // A minor
    set("track_loaded", 1.0);

    // Nothing goes to the screens until the MIDI side has switched them.
    let start = Instant::now();
    for i in 0..400 {
        let t = start + Duration::from_millis(i);
        midi.poll(t);
        screens.poll(t);
    }
    let sysex: Vec<Vec<u8>> = midi
        .take_output()
        .into_iter()
        .filter(|m| m[0] == 0xF0)
        .collect();
    let enter = [
        0xF0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x04, 0x01, 0x00, 0x03, 0x01, 0xF7,
    ];
    assert!(sysex.contains(&enter.to_vec()), "{sysex:x?}");
    let keepalive = [
        0xF0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x04, 0x01, 0x00, 0x50, 0x31, 0xF7,
    ];
    assert!(sysex.contains(&keepalive.to_vec()), "kept in HID mode");

    let reports = screens.take_reports();
    assert!(!reports.is_empty());
    // Report ID 0 then 128 bytes, deck 1 only.
    assert!(
        reports.iter().all(|r| r.len() == 129 && r[0] == 0),
        "{:x?}",
        reports[0]
    );
    let kinds: Vec<u8> = reports.iter().map(|r| r[2]).collect();
    assert!(reports.iter().all(|r| r[1] == 0x10));

    // State first, then the load in order.
    assert_eq!(kinds[0], 0x27);
    let load: Vec<u8> = kinds.iter().copied().filter(|&k| k != 0x27).collect();
    let mut expected = vec![0x30, 0x39, 0x39, 0x39, 0x33, 0x33, 0x33, 0x35, 0x35, 0x35];
    expected.extend(std::iter::repeat_n(0x36, 1500usize.div_ceil(19)));
    expected.push(0x2F);
    assert_eq!(&load[..expected.len()], &expected[..], "{load:x?}");
    // Then entries at the playhead now and then (5 s in = entry 750).
    let trickle = reports.iter().rev().find(|r| r[2] == 0x36).unwrap();
    assert_eq!(
        u32::from_le_bytes([trickle[11], trickle[12], trickle[13], trickle[14]]),
        750
    );

    let state = &reports[0][1..];
    assert_eq!(&state[5..7], &[0, 5], "five seconds played");
    assert_eq!(&state[9..11], &[0, 10], "a ten-second track");
    assert_eq!((state[13], state[14]), (128, 0x80), "128.5 BPM");
    assert_eq!(state[29], 0x82, "8A");
    assert_eq!(state[31], 0x02, "deck 1");

    let length = reports.iter().find(|r| r[2] == 0x30).unwrap();
    assert_eq!(&length[7..9], &[0, 10]);
    let count = reports.iter().find(|r| r[2] == 0x35).unwrap();
    assert_eq!(u16::from_le_bytes([count[3], count[4]]), 1500);
    // The art: its size, then 119 bytes, then 122 a report.
    let art: Vec<&Vec<u8>> = reports.iter().filter(|r| r[2] == 0x33).collect();
    assert_eq!(art.len(), 3);
    assert_eq!((art[0][3], art[0][5]), (1, 3));
    assert_eq!(u16::from_le_bytes([art[0][7], art[0][8]]), 300);
    assert_eq!((art[0][10], art[0][128]), (0, 118));
    assert_eq!((art[1][3], art[1][7]), (2, 119));
    assert_eq!((art[2][3], art[2][7], art[2][65]), (3, 241, 43));
    assert_eq!(art[2][66], 0, "nothing after the end");
    let labels = reports.iter().find(|r| r[2] == 0x39).unwrap();
    assert_eq!(&labels[8..15], b"HOT CUE");
    // Red at full brightness, full height: (7 << 13) | (31 << 2).
    let first = reports.iter().find(|r| r[2] == 0x36).unwrap();
    assert_eq!(&first[11..15], &[0, 0, 0, 0], "from entry 0");
    assert_eq!(&first[15..17], &0xE07Cu16.to_le_bytes());
    assert_eq!(first[7], 19);

    // Unplugged or turned off: the MIDI displays come back.
    screens.shutdown();
    screens.poll(Instant::now());
    let mut t = Instant::now();
    for _ in 0..100 {
        t += Duration::from_millis(1);
        midi.poll(t);
    }
    let _ = midi.take_output();
    for _ in 0..400 {
        t += Duration::from_millis(1);
        midi.poll(t);
    }
    let out = midi.take_output();
    assert!(
        !out.iter().any(|m| m[..] == keepalive[..]),
        "keepalive stopped"
    );
    assert!(out.iter().any(|m| m[0] == 0xBF), "MIDI jog displays again");
    assert_eq!(midi.errors(), Vec::<String>::new());
    assert_eq!(screens.errors(), Vec::<String>::new());
    opendj_controllers::waves::set_source(None);
    opendj_controllers::waves::set_cover_source(None);
}

/// openDJ's X1 MK3 mapping: transport and hot cues per side, the loop
/// encoder, MODE switching the top knobs from effects to the mixer, and the
/// lights going out as report 0x80.
#[test]
fn traktor_kontrol_x1_mk3_plays_as_traktor() {
    let (reg, mut c, _device) = load("Traktor Kontrol X1 MK3.hid.xml");
    settle(&mut c, 50);
    let mut idle = vec![0u8; 25];
    idle[0] = 0x01;
    c.receive_report(&idle);
    c.receive_report(&idle);
    // PLAY on the right deck.
    tap(&mut c, &idle, 4, 0x10);
    assert_eq!(get(&reg, "[Channel2]", "play"), 1.0, "right PLAY");
    // SHIFT + REV turns flux on; REV held is a reverse roll.
    let mut shifted = idle.clone();
    shifted[1] |= 0x01;
    c.receive_report(&shifted);
    tap(&mut c, &shifted, 4, 0x04);
    c.receive_report(&idle);
    settle(&mut c, 10);
    assert_eq!(get(&reg, "[Channel1]", "slip_enabled"), 1.0, "SHIFT + REV");
    // SHIFT + loop encoder one step up jumps a loop's length.
    let size = get(&reg, "[Channel1]", "beatloop_size");
    let mut turned = shifted.clone();
    turned[7] = 0x01;
    c.receive_report(&shifted);
    c.receive_report(&turned);
    settle(&mut c, 10);
    assert_eq!(
        get(&reg, "[Channel1]", "beatjump"),
        size,
        "SHIFT + loop encoder"
    );
    turned[1] &= !0x01;
    c.receive_report(&turned);
    idle = turned;
    // MODE twice: mixer; the fourth left knob is deck 1's volume.
    tap(&mut c, &idle, 1, 0x02);
    tap(&mut c, &idle, 1, 0x02);
    // Soft takeover: the knob picks the volume up near where it is.
    reg.get("[Channel1]", "volume").unwrap().set(0.49);
    let mut knob = idle.clone();
    knob[15] = 0x00;
    knob[16] = 0x08;
    c.receive_report(&knob);
    settle(&mut c, 10);
    let volume = get(&reg, "[Channel1]", "volume");
    assert!((volume - 0.5).abs() < 0.01, "mixer volume {volume}");
    let reports = c.take_reports();
    assert!(
        reports
            .iter()
            .any(|r| r.first() == Some(&0x80) && r.len() == 49),
        "LED report"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// openDJ's Z1 MK2 mapping: headphone cue, Mixer FX shortcuts and STEMS
/// mode turning the gain knob into stem 1's volume.
#[test]
fn traktor_kontrol_z1_mk2_plays_as_traktor() {
    let (reg, mut c, _device) = load("Traktor Kontrol Z1 MK2.hid.xml");
    settle(&mut c, 50);
    let mut idle = vec![0u8; 35];
    idle[0] = 0x01;
    c.receive_report(&idle);
    c.receive_report(&idle);
    tap(&mut c, &idle, 2, 0x10);
    assert_eq!(get(&reg, "[Channel1]", "pfl"), 1.0, "left CUE");
    tap(&mut c, &idle, 1, 0x80);
    assert_eq!(
        get(&reg, "[QuickEffectRack1_[Channel2]]", "loaded_chain_preset"),
        2.0,
        "FX 1"
    );
    // STEMS on the left with a stem track: GAIN is stem 1's volume.
    reg.get("[Channel1]", "stem_count").unwrap().set(4.0);
    reg.get("[Channel1_Stem1]", "volume").unwrap().set(0.49);
    tap(&mut c, &idle, 1, 0x02);
    let mut knob = idle.clone();
    knob[3] = 0xFF;
    knob[4] = 0x07;
    c.receive_report(&knob);
    settle(&mut c, 10);
    assert_eq!(
        get(&reg, "[Channel1_Stem1]", "volume"),
        0.5,
        "GAIN in STEMS mode"
    );
    let reports = c.take_reports();
    assert!(
        reports
            .iter()
            .any(|r| r.first() == Some(&0x80) && r.len() == 47),
        "LED report"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// The D2 as Traktor plays it: DECK switches A and C, DECK held with B picks
/// deck B, A-D send decks to the effect unit on top, the faders are stem
/// volumes and the LOOP pads jump by the loop.
#[test]
fn traktor_kontrol_d2_plays_as_traktor() {
    let (reg, mut c, _device) = load("Traktor Kontrol D2.hid.xml");
    settle(&mut c, 50);
    let mut idle = vec![0u8; 17];
    idle[0] = 0x01;
    let mut knobs = vec![0u8; 25];
    knobs[0] = 0x02;
    c.receive_report(&idle);
    c.receive_report(&knobs);
    tap(&mut c, &idle, 8, 0x10);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0, "PLAY on deck A");
    tap(&mut c, &idle, 7, 0x04);
    tap(&mut c, &idle, 8, 0x10);
    assert_eq!(
        get(&reg, "[Channel3]", "play"),
        1.0,
        "DECK, then PLAY on deck C"
    );
    // Hold DECK and press B.
    let mut held = idle.clone();
    held[7] |= 0x04;
    c.receive_report(&held);
    tap(&mut c, &held, 5, 0x02);
    c.receive_report(&idle);
    tap(&mut c, &idle, 8, 0x10);
    assert_eq!(
        get(&reg, "[Channel2]", "play"),
        1.0,
        "DECK + B, then PLAY on deck B"
    );
    tap(&mut c, &idle, 5, 0x01);
    assert_eq!(
        get(&reg, "[EffectRack1_EffectUnit2]", "group_[Channel1]_enable"),
        1.0,
        "A sends deck A to the B/D effect unit"
    );
    reg.get("[Channel2]", "stem_count").unwrap().set(4.0);
    reg.get("[Channel2_Stem2]", "volume").unwrap().set(0.49);
    let mut fader = knobs.clone();
    fader[11] = 0xF7;
    fader[12] = 0x07;
    c.receive_report(&fader);
    settle(&mut c, 10);
    assert_eq!(
        get(&reg, "[Channel2_Stem2]", "volume"),
        0.5,
        "fader 2 is stem 2"
    );
    tap(&mut c, &idle, 4, 0x08);
    assert_eq!(
        get(&reg, "[QuickEffectRack1_[Channel2_Stem2]]", "enabled"),
        1.0,
        "ON 2 sends stem 2 to the stem effect"
    );
    tap(&mut c, &idle, 8, 0x04);
    let size = get(&reg, "[Channel2]", "beatloop_size");
    tap(&mut c, &idle, 7, 0x02);
    assert_eq!(
        get(&reg, "[Channel2]", "beatjump"),
        size,
        "LOOP pad 8 jumps by the loop"
    );
    let reports = c.take_reports();
    assert!(
        reports
            .iter()
            .any(|r| r.first() == Some(&0x80) && r.len() == 123),
        "LED report"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// The S8's two deck sections play decks A and C, B and D; the mixer's CUE
/// lights through feature report 0xF4, as the S8 wants.
#[test]
fn traktor_kontrol_s8_plays_as_traktor() {
    let (reg, mut c, device) = load("Traktor Kontrol S8.hid.xml");
    settle(&mut c, 50);
    let mut idle = vec![0u8; 41];
    idle[0] = 0x01;
    let mut knobs = vec![0u8; 109];
    knobs[0] = 0x02;
    c.receive_report(&idle);
    c.receive_report(&knobs);
    tap(&mut c, &idle, 9, 0x10);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0, "left PLAY on deck A");
    tap(&mut c, &idle, 17, 0x10);
    assert_eq!(get(&reg, "[Channel2]", "play"), 1.0, "right PLAY on deck B");
    tap(&mut c, &idle, 16, 0x04);
    tap(&mut c, &idle, 17, 0x10);
    assert_eq!(
        get(&reg, "[Channel4]", "play"),
        1.0,
        "right DECK, then PLAY on deck D"
    );
    device.feature_sent.borrow_mut().clear();
    tap(&mut c, &idle, 20, 0x08);
    assert_eq!(get(&reg, "[Channel1]", "pfl"), 1.0, "CUE A");
    assert!(
        device
            .feature_sent
            .borrow()
            .iter()
            .any(|r| r[..3] == [0xF4, 0x26, 0x01]),
        "CUE A lights"
    );
    let mut xfader = knobs.clone();
    xfader[41] = 0xF7;
    xfader[42] = 0x07;
    c.receive_report(&xfader);
    settle(&mut c, 10);
    assert_eq!(
        get(&reg, "[Master]", "crossfader"),
        0.0,
        "crossfader centred"
    );
    tap(&mut c, &idle, 25, 0x10);
    assert_eq!(
        get(&reg, "[EffectRack1_EffectUnit1]", "group_[Channel2]_enable"),
        1.0,
        "FX 1 on channel B"
    );
    let reports = c.take_reports();
    for (id, len) in [(0x80, 119), (0x81, 119), (0x82, 74)] {
        assert!(
            reports
                .iter()
                .any(|r| r.first() == Some(&id) && r.len() == len),
            "LED report {id:#x}"
        );
    }
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// The S5 as the S8 without the performance section: decks A and C on the
/// left, B and D on the right; CUE lights in the mixer's own LED report.
#[test]
fn traktor_kontrol_s5_plays_as_traktor() {
    let (reg, mut c, _device) = load("Traktor Kontrol S5.hid.xml");
    settle(&mut c, 50);
    let mut idle = vec![0u8; 30];
    idle[0] = 0x01;
    let mut knobs = vec![0u8; 79];
    knobs[0] = 0x02;
    c.receive_report(&idle);
    c.receive_report(&knobs);
    tap(&mut c, &idle, 9, 0x10);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0, "left PLAY on deck A");
    tap(&mut c, &idle, 8, 0x04);
    tap(&mut c, &idle, 9, 0x10);
    assert_eq!(
        get(&reg, "[Channel3]", "play"),
        1.0,
        "left DECK, then PLAY on deck C"
    );
    let _ = c.take_reports();
    tap(&mut c, &idle, 23, 0x20);
    assert_eq!(get(&reg, "[Channel2]", "pfl"), 1.0, "CUE B");
    let reports = c.take_reports();
    assert!(
        reports
            .iter()
            .any(|r| r.len() == 74 && r[0] == 0x82 && r[2] == 0x7F),
        "CUE B lights"
    );
    let mut xfader = knobs.clone();
    xfader[25] = 0xF7;
    xfader[26] = 0x07;
    c.receive_report(&xfader);
    settle(&mut c, 10);
    assert_eq!(
        get(&reg, "[Master]", "crossfader"),
        0.0,
        "crossfader centred"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// The Komplete Kontrol M32 plays the instrument deck: its knobs turn the
/// instrument's knobs on the page shown, < > turn the page, the transport
/// runs the sequencer, PRESET asks for the next preset, and the 4-D encoder
/// browses the instruments the app lists.
#[test]
fn komplete_kontrol_m32_plays_the_instrument_deck() {
    let (reg, mut c, _device) = load("Native Instruments Komplete Kontrol M32.hid.xml");
    reg.get("[Channel2]", "live_input").unwrap().set(3.0);
    reg.get("[Instrument1]", "num_parameters")
        .unwrap()
        .set(20.0);
    opendj_controllers::texts::set_text("[Instrument1]", "name", "Test Synth");
    opendj_controllers::texts::set_text("[Instrument1]", "instruments", "One\nTwo\nThree");
    opendj_controllers::texts::set_text("[Instrument1]", "parameter1_name", "Cutoff");
    settle(&mut c, 100);
    let reports = c.take_reports();
    assert!(reports.iter().any(|r| r[0] == 0x80 && r.len() == 1 + 21));
    let screen: Vec<&Vec<u8>> = reports.iter().filter(|r| r[0] == 0xe0).collect();
    assert!(!screen.is_empty() && screen.iter().all(|r| r.len() == 1 + 8 + 256));

    // Knob counters start at 100; the first report only says where they are.
    let mut idle = vec![0u8; 32];
    idle[0] = 0x01;
    for k in 0..8 {
        idle[6 + 2 * k] = 100;
    }
    c.receive_report(&idle);
    let send = |c: &mut Controller, r: &[u8]| {
        c.receive_report(r);
        c.poll(Instant::now());
    };
    let tap = |c: &mut Controller, idle: &[u8], byte: usize, mask: u8| {
        let mut r = idle.to_vec();
        r[byte] |= mask;
        c.receive_report(&r);
        c.receive_report(idle);
        c.poll(Instant::now());
    };
    // Knob 1 on 50 counts: a tenth of the way up.
    let mut knob = idle.clone();
    knob[6] = 150;
    send(&mut c, &knob);
    assert!((get(&reg, "[Instrument1]", "parameter1") - 0.1).abs() < 1e-9);
    // Back down past 0 wraps: 150 -> 990 is 160 counts down, clamped at 0.
    knob[6] = (990u16 & 0xff) as u8;
    knob[7] = (990u16 >> 8) as u8;
    send(&mut c, &knob);
    assert_eq!(get(&reg, "[Instrument1]", "parameter1"), 0.0);
    let idle = knob;

    // > (S): page 2, where knob 1 turns knob 9.
    tap(&mut c, &idle, 2, 0x80);
    assert_eq!(get(&reg, "[Instrument1]", "knob_page"), 1.0);
    let mut knob = idle.clone();
    knob[6] = ((990u16 + 5) & 0xff) as u8;
    send(&mut c, &knob);
    assert!(get(&reg, "[Instrument1]", "parameter9") > 0.0);
    assert_eq!(get(&reg, "[Instrument1]", "parameter1"), 0.0);
    let idle = knob;

    // PLAY and STOP run the sequencer; PRESET DOWN asks for the next preset.
    tap(&mut c, &idle, 2, 0x02);
    assert_eq!(get(&reg, "[Instrument1]", "seq"), 1.0);
    tap(&mut c, &idle, 2, 0x08);
    assert_eq!(get(&reg, "[Instrument1]", "seq"), 0.0);
    tap(&mut c, &idle, 2, 0x20);
    assert_eq!(get(&reg, "[Instrument1]", "preset_next"), 1.0);
    // ARP and SCALE.
    tap(&mut c, &idle, 1, 0x04);
    assert_eq!(get(&reg, "[Instrument1]", "arp"), 1.0);
    tap(&mut c, &idle, 1, 0x02);
    assert_eq!(get(&reg, "[Instrument1]", "scale_mode"), 0.0);

    // The encoder: the first turn opens the instruments, the next picks the
    // second, the press loads it.
    for turn in [1u8, 2] {
        let mut r = idle.clone();
        r[24] = turn;
        send(&mut c, &r);
    }
    tap(
        &mut c,
        &{
            let mut r = idle.clone();
            r[24] = 2;
            r
        },
        5,
        0x02,
    );
    assert_eq!(get(&reg, "[Instrument1]", "instrument_load"), 2.0);
    settle(&mut c, 100);
    let _ = c.take_reports();
    assert!(c.errors().is_empty(), "{:#?}", c.errors());
    c.shutdown();
}
