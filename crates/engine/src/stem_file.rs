//! NI STEM files (`.stem.mp4`) made from a track's high-quality stems, so
//! the track carries them to other DJ software and computers.
//!
//! An NI STEM file is an MP4 with five audio tracks: the track itself
//! first (what players without stems play), then drums, bass, other and
//! vocals, with a JSON manifest of their names and colours in
//! `moov/udta/stem`. These are Apple Lossless (ALAC) at the track's rate,
//! 16-bit, so the stems come out exactly as kept in the cache.

use std::io::Write;
use std::path::{Path, PathBuf};

use alac_encoder::{AlacEncoder, FormatDescription};

use crate::stems::{default_info, StemSource, OTHER};

/// Frames in each ALAC packet (one MP4 sample).
const PACKET: usize = alac_encoder::DEFAULT_FRAMES_PER_PACKET as usize;
/// Packets in each MP4 chunk: the five tracks take turns about every
/// second, so a player reading them together reads the file in order.
const CHUNK_PACKETS: usize = 10;

/// The tags a stem file carries.
#[derive(Clone, Debug, Default)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// The file name for a track's stem file: its own name, `.stem.mp4`.
pub fn file_name(track: &Path) -> String {
    let stem = track
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Track");
    format!("{stem}.stem.mp4")
}

/// Writes an NI STEM file of the file at `track` to `to`, from its
/// high-quality stems in the cache. An error if it has none yet, or once
/// `keep_going` says no.
pub fn write(
    track: &Path,
    to: &Path,
    tags: &Tags,
    keep_going: &dyn Fn() -> bool,
) -> Result<(), String> {
    let Some(StemSource::Files(files)) = crate::stems::find(track) else {
        return Err("no high-quality stems yet".into());
    };
    let mix = crate::decode::decode(track)?;
    let rate = mix.sample_rate;
    let len = mix.len();
    let [ml, mr] = crate::separation::channels(&mix);
    drop(mix);
    // Drums, bass, other and vocals; Other is what the rest leave of the
    // track when the cache didn't keep it.
    let mut stems: Vec<Option<[Vec<f32>; 2]>> = Vec::new();
    for (k, f) in files.iter().enumerate() {
        if k == OTHER && !f.is_file() {
            stems.push(None);
            continue;
        }
        let audio = crate::decode::decode(f)?;
        if audio.sample_rate != rate {
            return Err("the stems aren't at the track's rate".into());
        }
        let [mut l, mut r] = crate::separation::channels(&audio);
        l.resize(len, 0.0);
        r.resize(len, 0.0);
        stems.push(Some([l, r]));
    }
    if !keep_going() {
        return Err("stopped".into());
    }
    let other = stems[OTHER].is_none().then(|| {
        [(&ml, 0), (&mr, 1)].map(|(m, c)| {
            (0..len)
                .map(|i| m[i] - stems.iter().flatten().map(|s| s[c][i]).sum::<f32>())
                .collect::<Vec<f32>>()
        })
    });
    let mut tracks: Vec<[&[f32]; 2]> = vec![[&ml, &mr]];
    for (k, s) in stems.iter().enumerate() {
        match (s, &other) {
            (Some([l, r]), _) => tracks.push([l, r]),
            (None, Some([l, r])) if k == OTHER => tracks.push([l, r]),
            _ => return Err("a stem is missing".into()),
        }
    }
    let encoded: Vec<Encoded> = std::thread::scope(|s| {
        let jobs: Vec<_> = tracks
            .iter()
            .map(|t| s.spawn(move || encode(*t, rate)))
            .collect();
        jobs.into_iter()
            .map(|j| j.join().expect("encoding doesn't panic"))
            .collect()
    });
    if !keep_going() {
        return Err("stopped".into());
    }
    let part = part_path(to);
    let written = write_mp4(&part, rate, len, &encoded, &udta(tags))
        .map_err(|e| format!("{}: {e}", to.display()))
        .and_then(|()| std::fs::rename(&part, to).map_err(|e| e.to_string()));
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written
}

fn part_path(to: &Path) -> PathBuf {
    let mut name = to.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    to.with_file_name(name)
}

/// A track as ALAC packets, with the decoder's configuration.
struct Encoded {
    cookie: Vec<u8>,
    packets: Vec<Vec<u8>>,
    /// Frames in the last packet.
    last: usize,
}

/// Encodes a stereo track to ALAC, 16-bit.
fn encode([l, r]: [&[f32]; 2], rate: u32) -> Encoded {
    let input = FormatDescription::pcm::<i16>(f64::from(rate), 2);
    let output = FormatDescription::alac(f64::from(rate), PACKET as u32, 2);
    let mut encoder = AlacEncoder::new(&output);
    let mut buf = vec![0u8; output.max_packet_size()];
    let len = l.len().min(r.len());
    let mut packets = Vec::with_capacity(len.div_ceil(PACKET));
    let mut pcm = Vec::with_capacity(PACKET * 4);
    let mut last = PACKET;
    for start in (0..len).step_by(PACKET) {
        let end = (start + PACKET).min(len);
        pcm.clear();
        for i in start..end {
            pcm.extend(crate::stems::to_i16(l[i]).to_le_bytes());
            pcm.extend(crate::stems::to_i16(r[i]).to_le_bytes());
        }
        let n = encoder.encode(&input, &pcm, &mut buf);
        packets.push(buf[..n].to_vec());
        last = end - start;
    }
    Encoded {
        cookie: encoder.magic_cookie(),
        packets,
        last,
    }
}

/// The NI manifest: the stems' names and colours, and the mastering
/// effects NI's format describes (off).
fn manifest() -> String {
    let stems: Vec<serde_json::Value> = default_info()
        .iter()
        .map(|s| serde_json::json!({ "name": s.name, "color": format!("#{:06X}", s.color) }))
        .collect();
    serde_json::json!({
        "version": 1,
        "stems": stems,
        "mastering_dsp": {
            "compressor": {
                "enabled": false, "input_gain": 0.5, "output_gain": 0.5,
                "threshold": 0, "dry_wet": 50, "attack": 0.003, "release": 0.3,
                "ratio": 3, "hp_cutoff": 300
            },
            "limiter": { "enabled": false, "threshold": 0, "ceiling": -0.35, "release": 0.05 }
        }
    })
    .to_string()
}

/// `moov/udta`'s contents: the NI manifest and iTunes-style tags.
fn udta(tags: &Tags) -> Vec<u8> {
    let mut out = boxed(b"stem", manifest().as_bytes());
    let tag = |kind: &[u8; 4], value: &str| {
        // A UTF-8 `data` box (type 1, no locale).
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend(value.as_bytes());
        boxed(kind, &boxed(b"data", &data))
    };
    let mut ilst = Vec::new();
    for (kind, value) in [
        (b"\xA9nam", &tags.title),
        (b"\xA9ART", &tags.artist),
        (b"\xA9alb", &tags.album),
    ] {
        if !value.is_empty() {
            ilst.extend(tag(kind, value));
        }
    }
    ilst.extend(tag(b"\xA9too", "openDJ"));
    let mut hdlr = vec![0u8; 8];
    hdlr[4..8].copy_from_slice(b"mdir");
    hdlr.extend(b"appl");
    hdlr.extend([0u8; 9]);
    let mut meta = full(b"hdlr", 0, &hdlr);
    meta.extend(boxed(b"ilst", &ilst));
    out.extend(full(b"meta", 0, &meta));
    out
}

/// An MP4 box.
fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 8);
    b.extend(((body.len() + 8) as u32).to_be_bytes());
    b.extend(kind);
    b.extend(body);
    b
}

/// An MP4 full box: version 0 and `flags`.
fn full(kind: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = flags.to_be_bytes().to_vec();
    b[0] = 0;
    b.extend(body);
    boxed(kind, &b)
}

fn be32(v: impl TryInto<u32>) -> [u8; 4] {
    v.try_into().unwrap_or(u32::MAX).to_be_bytes()
}

/// The identity matrix of `mvhd` and `tkhd`.
fn matrix() -> Vec<u8> {
    [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect()
}

/// Writes the MP4: `ftyp`, the packets in `mdat` (the tracks taking turns
/// a chunk at a time), then `moov`.
fn write_mp4(
    path: &Path,
    rate: u32,
    frames: usize,
    tracks: &[Encoded],
    udta_body: &[u8],
) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut ftyp = b"M4A ".to_vec();
    ftyp.extend(0u32.to_be_bytes());
    for brand in [b"M4A ", b"mp42", b"isom"] {
        ftyp.extend(brand);
    }
    let ftyp = boxed(b"ftyp", &ftyp);
    f.write_all(&ftyp)?;
    // mdat with a 64-bit size, so any length fits.
    let data: u64 = tracks
        .iter()
        .flat_map(|t| &t.packets)
        .map(|p| p.len() as u64)
        .sum();
    f.write_all(&1u32.to_be_bytes())?;
    f.write_all(b"mdat")?;
    f.write_all(&(data + 16).to_be_bytes())?;
    let mut at = ftyp.len() as u64 + 16;
    let chunks = tracks
        .iter()
        .map(|t| t.packets.len().div_ceil(CHUNK_PACKETS))
        .max()
        .unwrap_or(0);
    let mut offsets: Vec<Vec<u64>> = vec![Vec::with_capacity(chunks); tracks.len()];
    for c in 0..chunks {
        for (t, track) in tracks.iter().enumerate() {
            let from = c * CHUNK_PACKETS;
            let Some(packets) = track
                .packets
                .get(from..(from + CHUNK_PACKETS).min(track.packets.len()))
            else {
                continue;
            };
            if packets.is_empty() {
                continue;
            }
            offsets[t].push(at);
            for p in packets {
                f.write_all(p)?;
                at += p.len() as u64;
            }
        }
    }
    let mut moov = Vec::new();
    let duration = be32(frames);
    // mvhd
    let mut mvhd = Vec::new();
    mvhd.extend([0u8; 8]); // created, modified
    mvhd.extend(rate.to_be_bytes());
    mvhd.extend(duration);
    mvhd.extend(0x0001_0000u32.to_be_bytes()); // rate 1.0
    mvhd.extend(0x0100u16.to_be_bytes()); // volume 1.0
    mvhd.extend([0u8; 10]);
    mvhd.extend(matrix());
    mvhd.extend([0u8; 24]);
    mvhd.extend(be32(tracks.len() + 1));
    moov.extend(full(b"mvhd", 0, &mvhd));
    for (t, track) in tracks.iter().enumerate() {
        moov.extend(trak(t, rate, frames, track, &offsets[t]));
    }
    moov.extend(boxed(b"udta", udta_body));
    f.write_all(&boxed(b"moov", &moov))?;
    f.flush()
}

/// Track `t` (0 the mix, which alone is enabled, so players without stems
/// play just it; all in one alternate group).
fn trak(t: usize, rate: u32, frames: usize, track: &Encoded, chunks: &[u64]) -> Vec<u8> {
    let mut tkhd = Vec::new();
    tkhd.extend([0u8; 8]);
    tkhd.extend(be32(t + 1));
    tkhd.extend([0u8; 4]);
    tkhd.extend(be32(frames));
    tkhd.extend([0u8; 8]);
    tkhd.extend(0u16.to_be_bytes()); // layer
    tkhd.extend(1u16.to_be_bytes()); // alternate group
    tkhd.extend(0x0100u16.to_be_bytes()); // volume
    tkhd.extend([0u8; 2]);
    tkhd.extend(matrix());
    tkhd.extend([0u8; 8]); // width, height
    let flags = if t == 0 { 3 } else { 2 };
    let tkhd = full(b"tkhd", flags, &tkhd);

    let mut mdhd = Vec::new();
    mdhd.extend([0u8; 8]);
    mdhd.extend(rate.to_be_bytes());
    mdhd.extend(be32(frames));
    mdhd.extend(0x55C4u16.to_be_bytes()); // "und"
    mdhd.extend([0u8; 2]);
    let mut hdlr = vec![0u8; 4];
    hdlr.extend(b"soun");
    hdlr.extend([0u8; 12]);
    hdlr.extend(b"SoundHandler\0");

    // The sample description: ALAC, stereo, 16-bit.
    let mut entry = vec![0u8; 6];
    entry.extend(1u16.to_be_bytes()); // data reference
    entry.extend([0u8; 8]);
    entry.extend(2u16.to_be_bytes());
    entry.extend(16u16.to_be_bytes());
    entry.extend([0u8; 4]);
    entry.extend(if rate <= 0xFFFF { rate << 16 } else { 0 }.to_be_bytes());
    entry.extend(full(b"alac", 0, &track.cookie));
    let mut stsd = 1u32.to_be_bytes().to_vec();
    stsd.extend(boxed(b"alac", &entry));

    let n = track.packets.len();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    if n > 0 {
        if track.last < PACKET {
            if n > 1 {
                runs.push((n - 1, PACKET));
            }
            runs.push((1, track.last));
        } else {
            runs.push((n, PACKET));
        }
    }
    let mut stts = be32(runs.len()).to_vec();
    for (count, delta) in &runs {
        stts.extend(be32(*count));
        stts.extend(be32(*delta));
    }
    let mut stsc_runs = vec![(1usize, CHUNK_PACKETS.min(n))];
    let rest = n % CHUNK_PACKETS;
    if rest != 0 && n > CHUNK_PACKETS {
        stsc_runs.push((chunks.len(), rest));
    }
    let mut stsc = be32(stsc_runs.len()).to_vec();
    for (first, per) in stsc_runs {
        stsc.extend(be32(first));
        stsc.extend(be32(per));
        stsc.extend(be32(1u32));
    }
    let mut stsz = vec![0u8; 4];
    stsz.extend(be32(n));
    for p in &track.packets {
        stsz.extend(be32(p.len()));
    }
    let big = chunks.last().is_some_and(|&o| o > u64::from(u32::MAX));
    let mut stco = be32(chunks.len()).to_vec();
    for &o in chunks {
        if big {
            stco.extend(o.to_be_bytes());
        } else {
            stco.extend((o as u32).to_be_bytes());
        }
    }
    let mut stbl = full(b"stsd", 0, &stsd);
    stbl.extend(full(b"stts", 0, &stts));
    stbl.extend(full(b"stsc", 0, &stsc));
    stbl.extend(full(b"stsz", 0, &stsz));
    stbl.extend(full(if big { b"co64" } else { b"stco" }, 0, &stco));

    let mut minf = full(b"smhd", 0, &[0u8; 4]);
    let dref = {
        let mut d = 1u32.to_be_bytes().to_vec();
        d.extend(full(b"url ", 1, &[]));
        full(b"dref", 0, &d)
    };
    minf.extend(boxed(b"dinf", &dref));
    minf.extend(boxed(b"stbl", &stbl));

    let mut mdia = full(b"mdhd", 0, &mdhd);
    mdia.extend(full(b"hdlr", 0, &hdlr));
    mdia.extend(boxed(b"minf", &minf));

    let mut trak = tkhd;
    trak.extend(boxed(b"mdia", &mdia));
    boxed(b"trak", &trak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stems::{Decode, MAX_STEMS};

    #[test]
    fn names_follow_the_track() {
        assert_eq!(
            file_name(Path::new("/m/Artist - Song.mp3")),
            "Artist - Song.stem.mp4"
        );
    }

    #[test]
    fn a_stem_file_carries_the_cached_stems_exactly() {
        let dir = std::env::temp_dir().join(format!("opendj-stemfile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A 48 kHz track (a little over three packets per chunk boundary)
        // and its three kept stems, each a tone of its own.
        let rate = 48_000;
        let n = CHUNK_PACKETS * PACKET + 12_345;
        let tone =
            |hz: f32, i: usize| (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin() * 0.2;
        let stem = |k: usize, i: usize| tone([60.0, 110.0, 330.0, 880.0][k], i);
        let track = dir.join("Song.flac");
        let mix: Vec<f32> = (0..n).map(|i| (0..4).map(|k| stem(k, i)).sum()).collect();
        crate::recorder::write_flac(&track, rate, [&mix, &mix]).unwrap();
        let cache = crate::cache::stems_dir(&track).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        for (k, name) in ["drums", "bass", "other", "vocals"].iter().enumerate() {
            if k != OTHER {
                let s: Vec<f32> = (0..n).map(|i| stem(k, i)).collect();
                crate::recorder::write_flac(&cache.join(format!("{name}.flac")), rate, [&s, &s])
                    .unwrap();
            }
        }
        crate::stems::mark_aligned(&cache);
        let out = dir.join(file_name(&track));
        let tags = Tags {
            title: "Song".into(),
            artist: "Someone".into(),
            album: String::new(),
        };
        write(&track, &out, &tags, &|| true).unwrap();
        assert!(!part_path(&out).exists());

        // openDJ reads it back as an NI STEM file.
        let source = crate::stems::find(&out).unwrap();
        assert_eq!(source, StemSource::NiStem(out.clone()));
        let info = crate::stems::read_manifest(&out).unwrap();
        assert_eq!(info, default_info());
        let back = Decode::open(&source).unwrap().run(|| true).unwrap();
        assert_eq!(back.decoded(), n);
        // The mix is the track; each stem is the cached one, Other what the
        // rest leave of the track; to 16-bit steps.
        let decoded = crate::decode::decode(&out).unwrap();
        assert_eq!(decoded.sample_rate, rate);
        assert_eq!(decoded.len(), n);
        let step = 2.0 / 32_768.0;
        for i in (0..n).step_by(97) {
            assert!((decoded.frame(i)[0] - mix[i]).abs() < step, "mix at {i}");
            for (k, theirs) in [0usize, 1, 2, 3].iter().enumerate() {
                let mut g = [0.0; MAX_STEMS];
                g[*theirs] = 1.0;
                if *theirs == 0 {
                    // Drums are split into kick and hats on reading.
                    g[4] = 1.0;
                }
                let got = back.mix(i, &g, || [0.0; 2])[0];
                assert!(
                    (got - stem(k, i)).abs() < 3.0 * step,
                    "stem {k} at {i}: {got}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&cache);
    }
}
