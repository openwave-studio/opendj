//! Decodes a whole audio file into memory with Symphonia.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use crate::track::{Audio, AudioWriter};

/// File extensions the scanner offers and the decoder can read.
pub const EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "wave", "ogg", "oga", "m4a", "mp4", "aac", "aif", "aiff", "caf", "mka",
];

/// Decodes `path` to stereo frames. Mono is duplicated to both sides; channels
/// beyond the first two are dropped.
pub fn decode(path: &Path) -> Result<Audio, String> {
    let audio = Decoder::open(path)?.run(|_| true)?;
    Ok(Arc::try_unwrap(audio).unwrap_or_else(|_| unreachable!("the writer is gone")))
}

/// How much later the same sound is in rekordbox than in openDJ, in
/// seconds, for the file at `path`: what to add to a time going to
/// rekordbox (a stick, its XML) and take off one coming from it. Only MP3s
/// differ. openDJ decodes them gaplessly, cutting the encoder and decoder
/// delay a LAME tag names (1105 samples for LAME's usual 576); rekordbox
/// keeps it, and also plays an `Info` header frame with a table of contents
/// (LAME's and ffmpeg's CBR) as a frame of silence, where it skips a `Xing`
/// frame as openDJ does. Measured against rekordbox 7's own waveforms by
/// Bake'n Deck (MIT licence, Copyright (c) Masanari Higashi). 0 for other
/// files and for files that can't be read.
pub fn rekordbox_offset(path: &Path) -> f64 {
    rekordbox_lead(path).map_or(0.0, |(frames, rate)| frames as f64 / rate as f64)
}

/// [`rekordbox_offset`] in frames, with the sample rate.
fn rekordbox_lead(path: &Path) -> Option<(u32, u32)> {
    use std::io::{Seek, SeekFrom};
    use symphonia::core::codecs::audio::well_known::CODEC_ID_MP3;
    let ext = path.extension().and_then(|e| e.to_str())?;
    if !ext.eq_ignore_ascii_case("mp3") {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let info_frame = info_frame_samples(&mut file).unwrap_or(0);
    file.seek(SeekFrom::Start(0)).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let track = format.default_track(TrackType::Audio)?;
    let Some(CodecParameters::Audio(params)) = &track.codec_params else {
        return None;
    };
    let rate = params.sample_rate.filter(|&r| r > 0)?;
    (params.codec == CODEC_ID_MP3).then(|| (track.delay.unwrap_or(0) + info_frame, rate))
}

/// The samples of an MP3's first frame when it is an `Info` header with a
/// table of contents: 1152 for MPEG-1, 576 for MPEG-2 and 2.5.
fn info_frame_samples(file: &mut File) -> Option<u32> {
    use std::io::{Read, Seek, SeekFrom};
    let mut head = |at: u64| -> Option<Vec<u8>> {
        file.seek(SeekFrom::Start(at)).ok()?;
        let mut buf = Vec::new();
        file.by_ref().take(64 * 1024).read_to_end(&mut buf).ok()?;
        Some(buf)
    };
    let mut buf = head(0)?;
    if buf.starts_with(b"ID3") && buf.len() >= 10 {
        let size = buf[6..10]
            .iter()
            .fold(0u64, |n, &b| n << 7 | u64::from(b & 0x7f));
        let footer = if buf[5] & 0x10 != 0 { 10 } else { 0 };
        buf = head(10 + size + footer)?;
    }
    // The first frame that another follows, so a stray sync isn't taken for it.
    let start = (0..buf.len()).find(|&i| {
        mp3_frame(&buf[i..])
            .is_some_and(|(len, _)| buf.get(i + len..).and_then(mp3_frame).is_some())
    })?;
    let (_, side_info) = mp3_frame(&buf[start..])?;
    let tag = buf.get(start + 4 + side_info..start + 12 + side_info)?;
    let toc = tag[7] & 4 != 0;
    let mpeg1 = (buf[start + 1] >> 3) & 3 == 3;
    (&tag[..4] == b"Info" && toc).then_some(if mpeg1 { 1152 } else { 576 })
}

/// The length and side information size of the MPEG Layer III frame that
/// `b` starts with.
fn mp3_frame(b: &[u8]) -> Option<(usize, usize)> {
    const MPEG1: [usize; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const MPEG2: [usize; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    if b.len() < 4 || b[0] != 0xff || b[1] & 0xe0 != 0xe0 {
        return None;
    }
    let version = (b[1] >> 3) & 3; // 3 MPEG-1, 2 MPEG-2, 0 MPEG-2.5
    let layer3 = (b[1] >> 1) & 3 == 1;
    let bitrate = (b[2] >> 4) as usize;
    let rate = ((b[2] >> 2) & 3) as usize;
    if version == 1 || !layer3 || bitrate == 0 || bitrate == 15 || rate == 3 {
        return None;
    }
    let (kbps, rates, per_kbit) = match version {
        3 => (MPEG1[bitrate], [44100, 48000, 32000], 144),
        2 => (MPEG2[bitrate], [22050, 24000, 16000], 72),
        _ => (MPEG2[bitrate], [11025, 12000, 8000], 72),
    };
    let len = per_kbit * kbps * 1000 / rates[rate] + ((b[2] >> 1) & 1) as usize;
    let mono = b[3] >> 6 == 3;
    let side_info = match (version == 3, mono) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    };
    Some((len, side_info))
}

/// An opened file, ready to decode into an [`Audio`] that can play while the
/// rest is still decoding.
pub struct Decoder {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    writer: AudioWriter,
    /// Whether the file says how long it is; if not, the length is only an
    /// estimate and the audio shouldn't play until decoding is done.
    length_known: bool,
}

impl Decoder {
    /// Opens `path` and reads its headers; doesn't decode any audio yet.
    pub fn open(path: &Path) -> Result<Decoder, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Decoder::open_source(path, Box::new(file))
    }

    /// Opens `path` while it's still arriving: it will hold `size` bytes,
    /// and reads wait for them while `wanted` says so.
    pub fn open_growing(
        path: &Path,
        size: u64,
        wanted: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Decoder, String> {
        let file = crate::growing::GrowingFile::open(path, size, wanted)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Decoder::open_source(path, Box::new(file))
    }

    /// Reads the headers of `source`, the bytes of `path`.
    fn open_source(path: &Path, source: Box<dyn MediaSource>) -> Result<Decoder, String> {
        let err = |e: Error| format!("{}: {e}", path.display());
        let source = pad_short_aiff(source).map_err(|e| format!("{}: {e}", path.display()))?;
        let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(err)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| format!("{}: no audio track", path.display()))?;
        let track_id = track.id;
        let n_frames = track.num_frames;
        let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
            return Err(format!("{}: unknown codec", path.display()));
        };
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(err)?;
        let sample_rate = params.sample_rate.unwrap_or(0);
        let length_known = n_frames.is_some() && sample_rate > 0;
        // Without a length, guess ten minutes; the audio grows past it as needed.
        let expected = n_frames.map_or(600 * sample_rate.max(44_100) as usize, |n| n as usize);
        let (_, writer) = Audio::writer(sample_rate, expected);
        Ok(Decoder {
            path: path.to_owned(),
            format,
            decoder,
            track_id,
            writer,
            length_known,
        })
    }

    /// Moves to `seconds` into the file, for [`Decoder::stream`] to go on
    /// from there. Returns how many frames the stream gives before that
    /// point (decoding starts at the packet holding it), to be skipped.
    pub fn seek(&mut self, seconds: f64) -> Result<u64, String> {
        use symphonia::core::formats::{SeekMode, SeekTo};
        use symphonia::core::units::Time;
        if seconds <= 0.0 {
            return Ok(0);
        }
        let time = Time::try_from_secs_f64(seconds)
            .ok_or_else(|| format!("{}: can't seek to {seconds} s", self.path.display()))?;
        let to = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        self.decoder.reset();
        Ok((to.required_ts.get() - to.actual_ts.get()).max(0) as u64)
    }

    /// The audio being decoded; frames appear in it as [`Decoder::run`] goes.
    pub fn audio(&self) -> &Arc<Audio> {
        self.writer.audio()
    }

    pub fn length_known(&self) -> bool {
        self.length_known
    }

    /// The file's length in seconds, when its headers say.
    pub fn seconds(&self) -> Option<f64> {
        let a = self.writer.audio();
        (self.length_known && a.sample_rate > 0).then(|| a.len() as f64 / a.sample_rate as f64)
    }

    /// Decodes to the end without keeping the audio: `each` gets the sample
    /// rate and each packet as interleaved stereo, and stops decoding early
    /// by returning false. For files too long to hold in memory, such as a
    /// recorded set. Returns the sample rate and the frames decoded.
    pub fn stream(
        mut self,
        mut each: impl FnMut(u32, &[f32]) -> bool,
    ) -> Result<(u32, u64), String> {
        let path = self.path.clone();
        let err = |e: Error| format!("{}: {e}", path.display());
        let mut buf: Vec<f32> = Vec::new();
        let mut stereo: Vec<f32> = Vec::new();
        let mut rate = 0;
        let mut frames = 0u64;
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(err(e)),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(err(e)),
            };
            let spec = decoded.spec();
            if rate == 0 {
                rate = spec.rate();
            } else if rate != spec.rate() {
                return Err(format!("{}: sample rate changes", path.display()));
            }
            let channels = spec.channels().count().max(1);
            decoded.copy_to_vec_interleaved(&mut buf);
            stereo.clear();
            for f in buf.chunks_exact(channels) {
                stereo.extend_from_slice(&[f[0], if channels > 1 { f[1] } else { f[0] }]);
            }
            frames += (stereo.len() / 2) as u64;
            if !each(rate, &stereo) {
                return Err(format!("{}: decoding cancelled", path.display()));
            }
        }
        if frames == 0 || rate == 0 {
            return Err(format!("{}: no audio decoded", path.display()));
        }
        Ok((rate, frames))
    }

    /// Decodes to the end, calling `progress` with the audio after each
    /// packet; decoding stops early if it returns false.
    pub fn run(mut self, mut progress: impl FnMut(&Audio) -> bool) -> Result<Arc<Audio>, String> {
        let path = self.path.clone();
        let err = |e: Error| format!("{}: {e}", path.display());
        let mut buf: Vec<f32> = Vec::new();
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(err(e)),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                // A corrupt packet is skipped, as players do.
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(err(e)),
            };
            let spec = decoded.spec();
            if self.writer.audio().sample_rate != spec.rate() {
                if self.writer.audio().decoded() > 0 {
                    return Err(format!("{}: sample rate changes", path.display()));
                }
                // The headers didn't give the rate; start over with the real one.
                let expected = self.writer.audio().len();
                self.writer = Audio::writer(spec.rate(), expected).1;
                self.length_known = false;
            }
            let channels = spec.channels().count().max(1);
            decoded.copy_to_vec_interleaved(&mut buf);
            for f in buf.chunks_exact(channels) {
                self.writer
                    .push([f[0], if channels > 1 { f[1] } else { f[0] }]);
            }
            if !progress(self.writer.audio()) {
                return Err(format!("{}: decoding cancelled", path.display()));
            }
        }
        let audio = self.writer.finish();
        if audio.is_empty() || audio.sample_rate == 0 {
            return Err(format!("{}: no audio decoded", path.display()));
        }
        Ok(audio)
    }
}

/// AIFF files whose last chunk has an odd length often leave out the pad
/// byte the FORM length counts (some taggers write them like that), and
/// Symphonia then fails with an unexpected end of file. If an AIFF source is
/// shorter than its FORM header says, this reads the missing tail as zeros.
fn pad_short_aiff(mut source: Box<dyn MediaSource>) -> std::io::Result<Box<dyn MediaSource>> {
    use std::io::{Read, Seek, SeekFrom};
    let Some(real) = source.byte_len() else {
        return Ok(source);
    };
    if real < 12 || !source.is_seekable() {
        return Ok(source);
    }
    let mut head = [0u8; 12];
    source.read_exact(&mut head)?;
    source.seek(SeekFrom::Start(0))?;
    let total = 8 + u64::from(u32::from_be_bytes([head[4], head[5], head[6], head[7]]));
    let aiff = &head[..4] == b"FORM" && matches!(&head[8..], b"AIFF" | b"AIFC");
    if !aiff || total <= real {
        return Ok(source);
    }
    Ok(Box::new(ZeroTail {
        inner: source,
        real,
        total,
        pos: 0,
    }))
}

/// A source that goes on past its real end with zeros up to `total` bytes.
struct ZeroTail {
    inner: Box<dyn MediaSource>,
    real: u64,
    total: u64,
    pos: u64,
}

impl std::io::Read for ZeroTail {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = if self.pos < self.real {
            let want = buf.len().min((self.real - self.pos) as usize);
            self.inner.read(&mut buf[..want])?
        } else {
            let n = buf.len().min(self.total.saturating_sub(self.pos) as usize);
            buf[..n].fill(0);
            n
        };
        self.pos += n as u64;
        Ok(n)
    }
}

impl std::io::Seek for ZeroTail {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        use std::io::SeekFrom;
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.total.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        }
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before start")
        })?;
        self.inner.seek(SeekFrom::Start(pos.min(self.real)))?;
        self.pos = pos;
        Ok(pos)
    }
}

impl MediaSource for ZeroTail {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.total)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// Writes a 16-bit PCM WAV file.
    pub(crate) fn write_wav(path: &Path, rate: u32, channels: u16, samples: &[i16]) {
        let data_len = (samples.len() * 2) as u32;
        let mut f = File::create(path).unwrap();
        let block = channels * 2;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * block as u32).to_le_bytes()).unwrap();
        f.write_all(&block.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        for s in samples {
            f.write_all(&s.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn decodes_mono_wav_to_stereo() {
        let dir = std::env::temp_dir().join(format!("opendj-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        let samples: Vec<i16> = (0..48_000)
            .map(|i| ((i as f32 / 48.0).sin() * 16_000.0) as i16)
            .collect();
        write_wav(&path, 48_000, 1, &samples);
        let audio = decode(&path).unwrap();
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.len(), 48_000);
        assert!(audio.is_complete());
        let f = audio.frame(100);
        assert_eq!(f[0], f[1]);
        assert!((f[0] - samples[100] as f32 / 32_768.0).abs() < 1e-3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Writes a 16-bit stereo AIFF with a trailing odd-length ID3 chunk whose
    /// pad byte is left out, like files some taggers write.
    fn write_aiff_missing_pad(path: &Path, rate: u32, frames: &[[i16; 2]]) {
        let mut comm = Vec::new();
        comm.extend_from_slice(&2u16.to_be_bytes());
        comm.extend_from_slice(&(frames.len() as u32).to_be_bytes());
        comm.extend_from_slice(&16u16.to_be_bytes());
        // Sample rate as an 80-bit extended float.
        let exp = 31 - rate.leading_zeros();
        comm.extend_from_slice(&((16_383 + exp) as u16).to_be_bytes());
        comm.extend_from_slice(&((u64::from(rate)) << (63 - exp)).to_be_bytes());
        let mut ssnd = vec![0u8; 8];
        for f in frames {
            ssnd.extend_from_slice(&f[0].to_be_bytes());
            ssnd.extend_from_slice(&f[1].to_be_bytes());
        }
        // An ID3v2.4 tag with one TIT2 frame, 23 bytes long in all.
        let mut id3 = b"ID3\x04\x00\x00\x00\x00\x00\x0d".to_vec();
        id3.extend_from_slice(b"TIT2\x00\x00\x00\x03\x00\x00\x03Hi");
        assert_eq!(id3.len() % 2, 1);
        let mut body = b"AIFF".to_vec();
        for (tag, data) in [(b"COMM", &comm), (b"SSND", &ssnd), (b"ID3 ", &id3)] {
            body.extend_from_slice(tag);
            body.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
        }
        // The FORM length counts the pad byte, which isn't written.
        let mut f = File::create(path).unwrap();
        f.write_all(b"FORM").unwrap();
        f.write_all(&(body.len() as u32 + 1).to_be_bytes()).unwrap();
        f.write_all(&body).unwrap();
    }

    #[test]
    fn decodes_aiff_missing_last_pad_byte() {
        let dir = std::env::temp_dir().join(format!("opendj-aiff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("short.aiff");
        let frames: Vec<[i16; 2]> = (0..44_100)
            .map(|i| {
                let v = ((i as f32 / 30.0).sin() * 16_000.0) as i16;
                [v, -v]
            })
            .collect();
        write_aiff_missing_pad(&path, 44_100, &frames);
        let audio = decode(&path).unwrap();
        assert_eq!(audio.sample_rate, 44_100);
        assert_eq!(audio.len(), 44_100);
        let f = audio.frame(1_000);
        assert_eq!(f[0], frames[1_000][0] as f32 / 32_768.0);
        assert_eq!(f[1], frames[1_000][1] as f32 / 32_768.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn audio_plays_while_decoding() {
        let dir = std::env::temp_dir().join(format!("opendj-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("long.wav");
        // Ten seconds, so it spans several chunks.
        let samples: Vec<i16> = (0..441_000)
            .flat_map(|i| {
                let v = ((i as f32 / 20.0).sin() * 16_000.0) as i16;
                [v, -v]
            })
            .collect();
        write_wav(&path, 44_100, 2, &samples);
        let d = Decoder::open(&path).unwrap();
        assert!(d.length_known());
        let audio = d.audio().clone();
        assert_eq!(audio.len(), 441_000);
        assert_eq!(audio.decoded(), 0);
        let mut seen_partial = false;
        let done = d
            .run(|a| {
                if a.decoded() > 0 && !a.is_complete() {
                    seen_partial = true;
                    assert_eq!(a.frame(100)[0], samples[200] as f32 / 32_768.0);
                    // Not decoded yet: silence.
                    assert_eq!(a.frame(a.len() - 1), [0.0; 2]);
                }
                true
            })
            .unwrap();
        assert!(seen_partial);
        assert!(Arc::ptr_eq(&done, &audio) && audio.is_complete());
        let last = audio.frame(440_999);
        assert_eq!(last[1], samples[2 * 440_999 + 1] as f32 / 32_768.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An MP3 of 40 silent frames (MPEG-1 Layer III, 44.1 kHz, 128 kb/s)
    /// after an ID3v2 tag, led by a `Xing` or `Info` frame (`tag`) with a
    /// table of contents and a LAME tag naming LAME's usual encoder delay.
    fn write_mp3(path: &Path, tag: Option<&[u8; 4]>) {
        let frame = || {
            let mut f = vec![0u8; 417];
            f[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
            f
        };
        let mut data = b"ID3\x04\0\0\0\0\0\x20".to_vec();
        data.resize(10 + 32, 0);
        if let Some(tag) = tag {
            let mut t = tag.to_vec();
            t.extend(5u32.to_be_bytes()); // a frame count and a table of contents
            t.extend(40u32.to_be_bytes());
            t.extend([0; 100]);
            t.extend(b"LAME3.100");
            t.extend([0; 12]);
            t.extend([0x24, 0x03, 0xe8]); // delay 576, padding 1000
            let mut f = frame();
            f[36..36 + t.len()].copy_from_slice(&t);
            data.extend(f);
        }
        for _ in 0..40 {
            data.extend(frame());
        }
        std::fs::write(path, data).unwrap();
    }

    #[test]
    fn rekordbox_runs_later_for_lame_tagged_mp3s() {
        let dir = std::env::temp_dir().join(format!("opendj-rb-offset-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let offset = |name: &str, tag: Option<&[u8; 4]>| {
            let path = dir.join(name);
            write_mp3(&path, tag);
            (rekordbox_offset(&path) * 44_100.0).round()
        };
        // The 576 the LAME tag names and the decoder's 529, which openDJ cuts.
        assert_eq!(offset("xing.mp3", Some(b"Xing")), 1105.0);
        // And a frame of silence for the Info frame.
        assert_eq!(offset("info.mp3", Some(b"Info")), 2257.0);
        assert_eq!(offset("plain.mp3", None), 0.0);
        let wav = dir.join("a.wav");
        write_wav(&wav, 44_100, 2, &[0; 64]);
        assert_eq!(rekordbox_offset(&wav), 0.0);
        assert_eq!(rekordbox_offset(&dir.join("gone.mp3")), 0.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
