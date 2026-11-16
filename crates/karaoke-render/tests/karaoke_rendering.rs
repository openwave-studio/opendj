//! Render the actual karaoke UI: timing-only tests cannot catch displaced text.
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, SharedString, VecModel};

slint::slint! {
    export { KaraokeWindow } from "../../app/ui/karaoke.slint";
}

struct RenderPlatform {
    window: Rc<MinimalSoftwareWindow>,
    now: Rc<Cell<Duration>>,
}

impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> Duration {
        self.now.get()
    }
}

struct Scene {
    window: Rc<MinimalSoftwareWindow>,
    now: Rc<Cell<Duration>>,
    ui: KaraokeWindow,
}

impl Scene {
    fn new() -> Self {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        let now = Rc::new(Cell::new(Duration::ZERO));
        slint::platform::set_platform(Box::new(RenderPlatform {
            window: window.clone(),
            now: now.clone(),
        }))
        .unwrap();
        let ui = KaraokeWindow::new().unwrap();
        window.set_size(PhysicalSize::new(1280, 720));
        ui.set_state(3);
        ui.set_current_line("Nadeszły wakacje ożyły osiedla".into());
        ui.set_letters(28);
        ui.set_sung("Nadeszły ".into());
        ui.set_word("wakacje".into());
        ui.set_rest(" ożyły osiedla".into());
        ui.set_highlight(slint::Color::from_rgb_u8(255, 70, 120));
        ui.show().unwrap();
        Self { window, now, ui }
    }

    fn render(&self, millis: u64) -> Vec<Rgb8Pixel> {
        self.now.set(self.now.get() + Duration::from_millis(millis));
        slint::platform::update_timers_and_animations();
        let size = self.window.size();
        let mut pixels = vec![Rgb8Pixel::default(); (size.width * size.height) as usize];
        // Render the entire scene for comparisons, even when the normal backend
        // would reuse the previous buffer's undamaged pixels.
        self.window.request_redraw();
        self.window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, size.width as usize);
        });
        pixels
    }
}

#[test]
fn karaoke_text_stays_aligned_while_filling_and_restoring() {
    let scene = Scene::new();
    scene.ui.set_sung("".into());
    scene.ui.set_word("Nadeszły".into());
    scene.ui.set_part(0.0);
    let unfilled = scene.render(1000);
    assert!(
        !unfilled.iter().any(|p| p.r > 160 && p.g < 100),
        "a zero-progress word must not show a highlight"
    );
    for (sung, word, part) in [
        ("", "Nadeszły", 0.2),
        ("", "Nadeszły", 0.6),
        ("", "Nadeszły", 1.0),
        ("Nadeszły ", "wakacje", 0.4),
        ("Nadeszły wakacje ", "ożyły", 0.7),
    ] {
        scene.ui.set_sung(sung.into());
        scene.ui.set_word(word.into());
        scene.ui.set_part(part);
        let filled = scene.render(20);
        let colored: Vec<_> = filled
            .iter()
            .enumerate()
            .filter(|(_, p)| p.r > 160 && p.g < 100)
            .collect();
        assert!(colored.len() > 100, "the highlight must be visible");
        let displaced = colored.iter().filter(|(i, _)| unfilled[*i].g < 60).count();
        assert!(
            displaced * 100 < colored.len(),
            "highlight drifted away from its glyphs: {displaced}/{}",
            colored.len()
        );
    }

    scene.ui.set_cover_on(true);
    scene.ui.set_song("4 Pory Roku".into());
    scene.ui.set_artist("Qbik".into());
    scene.ui.set_has_cover(true);
    let art = [30u8, 90, 180, 255].repeat(1200 * 1200);
    scene
        .ui
        .set_cover(slint::Image::from_rgba8(slint::SharedPixelBuffer::<
            slint::Rgba8Pixel,
        >::clone_from_slice(
            &art, 1200, 1200
        )));

    let lines: Vec<SharedString> = vec![
        "First short line".into(),
        "A longer lyric that wraps across several rows in the artwork column".into(),
        "The active words keep their position".into(),
        "The following line".into(),
    ];
    scene
        .ui
        .set_artwork_lines(ModelRc::from(Rc::new(VecModel::from(lines))));
    scene.ui.set_active_line(1);
    scene.ui.set_sung("A longer lyric ".into());
    scene.ui.set_word("that".into());
    scene
        .ui
        .set_rest(" wraps across several rows in the artwork column".into());
    scene.ui.set_part(0.1);
    scene.render(1000);
    let dim = scene.render(1000);
    scene.ui.set_part(0.9);
    let bright = scene.render(100);
    // The cover's upper-left corner is explicitly anchored. Metadata must
    // occupy the space below it, never the image itself.
    let inside_cover = |x: usize, y: usize| {
        let p = bright[y * 1280 + x];
        p.r > 180 && p.g > 180 && p.b > 180
    };
    assert!(
        !(145..435).any(|y| (180..460).any(|x| inside_cover(x, y))),
        "metadata must not overlap the cover"
    );
    assert!(
        (465..545).any(|y| (162..478).any(|x| inside_cover(x, y))),
        "the title and artist should be visible below the cover"
    );

    assert!(
        dim.iter()
            .zip(&bright)
            .filter(|(a, b)| b.r > a.r.saturating_add(30))
            .count()
            > 20,
        "the current word must animate gradually, not turn white at its start"
    );

    // Focus loss and a large clock jump must not leave duplicate or displaced
    // rows after returning to the karaoke window.
    scene
        .window
        .dispatch_event(WindowEvent::WindowActiveChanged(false));
    scene.ui.set_active_line(2);
    scene.ui.set_sung("The active ".into());
    scene.ui.set_word("words".into());
    scene.ui.set_rest(" keep their position".into());
    let start_scroll = scene.render(1);
    let mid_scroll = scene.render(240);
    let end_scroll = scene.render(1000);
    assert_ne!(
        start_scroll, mid_scroll,
        "the lyric column should scroll between rows"
    );
    assert_ne!(
        mid_scroll, end_scroll,
        "scrolling should have intermediate frames"
    );
    scene.render(10_000);
    scene
        .window
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    let restored = scene.render(1);
    let settled = scene.render(1000);
    assert_eq!(
        restored, settled,
        "the restored scene must not keep drifting"
    );

    scene.ui.set_state(1);
    let no_lyrics = scene.render(1000);
    for y in 130..550 {
        for x in 160..510 {
            assert_eq!(
                restored[y * 1280 + x],
                no_lyrics[y * 1280 + x],
                "missing-lyrics messages must stay out of the artwork pane"
            );
        }
    }
    scene.ui.set_state(3);
    scene.window.set_size(PhysicalSize::new(960, 720));
    scene.render(1);
    scene.render(1000);
    scene.window.set_size(PhysicalSize::new(1280, 720));
    scene.render(1);
    scene.render(1000);
    assert_eq!(
        restored,
        scene.render(1000),
        "resizing must restore stable lyric wrapping and scroll position"
    );

    scene
        .ui
        .set_artwork_lines(ModelRc::from(Rc::new(VecModel::from(
            vec![SharedString::from("Words soften before leaving the screen"); 10],
        ))));
    scene.ui.set_active_line(0);
    scene.ui.set_sung("".into());
    scene.ui.set_word("Words".into());
    scene
        .ui
        .set_rest(" soften before leaving the screen".into());
    scene.render(1000);
    let edges = scene.render(1000);
    for y in 635..648 {
        let background = edges[y * 1280 + 1250];
        for x in 660..1170 {
            let pixel = edges[y * 1280 + x];
            assert!(
                pixel.r.abs_diff(background.r) < 8 && pixel.g.abs_diff(background.g) < 8,
                "lyrics must fade out before the safety clip, not stop at a hard edge"
            );
        }
    }

    // The buttons stay hidden until the mouse moves, then fade in.
    let top_right = |pixels: &[Rgb8Pixel]| {
        (20..64)
            .flat_map(|y| (1100..1256).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let p = pixels[y * 1280 + x];
                p.r > 200 && p.g > 200 && p.b > 200
            })
            .count()
    };
    assert_eq!(
        top_right(&edges),
        0,
        "no buttons on the audience screen at rest"
    );
    scene.ui.set_show_controls(true);
    scene.render(1);
    let poked = scene.render(1000);
    assert!(
        top_right(&poked) > 40,
        "the buttons show while the mouse moves"
    );
    scene.ui.set_show_controls(false);
    scene.render(1);
    scene.render(1000);

    if let Ok(dir) = std::env::var("OPENDJ_KARAOKE_RENDER_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, pixels) in [
            ("original", unfilled),
            ("artwork", bright),
            ("edges", edges),
            ("buttons", poked),
        ] {
            let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
            image::save_buffer(
                format!("{dir}/{name}.png"),
                &bytes,
                1280,
                720,
                image::ColorType::Rgb8,
            )
            .unwrap();
        }
    }
}
