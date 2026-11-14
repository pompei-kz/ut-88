//! Векторный рисунок компьютера на программном холсте [`Canvas`]: корпус
//! процессорного модуля с семисегментными индикаторами и клавиатурой, телевизор-монитор,
//! полная клавиатура.
//!
//! Перенос `draw.rs` (egui) один в один по геометрии и цветам. Всё рисуется в «условных
//! единицах» и масштабируется под окно преобразованием [`Xf`].
//!
//! Рисунок делится на две части, чтобы большую часть кадра брать из кэша:
//! - **статическая** ([`computer_static`]) — стол, корпуса, клавиши, телевизор с погасшим
//!   экраном и погашенные индикаторы; зависит от прямоугольника, масштаба, нажатых
//!   клавиш и цвета люминофора;
//! - **живая** ([`computer_live`]) — горящие индикаторы и изображение на экране; рисуется
//!   поверх статической каждый кадр.
//!
//! [`computer`] рисует обе части подряд — результат тот же, что у слоя-кэша, наложенного
//! на кадр, плюс живая часть.

use ut88_core::{KbKey, PadKey};

use crate::gfx::{Align2, Canvas, Color, FontKind, Pos2, Rect, Vec2, pos2, vec2};
use crate::keymap;

/// Преобразование условных единиц в точки экрана.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xf {
    /// Начало координат на экране.
    pub origin: Pos2,
    /// Масштаб.
    pub s: f32,
}

impl Xf {
    /// Вписать область `w × h` единиц в прямоугольник `r` по центру.
    pub fn fit(r: Rect, w: f32, h: f32) -> Self {
        let s = (r.width() / w).min(r.height() / h);
        let origin = r.center() - vec2(w, h) * s / 2.0;
        Self { origin, s }
    }

    /// Точка.
    pub fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.s
    }

    /// Прямоугольник.
    pub fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.p(x, y), vec2(w, h) * self.s)
    }

    /// Длина.
    pub fn l(&self, v: f32) -> f32 {
        v * self.s
    }

    /// Вложенное преобразование: начало в (x, y), масштаб × k.
    pub fn sub(&self, x: f32, y: f32, k: f32) -> Self {
        Self {
            origin: self.p(x, y),
            s: self.s * k,
        }
    }
}

/// Палитра корпуса.
pub mod pal {
    use crate::gfx::Color;

    /// Корпус (бежевая пластмасса).
    pub const CASE: Color = Color::from_rgb(0xD9, 0xD0, 0xB8);
    /// Тень корпуса.
    pub const CASE_DARK: Color = Color::from_rgb(0xA8, 0x9E, 0x86);
    /// Светлая грань корпуса.
    pub const CASE_LIGHT: Color = Color::from_rgb(0xEC, 0xE6, 0xD4);
    /// Окно индикаторов.
    pub const WINDOW: Color = Color::from_rgb(0x1C, 0x0C, 0x0C);
    /// Горящий сегмент.
    pub const SEG_ON: Color = Color::from_rgb(0xFF, 0x30, 0x18);
    /// Погашенный сегмент.
    pub const SEG_OFF: Color = Color::from_rgb(0x2C, 0x10, 0x0E);
    /// Тёмная клавиша.
    pub const KEY_DARK: Color = Color::from_rgb(0x3A, 0x2E, 0x2A);
    /// Светлая клавиша.
    pub const KEY_LIGHT: Color = Color::from_rgb(0xF0, 0xEC, 0xE0);
    /// Серая клавиша.
    pub const KEY_GREY: Color = Color::from_rgb(0x8C, 0x86, 0x7C);
    /// Надписи на корпусе.
    pub const INK: Color = Color::from_rgb(0x4A, 0x3C, 0x30);
    /// Стол (фон под рисунком).
    pub const TABLE: Color = Color::from_rgb(0x2C, 0x2A, 0x2E);
}

/// Что нарисовано и куда можно нажать.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hit {
    /// Кнопка модуля 1.
    Pad(PadKey),
    /// Клавиша полной клавиатуры.
    Kb(KbKey),
}

/// Как нарисована клавиша — всё, что нужно, чтобы перерисовать её одну
/// ([`redraw_key`]).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyFace {
    /// Клавиша.
    pub hit: Hit,
    /// Прямоугольник клавиши (он же зона нажатия).
    pub rect: Rect,
    /// Цвет клавиши.
    pub color: Color,
    /// Основная надпись.
    pub main: String,
    /// Надпись сверху.
    pub top: String,
    /// Кегль надписи.
    pub font: f32,
    /// Однотонный фон под клавишей (панель или корпус).
    pub under: Color,
}

impl KeyFace {
    /// Область, которую клавиша закрашивает (с тенью), расширенная до целых физических
    /// пикселей при масштабе `ppp`: сглаженные края фигур не выходят за пиксели, которых
    /// касается фигура. Лежит на однотонном фоне `under`.
    fn area(&self, ppp: f32) -> Rect {
        let depth = (self.rect.height() * 0.09).max(2.0);
        let min = self.rect.min;
        let max = self.rect.max + vec2(0.0, depth * 0.6);
        Rect::from_min_max(
            pos2((min.x * ppp).floor() / ppp, (min.y * ppp).floor() / ppp),
            pos2((max.x * ppp).ceil() / ppp, (max.y * ppp).ceil() / ppp),
        )
    }

    /// Нарисовать клавишу.
    fn draw(&self, c: &mut Canvas, down: bool) {
        key(
            c, self.rect, self.color, &self.main, &self.top, down, self.font,
        );
    }
}

/// Зоны нажатия, собранные при рисовании.
#[derive(Debug, Default, Clone)]
pub struct Hits {
    /// `(область на экране, клавиша)`.
    pub zones: Vec<(Rect, Hit)>,
    /// Как нарисованы клавиши (в том же порядке, что `zones`).
    pub faces: Vec<KeyFace>,
}

impl Hits {
    /// Нарисовать клавишу и запомнить её зону и вид.
    fn put(&mut self, c: &mut Canvas, face: KeyFace, pressed: &dyn Pressed) {
        face.draw(c, pressed.is_down(face.hit));
        self.zones.push((face.rect, face.hit));
        self.faces.push(face);
    }

    /// Клавиша под точкой `p` (последняя нарисованная, как в `App::mouse_input`).
    pub fn at(&self, p: Pos2) -> Option<Hit> {
        self.zones
            .iter()
            .rev()
            .find(|(r, _)| r.contains(p))
            .map(|&(_, h)| h)
    }
}

/// Состояние для рисования клавиш: какие нажаты.
pub trait Pressed {
    /// Клавиша показывается нажатой.
    fn is_down(&self, h: Hit) -> bool;
}

/// Ни одна клавиша не нажата.
pub struct NonePressed;

impl Pressed for NonePressed {
    fn is_down(&self, _h: Hit) -> bool {
        false
    }
}

/// Радиус скругления так, как его хранит `egui::CornerRadius::same(v as u8)`: целое
/// число точек от 0 до 255 (дробная часть отбрасывается).
fn cr(v: f32) -> f32 {
    f32::from(v.clamp(0.0, 255.0) as u8)
}

/// Рамка внутри прямоугольника (`StrokeKind::Inside` в egui).
fn stroke_inside(c: &mut Canvas, r: Rect, radius: f32, width: f32, color: Color) {
    c.stroke_rect(
        r.shrink(width / 2.0),
        (radius - width / 2.0).max(0.0),
        width,
        color,
    );
}

fn darker(c: Color, k: f32) -> Color {
    Color::from_rgb(
        (f32::from(c.r()) * k) as u8,
        (f32::from(c.g()) * k) as u8,
        (f32::from(c.b()) * k) as u8,
    )
}

fn lighter(c: Color, k: f32) -> Color {
    let f = |v: u8| (f32::from(v) + (255.0 - f32::from(v)) * k) as u8;
    Color::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Рельефный корпус.
pub fn case(c: &mut Canvas, r: Rect, radius: f32, color: Color) {
    let rad = cr(radius);
    c.fill_rect(
        r.translate(vec2(radius * 0.3, radius * 0.5)),
        rad,
        Color::from_black_alpha(90),
    );
    c.fill_rect(r, rad, darker(color, 0.78));
    let top = Rect::from_min_max(r.min, r.max - vec2(radius * 0.25, radius * 0.4));
    c.fill_rect(top, rad, color);
    stroke_inside(c, r, rad, 1.0, darker(color, 0.6));
}

/// Клавиша «калькуляторного» вида. `top` — надпись сверху (мелко), `main` — основная.
#[allow(clippy::too_many_arguments)]
pub fn key(c: &mut Canvas, r: Rect, color: Color, main: &str, top: &str, down: bool, font: f32) {
    let depth = (r.height() * 0.09).max(2.0);
    let radius = (r.height() * 0.14).clamp(1.0, 255.0) as u8;
    let rad = f32::from(radius);
    // Тень и боковина.
    c.fill_rect(
        r.translate(vec2(0.0, depth * 0.6)),
        rad,
        Color::from_black_alpha(80),
    );
    let base = Rect::from_min_max(r.min + vec2(0.0, depth), r.max);
    c.fill_rect(base, rad, darker(color, 0.55));
    // Верхняя грань.
    let shift = if down { depth * 0.8 } else { 0.0 };
    let face = Rect::from_min_max(r.min + vec2(0.0, shift), r.max - vec2(0.0, depth - shift));
    c.fill_rect(face, rad, if down { darker(color, 0.85) } else { color });
    let inner = face.shrink(face.width().min(face.height()) * 0.08);
    c.fill_rect(
        inner,
        f32::from(radius / 2 + 1),
        if down {
            darker(color, 0.9)
        } else {
            lighter(color, 0.08)
        },
    );
    let ink = if color.r() as u32 + color.g() as u32 + color.b() as u32 > 380 {
        Color::from_rgb(0x20, 0x18, 0x14)
    } else {
        Color::from_rgb(0xF4, 0xF0, 0xE6)
    };
    if let Some(dir) = arrow_dir(main) {
        // Стрелки рисуются линиями — так же, как в egui-версии.
        let ctr = face.center();
        let len = font * 0.45;
        let tip = ctr + dir * len;
        let tail = ctr - dir * len;
        let side = vec2(-dir.y, dir.x) * len * 0.55;
        let w = font * 0.12;
        c.line(tail, tip, w, ink);
        c.line(tip, tip - dir * len * 0.6 + side, w, ink);
        c.line(tip, tip - dir * len * 0.6 - side, w, ink);
    } else if top.is_empty() {
        let lines: Vec<&str> = main.split('\n').collect();
        let n = lines.len() as f32;
        for (i, l) in lines.iter().enumerate() {
            let fs = if n > 1.0 { font * 0.62 } else { font };
            let y = face.center().y + (i as f32 - (n - 1.0) / 2.0) * fs * 1.1;
            c.text(
                pos2(face.center().x, y),
                Align2::CENTER_CENTER,
                l,
                fs,
                FontKind::Sans,
                ink,
            );
        }
    } else {
        c.text(
            pos2(face.center().x, face.top() + face.height() * 0.3),
            Align2::CENTER_CENTER,
            top,
            font * 0.72,
            FontKind::Sans,
            ink,
        );
        c.text(
            pos2(face.center().x, face.top() + face.height() * 0.7),
            Align2::CENTER_CENTER,
            main,
            font * 0.72,
            FontKind::Sans,
            ink,
        );
    }
}

/// Направление стрелки для подписей «←↑→↓↖».
fn arrow_dir(label: &str) -> Option<Vec2> {
    Some(match label {
        "←" => vec2(-1.0, 0.0),
        "→" => vec2(1.0, 0.0),
        "↑" => vec2(0.0, -1.0),
        "↓" => vec2(0.0, 1.0),
        "↖" => vec2(-1.0, -1.0).normalized(),
        _ => return None,
    })
}

/// Семисегментный разряд в прямоугольнике `r`; `segs` — бит 0 = a … бит 6 = g.
pub fn seven_segment(c: &mut Canvas, r: Rect, segs: u8) {
    let w = r.width();
    let h = r.height();
    let t = w * 0.17; // толщина сегмента
    let g = t * 0.12; // зазор
    let skew = 0.12; // наклон цифр
    let x0 = r.left() + t * 0.5 + h * skew * 0.5;
    let x1 = r.right() - t * 0.5 - h * skew * 0.5;
    let y0 = r.top() + t * 0.5;
    let ym = r.center().y;
    let y1 = r.bottom() - t * 0.5;
    let sk = |pt: Pos2| pos2(pt.x + (ym - pt.y) * skew, pt.y);
    let hseg = |xa: f32, xb: f32, y: f32| -> [Pos2; 6] {
        let xa = xa + g;
        let xb = xb - g;
        [
            pos2(xa, y),
            pos2(xa + t / 2.0, y - t / 2.0),
            pos2(xb - t / 2.0, y - t / 2.0),
            pos2(xb, y),
            pos2(xb - t / 2.0, y + t / 2.0),
            pos2(xa + t / 2.0, y + t / 2.0),
        ]
        .map(sk)
    };
    let vseg = |x: f32, ya: f32, yb: f32| -> [Pos2; 6] {
        let ya = ya + g;
        let yb = yb - g;
        [
            pos2(x, ya),
            pos2(x + t / 2.0, ya + t / 2.0),
            pos2(x + t / 2.0, yb - t / 2.0),
            pos2(x, yb),
            pos2(x - t / 2.0, yb - t / 2.0),
            pos2(x - t / 2.0, ya + t / 2.0),
        ]
        .map(sk)
    };
    let shapes = [
        hseg(x0, x1, y0), // a
        vseg(x1, y0, ym), // b
        vseg(x1, ym, y1), // c
        hseg(x0, x1, y1), // d
        vseg(x0, ym, y1), // e
        vseg(x0, y0, ym), // f
        hseg(x0, x1, ym), // g
    ];
    for (i, pts) in shapes.iter().enumerate() {
        let on = segs & (1 << i) != 0;
        if on {
            // Ореол свечения.
            c.stroke_polygon(
                pts,
                t * 0.5,
                Color::from_rgba_unmultiplied(0xFF, 0x30, 0x10, 40),
            );
        }
        c.fill_polygon(pts, if on { pal::SEG_ON } else { pal::SEG_OFF });
    }
}

/// Размер процессорного модуля в условных единицах.
pub const MODULE_W: f32 = 620.0;
/// Высота процессорного модуля.
pub const MODULE_H: f32 = 580.0;

/// Разряды индикаторов: ширина, высота, промежуток, промежуток между группами
/// «адрес» и «данные».
const DIG_W: f32 = 56.0;
const DIG_H: f32 = 92.0;
const DIG_GAP: f32 = 18.0;
const DIG_GROUP_GAP: f32 = 34.0;
const DIG_TOTAL: f32 = 6.0 * DIG_W + 5.0 * DIG_GAP + DIG_GROUP_GAP;
/// Верх разрядов.
const DIG_Y: f32 = 100.0;

/// Шесть разрядов индикаторов процессорного модуля (без окна под ними).
fn digits(c: &mut Canvas, x: Xf, segments: [u8; 6]) {
    let mut dx = (MODULE_W - DIG_TOTAL) / 2.0;
    for (i, &s) in segments.iter().enumerate() {
        seven_segment(c, x.r(dx, DIG_Y, DIG_W, DIG_H), s);
        dx += DIG_W + DIG_GAP;
        if i == 3 {
            dx += DIG_GROUP_GAP - DIG_GAP + DIG_GAP;
        }
    }
}

/// Часть окна индикаторов, которую перекрашивает [`indicators`]: все разряды с ореолами,
/// но без скруглённых углов окна и блика на стекле.
fn indicators_area(x: Xf) -> Rect {
    x.r(44.0, 94.0, MODULE_W - 88.0, 116.0)
}

/// Перерисовать индикаторы процессорного модуля поверх уже нарисованного модуля
/// (живая часть рисунка): закрасить окно под разрядами и нарисовать разряды заново.
/// Результат совпадает с [`cpu_module`] с теми же сегментами.
pub fn indicators(c: &mut Canvas, x: Xf, segments: [u8; 6]) {
    c.fill_rect(indicators_area(x), 0.0, pal::WINDOW);
    digits(c, x, segments);
}

/// Процессорный модуль («блок интерфейса»): индикаторы и 18 кнопок.
pub fn cpu_module(
    c: &mut Canvas,
    x: Xf,
    segments: [u8; 6],
    pressed: &dyn Pressed,
    hits: &mut Hits,
) {
    // Корпус.
    case(c, x.r(0.0, 0.0, MODULE_W, MODULE_H), x.l(26.0), pal::CASE);
    // Вентиляционные прорези сверху.
    for i in 0..18 {
        let xx = 60.0 + i as f32 * 28.0;
        c.line(
            x.p(xx, 22.0),
            x.p(xx + 14.0, 22.0),
            x.l(4.0),
            pal::CASE_DARK,
        );
    }
    // Надпись.
    c.text(
        x.p(36.0, 52.0),
        Align2::LEFT_CENTER,
        "ЮТ-88",
        x.l(26.0),
        FontKind::Sans,
        pal::INK,
    );
    c.text(
        x.p(MODULE_W - 36.0, 52.0),
        Align2::RIGHT_CENTER,
        "МИКРО-ЭВМ",
        x.l(16.0),
        FontKind::Sans,
        pal::INK,
    );
    // Окно индикаторов (тёмное стекло).
    let win = x.r(36.0, 76.0, MODULE_W - 72.0, 140.0);
    c.fill_rect(win.expand(x.l(6.0)), cr(x.l(8.0)), pal::CASE_DARK);
    c.fill_rect(win, cr(x.l(4.0)), pal::WINDOW);
    // Блик на стекле.
    c.fill_rect(
        x.r(44.0, 82.0, MODULE_W - 88.0, 10.0),
        cr(x.l(3.0)),
        Color::from_white_alpha(14),
    );
    digits(c, x, segments);
    // Подписи разрядов: HL — адрес, A — данные.
    let (dw, gap) = (DIG_W, DIG_GAP);
    let sx = (MODULE_W - DIG_TOTAL) / 2.0;
    let lbl = |c: &mut Canvas, text: &str, from: f32, to: f32| {
        c.line(x.p(from, 236.0), x.p(to, 236.0), x.l(1.5), pal::INK);
        c.text(
            x.p((from + to) / 2.0, 250.0),
            Align2::CENTER_CENTER,
            text,
            x.l(13.0),
            FontKind::Sans,
            pal::INK,
        );
    };
    lbl(c, "H", sx, sx + 2.0 * dw + gap);
    lbl(c, "L", sx + 2.0 * (dw + gap), sx + 4.0 * dw + 3.0 * gap);
    let ax = sx + 4.0 * (dw + gap) + DIG_GROUP_GAP;
    lbl(c, "A", ax, ax + 2.0 * dw + gap);

    // Клавиатура 4 × 4 (рис. 1 журнала: C D E F / 8 9 A B / 4 5 6 7 / 0 1 2 3).
    let kx = 60.0;
    let ky = 282.0;
    let ks = 54.0;
    let kg = 10.0;
    let pad_panel = x.r(
        kx - 14.0,
        ky - 14.0,
        4.0 * ks + 3.0 * kg + 28.0,
        4.0 * ks + 3.0 * kg + 28.0,
    );
    c.fill_rect(pad_panel, cr(x.l(6.0)), pal::CASE_DARK);
    const ROWS: [[u8; 4]; 4] = [
        [0xC, 0xD, 0xE, 0xF],
        [8, 9, 0xA, 0xB],
        [4, 5, 6, 7],
        [0, 1, 2, 3],
    ];
    for (ri, row) in ROWS.iter().enumerate() {
        for (ci, &d) in row.iter().enumerate() {
            let r = x.r(
                kx + ci as f32 * (ks + kg),
                ky + ri as f32 * (ks + kg),
                ks,
                ks,
            );
            let face = KeyFace {
                hit: Hit::Pad(PadKey::Hex(d)),
                rect: r,
                color: pal::KEY_DARK,
                main: format!("{d:X}"),
                top: String::new(),
                font: x.l(24.0),
                under: pal::CASE_DARK,
            };
            hits.put(c, face, pressed);
        }
    }
    // Правый блок: «ШАГ НАЗАД» и «УСТ».
    let rx = kx + 4.0 * ks + 3.0 * kg + 60.0;
    let panel = x.r(
        rx - 14.0,
        ky - 14.0,
        2.0 * ks + kg + 60.0,
        4.0 * ks + 3.0 * kg + 28.0,
    );
    c.fill_rect(panel, cr(x.l(6.0)), pal::CASE_DARK);
    let bw = 2.0 * ks + kg + 32.0;
    let r = x.r(rx, ky, bw, ks * 2.0 + kg);
    let face = KeyFace {
        hit: Hit::Pad(PadKey::StepBack),
        rect: r,
        color: pal::KEY_GREY,
        main: "ШАГ\nНАЗАД".into(),
        top: String::new(),
        font: x.l(30.0),
        under: pal::CASE_DARK,
    };
    hits.put(c, face, pressed);
    let r = x.r(rx, ky + 2.0 * (ks + kg), bw, ks * 2.0 + kg);
    let face = KeyFace {
        hit: Hit::Pad(PadKey::Reset),
        rect: r,
        color: pal::KEY_LIGHT,
        main: "УСТ".into(),
        top: String::new(),
        font: x.l(24.0),
        under: pal::CASE_DARK,
    };
    hits.put(c, face, pressed);

    // Гнездо магнитофона (рис. 1: «МАГНИТОФОН»).
    let jack = x.p(MODULE_W - 46.0, 300.0);
    c.fill_circle(jack, x.l(15.0), pal::CASE_DARK);
    c.fill_circle(jack, x.l(10.0), Color::from_rgb(0x30, 0x2A, 0x26));
    for i in 0..5 {
        let a = std::f32::consts::PI * (0.75 + i as f32 * 0.375);
        c.fill_circle(
            jack + Vec2::angled(a) * x.l(6.0),
            x.l(1.6),
            Color::from_gray(160),
        );
    }
    c.text(
        x.p(MODULE_W - 46.0, 326.0),
        Align2::CENTER_CENTER,
        "МАГ",
        x.l(11.0),
        FontKind::Sans,
        pal::INK,
    );
}

/// Размер телевизора.
pub const TV_W: f32 = 760.0;
/// Высота телевизора.
pub const TV_H: f32 = 560.0;

/// Ширина растра экрана дисплейного модуля в пикселях.
const RASTER_W: f32 = 384.0;
/// Высота растра.
const RASTER_H: f32 = 280.0;

/// Стекло кинескопа телевизора (область под изображением и свечением).
fn tv_glass(x: Xf) -> Rect {
    let bezel = x.r(24.0, 24.0, TV_W - 160.0, TV_H - 60.0);
    bezel.shrink(x.l(28.0))
}

/// Прямоугольник изображения на стекле: растр 384 × 280, вписанный с сохранением
/// пропорций.
fn tv_image_rect(scr: Rect) -> Rect {
    let k = (scr.width() / RASTER_W).min(scr.height() / RASTER_H);
    Rect::from_center_size(scr.center(), vec2(RASTER_W * k, RASTER_H * k))
}

/// Свечение и блик стекла — рисуются поверх изображения.
fn tv_glass_overlay(c: &mut Canvas, x: Xf, scr: Rect, glow: Color) {
    stroke_inside(
        c,
        scr,
        cr(x.l(30.0)),
        x.l(8.0),
        Color::from_rgba_unmultiplied(glow.r(), glow.g(), glow.b(), 18),
    );
    c.fill_rect(
        Rect::from_min_size(
            scr.min + vec2(x.l(20.0), x.l(10.0)),
            vec2(scr.width() * 0.4, x.l(14.0)),
        ),
        cr(x.l(7.0)),
        Color::from_white_alpha(10),
    );
}

/// Экран: картинка RGBA без умножения на альфу и её размер в пикселях.
pub type Screen<'a> = (&'a [u8], u32, u32);

/// Вывести изображение на экран уже нарисованного телевизора (живая часть рисунка):
/// картинка, а поверх неё — свечение и блик стекла, отсечённые по картинке, чтобы
/// вокруг неё они не легли второй раз. Результат совпадает с [`tv`] с тем же экраном.
pub fn tv_screen(c: &mut Canvas, x: Xf, screen: Screen<'_>, glow: Color) {
    let scr = tv_glass(x);
    let img = tv_image_rect(scr);
    let (rgba, w, h) = screen;
    c.image(img, w, h, rgba, true);
    // Отсечение округляется до пикселя так же, как область картинки в Canvas::image.
    c.push_clip(img);
    tv_glass_overlay(c, x, scr, glow);
    c.pop_clip();
}

/// Телевизор-монитор с экраном.
pub fn tv(c: &mut Canvas, x: Xf, screen: Option<Screen<'_>>, glow: Color) {
    // Корпус светлый, как на фото («Электроника»).
    case(
        c,
        x.r(0.0, 0.0, TV_W, TV_H),
        x.l(30.0),
        Color::from_rgb(0xE4, 0xE0, 0xD6),
    );
    // Тёмная рамка кинескопа.
    let bezel = x.r(24.0, 24.0, TV_W - 160.0, TV_H - 60.0);
    c.fill_rect(bezel, cr(x.l(24.0)), Color::from_rgb(0x26, 0x24, 0x22));
    let scr = tv_glass(x);
    c.fill_rect(
        scr.expand(x.l(6.0)),
        cr(x.l(30.0)),
        Color::from_rgb(0x0A, 0x0C, 0x0A),
    );
    if let Some((rgba, w, h)) = screen {
        c.image(tv_image_rect(scr), w, h, rgba, true);
    }
    // Свечение и блик стекла.
    tv_glass_overlay(c, x, scr, glow);
    // Панель ручек справа.
    let px = TV_W - 118.0;
    for (i, label) in ["ЯРК", "КОНТР", "ГРОМК"].iter().enumerate() {
        let ctr = x.p(px + 46.0, 90.0 + i as f32 * 110.0);
        c.fill_circle(
            ctr + vec2(x.l(2.0), x.l(3.0)),
            x.l(26.0),
            Color::from_black_alpha(70),
        );
        c.fill_circle(ctr, x.l(26.0), Color::from_rgb(0x3A, 0x36, 0x32));
        c.fill_circle(ctr, x.l(19.0), Color::from_rgb(0x52, 0x4C, 0x46));
        c.line(
            ctr,
            ctr + vec2(0.0, -x.l(17.0)),
            x.l(3.0),
            Color::from_gray(200),
        );
        c.text(
            ctr + vec2(0.0, x.l(40.0)),
            Align2::CENTER_CENTER,
            label,
            x.l(13.0),
            FontKind::Sans,
            pal::INK,
        );
    }
    // Индикатор питания.
    c.fill_circle(
        x.p(px + 46.0, TV_H - 70.0),
        x.l(6.0),
        Color::from_rgb(0x40, 0xE0, 0x40),
    );
    c.text(
        x.p(px + 46.0, TV_H - 48.0),
        Align2::CENTER_CENTER,
        "СЕТЬ",
        x.l(12.0),
        FontKind::Sans,
        pal::INK,
    );
    // Решётка динамика внизу.
    for i in 0..10 {
        let yy = TV_H - 26.0;
        let xx = 60.0 + i as f32 * 22.0;
        c.line(
            x.p(xx, yy),
            x.p(xx + 12.0, yy),
            x.l(3.0),
            Color::from_gray(150),
        );
    }
}

/// Ширина клавиатуры.
pub const KB_W: f32 = 1300.0;
/// Высота клавиатуры.
pub const KB_H: f32 = 420.0;

/// Ряды клавиатуры (рис. 2, 1989-05): `(сдвиг ряда в клавишах, клавиши)`; клавиша —
/// код основного символа.
const ROWS: [(f32, &[u8]); 4] = [
    (0.0, b";1234567890-"),
    (0.45, b"JCUKENG[]ZH:"),
    (0.0, b"FYWAPROLDV\\."),
    (0.0, b"Q^SMITXB@,/_"),
];

/// Надпись над основным символом клавиши: символ с СС или русская буква с РУС.
fn top_label(code: u8) -> String {
    match code {
        0x30 => String::new(),
        0x31..=0x3B => ut88_core::screen::koi7_to_char(code - 0x10).to_string(),
        0x2C..=0x2F => ut88_core::screen::koi7_to_char(code + 0x10).to_string(),
        b'_' => String::new(),
        0x40..=0x5E => ut88_core::screen::koi7_to_char(code + 0x20).to_string(),
        _ => String::new(),
    }
}

/// Полная клавиатура дисплейного модуля.
pub fn keyboard(c: &mut Canvas, x: Xf, pressed: &dyn Pressed, hits: &mut Hits) {
    case(c, x.r(0.0, 0.0, KB_W, KB_H), x.l(28.0), pal::CASE);
    let u = 72.0; // шаг клавиш
    let ks = 64.0; // размер клавиши
    let x0 = 40.0;
    let y0 = 36.0;
    let font = x.l(22.0);
    let mut put =
        |c: &mut Canvas, xx: f32, yy: f32, w: f32, h: Hit, color: Color, main: &str, top: &str| {
            let face = KeyFace {
                hit: h,
                rect: x.r(xx, yy, w, ks),
                color,
                main: main.into(),
                top: top.into(),
                font,
                under: pal::CASE,
            };
            hits.put(c, face, pressed);
        };
    for (ri, (shift, codes)) in ROWS.iter().enumerate() {
        let yy = y0 + ri as f32 * u;
        let mut xx = x0 + shift * u;
        // Левый модификатор.
        let left = match ri {
            2 => Some(KbKey::Us),
            3 => Some(KbKey::Ss),
            _ => None,
        };
        if let Some(m) = left {
            put(
                c,
                xx,
                yy,
                ks * 1.4,
                Hit::Kb(m),
                pal::KEY_GREY,
                &keymap::kb_label(m),
                "",
            );
            xx += u * 1.4 + 2.0;
        }
        for &code in codes.iter() {
            let (k, _) = KbKey::for_code(code).expect("клавиша раскладки");
            let main = if code == b'0' {
                "0".into()
            } else {
                keymap::code_label(code)
            };
            put(
                c,
                xx,
                yy,
                ks,
                Hit::Kb(k),
                pal::KEY_DARK,
                &main,
                &top_label(code),
            );
            xx += u;
        }
        match ri {
            0 => put(
                c,
                xx,
                yy,
                ks * 1.4,
                Hit::Kb(KbKey::Rus),
                pal::KEY_GREY,
                "РУС",
                "",
            ),
            1 => {
                let (k, _) = KbKey::for_code(0x0D).expect("ВК");
                put(c, xx, yy, ks * 1.3, Hit::Kb(k), pal::KEY_GREY, "ВК", "");
            }
            _ => {}
        }
    }
    // Пробел.
    let (sp, _) = KbKey::for_code(b' ').expect("пробел");
    let yy = y0 + 4.0 * u;
    put(
        c,
        x0 + 2.4 * u,
        yy,
        7.6 * u,
        Hit::Kb(sp),
        pal::KEY_LIGHT,
        "ПРОБЕЛ",
        "",
    );
    // Блок управления курсором справа (на рис. 2 журнала его нет, клавиши — из матрицы).
    let cx = x0 + 14.2 * u;
    let cursor: [(f32, f32, u8); 6] = [
        (0.0, 0.0, 0x0C),
        (1.0, 0.0, 0x1F),
        (0.5, 2.0, 0x19),
        (-0.5, 3.0, 0x08),
        (0.5, 3.0, 0x1A),
        (1.5, 3.0, 0x18),
    ];
    for (dx, dy, code) in cursor {
        let (k, _) = KbKey::for_code(code).expect("клавиша курсора");
        put(
            c,
            cx + dx * u,
            y0 + dy * u,
            ks,
            Hit::Kb(k),
            pal::KEY_GREY,
            &keymap::code_label(code),
            "",
        );
    }
}

/// Перерисовать одну клавишу `hit` нажатой или отпущенной поверх уже нарисованного
/// рисунка (например, кэш-слоя статической части), не перерисовывая всё: область
/// клавиши закрашивается фоном под ней, и клавиша рисуется заново. Результат совпадает с
/// рисованием всего рисунка с новым состоянием клавиши. `hits` — собранные при рисовании
/// этого рисунка. Возвращает перерисованную область; `None` — такой клавиши нет или
/// рисунок так мелок, что клавиши почти касаются друг друга (тогда перерисовывается
/// весь слой).
pub fn redraw_key(
    c: &mut Canvas,
    layout: &Layout,
    hits: &Hits,
    hit: Hit,
    down: bool,
) -> Option<Rect> {
    let face = hits.faces.iter().find(|f| f.hit == hit)?;
    let ppp = c.scale();
    let area = face.area(ppp);
    let crowded = hits
        .faces
        .iter()
        .any(|f| f.hit != hit && f.area(ppp).intersects(area));
    if crowded {
        return None;
    }
    c.push_clip(layout.rect);
    c.fill_rect(area, 0.0, face.under);
    face.draw(c, down);
    c.pop_clip();
    Some(area)
}

/// Раскладка рисунка компьютера в прямоугольнике окна (как `App::computer`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Прямоугольник рисунка (стол), логические точки.
    pub rect: Rect,
    /// Процессорный модуль.
    pub module: Xf,
    /// Телевизор (только полная конфигурация).
    pub tv: Option<Xf>,
    /// Клавиатура (только полная конфигурация).
    pub keyboard: Option<Xf>,
}

impl Layout {
    /// Раскладка для полной (`full`: телевизор, модуль, клавиатура) или минимальной
    /// (один модуль) конфигурации.
    pub fn new(rect: Rect, full: bool) -> Self {
        if full {
            let (w, h) = (KB_W + 60.0, TV_H + KB_H + 50.0);
            let x = Xf::fit(rect.shrink(8.0), w, h);
            Layout {
                rect,
                module: x.sub(TV_W + 50.0, 40.0, 0.86),
                tv: Some(x.sub(20.0, 10.0, 1.0)),
                keyboard: Some(x.sub(30.0, TV_H + 40.0, 1.0)),
            }
        } else {
            let x = Xf::fit(rect.shrink(12.0), MODULE_W + 20.0, MODULE_H + 20.0);
            Layout {
                rect,
                module: x.sub(10.0, 6.0, 1.0),
                tv: None,
                keyboard: None,
            }
        }
    }
}

/// Статическая часть рисунка: стол, корпуса, клавиши (с учётом нажатых), телевизор с
/// погасшим экраном, погашенные индикаторы. Зоны нажатия добавляются в `hits`.
/// Годится для кэш-слоя: меняется только вместе с раскладкой, масштабом, нажатыми
/// клавишами и цветом люминофора `glow`.
pub fn computer_static(
    c: &mut Canvas,
    layout: &Layout,
    glow: Color,
    pressed: &dyn Pressed,
    hits: &mut Hits,
) {
    c.push_clip(layout.rect);
    // Стол.
    c.fill_rect(layout.rect, 0.0, pal::TABLE);
    if let Some(t) = layout.tv {
        tv(c, t, None, glow);
    }
    cpu_module(c, layout.module, [0; 6], pressed, hits);
    if let Some(k) = layout.keyboard {
        keyboard(c, k, pressed, hits);
    }
    c.pop_clip();
}

/// Живая часть рисунка поверх статической: индикаторы и изображение на экране
/// телевизора (если `screen` есть и в раскладке есть телевизор).
pub fn computer_live(
    c: &mut Canvas,
    layout: &Layout,
    segments: [u8; 6],
    screen: Option<Screen<'_>>,
    glow: Color,
) {
    c.push_clip(layout.rect);
    if let (Some(t), Some(s)) = (layout.tv, screen) {
        tv_screen(c, t, s, glow);
    }
    indicators(c, layout.module, segments);
    c.pop_clip();
}

/// Весь рисунок компьютера в прямоугольнике `rect`: статическая часть, затем живая.
/// Возвращает раскладку (для повторного рисования живой части поверх кэша).
#[allow(clippy::too_many_arguments)]
pub fn computer(
    c: &mut Canvas,
    rect: Rect,
    full: bool,
    segments: [u8; 6],
    screen: Option<Screen<'_>>,
    glow: Color,
    pressed: &dyn Pressed,
    hits: &mut Hits,
) -> Layout {
    let layout = Layout::new(rect, full);
    computer_static(c, &layout, glow, pressed, hits);
    computer_live(c, &layout, segments, screen, glow);
    layout
}

#[cfg(test)]
mod tests;
