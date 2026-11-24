//! Reads Ableton Live sets (.als): gzip-compressed XML. Tracks come in with
//! their names, colours, mixer settings and Session clips (MIDI notes, or
//! recordings with their warp markers); devices are listed by name until
//! the studio can load them.
//!
//! Written against Live 12 sets; Live 10 and 11 use the same layout except
//! `MasterTrack` for `MainTrack`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use roxmltree::{Document, Node};

use crate::clip::{
    AudioClip, Clip, DeviceInfo, Note, PluginFormat, PluginRef, Sound, Track, MAX_TRACKS, SCENES,
};

/// What the studio takes from a Live set.
pub struct LiveSet {
    pub tempo: f64,
    pub tracks: Vec<Track>,
    /// Return tracks: name and colour.
    pub returns: Vec<(String, u32)>,
    /// Scenes in the set (the studio shows the first eight).
    pub scenes: usize,
    /// What didn't come in, for the status line and `--als-info`.
    pub left_out: Vec<String>,
}

/// Live's 70 clip and track colours, by index.
const PALETTE: [u32; 70] = [
    0xFF94A6, 0xFFA529, 0xCC9927, 0xF7F47C, 0xBFFB00, 0x1AFF2F, 0x25FFA8, 0x5CFFE8, 0x8BC5FF,
    0x5480E4, 0x92A7FF, 0xD86CE4, 0xE553A0, 0xFFFFFF, 0xFF3636, 0xF66C03, 0x99724B, 0xFFF034,
    0x87FF67, 0x3DC300, 0x00BFAF, 0x19E9FF, 0x10A4EE, 0x007DC0, 0x886CE4, 0xB677C6, 0xFF39D4,
    0xD0D0D0, 0xE2675A, 0xFFA374, 0xD3AD71, 0xEDFFAE, 0xD2E498, 0xBAD074, 0x9BC48D, 0xD4FDE1,
    0xCDF1F8, 0xB9C1E3, 0xCDBBE4, 0xAE98E5, 0xE5DCE1, 0xA9A9A9, 0xC6928B, 0xB78256, 0x99836A,
    0xBFBA69, 0xA6BE00, 0x7DB04D, 0x88C2BA, 0x9BB3C4, 0x85A5C2, 0x8393CC, 0xA595B5, 0xBF9FBE,
    0xBC7196, 0x7B7B7B, 0xAF3333, 0xA95131, 0x724F41, 0xDBC300, 0x85961F, 0x539F31, 0x0A9C8E,
    0x236384, 0x1A2F96, 0x2F52A2, 0x624BAD, 0xA34BAD, 0xCC2E6E, 0x3C3C3C,
];

pub fn color(index: usize) -> u32 {
    PALETTE.get(index).copied().unwrap_or(0xB69CFF)
}

/// The XML inside an .als file.
pub fn xml(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.starts_with(b"<?xml") {
        return String::from_utf8(bytes).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_string(&mut out)
        .map_err(|e| format!("{}: not a Live set ({e})", path.display()))?;
    Ok(out)
}

/// Reads the set at `path`; audio files are found next to it but not
/// decoded (see [`decode_audio`]).
pub fn read(path: &Path) -> Result<LiveSet, String> {
    let text = xml(path)?;
    parse(&text, path.parent().unwrap_or(Path::new(".")))
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(name))
}

/// The node at a `/`-separated path of child names.
fn at<'a, 'i>(n: Node<'a, 'i>, path: &str) -> Option<Node<'a, 'i>> {
    path.split('/').try_fold(n, |n, name| child(n, name))
}

/// The `Value` attribute at `path`.
fn value<'a>(n: Node<'a, '_>, path: &str) -> Option<&'a str> {
    at(n, path)?.attribute("Value")
}

fn number(n: Node, path: &str) -> Option<f64> {
    value(n, path)?.parse().ok()
}

fn attr(n: Node, name: &str) -> Option<f64> {
    n.attribute(name)?.parse().ok()
}

/// A linear gain as a fader position (see `control::fader_gain`).
fn fader(gain: f64) -> f32 {
    if gain <= 0.000_4 {
        return 0.0;
    }
    let db = 20.0 * gain.log10();
    (1.0 - (6.0 - db) / 66.0).clamp(0.0, 1.0) as f32
}

pub fn parse(text: &str, folder: &Path) -> Result<LiveSet, String> {
    let doc = Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: u32::MAX,
            ..Default::default()
        },
    )
    .map_err(|e| format!("not a Live set: {e}"))?;
    let set = doc
        .descendants()
        .find(|n| n.has_tag_name("LiveSet"))
        .ok_or("not a Live set: no LiveSet")?;
    let main = child(set, "MainTrack").or_else(|| child(set, "MasterTrack"));
    let tempo = main
        .and_then(|m| number(m, "DeviceChain/Mixer/Tempo/Manual"))
        .unwrap_or(120.0);
    let scenes =
        child(set, "Scenes").map_or(SCENES, |s| s.children().filter(Node::is_element).count());
    let mut left_out = Vec::new();
    if scenes > SCENES {
        left_out.push(format!("scenes {} to {scenes}", SCENES + 1));
    }
    let mut tracks = Vec::new();
    let mut returns = Vec::new();
    let all = child(set, "Tracks").ok_or("not a Live set: no Tracks")?;
    for t in all.children().filter(Node::is_element) {
        let name = value(t, "Name/EffectiveName")
            .unwrap_or("Track")
            .to_string();
        let colour = color(number(t, "Color").unwrap_or(0.0) as usize);
        match t.tag_name().name() {
            "ReturnTrack" => returns.push((name, colour)),
            "GroupTrack" => left_out.push(format!("group track {name} (its tracks come in)")),
            kind @ ("AudioTrack" | "MidiTrack") => {
                if tracks.len() == MAX_TRACKS {
                    left_out.push(format!("track {name} (the studio holds {MAX_TRACKS})"));
                    continue;
                }
                let name = without_number(&name, tracks.len() + 1);
                let track = read_track(
                    t,
                    kind == "AudioTrack",
                    name,
                    colour,
                    tempo,
                    folder,
                    &mut left_out,
                );
                tracks.push(track);
            }
            _ => {}
        }
    }
    Ok(LiveSet {
        tempo,
        tracks,
        returns,
        scenes,
        left_out,
    })
}

/// A track name without a leading number that only repeats its position
/// ("1 Mic" or "1-Audio" for the first track), which the window already
/// shows.
fn without_number(name: &str, position: usize) -> String {
    let n = position.to_string();
    match name.strip_prefix(&n) {
        Some(rest) if rest.starts_with([' ', '-']) && !rest[1..].trim().is_empty() => {
            rest[1..].trim().to_string()
        }
        _ => name.to_string(),
    }
}

fn read_track(
    t: Node,
    audio: bool,
    name: String,
    colour: u32,
    tempo: f64,
    folder: &Path,
    left_out: &mut Vec<String>,
) -> Track {
    let chain = child(t, "DeviceChain");
    let devices: Vec<DeviceInfo> = chain
        .and_then(|c| at(c, "DeviceChain/Devices"))
        .map(|d| {
            d.children()
                .filter(Node::is_element)
                .flat_map(devices)
                .collect()
        })
        .unwrap_or_default();
    let sound = if audio {
        Sound::Audio
    } else {
        stand_in(&name, chain.and_then(|c| at(c, "DeviceChain/Devices")))
    };
    let mut track = Track::new(&name, sound, vec![None; SCENES]);
    track.color = Some(colour);
    track.devices = devices;
    if let Some(c) = chain {
        track.volume = number(c, "Mixer/Volume/Manual").map(fader);
        track.pan = number(c, "Mixer/Pan/Manual").map_or(0.5, |p| ((p + 1.0) / 2.0) as f32);
        track.input = if audio {
            input_name(value(c, "AudioInputRouting/Target").unwrap_or(""))
        } else {
            midi_input_name(value(c, "MidiInputRouting/Target").unwrap_or(""))
        };
        if let Some(slots) = at(c, "MainSequencer/ClipSlotList") {
            for (s, slot) in slots.children().filter(Node::is_element).enumerate() {
                let Some(clip) = slot
                    .descendants()
                    .find(|n| n.has_tag_name("AudioClip") || n.has_tag_name("MidiClip"))
                else {
                    continue;
                };
                if s >= SCENES {
                    continue;
                }
                track.slots[s] = if clip.has_tag_name("MidiClip") {
                    midi_clip(clip)
                } else {
                    audio_clip(clip, tempo, folder)
                };
            }
        }
        let arranged = at(c, "MainSequencer/Sample/ArrangerAutomation/Events")
            .into_iter()
            .chain(at(
                c,
                "MainSequencer/ClipTimeable/ArrangerAutomation/Events",
            ))
            .map(|e| e.children().filter(Node::is_element).count())
            .sum::<usize>();
        if arranged > 0 {
            left_out.push(format!("{arranged} Arrangement clips on {name}"));
        }
    }
    track
}

/// A device's name and kind.
/// The devices a chain's device node stands for: itself, or, for a rack
/// with a single chain, the devices in that chain (a rack used only to
/// group effects).
fn devices(d: Node) -> Vec<DeviceInfo> {
    let rack = matches!(
        d.tag_name().name(),
        "AudioEffectGroupDevice" | "InstrumentGroupDevice" | "MidiEffectGroupDevice"
    );
    if rack {
        let branches: Vec<Node> = child(d, "Branches")
            .map(|b| b.children().filter(Node::is_element).collect())
            .unwrap_or_default();
        if let [branch] = branches[..] {
            let inner = [
                "MidiToAudioDeviceChain",
                "AudioToAudioDeviceChain",
                "MidiToMidiDeviceChain",
            ]
            .iter()
            .find_map(|c| at(branch, &format!("DeviceChain/{c}/Devices")));
            if let Some(inner) = inner {
                let on = on(d);
                return inner
                    .children()
                    .filter(Node::is_element)
                    .flat_map(devices)
                    .map(|mut x| {
                        x.on &= on;
                        x
                    })
                    .collect();
            }
        }
    }
    vec![device(d)]
}

fn on(d: Node) -> bool {
    value(d, "On/Manual") != Some("false")
}

/// A four-character code from Live's number for it.
fn four_cc(n: i64) -> String {
    (n as u32)
        .to_be_bytes()
        .iter()
        .map(|&b| b as char)
        .collect()
}

/// Hex digits (with any whitespace) as bytes.
fn unhex(text: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

fn hex_text(n: Option<Node>) -> Option<Vec<u8>> {
    unhex(n?.text()?).filter(|b| !b.is_empty())
}

/// An Audio Unit or VST3 device's plug-in: which one, and its saved sound.
fn plugin_ref(d: Node) -> Option<PluginRef> {
    let desc = at(d, "PluginDesc")?;
    let info = desc.children().find(Node::is_element)?;
    let name = value(info, "PlugName")
        .or_else(|| value(info, "Name"))
        .unwrap_or("Plugin")
        .to_string();
    let vendor = value(info, "Manufacturer").unwrap_or("").to_string();
    let find = |tag: &str| info.descendants().find(|n| n.has_tag_name(tag));
    match info.tag_name().name() {
        "AuPluginInfo" => {
            let code = |p: &str| value(info, p)?.parse::<i64>().ok().map(four_cc);
            let (t, s, m) = (
                code("ComponentType")?,
                code("ComponentSubType")?,
                code("ComponentManufacturer")?,
            );
            Some(PluginRef {
                name,
                vendor,
                instrument: t == "aumu",
                format: PluginFormat::Au(format!("{t}:{s}:{m}")),
                state: hex_text(find("Buffer")).map(Arc::new),
            })
        }
        "Vst3PluginInfo" => {
            let uid = child(info, "Uid")?;
            let id: String = (0..4)
                .map(|i| {
                    let v: i64 = value(uid, &format!("Fields.{i}"))
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    format!("{:08X}", v as u32)
                })
                .collect();
            let component = hex_text(find("ProcessorState"));
            let controller = hex_text(find("ControllerState")).unwrap_or_default();
            Some(PluginRef {
                name,
                vendor,
                // Live's device type for a VST3: 1 an instrument, 2 an effect.
                instrument: value(info, "DeviceType") == Some("1"),
                format: PluginFormat::Vst3(id),
                state: component
                    .map(|c| Arc::new(opendj_plugin_host::presets::pack_states(&c, &controller))),
            })
        }
        _ => None,
    }
}

fn device(d: Node) -> DeviceInfo {
    let tag = d.tag_name().name();
    let on = on(d);
    if tag == "AuPluginDevice" || tag == "PluginDevice" {
        let kind = match at(d, "PluginDesc")
            .and_then(|p| p.children().find(Node::is_element))
            .map(|i| i.tag_name().name())
        {
            Some("AuPluginInfo") => "AU",
            Some("Vst3PluginInfo") => "VST3",
            Some("VstPluginInfo") => "VST",
            _ => "Plugin",
        };
        let plugin = plugin_ref(d);
        let name = plugin.as_ref().map_or_else(
            || {
                at(d, "PluginDesc")
                    .and_then(|p| p.children().find(Node::is_element))
                    .and_then(|i| value(i, "PlugName").or_else(|| value(i, "Name")))
                    .unwrap_or("Plugin")
                    .to_string()
            },
            |p| p.name.clone(),
        );
        return DeviceInfo {
            name,
            kind: kind.into(),
            on,
            plugin,
        };
    }
    if tag == "InstrumentVector" {
        // Wavetable has no open equivalent: Vital plays in its place, from
        // its first sound.
        return DeviceInfo {
            name: "Vital".into(),
            kind: "for Wavetable".into(),
            on,
            plugin: Some(PluginRef {
                name: "Vital".into(),
                vendor: "Vital Audio".into(),
                format: PluginFormat::Named,
                instrument: true,
                state: None,
            }),
        };
    }
    let user = value(d, "UserName").filter(|n| !n.is_empty());
    let live = match tag {
        "Eq8" => "EQ Eight",
        "ChannelEq" => "Channel EQ",
        "AudioEffectGroupDevice" => "Audio Effect Rack",
        "InstrumentGroupDevice" => "Instrument Rack",
        "MidiEffectGroupDevice" => "MIDI Effect Rack",
        "DrumGroupDevice" => "Drum Rack",
        "Hybrid" => "Hybrid Reverb",
        "Compressor2" => "Compressor",
        "GlueCompressor" => "Glue Compressor",
        "Redux2" => "Redux",
        "StereoGain" => "Utility",
        "SpectrumAnalyzer" => "Spectrum",
        "MidiScale" => "Scale",
        "MidiChord" => "Chord",
        "DrumBuss" => "Drum Buss",
        "OriginalSimpler" => "Simpler",
        "MultiSampler" => "Sampler",
        other => other,
    };
    DeviceInfo {
        name: match user {
            Some(u) if u != live => format!("{u} ({live})"),
            _ => live.to_string(),
        },
        kind: "Live".into(),
        on,
        plugin: None,
    }
}

/// The built-in sound that stands in for a MIDI track's instrument.
fn stand_in(track: &str, devices: Option<Node>) -> Sound {
    let mut names = track.to_lowercase();
    for d in devices
        .iter()
        .flat_map(|d| d.children().filter(Node::is_element))
    {
        if d.has_tag_name("DrumGroupDevice") {
            return Sound::Drums("Analog");
        }
        names.push(' ');
        names.push_str(&device(d).name.to_lowercase());
    }
    if ["drum", "kit", "808", "909", "606", "beat"]
        .iter()
        .any(|w| names.contains(w))
    {
        Sound::Drums("Analog")
    } else if names.contains("bass") {
        Sound::Bass
    } else if [
        "pad",
        "pigments",
        "stage",
        "piano",
        "keys",
        "chord",
        "strings",
        "wavetable",
    ]
    .iter()
    .any(|w| names.contains(w))
    {
        Sound::Pad
    } else {
        Sound::Lead
    }
}

/// "AudioIn/External/M0" as "In 1", "S2" as "In 5/6".
fn input_name(target: &str) -> String {
    let last = target.rsplit('/').next().unwrap_or("");
    let n: usize = last.get(1..).and_then(|n| n.parse().ok()).unwrap_or(0);
    match last.chars().next() {
        _ if !target.starts_with("AudioIn/External") => {
            target.rsplit('/').next().unwrap_or("").to_string()
        }
        Some('M') => format!("In {}", n + 1),
        Some('S') => format!("In {}/{}", 2 * n + 1, 2 * n + 2),
        _ => String::new(),
    }
}

/// "MidiIn/External.Dev:Arturia KeyLab Essential 61 (MIDI In){…}/-1" as
/// "Arturia KeyLab Essential 61".
fn midi_input_name(target: &str) -> String {
    match target.split_once(".Dev:") {
        Some((_, dev)) => dev
            .split(['{', '('])
            .next()
            .unwrap_or("")
            .trim()
            .to_string(),
        None if target.contains("External.All") => "All MIDI inputs".into(),
        None => String::new(),
    }
}

/// The loop a clip plays: start and length in clip beats.
fn clip_loop(c: Node) -> (f64, f64) {
    let start = number(c, "Loop/LoopStart").unwrap_or(0.0);
    let end = number(c, "Loop/LoopEnd").unwrap_or(4.0);
    let on = value(c, "Loop/LoopOn") != Some("false");
    if on {
        (start, (end - start).max(0.0))
    } else {
        let a = number(c, "CurrentStart").unwrap_or(start);
        let b = number(c, "CurrentEnd").unwrap_or(end);
        (a, (b - a).max(0.0))
    }
}

fn midi_clip(c: Node) -> Option<Clip> {
    let name = value(c, "Name").unwrap_or("").to_string();
    let (start, beats) = clip_loop(c);
    let mut notes = Vec::new();
    for key in c.descendants().filter(|n| n.has_tag_name("KeyTrack")) {
        let pitch = number(key, "MidiKey").unwrap_or(60.0).clamp(0.0, 127.0) as u8;
        for e in key
            .descendants()
            .filter(|n| n.has_tag_name("MidiNoteEvent"))
        {
            if e.attribute("IsEnabled") == Some("false") {
                continue;
            }
            notes.push(Note {
                start: attr(e, "Time").unwrap_or(0.0) - start,
                length: attr(e, "Duration").unwrap_or(0.25),
                pitch,
                velocity: (attr(e, "Velocity").unwrap_or(100.0) / 127.0).clamp(0.0, 1.0) as f32,
                chance: attr(e, "Probability").unwrap_or(1.0).clamp(0.0, 1.0) as f32,
            });
        }
    }
    Some(Clip::new(
        if name.is_empty() { "MIDI" } else { &name },
        beats,
        notes,
    ))
}

fn audio_clip(c: Node, tempo: f64, folder: &Path) -> Option<Clip> {
    let name = value(c, "Name").unwrap_or("Audio").to_string();
    let warped = value(c, "IsWarped") != Some("false");
    let (start, length) = clip_loop(c);
    let file = at(c, "SampleRef/FileRef");
    let relative = file.and_then(|f| value(f, "RelativePath")).unwrap_or("");
    let absolute = file.and_then(|f| value(f, "Path")).unwrap_or("");
    let path = [folder.join(relative), PathBuf::from(absolute)]
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| folder.join(relative));
    let mut warp: Vec<(f64, f64)> = if warped {
        c.descendants()
            .filter(|n| n.has_tag_name("WarpMarker"))
            .filter_map(|m| Some((attr(m, "BeatTime")?, attr(m, "SecTime")?)))
            .collect()
    } else {
        Vec::new()
    };
    warp.sort_by(|a, b| a.0.total_cmp(&b.0));
    warp.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
    // Unwarped clips keep their loop in seconds: as beats at the set's
    // tempo, read back at the file's own speed.
    let (start, beats) = if warped {
        (start, length)
    } else {
        let per = tempo / 60.0;
        (start * per, length * per)
    };
    let mut clip = Clip::new(&name, beats, Vec::new());
    clip.audio = Some(AudioClip {
        path,
        data: None,
        warp,
        start,
        gain: number(c, "SampleVolume").unwrap_or(1.0) as f32,
    });
    Some(clip)
}

/// Decodes every audio clip's file, a few at a time; returns the files
/// that couldn't be read.
pub fn decode_audio(tracks: &mut [Track]) -> Vec<String> {
    let mut jobs: Vec<&mut AudioClip> = tracks
        .iter_mut()
        .flat_map(|t| t.slots.iter_mut().flatten())
        .filter_map(|c| c.audio.as_mut())
        .collect();
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(6));
    let chunk = jobs.len().div_ceil(threads).max(1);
    let mut missing = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .chunks_mut(chunk)
            .map(|group| {
                scope.spawn(move || {
                    let mut failed = Vec::new();
                    for clip in group.iter_mut() {
                        match opendj_engine::decode::decode(&clip.path) {
                            Ok(a) => clip.data = Some(std::sync::Arc::new(a)),
                            Err(_) => failed.push(clip.path.display().to_string()),
                        }
                    }
                    failed
                })
            })
            .collect();
        for h in handles {
            missing.extend(h.join().unwrap_or_default());
        }
    });
    missing.sort();
    missing.dedup();
    missing
}

/// A summary of a set for `--als-info`.
pub fn summary(set: &LiveSet) -> String {
    let mut out = format!(
        "{:.2} BPM, {} tracks, {} returns, {} scenes\n",
        set.tempo,
        set.tracks.len(),
        set.returns.len(),
        set.scenes
    );
    for (i, t) in set.tracks.iter().enumerate() {
        let kind = if t.sound == Sound::Audio {
            "audio"
        } else {
            "MIDI"
        };
        out += &format!(
            "{:>2} {} ({kind}, #{:06X}, {}{}): plays on {:?}\n",
            i + 1,
            t.name,
            t.color.unwrap_or(0),
            if t.input.is_empty() {
                "no input".into()
            } else {
                t.input.clone()
            },
            t.volume.map_or(String::new(), |v| format!(
                ", fader {}",
                crate::control::fader_text(v)
            )),
            t.sound,
        );
        if !t.devices.is_empty() {
            let d: Vec<String> = t
                .devices
                .iter()
                .map(|d| {
                    let saved = d.plugin.as_ref().is_some_and(|p| p.state.is_some());
                    format!(
                        "{} [{}{}]{}",
                        d.name,
                        d.kind,
                        if saved { ", saved sound" } else { "" },
                        if d.on { "" } else { " (off)" }
                    )
                })
                .collect();
            out += &format!("     devices: {}\n", d.join(", "));
        }
        for (s, c) in t.slots.iter().enumerate() {
            let Some(c) = c else { continue };
            match &c.audio {
                Some(a) => {
                    out += &format!(
                        "     scene {}: {} ({:.1} beats, {} warp markers, {}{})\n",
                        s + 1,
                        c.name,
                        c.beats,
                        a.warp.len(),
                        a.path
                            .file_name()
                            .map_or(String::new(), |f| f.to_string_lossy().into()),
                        if a.path.is_file() {
                            ""
                        } else {
                            ", file not found"
                        }
                    )
                }
                None => {
                    out += &format!(
                        "     scene {}: {} ({:.1} beats, {} notes)\n",
                        s + 1,
                        c.name,
                        c.beats,
                        c.notes.len()
                    )
                }
            }
        }
    }
    for (name, _) in &set.returns {
        out += &format!("return {name}\n");
    }
    for l in &set.left_out {
        out += &format!("left out: {l}\n");
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_lose_a_repeated_position() {
        use super::without_number;
        assert_eq!(without_number("1 Mic", 1), "Mic");
        assert_eq!(without_number("3-Audio", 3), "Audio");
        assert_eq!(without_number("12 Bass", 12), "Bass");
        assert_eq!(without_number("1 Mic", 2), "1 Mic");
        assert_eq!(without_number("808", 8), "808");
        assert_eq!(without_number("10 Pad", 1), "10 Pad");
    }

    use super::*;

    /// A small set in Live 12's layout: an audio track with a warped
    /// recording and a MIDI track with a Drum Rack and a clip.
    const SET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12402" Creator="Ableton Live 12.4.2">
<LiveSet>
<Tracks>
  <AudioTrack Id="1">
    <Name><EffectiveName Value="1 Mic" /></Name>
    <Color Value="2" />
    <DeviceChain>
      <AudioInputRouting><Target Value="AudioIn/External/M0" /></AudioInputRouting>
      <Mixer><Volume><Manual Value="1" /></Volume><Pan><Manual Value="-0.5" /></Pan></Mixer>
      <MainSequencer><ClipSlotList>
        <ClipSlot Id="0"><ClipSlot><Value>
          <AudioClip Id="0" Time="0">
            <Name Value="Take 1" />
            <Loop><LoopStart Value="0" /><LoopEnd Value="16" /><LoopOn Value="true" /></Loop>
            <IsWarped Value="true" />
            <SampleVolume Value="0.5" />
            <SampleRef><FileRef><RelativePath Value="Samples/Recorded/take.wav" /><Path Value="/nowhere/take.wav" /></FileRef></SampleRef>
            <WarpMarkers><WarpMarker Id="0" SecTime="0" BeatTime="0" /><WarpMarker Id="1" SecTime="0.5" BeatTime="1" /></WarpMarkers>
          </AudioClip>
        </Value></ClipSlot></ClipSlot>
        <ClipSlot Id="1"><ClipSlot><Value /></ClipSlot></ClipSlot>
      </ClipSlotList></MainSequencer>
      <DeviceChain><Devices>
        <AudioEffectGroupDevice Id="9"><Branches><AudioEffectBranch Id="0"><DeviceChain><AudioToAudioDeviceChain><Devices>
          <Eq8 Id="2"><UserName Value="" /></Eq8>
        </Devices></AudioToAudioDeviceChain></DeviceChain></AudioEffectBranch></Branches></AudioEffectGroupDevice>
        <AuPluginDevice Id="3"><On><Manual Value="false" /></On><PluginDesc><AuPluginInfo>
          <ComponentType Value="1635083896" /><ComponentSubType Value="926044743" /><ComponentManufacturer Value="1094798689" />
          <Name Value="ValhallaVintageVerb" /><Manufacturer Value="Valhalla DSP, LLC" />
          <Preset><AuPreset Id="1"><Buffer>3C3F786D
            6C3E</Buffer></AuPreset></Preset>
        </AuPluginInfo></PluginDesc></AuPluginDevice>
      </Devices></DeviceChain>
    </DeviceChain>
  </AudioTrack>
  <MidiTrack Id="4">
    <Name><EffectiveName Value="808 Core Kit" /></Name>
    <Color Value="12" />
    <DeviceChain>
      <MidiInputRouting><Target Value="MidiIn/External.Dev:Arturia KeyLab Essential 61 (MIDI In){1C75}/-1" /></MidiInputRouting>
      <Mixer><Volume><Manual Value="0.5" /></Volume><Pan><Manual Value="0" /></Pan></Mixer>
      <MainSequencer><ClipSlotList>
        <ClipSlot Id="0"><ClipSlot><Value>
          <MidiClip Id="1" Time="0">
            <Name Value="" />
            <Loop><LoopStart Value="0" /><LoopEnd Value="4" /><LoopOn Value="true" /></Loop>
            <Notes><KeyTracks>
              <KeyTrack Id="0"><Notes>
                <MidiNoteEvent Time="0" Duration="0.25" Velocity="127" IsEnabled="true" Probability="1" />
                <MidiNoteEvent Time="2" Duration="0.25" Velocity="64" IsEnabled="true" Probability="0.5" />
                <MidiNoteEvent Time="3" Duration="0.25" Velocity="64" IsEnabled="false" />
              </Notes><MidiKey Value="36" /></KeyTrack>
            </KeyTracks></Notes>
          </MidiClip>
        </Value></ClipSlot></ClipSlot>
      </ClipSlotList></MainSequencer>
      <DeviceChain><Devices>
        <DrumGroupDevice Id="5"><UserName Value="808 Core Kit" /></DrumGroupDevice>
        <PluginDevice Id="6"><PluginDesc><Vst3PluginInfo>
          <Uid><Fields.0 Value="-1412567295" /><Fields.1 Value="1" /><Fields.2 Value="2" /><Fields.3 Value="3" /></Uid>
          <DeviceType Value="2" /><Name Value="TONE3000" />
          <Preset><Vst3Preset Id="2"><ProcessorState>0102</ProcessorState><ControllerState /></Vst3Preset></Preset>
        </Vst3PluginInfo></PluginDesc></PluginDevice>
        <InstrumentVector Id="8" />
      </Devices></DeviceChain>
    </DeviceChain>
  </MidiTrack>
  <ReturnTrack Id="7"><Name><EffectiveName Value="A-Reverb" /></Name><Color Value="25" /></ReturnTrack>
</Tracks>
<MainTrack><DeviceChain><Mixer><Tempo><Manual Value="99.5" /></Tempo></Mixer></DeviceChain></MainTrack>
<Scenes><Scene Id="0" /><Scene Id="1" /></Scenes>
</LiveSet>
</Ableton>"#;

    #[test]
    fn reads_tracks_clips_and_devices() {
        let set = parse(SET, Path::new("/sets")).expect("parses");
        assert_eq!(set.tempo, 99.5);
        assert_eq!(set.scenes, 2);
        assert_eq!(set.returns, vec![("A-Reverb".to_string(), 0xB677C6)]);
        let [mic, kit] = &set.tracks[..] else {
            panic!("two tracks")
        };
        assert_eq!(
            (mic.name.as_str(), mic.sound, mic.color),
            ("Mic", Sound::Audio, Some(0xCC9927))
        );
        assert_eq!(mic.input, "In 1");
        assert_eq!(mic.pan, 0.25);
        assert!((mic.volume.unwrap() - crate::control::UNITY).abs() < 1e-4);
        let devices: Vec<(&str, &str)> = mic
            .devices
            .iter()
            .map(|d| (d.name.as_str(), d.kind.as_str()))
            .collect();
        // The one-chain rack opens up into its devices.
        assert_eq!(
            devices,
            [("EQ Eight", "Live"), ("ValhallaVintageVerb", "AU")]
        );
        let verb = &mic.devices[1];
        assert!(!verb.on && mic.devices[0].on);
        let p = verb.plugin.as_ref().expect("a plug-in");
        assert_eq!(p.format, PluginFormat::Au("aufx:72RG:AAMa".into()));
        assert_eq!(p.state.as_deref().map(Vec::as_slice), Some(&b"<?xml>"[..]));
        let take = mic.slots[0].as_ref().expect("a recording");
        let audio = take.audio.as_ref().unwrap();
        assert_eq!((take.beats, audio.gain), (16.0, 0.5));
        assert_eq!(audio.path, Path::new("/sets/Samples/Recorded/take.wav"));
        // Two beats a second, extended past the last marker.
        assert_eq!(audio.seconds(4.0, 99.5), (2.0, 0.5));
        assert!(mic.slots[1].is_none());

        assert_eq!(kit.sound, Sound::Drums("Analog"));
        assert_eq!(kit.input, "Arturia KeyLab Essential 61");
        assert_eq!(kit.devices[0].name, "808 Core Kit (Drum Rack)");
        assert_eq!(kit.devices[1].kind, "VST3");
        let tone = kit.devices[1].plugin.as_ref().expect("a plug-in");
        assert_eq!(
            tone.format,
            PluginFormat::Vst3("ABCDEF01000000010000000200000003".into())
        );
        assert!(!tone.instrument);
        assert_eq!(
            tone.state.as_deref().map(Vec::as_slice),
            Some(&opendj_plugin_host::presets::pack_states(&[1, 2], &[])[..])
        );
        // Wavetable: Vital plays in its place.
        let vital = kit.devices[2].plugin.as_ref().expect("Vital");
        assert_eq!(
            (vital.name.as_str(), &vital.format),
            ("Vital", &PluginFormat::Named)
        );
        assert!(vital.instrument);
        let clip = kit.slots[0].as_ref().unwrap();
        assert_eq!(
            (clip.name.as_str(), clip.beats, clip.notes.len()),
            ("MIDI", 4.0, 2)
        );
        assert_eq!(clip.notes[1].chance, 0.5);
        assert_eq!(clip.notes[0].velocity, 1.0);
    }

    #[test]
    fn reads_gzipped_files() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("als-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.als");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(SET.as_bytes()).unwrap();
        std::fs::write(&path, gz.finish().unwrap()).unwrap();
        let set = read(&path).expect("reads");
        assert_eq!(set.tracks.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
