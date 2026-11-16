//! Decode, resize, pick a background color and encode the karaoke cover off
//! the UI thread. Only the latest track's result wins.
use opendj_engine::{
    cover::{self, Cover},
    Source, Track,
};
use std::sync::{mpsc, Arc};

/// The cover drawn on the window: sharp on a 4K screen at its largest.
const COVER_SIDE: u32 = 1600;
/// The cover sent to the web page, for TV browsers on Wi-Fi.
const PAGE_SIDE: u32 = 1000;
/// The background is the cover's average color scaled down to this, so
/// white words stay readable on it.
const BACKGROUND_LEVEL: f32 = 0.42;

struct Request {
    id: u64,
    source: Source,
    album: String,
    fallback: Option<Arc<Cover>>,
}

pub(super) struct Prepared {
    pub cover: Cover,
    /// One flat color from the cover, 0xRRGGBB.
    pub background: u32,
    /// The cover for the web page.
    pub jpeg: Vec<u8>,
}

pub(super) struct Worker {
    next: u64,
    requests: mpsc::Sender<Request>,
    results: mpsc::Receiver<(u64, Option<Prepared>)>,
}

impl Worker {
    pub fn new() -> Self {
        let (requests, rx) = mpsc::channel::<Request>();
        let (tx, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("karaoke-artwork".into())
            .spawn(move || {
                while let Ok(mut request) = rx.recv() {
                    // Rapid track changes shouldn't queue expensive image jobs.
                    while let Ok(newer) = rx.try_recv() {
                        request = newer;
                    }
                    let id = request.id;
                    if tx.send((id, prepare(request))).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn karaoke artwork worker");
        Self {
            next: 0,
            requests,
            results,
        }
    }

    pub fn request(&mut self, track: &Track) -> u64 {
        self.next += 1;
        let _ = self.requests.send(Request {
            id: self.next,
            source: track.source.clone(),
            album: track.album.clone(),
            fallback: track.cover.clone(),
        });
        self.next
    }

    /// Only yields artwork for `wanted`. Pending, absent and stale artwork
    /// leave the placeholder established when the request was sent.
    pub fn poll(&self, wanted: u64) -> Option<Prepared> {
        let mut current = None;
        while let Ok((id, result)) = self.results.try_recv() {
            if id == wanted {
                current = result;
            }
        }
        current
    }
}

fn prepare(request: Request) -> Option<Prepared> {
    let cover = match request.source {
        Source::File(path) => cover::find_sized(&path, &request.album, COVER_SIDE),
        _ => None,
    }
    .or_else(|| {
        request.fallback.map(|c| Cover {
            width: c.width,
            height: c.height,
            rgba: c.rgba.clone(),
        })
    })?;
    let rgba = image::RgbaImage::from_raw(cover.width, cover.height, cover.rgba)?;
    // Apply the same limit to fallback art supplied by streaming sources.
    let rgba = if rgba.width().max(rgba.height()) > COVER_SIDE {
        image::imageops::thumbnail(&rgba, COVER_SIDE, COVER_SIDE)
    } else {
        rgba
    };
    let background = background(&rgba);
    let page = if rgba.width().max(rgba.height()) > PAGE_SIDE {
        image::imageops::thumbnail(&rgba, PAGE_SIDE, PAGE_SIDE)
    } else {
        rgba.clone()
    };
    let rgb = image::DynamicImage::ImageRgba8(page).to_rgb8();
    let mut jpeg = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85),
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )
    .ok()?;
    let cover = Cover {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    };
    Some(Prepared {
        cover,
        background,
        jpeg,
    })
}

/// The cover's average color, darkened so white text keeps its contrast.
fn background(rgba: &image::RgbaImage) -> u32 {
    let small = image::imageops::thumbnail(rgba, 16, 16);
    let mut sum = [0f32; 3];
    for p in small.pixels() {
        for (s, c) in sum.iter_mut().zip(p.0) {
            *s += c as f32;
        }
    }
    let n = (small.width() * small.height()).max(1) as f32;
    let [r, g, b] = sum.map(|s| (s / n * BACKGROUND_LEVEL).round() as u32);
    (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_large_art_and_sends_the_page_a_smaller_jpeg() {
        let fallback = Arc::new(Cover {
            width: 1200,
            height: 1200,
            rgba: [30, 90, 180, 255].repeat(1200 * 1200),
        });
        let result = prepare(Request {
            id: 1,
            source: Source::File("/missing/track.mp3".into()),
            album: String::new(),
            fallback: Some(fallback),
        })
        .unwrap();
        assert_eq!((result.cover.width, result.cover.height), (1200, 1200));
        let page =
            image::load_from_memory_with_format(&result.jpeg, image::ImageFormat::Jpeg).unwrap();
        assert_eq!(page.width(), PAGE_SIDE);
    }

    #[test]
    fn oversized_fallback_art_is_bounded_too() {
        let fallback = Arc::new(Cover {
            width: 1800,
            height: 1200,
            rgba: [30, 90, 180, 255].repeat(1800 * 1200),
        });
        let result = prepare(Request {
            id: 1,
            source: Source::File("/missing/track.mp3".into()),
            album: String::new(),
            fallback: Some(fallback),
        })
        .unwrap();
        assert_eq!(result.cover.width, COVER_SIDE);
        assert!(result.cover.height <= COVER_SIDE);
        assert_eq!(&result.cover.rgba[..4], &[30, 90, 180, 255]);
    }

    #[test]
    fn background_is_the_cover_color_darkened() {
        let blue = image::RgbaImage::from_pixel(40, 40, image::Rgba([100, 150, 250, 255]));
        let c = background(&blue);
        let (r, g, b) = ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
        assert_eq!((r, g, b), (42, 63, 105));
    }

    #[test]
    fn missing_cover_is_a_normal_result() {
        assert!(prepare(Request {
            id: 1,
            source: Source::File("/missing/track.mp3".into()),
            album: String::new(),
            fallback: None
        })
        .is_none());
    }
    #[test]
    fn old_track_results_cannot_replace_the_current_track() {
        let (requests, _) = mpsc::channel();
        let (tx, results) = mpsc::channel();
        let worker = Worker {
            next: 2,
            requests,
            results,
        };
        let prepared = |pixel| Prepared {
            cover: Cover {
                width: 1,
                height: 1,
                rgba: vec![pixel; 4],
            },
            background: 0,
            jpeg: vec![],
        };
        tx.send((2, Some(prepared(200)))).unwrap();
        tx.send((1, Some(prepared(100)))).unwrap();
        assert_eq!(worker.poll(2).unwrap().cover.rgba, vec![200; 4]);
        tx.send((1, Some(prepared(100)))).unwrap();
        assert!(worker.poll(2).is_none());
    }
}
