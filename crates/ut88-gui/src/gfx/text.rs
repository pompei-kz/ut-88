//! Текст: вшитые шрифты DejaVu, раскладка строки, кэш растеризованных глифов.
//!
//! Размер шрифта задаётся, как в egui, размером «em» (кегль): при размере 14 буква «М»
//! заметно ниже 14 точек, а высота строки (`ascent − descent`) у DejaVu ≈ 1,16 × размера.
//!
//! Глифы растеризуются ab_glyph в физических пикселях и кэшируются на весь процесс.
//! Ключ кэша — шрифт, глиф, размер (квантуется до 1/4 пикселя) и субпиксельный сдвиг по X
//! (четверти пикселя); базовая линия ставится на целый пиксель.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

use ab_glyph::{Font, FontRef, GlyphId, OutlinedGlyph, PxScaleFactor};

/// Шрифт DejaVu Sans (пропорциональный).
static SANS_TTF: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
/// Шрифт DejaVu Sans Mono (моноширинный).
static MONO_TTF: &[u8] = include_bytes!("../../assets/fonts/DejaVuSansMono.ttf");

/// Квантование размера шрифта и субпиксельного сдвига: доли пикселя.
const QUANT: f32 = 4.0;
/// Предел числа глифов в кэше: при переполнении кэш очищается (так бывает только при
/// непрерывном изменении размера окна, когда размеры шрифта плавают).
const CACHE_LIMIT: usize = 16384;

/// Начертание шрифта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontKind {
    /// Пропорциональный DejaVu Sans (аналог `FontId::proportional`).
    #[default]
    Sans,
    /// Моноширинный DejaVu Sans Mono (аналог `FontId::monospace`).
    Mono,
}

/// Разобранный шрифт и его метрики в единицах шрифта.
struct FontData {
    font: FontRef<'static>,
    units_per_em: f32,
    ascent: f32,
    descent: f32,
}

impl FontData {
    fn parse(bytes: &'static [u8]) -> Self {
        let font = FontRef::try_from_slice(bytes).expect("вшитый шрифт DejaVu повреждён");
        FontData {
            units_per_em: font.units_per_em().unwrap_or(2048.0),
            ascent: font.ascent_unscaled(),
            descent: font.descent_unscaled(),
            font,
        }
    }
}

static FONTS: LazyLock<[FontData; 2]> =
    LazyLock::new(|| [FontData::parse(SANS_TTF), FontData::parse(MONO_TTF)]);

fn font_data(kind: FontKind) -> &'static FontData {
    &FONTS[kind as usize]
}

/// Растеризованный глиф: альфа-маска и её смещение от начала глифа на базовой линии.
struct GlyphBitmap {
    left: i32,
    top: i32,
    w: usize,
    h: usize,
    alpha: Vec<u8>,
}

/// Ключ кэша глифов.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    kind: FontKind,
    glyph: u16,
    /// Размер в четвертях физического пикселя.
    size_q: u32,
    /// Субпиксельный сдвиг в четвертях пикселя (0..4).
    sub: u8,
}

/// Кэш на весь процесс.
#[derive(Default)]
struct Cache {
    /// Символ → глиф и ширина шага (в единицах шрифта).
    chars: HashMap<(FontKind, char), (GlyphId, f32)>,
    /// Растеризованные глифы (`None` — у глифа нет контура, например пробел).
    glyphs: HashMap<GlyphKey, Option<GlyphBitmap>>,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

fn cache() -> MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

impl Cache {
    fn char_info(&mut self, kind: FontKind, c: char) -> (GlyphId, f32) {
        *self.chars.entry((kind, c)).or_insert_with(|| {
            let f = &font_data(kind).font;
            let id = f.glyph_id(c);
            (id, f.h_advance_unscaled(id))
        })
    }

    fn glyph(&mut self, key: GlyphKey) -> Option<&GlyphBitmap> {
        if !self.glyphs.contains_key(&key) && self.glyphs.len() >= CACHE_LIMIT {
            self.glyphs.clear();
        }
        self.glyphs
            .entry(key)
            .or_insert_with(|| rasterize(key))
            .as_ref()
    }
}

fn rasterize(key: GlyphKey) -> Option<GlyphBitmap> {
    let fd = font_data(key.kind);
    let id = GlyphId(key.glyph);
    let outline = fd.font.outline(id)?;
    let k = key.size_q as f32 / QUANT / fd.units_per_em;
    let glyph = ab_glyph::Glyph {
        id,
        // Масштаб берётся из PxScaleFactor ниже; поле при растеризации не читается.
        scale: ab_glyph::PxScale::from(0.0),
        position: ab_glyph::point(key.sub as f32 / QUANT, 0.0),
    };
    let og = OutlinedGlyph::new(
        glyph,
        outline,
        PxScaleFactor {
            horizontal: k,
            vertical: k,
        },
    );
    let bb = og.px_bounds();
    let (w, h) = (bb.width() as usize, bb.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut alpha = vec![0u8; w * h];
    og.draw(|x, y, v| {
        let (x, y) = (x as usize, y as usize);
        if x < w && y < h {
            alpha[y * w + x] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    });
    Some(GlyphBitmap {
        left: bb.min.x as i32,
        top: bb.min.y as i32,
        w,
        h,
        alpha,
    })
}

/// Размер шрифта в физических пикселях после квантования, в четвертях пикселя.
fn quantize_size(size_px: f32) -> u32 {
    if size_px.is_finite() && size_px > 0.0 {
        (size_px * QUANT).round().max(1.0) as u32
    } else {
        0
    }
}

/// Раскладка строки в физических пикселях.
pub(crate) struct Layout {
    /// Глифы и их X от начала строки.
    pub glyphs: Vec<(GlyphId, f32)>,
    /// Ширина строки (сумма шагов с кернингом).
    pub width: f32,
    /// Высота строки (`ascent − descent`).
    pub height: f32,
    /// Подъём над базовой линией.
    pub ascent: f32,
    /// Квантованный размер (четверти пикселя).
    size_q: u32,
    kind: FontKind,
}

fn layout_locked(cache: &mut Cache, text: &str, size_px: f32, kind: FontKind) -> Layout {
    let fd = font_data(kind);
    let size_q = quantize_size(size_px);
    let k = size_q as f32 / QUANT / fd.units_per_em;
    let mut glyphs = Vec::with_capacity(text.len());
    let mut x = 0.0f32;
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        if c.is_control() {
            continue;
        }
        let (id, adv) = cache.char_info(kind, c);
        if let Some(p) = prev {
            x += fd.font.kern_unscaled(p, id) * k;
        }
        glyphs.push((id, x));
        x += adv * k;
        prev = Some(id);
    }
    Layout {
        glyphs,
        width: x,
        height: (fd.ascent - fd.descent) * k,
        ascent: fd.ascent * k,
        size_q,
        kind,
    }
}

/// Размер строки в физических пикселях: ширина и высота (`ascent − descent`).
pub(crate) fn measure_px(text: &str, size_px: f32, kind: FontKind) -> (f32, f32) {
    let l = layout_locked(&mut cache(), text, size_px, kind);
    (l.width, l.height)
}

/// Прямоугольник отсечения в целых физических пикселях: `x0 <= x < x1`, `y0 <= y < y1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    pub fn intersect(&self, o: IRect) -> IRect {
        IRect {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }
}

/// Деление на 255 с округлением для `x` в диапазоне `0..=255*255`.
#[inline]
pub(crate) fn div255(x: u32) -> u32 {
    (x + 128 + ((x + 128) >> 8)) >> 8
}

/// Нарисовать строку в буфер RGBA с предварительно умноженной альфой.
///
/// `left` — X начала строки, `top` — Y верха строки (физические пиксели). Возвращает
/// раскладку (размеры строки).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text(
    data: &mut [u8],
    stride: usize,
    clip: IRect,
    left: f32,
    top: f32,
    text: &str,
    size_px: f32,
    kind: FontKind,
    color: [u8; 4],
) -> (f32, f32) {
    let mut cache = cache();
    let layout = layout_locked(&mut cache, text, size_px, kind);
    let size = (layout.width, layout.height);
    if color[3] == 0 || layout.size_q == 0 || clip.is_empty() {
        return size;
    }
    let baseline = (top + layout.ascent).round() as i32;
    let [r, g, b, a] = color.map(u32::from);
    for &(id, gx) in &layout.glyphs {
        let pen = left + gx;
        let mut ix = pen.floor();
        let mut sub = ((pen - ix) * QUANT).round() as u8;
        if sub as f32 >= QUANT {
            sub = 0;
            ix += 1.0;
        }
        let key = GlyphKey {
            kind: layout.kind,
            glyph: id.0,
            size_q: layout.size_q,
            sub,
        };
        let Some(bm) = cache.glyph(key) else {
            continue;
        };
        let gx0 = ix as i32 + bm.left;
        let gy0 = baseline + bm.top;
        let area = clip.intersect(IRect {
            x0: gx0,
            y0: gy0,
            x1: gx0 + bm.w as i32,
            y1: gy0 + bm.h as i32,
        });
        if area.is_empty() {
            continue;
        }
        for y in area.y0..area.y1 {
            let src = &bm.alpha[(y - gy0) as usize * bm.w..][..bm.w];
            let row = y as usize * stride;
            for x in area.x0..area.x1 {
                let cov = src[(x - gx0) as usize] as u32;
                if cov == 0 {
                    continue;
                }
                let sa = div255(a * cov);
                if sa == 0 {
                    continue;
                }
                let inv = 255 - sa;
                let p = &mut data[(row + x as usize) * 4..][..4];
                p[0] = (div255(r * sa) + div255(p[0] as u32 * inv)) as u8;
                p[1] = (div255(g * sa) + div255(p[1] as u32 * inv)) as u8;
                p[2] = (div255(b * sa) + div255(p[2] as u32 * inv)) as u8;
                p[3] = (sa + div255(p[3] as u32 * inv)) as u8;
            }
        }
    }
    size
}

/// Символы строки, которых нет в шрифте `kind` (каждый — один раз, в порядке появления).
/// Управляющие символы (`'\n'`, `'\t'`, …) не рисуются и не считаются.
pub fn missing_glyphs(text: &str, kind: FontKind) -> Vec<char> {
    let mut cache = cache();
    let mut out = Vec::new();
    for c in text.chars() {
        if c.is_control() || out.contains(&c) {
            continue;
        }
        if cache.char_info(kind, c).0.0 == 0 {
            out.push(c);
        }
    }
    out
}
