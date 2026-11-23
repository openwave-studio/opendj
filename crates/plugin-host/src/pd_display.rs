//! A Pd patch's Lua displays (pd-lua objects with a `paint` method), drawn
//! by the helper itself (feature `pd`).
//!
//! pd-lua is built as plugdata builds it: instead of drawing in Pd's Tk
//! editor, each drawing call (`g:fill_rect(...)`, `g:draw_text(...)`) comes
//! here as a [`Cmd`]. A paint's calls are kept until the next paint ends, and
//! [`paint`] draws the latest of each display into the helper's editor
//! window. Mouse events go back to the object's `mouse_down`, `mouse_drag`,
//! `mouse_up` and `mouse_move`.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::{Mutex, MutexGuard, OnceLock};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, Transform,
};

/// One drawing call, in the object's own coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// r, g, b (0-255), alpha (0-1).
    Color(f32, f32, f32, f32),
    /// One of the theme's colours: 0 background, 1 foreground, 2 outline.
    ColorId(u32),
    FillAll,
    FillRect(f32, f32, f32, f32),
    /// x, y, w, h, line width.
    StrokeRect(f32, f32, f32, f32, f32),
    /// x, y, w, h, corner radius.
    FillRounded(f32, f32, f32, f32, f32),
    /// x, y, w, h, corner radius, line width.
    StrokeRounded(f32, f32, f32, f32, f32, f32),
    FillEllipse(f32, f32, f32, f32),
    /// x, y, w, h, line width.
    StrokeEllipse(f32, f32, f32, f32, f32),
    /// x1, y1, x2, y2, line width.
    Line(f32, f32, f32, f32, f32),
    /// text, x, y (top), wrap width, font size.
    Text(String, f32, f32, f32, f32),
    /// Line width, then the points (x, y, x, y...).
    StrokePath(f32, Vec<f32>),
    FillPath(Vec<f32>),
    Translate(f32, f32),
    Scale(f32, f32),
    ResetTransform,
}

/// One Lua object that draws.
#[derive(Clone, Debug, Default)]
pub struct Display {
    /// The object (`t_pdlua *`), as an address.
    pub obj: usize,
    /// Where it sits in the patch.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Each layer's last whole paint.
    pub layers: Vec<Vec<Cmd>>,
    /// A paint in progress: (layer, its calls so far).
    painting: Option<(usize, Vec<Cmd>)>,
}

impl Display {
    pub fn new(obj: usize, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            obj,
            x,
            y,
            width,
            height,
            ..Self::default()
        }
    }
}

#[derive(Default)]
pub struct Displays {
    pub list: Vec<Display>,
    /// Bumped whenever a paint ends or a display is resized.
    pub version: u64,
}

fn store() -> &'static Mutex<Displays> {
    static STORE: OnceLock<Mutex<Displays>> = OnceLock::new();
    STORE.get_or_init(Default::default)
}

/// The displays, to read or to add to.
pub fn displays() -> MutexGuard<'static, Displays> {
    store().lock().unwrap_or_else(|e| e.into_inner())
}

/// Turns pd-lua's call `what` (with its numbers and text) into a [`Cmd`].
fn command(what: &str, n: &[f32], text: Option<&str>) -> Option<Cmd> {
    let at = |i: usize| n.get(i).copied().unwrap_or(0.0);
    Some(match what {
        "lua_set_color" if n.len() == 1 => Cmd::ColorId(at(0) as u32),
        "lua_set_color" => Cmd::Color(at(0), at(1), at(2), n.get(3).copied().unwrap_or(1.0)),
        "lua_fill_all" => Cmd::FillAll,
        "lua_fill_rect" => Cmd::FillRect(at(0), at(1), at(2), at(3)),
        "lua_stroke_rect" => Cmd::StrokeRect(at(0), at(1), at(2), at(3), at(4)),
        "lua_fill_rounded_rect" => Cmd::FillRounded(at(0), at(1), at(2), at(3), at(4)),
        "lua_stroke_rounded_rect" => Cmd::StrokeRounded(at(0), at(1), at(2), at(3), at(4), at(5)),
        "lua_fill_ellipse" => Cmd::FillEllipse(at(0), at(1), at(2), at(3)),
        "lua_stroke_ellipse" => Cmd::StrokeEllipse(at(0), at(1), at(2), at(3), at(4)),
        "lua_draw_line" => Cmd::Line(at(0), at(1), at(2), at(3), at(4)),
        "lua_draw_text" => Cmd::Text(text?.to_owned(), at(0), at(1), at(2), at(3)),
        "lua_stroke_path" => Cmd::StrokePath(at(0), n.get(1..).unwrap_or(&[]).to_vec()),
        "lua_fill_path" => Cmd::FillPath(n.to_vec()),
        "lua_translate" => Cmd::Translate(at(0), at(1)),
        "lua_scale" => Cmd::Scale(at(0), at(1)),
        "lua_reset_transform" => Cmd::ResetTransform,
        _ => return None,
    })
}

/// Takes one call from pd-lua (see `pdlua_glue.c`). Runs on whichever thread
/// is running Pd.
pub fn take(obj: usize, layer: i32, what: &str, n: &[f32], text: Option<&str>) {
    let mut all = displays();
    let Displays { list, version } = &mut *all;
    let Some(d) = list.iter_mut().find(|d| d.obj == obj) else {
        return;
    };
    let layer = layer.max(0) as usize;
    match what {
        "lua_start_paint" => d.painting = Some((layer, Vec::new())),
        "lua_end_paint" => {
            if let Some((layer, cmds)) = d.painting.take() {
                if d.layers.len() <= layer {
                    d.layers.resize_with(layer + 1, Vec::new);
                }
                d.layers[layer] = cmds;
                *version += 1;
            }
        }
        "lua_resized" => {
            d.width = n.first().copied().unwrap_or(d.width);
            d.height = n.get(1).copied().unwrap_or(d.height);
            *version += 1;
        }
        _ => {
            if let (Some((_, cmds)), Some(cmd)) = (d.painting.as_mut(), command(what, n, text)) {
                cmds.push(cmd);
            }
        }
    }
}

/// pd-lua's drawing callback.
///
/// # Safety
/// `what` and `text` are NUL-terminated or null, and `nums` holds `n`
/// numbers, for the call.
pub unsafe extern "C" fn draw_call(
    obj: *mut c_void,
    layer: c_int,
    what: *const c_char,
    nums: *const f64,
    n: c_int,
    text: *const c_char,
) {
    if what.is_null() {
        return;
    }
    // SAFETY: pd-lua hands a NUL-terminated name, `n` numbers and an
    // optional NUL-terminated text, valid for this call.
    let (what, nums, text) = unsafe {
        (
            CStr::from_ptr(what).to_string_lossy(),
            if nums.is_null() || n <= 0 {
                &[][..]
            } else {
                std::slice::from_raw_parts(nums, n as usize)
            },
            (!text.is_null()).then(|| CStr::from_ptr(text).to_string_lossy()),
        )
    };
    let n: Vec<f32> = nums.iter().map(|&x| x as f32).collect();
    take(obj as usize, layer, &what, &n, text.as_deref());
}

/// The area all displays cover: (left, top, width, height), in patch units.
pub fn bounds(list: &[Display]) -> Option<(f32, f32, f32, f32)> {
    let left = list.iter().map(|d| d.x).reduce(f32::min)?;
    let top = list.iter().map(|d| d.y).reduce(f32::min)?;
    let right = list.iter().map(|d| d.x + d.width).reduce(f32::max)?;
    let bottom = list.iter().map(|d| d.y + d.height).reduce(f32::max)?;
    Some((left, top, (right - left).max(1.0), (bottom - top).max(1.0)))
}

/// The display at a point in window units, with the point in its own units.
pub fn hit(list: &[Display], x: f32, y: f32) -> Option<(usize, f32, f32)> {
    let (left, top, _, _) = bounds(list)?;
    let (px, py) = (x + left, y + top);
    list.iter()
        .rev()
        .find(|d| px >= d.x && py >= d.y && px < d.x + d.width && py < d.y + d.height)
        .map(|d| (d.obj, px - d.x, py - d.y))
}

/// Where a display's point is, in its own units, for a window point (to keep
/// sending a drag to the display it started on).
pub fn local(list: &[Display], obj: usize, x: f32, y: f32) -> Option<(f32, f32)> {
    let (left, top, _, _) = bounds(list)?;
    let d = list.iter().find(|d| d.obj == obj)?;
    Some((x + left - d.x, y + top - d.y))
}

fn font() -> &'static FontRef<'static> {
    static FONT: OnceLock<FontRef<'static>> = OnceLock::new();
    FONT.get_or_init(|| {
        FontRef::try_from_slice(include_bytes!(
            "../../app/ui/fonts/InterTabular-Regular.otf"
        ))
        .expect("the bundled font reads")
    })
}

/// The window's background, around and under the displays.
const BACKGROUND: [u8; 3] = [0x10, 0x11, 0x12];

fn theme(id: u32) -> Color {
    match id {
        0 => Color::from_rgba8(0x16, 0x18, 0x19, 255),
        2 => Color::from_rgba8(0x4a, 0x4a, 0x4d, 255),
        _ => Color::from_rgba8(0xe6, 0xe6, 0xe6, 255),
    }
}

/// Draws every display into `pixmap` (the whole window, `scale` pixels per
/// patch unit).
pub fn paint(list: &[Display], pixmap: &mut Pixmap, scale: f32) {
    pixmap.fill(Color::from_rgba8(
        BACKGROUND[0],
        BACKGROUND[1],
        BACKGROUND[2],
        255,
    ));
    let Some((left, top, _, _)) = bounds(list) else {
        return;
    };
    for d in list {
        let w = (d.width * scale).ceil().max(1.0) as u32;
        let h = (d.height * scale).ceil().max(1.0) as u32;
        let Some(mut own) = Pixmap::new(w, h) else {
            continue;
        };
        let mut pen = Pen::new(scale, d.width, d.height);
        for layer in &d.layers {
            pen.reset();
            for cmd in layer {
                pen.run(&mut own, cmd);
            }
        }
        pixmap.draw_pixmap(
            ((d.x - left) * scale).round() as i32,
            ((d.y - top) * scale).round() as i32,
            own.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
}

/// The drawing state of one display while its calls run.
struct Pen {
    scale: f32,
    width: f32,
    height: f32,
    color: Color,
    /// The object's own transform: (translate x, y, scale x, y).
    tx: f32,
    ty: f32,
    sx: f32,
    sy: f32,
}

impl Pen {
    fn new(scale: f32, width: f32, height: f32) -> Self {
        Self {
            scale,
            width,
            height,
            color: theme(1),
            tx: 0.0,
            ty: 0.0,
            sx: 1.0,
            sy: 1.0,
        }
    }

    fn reset(&mut self) {
        (self.tx, self.ty, self.sx, self.sy) = (0.0, 0.0, 1.0, 1.0);
    }

    fn transform(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale)
            .pre_translate(self.tx, self.ty)
            .pre_scale(self.sx, self.sy)
    }

    fn paint(&self) -> Paint<'static> {
        let mut p = Paint::default();
        p.set_color(self.color);
        p.anti_alias = true;
        p
    }

    fn fill(&self, px: &mut Pixmap, path: Option<tiny_skia::Path>) {
        if let Some(path) = path {
            px.fill_path(
                &path,
                &self.paint(),
                FillRule::Winding,
                self.transform(),
                None,
            );
        }
    }

    fn stroke(&self, px: &mut Pixmap, path: Option<tiny_skia::Path>, width: f32) {
        if let Some(path) = path {
            let stroke = Stroke {
                width: width.max(0.1),
                ..Stroke::default()
            };
            px.stroke_path(&path, &self.paint(), &stroke, self.transform(), None);
        }
    }

    fn run(&mut self, px: &mut Pixmap, cmd: &Cmd) {
        match *cmd {
            Cmd::Color(r, g, b, a) => {
                self.color = Color::from_rgba(
                    (r / 255.0).clamp(0.0, 1.0),
                    (g / 255.0).clamp(0.0, 1.0),
                    (b / 255.0).clamp(0.0, 1.0),
                    a.clamp(0.0, 1.0),
                )
                .unwrap_or(Color::WHITE)
            }
            Cmd::ColorId(id) => self.color = theme(id),
            Cmd::FillAll => {
                let (tx, ty, sx, sy) = (self.tx, self.ty, self.sx, self.sy);
                self.reset();
                self.fill(px, rect(0.0, 0.0, self.width, self.height));
                (self.tx, self.ty, self.sx, self.sy) = (tx, ty, sx, sy);
            }
            Cmd::FillRect(x, y, w, h) => self.fill(px, rect(x, y, w, h)),
            Cmd::StrokeRect(x, y, w, h, lw) => self.stroke(px, rect(x, y, w, h), lw),
            Cmd::FillRounded(x, y, w, h, r) => self.fill(px, rounded(x, y, w, h, r)),
            Cmd::StrokeRounded(x, y, w, h, r, lw) => self.stroke(px, rounded(x, y, w, h, r), lw),
            Cmd::FillEllipse(x, y, w, h) => self.fill(px, ellipse(x, y, w, h)),
            Cmd::StrokeEllipse(x, y, w, h, lw) => self.stroke(px, ellipse(x, y, w, h), lw),
            Cmd::Line(x1, y1, x2, y2, lw) => {
                let mut pb = PathBuilder::new();
                pb.move_to(x1, y1);
                pb.line_to(x2, y2);
                self.stroke(px, pb.finish(), lw);
            }
            Cmd::StrokePath(lw, ref pts) => self.stroke(px, polyline(pts, false), lw),
            Cmd::FillPath(ref pts) => self.fill(px, polyline(pts, true)),
            Cmd::Text(ref text, x, y, w, size) => self.text(px, text, x, y, w, size),
            Cmd::Translate(x, y) => {
                self.tx += x * self.sx;
                self.ty += y * self.sy;
            }
            Cmd::Scale(x, y) => {
                self.sx *= x;
                self.sy *= y;
            }
            Cmd::ResetTransform => self.reset(),
        }
    }

    /// Text from its top left, wrapped at `wrap` (as plugdata does).
    fn text(&self, px: &mut Pixmap, text: &str, x: f32, y: f32, wrap: f32, size: f32) {
        let font = font();
        let units = (size.max(1.0) * self.sy.abs()).max(1.0);
        let pixels = units * self.scale;
        let scaled = font.as_scaled(PxScale::from(pixels));
        let line_height = scaled.height() + scaled.line_gap();
        // Lay out in pixels, from the text's top left.
        let origin_x = (x * self.sx + self.tx) * self.scale;
        let origin_y = (y * self.sy + self.ty) * self.scale;
        let wrap = if wrap > 0.0 {
            wrap * self.sx.abs() * self.scale
        } else {
            f32::INFINITY
        };
        let advance = |s: &str| -> f32 {
            let mut w = 0.0;
            let mut last = None;
            for c in s.chars() {
                let id = font.glyph_id(c);
                if let Some(prev) = last {
                    w += scaled.kern(prev, id);
                }
                w += scaled.h_advance(id);
                last = Some(id);
            }
            w
        };
        let mut lines: Vec<String> = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let tried = if line.is_empty() {
                    word.to_owned()
                } else {
                    format!("{line} {word}")
                };
                if !line.is_empty() && advance(&tried) > wrap {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_owned();
                } else {
                    line = tried;
                }
            }
            lines.push(line);
        }
        let c = self.color;
        let (pw, ph) = (px.width() as i32, px.height() as i32);
        let data = px.data_mut();
        for (i, line) in lines.iter().enumerate() {
            let baseline = origin_y + scaled.ascent() + i as f32 * line_height;
            let mut pen_x = origin_x;
            let mut last = None;
            for ch in line.chars() {
                let id = font.glyph_id(ch);
                if let Some(prev) = last {
                    pen_x += scaled.kern(prev, id);
                }
                let glyph = id.with_scale_and_position(pixels, ab_glyph::point(pen_x, baseline));
                pen_x += scaled.h_advance(id);
                last = Some(id);
                let Some(outline) = font.outline_glyph(glyph) else {
                    continue;
                };
                let b = outline.px_bounds();
                outline.draw(|gx, gy, cov| {
                    let (dx, dy) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if dx < 0 || dy < 0 || dx >= pw || dy >= ph {
                        return;
                    }
                    let a = (cov.clamp(0.0, 1.0) * c.alpha()).clamp(0.0, 1.0);
                    let i = (dy as usize * pw as usize + dx as usize) * 4;
                    let src = [c.red() * a, c.green() * a, c.blue() * a, a];
                    for k in 0..4 {
                        let dst = data[i + k] as f32 / 255.0;
                        data[i + k] = ((src[k] + dst * (1.0 - a)) * 255.0).round() as u8;
                    }
                });
            }
        }
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<tiny_skia::Path> {
    Some(PathBuilder::from_rect(Rect::from_xywh(
        x,
        y,
        w.max(0.01),
        h.max(0.01),
    )?))
}

fn ellipse(x: f32, y: f32, w: f32, h: f32) -> Option<tiny_skia::Path> {
    PathBuilder::from_oval(Rect::from_xywh(x, y, w.max(0.01), h.max(0.01))?)
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.clamp(0.0, w.min(h) / 2.0);
    if r <= 0.0 {
        return rect(x, y, w, h);
    }
    // A quarter circle as a cubic: control points at k * r.
    let k = 0.552_284_8 * r;
    let (r2, b) = (x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(r2 - r, y);
    pb.cubic_to(r2 - r + k, y, r2, y + r - k, r2, y + r);
    pb.line_to(r2, b - r);
    pb.cubic_to(r2, b - r + k, r2 - r + k, b, r2 - r, b);
    pb.line_to(x + r, b);
    pb.cubic_to(x + r - k, b, x, b - r + k, x, b - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

fn polyline(pts: &[f32], close: bool) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let (points, _) = pts.as_chunks::<2>();
    let (first, rest) = points.split_first()?;
    pb.move_to(first[0], first[1]);
    for p in rest {
        pb.line_to(p[0], p[1]);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

/// Copies `pixmap` (opaque) into a window buffer of 0RGB pixels.
pub fn to_window(pixmap: &Pixmap, out: &mut [u32]) {
    for (o, p) in out.iter_mut().zip(pixmap.data().as_chunks::<4>().0) {
        *o = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paint_shows_once_it_ends() {
        let obj = 0xdead_beef;
        displays().list.push(Display {
            obj,
            width: 40.0,
            height: 20.0,
            ..Display::default()
        });
        take(obj, 0, "lua_start_paint", &[], None);
        take(obj, 0, "lua_set_color", &[255.0, 0.0, 0.0], None);
        take(obj, 0, "lua_fill_all", &[], None);
        take(obj, 0, "lua_draw_text", &[2.0, 2.0, 30.0, 12.0], Some("hi"));
        {
            let all = displays();
            let d = all.list.iter().find(|d| d.obj == obj).unwrap();
            assert!(d.layers.is_empty(), "nothing shows mid-paint");
        }
        take(obj, 0, "lua_end_paint", &[], None);
        let all = displays();
        let d = all.list.iter().find(|d| d.obj == obj).unwrap();
        assert_eq!(
            d.layers[0],
            [
                Cmd::Color(255.0, 0.0, 0.0, 1.0),
                Cmd::FillAll,
                Cmd::Text("hi".into(), 2.0, 2.0, 30.0, 12.0),
            ]
        );
        let mut px = Pixmap::new(80, 40).unwrap();
        paint(std::slice::from_ref(d), &mut px, 2.0);
        // Red fill, white text somewhere in it.
        let red = px.pixel(70, 30).unwrap();
        assert_eq!((red.red(), red.green(), red.blue()), (255, 0, 0));
    }

    #[test]
    fn clicks_land_in_the_display_under_them() {
        let list = [
            Display {
                obj: 1,
                x: 20.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
                ..Display::default()
            },
            Display {
                obj: 2,
                x: 20.0,
                y: 100.0,
                width: 100.0,
                height: 50.0,
                ..Display::default()
            },
        ];
        assert_eq!(bounds(&list), Some((20.0, 20.0, 100.0, 130.0)));
        assert_eq!(hit(&list, 10.0, 5.0), Some((1, 10.0, 5.0)));
        assert_eq!(hit(&list, 10.0, 90.0), Some((2, 10.0, 10.0)));
        assert_eq!(hit(&list, 10.0, 60.0), None);
        assert_eq!(local(&list, 1, 10.0, 90.0), Some((10.0, 90.0)));
    }
}
