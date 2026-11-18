//! Waveforms drawn on the CPU into images: the whole track once, and a few
//! seconds around the play position every frame.
//!
//! The desktop app draws its waveforms with wgpu through Slint's GPU
//! interop. That interop isn't known to work with the Skia renderer Slint
//! uses on iPadOS, and two small images a frame are cheap, so the tablet
//! build draws them itself.

use opendj_engine::Track;
use slint::{Rgba8Pixel, SharedPixelBuffer};

/// The background behind the waveforms (the list's darker row colour).
const BACKGROUND: Rgba8Pixel = Rgba8Pixel {
    r: 0x10,
    g: 0x11,
    b: 0x12,
    a: 0xff,
};

/// Colour for a slice from its bands, as Mixxx's RGB waveforms: bass red,
/// mids green, highs blue, scaled so the strongest band is at full.
fn colour(low: u8, mid: u8, high: u8) -> [u8; 3] {
    let top = low.max(mid).max(high).max(1) as u32;
    let scale = |v: u8| (v as u32 * 255 / top) as u8;
    // A floor keeps quiet slices visible against the background.
    let lift = |v: u8| 60 + (v as u32 * 195 / 255) as u8;
    [lift(scale(low)), lift(scale(mid)), lift(scale(high))]
}

fn blank(width: u32, height: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buffer = SharedPixelBuffer::new(width.max(1), height.max(1));
    buffer.make_mut_slice().fill(BACKGROUND);
    buffer
}

/// Draws a vertical bar centred on the middle row, `level` 0..255 of the
/// half height.
fn bar(pixels: &mut [Rgba8Pixel], width: u32, height: u32, x: u32, level: u8, rgb: [u8; 3]) {
    let half = height / 2;
    let reach = (level as u32 * half / 255).max(1);
    let colour = Rgba8Pixel {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
        a: 0xff,
    };
    for y in half.saturating_sub(reach)..(half + reach).min(height) {
        pixels[(y * width + x) as usize] = colour;
    }
}

/// The whole track in `width` × `height` pixels; blank until it is analysed.
pub fn overview(track: &Track, width: u32, height: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buffer = blank(width, height);
    let slices = track.waveform.overview(width as usize);
    let pixels = buffer.make_mut_slice();
    for (x, s) in slices.iter().enumerate() {
        // s: peak, RMS, mids' share, highs' share (the bass has the rest).
        let (mid, high) = (s[2], s[3]);
        let low = 255u8.saturating_sub(mid).saturating_sub(high);
        bar(
            pixels,
            width,
            height,
            x as u32,
            s[0],
            colour(low, mid, high),
        );
    }
    buffer
}

/// `span` seconds of the track centred on `at`, with a tick on each beat
/// (brighter on each bar's first).
pub fn detail(
    track: &Track,
    at: f64,
    span: f64,
    width: u32,
    height: u32,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buffer = blank(width, height);
    let pixels = buffer.make_mut_slice();
    let waveform = &track.waveform;
    let per_pixel = span / width as f64;
    let start = at - span / 2.0;
    // The coarsest level that still has a slice per pixel, so a column
    // never has to look at many slices.
    let finest_per_pixel = per_pixel * waveform.bins_per_second;
    let level = (finest_per_pixel.max(1.0).log2().floor() as usize)
        .min(waveform.levels.len().saturating_sub(1));
    if let Some(bins) = waveform.levels.get(level).filter(|b| !b.is_empty()) {
        let bins_per_second = waveform.bins_per_second / (1u64 << level) as f64;
        for x in 0..width {
            let t0 = start + x as f64 * per_pixel;
            let t1 = t0 + per_pixel;
            if t1 <= 0.0 {
                continue;
            }
            let i0 = (t0.max(0.0) * bins_per_second) as usize;
            let i1 = ((t1 * bins_per_second).ceil() as usize).max(i0 + 1);
            if i0 >= bins.len() {
                break;
            }
            let part = &bins[i0..i1.min(bins.len())];
            let peak = |k: usize| part.iter().map(|b| b[k]).max().unwrap_or(0);
            bar(
                pixels,
                width,
                height,
                x,
                peak(3),
                colour(peak(0), peak(1), peak(2)),
            );
        }
    }
    // Beat ticks along the top and bottom edges.
    if track.bpm > 0.0 {
        let grid = &track.grid;
        let beat_at = |s: f64| {
            if grid.is_empty() {
                (s - track.first_beat) * track.bpm / 60.0
            } else {
                grid.beat_at(s)
            }
        };
        let seconds_at = |b: f64| {
            if grid.is_empty() {
                track.first_beat + b * 60.0 / track.bpm
            } else {
                grid.seconds_at(b)
            }
        };
        let mut beat = beat_at(start).ceil();
        loop {
            let s = seconds_at(beat);
            if s > start + span || !s.is_finite() {
                break;
            }
            if s >= 0.0 {
                let x = ((s - start) / per_pixel) as u32;
                if x < width {
                    let bar_start = (beat as i64).rem_euclid(4) == 0;
                    let tick = if bar_start { height / 4 } else { height / 10 };
                    let shade = if bar_start { 0xff } else { 0x90 };
                    let c = Rgba8Pixel {
                        r: shade,
                        g: shade,
                        b: shade,
                        a: 0xff,
                    };
                    for y in (0..tick).chain(height.saturating_sub(tick)..height) {
                        pixels[(y * width + x) as usize] = c;
                    }
                }
            }
            beat += 1.0;
        }
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendj_engine::Waveform;
    use std::sync::Arc;

    fn track() -> Track {
        let finest = (0..10_000).map(|i| [200, 100, 50, (i % 256) as u8, 0, 0, 0, 0]);
        let mut t = opendj_engine::scan::read_track(0, std::path::Path::new("none.mp3"));
        t.waveform = Arc::new(Waveform::from_finest(1000.0, finest.collect()));
        t.bpm = 120.0;
        t.first_beat = 0.0;
        t.duration = 10.0;
        t
    }

    #[test]
    fn draws_around_the_play_position() {
        let t = track();
        let img = detail(&t, 5.0, 4.0, 400, 100);
        assert_eq!((img.width(), img.height()), (400, 100));
        // Something other than background is drawn in the middle row.
        let row = &img.as_slice()[50 * 400..51 * 400];
        assert!(row.iter().any(|p| *p != BACKGROUND));
    }

    #[test]
    fn leaves_before_the_start_blank() {
        let t = track();
        let img = detail(&t, 0.0, 4.0, 400, 100);
        let middle = &img.as_slice()[50 * 400..51 * 400];
        // The first half is before the track starts.
        assert!(middle[..190].iter().all(|p| *p == BACKGROUND));
        assert!(middle[210..].iter().any(|p| *p != BACKGROUND));
    }

    #[test]
    fn overview_is_blank_until_analysed() {
        let mut t = track();
        t.waveform = Arc::new(Waveform::default());
        let img = overview(&t, 100, 20);
        assert!(img.as_slice().iter().all(|p| *p == BACKGROUND));
    }
}
