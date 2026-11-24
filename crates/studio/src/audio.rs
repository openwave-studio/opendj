//! Running the studio on the sound card, or on a timer when there is none.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::engine::{Studio, MAX_BLOCK};

/// Frames per callback we ask for: 5.3 ms at 48 kHz.
pub const BUFFER_FRAMES: u32 = 256;

/// Keeps the audio running; stops it when dropped.
pub enum Output {
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

/// A device's name, for the status line.
pub fn name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "audio device".into())
}

/// The device whose name has `env`'s value in it (any case), if that is
/// set and one does.
fn named(devices: impl Iterator<Item = cpal::Device>, env: &str) -> Option<cpal::Device> {
    let want = std::env::var(env).ok()?.to_lowercase();
    let found = devices
        .into_iter()
        .find(|d| name(d).to_lowercase().contains(&want));
    if found.is_none() {
        eprintln!("{env}: no device named like {want}; using the default");
    }
    found
}

/// The output (the default, or `OPENDJ_STUDIO_OUTPUT`) and its settings,
/// or None when there is no output.
pub fn probe() -> Option<(cpal::Device, cpal::SupportedStreamConfig)> {
    let host = cpal::default_host();
    let device = host
        .output_devices()
        .ok()
        .and_then(|d| named(d, "OPENDJ_STUDIO_OUTPUT"))
        .or_else(|| host.default_output_device())?;
    let config = device.default_output_config().ok()?;
    Some((device, config))
}

/// The sound card input, filling the studio's input ring.
pub struct Input {
    /// Held only to keep the stream running.
    _stream: cpal::Stream,
    pub name: String,
    pub channels: usize,
}

/// Opens the input (the default, or `OPENDJ_STUDIO_INPUT`) at `rate`, all
/// its channels, writing into a ring the studio reads; returns the input
/// and the ring's reading end.
pub fn start_input(rate: u32) -> Result<(Input, rtrb::Consumer<f32>), String> {
    let host = cpal::default_host();
    let device = host
        .input_devices()
        .ok()
        .and_then(|d| named(d, "OPENDJ_STUDIO_INPUT"))
        .or_else(|| host.default_input_device())
        .ok_or("no audio input")?;
    let default = device.default_input_config().map_err(|e| e.to_string())?;
    let config = if default.sample_rate() == rate {
        default
    } else {
        // The same channels at the output's rate, if it can.
        device
            .supported_input_configs()
            .map_err(|e| e.to_string())?
            .filter(|c| c.min_sample_rate() <= rate && rate <= c.max_sample_rate())
            .max_by_key(|c| c.channels())
            .map(|c| c.with_sample_rate(rate))
            .ok_or_else(|| {
                format!(
                    "{} runs at {} Hz, the output at {rate} Hz",
                    name(&device),
                    default.sample_rate()
                )
            })?
    };
    let channels = (config.channels() as usize).min(crate::engine::MAX_CHANNELS);
    // A quarter second of room; the studio keeps only a few blocks of it.
    let (tx, rx) = rtrb::RingBuffer::new(rate as usize / 4 * channels.max(1));
    let stream = match config.sample_format() {
        SampleFormat::F32 => build_input::<f32>(&device, &config, tx),
        SampleFormat::I16 => build_input::<i16>(&device, &config, tx),
        SampleFormat::I32 => build_input::<i32>(&device, &config, tx),
        other => Err(format!("unsupported input format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((
        Input {
            _stream: stream,
            name: name(&device),
            channels,
        },
        rx,
    ))
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
    let channels = all.min(crate::engine::MAX_CHANNELS);
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

/// Starts `studio` on `device`.
pub fn start(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    studio: Studio,
) -> Result<Output, String> {
    let open = |studio: Studio, size| match config.sample_format() {
        SampleFormat::F32 => build::<f32>(device, config, size, studio),
        SampleFormat::I16 => build::<i16>(device, config, size, studio),
        SampleFormat::I32 => build::<i32>(device, config, size, studio),
        other => Err(format!("unsupported sample format {other:?}")),
    };
    let size = match config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            cpal::BufferSize::Fixed(BUFFER_FRAMES.clamp(*min, *max))
        }
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    };
    let stream = open(studio, size)?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Output::Device(stream))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    buffer_size: cpal::BufferSize,
    mut studio: Studio,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels() as usize;
    let rate = config.sample_rate() as f64;
    // Tracks sent to Ext. Out reach every output channel.
    studio.engine.set_outputs(channels);
    // Allocated here, never in the callback.
    let mut stereo = vec![[0.0f32; 2]; MAX_BLOCK];
    let monitor = studio.monitor.clone();
    let mut load = 0.0f32;
    device
        .build_output_stream::<T, _, _>(
            cpal::StreamConfig {
                buffer_size,
                ..config.config()
            },
            move |data: &mut [T], _| {
                let began = Instant::now();
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
                monitor.set_load(load);
            },
            |e| eprintln!("audio output: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Runs `studio` in real time on a thread, sending the sound nowhere.
pub fn start_silent(mut studio: Studio) -> Output {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let rate = studio.rate();
    std::thread::Builder::new()
        .name("studio-silent-audio".into())
        .spawn(move || {
            let mut buf = vec![[0.0f32; 2]; BUFFER_FRAMES as usize];
            let block = Duration::from_secs_f64(BUFFER_FRAMES as f64 / rate);
            let mut next = Instant::now();
            while !flag.load(Ordering::Relaxed) {
                let began = Instant::now();
                studio.process(&mut buf);
                let used = began.elapsed().as_secs_f32() / block.as_secs_f32();
                studio.monitor.set_load(used);
                next += block;
                if let Some(wait) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        })
        .expect("audio thread starts");
    Output::Silent(stop)
}
