//! Stem separation inside openDJ (`crates/separation`), without Python:
//! Spleeter whenever a track loads, so its stems are there within a second
//! or two, starting where the DJ will play from; HTDemucs when the DJ asks
//! for high-quality stems, kept in the cache.
//!
//! The model files are downloaded to [`models_dir`] the first time they
//! are needed, through the app's [`set_fetch`]; without them a track
//! simply has no stems on load, and high-quality stems fall back to the
//! Demucs command if it is installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use opendj_separation::rates::Rates;
use opendj_separation::spleeter;

use crate::dsp::Lr4;
use crate::stems::{
    default_info, split, split_drums, to_i16, Quality, StemFrame, StemSource, Stems, CHUNK,
    KICK_SPLIT_HZ, MAX_STEMS,
};
use crate::track::Audio;

/// The Spleeter weights (`scripts/models/convert_spleeter.py`).
pub const SPLEETER: &str = "spleeter-4stems.safetensors";
/// The HTDemucs weights.
pub const HTDEMUCS: &str = "htdemucs.safetensors";

/// Each model file and its SHA-256, as the `stem-models` release of openDJ
/// has them (`.github/workflows/stem-models.yml`).
pub const MODELS: [(&str, &str); 2] = [
    (
        SPLEETER,
        "4f3c1011feda8f2cccaef9735fad04f272c87bd5ad0a93dd53a3d622c4ed31c5",
    ),
    (
        HTDEMUCS,
        "8193504cdfb3943adaf039b8acb524a46e87ebf232c383ac7a32c80a6578423e",
    ),
];

/// Downloads a model file: `(name, sha256, to)`. The app sets it, so the
/// engine itself never goes online.
pub type Fetch = dyn Fn(&str, &str, &Path) -> Result<(), String> + Send + Sync;

static FETCH: OnceLock<Box<Fetch>> = OnceLock::new();

/// Sets how missing model files are downloaded.
pub fn set_fetch(fetch: Box<Fetch>) {
    let _ = FETCH.set(fetch);
}

/// The model file `name`, downloaded first if it isn't yet (one download
/// at a time), checked against its SHA-256 in `models`.
pub(crate) fn model_or_fetch(name: &str, models: &[(&str, &str)]) -> Result<PathBuf, String> {
    static DOWNLOADING: Mutex<()> = Mutex::new(());
    let _turn = DOWNLOADING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = model(name) {
        return Ok(p);
    }
    let fetch = FETCH.get().ok_or("the models aren't downloaded")?;
    let (_, sha) = models
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or("unknown model")?;
    let dir = models_dir().ok_or("no folder for the models")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    let part = path.with_extension("part");
    let result = fetch(name, sha, &part)
        .and_then(|()| std::fs::rename(&part, &path).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
        .map(|()| path)
        .map_err(|e| format!("couldn't download {name}: {e}"))
}

/// Where the separation models are kept: `OPENDJ_MODELS`, else `models`
/// in the cache folder (they can always be downloaded again).
pub fn models_dir() -> Option<PathBuf> {
    match std::env::var_os("OPENDJ_MODELS") {
        Some(d) => Some(PathBuf::from(d)),
        None => crate::cache::models_dir(),
    }
}

/// The model file `name`, if it has been downloaded.
pub fn model(name: &str) -> Option<PathBuf> {
    let p = models_dir()?.join(name);
    p.is_file().then_some(p)
}

/// Whether loaded tracks are separated with Spleeter (Settings › Analysis ›
/// Stems on load); on unless the app turns it off.
static ON_LOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// The most bytes of high-quality stems kept in the cache; 0 keeps all.
static CACHE_LIMIT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(DEFAULT_CACHE_LIMIT);

/// 10 GB: about 150 tracks of five minutes.
pub const DEFAULT_CACHE_LIMIT: u64 = 10 << 30;

/// Sets whether tracks loaded from now on get stems at once.
pub fn set_on_load(on: bool) {
    ON_LOAD.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether tracks get stems when they load.
pub fn on_load() -> bool {
    ON_LOAD.load(std::sync::atomic::Ordering::Relaxed)
}

/// Sets the most bytes of high-quality stems kept (0 for no limit); the
/// cache is trimmed to it after each separation.
pub fn set_cache_limit(bytes: u64) {
    CACHE_LIMIT.store(bytes, std::sync::atomic::Ordering::Relaxed);
}

/// The most bytes of high-quality stems kept; 0 for no limit.
pub fn cache_limit() -> u64 {
    CACHE_LIMIT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Trims the kept stems to [`cache_limit`], keeping those of `keep`.
pub fn trim_cache(keep: &[PathBuf]) -> usize {
    match cache_limit() {
        0 => 0,
        limit => crate::cache::trim_stems(limit, keep),
    }
}

/// Samples of the drums before a chunk that the kick/hi-hat split runs
/// over first, so its filters have settled where the chunk starts.
const SPLIT_LEAD: usize = 8192;

/// Spleeter's stems ([`spleeter::STEMS`]: vocals, drums, bass, other) for
/// each of ours (drums, bass, other, vocals).
const FROM_SPLEETER: [usize; 4] = [1, 2, 3, 0];

/// The Spleeter networks, loaded once.
pub(crate) fn spleeter_net() -> Result<Arc<spleeter::cpu::Net>, String> {
    static NET: Mutex<Option<Arc<spleeter::cpu::Net>>> = Mutex::new(None);
    let mut net = NET.lock().unwrap();
    if let Some(n) = net.as_ref() {
        return Ok(n.clone());
    }
    let path = model_or_fetch(SPLEETER, &MODELS)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let n = Arc::new(spleeter::cpu::Net::load(&bytes)?);
    // Half the cores, so a playing deck keeps its share.
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    spleeter::cpu::set_threads((cores / 2).max(1));
    *net = Some(n.clone());
    Ok(n)
}

/// The track's channels, whole.
pub(crate) fn channels(audio: &Audio) -> [Vec<f32>; 2] {
    let mut l = Vec::with_capacity(audio.len());
    let mut r = Vec::with_capacity(audio.len());
    for f in audio.decoded_chunks().flatten() {
        l.push(f[0]);
        r.push(f[1]);
    }
    [l, r]
}

/// Our stems (drums split into kick and hi-hat), from the model's `parts`
/// (`[stem][channel][sample]`, ours `s` being its `from[s]`) at `rate`,
/// leaving out the first `lead` frames, which only settle the split.
#[allow(clippy::needless_range_loop)] // frame by frame across the stems
fn stem_frames(
    parts: &[[&[f32]; 2]],
    from: [usize; 4],
    lead: usize,
    rate: u32,
) -> Box<[StemFrame]> {
    let mut low = Lr4::lowpass(rate as f64, KICK_SPLIT_HZ);
    let n = parts[0][0].len();
    let mut out = Vec::with_capacity(n.saturating_sub(lead));
    for i in 0..n {
        let at = |s: usize| [parts[from[s]][0][i], parts[from[s]][1][i]];
        let (k, h) = split(&mut low, at(0));
        if i < lead {
            continue;
        }
        let mut f: StemFrame = [[0; 2]; MAX_STEMS];
        f[0] = k.map(to_i16);
        for s in 1..4 {
            f[s] = at(s).map(to_i16);
        }
        f[4] = h.map(to_i16);
        out.push(f);
    }
    out.into_boxed_slice()
}

/// Separates the decoded `audio` with Spleeter and gives it the stems,
/// which play at once: the stretch around frame `from` is separated first,
/// in about a second, and the original plays wherever the stems aren't
/// there yet. Does nothing if the audio has stems already. Stops when
/// `keep_going` turns false.
pub fn separate_fast(
    audio: &Audio,
    from: usize,
    keep_going: impl Fn() -> bool,
) -> Result<(), String> {
    if audio.stems().is_some() || audio.is_empty() {
        return Ok(());
    }
    let net = spleeter_net()?;
    let rate = audio.sample_rate;
    let mut info = default_info();
    split_drums(&mut info);
    let stems = Arc::new(Stems::separated(info, rate, audio.len(), Quality::Fast));
    if !audio.set_stems(stems.clone()) {
        return Ok(());
    }
    let [l, r] = channels(audio);
    let model_rate = opendj_separation::SAMPLE_RATE;
    let frames = |parts: &[[&[f32]; 2]], lead: usize| stem_frames(parts, FROM_SPLEETER, lead, rate);
    if rate == model_rate {
        let complete =
            spleeter::separate_in_chunks(&*net, [&l, &r], from, CHUNK, SPLIT_LEAD, |k, parts| {
                let lead = (k * CHUNK).min(SPLIT_LEAD);
                stems.fill(k, frames(&parts, lead));
                keep_going()
            });
        if complete {
            stems.finish();
        }
        return Ok(());
    }
    // Another rate: separate at the model's, from the cue as at 44.1 kHz,
    // and bring each of our chunks back once the model's chunks it is made
    // from are separated, so the stems still play within about a second.
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let there = Rates::new(rate, model_rate);
    let back = Rates::new(model_rate, rate);
    let (ml, mr) = (
        there.resample_all(&l, threads),
        there.resample_all(&r, threads),
    );
    drop((l, r));
    let (len, mlen) = (audio.len(), ml.len());
    let ours = len.div_ceil(CHUNK);
    // Our chunk `t` with its lead, and the model's samples it needs.
    let need = |t: usize| {
        let start = t * CHUNK;
        let lead = start.min(SPLIT_LEAD);
        let out = start - lead..(start + CHUNK).min(len);
        let input = back.input_for(out.clone(), mlen);
        (out, lead, input)
    };
    // Our chunks that may need the model's chunk `m`.
    let users = |m: usize| {
        let at = |x: usize| x * rate as usize / model_rate as usize;
        let lo = at(m * CHUNK).saturating_sub(SPLIT_LEAD + CHUNK / 2) / CHUNK;
        let hi = (at((m + 1) * CHUNK) + CHUNK / 2) / CHUNK;
        lo..(hi + 1).min(ours)
    };
    let mut got: HashMap<usize, Vec<[Vec<f32>; 2]>> = HashMap::new();
    let mut filled = vec![false; ours];
    let complete = spleeter::separate_in_chunks(
        &*net,
        [&ml, &mr],
        (from as f64 * model_rate as f64 / rate as f64) as usize,
        CHUNK,
        0,
        |m, parts| {
            got.insert(
                m,
                parts
                    .iter()
                    .map(|[a, b]| [a.to_vec(), b.to_vec()])
                    .collect(),
            );
            for t in users(m) {
                if filled[t] {
                    continue;
                }
                let (out, lead, input) = need(t);
                let first = input.start / CHUNK;
                let last = (input.end.max(1) - 1) / CHUNK;
                if !(first..=last).all(|k| got.contains_key(&k)) {
                    continue;
                }
                let resampled: Vec<[Vec<f32>; 2]> = (0..parts.len())
                    .map(|stem| {
                        [0, 1].map(|c| {
                            let joined: Vec<f32> = (first..=last)
                                .flat_map(|k| got[&k][stem][c].iter().copied())
                                .collect();
                            let at = input.start - first * CHUNK;
                            let slice = &joined[at..at + input.len()];
                            back.resample_part(slice, input.start, out.clone())
                        })
                    })
                    .collect();
                let views: Vec<[&[f32]; 2]> =
                    resampled.iter().map(|[a, b]| [&a[..], &b[..]]).collect();
                stems.fill(t, frames(&views, lead));
                filled[t] = true;
            }
            // Let go of the model's chunks all their users have.
            got.retain(|&k, _| users(k).any(|t| !filled[t]));
            keep_going()
        },
    );
    if complete && filled.iter().all(|f| *f) {
        stems.finish();
    }
    Ok(())
}

/// Separates the file at `path` into high-quality stems with HTDemucs,
/// leaving four FLAC files at the track's own rate where
/// [`crate::stems::find`] looks. Uses the GPU (Metal on macOS, Vulkan on
/// Linux), or the CPU once the GPU has failed. Without the model, or when
/// both fail, the Demucs command does it if it is installed.
pub fn separate_hq(path: &Path) -> Result<StemSource, String> {
    separate_hq_watched(path, &[], &mut |_, _| {}, &|| true)
}

/// As [`separate_hq`], telling `progress` as each of the model's chunks is
/// done (it may wait, pausing between chunks), and giving up with
/// [`opendj_separation::htdemucs::STOPPED`] once `keep_going` says no,
/// without trying the CPU or the Demucs command after.
///
/// `live` is the track's audio on the decks that have it, each with its
/// play position (a frame), the first deck's first: HTDemucs then starts
/// there and works outwards, and each stretch of the stems plays as soon
/// as it is separated (see [`Stems::set_upgrade`]), so a deck has
/// high-quality stems seconds after asking rather than once the whole
/// track is done.
pub fn separate_hq_watched(
    path: &Path,
    live: &[(Arc<Audio>, usize)],
    progress: &mut dyn FnMut(usize, usize),
    keep_going: &dyn Fn() -> bool,
) -> Result<StemSource, String> {
    use opendj_separation::htdemucs::STOPPED;
    use std::sync::atomic::{AtomicBool, Ordering};
    // A GPU failure can leave burn's locks poisoned for the rest of the
    // run, so after one the CPU does the separating.
    static GPU_FAILED: AtomicBool = AtomicBool::new(false);
    let weights = match model_or_fetch(HTDEMUCS, &MODELS) {
        Ok(w) => w,
        Err(e) => return crate::stems::run_demucs(path).map_err(|_| e),
    };
    let track = crate::decode::decode(path)?;
    let live = attach(live, &track);
    let live = live.as_ref().map(|(up, from)| (&**up, *from));
    let mut result = Err(String::new());
    if !GPU_FAILED.load(Ordering::Relaxed) {
        if let Some(dir) = crate::cache::cache_dir() {
            opendj_separation::set_gpu_cache_dir(dir.join("gpu"));
        }
        result = caught(|| {
            htdemucs::<opendj_separation::Gpu>(
                path,
                &track,
                &weights,
                live,
                &mut *progress,
                keep_going,
            )
        });
        if !keep_going() {
            return Err(STOPPED.into());
        }
        if let Err(e) = &result {
            eprintln!("HTDemucs on the GPU: {e}; using the CPU");
            GPU_FAILED.store(true, Ordering::Relaxed);
        }
    }
    if result.is_err() {
        result = caught(|| {
            htdemucs::<opendj_separation::Cpu>(
                path,
                &track,
                &weights,
                live,
                &mut *progress,
                keep_going,
            )
        });
    }
    match result {
        Ok(s) => Ok(s),
        Err(_) if !keep_going() => Err(STOPPED.into()),
        Err(e) => {
            eprintln!("HTDemucs: {e}; trying the Demucs command");
            crate::stems::run_demucs(path).map_err(|_| e)
        }
    }
}

/// Rewrites the FLAC file at `path` at `rate`.
pub(crate) fn resample_flac(path: &Path, rate: u32) -> Result<(), String> {
    let audio = crate::decode::decode(path)?;
    let [l, r] = channels(&audio);
    let rates = Rates::new(audio.sample_rate, rate);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let (l, r) = (
        rates.resample_all(&l, threads),
        rates.resample_all(&r, threads),
    );
    crate::recorder::write_flac(path, rate, [&l, &r]).map_err(|e| e.to_string())
}

/// Runs `f`, turning a panic into an error with its message.
fn caught<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let msg = p
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(format!("the separation crashed: {msg}"))
    })
}

/// High-quality stems for the decks in `live` that fit `track`: set as
/// the upgrade of the stems they have (or as their stems, if none), to be
/// filled as HTDemucs goes. With the first deck's play position.
fn attach(live: &[(Arc<Audio>, usize)], track: &Audio) -> Option<(Arc<Stems>, usize)> {
    let mut info = default_info();
    split_drums(&mut info);
    let rate = track.sample_rate;
    let up = Arc::new(Stems::separated(info, rate, track.len(), Quality::High));
    let mut from = None;
    for (audio, at) in live {
        if audio.sample_rate != rate {
            continue;
        }
        let given = match audio.stems() {
            Some(s) => s.set_upgrade(up.clone()),
            None => audio.set_stems(up.clone()),
        };
        if given && from.is_none() {
            from = Some((*at).min(track.len()));
        }
    }
    from.map(|f| (up, f))
}

/// Merged stretches of samples.
#[derive(Default)]
struct Stretches(Vec<std::ops::Range<usize>>);

impl Stretches {
    fn add(&mut self, r: std::ops::Range<usize>) {
        self.0.push(r);
        self.0.sort_unstable_by_key(|r| r.start);
        let mut merged: Vec<std::ops::Range<usize>> = Vec::with_capacity(self.0.len());
        for r in self.0.drain(..) {
            match merged.last_mut() {
                Some(m) if r.start <= m.end => m.end = m.end.max(r.end),
                _ => merged.push(r),
            }
        }
        self.0 = merged;
    }

    fn covers(&self, r: &std::ops::Range<usize>) -> bool {
        r.is_empty() || self.0.iter().any(|m| m.start <= r.start && m.end >= r.end)
    }
}

fn htdemucs<B: opendj_separation::burn::prelude::Backend>(
    path: &Path,
    track: &Audio,
    weights: &Path,
    live: Option<(&Stems, usize)>,
    progress: &mut dyn FnMut(usize, usize),
    keep_going: &dyn Fn() -> bool,
) -> Result<StemSource, String> {
    use opendj_separation::htdemucs::HtDemucs;
    let rate = track.sample_rate;
    let len = track.len();
    let [l, r] = channels(track);
    let bytes = std::fs::read(weights).map_err(|e| format!("{}: {e}", weights.display()))?;
    let device = B::Device::default();
    let model = HtDemucs::<B>::load(&bytes, &device)?;
    // At the model's 44.1 kHz. Told 44.1 kHz for a 48 kHz track, it would
    // hear it 9% slow and its stems, stretched back, would drift off the
    // original they are taken out of, so muting one would add it back late
    // instead of removing it.
    let model_rate = opendj_separation::SAMPLE_RATE;
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let back = (rate != model_rate).then(|| Rates::new(model_rate, rate));
    let (ml, mr) = match back {
        Some(_) => {
            let there = Rates::new(rate, model_rate);
            (
                there.resample_all(&l, threads),
                there.resample_all(&r, threads),
            )
        }
        None => (l, r),
    };
    let mlen = ml.len();
    // Each of our chunks with its lead for the drum split, and the model's
    // samples it is made from.
    let need = |t: usize| {
        let start = t * CHUNK;
        let lead = start.min(SPLIT_LEAD);
        let out = start - lead..(start + CHUNK).min(len);
        let input = match &back {
            Some(b) => b.input_for(out.clone(), mlen),
            None => out.clone(),
        };
        (out, lead, input)
    };
    let ours = len.div_ceil(CHUNK);
    let mut filled = vec![false; ours];
    let mut separated = Stretches::default();
    let from = live.map_or(0, |(_, f)| {
        (f as f64 * model_rate as f64 / rate as f64) as usize
    });
    let stems =
        model.separate_from([&ml, &mr], from, progress, keep_going, &mut |range, out| {
            let Some((up, _)) = live else {
                return;
            };
            separated.add(range);
            for (t, done) in filled.iter_mut().enumerate() {
                if *done {
                    continue;
                }
                let (part, lead, input) = need(t);
                if !separated.covers(&input) {
                    continue;
                }
                let parts: Vec<[Vec<f32>; 2]> = out
                    .iter()
                    .map(|stem| {
                        [0, 1].map(|c| match &back {
                            Some(b) => {
                                b.resample_part(&stem[c][input.clone()], input.start, part.clone())
                            }
                            None => stem[c][part.clone()].to_vec(),
                        })
                    })
                    .collect();
                let views: Vec<[&[f32]; 2]> = parts.iter().map(|[a, b]| [&a[..], &b[..]]).collect();
                // HTDemucs's order is ours: drums, bass, other, vocals.
                up.fill(t, stem_frames(&views, [0, 1, 2, 3], lead, rate));
                *done = true;
            }
        })?;
    if let Some((up, _)) = live {
        if filled.iter().all(|f| *f) {
            up.finish();
        }
    }
    let stems: Vec<[Vec<f32>; 2]> = match &back {
        Some(b) => stems
            .into_iter()
            .map(|[sl, sr]| [b.resample_all(&sl, threads), b.resample_all(&sr, threads)])
            .collect(),
        None => stems,
    };
    let dir = crate::cache::stems_dir(path).ok_or("no cache folder")?;
    let work = dir.with_extension(format!("work{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let written = (|| -> Result<(), String> {
        // HTDemucs's order is ours: drums, bass, other, vocals.
        for (name, [sl, sr]) in opendj_separation::htdemucs::STEMS.iter().zip(&stems) {
            let fit = |x: &[f32]| -> Vec<f32> {
                let mut y = x.to_vec();
                y.resize(len, 0.0);
                y
            };
            let (sl, sr) = (fit(sl), fit(sr));
            crate::recorder::write_flac(&work.join(format!("{name}.flac")), rate, [&sl, &sr])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&work);
        return Err(e);
    }
    crate::stems::mark_aligned(&work);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&work, &dir).map_err(|e| e.to_string())?;
    crate::stems::find(path).ok_or_else(|| "the stems weren't written".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the Spleeter weights: `OPENDJ_MODELS=<folder with
    /// spleeter-4stems.safetensors> cargo test -p opendj-engine -- --ignored spleeter_on`.
    /// As above, for a 48 kHz track: separated at the model's rate a chunk
    /// at a time and brought back, the stems match separating it all at
    /// once and resampling the lot.
    #[test]
    #[ignore]
    fn spleeter_on_load_works_a_chunk_at_a_time_at_48_khz() {
        let model_rate = opendj_separation::SAMPLE_RATE;
        let synth = crate::synth::render_with(124.0, 0.25, 40.0, 5);
        let [l, r] = channels(&synth);
        let up = |x: &[f32]| opendj_separation::resample(x, model_rate, 48_000).unwrap();
        let (l, r) = (up(&l), up(&r));
        let audio = Audio::from_frames(48_000, l.iter().zip(&r).map(|(a, b)| [*a, *b]).collect());
        let t = std::time::Instant::now();
        // When the first stems around the cue are there to play.
        let first = std::thread::scope(|s| {
            let watch = s.spawn(|| loop {
                if audio.stems().is_some_and(|st| st.decoded() > 0) {
                    return t.elapsed().as_secs_f64();
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            });
            separate_fast(&audio, 20 * 48_000, || true).unwrap();
            watch.join().unwrap()
        });
        println!(
            "40 s at 48 kHz: first stems in {first:.2} s, all in {:.2} s",
            t.elapsed().as_secs_f64()
        );
        let stems = audio.stems().unwrap();
        assert!(stems.is_complete());
        // All at once, as before.
        let net = spleeter_net().unwrap();
        let down = |x: &[f32]| opendj_separation::resample(x, 48_000, model_rate).unwrap();
        let whole = spleeter::separate(&*net, [&down(&l), &down(&r)], 0, |_, _| {});
        let back: Vec<Vec<f32>> = whole
            .iter()
            .map(|[sl, _]| opendj_separation::resample(sl, model_rate, 48_000).unwrap())
            .collect();
        // Bass, Instru and Vocal (ours 1 to 3) against Spleeter's.
        let mut worst = 0.0f32;
        for ours in 1..4 {
            let mut g = [1.0; MAX_STEMS];
            g[ours] = 0.0;
            for i in (0..audio.len()).step_by(997) {
                let got = -stems.mix(i, &g, || [0.0; 2])[0];
                let want = back[FROM_SPLEETER[ours]][i];
                worst = worst.max((got - want).abs());
            }
        }
        println!("largest difference {worst:.5}");
        assert!(worst < 2e-3, "{worst}");
    }

    #[test]
    #[ignore]
    fn spleeter_on_load_fills_the_stems_from_the_cue() {
        let audio = crate::synth::render_with(124.0, 0.25, 60.0, 5);
        let audio = Audio::from_frames(
            opendj_separation::SAMPLE_RATE,
            audio.decoded_chunks().flatten().copied().collect(),
        );
        let t = std::time::Instant::now();
        separate_fast(&audio, 30 * 44_100, || true).unwrap();
        println!("60 s separated in {:.2} s", t.elapsed().as_secs_f64());
        let stems = audio.stems().unwrap();
        assert!(stems.is_complete());
        assert_eq!(stems.count(), 5);
        assert_eq!(stems.quality, Quality::Fast);
        // The stems add up to the track.
        let i = 20 * 44_100;
        let (orig, all) = (audio.frame(i), stems.mix(i, &[0.0; MAX_STEMS], || [0.0; 2]));
        assert!((orig[0] + all[0]).abs() < 2e-3, "{orig:?} {all:?}");
        // The synth's drums are mostly kick and hats: muting both takes out most of it.
        let rms = |g: [f32; MAX_STEMS]| {
            let e: f32 = (0..44_100 * 10)
                .map(|k| stems.mix(i + k, &g, || audio.frame(i + k))[0].powi(2))
                .sum();
            e.sqrt()
        };
        let full = rms([1.0; MAX_STEMS]);
        let no_drums = rms([0.0, 1.0, 1.0, 1.0, 0.0]);
        println!("full {full:.1}, without kick and hats {no_drums:.1}");
        assert!(no_drums < full);
    }

    /// Needs the HTDemucs weights and takes a few minutes on the CPU:
    /// `OPENDJ_MODELS=<folder with htdemucs.safetensors> cargo test -p
    /// opendj-engine --release -- --ignored hq_stems_from`.
    /// A deck playing a 48 kHz track 10 s in gets high-quality stems there
    /// first, and the stems it is given a stretch at a time are those kept
    /// in the cache.
    #[test]
    #[ignore]
    fn hq_stems_from_the_play_position_match_the_cache() {
        let model_rate = opendj_separation::SAMPLE_RATE;
        let synth = crate::synth::render_with(124.0, 0.25, 16.0, 5);
        let [l, r] = channels(&synth);
        let up = |x: &[f32]| opendj_separation::resample(x, model_rate, 48_000).unwrap();
        let file = std::env::temp_dir().join(format!("opendj-hq-{}.flac", std::process::id()));
        crate::recorder::write_flac(&file, 48_000, [&up(&l), &up(&r)]).unwrap();
        let audio = Arc::new(crate::decode::decode(&file).unwrap());
        let from = 10 * 48_000;
        let t = std::time::Instant::now();
        let (first, source) = std::thread::scope(|s| {
            let watch = s.spawn(|| loop {
                if audio.stems().is_some_and(|st| st.has(from)) {
                    return t.elapsed().as_secs_f64();
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            });
            let source =
                separate_hq_watched(&file, &[(audio.clone(), from)], &mut |_, _| {}, &|| true)
                    .unwrap();
            (watch.join().unwrap(), source)
        });
        println!(
            "16 s at 48 kHz: HQ stems at the play position in {first:.1} s, all in {:.1} s",
            t.elapsed().as_secs_f64()
        );
        let live = audio.stems().unwrap().clone();
        assert!(live.is_complete());
        assert_eq!(live.quality, Quality::High);
        let cached = crate::stems::Decode::open(&source)
            .unwrap()
            .run(|| true)
            .unwrap();
        let stem = |st: &Stems, k: usize, i: usize| {
            let mut g = [1.0; MAX_STEMS];
            g[k] = 0.0;
            -st.mix(i, &g, || [0.0; 2])[0]
        };
        let mut worst = 0.0f32;
        for i in (0..audio.len()).step_by(331) {
            // The cache's 16-bit files clip at full scale where a stem goes
            // past it; the stems kept in memory clip each part of the drums.
            let loud = (0..MAX_STEMS).any(|k| stem(&cached, k, i).abs() > 0.97);
            if loud {
                continue;
            }
            // Bass, Instru and Vocal, and the drums whole (kick and hats
            // split with a little less run-up here than from the file).
            for k in 1..4 {
                worst = worst.max((stem(&live, k, i) - stem(&cached, k, i)).abs());
            }
            let drums = |st: &Stems| stem(st, 0, i) + stem(st, 4, i);
            worst = worst.max((drums(&live) - drums(&cached)).abs());
        }
        println!("largest difference {worst:.5}");
        assert!(worst < 2e-4, "{worst}");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn stems_from_the_demucs_command_come_back_at_the_track_rate() {
        let file =
            std::env::temp_dir().join(format!("opendj-resample-{}.flac", std::process::id()));
        let tone: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.03).sin() * 0.5).collect();
        crate::recorder::write_flac(&file, 44_100, [&tone, &tone]).unwrap();
        resample_flac(&file, 48_000).unwrap();
        let audio = crate::decode::decode(&file).unwrap();
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.len(), 48_000);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn checksums_match_the_published_models() {
        let sums = include_str!("../../../scripts/models/SHA256SUMS");
        for (name, sha) in MODELS {
            assert!(
                sums.lines().any(|l| l == format!("{sha}  {name}")),
                "{name}"
            );
        }
    }

    #[test]
    fn spleeter_stems_map_onto_ours() {
        // Ours: drums, bass, other, vocals; Spleeter's: vocals, drums, bass, other.
        for (ours, theirs) in FROM_SPLEETER.iter().enumerate() {
            let name = spleeter::STEMS[*theirs];
            let want = ["drums", "bass", "other", "vocals"][ours];
            assert_eq!(name, want);
        }
    }
}
