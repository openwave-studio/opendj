//! Running the studio on the sound card, or on a timer when there is none.
//!
//! The studio sits in a shared slot that the output callback borrows each
//! block (a lock only the Audio settings ever wait on), so the devices can
//! change while a set plays: the streams stop, the studio is set up for the
//! new devices, and new streams start on the same slot.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::engine::{Studio, MAX_BLOCK, MAX_CHANNELS};
use crate::settings::AudioSettings;

/// The studio, shared between the audio callback and the settings.
type Shared = Arc<Mutex<Option<Studio>>>;

/// A device's name, as the settings list it.
pub fn name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "audio device".into())
}

fn no_audio() -> bool {
    std::env::var_os("OPENDJ_NO_AUDIO").is_some()
}

/// The output called `name`, or the system default for an empty name (or
/// one no longer there).
fn find_output(want: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if !want.is_empty() {
        if let Some(d) = host
            .output_devices()
            .ok()
            .and_then(|mut d| d.find(|d| name(d) == want))
        {
            return Some(d);
        }
        eprintln!("audio output {want} not found; using the default");
    }
    host.default_output_device()
}

fn find_input(want: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if !want.is_empty() {
        if let Some(d) = host
            .input_devices()
            .ok()
            .and_then(|mut d| d.find(|d| name(d) == want))
        {
            return Some(d);
        }
        eprintln!("audio input {want} not found; using the default");
    }
    host.default_input_device()
}

/// Output and input devices, as the settings list them (the system's own;
/// openDJ's ASIO drivers are left out for now).
pub fn devices() -> (Vec<String>, Vec<String>) {
    if no_audio() {
        return (Vec::new(), Vec::new());
    }
    let names = |list: Vec<opendj_engine::OutputDevice>| {
        list.into_iter()
            .map(|d| d.name)
            .filter(|n| !n.starts_with("ASIO: "))
            .collect()
    };
    (
        names(opendj_engine::output_devices()),
        names(opendj_engine::input_devices()),
    )
}

/// The rate the studio runs at: the settings' if the output can, else the
/// output's own; 48 kHz without an output.
pub fn rate(settings: &AudioSettings) -> f64 {
    if no_audio() {
        return 48_000.0;
    }
    let Some(device) = find_output(&settings.output) else {
        return 48_000.0;
    };
    let own = device
        .default_output_config()
        .map_or(48_000, |c| c.sample_rate());
    let wanted = settings.sample_rate.filter(|&r| {
        device
            .supported_output_configs()
            .is_ok_and(|mut c| c.any(|c| c.min_sample_rate() <= r && r <= c.max_sample_rate()))
    });
    wanted.unwrap_or(own) as f64
}

/// A config of `device` at `rate`: its default if that runs at `rate`,
/// else the one with the most channels that can.
fn config_at(
    default: Result<cpal::SupportedStreamConfig, String>,
    configs: Result<Vec<cpal::SupportedStreamConfigRange>, String>,
    rate: u32,
    device: &str,
) -> Result<cpal::SupportedStreamConfig, String> {
    let default = default?;
    if default.sample_rate() == rate {
        return Ok(default);
    }
    configs?
        .into_iter()
        .filter(|c| c.min_sample_rate() <= rate && rate <= c.max_sample_rate())
        .max_by_key(|c| c.channels())
        .map(|c| c.with_sample_rate(rate))
        .ok_or_else(|| format!("{device} can't run at {rate} Hz"))
}

enum Output {
    /// Held only to keep the stream playing.
    Device(#[allow(dead_code)] cpal::Stream),
    /// No sound card: the engine runs on a timer so the UI still moves.
    Silent(Arc<AtomicBool>),
}

impl Drop for Output {
    fn drop(&mut self) {
        if let Output::Silent(stop) = self {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

/// The studio's sound: its output and input streams.
pub struct Audio {
    shared: Shared,
    output: Option<Output>,
    /// Held only to keep the stream running.
    input: Option<cpal::Stream>,
    /// The studio's sample rate, fixed while a set is open.
    pub rate: f64,
    /// Whether the set wants the input (it has audio tracks).
    wants_input: bool,
    /// For the status line and the settings.
    pub status: String,
    pub out_name: String,
    pub out_channels: usize,
    pub in_name: String,
    pub in_channels: usize,
    pub buffer: u32,
}

impl Audio {
    /// Starts `studio` on the devices `settings` name.
    pub fn start(studio: Studio, settings: &AudioSettings, wants_input: bool) -> Audio {
        let mut audio = Audio {
            rate: studio.rate(),
            shared: Arc::new(Mutex::new(Some(studio))),
            output: None,
            input: None,
            wants_input,
            status: String::new(),
            out_name: String::new(),
            out_channels: 0,
            in_name: String::new(),
            in_channels: 0,
            buffer: settings.buffer,
        };
        audio.open(settings);
        audio
    }

    /// Moves the studio to the devices `settings` name. The sample rate
    /// stays as the set opened with.
    pub fn open(&mut self, settings: &AudioSettings) {
        // Stop the streams first: the callbacks then let go of the studio.
        self.output = None;
        self.input = None;
        self.buffer = settings.buffer;
        let shared = self.shared.clone();
        let Ok(mut guard) = shared.lock() else {
            return;
        };
        let Some(studio) = guard.as_mut() else {
            return;
        };
        let rate = self.rate as u32;
        // The input.
        studio.clear_input();
        self.in_name.clear();
        self.in_channels = 0;
        let mut notes = Vec::new();
        match &settings.input {
            Some(want)
                if self.wants_input
                    && !no_audio()
                    && std::env::var_os("OPENDJ_NO_INPUT").is_none() =>
            {
                match start_input(want, rate) {
                    Ok((stream, ring, device, channels)) => {
                        studio.set_input(ring, channels);
                        self.input = Some(stream);
                        self.in_name = device;
                        self.in_channels = channels;
                    }
                    Err(e) => {
                        eprintln!("audio input: {e}");
                        notes.push(format!("no input: {e}"));
                    }
                }
            }
            _ => {}
        }
        // The output.
        let out = if no_audio() {
            Err("no audio".to_string())
        } else {
            find_output(&settings.output)
                .ok_or_else(|| "no audio output found".to_string())
                .and_then(|d| {
                    let n = name(&d);
                    let config = config_at(
                        d.default_output_config().map_err(|e| e.to_string()),
                        d.supported_output_configs()
                            .map(|c| c.collect())
                            .map_err(|e| e.to_string()),
                        rate,
                        &n,
                    )?;
                    Ok((d, config, n))
                })
        };
        let channels = out.as_ref().map_or(2, |(_, c, _)| c.channels() as usize);
        studio.engine.set_outputs(channels);
        if let Some(link) = studio.link.as_mut() {
            link.set_latency(settings.buffer as f64 / self.rate);
        }
        drop(guard);
        self.out_channels = channels;
        let started = out.and_then(|(device, config, n)| {
            start_output(&device, &config, settings.buffer, self.shared.clone()).map(|s| (s, n))
        });
        match started {
            Ok((stream, n)) => {
                self.output = Some(Output::Device(stream));
                self.status = format!("{n} · {rate} Hz · {} frames", settings.buffer);
                self.out_name = n;
            }
            Err(e) => {
                eprintln!("audio output: {e}");
                self.output = Some(start_silent(self.shared.clone(), self.rate));
                self.status = format!("No sound ({e}): playing silently");
                self.out_name.clear();
                self.out_channels = 2;
            }
        }
        if !self.in_name.is_empty() {
            self.status += &format!(" · in: {} ({} ch)", self.in_name, self.in_channels);
        }
        for n in notes {
            self.status += &format!(" · {n}");
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.output = None;
        self.input = None;
    }
}

/// Opens an input at `rate`, all its channels, writing into a ring the
/// studio reads; returns the stream, the ring's reading end, the device's
/// name and the channels.
fn start_input(
    want: &str,
    rate: u32,
) -> Result<(cpal::Stream, rtrb::Consumer<f32>, String, usize), String> {
    let device = find_input(want).ok_or("no audio input")?;
    let n = name(&device);
    let config = config_at(
        device.default_input_config().map_err(|e| e.to_string()),
        device
            .supported_input_configs()
            .map(|c| c.collect())
            .map_err(|e| e.to_string()),
        rate,
        &n,
    )?;
    let channels = (config.channels() as usize).min(MAX_CHANNELS);
    // A quarter second of room; the studio keeps only a few blocks of it.
    let (tx, rx) = rtrb::RingBuffer::new(rate as usize / 4 * channels.max(1));
    let stream = match config.sample_format() {
        SampleFormat::F32 => build_input::<f32>(&device, &config, tx),
        SampleFormat::I16 => build_input::<i16>(&device, &config, tx),
        SampleFormat::I32 => build_input::<i32>(&device, &config, tx),
        other => Err(format!("unsupported input format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rx, n, channels))
}

fn build_input<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    mut ring: rtrb::Producer<f32>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let all = config.channels() as usize;
    let channels = all.min(MAX_CHANNELS);
    device
        .build_input_stream::<T, _, _>(
            config.config(),
            move |data: &[T], _| {
                // Whole frames, as many as fit.
                let frames = (data.len() / all).min(ring.slots() / channels.max(1));
                if let Ok(chunk) = ring.write_chunk_uninit(frames * channels) {
                    chunk.fill_from_iter(
                        data.chunks(all)
                            .take(frames)
                            .flat_map(|f| f[..channels].iter().map(|s| s.to_sample::<f32>())),
                    );
                }
            },
            |e| eprintln!("audio input: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

fn start_output(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    buffer: u32,
    shared: Shared,
) -> Result<cpal::Stream, String> {
    let size = match config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            cpal::BufferSize::Fixed(buffer.clamp(*min, *max))
        }
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    };
    let stream = match config.sample_format() {
        SampleFormat::F32 => build::<f32>(device, config, size, shared),
        SampleFormat::I16 => build::<i16>(device, config, size, shared),
        SampleFormat::I32 => build::<i32>(device, config, size, shared),
        other => Err(format!("unsupported sample format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    buffer_size: cpal::BufferSize,
    shared: Shared,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels() as usize;
    let rate = config.sample_rate() as f64;
    // Allocated here, never in the callback.
    let mut stereo = vec![[0.0f32; 2]; MAX_BLOCK];
    let mut load = 0.0f32;
    device
        .build_output_stream::<T, _, _>(
            cpal::StreamConfig {
                buffer_size,
                ..config.config()
            },
            move |data: &mut [T], _| {
                let began = Instant::now();
                // Only the settings ever hold it, while they change devices.
                let Ok(mut guard) = shared.try_lock() else {
                    data.fill(T::EQUILIBRIUM);
                    return;
                };
                let Some(studio) = guard.as_mut() else {
                    data.fill(T::EQUILIBRIUM);
                    return;
                };
                for chunk in data.chunks_mut(MAX_BLOCK * channels) {
                    let frames = chunk.len() / channels;
                    let buf = &mut stereo[..frames];
                    studio.process(buf);
                    let ext = studio.engine.ext(frames);
                    for (f, (frame, s)) in chunk.chunks_mut(channels).zip(buf.iter()).enumerate() {
                        for (c, d) in frame.iter_mut().enumerate() {
                            let main = if c < 2 { s[c] } else { 0.0 };
                            let e = ext.get(f * channels + c).copied().unwrap_or(0.0);
                            *d = T::from_sample((main + e).clamp(-1.0, 1.0));
                        }
                    }
                }
                let frames = data.len() / channels.max(1);
                let budget = frames as f32 / rate as f32;
                let used = began.elapsed().as_secs_f32() / budget.max(1e-6);
                load += (used - load) * 0.05;
                studio.monitor.set_load(load);
            },
            |e| eprintln!("audio output: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Runs the studio in real time on a thread, sending the sound nowhere.
fn start_silent(shared: Shared, rate: f64) -> Output {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let frames = crate::settings::DEFAULT_BUFFER as usize;
    std::thread::Builder::new()
        .name("studio-silent-audio".into())
        .spawn(move || {
            let mut buf = vec![[0.0f32; 2]; frames];
            let block = Duration::from_secs_f64(frames as f64 / rate);
            let mut next = Instant::now();
            while !flag.load(Ordering::Relaxed) {
                let began = Instant::now();
                if let Ok(mut guard) = shared.try_lock() {
                    if let Some(studio) = guard.as_mut() {
                        studio.process(&mut buf);
                        let used = began.elapsed().as_secs_f32() / block.as_secs_f32();
                        studio.monitor.set_load(used);
                    }
                }
                next += block;
                if let Some(wait) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        })
        .expect("audio thread starts");
    Output::Silent(stop)
}
