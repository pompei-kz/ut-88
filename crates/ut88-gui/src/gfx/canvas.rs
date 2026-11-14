//! Холст: кадр в памяти поверх `tiny_skia::Pixmap`.

use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Shader, Stroke, Transform,
};

use super::color::Color;
use super::geom::{Align2, Pos2, Rect, Vec2, pos2, vec2};
use super::text::{self, FontKind, IRect, div255};

/// Как рисовать фигуру с учётом отсечения.
enum ClipMode {
    /// Фигура целиком вне области отсечения — не рисовать.
    Skip,
    /// Фигура целиком внутри — рисовать без маски.
    Free,
    /// Фигура пересекает границу — рисовать через маску отсечения.
    Masked,
}

/// Холст для рисования кадра.
///
/// Пиксели хранятся в `tiny_skia::Pixmap` (RGBA с предварительно умноженной альфой).
/// Размер холста — в **физических** пикселях, а все операции рисования принимают
/// **логические** точки и умножаются на масштаб [`Canvas::set_scale`] (HiDPI). Фигуры
/// рисуются со сглаживанием.
///
/// Отсечение — стек прямоугольников ([`Canvas::push_clip`] / [`Canvas::pop_clip`]):
/// рисуется только пересечение всех прямоугольников стека. Границы отсечения
/// округляются до целых физических пикселей.
pub struct Canvas {
    pixmap: Pixmap,
    scale: f32,
    /// Стек отсечения в физических пикселях; каждый элемент — уже пересечение с предыдущим.
    clips: Vec<IRect>,
    /// Маска для текущего отсечения (строится лениво, только когда фигура пересекает
    /// границу отсечения).
    mask: Option<Mask>,
    /// Для какого отсечения построена `mask`.
    mask_for: Option<IRect>,
}

impl std::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Canvas")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("scale", &self.scale)
            .field("clips", &self.clips.len())
            .finish()
    }
}

fn new_pixmap(w: u32, h: u32) -> Pixmap {
    Pixmap::new(w.max(1), h.max(1)).expect("размер холста слишком велик")
}

/// Сплошная заливка цветом со сглаживанием.
fn solid(color: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(color.to_skia()),
        anti_alias: true,
        ..Paint::default()
    }
}

/// Контур прямоугольника со скруглёнными углами (в физических пикселях).
fn rect_path(x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) -> Option<Path> {
    let r = radius.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5);
    if r <= 0.0 {
        let rect = tiny_skia::Rect::from_ltrb(x0, y0, x1, y1)?;
        return Some(PathBuilder::from_rect(rect));
    }
    // Четверть окружности кубической кривой Безье.
    let k = r * (1.0 - 0.552_284_8);
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - k, x1 - k, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + k, x0 + k, y0, x0 + r, y0);
    pb.close();
    pb.finish()
}

impl Canvas {
    /// Холст `w × h` физических пикселей, прозрачный, масштаб 1. Нулевой размер
    /// заменяется на 1.
    pub fn new(w: u32, h: u32) -> Self {
        Canvas {
            pixmap: new_pixmap(w, h),
            scale: 1.0,
            clips: Vec::new(),
            mask: None,
            mask_for: None,
        }
    }

    /// Изменить размер (в физических пикселях). Если размер изменился, содержимое
    /// становится прозрачным, а стек отсечения очищается.
    pub fn resize(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) != (self.width(), self.height()) {
            self.pixmap = new_pixmap(w, h);
            self.clips.clear();
            self.mask = None;
            self.mask_for = None;
        }
    }

    /// Ширина в физических пикселях.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// Высота в физических пикселях.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Масштаб: физических пикселей в логической точке (`pixels_per_point`).
    pub fn set_scale(&mut self, ppp: f32) {
        if ppp.is_finite() && ppp > 0.0 {
            self.scale = ppp;
        }
    }

    /// Текущий масштаб.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Размер холста в логических точках.
    pub fn size_points(&self) -> Vec2 {
        vec2(self.width() as f32, self.height() as f32) / self.scale
    }

    /// Весь холст в логических точках.
    pub fn rect_points(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, self.size_points())
    }

    fn full(&self) -> IRect {
        IRect {
            x0: 0,
            y0: 0,
            x1: self.width() as i32,
            y1: self.height() as i32,
        }
    }

    fn clip(&self) -> IRect {
        self.clips.last().copied().unwrap_or_else(|| self.full())
    }

    /// Текущая область отсечения в логических точках.
    pub fn clip_rect(&self) -> Rect {
        let c = self.clip();
        let s = self.scale;
        Rect::from_min_max(
            pos2(c.x0 as f32 / s, c.y0 as f32 / s),
            pos2(c.x1 as f32 / s, c.y1 as f32 / s),
        )
    }

    /// Сузить отсечение до `rect` (логические точки); отменяется [`Canvas::pop_clip`].
    pub fn push_clip(&mut self, rect: Rect) {
        let s = self.scale;
        let r = IRect {
            x0: (rect.min.x * s).round() as i32,
            y0: (rect.min.y * s).round() as i32,
            x1: (rect.max.x * s).round() as i32,
            y1: (rect.max.y * s).round() as i32,
        };
        let c = self.clip().intersect(r);
        self.clips.push(c);
    }

    /// Вернуть отсечение, бывшее до последнего [`Canvas::push_clip`].
    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }

    /// Глубина стека отсечения (для проверки парности push/pop).
    pub fn clip_depth(&self) -> usize {
        self.clips.len()
    }

    /// Залить **весь** холст цветом (отсечение не учитывается).
    pub fn clear(&mut self, color: Color) {
        self.pixmap.fill(color.to_skia());
    }

    /// Куда попадает фигура с границами `(x0, y0, x1, y1)` (физические пиксели).
    fn classify(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> ClipMode {
        let c = self.clip();
        if c.is_empty() {
            return ClipMode::Skip;
        }
        // Запас на сглаживание.
        let (x0, y0, x1, y1) = (x0 - 1.0, y0 - 1.0, x1 + 1.0, y1 + 1.0);
        let (cx0, cy0, cx1, cy1) = (c.x0 as f32, c.y0 as f32, c.x1 as f32, c.y1 as f32);
        if x1 <= cx0 || y1 <= cy0 || x0 >= cx1 || y0 >= cy1 {
            ClipMode::Skip
        } else if c == self.full() || (x0 >= cx0 && y0 >= cy0 && x1 <= cx1 && y1 <= cy1) {
            // tiny-skia сам отсекает по краям холста.
            ClipMode::Free
        } else {
            ClipMode::Masked
        }
    }

    /// Построить маску текущего отсечения, если она устарела.
    fn ensure_mask(&mut self) {
        let c = self.clip();
        if self.mask_for == Some(c) && self.mask.is_some() {
            return;
        }
        let (w, h) = (self.width(), self.height());
        let mask = match &mut self.mask {
            Some(m) if m.width() == w && m.height() == h => m,
            slot => slot.insert(Mask::new(w, h).expect("размер маски")),
        };
        let data = mask.data_mut();
        data.fill(0);
        let c = c.intersect(IRect {
            x0: 0,
            y0: 0,
            x1: w as i32,
            y1: h as i32,
        });
        if !c.is_empty() {
            for y in c.y0..c.y1 {
                let row = y as usize * w as usize;
                data[row + c.x0 as usize..row + c.x1 as usize].fill(255);
            }
        }
        self.mask_for = Some(c);
    }

    /// Маска для режима `mode` (строится при необходимости).
    fn mask_for_mode(&mut self, mode: &ClipMode) -> bool {
        if matches!(mode, ClipMode::Masked) {
            self.ensure_mask();
            true
        } else {
            false
        }
    }

    /// Залить контур (физические пиксели) с учётом отсечения.
    fn fill_path_px(&mut self, path: &Path, color: Color) {
        if color.is_transparent() {
            return;
        }
        let b = path.bounds();
        let mode = self.classify(b.left(), b.top(), b.right(), b.bottom());
        if matches!(mode, ClipMode::Skip) {
            return;
        }
        let masked = self.mask_for_mode(&mode);
        let mask = if masked { self.mask.as_ref() } else { None };
        self.pixmap.fill_path(
            path,
            &solid(color),
            FillRule::Winding,
            Transform::identity(),
            mask,
        );
    }

    /// Обвести контур (физические пиксели) с учётом отсечения.
    fn stroke_path_px(&mut self, path: &Path, width: f32, cap: LineCap, color: Color) {
        if color.is_transparent() || width <= 0.0 || !width.is_finite() {
            return;
        }
        let b = path.bounds();
        // Запас на толщину, острые углы (предел среза 4) и концы линий.
        let m = width * 2.0;
        let mode = self.classify(b.left() - m, b.top() - m, b.right() + m, b.bottom() + m);
        if matches!(mode, ClipMode::Skip) {
            return;
        }
        let masked = self.mask_for_mode(&mode);
        let mask = if masked { self.mask.as_ref() } else { None };
        let stroke = Stroke {
            width,
            line_cap: cap,
            line_join: LineJoin::Miter,
            ..Stroke::default()
        };
        self.pixmap
            .stroke_path(path, &solid(color), &stroke, Transform::identity(), mask);
    }

    /// Прямоугольник в физических пикселях, обрезанный отсечением; `None` — пусто.
    fn clip_rect_px(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<tiny_skia::Rect> {
        let c = self.clip();
        let x0 = x0.max(c.x0 as f32);
        let y0 = y0.max(c.y0 as f32);
        let x1 = x1.min(c.x1 as f32);
        let y1 = y1.min(c.y1 as f32);
        if x0 < x1 && y0 < y1 {
            tiny_skia::Rect::from_ltrb(x0, y0, x1, y1)
        } else {
            None
        }
    }

    /// Залить прямоугольник; `radius` — радиус скругления углов (0 — острые углы).
    pub fn fill_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        if color.is_transparent() {
            return;
        }
        let s = self.scale;
        let (x0, y0, x1, y1) = (
            rect.min.x * s,
            rect.min.y * s,
            rect.max.x * s,
            rect.max.y * s,
        );
        if radius <= 0.0 {
            // Прямоугольник отсекается геометрически, маска не нужна.
            if let Some(r) = self.clip_rect_px(x0, y0, x1, y1) {
                self.pixmap
                    .fill_rect(r, &solid(color), Transform::identity(), None);
            }
        } else if let Some(path) = rect_path(x0, y0, x1, y1, radius * s) {
            self.fill_path_px(&path, color);
        }
    }

    /// Рамка прямоугольника толщиной `width`; линия идёт **по центру** границы `rect`
    /// (половина толщины снаружи). Для рамки внутри (`StrokeKind::Inside` в egui)
    /// передайте `rect.shrink(width / 2.0)` и `radius - width / 2.0`.
    pub fn stroke_rect(&mut self, rect: Rect, radius: f32, width: f32, color: Color) {
        let s = self.scale;
        if let Some(path) = rect_path(
            rect.min.x * s,
            rect.min.y * s,
            rect.max.x * s,
            rect.max.y * s,
            radius.max(0.0) * s,
        ) {
            self.stroke_path_px(&path, width * s, LineCap::Butt, color);
        }
    }

    /// Залить круг.
    pub fn fill_circle(&mut self, center: Pos2, radius: f32, color: Color) {
        let s = self.scale;
        if let Some(path) = PathBuilder::from_circle(center.x * s, center.y * s, radius * s) {
            self.fill_path_px(&path, color);
        }
    }

    /// Окружность толщиной `width` (линия по центру радиуса).
    pub fn stroke_circle(&mut self, center: Pos2, radius: f32, width: f32, color: Color) {
        let s = self.scale;
        if let Some(path) = PathBuilder::from_circle(center.x * s, center.y * s, radius * s) {
            self.stroke_path_px(&path, width * s, LineCap::Butt, color);
        }
    }

    /// Отрезок толщиной `width` с плоскими концами (как `line_segment` в egui).
    pub fn line(&mut self, a: Pos2, b: Pos2, width: f32, color: Color) {
        let s = self.scale;
        let mut pb = PathBuilder::new();
        pb.move_to(a.x * s, a.y * s);
        pb.line_to(b.x * s, b.y * s);
        if let Some(path) = pb.finish() {
            self.stroke_path_px(&path, width * s, LineCap::Butt, color);
        }
    }

    fn poly_path(&self, points: &[Pos2], close: bool) -> Option<Path> {
        let s = self.scale;
        let (first, rest) = points.split_first()?;
        let mut pb = PathBuilder::with_capacity(points.len() + 1, points.len() + 1);
        pb.move_to(first.x * s, first.y * s);
        for p in rest {
            pb.line_to(p.x * s, p.y * s);
        }
        if close {
            pb.close();
        }
        pb.finish()
    }

    /// Залить многоугольник (выпуклый или нет; правило заполнения — ненулевое число
    /// оборотов).
    pub fn fill_polygon(&mut self, points: &[Pos2], color: Color) {
        if points.len() < 3 {
            return;
        }
        if let Some(path) = self.poly_path(points, true) {
            self.fill_path_px(&path, color);
        }
    }

    /// Контур замкнутого многоугольника толщиной `width` (линия по центру сторон).
    pub fn stroke_polygon(&mut self, points: &[Pos2], width: f32, color: Color) {
        if points.len() < 2 {
            return;
        }
        if let Some(path) = self.poly_path(points, true) {
            self.stroke_path_px(&path, width * self.scale, LineCap::Butt, color);
        }
    }

    /// Ломаная толщиной `width` (не замкнутая, концы плоские).
    pub fn polyline(&mut self, points: &[Pos2], width: f32, color: Color) {
        if points.len() < 2 {
            return;
        }
        if let Some(path) = self.poly_path(points, false) {
            self.stroke_path_px(&path, width * self.scale, LineCap::Butt, color);
        }
    }

    /// Размер строки в логических точках: ширина и высота строки (`ascent − descent`).
    /// Согласован с [`Canvas::text`]: прямоугольник, который вернёт `text`, имеет ровно
    /// этот размер. Зависит от масштаба (шрифт растеризуется в физических пикселях).
    pub fn measure_text(&self, text: &str, size: f32, font: FontKind) -> Vec2 {
        measure_text_scaled(text, size, font, self.scale)
    }

    /// Написать строку (одну, без переноса; `'\n'` и другие управляющие символы
    /// пропускаются). Строка размером [`Canvas::measure_text`] выравнивается точкой
    /// `align` к `pos` (как `Painter::text` в egui). Возвращает занятый прямоугольник
    /// (логические точки). `size` — кегль (em) в логических точках.
    pub fn text(
        &mut self,
        pos: Pos2,
        align: Align2,
        text: &str,
        size: f32,
        font: FontKind,
        color: Color,
    ) -> Rect {
        let s = self.scale;
        let rect = align.anchor_size(pos, self.measure_text(text, size, font));
        let clip = self.clip();
        let stride = self.width() as usize;
        text::draw_text(
            self.pixmap.data_mut(),
            stride,
            clip,
            rect.min.x * s,
            rect.min.y * s,
            text,
            size * s,
            font,
            [color.r, color.g, color.b, color.a],
        );
        rect
    }

    /// Вывести картинку `src_w × src_h` (байты RGBA без умножения на альфу, построчно)
    /// в прямоугольник `rect` с масштабированием: `linear` — билинейная фильтрация
    /// (как `TextureOptions::LINEAR`), иначе — ближайший пиксель.
    ///
    /// Закрашиваются физические пиксели, центры которых лежат внутри `rect` (края не
    /// сглаживаются); полупрозрачные пиксели картинки накладываются на фон.
    pub fn image(&mut self, rect: Rect, src_w: u32, src_h: u32, rgba: &[u8], linear: bool) {
        let (sw, sh) = (src_w as usize, src_h as usize);
        if sw == 0 || sh == 0 || rgba.len() < sw * sh * 4 {
            return;
        }
        let s = self.scale;
        let (x0, y0, x1, y1) = (
            rect.min.x * s,
            rect.min.y * s,
            rect.max.x * s,
            rect.max.y * s,
        );
        if !(x1 > x0 && y1 > y0) {
            return;
        }
        let area = self.clip().intersect(IRect {
            x0: x0.round() as i32,
            y0: y0.round() as i32,
            x1: x1.round() as i32,
            y1: y1.round() as i32,
        });
        if area.is_empty() {
            return;
        }
        let cols = sample_axis(area.x0, area.x1, x0, (x1 - x0) / sw as f32, sw, linear);
        let rows = sample_axis(area.y0, area.y1, y0, (y1 - y0) / sh as f32, sh, linear);
        let dw = self.width() as usize;
        let data = self.pixmap.data_mut();
        // Пиксель картинки с умножением на альфу.
        let src = |i: usize| -> [u32; 4] {
            let p = &rgba[i * 4..i * 4 + 4];
            let a = p[3] as u32;
            if a == 255 {
                [p[0] as u32, p[1] as u32, p[2] as u32, 255]
            } else {
                [
                    div255(p[0] as u32 * a),
                    div255(p[1] as u32 * a),
                    div255(p[2] as u32 * a),
                    a,
                ]
            }
        };
        // Строка картинки, уже смешанная по вертикали (значения × 256), — пересчитывается,
        // только когда меняется строка-источник или вес.
        let mut line = vec![[0u32; 4]; sw];
        let mut line_for = None;
        for (dy, &(sy, wy)) in (area.y0..area.y1).zip(&rows) {
            if line_for != Some((sy, wy)) {
                line_for = Some((sy, wy));
                let row0 = sy * sw;
                let row1 = (sy + 1).min(sh - 1) * sw;
                let wy = wy as u32;
                for (x, l) in line.iter_mut().enumerate() {
                    let (p0, p1) = (src(row0 + x), src(row1 + x));
                    for i in 0..4 {
                        l[i] = p0[i] * (256 - wy) + p1[i] * wy;
                    }
                }
            }
            let drow = &mut data[(dy as usize * dw + area.x0 as usize) * 4..][..cols.len() * 4];
            for (d, &(sx, wx)) in drow.as_chunks_mut::<4>().0.iter_mut().zip(&cols) {
                // Смесь по горизонтали, веса — в 1/256.
                let (p0, p1) = (line[sx], line[(sx + 1).min(sw - 1)]);
                let (w0, w1) = (256 - wx as u32, wx as u32);
                let c = [
                    (p0[0] * w0 + p1[0] * w1 + (1 << 15)) >> 16,
                    (p0[1] * w0 + p1[1] * w1 + (1 << 15)) >> 16,
                    (p0[2] * w0 + p1[2] * w1 + (1 << 15)) >> 16,
                    (p0[3] * w0 + p1[3] * w1 + (1 << 15)) >> 16,
                ];
                if c[3] >= 255 {
                    d[0] = c[0] as u8;
                    d[1] = c[1] as u8;
                    d[2] = c[2] as u8;
                    d[3] = 255;
                } else if c[3] != 0 {
                    let inv = 255 - c[3];
                    for i in 0..4 {
                        d[i] = (c[i] + div255(d[i] as u32 * inv)) as u8;
                    }
                }
            }
        }
    }

    /// Наложить другой холст (например, кэш-слой рисунка) левым верхним углом в точку
    /// `at` (логические точки, округляется до целого физического пикселя) — пиксель в
    /// пиксель, без масштабирования, с учётом прозрачности и отсечения.
    pub fn draw_canvas(&mut self, src: &Canvas, at: Pos2) {
        let s = self.scale;
        let ox = (at.x * s).round() as i32;
        let oy = (at.y * s).round() as i32;
        let (sw, sh) = (src.width() as i32, src.height() as i32);
        let area = self.clip().intersect(IRect {
            x0: ox,
            y0: oy,
            x1: ox + sw,
            y1: oy + sh,
        });
        if area.is_empty() {
            return;
        }
        let dw = self.width() as usize;
        let sd = src.pixmap.data();
        let dd = self.pixmap.data_mut();
        let len = (area.x1 - area.x0) as usize * 4;
        for y in area.y0..area.y1 {
            let si = ((y - oy) as usize * sw as usize + (area.x0 - ox) as usize) * 4;
            let di = (y as usize * dw + area.x0 as usize) * 4;
            let srow = &sd[si..si + len];
            let drow = &mut dd[di..di + len];
            if srow.as_chunks::<4>().0.iter().all(|p| p[3] == 255) {
                // Непрозрачная строка — простое копирование.
                drow.copy_from_slice(srow);
                continue;
            }
            for (d, p) in drow
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(srow.as_chunks::<4>().0.iter())
            {
                let a = p[3] as u32;
                if a == 255 {
                    d.copy_from_slice(p);
                } else if a != 0 {
                    let inv = 255 - a;
                    for i in 0..4 {
                        d[i] = (p[i] as u32 + div255(d[i] as u32 * inv)) as u8;
                    }
                }
            }
        }
    }

    /// Перевести кадр в формат softbuffer: `0x00RRGGBB` на пиксель, построчно,
    /// `out.len() >= width × height`. Полупрозрачные пиксели выводятся как наложенные на
    /// чёрный (кадр обычно начинается с непрозрачной заливки [`Canvas::clear`]).
    pub fn to_0rgb(&self, out: &mut [u32]) {
        let n = self.width() as usize * self.height() as usize;
        assert!(out.len() >= n, "буфер кадра меньше холста");
        for (o, p) in out[..n]
            .iter_mut()
            .zip(self.pixmap.data().as_chunks::<4>().0.iter())
        {
            let v = u32::from_le_bytes(*p);
            // В памяти R, G, B, A → 0x00RRGGBB.
            *o = (v & 0xFF) << 16 | (v & 0xFF00) | (v >> 16) & 0xFF;
        }
    }

    /// Цвет пикселя (физические координаты) без умножения на альфу; вне холста —
    /// прозрачный. Для тестов.
    pub fn pixel(&self, x: u32, y: u32) -> Color {
        match self.pixmap.pixel(x, y) {
            Some(p) => {
                let c = p.demultiply();
                Color::rgba(c.red(), c.green(), c.blue(), c.alpha())
            }
            None => Color::TRANSPARENT,
        }
    }

    /// Пиксели tiny-skia (RGBA с предварительно умноженной альфой).
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Пиксели tiny-skia для изменения напрямую (отсечение и масштаб не применяются).
    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }
}

/// Выборка по одной оси для [`Canvas::image`]: для каждого физического пикселя
/// `d0..d1` — номер пикселя картинки и вес следующего (в 1/256; 0 — без смешивания).
/// `origin` — начало картинки, `step` — физических пикселей на пиксель картинки.
fn sample_axis(
    d0: i32,
    d1: i32,
    origin: f32,
    step: f32,
    n: usize,
    linear: bool,
) -> Vec<(usize, u16)> {
    let last = n as f32 - 1.0;
    (d0..d1)
        .map(|d| {
            let u = (d as f32 + 0.5 - origin) / step;
            if linear {
                let u = (u - 0.5).clamp(0.0, last);
                let i = u.floor();
                (i as usize, ((u - i) * 256.0) as u16)
            } else {
                (u.floor().clamp(0.0, last) as usize, 0)
            }
        })
        .collect()
}

/// Размер строки в логических точках при масштабе `ppp` — то же, что
/// [`Canvas::measure_text`], но без холста (например, для раскладки до рисования).
pub fn measure_text_scaled(text: &str, size: f32, font: FontKind, ppp: f32) -> Vec2 {
    let (w, h) = text::measure_px(text, size * ppp, font);
    vec2(w / ppp, h / ppp)
}
