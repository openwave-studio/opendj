//! Waveforms drawn on the CPU into images: the whole track once, and a few
//! seconds around the play position every frame.
//!
//! The desktop app draws its waveforms with wgpu through Slint's GPU
//! interop. That interop isn't known to work with the Skia renderer Slint
//! uses on iPadOS, and two small images a frame are cheap, so the tablet
//! draws them itself, with the desktop's RGB look: the same band colours
//! (`opendj_waveform`'s default palette), the same body and height curve,
//! and bar lines through the waveform.

use opendj_engine::Track;
use slint::{Rgba8Pixel, SharedPixelBuffer};

/// The waveform well (`Theme.wave-well`).
const WELL: [f32; 3] = [
    0x0d as f32 / 255.0,
    0x0d as f32 / 255.0,
    0x0e as f32 / 255.0,
];
/// Bass, mids and highs, as the desktop's default RGB palette: dark to
/// light, deep red #C8372D, amber #F0A830, ice blue #B4E4FF, so brightness
/// follows the sound (darker = heavier).
const LOW: [f32; 3] = [0.784, 0.216, 0.176];
const MID: [f32; 3] = [0.941, 0.659, 0.188];
const HIGH: [f32; 3] = [0.706, 0.894, 1.0];
/// The analyser lifts the mids and highs; the colours leave that out, so
/// a quiet pad over a bassline still leans to the bass (`BAND_LIFT`).
const LIFT: [f32; 3] = [1.0, 1.4, 2.0];

/// A stored level (a square root, 0..255) as a linear level.
fn level(b: u8) -> f32 {
    let v = b as f32 / 255.0;
    v * v
}

/// How tall a level draws, 0..1, as the desktop shader's `height`.
fn height(v: f32) -> f32 {
    (v.powf(0.8) * 0.94).clamp(0.0, 1.0)
}

fn vmax(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2])
}

/// The colour of a column from the balance of its bands' levels, as the
/// desktop shader's `band_color`: mixed colours stay as vivid as pure ones,
/// at the brightness of the palette colours they come from, so the deep
/// red lows stay darker than the ice-blue highs.
fn band_colour(low: f32, mid: f32, high: f32) -> [f32; 3] {
    let w = [
        low * low / LIFT[0] + 1e-5,
        mid * mid / LIFT[1] + 1e-5,
        high * high / LIFT[2] + 1e-5,
    ];
    let sum = w[0] + w[1] + w[2];
    let c: [f32; 3] =
        std::array::from_fn(|k| (w[0] * LOW[k] + w[1] * MID[k] + w[2] * HIGH[k]) / sum);
    let v = (w[0] * vmax(LOW) + w[1] * vmax(MID) + w[2] * vmax(HIGH)) / sum;
    let top = vmax(c).max(0.35);
    c.map(|x| x / top * v)
}

/// How tall a slice draws, 0..1, as the desktop shader's `shape`: zoomed
/// out, the squashed height that keeps quiet parts readable; zoomed in
/// (`close` 1), the slice's true loudness, so the gaps between hits and
/// bass cycles stay open instead of filling into a solid block.
fn shape(body: f32, peak: f32, close: f32) -> f32 {
    let squashed = height(body);
    let loud = ((peak * peak).powf(0.85) * 0.94).clamp(0.0, 1.0);
    squashed + (loud - squashed) * close
}

/// 0 zoomed out, rising to 1 once a pixel covers about one finest slice
/// (a millisecond) or less: the shader's `close_zoom`.
fn close_zoom(bins_per_pixel: f64) -> f32 {
    let t = ((bins_per_pixel - 1.0) / 3.0).clamp(0.0, 1.0);
    (1.0 - t * t * (3.0 - 2.0 * t)) as f32
}

fn pixel(c: [f32; 3]) -> Rgba8Pixel {
    let to = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba8Pixel {
        r: to(c[0]),
        g: to(c[1]),
        b: to(c[2]),
        a: 0xff,
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
}

fn blank(width: u32, height: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buffer = SharedPixelBuffer::new(width.max(1), height.max(1));
    buffer.make_mut_slice().fill(pixel(WELL));
    buffer
}

/// A column centred on the middle row, reaching `reach` (0..1) of the half
/// height, with a soft edge: a bright core fading darker towards the edges,
/// for depth, as the desktop's RGB look.
fn column(pixels: &mut [Rgba8Pixel], width: u32, height: u32, x: u32, reach: f32, c: [f32; 3]) {
    let half = height as f32 / 2.0;
    let edge = reach * half;
    if edge <= 0.0 {
        return;
    }
    // Only the rows the column reaches; only its two edge pixels are
    // blended with what's under them.
    let top = (half - edge - 1.0).floor().max(0.0) as u32;
    let bottom = ((half + edge + 1.0).ceil() as u32).min(height);
    for y in top..bottom {
        let d = (y as f32 + 0.5 - half).abs();
        let cover = (edge - d + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 {
            continue;
        }
        let t = (1.0 - d / edge.max(1.0)).clamp(0.0, 1.0);
        let shade = 0.6 + 0.4 * t.sqrt();
        let shaded = c.map(|v| v * shade);
        let p = &mut pixels[(y * width + x) as usize];
        if cover >= 1.0 {
            *p = pixel(shaded);
        } else {
            let base = [p.r as f32 / 255.0, p.g as f32 / 255.0, p.b as f32 / 255.0];
            *p = pixel(mix(base, shaded, cover));
        }
    }
}

/// Lays a vertical line of `c` at `alpha` over column `x`, rows `from..to`.
fn line(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    x: u32,
    rows: std::ops::Range<u32>,
    c: [f32; 3],
    alpha: f32,
) {
    for y in rows {
        let p = &mut pixels[(y * width + x) as usize];
        let base = [p.r as f32 / 255.0, p.g as f32 / 255.0, p.b as f32 / 255.0];
        *p = pixel(mix(base, c, alpha));
    }
}

/// The whole track in `width` × `height` pixels; blank until it is analysed.
pub fn overview(track: &Track, width: u32, height: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buffer = blank(width, height);
    let slices = track.waveform.overview(width as usize);
    let pixels = buffer.make_mut_slice();
    for (x, s) in slices.iter().enumerate() {
        // s: peak, RMS, then the mids' and highs' shares of the power.
        let (mid, high) = (s[2] as f32 / 255.0, s[3] as f32 / 255.0);
        let low = (1.0 - mid - high).max(0.0);
        let body = (level(s[1]) * 2.0).min(level(s[0]));
        let c = band_colour(low.sqrt(), mid.sqrt(), high.sqrt());
        column(pixels, width, height, x as u32, self::height(body), c);
    }
    buffer
}

/// `span` seconds of the track centred on `at`, with a line on each bar
/// and a short tick on each beat along the top and bottom.
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
    let close = close_zoom(finest_per_pixel);
    let level_index = (finest_per_pixel.max(1.0).log2().floor() as usize)
        .min(waveform.levels.len().saturating_sub(1));
    if let Some(bins) = waveform.levels.get(level_index).filter(|b| !b.is_empty()) {
        let bins_per_second = waveform.bins_per_second / (1u64 << level_index) as f64;
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
            let top = |k: usize| level(part.iter().map(|b| b[k]).max().unwrap_or(0));
            let body = (top(7) * 2.0).min(top(3));
            let c = band_colour(top(4), top(5), top(6));
            column(pixels, width, height, x, shape(body, top(3), close), c);
        }
    }
    // Bars and beats.
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
        let white = [1.0, 1.0, 1.0];
        let mut beat = beat_at(start).ceil();
        loop {
            let s = seconds_at(beat);
            if s > start + span || !s.is_finite() {
                break;
            }
            if s >= 0.0 {
                let x = ((s - start) / per_pixel) as u32;
                if x < width {
                    if (beat as i64).rem_euclid(4) == 0 {
                        line(pixels, width, x, 0..height, white, 0.55);
                    } else {
                        let tick = height / 8;
                        line(pixels, width, x, 0..tick, white, 0.4);
                        line(pixels, width, x, height - tick..height, white, 0.4);
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
    #[test]
    fn column_fills_its_rows_only() {
        let mut px = vec![pixel(WELL); 10 * 40];
        column(&mut px, 10, 40, 3, 0.5, [1.0, 0.0, 0.0]);
        let at = |y: usize| px[y * 10 + 3];
        assert!(at(20).r > 200 && at(20).g == 0);
        assert!(at(12).r < at(20).r, "darker towards the edge");
        assert_eq!(at(0), pixel(WELL));
        assert_eq!(at(39), pixel(WELL));
        assert_eq!(px[20 * 10 + 2], pixel(WELL));
    }

    use super::*;
    use opendj_engine::Waveform;
    use std::sync::Arc;

    fn track() -> Track {
        let finest = (0..10_000).map(|i| {
            let v = (i % 200) as u8 + 40;
            [200, 100, 50, v, 180, 90, 40, v / 2]
        });
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
        let row = &img.as_slice()[50 * 400..51 * 400];
        assert!(row.iter().any(|p| *p != pixel(WELL)));
    }

    #[test]
    fn leaves_before_the_start_blank() {
        let t = track();
        let img = detail(&t, 0.0, 4.0, 400, 100);
        // Away from the bar line at the start, the first half is empty.
        let middle = &img.as_slice()[50 * 400..51 * 400];
        assert!(middle[..190].iter().all(|p| *p == pixel(WELL)));
        assert!(middle[210..].iter().any(|p| *p != pixel(WELL)));
    }

    #[test]
    fn bass_heavy_is_deep_red_and_darker_than_highs() {
        let low = band_colour(1.0, 0.05, 0.05);
        assert!((low[0] - LOW[0]).abs() < 0.03 && low[1] < 0.3 && low[2] < 0.3);
        let high = band_colour(0.05, 0.05, 1.0);
        assert!(vmax(high) > vmax(low));
    }

    #[test]
    fn opens_up_when_zoomed_in() {
        // A quiet slice: squashed high zoomed out, its true level up close.
        assert!(shape(0.1, 0.1, 0.0) > 0.1);
        assert!(shape(0.1, 0.1, 1.0) < 0.03);
        assert_eq!(close_zoom(8.0), 0.0);
        assert_eq!(close_zoom(1.0), 1.0);
    }

    #[test]
    fn overview_is_blank_until_analysed() {
        let mut t = track();
        t.waveform = Arc::new(Waveform::default());
        let img = overview(&t, 100, 20);
        assert!(img.as_slice().iter().all(|p| *p == pixel(WELL)));
    }
}
