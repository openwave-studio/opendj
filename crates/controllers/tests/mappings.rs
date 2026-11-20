//! Runs mappings for the DDJ-400, DDJ-FLX4, Xone K2 and MIDI Fighter Twister
//! against the real engine controls, feeding them MIDI as the hardware would,
//! and checks that every bundled mapping loads.

use std::sync::Arc;
use std::time::{Duration, Instant};

use opendj_control::ControlRegistry;
use opendj_controllers::{default_mappings_dir, list_mappings, Controller, Mapping};

fn load(file: &str) -> (Arc<ControlRegistry>, Controller) {
    let reg = ControlRegistry::new();
    // Creates every engine control; the engine itself isn't run.
    let _engine = opendj_engine::engine::Engine::new(&reg, 44_100.0);
    let path = default_mappings_dir().unwrap().join(file);
    let mapping = Mapping::load(&path).unwrap();
    let c = Controller::new("test", mapping, reg.clone()).unwrap();
    assert_eq!(
        c.errors(),
        Vec::<String>::new(),
        "{file}: errors during init"
    );
    (reg, c)
}

fn get(reg: &ControlRegistry, group: &str, key: &str) -> f64 {
    reg.get(group, key)
        .unwrap_or_else(|| panic!("{group},{key} missing"))
        .get()
}

/// Moves a 14-bit fader or knob from the bottom to the top and down to
/// `msb`, as a hand would: with pick-up, it only takes over once it passes
/// the control's value.
fn sweep14(c: &mut Controller, status: u8, msb_cc: u8, lsb_cc: u8, msb: u8) {
    for v in (0..=0x7F).chain((msb..0x7F).rev()) {
        c.receive(&[status, msb_cc, v]);
        c.receive(&[status, lsb_cc, 0x00]);
    }
}

/// Lets timers and connections run for a while.
fn settle(c: &mut Controller, ms: u64) {
    let start = Instant::now();
    for i in 0..=ms {
        c.poll(start + Duration::from_millis(i));
    }
}

#[test]
fn ddj_400_browse_keeps_signed_steps_and_distinct_press_directions() {
    let (reg, mut controller) = load("Pioneer-DDJ-400.midi.xml");
    let vertical = reg.free("[Library]", "MoveVertical");
    let forward = reg.free("[Library]", "MoveFocusForward");
    let backward = reg.free("[Library]", "MoveFocusBackward");

    controller.receive(&[0xB6, 0x40, 0x01]);
    controller.receive(&[0xB6, 0x40, 0x03]);
    controller.receive(&[0xB6, 0x40, 0x7F]);
    assert_eq!(vertical.get(), 3.0);
    vertical.set(0.0);
    controller.receive(&[0xB6, 0x40, 0x7E]);
    assert_eq!(vertical.get(), -2.0);

    controller.receive(&[0x96, 0x41, 0x7F]);
    controller.receive(&[0x96, 0x41, 0x00]);
    controller.receive(&[0x96, 0x42, 0x7F]);
    controller.receive(&[0x96, 0x42, 0x00]);
    assert_eq!(forward.get(), 1.0);
    assert_eq!(backward.get(), 1.0);
    assert!(controller.errors().is_empty());
}

#[test]
fn ddj_400_jog_bend_and_vinyl_mode() {
    let (reg, mut c) = load("Pioneer-DDJ-400.midi.xml");
    for channel in 0..2 {
        let group = format!("[Channel{}]", channel + 1);
        // Rim and untouched platter send signed ticks, never position nudges.
        for control in [0x21, 0x23] {
            for (value, ticks) in [(65, 1.0), (63, -1.0), (64, 0.0)] {
                reg.get(&group, "jog_bend").unwrap().set(0.0);
                c.receive(&[0xB0 + channel, control, value]);
                assert_eq!(get(&reg, &group, "jog_bend"), ticks);
                assert_eq!(get(&reg, &group, "jog"), 0.0);
            }
        }
        reg.get(&group, "vinyl_mode").unwrap().set(0.0);
        c.receive(&[0x90 + channel, 0x36, 127]);
        assert_eq!(get(&reg, &group, "scratch2_enable"), 0.0);
        c.receive(&[0xB0 + channel, 0x22, 66]);
        assert_eq!(get(&reg, &group, "jog_bend"), 2.0);
        c.receive(&[0x90 + channel, 0x36, 0]);
        reg.get(&group, "vinyl_mode").unwrap().set(1.0);
        c.receive(&[0x90 + channel, 0x36, 127]);
        assert_eq!(get(&reg, &group, "scratch2_enable"), 1.0);
        c.receive(&[0xB0 + channel, 0x22, 67]);
        assert_eq!(get(&reg, &group, "jog_bend"), 2.0);
        assert_eq!(get(&reg, &group, "jog"), 0.0);
        // The rim bends even while the deck is being scratched.
        reg.get(&group, "jog_bend").unwrap().set(0.0);
        c.receive(&[0xB0 + channel, 0x21, 61]);
        assert_eq!(get(&reg, &group, "jog_bend"), -3.0);
        c.receive(&[0x90 + channel, 0x36, 0]);
    }
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn ddj_400_slip_release_waits_for_wheel_stop() {
    for channel in 0..2 {
        let (reg, mut c) = load("Pioneer-DDJ-400.midi.xml");
        let group = format!("[Channel{}]", channel + 1);
        reg.get(&group, "vinyl_mode").unwrap().set(1.0);
        reg.get(&group, "track_loaded").unwrap().set(1.0);
        reg.get(&group, "play").unwrap().set(1.0);
        reg.get(&group, "slip_enabled").unwrap().set(1.0);
        c.receive(&[0x90 + channel, 0x36, 127]);
        c.receive(&[0xB0 + channel, 0x22, 60]);
        c.receive(&[0x90 + channel, 0x36, 0]);
        assert_eq!(get(&reg, &group, "scratch2_enable"), 1.0);
        // Timer creation uses wall time; feed movement over a real idle window.
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(20));
            // The platter spinning on after the hand let go.
            c.receive(&[0xB0 + channel, 0x23, 63]);
            c.poll(Instant::now());
            assert_eq!(get(&reg, &group, "scratch2_enable"), 1.0);
        }
        settle(&mut c, 100);
        assert_eq!(get(&reg, &group, "scratch2_enable"), 0.0);
        assert_eq!(c.errors(), Vec::<String>::new());
    }
}

#[test]
fn ddj_400_beat_fx() {
    let reg = ControlRegistry::new();
    let (mut engine, _) = opendj_engine::engine::Engine::new(&reg, 44_100.0);
    let path = default_mappings_dir()
        .unwrap()
        .join("Pioneer-DDJ-400.midi.xml");
    let mut c = Controller::new("test", Mapping::load(&path).unwrap(), reg.clone()).unwrap();
    let unit = "[EffectRack1_EffectUnit5]";
    let slot = "[EffectRack1_EffectUnit5_Effect1]";
    assert!(get(&reg, slot, "loaded_effect") > 0.0);
    c.receive(&[0x94, 0x47, 0x7f]);
    c.receive(&[0x94, 0x47, 0x00]);
    assert_eq!(get(&reg, slot, "enabled"), 1.0);
    assert_eq!(get(&reg, unit, "enabled"), 1.0);
    c.receive(&[0x94, 0x47, 0x7f]);
    c.receive(&[0x94, 0x47, 0x00]);
    assert_eq!(get(&reg, slot, "enabled"), 0.0);

    // CH SELECT sets the unit's route: deck 1, deck 2, the main mix.
    for (note, route, target) in [
        (0x10, 1.0, "Channel1"),
        (0x11, 2.0, "Channel2"),
        (0x14, 6.0, "Master"),
    ] {
        c.receive(&[0x94, note, 0x7f]);
        c.receive(&[0x94, note, 0x00]);
        engine.process(&mut [0.0; 128], 2);
        assert_eq!(get(&reg, unit, "route"), route);
        for route in ["Channel1", "Channel2", "Master"] {
            assert_eq!(
                get(&reg, unit, &format!("group_[{route}]_enable")),
                if route == target { 1.0 } else { 0.0 }
            );
        }
    }
    for value in (0..=127).rev() {
        c.receive(&[0xb4, 0x02, value]);
    }
    assert_eq!(get(&reg, slot, "meta"), 0.0);
    c.receive(&[0xb4, 0x02, 127]);
    assert_eq!(get(&reg, slot, "meta"), 1.0);
    // Each press must select another effect, including after an engine tick.
    assert_eq!(get(&reg, slot, "loaded_effect"), 1.0);
    // In the list's order: Echo, Echo Out, Filter Echo.
    for expected in [9.0, 18.0] {
        c.receive(&[0x94, 0x63, 0x7f]);
        c.receive(&[0x94, 0x63, 0x00]);
        engine.process(&mut [0.0; 128], 2);
        assert_eq!(get(&reg, slot, "loaded_effect"), expected);
    }
    c.receive(&[0x94, 0x64, 0x7f]);
    c.receive(&[0x94, 0x64, 0x00]);
    engine.process(&mut [0.0; 128], 2);
    assert_eq!(get(&reg, slot, "loaded_effect"), 9.0);
    c.receive(&[0x94, 0x47, 0x7f]);
    c.receive(&[0x94, 0x47, 0x00]);
    c.receive(&[0x94, 0x63, 0x7f]);
    c.receive(&[0x94, 0x63, 0x00]);
    c.receive(&[0x94, 0x43, 0x7f]);
    c.receive(&[0x94, 0x43, 0x00]);
    reg.get(slot, "beats").unwrap().set(0.5);
    c.receive(&[0x94, 0x4a, 0x7f]);
    c.receive(&[0x94, 0x4a, 0x00]);
    engine.process(&mut [0.0; 128], 2);
    assert_eq!(get(&reg, slot, "beats"), 0.25);
    c.receive(&[0x94, 0x4b, 0x7f]);
    engine.process(&mut [0.0; 128], 2);
    assert_eq!(get(&reg, slot, "beats"), 0.5);
    // Doubling stops at the effect's own limit.
    let mut limit = 0.0;
    for _ in 0..10 {
        c.receive(&[0x94, 0x4b, 0x00]);
        c.receive(&[0x94, 0x4b, 0x7f]);
        engine.process(&mut [0.0; 128], 2);
        let beats = get(&reg, slot, "beats");
        assert!(beats >= limit, "{beats} after {limit}");
        limit = beats;
    }
    assert!((1.0..=64.0).contains(&limit), "limit {limit}");
    // Leave the deck's separate FX controls alone.
    assert_eq!(
        get(&reg, "[EffectRack1_EffectUnit1_Effect1]", "enabled"),
        0.0
    );
    assert!(c.errors().is_empty(), "{:?}", c.errors());
}

#[test]
fn ddj_400_tempo_range_keeps_the_tempo() {
    let reg = ControlRegistry::new();
    let (mut engine, _input) = opendj_engine::engine::Engine::new(&reg, 44_100.0);
    let path = default_mappings_dir()
        .unwrap()
        .join("Pioneer-DDJ-400.midi.xml");
    let mut c = Controller::new("test", Mapping::load(&path).unwrap(), reg.clone()).unwrap();
    assert!(c.errors().is_empty(), "{:?}", c.errors());
    let ranges = [0.04, 0.06, 0.08, 0.10, 0.16, 0.24, 0.50, 1.00];
    // SHIFT + BEAT SYNC: note 0x60 on the deck's channel.
    for (status, group, other) in [
        (0x90, "[Channel1]", "[Channel2]"),
        (0x91, "[Channel2]", "[Channel1]"),
    ] {
        let other_range = get(&reg, other, "rateRange");
        for (i, old) in ranges.iter().copied().enumerate() {
            let next = ranges[(i + 1) % ranges.len()];
            reg.get(group, "rateRange").unwrap().set(old);
            // +1% speed at the old range.
            reg.get(group, "rate").unwrap().set(0.01 / old);
            c.receive(&[status, 0x60, 0x7f]);
            c.receive(&[status, 0x60, 0x00]);
            assert_eq!(get(&reg, group, "rate_range_set"), next);
            engine.process(&mut [0.0; 128], 2);
            assert_eq!(get(&reg, group, "rateRange"), next);
            assert!((get(&reg, group, "rate") * next - 0.01).abs() < 1e-12);
            assert_eq!(get(&reg, other, "rateRange"), other_range);
        }
        // A range not in the list steps up to the next one.
        reg.get(group, "rateRange").unwrap().set(0.25);
        c.receive(&[status, 0x60, 0x7f]);
        assert_eq!(get(&reg, group, "rate_range_set"), 0.50);
        c.receive(&[status, 0x60, 0x00]);
        // A second press before the audio thread applies the first.
        c.receive(&[status, 0x60, 0x7f]);
        assert_eq!(get(&reg, group, "rate_range_set"), 1.00);
        c.receive(&[status, 0x60, 0x00]);
        engine.process(&mut [0.0; 128], 2);
        assert_eq!(get(&reg, group, "rateRange"), 1.00);
    }
    assert!(c.errors().is_empty(), "{:?}", c.errors());
}

#[test]
fn ddj_flx4() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    let out = c.take_output();
    assert!(!out.is_empty(), "init lights the LEDs");

    // Play on deck 1, press and release.
    c.receive(&[0x90, 0x0B, 0x7F]);
    c.receive(&[0x90, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0);
    // The engine sets the indicator while processing audio; do it by hand here.
    reg.get("[Channel1]", "play_indicator").unwrap().set(1.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x90, 0x0B, 0x7F]),
        "play LED lit: {out:x?}"
    );

    // Channel fader, 14-bit (MSB 0x13, LSB 0x33). Pick-up: away from the
    // fader on screen, it waits.
    c.receive(&[0xB0, 0x13, 0x20]);
    c.receive(&[0xB0, 0x33, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "volume"), 1.0);
    sweep14(&mut c, 0xB0, 0x13, 0x33, 0x20);
    let v = get(&reg, "[Channel1]", "volume");
    assert!((v - 0.25).abs() < 0.02, "volume {v}");

    // Jog wheel: touching the platter enables scratching, turning it
    // moves the deck (CC 0x22 on channel 1 is the platter, 0x40 = still).
    c.receive(&[0x90, 0x36, 0x7F]);
    c.poll(Instant::now());
    assert_eq!(
        get(&reg, "[Channel1]", "scratch2_enable"),
        1.0,
        "touch enables scratch"
    );
    let start = Instant::now();
    for i in 0..20u64 {
        c.receive(&[0xB0, 0x22, 0x48]);
        c.poll(start + Duration::from_millis(i));
    }
    let s = get(&reg, "[Channel1]", "scratch2");
    assert!(s > 0.0, "scratching forward: {s}");
    c.receive(&[0x90, 0x36, 0x00]);
    settle(&mut c, 500);
    assert_eq!(
        get(&reg, "[Channel1]", "scratch2_enable"),
        0.0,
        "release stops scratch"
    );

    // Errors so far: none.
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn ddj_flx4_left_side_moves_to_deck_3() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    settle(&mut c, 10);
    c.take_output();
    reg.get("[Controller]", "deck3").unwrap().set(1.0);
    c.poll(Instant::now());

    // Play and the channel fader on the left now drive deck 3.
    c.receive(&[0x90, 0x0B, 0x7F]);
    c.receive(&[0x90, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel3]", "play"), 1.0);
    assert_eq!(get(&reg, "[Channel1]", "play"), 0.0);
    sweep14(&mut c, 0xB0, 0x13, 0x33, 0x20);
    let v = get(&reg, "[Channel3]", "volume");
    assert!((v - 0.25).abs() < 0.02, "deck 3 volume {v}");
    // The play LED follows deck 3.
    reg.get("[Channel3]", "play_indicator").unwrap().set(1.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x90, 0x0B, 0x7F]),
        "play LED lit for deck 3: {out:x?}"
    );

    // Back on deck 1, whose play LED is dark.
    reg.get("[Controller]", "deck3").unwrap().set(0.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(
        !out.iter().any(|m| m[..] == [0x90, 0x0B, 0x7F]),
        "play LED back to deck 1: {out:x?}"
    );
    c.receive(&[0x90, 0x0B, 0x7F]);
    c.receive(&[0x90, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0);
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn four_deck_mappings_ignore_the_deck_switches() {
    let (reg, mut c) = load("Allen and Heath Xone K2.midi.xml");
    settle(&mut c, 50);
    if let Some(l) = reg.get("[Controller]", "deck3") {
        l.set(1.0);
    }
    c.receive(&[0xBD, 0x10, 0x00]);
    c.receive(&[0xBD, 0x10, 0x01]);
    c.poll(Instant::now());
    assert!(get(&reg, "[Channel1]", "volume") < 0.05);
}

#[test]
fn xone_k2() {
    let (reg, mut c) = load("Allen and Heath Xone K2.midi.xml");
    settle(&mut c, 50);
    // MIDI channel 14 is the layout with deck A on the left column, two
    // effect units in the middle and deck B on the right. Faders are CC 16-19.
    c.receive(&[0xBD, 0x10, 0x00]);
    c.receive(&[0xBD, 0x10, 0x01]);
    c.poll(Instant::now());
    let v = get(&reg, "[Channel1]", "volume");
    assert!(v < 0.05, "deck A fader down: {v}");
    c.receive(&[0xBD, 0x13, 0x7F]);
    c.poll(Instant::now());
    assert!(get(&reg, "[Channel2]", "volume") > 0.95);
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn midi_fighter_twister_drives_the_effects() {
    let (reg, mut c) = load("DJ TechTools MIDI Fighter Twister.midi.xml");
    settle(&mut c, 50);
    let fx = "[EffectRack1_EffectUnit1_Effect1]";
    // Effect 1 of unit 1: knob (CC 0x11) is its meta. Turning it sends each
    // step; soft takeover drops one value early on, as in Mixxx.
    for v in 0x41..=0x50 {
        c.receive(&[0xB0, 0x11, v]);
    }
    c.poll(Instant::now());
    let meta = get(&reg, fx, "meta");
    assert!((meta - 79.0 / 126.0).abs() < 0.01, "meta {meta}");
    for v in 0x51..=0x7F {
        c.receive(&[0xB0, 0x11, v]);
        c.poll(Instant::now());
    }
    let meta = get(&reg, fx, "meta");
    assert!(meta > 0.99, "meta at the top {meta}");
    // Its push button (channel 2, same CC) turns the effect on.
    let before = get(&reg, fx, "enabled");
    c.receive(&[0xB1, 0x11, 0x7F]);
    c.receive(&[0xB1, 0x11, 0x00]);
    c.poll(Instant::now());
    assert_ne!(get(&reg, fx, "enabled"), before, "enable toggled");
    // Dry/wet of unit 1.
    for v in (0x20..=0x7F).rev() {
        c.receive(&[0xB0, 0x1D, v]);
    }
    c.poll(Instant::now());
    let mix = get(&reg, "[EffectRack1_EffectUnit1]", "mix");
    assert!((mix - 0.25).abs() < 0.01, "mix {mix}");
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// openDJ's own two-deck K2 mapping, with Latching Layers off on MIDI
/// channel 15.
#[test]
fn xone_k2_two_decks() {
    const K2: &str = "Allen and Heath Xone K2 - 2 Decks.opendj-mapping";
    let (reg, mut c) = load(K2);
    let press = |c: &mut Controller, note: u8| {
        c.receive(&[0x9E, note, 0x7F]);
        c.receive(&[0x8E, note, 0x7F]);
    };
    let set = |g: &str, k: &str, v: f64| reg.get(g, k).unwrap().set(v);

    // Volume faders are the inner columns, pitch the outer ones.
    c.receive(&[0xBE, 0x11, 0x00]);
    c.receive(&[0xBE, 0x11, 0x7F]);
    assert!(get(&reg, "[Channel1]", "volume") > 0.95);
    // A fader picks up once it passes the volume (full to start).
    c.receive(&[0xBE, 0x12, 0x7F]);
    c.receive(&[0xBE, 0x12, 0x00]);
    assert!(get(&reg, "[Channel2]", "volume") < 0.05);

    // The filter encoder moves a step a click, both ways, never jumping.
    let filter = |r: &ControlRegistry| {
        r.get("[QuickEffectRack1_[Channel1]]", "super1")
            .unwrap()
            .get_parameter()
    };
    let before = filter(&reg);
    c.receive(&[0xBE, 0x00, 0x01]);
    c.receive(&[0xBE, 0x00, 0x01]);
    let after = filter(&reg);
    assert!((after - before - 0.05).abs() < 1e-6, "{before} -> {after}");
    c.receive(&[0xBE, 0x00, 0x7F]);
    assert!((filter(&reg) - before - 0.025).abs() < 1e-6);

    // Shift + the top left encoder is the headphones, and the filter stays.
    let head = reg.get("[Master]", "headGain").unwrap().get_parameter();
    c.receive(&[0x9E, 0x0F, 0x7F]);
    c.receive(&[0xBE, 0x00, 0x7F]);
    let f = filter(&reg);
    c.receive(&[0x8E, 0x0F, 0x7F]);
    assert!(
        (filter(&reg) - before - 0.025).abs() < 1e-6,
        "filter moved with SHIFT: {f}"
    );
    assert!(reg.get("[Master]", "headGain").unwrap().get_parameter() < head);

    // SHIFT + a bottom encoder moves its own deck through the track
    // silently (a seek, not the jog wheel, which is heard while paused).
    set("[Channel2]", "playposition", 0.5);
    c.receive(&[0x9E, 0x0F, 0x7F]);
    c.receive(&[0xBE, 0x14, 0x01]);
    c.receive(&[0xBE, 0x14, 0x01]);
    c.receive(&[0xBE, 0x15, 0x7F]);
    c.receive(&[0x8E, 0x0F, 0x7F]);
    assert!((get(&reg, "[Channel1]", "playposition") - 0.01).abs() < 1e-9);
    assert!((get(&reg, "[Channel2]", "playposition") - 0.495).abs() < 1e-9);
    assert_eq!(get(&reg, "[Channel1]", "jog"), 0.0);
    assert_eq!(get(&reg, "[Channel2]", "jog"), 0.0);

    set("[Channel1]", "track_loaded", 1.0);
    // PLAY is inner for both decks: B for deck 1, O for deck 2. SYNC is outer.
    press(&mut c, 0x19);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0, "{:?}", c.errors());
    // SHIFT + PLAY goes back to the start and doesn't pause.
    c.receive(&[0x9E, 0x0F, 0x7F]);
    press(&mut c, 0x19);
    c.receive(&[0x8E, 0x0F, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "play"), 1.0);
    press(&mut c, 0x1B);
    assert_eq!(get(&reg, "[Channel2]", "sync_enabled"), 1.0);

    // Lights: deck 1's PLAY green while it plays (note + 72), deck 2's
    // SYNC red, the beat jumps amber (note + 36) on a loaded deck.
    c.take_output();
    settle(&mut c, 100);
    let out = c.take_output();
    let sent = |m: [u8; 3]| out.iter().any(|o| o.as_slice() == m);
    assert!(sent([0x9E, 0x19 + 72, 0x7F]), "PLAY green: {out:02X?}");
    assert!(sent([0x9E, 0x1B, 0x7F]), "SYNC red");
    assert!(
        sent([0x9E, 0x1C + 36, 0x7F]) && sent([0x9E, 0x1D + 36, 0x7F]),
        "jumps amber"
    );
    assert!(!sent([0x9E, 0x1E + 36, 0x7F]), "deck 2 has no track");

    // Paused with a track, PLAY blinks green.
    press(&mut c, 0x19);
    let start = Instant::now();
    let mut seen = Vec::new();
    for ms in (0..1200).step_by(20) {
        c.poll(start + Duration::from_millis(ms));
        seen.extend(c.take_output());
    }
    assert!(seen.iter().any(|o| o.as_slice() == [0x9E, 0x19 + 72, 0x7F]));
    assert!(seen.iter().any(|o| o.as_slice() == [0x9E, 0x19, 0x00]));

    // In stems mode the EQ knobs turn the stems: TREBLE the hi-hats.
    reg.get("[Mixer]", "knob_mode").unwrap().set(1.0);
    for v in (0x10..=0x40).rev() {
        c.receive(&[0xBE, 0x05, v]);
    }
    let hihat = reg
        .get("[Channel1_Stem5]", "volume")
        .unwrap()
        .get_parameter();
    assert!(
        (hihat - 0x10 as f64 / 127.0).abs() < 0.01,
        "hi-hats {hihat}"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// openDJ's K2 mapping for the Mixer deck: a column a channel.
#[test]
fn xone_k2_mixer() {
    const K2: &str = "Allen and Heath Xone K2 - Mixer.opendj-mapping";
    let (reg, mut c) = load(K2);
    let press = |c: &mut Controller, note: u8| {
        c.receive(&[0x9E, note, 0x7F]);
        c.receive(&[0x8E, note, 0x7F]);
    };

    // The third fader is Ch 3's volume (it picks up from where it is).
    for v in (0..=0x7F).chain((0x40..0x7F).rev()) {
        c.receive(&[0xBE, 0x12, v]);
    }
    let vol = reg.get("[Auxiliary3]", "volume").unwrap().get_parameter();
    assert!((vol - 0x40 as f64 / 127.0).abs() < 0.01, "volume {vol}");

    // The top encoder trims a step a click; pressing it goes back to 0 dB.
    let trim = |r: &ControlRegistry| r.get("[Auxiliary2]", "pregain").unwrap().get_parameter();
    let before = trim(&reg);
    c.receive(&[0xBE, 0x01, 0x01]);
    assert!(trim(&reg) > before);
    press(&mut c, 0x35);
    assert_eq!(get(&reg, "[Auxiliary2]", "pregain"), 1.0);

    // The first knob is PAN, picked up as it passes the centre.
    for v in (0x30..=0x50).chain((0..0x50).rev()) {
        c.receive(&[0xBE, 0x04, v]);
    }
    assert_eq!(get(&reg, "[Auxiliary1]", "pan"), -1.0);

    // The knob buttons are FX (on from the start, so a press takes the
    // channel out of the deck's effects), S and cue; the first matrix row
    // is on/off.
    let fx = get(&reg, "[Auxiliary4]", "fx");
    press(&mut c, 0x33);
    press(&mut c, 0x2F);
    press(&mut c, 0x2B);
    assert_eq!(get(&reg, "[Auxiliary4]", "fx"), 1.0 - fx);
    assert_eq!(get(&reg, "[Auxiliary4]", "solo"), 1.0);
    assert_eq!(get(&reg, "[Auxiliary4]", "pfl"), 1.0);
    let on = get(&reg, "[Auxiliary1]", "main_mix");
    press(&mut c, 0x24);
    assert_eq!(get(&reg, "[Auxiliary1]", "main_mix"), 1.0 - on);

    // Lights: Ch 4's S amber, its cue green; Ch 1's meter while it plays.
    reg.get("[Auxiliary1]", "vu_meter").unwrap().set(0.6);
    c.take_output();
    settle(&mut c, 100);
    let out = c.take_output();
    let sent = |m: [u8; 3]| out.iter().any(|o| o.as_slice() == m);
    assert!(sent([0x9E, 0x2F + 36, 0x7F]), "S amber: {out:02X?}");
    assert!(sent([0x9E, 0x2B + 72, 0x7F]), "cue green");
    assert!(
        sent([0x9E, 0x18 + 72, 0x7F]) && sent([0x9E, 0x1C + 72, 0x7F]),
        "meter"
    );
    assert!(!sent([0x9E, 0x20 + 36, 0x7F]), "no amber below 80%");
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn every_bundled_mapping_loads_and_takes_its_messages() {
    let dir = default_mappings_dir().unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.to_string_lossy();
            name.ends_with(".midi.xml") || name.ends_with(".opendj-mapping")
        })
        .collect();
    files.sort();
    assert_eq!(
        list_mappings(&dir).len(),
        files.len(),
        "every mapping is listed"
    );
    for path in files {
        let mapping = Mapping::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for s in &mapping.scripts {
            assert!(
                s.path.exists(),
                "{}: missing {}",
                path.display(),
                s.path.display()
            );
        }
        let messages: Vec<(u8, u8)> = mapping.inputs.keys().copied().collect();
        let reg = ControlRegistry::new();
        let _engine = opendj_engine::engine::Engine::new(&reg, 44_100.0);
        let mut c = Controller::new("test", mapping, reg)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // Some of Mixxx's mappings have script errors of their own; what
        // matters here is that nothing they do takes the host down.
        let start = Instant::now();
        for (i, &(status, control)) in messages.iter().enumerate() {
            for value in [0x7F, 0x00] {
                c.receive(&[status, control, value]);
            }
            c.poll(start + Duration::from_millis(i as u64));
        }
        settle(&mut c, 100);
        c.shutdown();
    }
}

/// A one-off mapping with its own script, in a fresh folder.
fn scripted(name: &str, xml_controls: &str, script: &str) -> (Arc<ControlRegistry>, Controller) {
    let dir = std::env::temp_dir().join(format!("opendj-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let common = "common-controller-scripts.js";
    std::fs::copy(
        default_mappings_dir().unwrap().join(common),
        dir.join(common),
    )
    .unwrap();
    std::fs::write(dir.join("test.js"), script).unwrap();
    let xml = format!(
        r#"<MixxxControllerPreset><info><name>{name}</name></info><controller>
        <scriptfiles><file filename="test.js" functionprefix="T"/></scriptfiles>
        <controls>{xml_controls}</controls><outputs/></controller></MixxxControllerPreset>"#
    );
    let mapping = Mapping::parse(&xml, &dir).unwrap();
    let reg = ControlRegistry::new();
    let _engine = opendj_engine::engine::Engine::new(&reg, 44_100.0);
    let c = Controller::new("test", mapping, reg.clone()).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    (reg, c)
}

#[test]
fn trigger_runs_connections_after_the_script_returns() {
    // As in Mixxx, where engine.trigger is queued: callbacks may use what
    // init sets up after triggering.
    let (_reg, c) = scripted(
        "trigger",
        "",
        r#"var T = {};
        T.init = function() {
            engine.makeConnection("[Channel1]", "play", function(v) { T.lights.push(v); });
            engine.trigger("[Channel1]", "play");
            T.lights = [];
        };"#,
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn input_handlers_get_their_messages() {
    let (reg, mut c) = scripted(
        "inputhandler",
        r#"<control><group>[Channel1]</group><key>T.stop</key><status>0x90</status>
        <midino>0x01</midino><options><script-binding/></options></control>"#,
        r#"var T = {};
        T.init = function() {
            T.handler = midi.makeInputHandler(0x91, 0x10, function(channel, control, value, status) {
                engine.setValue("[Channel1]", "pregain", channel + control / 100 + value / 1000);
            });
        };
        T.stop = function() { T.handler.disconnect(); };"#,
    );
    c.receive(&[0x91, 0x10, 0x05]);
    assert!((get(&reg, "[Channel1]", "pregain") - 1.165).abs() < 1e-9);
    // Disconnected, it gets nothing more.
    c.receive(&[0x90, 0x01, 0x7F]);
    reg.get("[Channel1]", "pregain").unwrap().set(1.0);
    c.receive(&[0x91, 0x10, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "pregain"), 1.0);
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn ddj_flx4_mixer_pads_and_eq_modes() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    settle(&mut c, 50);
    // Tempo fader pushed from the middle to the top: rate +1, which
    // rate_dir turns into slower (soft takeover wants it moved, not set).
    for msb in (0..=0x40).rev() {
        c.receive(&[0xB0, 0x00, msb]);
        c.receive(&[0xB0, 0x20, 0x00]);
    }
    assert_eq!(get(&reg, "[Channel1]", "rate"), 1.0);
    // Master and headphones level, and the headphones mix.
    for (msb, lsb, key) in [(0x08, 0x28, "gain"), (0x0D, 0x2D, "headGain")] {
        sweep14(&mut c, 0xB6, msb, lsb, 0x40);
        let v = get(&reg, "[Master]", key);
        assert!((v - 0.5).abs() < 0.01, "{key} {v}");
    }
    sweep14(&mut c, 0xB6, 0x0C, 0x2C, 0x00);
    assert_eq!(get(&reg, "[Master]", "headMix"), -1.0);
    // Its centre detent is half cue, half main.
    for msb in 0x01..=0x40 {
        c.receive(&[0xB6, 0x0C, msb]);
        c.receive(&[0xB6, 0x2C, 0x00]);
    }
    let centre = get(&reg, "[Master]", "headMix");
    assert!(centre.abs() < 0.01, "centre {centre}");
    // Hot cue LEDs follow hotcue_N_status.
    c.take_output();
    reg.get("[Channel1]", "hotcue_1_status").unwrap().set(1.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(out.iter().any(|m| m[..] == [0x97, 0x00, 0x7F]), "{out:x?}");
    // 1/2X and 2X press loop_halve / loop_double, so the loop size box
    // follows even with no loop on.
    c.receive(&[0x90, 0x51, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "loop_halve"), 1.0);
    c.receive(&[0x90, 0x51, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "loop_halve"), 0.0);
    c.receive(&[0x90, 0x53, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "loop_double"), 1.0);
    // RELOOP/EXIT starts a new loop of the set size (or ends one).
    c.receive(&[0x90, 0x4D, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "beatloop_activate"), 1.0);
    // BEAT FX: FX SELECT steps the Beat FX unit's effect, BEAT < > halve
    // and double its length, ON/OFF turns it on, SHIFT + ON/OFF releases it.
    let fx = "[EffectRack1_EffectUnit5_Effect1]";
    let first = get(&reg, fx, "loaded_effect");
    c.receive(&[0x94, 0x63, 0x7F]);
    c.receive(&[0x94, 0x63, 0x00]);
    let pushed = reg.get(fx, "next_effect").unwrap();
    assert!(
        pushed.version() > 0 && !pushed.get_bool(),
        "next_effect pressed: {first}"
    );
    // BEAT < > push the slot's beats_halve / beats_double, which keep the
    // length within the effect's limits.
    c.receive(&[0x94, 0x4B, 0x7F]);
    let double = reg.get(fx, "beats_double").unwrap();
    assert!(double.version() > 0 && !double.get_bool());
    c.receive(&[0x94, 0x4A, 0x7F]);
    let halve = reg.get(fx, "beats_halve").unwrap();
    assert!(halve.version() > 0 && !halve.get_bool());
    for v in 0..=0x40 {
        c.receive(&[0xB4, 0x02, v]);
    }
    assert!((get(&reg, fx, "meta") - 0.5).abs() < 0.01);
    c.receive(&[0x94, 0x47, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);
    let release = reg.get("[EffectRack1_EffectUnit5]", "release").unwrap();
    let before = release.version();
    c.receive(&[0x94, 0x43, 0x7F]);
    assert!(release.version() > before, "SHIFT + ON/OFF releases");
    // Pad mode buttons ask deck 2 to open its third pad tab.
    c.receive(&[0x91, 0x20, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "pad_tab"), 2.0);
    // SHIFT + left LOAD asks for the Linked tab; each press counts.
    reg.free("[Library]", "ShowLinked");
    c.receive(&[0x96, 0x58, 0x7F]);
    c.receive(&[0x96, 0x58, 0x00]);
    assert_eq!(get(&reg, "[Library]", "ShowLinked"), 1.0);
    // In stems mode the low EQ knob turns the kick stem.
    reg.get("[Mixer]", "knob_mode").unwrap().set(1.0);
    sweep14(&mut c, 0xB0, 0x0F, 0x2F, 0x10);
    let kick = reg
        .get("[Channel1_Stem1]", "volume")
        .unwrap()
        .get_parameter();
    assert!((kick - 0.125).abs() < 0.01, "kick {kick}");
    reg.get("[Mixer]", "knob_mode").unwrap().set(2.0);
    sweep14(&mut c, 0xB6, 0x17, 0x37, 0x40);
    let hpf = reg
        .get("[Sculpt_[Channel1]]", "hpf")
        .unwrap()
        .get_parameter();
    assert!((hpf - 0.5).abs() < 0.01, "hpf {hpf}");
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn ddj_flx4_pad_modes() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    settle(&mut c, 50);
    // Each of the eight pad mode buttons opens its tab on the screen by
    // place: 1-4, and 5-8 with SHIFT.
    for (button, tab) in [
        (0x1B, 0.0),
        (0x1E, 1.0),
        (0x20, 2.0),
        (0x22, 3.0),
        (0x69, 4.0),
        (0x6B, 5.0),
        (0x6D, 6.0),
        (0x6F, 7.0),
    ] {
        c.receive(&[0x90, button, 0x7F]);
        c.receive(&[0x90, button, 0x00]);
        assert_eq!(get(&reg, "[Channel1]", "pad_tab"), tab, "button {button:x}");
    }
    // KEYBOARD pads press keyboard_N_activate while held.
    c.receive(&[0x97, 0x41, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "keyboard_2_activate"), 1.0);
    c.receive(&[0x97, 0x41, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "keyboard_2_activate"), 0.0);
    // The pad of the key shift playing is lit.
    c.take_output();
    reg.get("[Channel1]", "pitch_adjust").unwrap().set(-4.0);
    settle(&mut c, 5);
    let out = c.take_output();
    assert!(out.iter().any(|m| m[..] == [0x97, 0x44, 0x7F]), "{out:x?}");
    // KEYBOARD, and KEYBOARD again: the stems pads, laid out as on screen:
    // pad 1 mutes the vocals (stem 4), pad 4 plays them alone (Acapella).
    reg.get("[Channel1]", "stem_count").unwrap().set(4.0);
    c.receive(&[0x90, 0x69, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "pad_tab"), 4.0);
    c.receive(&[0x90, 0x69, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "pad_slot_5"), 4.0);
    c.receive(&[0x97, 0x40, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    c.receive(&[0x97, 0x43, 0x7F]);
    let mutes: Vec<f64> = (1..=4)
        .map(|s| get(&reg, &format!("[Channel1_Stem{s}]"), "mute"))
        .collect();
    assert_eq!(mutes, [1.0, 1.0, 1.0, 0.0]);
    // Acapella again brings every stem back; Instrumental (pad 8) mutes the vocals.
    c.receive(&[0x97, 0x43, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem1]", "mute"), 0.0);
    c.receive(&[0x97, 0x47, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    assert_eq!(get(&reg, "[Channel1_Stem2]", "mute"), 0.0);
    // Instrumental again; then Stems FX (pad 7) switches the pads to echo
    // out: the vocals' pad sends them into the stem effect and mutes them,
    // and a second press brings them back dry.
    c.receive(&[0x97, 0x47, 0x7F]);
    let fx = "[QuickEffectRack1_[Channel1_Stem4]]";
    c.receive(&[0x97, 0x46, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx_mode"), 1.0);
    c.receive(&[0x97, 0x40, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);
    c.receive(&[0x97, 0x40, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 0.0);
    assert_eq!(get(&reg, fx, "enabled"), 0.0);
    // Instrumental echoes the vocals out too.
    c.receive(&[0x97, 0x47, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    c.receive(&[0x97, 0x47, 0x7F]);
    c.receive(&[0x97, 0x46, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx_mode"), 0.0);
    // And once more, back to the keyboard.
    c.receive(&[0x90, 0x69, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "pad_slot_5"), 7.0);
    // BEAT LOOP pad 1 is a quarter-beat loop.
    c.receive(&[0x97, 0x60, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "beatloop_0.25_toggle"), 1.0);
    // PAD FX2 pad 4 holds a brake on deck 2's pad FX unit.
    let slot = "[EffectRack1_EffectUnit7_Effect1]";
    c.receive(&[0x99, 0x53, 0x7F]);
    assert_eq!(get(&reg, slot, "loaded_effect"), 10.0);
    assert_eq!(get(&reg, slot, "enabled"), 1.0);
    assert_eq!(
        get(&reg, "[EffectRack1_EffectUnit7]", "group_[Channel2]_enable"),
        1.0
    );
    c.receive(&[0x99, 0x53, 0x00]);
    assert_eq!(get(&reg, slot, "enabled"), 0.0);
    // PAD FX1 pad 1 is an echo.
    c.receive(&[0x97, 0x10, 0x7F]);
    assert_eq!(
        get(&reg, "[EffectRack1_EffectUnit6_Effect1]", "loaded_effect"),
        1.0
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn ddj_flx4_sampler_pads_follow_the_page() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    settle(&mut c, 50);
    // Page 1: the left and right pads play slots 1-8 alike.
    c.receive(&[0x97, 0x30, 0x7F]);
    assert_eq!(get(&reg, "[Sampler1]", "cue_gotoandplay"), 1.0);
    c.receive(&[0x97, 0x30, 0x00]);
    c.receive(&[0x99, 0x34, 0x7F]);
    assert_eq!(get(&reg, "[Sampler5]", "cue_gotoandplay"), 1.0);
    c.receive(&[0x99, 0x34, 0x00]);
    // The right deck on page 2: its first pad plays slot 9, and its LEDs
    // show slots 9-16, which are empty.
    c.take_output();
    reg.get("[Channel2]", "sampler_page").unwrap().set(1.0);
    settle(&mut c, 5);
    let out = c.take_output();
    assert!(out.iter().any(|m| m[..] == [0x99, 0x30, 0x00]), "{out:x?}");
    // Empty, so the pad asks for the track selected in the library.
    c.receive(&[0x99, 0x30, 0x7F]);
    assert_eq!(get(&reg, "[Sampler9]", "LoadSelectedTrack"), 1.0);
    c.receive(&[0x99, 0x30, 0x00]);
    reg.get("[Sampler9]", "track_loaded").unwrap().set(1.0);
    settle(&mut c, 5);
    let out = c.take_output();
    assert!(out.iter().any(|m| m[..] == [0x99, 0x30, 0x7F]), "{out:x?}");
    c.receive(&[0x99, 0x30, 0x7F]);
    assert_eq!(get(&reg, "[Sampler9]", "cue_gotoandplay"), 1.0);
    assert_eq!(get(&reg, "[Sampler1]", "cue_gotoandplay"), 0.0);
}

#[test]
fn ddj_flx4_pads_follow_the_tabs() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    settle(&mut c, 50);
    reg.get("[Channel1]", "stem_count").unwrap().set(4.0);
    // STEMS in tab 4: SAMPLER opens it, and its pads mute stems (pad 1 the vocals).
    reg.get("[Channel1]", "pad_slot_4").unwrap().set(4.0);
    settle(&mut c, 5);
    c.receive(&[0x90, 0x22, 0x7F]);
    c.receive(&[0x90, 0x22, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "pad_tab"), 3.0);
    c.take_output();
    c.receive(&[0x97, 0x30, 0x7F]);
    c.receive(&[0x97, 0x30, 0x00]);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    assert_eq!(get(&reg, "[Sampler1]", "cue_gotoandplay"), 0.0);
    // The stem's light shows on the SAMPLER pad.
    settle(&mut c, 5);
    let out = c.take_output();
    assert!(out.iter().any(|m| m[..] == [0x97, 0x30, 0x00]), "{out:x?}");
    // LOOP ROLL in tab 1: HOT CUE pads roll while held.
    reg.get("[Channel1]", "pad_slot_1").unwrap().set(9.0);
    settle(&mut c, 5);
    c.receive(&[0x90, 0x1B, 0x7F]);
    c.receive(&[0x97, 0x02, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "beatlooproll_0.25_activate"), 1.0);
    c.receive(&[0x97, 0x02, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "beatlooproll_0.25_activate"), 0.0);
    assert_eq!(get(&reg, "[Channel1]", "hotcue_3_activate"), 0.0);
    // Back to hot cues.
    reg.get("[Channel1]", "pad_slot_1").unwrap().set(0.0);
    settle(&mut c, 5);
    c.receive(&[0x97, 0x02, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "hotcue_3_activate"), 1.0);
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// The controllers openDJ looks after start without script errors.
#[test]
fn looked_after_mappings_start_cleanly() {
    for c in opendj_controllers::support::all() {
        // HID mappings need a device to answer them: see tests/hid.rs.
        if let Some(file) = c.file.filter(|f| !f.ends_with(".hid.xml")) {
            let (_reg, mut c) = load(&file);
            settle(&mut c, 50);
            assert_eq!(c.errors(), Vec::<String>::new(), "{file}");
            c.shutdown();
        }
    }
}

/// The Mixtrack Platinum FX's jog wheel screens get the deck's BPM and
/// time over SysEx.
#[test]
fn mixtrack_platinum_fx_screens() {
    let (reg, mut c) = load("Numark Mixtrack Platinum FX.midi.xml");
    settle(&mut c, 20);
    c.take_output();
    reg.get("[Channel1]", "duration").unwrap().set(200.0);
    reg.get("[Channel1]", "bpm").unwrap().set(128.0);
    reg.get("[Channel1]", "playposition").unwrap().set(0.25);
    settle(&mut c, 20);
    let out = c.take_output();
    let screen = |kind: u8| {
        out.iter()
            .any(|m| m.starts_with(&[0xF0, 0x00, 0x20, 0x7F, 0x01, kind]))
    };
    assert!(screen(0x01), "BPM: {out:02X?}");
    assert!(screen(0x03), "duration");
    assert!(screen(0x04), "time");
}

/// The V7's mapping is from Mixxx 1.11: its FX, Reloop/Exit and Tap use
/// controls Mixxx later dropped, which openDJ keeps for it.
#[test]
fn numark_v7_reaches_its_old_controls() {
    let (reg, mut c) = load("Numark V7.midi.xml");
    settle(&mut c, 4000);
    let press = |c: &mut Controller, note: u8| {
        c.receive(&[0x90, note, 0x7F]);
        c.receive(&[0x90, note, 0x00]);
    };
    // FX ON for deck A.
    press(&mut c, 0x52);
    assert_eq!(get(&reg, "[Channel1]", "flanger"), 1.0);
    // FX AMOUNT for deck A: coarse then fine.
    c.receive(&[0xB0, 0x57, 0x40]);
    c.receive(&[0xB0, 0x77, 0x00]);
    let depth = get(&reg, "[Flanger]", "lfoDepth");
    assert!((depth - 0.5).abs() < 0.02, "{depth}");
    // RELOOP/EXIT on deck B reaches reloop_toggle.
    let reloop = reg.get("[Channel2]", "reloop_toggle").unwrap();
    let before = reloop.version();
    press(&mut c, 0x45);
    assert_ne!(reloop.version(), before);
    // TAP for deck A.
    let tap = reg.get("[Channel1]", "bpm_tap").unwrap();
    let before = tap.version();
    press(&mut c, 0x1E);
    assert_ne!(tap.version(), before);
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn pickup_shows_where_the_hardware_is() {
    use opendj_control::ConfigKey;
    use opendj_controllers::pickup;
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    c.publish_positions();
    settle(&mut c, 10);
    // Deck 2's channel fader brought to the middle: it picked up.
    sweep14(&mut c, 0xB1, 0x13, 0x33, 0x40);
    let key = ConfigKey::new("[Channel2]", "volume");
    let hw = pickup::hardware_position(&key).unwrap();
    assert!((hw - 0.5).abs() < 0.01, "hardware {hw}");
    // Moved up on screen: the hardware stays where it is and does nothing
    // until it reaches the fader again.
    std::thread::sleep(Duration::from_millis(60));
    reg.get("[Channel2]", "volume").unwrap().set(1.0);
    c.receive(&[0xB1, 0x13, 0x41]);
    c.receive(&[0xB1, 0x33, 0x00]);
    assert_eq!(get(&reg, "[Channel2]", "volume"), 1.0);
    assert!(pickup::hardware_position(&key).unwrap() < 0.6);
    // The crossfader always follows the hardware, and needs no marker.
    c.receive(&[0xB6, 0x1F, 0x00]);
    c.receive(&[0xB6, 0x3F, 0x00]);
    assert_eq!(get(&reg, "[Master]", "crossfader"), -1.0);
    assert!(pickup::hardware_position(&ConfigKey::new("[Master]", "crossfader")).is_none());
    // Unplugged: its positions go.
    c.shutdown();
    assert!(pickup::hardware_position(&key).is_none());
}

#[test]
fn ddj_flx6_four_decks_and_mixer() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX6.midi.xml");
    settle(&mut c, 50);
    c.take_output();

    // Each deck has its own channel: PLAY on channel 3 plays deck 3.
    c.receive(&[0x92, 0x0B, 0x7F]);
    c.receive(&[0x92, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel3]", "play"), 1.0);
    assert_eq!(get(&reg, "[Channel1]", "play"), 0.0);
    reg.get("[Channel3]", "play_indicator").unwrap().set(1.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x92, 0x0B, 0x7F]),
        "deck 3 play LED on its own channel: {out:x?}"
    );

    // The mixer has all four channels, whichever deck a side plays.
    for (status, deck) in [(0xB0, 1), (0xB1, 2), (0xB2, 3), (0xB3, 4)] {
        sweep14(&mut c, status, 0x13, 0x33, 0x20);
        let v = get(&reg, &format!("[Channel{deck}]"), "volume");
        assert!((v - 0.25).abs() < 0.02, "deck {deck} volume {v}");
    }
    sweep14(&mut c, 0xB6, 0x1A, 0x3A, 0x10);
    let filter = reg
        .get("[QuickEffectRack1_[Channel4]]", "super1")
        .unwrap()
        .get_parameter();
    assert!((filter - 0.125).abs() < 0.01, "deck 4 filter {filter}");
    c.receive(&[0x96, 0x49, 0x7F]);
    c.receive(&[0x96, 0x49, 0x00]);

    // Deck 4's pads: hot cue 1, and its pad mode buttons open its tabs.
    c.receive(&[0x9D, 0x00, 0x7F]);
    assert_eq!(get(&reg, "[Channel4]", "hotcue_1_activate"), 1.0);
    c.receive(&[0x9D, 0x00, 0x00]);
    c.receive(&[0x93, 0x20, 0x7F]);
    assert_eq!(get(&reg, "[Channel4]", "pad_tab"), 2.0);
    assert_ne!(get(&reg, "[Channel2]", "pad_tab"), 2.0);

    // Deck 3's jog scratches deck 3.
    c.receive(&[0x92, 0x36, 0x7F]);
    c.poll(Instant::now());
    assert_eq!(get(&reg, "[Channel3]", "scratch2_enable"), 1.0);
    c.receive(&[0x92, 0x36, 0x00]);

    // MASTER pins the sync leader, and again lets go.
    c.receive(&[0x91, 0x5C, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "sync_leader"), 2.0);
    c.receive(&[0x91, 0x5C, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "sync_leader"), 0.0);

    // BEAT FX: FX2's slots drive the Beat FX too; CH SELECT picks a deck
    // or, on MST, the main mix.
    let unit = "[EffectRack1_EffectUnit5]";
    let fx = "[EffectRack1_EffectUnit5_Effect1]";
    c.receive(&[0x95, 0x48, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);
    c.receive(&[0x94, 0x1E, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 3.0);
    c.receive(&[0x94, 0x14, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 6.0);

    // MERGE FX follows the deck the side plays: after DECK 3, the left
    // knob drives deck 3's FX unit.
    c.receive(&[0x92, 0x3C, 0x7F]);
    for _ in 0..16 {
        c.receive(&[0xB4, 0x08, 0x02]);
    }
    let super1 = get(&reg, "[EffectRack1_EffectUnit3]", "super1");
    assert!((super1 - 0.5).abs() < 0.01, "deck 3 FX super {super1}");
    c.receive(&[0x94, 0x2E, 0x7F]);
    assert_eq!(
        get(&reg, "[EffectRack1_EffectUnit3_Effect1]", "enabled"),
        1.0
    );
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0xB4, 0x10, 0x7F]),
        "MERGE FX lit: {out:x?}"
    );

    // The jog ring shows the playhead.
    reg.get("[Channel1]", "track_loaded").unwrap().set(1.0);
    reg.get("[Channel1]", "duration").unwrap().set(180.0);
    reg.get("[Channel1]", "playposition")
        .unwrap()
        .set(0.5 / 180.0);
    settle(&mut c, 100);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[0] == 0xBB && m[1] == 0x00 && m[2] > 0),
        "jog ring moved: {out:x?}"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn ddj_grv6_decks_beat_fx_and_groove_circuit() {
    let (reg, mut c) = load("Pioneer-DDJ-GRV6.midi.xml");
    settle(&mut c, 50);
    c.take_output();

    // Each deck has its own channel, as on the FLX6.
    c.receive(&[0x93, 0x0B, 0x7F]);
    c.receive(&[0x93, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel4]", "play"), 1.0);
    for (status, deck) in [(0xB0, 1), (0xB3, 4)] {
        sweep14(&mut c, status, 0x13, 0x33, 0x20);
        let v = get(&reg, &format!("[Channel{deck}]"), "volume");
        assert!((v - 0.25).abs() < 0.02, "deck {deck} volume {v}");
    }

    // The STEMS button opens the stems pads, SHIFT + STEMS the pad FX.
    assert_eq!(get(&reg, "[Channel1]", "pad_slot_2"), 4.0);
    assert_eq!(get(&reg, "[Channel1]", "pad_slot_6"), 1.0);
    c.receive(&[0x90, 0x1E, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "pad_tab"), 1.0);

    // MASTER TEMPO, SLIP and KEY SYNC.
    let keylock = get(&reg, "[Channel2]", "keylock");
    c.receive(&[0x91, 0x1A, 0x7F]);
    c.receive(&[0x91, 0x1A, 0x00]);
    assert_ne!(get(&reg, "[Channel2]", "keylock"), keylock);
    let slip = get(&reg, "[Channel2]", "slip_enabled");
    c.receive(&[0x91, 0x40, 0x7F]);
    c.receive(&[0x91, 0x40, 0x00]);
    assert_ne!(get(&reg, "[Channel2]", "slip_enabled"), slip);
    let vinyl = get(&reg, "[Channel2]", "vinyl_mode");
    c.receive(&[0x91, 0x17, 0x7F]);
    assert_ne!(get(&reg, "[Channel2]", "vinyl_mode"), vinyl);

    // SHIFT + BEAT SYNC pins the sync leader.
    c.receive(&[0x91, 0x5C, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "sync_leader"), 2.0);

    // BEAT FX: the lever picks an effect, CH SELECT where it goes, ON/OFF.
    let unit = "[EffectRack1_EffectUnit5]";
    let fx = "[EffectRack1_EffectUnit5_Effect1]";
    c.receive(&[0x94, 0x25, 0x7F]);
    assert_eq!(get(&reg, fx, "loaded_effect"), 2.0);
    c.receive(&[0x94, 0x12, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 3.0);
    c.receive(&[0x94, 0x14, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 6.0);
    c.receive(&[0x94, 0x47, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);

    // SOUND COLOR FX turns the COLOR knobs' effect on for all four.
    let cfx = |d: u8| get(&reg, &format!("[QuickEffectRack1_[Channel{d}]]"), "enabled");
    let before = cfx(1);
    c.receive(&[0x96, 0x00, 0x7F]);
    assert_ne!(cfx(1), before);
    assert_eq!(cfx(4), cfx(1));

    // GROOVE CIRCUIT on a track with five stems: DRUM SWAP 3 takes out
    // the kick and hi-hats and lights; DRUM RELEASE echoes them out while
    // held; GAIN sets their level.
    reg.get("[Channel1]", "stem_count").unwrap().set(5.0);
    c.receive(&[0x90, 0x02, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem1]", "mute"), 1.0);
    assert_eq!(get(&reg, "[Channel1_Stem5]", "mute"), 1.0);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 0.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x90, 0x02, 0x7F]),
        "DRUM SWAP 3 lit: {out:x?}"
    );
    c.receive(&[0x90, 0x02, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem1]", "mute"), 0.0);
    c.receive(&[0x90, 0x24, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem5]", "mute"), 1.0);
    assert_eq!(
        get(&reg, "[QuickEffectRack1_[Channel1_Stem5]]", "enabled"),
        1.0
    );
    c.receive(&[0x90, 0x24, 0x00]);
    assert_eq!(get(&reg, "[Channel1_Stem5]", "mute"), 0.0);
    c.receive(&[0xB0, 0x12, 0x00]);
    assert_eq!(get(&reg, "[Channel1_Stem1]", "volume"), 0.0);
    c.receive(&[0xB0, 0x12, 0x7F]);

    // DRUM ROLL rolls the drums alone through the stem effect, DRUM TRANS
    // chops them; let go, the stem effect is back as it was.
    c.receive(&[0x90, 0x05, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx"), 3.0);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx_beats"), 0.25);
    for stem in [1, 5] {
        let fx = format!("[QuickEffectRack1_[Channel1_Stem{stem}]]");
        assert_eq!(get(&reg, &fx, "enabled"), 1.0);
    }
    assert_eq!(
        get(&reg, "[QuickEffectRack1_[Channel1_Stem4]]", "enabled"),
        0.0
    );
    assert_eq!(get(&reg, "[Channel1]", "beatlooproll_0.25_activate"), 0.0);
    c.receive(&[0x90, 0x05, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx"), 1.0);
    assert_eq!(
        get(&reg, "[QuickEffectRack1_[Channel1_Stem1]]", "enabled"),
        0.0
    );
    c.receive(&[0x90, 0x33, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx"), 4.0);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx_beats"), 1.0);
    c.receive(&[0x90, 0x33, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "stem_fx"), 1.0);
    // Without stems the whole deck rolls.
    c.receive(&[0x91, 0x05, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "beatlooproll_0.25_activate"), 1.0);
    c.receive(&[0x91, 0x05, 0x00]);

    // SHIFT + DRUM SWAP: Bank B takes out the other stems; Single ends the
    // swap before, Multi adds them up.
    c.receive(&[0x90, 0x2D, 0x7F]);
    c.receive(&[0x90, 0x02, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    c.receive(&[0x90, 0x00, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem2]", "mute"), 1.0);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 0.0);
    c.receive(&[0x90, 0x2F, 0x7F]);
    c.receive(&[0x90, 0x02, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem2]", "mute"), 1.0);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    c.poll(Instant::now());
    let out = c.take_output();
    for lit in [0x2D, 0x2F] {
        assert!(
            out.iter().any(|m| m[..] == [0x90, lit, 0x7F]),
            "SHIFT light {lit:x}: {out:x?}"
        );
    }

    // CAPTURE grabs bars into an open Looper deck; SHIFT steps the length.
    let bars = get(&reg, "[Looper1]", "capture_bars");
    c.receive(&[0x90, 0x2B, 0x7F]);
    assert_ne!(get(&reg, "[Looper3]", "capture_bars"), bars);
    reg.get("[Looper3]", "enabled").unwrap().set(1.0);
    let capture = reg.get("[Looper3]", "capture").unwrap();
    let version = capture.version();
    c.receive(&[0x90, 0x08, 0x7F]);
    assert!(
        capture.version().wrapping_sub(version) >= 2,
        "CAPTURE pushed"
    );

    // The BEAT FX beat lights show the effect's length.
    reg.get(fx, "beats").unwrap().set(4.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0xB4, 0x64, 0x07]),
        "4 beats: {out:x?}"
    );

    // LOAD lights follow the decks; the jog ring is on channel 16.
    reg.get("[Channel1]", "track_loaded").unwrap().set(1.0);
    reg.get("[Channel1]", "duration").unwrap().set(180.0);
    reg.get("[Channel1]", "playposition")
        .unwrap()
        .set(0.5 / 180.0);
    settle(&mut c, 100);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x9F, 0x00, 0x7F]),
        "LOAD 1 lit: {out:x?}"
    );
    assert!(
        out.iter().any(|m| m[0] == 0xBF && m[1] == 0x00 && m[2] > 0),
        "jog ring moved: {out:x?}"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn xdj_rx3_decks_mixer_and_pads() {
    let (reg, mut c) = load("Pioneer-XDJ-RX3.midi.xml");
    settle(&mut c, 50);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0xB4, 0x65, 0x00]),
        "mixer switched to MIDI control: {out:x?}"
    );

    // Each deck has its own channel: PLAY on channel 4 plays deck 4.
    c.receive(&[0x93, 0x00, 0x7F]);
    c.receive(&[0x93, 0x00, 0x00]);
    assert_eq!(get(&reg, "[Channel4]", "play"), 1.0);
    assert_eq!(get(&reg, "[Channel2]", "play"), 0.0);

    // The mixer on channel 5: deck 3's fader, deck 2's EQ.
    c.receive(&[0xB4, 0x13, 0x7F]);
    assert_eq!(get(&reg, "[Channel3]", "volume"), 1.0);
    c.receive(&[0xB4, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Master]", "crossfader"), -1.0);

    // Hot cue pads on deck 1's pad channel.
    c.receive(&[0x95, 0x00, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "hotcue_1_activate"), 1.0);
    c.receive(&[0x95, 0x00, 0x00]);
    // BEAT JUMP opens its tab, and its page's pads jump.
    c.receive(&[0x91, 0x25, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "pad_tab"), 2.0);
    assert_ne!(get(&reg, "[Channel1]", "pad_tab"), 2.0);

    // MASTER pins the sync leader.
    c.receive(&[0x90, 0x1E, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "sync_leader"), 2.0);

    // BEAT FX: ON/OFF, CH SELECT on MASTER sends it to the main mix, on CH1
    // to the left channel (decks 1 and 3).
    let unit = "[EffectRack1_EffectUnit5]";
    let fx = "[EffectRack1_EffectUnit5_Effect1]";
    c.receive(&[0xB4, 0x72, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);
    c.receive(&[0xB4, 0x29, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 6.0);
    reg.get(unit, "group_[Master]_enable").unwrap().set(1.0);
    c.receive(&[0xB4, 0x22, 0x7F]);
    assert_eq!(get(&reg, unit, "group_[Channel3]_enable"), 1.0);
    assert_eq!(get(&reg, unit, "group_[Channel2]_enable"), 0.0);
    assert_eq!(get(&reg, unit, "group_[Master]_enable"), 0.0);
    // EFFECT SELECT REVERB.
    c.receive(&[0xB4, 0x36, 0x7F]);
    assert_eq!(get(&reg, fx, "loaded_effect"), 2.0);

    // COLOR FX: NOISE on every deck's COLOR knob.
    c.receive(&[0xB4, 0x55, 0x7F]);
    for deck in 1..=4 {
        assert_eq!(
            get(
                &reg,
                &format!("[QuickEffectRack1_[Channel{deck}]]"),
                "loaded_chain_preset"
            ),
            4.0
        );
    }

    // The jog display shows the playhead.
    reg.get("[Channel1]", "track_loaded").unwrap().set(1.0);
    reg.get("[Channel1]", "duration").unwrap().set(180.0);
    reg.get("[Channel1]", "playposition")
        .unwrap()
        .set(0.5 / 180.0);
    settle(&mut c, 100);
    let out = c.take_output();
    assert!(
        out.iter()
            .any(|m| m[0] == 0xBB && (m[1] == 0x00 || m[1] == 0x20) && m[2] > 0),
        "jog display moved: {out:x?}"
    );
    assert_eq!(c.errors(), Vec::<String>::new());

    // Quitting hands the mixer back to the XDJ-RX3.
    c.shutdown();
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0xB4, 0x65, 0x7F]),
        "mixer handed back: {out:x?}"
    );
}

#[test]
fn a_controller_follows_the_instrument_to_its_deck() {
    use opendj_controllers::routing::{set_deck_choice, DeckChoice};
    // Deck choices are kept by device name, process-wide: one of its own.
    const PORT: &str = "Follow Test K2";
    const K2: &str = "Allen and Heath Xone K2 - 2 Decks.opendj-mapping";
    let (reg, mut c) = load(K2);
    c.follow_deck_choice(PORT);
    let press = |c: &mut Controller| {
        c.receive(&[0x9E, 0x19, 0x7F]);
        c.receive(&[0x8E, 0x19, 0x7F]);
    };
    let playing = |d: usize| get(&reg, &format!("[Channel{d}]"), "play") > 0.0;

    // As mapped: PLAY 1 plays deck 1.
    press(&mut c);
    assert!(playing(1));
    press(&mut c);
    assert!(!playing(1));

    // Following, with the instrument on deck 3: PLAY 1 plays deck 3.
    set_deck_choice(PORT, DeckChoice::Follow);
    reg.get("[Channel3]", "live_input").unwrap().set(3.0);
    c.poll(Instant::now());
    press(&mut c);
    assert!(playing(3) && !playing(1));
    press(&mut c);

    // The instrument dragged to deck 4: the controller goes with it.
    reg.get("[Channel3]", "live_input").unwrap().set(0.0);
    reg.get("[Channel4]", "live_input").unwrap().set(3.0);
    c.poll(Instant::now());
    press(&mut c);
    assert!(playing(4) && !playing(3));
    press(&mut c);

    // Pinned to deck 2.
    set_deck_choice(PORT, DeckChoice::Deck(2));
    c.poll(Instant::now());
    press(&mut c);
    assert!(playing(2));
}

#[test]
fn a_mixxx_mapping_follows_too() {
    use opendj_controllers::routing::{set_deck_choice, DeckChoice};
    const PORT: &str = "Follow Test FLX4";
    let (reg, mut c) = load("Pioneer-DDJ-FLX4.midi.xml");
    c.follow_deck_choice(PORT);
    set_deck_choice(PORT, DeckChoice::Deck(3));
    c.poll(Instant::now());
    c.receive(&[0x90, 0x0B, 0x7F]);
    c.receive(&[0x90, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel3]", "play"), 1.0);
    assert_eq!(get(&reg, "[Channel1]", "play"), 0.0);
}

/// The DDJ-SX2 and DDJ-RX run Mixxx's DDJ-SX mapping: the same messages
/// play, scratch and move the faders on all three.
#[test]
fn ddj_sx_family_plays_and_mixes() {
    for file in [
        "Pioneer DDJ-SX.midi.xml",
        "Pioneer DDJ-SX2.midi.xml",
        "Pioneer DDJ-RX.midi.xml",
        "Pioneer DDJ-SZ.midi.xml",
        "Pioneer DDJ-RZ.midi.xml",
        "Pioneer DDJ-RZX.midi.xml",
        "Pioneer DDJ-RR.midi.xml",
    ] {
        let (reg, mut c) = load(file);
        settle(&mut c, 50);
        assert!(!c.take_output().is_empty(), "{file}: init lights the LEDs");
        // PLAY on deck 2 (right side).
        c.receive(&[0x91, 0x0B, 0x7F]);
        c.receive(&[0x91, 0x0B, 0x00]);
        assert_eq!(get(&reg, "[Channel2]", "play"), 1.0, "{file}");
        // Channel 1 fader, 14-bit: all the way down.
        sweep14(&mut c, 0xB0, 0x13, 0x33, 0x00);
        assert!(get(&reg, "[Channel1]", "volume") < 0.02, "{file}");
        // PLAY on deck 3: the left side's second deck, its own channel.
        c.receive(&[0x92, 0x0B, 0x7F]);
        c.receive(&[0x92, 0x0B, 0x00]);
        assert_eq!(get(&reg, "[Channel3]", "play"), 1.0, "{file}");
        // Pads in every mode and the jogs run without script errors.
        for status in 0x97..=0x9A {
            for pad in [0x00, 0x10, 0x20, 0x30, 0x50] {
                c.receive(&[status, pad, 0x7F]);
                c.receive(&[status, pad, 0x00]);
            }
        }
        c.receive(&[0x90, 0x36, 0x7F]);
        c.receive(&[0xB0, 0x22, 0x45]);
        c.receive(&[0x90, 0x36, 0x00]);
        settle(&mut c, 50);
        assert_eq!(c.errors(), Vec::<String>::new(), "{file}");
        c.shutdown();
    }
}

/// The DDJ-RB runs the DDJ-400 script with its own pad numbering: deck 1's
/// pads on channel 8, deck 2's on 9, SHIFT adding 8 to the note.
#[test]
fn ddj_rb_pads_and_effects() {
    let (reg, mut c) = load("Pioneer-DDJ-RB.midi.xml");
    settle(&mut c, 50);
    c.receive(&[0x91, 0x0B, 0x7F]);
    c.receive(&[0x91, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel2]", "play"), 1.0);
    sweep14(&mut c, 0xB1, 0x13, 0x33, 0x00);
    assert!(get(&reg, "[Channel2]", "volume") < 0.02);
    // Deck 2's pad 1 is hot cue 1; SHIFT (note + 8) deletes it.
    c.receive(&[0x98, 0x00, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "hotcue_1_activate"), 1.0);
    c.receive(&[0x98, 0x00, 0x00]);
    c.receive(&[0x98, 0x08, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "hotcue_1_clear"), 1.0);
    c.receive(&[0x98, 0x08, 0x00]);
    // Its light follows the hot cue.
    c.take_output();
    reg.get("[Channel2]", "hotcue_1_enabled").unwrap().set(1.0);
    c.poll(Instant::now());
    let out = c.take_output();
    assert!(out.iter().any(|m| m[..] == [0x98, 0x00, 0x7F]), "{out:x?}");
    // BEAT JUMP pads, slicer rolls, pad FX, sampler and SLICER LOOP pads.
    for status in [0x97, 0x98] {
        for pad in [0x10, 0x18, 0x20, 0x30, 0x38, 0x40, 0x41, 0x50, 0x60] {
            c.receive(&[status, pad, 0x7F]);
            c.receive(&[status, pad, 0x00]);
        }
    }
    // FX2's ON button.
    let slot = "[EffectRack1_EffectUnit2_Effect1]";
    let on = get(&reg, slot, "enabled");
    c.receive(&[0x95, 0x47, 0x7F]);
    c.receive(&[0x95, 0x47, 0x00]);
    assert_eq!(get(&reg, slot, "enabled"), 1.0 - on);
    settle(&mut c, 600);
    assert_eq!(c.errors(), Vec::<String>::new());
    c.shutdown();
}

/// openDJ's own WeGO and ERGO mappings: decks, mixer, hot cues, the
/// sampler and the effects.
#[test]
fn ddj_wego_family_and_ergo() {
    for file in [
        "Pioneer-DDJ-WeGO.midi.xml",
        "Pioneer-DDJ-WeGO2.midi.xml",
        "Pioneer-DDJ-WeGO3.midi.xml",
        "Pioneer-DDJ-WeGO4.midi.xml",
        "Pioneer-DDJ-ERGO.midi.xml",
    ] {
        let (reg, mut c) = load(file);
        settle(&mut c, 50);
        c.receive(&[0x91, 0x0B, 0x7F]);
        c.receive(&[0x91, 0x0B, 0x00]);
        assert_eq!(get(&reg, "[Channel2]", "play"), 1.0, "{file}");
        // Channel fader 1 is B6 13/33.
        sweep14(&mut c, 0xB6, 0x13, 0x33, 0x00);
        assert!(get(&reg, "[Channel1]", "volume") < 0.02, "{file}");
        // Tempo all the way to the top: slowest.
        c.receive(&[0xB0, 0x00, 0x7F]);
        c.receive(&[0xB0, 0x20, 0x7F]);
        assert!(get(&reg, "[Channel1]", "rate") < -0.9, "{file}");
        // Pad 2 on deck 1 is hot cue 2; SHIFT deletes it.
        c.receive(&[0x90, 0x2F, 0x7F]);
        assert_eq!(get(&reg, "[Channel1]", "hotcue_2_activate"), 1.0, "{file}");
        c.receive(&[0x90, 0x2F, 0x00]);
        c.receive(&[0x90, 0x60, 0x7F]);
        assert_eq!(get(&reg, "[Channel1]", "hotcue_2_clear"), 1.0, "{file}");
        c.receive(&[0x90, 0x60, 0x00]);
        // The jog: touch, scratch, let go; bend; search.
        c.receive(&[0x90, 0x36, 0x7F]);
        c.receive(&[0xB0, 0x22, 0x45]);
        c.receive(&[0x90, 0x36, 0x00]);
        c.receive(&[0xB0, 0x21, 0x41]);
        c.receive(&[0xB0, 0x26, 0x41]);
        // FX unit 1's first effect on and off.
        let slot = "[EffectRack1_EffectUnit1_Effect1]";
        let on_note = if file.contains("ERGO") { 0x47 } else { 0x43 };
        let on = get(&reg, slot, "enabled");
        c.receive(&[0x94, on_note, 0x7F]);
        c.receive(&[0x94, on_note, 0x00]);
        assert_eq!(get(&reg, slot, "enabled"), 1.0 - on, "{file}");
        settle(&mut c, 100);
        assert_eq!(c.errors(), Vec::<String>::new(), "{file}");
        c.shutdown();
    }
}

/// On the DDJ-RX, note 0x35 on a deck channel is QUANTIZE.
#[test]
fn ddj_rx_quantize() {
    let (reg, mut c) = load("Pioneer DDJ-RX.midi.xml");
    settle(&mut c, 50);
    let before = get(&reg, "[Channel3]", "quantize");
    c.receive(&[0x92, 0x35, 0x7F]);
    c.receive(&[0x92, 0x35, 0x00]);
    assert_eq!(get(&reg, "[Channel3]", "quantize"), 1.0 - before);
    c.poll(Instant::now());
    let want = if before == 0.0 { 0x7F } else { 0x00 };
    assert!(
        c.take_output().iter().any(|m| m[..] == [0x92, 0x35, want]),
        "QUANTIZE LED follows"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
}

/// openDJ's DDJ-800 mapping: the DDJ-FLX4's, moved to the DDJ-800's
/// messages, plus its own buttons and Beat FX switches.
#[test]
fn ddj_800_decks_mixer_and_beat_fx() {
    let (reg, mut c) = load("Pioneer-DDJ-800.midi.xml");
    settle(&mut c, 50);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x90, 0x5D, 0x00]),
        "jog display shown: {out:x?}"
    );

    // PLAY on deck 2.
    c.receive(&[0x91, 0x0B, 0x7F]);
    c.receive(&[0x91, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel2]", "play"), 1.0);

    // Channel 1 fader, 14-bit, all the way down.
    sweep14(&mut c, 0xB0, 0x13, 0x33, 0x00);
    assert!(get(&reg, "[Channel1]", "volume") < 0.02);

    // QUANTIZE and MASTER TEMPO toggle, and their lights follow.
    for (note, key) in [(0x35, "quantize"), (0x1A, "keylock")] {
        let before = get(&reg, "[Channel1]", key);
        c.receive(&[0x90, note, 0x7F]);
        c.receive(&[0x90, note, 0x00]);
        assert_eq!(get(&reg, "[Channel1]", key), 1.0 - before, "{key}");
        c.poll(Instant::now());
        let want = if before == 0.0 { 0x7F } else { 0x00 };
        assert!(
            c.take_output().iter().any(|m| m[..] == [0x90, note, want]),
            "{key} light"
        );
    }

    // Beat FX: CH SELECT to CH2, FX SELECT to REVERB, ON.
    c.receive(&[0x94, 0x1D, 0x7F]);
    let unit = "[EffectRack1_EffectUnit5]";
    assert_eq!(get(&reg, unit, "route"), 2.0);
    c.receive(&[0x94, 0x14, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 6.0);
    c.receive(&[0x94, 0x24, 0x7F]);
    let slot = "[EffectRack1_EffectUnit5_Effect1]";
    assert_eq!(
        opendj_engine::effect_name(get(&reg, slot, "loaded_effect")),
        "Reverb"
    );
    let on = get(&reg, slot, "enabled");
    c.receive(&[0x94, 0x47, 0x7F]);
    c.receive(&[0x94, 0x47, 0x00]);
    assert_eq!(get(&reg, slot, "enabled"), 1.0 - on);

    // SEARCH and KEY SYNC run without script errors.
    for note in [0x5E, 0x5F, 0x65, 0x64] {
        c.receive(&[0x91, note, 0x7F]);
        settle(&mut c, 20);
        c.receive(&[0x91, note, 0x00]);
    }
    settle(&mut c, 100);
    assert_eq!(c.errors(), Vec::<String>::new());
    c.shutdown();
}

#[test]
fn xdj_az_decks_mixer_pads_and_beat_fx() {
    let (reg, mut c) = load("Pioneer-XDJ-AZ.midi.xml");
    settle(&mut c, 50);
    c.take_output();

    // Each deck has its own channel: PLAY on channel 4 plays deck 4.
    c.receive(&[0x93, 0x00, 0x7F]);
    c.receive(&[0x93, 0x00, 0x00]);
    assert_eq!(get(&reg, "[Channel4]", "play"), 1.0);
    assert_eq!(get(&reg, "[Channel2]", "play"), 0.0);

    // DECK QUANTIZE moved to 0x1A; KEY SYNC, 4 BEAT and BEAT JUMP are new.
    let quantize = get(&reg, "[Channel1]", "quantize");
    c.receive(&[0x90, 0x1A, 0x7F]);
    assert_ne!(get(&reg, "[Channel1]", "quantize"), quantize);
    c.receive(&[0x90, 0x43, 0x7F]);
    c.receive(&[0x90, 0x43, 0x00]);
    c.receive(&[0x90, 0x17, 0x7F]);
    let size = get(&reg, "[Channel1]", "beatjump_size");
    c.receive(&[0x90, 0x19, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "beatjump_size"), size / 2.0);

    // The mixer on channel 5: deck 3's fader, the crossfader.
    c.receive(&[0xB4, 0x13, 0x7F]);
    assert_eq!(get(&reg, "[Channel3]", "volume"), 1.0);
    c.receive(&[0xB4, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Master]", "crossfader"), -1.0);
    // CROSSFADER ASSIGN: one CC a channel, A / THRU / B.
    c.receive(&[0xB4, 0x42, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "orientation"), 2.0);
    c.receive(&[0xB4, 0x41, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "orientation"), 0.0);
    c.receive(&[0xB4, 0x43, 0x40]);
    assert_eq!(get(&reg, "[Channel3]", "orientation"), 1.0);

    // Pads: pad 1 on deck 1's first pad channel, pad 6 on its second
    // (channel 10), each mode on its own note, routed onto the pages.
    c.receive(&[0x95, 0x00, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "hotcue_1_activate"), 1.0);
    c.receive(&[0x95, 0x00, 0x00]);
    c.receive(&[0x99, 0x13, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "hotcue_6_activate"), 1.0);
    c.receive(&[0x99, 0x13, 0x00]);
    // Deck 2's pads 3 and 7 in BEAT JUMP mode send 2 * 19 + 9.
    c.receive(&[0x91, 0x25, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "pad_tab"), 2.0);
    c.receive(&[0x96, 0x2F, 0x7F]);
    c.receive(&[0x96, 0x2F, 0x00]);
    assert_eq!(get(&reg, "[Channel2]", "beatjump_size"), 8.0);
    c.receive(&[0x9A, 0x2F, 0x7F]);
    c.receive(&[0x9A, 0x2F, 0x00]);
    assert_eq!(get(&reg, "[Channel2]", "beatjump_size"), 32.0);
    // A note with no page does nothing.
    c.receive(&[0x95, 0x04, 0x7F]);
    c.receive(&[0x95, 0x04, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "hotcue_5_activate"), 0.0);

    // Hot cue 6's light reaches pad 6 on channel 10.
    reg.get("[Channel1]", "hotcue_6_status").unwrap().set(1.0);
    settle(&mut c, 20);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[0] == 0x99 && m[1] == 0x13 && m[2] > 0),
        "pad 6 lit: {out:x?}"
    );

    // BEAT FX: CH SELECT CH3 sends it to deck 3, MASTER to the main mix; CF.B follows the
    // crossfader assign; EFFECT SELECT REVERB; AUTO/TAP.
    let unit = "[EffectRack1_EffectUnit5]";
    let fx = "[EffectRack1_EffectUnit5_Effect1]";
    c.receive(&[0xB4, 0x72, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);
    c.receive(&[0xB4, 0x22, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 3.0);
    c.receive(&[0xB4, 0x29, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 6.0);
    c.receive(&[0xB4, 0x28, 0x7F]);
    assert_eq!(get(&reg, unit, "group_[Channel2]_enable"), 1.0);
    assert_eq!(get(&reg, unit, "group_[Channel1]_enable"), 0.0);
    c.receive(&[0xB4, 0x36, 0x7F]);
    assert_eq!(get(&reg, fx, "loaded_effect"), 2.0);
    c.receive(&[0xB4, 0x45, 0x7F]);

    // The jog display on channel 14 shows the playhead.
    reg.get("[Channel1]", "track_loaded").unwrap().set(1.0);
    reg.get("[Channel1]", "duration").unwrap().set(180.0);
    reg.get("[Channel1]", "playposition")
        .unwrap()
        .set(0.5 / 180.0);
    settle(&mut c, 100);
    let out = c.take_output();
    assert!(
        out.iter()
            .any(|m| m[0] == 0xBD && (m[1] == 0x00 || m[1] == 0x20) && m[2] > 0),
        "jog display moved: {out:x?}"
    );
    assert!(
        out.iter().any(|m| m[..] == [0x9D, 0x00, 0x7F]),
        "LOAD light: {out:x?}"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
    c.shutdown();
}

#[test]
fn opus_quad_decks_hot_cues_mixer_and_jog_rings() {
    let (reg, mut c) = load("Pioneer-OPUS-QUAD.midi.xml");
    settle(&mut c, 50);
    c.take_output();

    // Each deck has its own channel: PLAY on channel 3 plays deck 3.
    c.receive(&[0x92, 0x00, 0x7F]);
    c.receive(&[0x92, 0x00, 0x00]);
    assert_eq!(get(&reg, "[Channel3]", "play"), 1.0);
    assert_eq!(get(&reg, "[Channel1]", "play"), 0.0);

    // DECK QUANTIZE on 0x09; BEAT JUMP on 0x2C/0x2D, SHIFT sizes it.
    let quantize = get(&reg, "[Channel1]", "quantize");
    c.receive(&[0x90, 0x09, 0x7F]);
    assert_ne!(get(&reg, "[Channel1]", "quantize"), quantize);
    c.receive(&[0x90, 0x49, 0x7F]);
    let size = get(&reg, "[Channel1]", "beatjump_size");
    c.receive(&[0x90, 0x4A, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "beatjump_size"), size / 2.0);

    // Hot cues A-H on the deck channel: HOT CUE A is 0x08, H is 0x1B; SHIFT
    // + HOT CUE D (0x14) clears hot cue 4.
    c.receive(&[0x91, 0x08, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "hotcue_1_activate"), 1.0);
    c.receive(&[0x91, 0x08, 0x00]);
    c.receive(&[0x91, 0x1B, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "hotcue_8_activate"), 1.0);
    c.receive(&[0x91, 0x1B, 0x00]);
    c.receive(&[0x91, 0x14, 0x7F]);
    c.receive(&[0x91, 0x14, 0x00]);
    reg.get("[Channel2]", "hotcue_5_status").unwrap().set(1.0);
    settle(&mut c, 20);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x91, 0x15, 0x7F]),
        "hot cue E lit: {out:x?}"
    );

    // BEAT LOOP: turning right doubles the loop size, pressing sets a loop.
    let loop_size = get(&reg, "[Channel1]", "beatloop_size");
    c.receive(&[0xB0, 0x2E, 0x01]);
    assert_eq!(get(&reg, "[Channel1]", "beatloop_size"), loop_size * 2.0);
    c.receive(&[0xB0, 0x2E, 0x7F]);
    assert_eq!(get(&reg, "[Channel1]", "beatloop_size"), loop_size);
    c.receive(&[0x90, 0x43, 0x7F]);
    c.receive(&[0x90, 0x43, 0x00]);

    // The mixer on channel 5: deck 4's fader and EQ, the crossfader assign.
    c.receive(&[0xB4, 0x14, 0x7F]);
    assert_eq!(get(&reg, "[Channel4]", "volume"), 1.0);
    c.receive(&[0xB4, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Master]", "crossfader"), -1.0);
    c.receive(&[0xB4, 0x44, 0x7F]);
    assert_eq!(get(&reg, "[Channel4]", "orientation"), 2.0);

    // BEAT FX: VINYL BRAKE loads the brake, PITCH keeps it; ON/OFF.
    let fx = "[EffectRack1_EffectUnit5_Effect1]";
    c.receive(&[0xB4, 0x3D, 0x7F]);
    assert_eq!(get(&reg, fx, "loaded_effect"), 10.0);
    c.receive(&[0xB4, 0x3F, 0x7F]);
    assert_eq!(get(&reg, fx, "loaded_effect"), 10.0);
    let on = get(&reg, fx, "enabled");
    c.receive(&[0xB4, 0x72, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0 - on);

    // The jog ring on channel 12 shows the playhead (1 to 72); LOAD flashes.
    reg.get("[Channel1]", "track_loaded").unwrap().set(1.0);
    reg.get("[Channel1]", "duration").unwrap().set(180.0);
    reg.get("[Channel1]", "playposition")
        .unwrap()
        .set(0.5 / 180.0);
    settle(&mut c, 100);
    let out = c.take_output();
    assert!(
        out.iter()
            .any(|m| m[0] == 0xBB && m[1] == 0x00 && (1..=72).contains(&m[2])),
        "jog ring moved: {out:x?}"
    );
    assert!(
        out.iter().any(|m| m[..] == [0x9B, 0x04, 0x7F]),
        "LOAD light: {out:x?}"
    );

    // Every other deck button runs without script errors.
    for note in [
        0x02, 0x04, 0x05, 0x10, 0x11, 0x12, 0x13, 0x1E, 0x1F, 0x21, 0x2C, 0x2D, 0x44, 0x45,
    ] {
        c.receive(&[0x93, note, 0x7F]);
        settle(&mut c, 5);
        c.receive(&[0x93, note, 0x00]);
    }
    c.receive(&[0x90, 0x31, 0x7F]);
    c.receive(&[0x90, 0x31, 0x00]);
    settle(&mut c, 50);
    assert_eq!(c.errors(), Vec::<String>::new());
    c.shutdown();
}

#[test]
fn ddj_flx10_decks_mixer_fx_and_displays() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX10.midi.xml");
    settle(&mut c, 50);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x9F, 0x5D, 0x00]),
        "jog display shown: {out:x?}"
    );

    // Each deck has its own channel: PLAY on channel 3 plays deck 3.
    c.receive(&[0x92, 0x0B, 0x7F]);
    c.receive(&[0x92, 0x0B, 0x00]);
    assert_eq!(get(&reg, "[Channel3]", "play"), 1.0);
    assert_eq!(get(&reg, "[Channel1]", "play"), 0.0);

    // The mixer, as on the FLX6.
    sweep14(&mut c, 0xB3, 0x13, 0x33, 0x20);
    let v = get(&reg, "[Channel4]", "volume");
    assert!((v - 0.25).abs() < 0.02, "deck 4 volume {v}");

    // Deck 4's pads: hot cue 1.
    c.receive(&[0x9D, 0x00, 0x7F]);
    assert_eq!(get(&reg, "[Channel4]", "hotcue_1_activate"), 1.0);
    c.receive(&[0x9D, 0x00, 0x00]);

    // The FLX10's own buttons: QUANTIZE, SLIP, KEY SYNC, 4 BEAT JUMP,
    // CROSSFADER ASSIGN; SHIFT + BEAT SYNC pins the sync leader.
    let quantize = get(&reg, "[Channel1]", "quantize");
    c.receive(&[0x90, 0x35, 0x7F]);
    assert_ne!(get(&reg, "[Channel1]", "quantize"), quantize);
    c.receive(&[0x91, 0x40, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "slip_enabled"), 1.0);
    c.receive(&[0x90, 0x65, 0x7F]);
    c.receive(&[0x90, 0x65, 0x00]);
    c.receive(&[0x90, 0x5E, 0x7F]);
    c.receive(&[0x92, 0x18, 0x7F]);
    assert_eq!(get(&reg, "[Channel3]", "orientation"), 2.0);
    c.receive(&[0x92, 0x16, 0x7F]);
    assert_eq!(get(&reg, "[Channel3]", "orientation"), 0.0);
    c.receive(&[0x91, 0x5C, 0x7F]);
    assert_eq!(get(&reg, "[Channel2]", "sync_leader"), 2.0);

    // BEAT FX: ON/OFF, SELECT on REVERB, CH SELECT on CH3 then MASTER.
    let unit = "[EffectRack1_EffectUnit5]";
    let fx = "[EffectRack1_EffectUnit5_Effect1]";
    c.receive(&[0x94, 0x47, 0x7F]);
    assert_eq!(get(&reg, fx, "enabled"), 1.0);
    c.receive(&[0x94, 0x24, 0x7F]);
    assert_eq!(get(&reg, fx, "loaded_effect"), 2.0);
    c.receive(&[0x94, 0x12, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 3.0);
    c.receive(&[0x94, 0x14, 0x7F]);
    assert_eq!(get(&reg, unit, "route"), 6.0);

    // SOUND COLOR FX: NOISE on every deck's COLOR knob, lit.
    c.receive(&[0x96, 0x04, 0x7F]);
    for deck in 1..=4 {
        assert_eq!(
            get(
                &reg,
                &format!("[QuickEffectRack1_[Channel{deck}]]"),
                "loaded_chain_preset"
            ),
            4.0
        );
    }

    // ACTIVE PART VOCAL mutes deck 1's vocals stem.
    c.receive(&[0x90, 0x0E, 0x7F]);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);

    // The jog display shows the playhead and the time.
    reg.get("[Channel1]", "track_loaded").unwrap().set(1.0);
    reg.get("[Channel1]", "duration").unwrap().set(180.0);
    reg.get("[Channel1]", "playposition")
        .unwrap()
        .set(61.5 / 180.0);
    settle(&mut c, 100);
    let out = c.take_output();
    assert!(
        out.iter()
            .any(|m| m[0] == 0xBF && (m[1] == 0x10 || m[1] == 0x30) && m[2] > 0),
        "jog display moved: {out:x?}"
    );
    assert!(
        out.iter().any(|m| m[..] == [0xBF, 0x42, 0x01]),
        "minutes shown: {out:x?}"
    );
    assert!(
        out.iter()
            .any(|m| m[0] == 0x96 && m[1] == 0x04 && m[2] == 0x7F),
        "NOISE lit: {out:x?}"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
    c.shutdown();
}

#[test]
fn ddj_flx10_pad_colours_lights_and_jog_scale() {
    let (reg, mut c) = load("Pioneer-DDJ-FLX10.midi.xml");
    settle(&mut c, 50);
    c.take_output();

    // A set hot cue lights its pad in the hot cue's colour, not white:
    // hot cue 2 is green (21) on deck 1's HOT CUE pads.
    reg.get("[Channel1]", "hotcue_2_status").unwrap().set(1.0);
    settle(&mut c, 5);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x97, 0x01, 21]),
        "hot cue 2 green: {out:x?}"
    );

    // ACTIVE PART: lit while the stems play, dimmed once muted.
    reg.get("[Channel1]", "stem_count").unwrap().set(5.0);
    settle(&mut c, 5);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0x90, 0x0E, 0x7F]),
        "VOCAL lit: {out:x?}"
    );
    c.receive(&[0x90, 0x0E, 0x7F]);
    c.receive(&[0x90, 0x0E, 0x00]);
    settle(&mut c, 5);
    assert_eq!(get(&reg, "[Channel1_Stem4]", "mute"), 1.0);
    let out = c.take_output();
    assert_eq!(
        out.iter()
            .rev()
            .find(|m| m[0] == 0x90 && m[1] == 0x0E)
            .map(|m| m[2]),
        Some(0x00),
        "VOCAL dimmed: {out:x?}"
    );

    // The jog display shows the key: A minor is 2.
    reg.get("[Channel1]", "track_loaded").unwrap().set(1.0);
    reg.get("[Channel1]", "key").unwrap().set(22.0);
    settle(&mut c, 100);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0xB0, 0x49, 0x02]),
        "key shown: {out:x?}"
    );
    // And the tempo: +4.0% is 1040 tenths above -100%.
    reg.get("[Channel1]", "rateRange").unwrap().set(0.08);
    reg.get("[Channel1]", "rate").unwrap().set(0.5);
    // settle counts from now, so run past the last pass's timer.
    settle(&mut c, 300);
    let out = c.take_output();
    assert!(
        out.iter().any(|m| m[..] == [0xBF, 0x18, 8])
            && out.iter().any(|m| m[..] == [0xBF, 0x38, 16]),
        "tempo shown: {out:x?}"
    );

    // The big jog's rim: ten ticks bend as much as an FLX4 tick and a quarter.
    c.receive(&[0xB0, 0x21, 64 + 10]);
    let bend = get(&reg, "[Channel1]", "jog_bend");
    assert!(
        (bend - 10.0 * 720.0 / 5760.0).abs() < 1e-6,
        "jog_bend {bend}"
    );
    assert_eq!(c.errors(), Vec::<String>::new());
    c.shutdown();
}

#[test]
fn ddj_400_secondary_pad_modes() {
    let (reg, mut c) = load("Pioneer-DDJ-400.midi.xml");
    for (deck, status) in [(1, 0x97), (2, 0x99)] {
        let group = format!("[Channel{deck}]");
        for shifted in 0..=1 {
            for pad in 0..8 {
                let key = format!("keyboard_{}_activate", pad + 1);
                c.receive(&[status + shifted, 0x40 + pad, 0x7f]);
                assert_eq!(get(&reg, &group, &key), 1.0);
                c.receive(&[status + shifted, 0x40 + pad, 0]);
                assert_eq!(get(&reg, &group, &key), 0.0);
            }
            for (note, effect) in [(0x10, 1.0), (0x50, 8.0)] {
                let slot = format!("[EffectRack1_EffectUnit{}_Effect1]", deck + 5);
                c.receive(&[status + shifted, note, 0x7f]);
                assert_eq!(get(&reg, &slot, "loaded_effect"), effect);
                assert_eq!(get(&reg, &slot, "enabled"), 1.0);
                c.receive(&[status + shifted, note, 0]);
                assert_eq!(get(&reg, &slot, "enabled"), 0.0);
            }
        }
        c.receive(&[status, 0x72, 0x7f]);
        c.receive(&[status, 0x72, 0]);
        assert_eq!(get(&reg, &group, "pitch_adjust"), 2.0);
        c.receive(&[status + 1, 0x74, 0x7f]);
        c.receive(&[status + 1, 0x74, 0]);
        assert_eq!(get(&reg, &group, "pitch_adjust"), 1.0);
    }
    assert_eq!(c.errors(), Vec::<String>::new());
}

#[test]
fn numark_ns4fx_sync_loops_and_crossfader_assign() {
    let (reg, mut c) = load("Numark NS4FX.midi.xml");
    settle(&mut c, 50);
    c.take_output();
    let version = |key: &str| reg.get("[Channel1]", key).unwrap().version();

    // A tap of SYNC syncs once, every time.
    for i in 0..3 {
        let before = version("beatsync");
        c.receive(&[0x90, 0x02, 0x7F]);
        c.receive(&[0x80, 0x02, 0x00]);
        assert_ne!(version("beatsync"), before, "tap {i} of SYNC syncs");
    }
    assert_eq!(get(&reg, "[Channel1]", "sync_enabled"), 0.0);

    // 1/2 and x2 halve and double the loop, every time.
    for key_note in [("loop_halve", 0x34), ("loop_double", 0x35)] {
        for i in 0..3 {
            let before = version(key_note.0);
            c.receive(&[0x94, key_note.1, 0x7F]);
            c.receive(&[0x84, key_note.1, 0x00]);
            assert_ne!(version(key_note.0), before, "press {i} of {}", key_note.0);
        }
    }

    // The A / off / B switch sets the deck's crossfader side.
    c.receive(&[0xB0, 0x1E, 0x01]);
    assert_eq!(get(&reg, "[Channel1]", "orientation"), 0.0);
    c.receive(&[0xB0, 0x1E, 0x02]);
    assert_eq!(get(&reg, "[Channel1]", "orientation"), 2.0);
    c.receive(&[0xB0, 0x1E, 0x00]);
    assert_eq!(get(&reg, "[Channel1]", "orientation"), 1.0);
}
