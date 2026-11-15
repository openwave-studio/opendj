//! Decode, resize, blur and encode off the UI thread. Only current results win.
use opendj_engine::{
    cover::{self, Cover},
    Source, Track,
};
use std::sync::{mpsc, Arc};

const COVER_SIDE: u32 = 1600;
const BACKDROP_SIDE: u32 = 64;
const BACKDROP_BLUR: f32 = 12.0;

struct Request {
    id: u64,
    source: Source,
    album: String,
    fallback: Option<Arc<Cover>>,
}

pub(super) struct Prepared {
    pub cover: Cover,
    pub backdrop: Cover,
    pub png: Vec<u8>,
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
    let soft = image::imageops::blur(
        &image::imageops::thumbnail(&rgba, BACKDROP_SIDE, BACKDROP_SIDE),
        BACKDROP_BLUR,
    );
    let backdrop = Cover {
        width: soft.width(),
        height: soft.height(),
        rgba: soft.into_raw(),
    };
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    let cover = Cover {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    };
    Some(Prepared {
        cover,
        backdrop,
        png,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_large_artwork_and_prepares_a_small_blurred_backdrop() {
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
        assert!(result.backdrop.width <= 64 && result.backdrop.height <= 64);
        assert_eq!(image::load_from_memory(&result.png).unwrap().width(), 1200);
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
        let decoded = image::load_from_memory(&result.png).unwrap().to_rgba8();
        assert_eq!(
            decoded.dimensions(),
            (result.cover.width, result.cover.height)
        );
        assert_eq!(decoded.get_pixel(0, 0).0, [30, 90, 180, 255]);
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
            backdrop: Cover {
                width: 1,
                height: 1,
                rgba: vec![pixel; 4],
            },
            png: vec![],
        };
        tx.send((2, Some(prepared(200)))).unwrap();
        tx.send((1, Some(prepared(100)))).unwrap();
        assert_eq!(worker.poll(2).unwrap().cover.rgba, vec![200; 4]);
        tx.send((1, Some(prepared(100)))).unwrap();
        assert!(worker.poll(2).is_none());
    }
}
