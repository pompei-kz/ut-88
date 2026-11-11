//! Векторный рисунок компьютера: корпус процессорного модуля с семисегментными
//! индикаторами и клавиатурой, телевизор-монитор, полная клавиатура.
//!
//! Всё рисуется в «условных единицах» и масштабируется под окно преобразованием
//! [`Xf`].

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2,
    vec2,
};
use ut88_core::{KbKey, PadKey};

use crate::keymap;

/// Преобразование условных единиц в точки экрана.
#[derive(Debug, Clone, Copy)]
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
    use eframe::egui::Color32;

    /// Корпус (бежевая пластмасса).
    pub const CASE: Color32 = Color32::from_rgb(0xD9, 0xD0, 0xB8);
    /// Тень корпуса.
    pub const CASE_DARK: Color32 = Color32::from_rgb(0xA8, 0x9E, 0x86);
    /// Светлая грань корпуса.
    pub const CASE_LIGHT: Color32 = Color32::from_rgb(0xEC, 0xE6, 0xD4);
    /// Окно индикаторов.
    pub const WINDOW: Color32 = Color32::from_rgb(0x1C, 0x0C, 0x0C);
    /// Горящий сегмент.
    pub const SEG_ON: Color32 = Color32::from_rgb(0xFF, 0x30, 0x18);
    /// Погашенный сегмент.
    pub const SEG_OFF: Color32 = Color32::from_rgb(0x2C, 0x10, 0x0E);
    /// Тёмная клавиша.
    pub const KEY_DARK: Color32 = Color32::from_rgb(0x3A, 0x2E, 0x2A);
    /// Светлая клавиша.
    pub const KEY_LIGHT: Color32 = Color32::from_rgb(0xF0, 0xEC, 0xE0);
    /// Серая клавиша.
    pub const KEY_GREY: Color32 = Color32::from_rgb(0x8C, 0x86, 0x7C);
    /// Надписи на корпусе.
    pub const INK: Color32 = Color32::from_rgb(0x4A, 0x3C, 0x30);
}

/// Что нарисовано и куда можно нажать.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hit {
    /// Кнопка модуля 1.
    Pad(PadKey),
    /// Клавиша полной клавиатуры.
    Kb(KbKey),
}

/// Зоны нажатия, собранные при рисовании.
#[derive(Default)]
pub struct Hits {
    /// `(область на экране, клавиша)`.
    pub zones: Vec<(Rect, Hit)>,
}

/// Состояние для рисования клавиш: какие нажаты.
pub trait Pressed {
    /// Клавиша показывается нажатой.
    fn is_down(&self, h: Hit) -> bool;
}

fn darker(c: Color32, k: f32) -> Color32 {
    Color32::from_rgb(
        (f32::from(c.r()) * k) as u8,
        (f32::from(c.g()) * k) as u8,
        (f32::from(c.b()) * k) as u8,
    )
}

fn lighter(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (f32::from(v) + (255.0 - f32::from(v)) * k) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Рельефный корпус.
pub fn case(p: &egui::Painter, r: Rect, radius: f32, color: Color32) {
    let cr = CornerRadius::same(radius.clamp(0.0, 255.0) as u8);
    p.rect_filled(
        r.translate(vec2(radius * 0.3, radius * 0.5)),
        cr,
        Color32::from_black_alpha(90),
    );
    p.rect_filled(r, cr, darker(color, 0.78));
    let top = Rect::from_min_max(r.min, r.max - vec2(radius * 0.25, radius * 0.4));
    p.rect_filled(top, cr, color);
    p.rect_stroke(
        r,
        cr,
        Stroke::new(1.0_f32, darker(color, 0.6)),
        StrokeKind::Inside,
    );
}

/// Клавиша «калькуляторного» вида. `top` — надпись сверху (мелко), `main` — основная.
#[allow(clippy::too_many_arguments)]
pub fn key(
    p: &egui::Painter,
    r: Rect,
    color: Color32,
    main: &str,
    top: &str,
    down: bool,
    font: f32,
) {
    let depth = (r.height() * 0.09).max(2.0);
    let radius = (r.height() * 0.14).clamp(1.0, 255.0) as u8;
    let cr = CornerRadius::same(radius);
    // Тень и боковина.
    p.rect_filled(
        r.translate(vec2(0.0, depth * 0.6)),
        cr,
        Color32::from_black_alpha(80),
    );
    let base = Rect::from_min_max(r.min + vec2(0.0, depth), r.max);
    p.rect_filled(base, cr, darker(color, 0.55));
    // Верхняя грань.
    let shift = if down { depth * 0.8 } else { 0.0 };
    let face = Rect::from_min_max(r.min + vec2(0.0, shift), r.max - vec2(0.0, depth - shift));
    p.rect_filled(face, cr, if down { darker(color, 0.85) } else { color });
    let inner = face.shrink(face.width().min(face.height()) * 0.08);
    p.rect_filled(
        inner,
        CornerRadius::same(radius / 2 + 1),
        if down {
            darker(color, 0.9)
        } else {
            lighter(color, 0.08)
        },
    );
    let ink = if color.r() as u32 + color.g() as u32 + color.b() as u32 > 380 {
        Color32::from_rgb(0x20, 0x18, 0x14)
    } else {
        Color32::from_rgb(0xF4, 0xF0, 0xE6)
    };
    if let Some(dir) = arrow_dir(main) {
        // Стрелки рисуются линиями: в шрифтах egui нет этих знаков.
        let c = face.center();
        let len = font * 0.45;
        let tip = c + dir * len;
        let tail = c - dir * len;
        let side = vec2(-dir.y, dir.x) * len * 0.55;
        let st = Stroke::new(font * 0.12, ink);
        p.line_segment([tail, tip], st);
        p.line_segment([tip, tip - dir * len * 0.6 + side], st);
        p.line_segment([tip, tip - dir * len * 0.6 - side], st);
    } else if top.is_empty() {
        let lines: Vec<&str> = main.split('\n').collect();
        let n = lines.len() as f32;
        for (i, l) in lines.iter().enumerate() {
            let fs = if n > 1.0 { font * 0.62 } else { font };
            let y = face.center().y + (i as f32 - (n - 1.0) / 2.0) * fs * 1.1;
            p.text(
                pos2(face.center().x, y),
                Align2::CENTER_CENTER,
                *l,
                FontId::proportional(fs),
                ink,
            );
        }
    } else {
        p.text(
            pos2(face.center().x, face.top() + face.height() * 0.3),
            Align2::CENTER_CENTER,
            top,
            FontId::proportional(font * 0.72),
            ink,
        );
        p.text(
            pos2(face.center().x, face.top() + face.height() * 0.7),
            Align2::CENTER_CENTER,
            main,
            FontId::proportional(font * 0.72),
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
pub fn seven_segment(p: &egui::Painter, r: Rect, segs: u8) {
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
    let hseg = |xa: f32, xb: f32, y: f32| -> Vec<Pos2> {
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
        .into_iter()
        .map(sk)
        .collect()
    };
    let vseg = |x: f32, ya: f32, yb: f32| -> Vec<Pos2> {
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
        .into_iter()
        .map(sk)
        .collect()
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
    for (i, pts) in shapes.into_iter().enumerate() {
        let on = segs & (1 << i) != 0;
        if on {
            // Ореол свечения.
            let glow: Vec<Pos2> = pts.clone();
            p.add(Shape::convex_polygon(
                glow,
                Color32::TRANSPARENT,
                Stroke::new(
                    t * 0.5,
                    Color32::from_rgba_unmultiplied(0xFF, 0x30, 0x10, 40),
                ),
            ));
        }
        p.add(Shape::convex_polygon(
            pts,
            if on { pal::SEG_ON } else { pal::SEG_OFF },
            Stroke::NONE,
        ));
    }
}

/// Размер процессорного модуля в условных единицах.
pub const MODULE_W: f32 = 620.0;
/// Высота процессорного модуля.
pub const MODULE_H: f32 = 580.0;

/// Процессорный модуль («блок интерфейса»): индикаторы и 18 кнопок.
pub fn cpu_module(
    p: &egui::Painter,
    x: Xf,
    segments: [u8; 6],
    pressed: &dyn Pressed,
    hits: &mut Hits,
) {
    // Корпус.
    case(p, x.r(0.0, 0.0, MODULE_W, MODULE_H), x.l(26.0), pal::CASE);
    // Вентиляционные прорези сверху.
    for i in 0..18 {
        let xx = 60.0 + i as f32 * 28.0;
        p.line_segment(
            [x.p(xx, 22.0), x.p(xx + 14.0, 22.0)],
            Stroke::new(x.l(4.0), pal::CASE_DARK),
        );
    }
    // Надпись.
    p.text(
        x.p(36.0, 52.0),
        Align2::LEFT_CENTER,
        "ЮТ-88",
        FontId::proportional(x.l(26.0)),
        pal::INK,
    );
    p.text(
        x.p(MODULE_W - 36.0, 52.0),
        Align2::RIGHT_CENTER,
        "МИКРО-ЭВМ",
        FontId::proportional(x.l(16.0)),
        pal::INK,
    );
    // Окно индикаторов (тёмное стекло).
    let win = x.r(36.0, 76.0, MODULE_W - 72.0, 140.0);
    p.rect_filled(
        win.expand(x.l(6.0)),
        CornerRadius::same(x.l(8.0) as u8),
        pal::CASE_DARK,
    );
    p.rect_filled(win, CornerRadius::same(x.l(4.0) as u8), pal::WINDOW);
    // Блик на стекле.
    p.rect_filled(
        x.r(44.0, 82.0, MODULE_W - 88.0, 10.0),
        CornerRadius::same(x.l(3.0) as u8),
        Color32::from_white_alpha(14),
    );
    let dw = 56.0;
    let dh = 92.0;
    let gap = 18.0;
    let group_gap = 34.0;
    let total = 6.0 * dw + 5.0 * gap + group_gap;
    let mut dx = (MODULE_W - total) / 2.0;
    for (i, &s) in segments.iter().enumerate() {
        seven_segment(p, x.r(dx, 100.0, dw, dh), s);
        dx += dw + gap;
        if i == 3 {
            dx += group_gap - gap + gap;
        }
    }
    // Подписи разрядов: HL — адрес, A — данные.
    let sx = (MODULE_W - total) / 2.0;
    let lbl = |text: &str, from: f32, to: f32| {
        p.line_segment(
            [x.p(from, 236.0), x.p(to, 236.0)],
            Stroke::new(x.l(1.5), pal::INK),
        );
        p.text(
            x.p((from + to) / 2.0, 250.0),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(x.l(13.0)),
            pal::INK,
        );
    };
    lbl("H", sx, sx + 2.0 * dw + gap);
    lbl("L", sx + 2.0 * (dw + gap), sx + 4.0 * dw + 3.0 * gap);
    let ax = sx + 4.0 * (dw + gap) + group_gap;
    lbl("A", ax, ax + 2.0 * dw + gap);

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
    p.rect_filled(
        pad_panel,
        CornerRadius::same(x.l(6.0) as u8),
        pal::CASE_DARK,
    );
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
            let h = Hit::Pad(PadKey::Hex(d));
            key(
                p,
                r,
                pal::KEY_DARK,
                &format!("{d:X}"),
                "",
                pressed.is_down(h),
                x.l(24.0),
            );
            hits.zones.push((r, h));
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
    p.rect_filled(panel, CornerRadius::same(x.l(6.0) as u8), pal::CASE_DARK);
    let bw = 2.0 * ks + kg + 32.0;
    let r = x.r(rx, ky, bw, ks * 2.0 + kg);
    let h = Hit::Pad(PadKey::StepBack);
    key(
        p,
        r,
        pal::KEY_GREY,
        "ШАГ\nНАЗАД",
        "",
        pressed.is_down(h),
        x.l(30.0),
    );
    hits.zones.push((r, h));
    let r = x.r(rx, ky + 2.0 * (ks + kg), bw, ks * 2.0 + kg);
    let h = Hit::Pad(PadKey::Reset);
    key(
        p,
        r,
        pal::KEY_LIGHT,
        "УСТ",
        "",
        pressed.is_down(h),
        x.l(24.0),
    );
    hits.zones.push((r, h));

    // Гнездо магнитофона (рис. 1: «МАГНИТОФОН»).
    let jack = x.p(MODULE_W - 46.0, 300.0);
    p.circle_filled(jack, x.l(15.0), pal::CASE_DARK);
    p.circle_filled(jack, x.l(10.0), Color32::from_rgb(0x30, 0x2A, 0x26));
    for i in 0..5 {
        let a = std::f32::consts::PI * (0.75 + i as f32 * 0.375);
        p.circle_filled(
            jack + Vec2::angled(a) * x.l(6.0),
            x.l(1.6),
            Color32::from_gray(160),
        );
    }
    p.text(
        x.p(MODULE_W - 46.0, 326.0),
        Align2::CENTER_CENTER,
        "МАГ",
        FontId::proportional(x.l(11.0)),
        pal::INK,
    );
}

/// Размер телевизора.
pub const TV_W: f32 = 760.0;
/// Высота телевизора.
pub const TV_H: f32 = 560.0;

/// Телевизор-монитор с экраном.
pub fn tv(p: &egui::Painter, x: Xf, screen: Option<&egui::TextureHandle>, glow: Color32) {
    // Корпус светлый, как на фото («Электроника»).
    case(
        p,
        x.r(0.0, 0.0, TV_W, TV_H),
        x.l(30.0),
        Color32::from_rgb(0xE4, 0xE0, 0xD6),
    );
    // Тёмная рамка кинескопа.
    let bezel = x.r(24.0, 24.0, TV_W - 160.0, TV_H - 60.0);
    p.rect_filled(
        bezel,
        CornerRadius::same(x.l(24.0) as u8),
        Color32::from_rgb(0x26, 0x24, 0x22),
    );
    let scr = bezel.shrink(x.l(28.0));
    p.rect_filled(
        scr.expand(x.l(6.0)),
        CornerRadius::same(x.l(30.0) as u8),
        Color32::from_rgb(0x0A, 0x0C, 0x0A),
    );
    if let Some(tex) = screen {
        // Растр 384 × 280 → вписать с сохранением пропорций.
        let k = (scr.width() / 384.0).min(scr.height() / 280.0);
        let img = Rect::from_center_size(scr.center(), vec2(384.0 * k, 280.0 * k));
        p.image(
            tex.id(),
            img,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    // Свечение и блик стекла.
    p.rect_stroke(
        scr,
        CornerRadius::same(x.l(30.0) as u8),
        Stroke::new(
            x.l(8.0),
            Color32::from_rgba_unmultiplied(glow.r(), glow.g(), glow.b(), 18),
        ),
        StrokeKind::Inside,
    );
    p.rect_filled(
        Rect::from_min_size(
            scr.min + vec2(x.l(20.0), x.l(10.0)),
            vec2(scr.width() * 0.4, x.l(14.0)),
        ),
        CornerRadius::same(x.l(7.0) as u8),
        Color32::from_white_alpha(10),
    );
    // Панель ручек справа.
    let px = TV_W - 118.0;
    for (i, label) in ["ЯРК", "КОНТР", "ГРОМК"].iter().enumerate() {
        let c = x.p(px + 46.0, 90.0 + i as f32 * 110.0);
        p.circle_filled(
            c + vec2(x.l(2.0), x.l(3.0)),
            x.l(26.0),
            Color32::from_black_alpha(70),
        );
        p.circle_filled(c, x.l(26.0), Color32::from_rgb(0x3A, 0x36, 0x32));
        p.circle_filled(c, x.l(19.0), Color32::from_rgb(0x52, 0x4C, 0x46));
        p.line_segment(
            [c, c + vec2(0.0, -x.l(17.0))],
            Stroke::new(x.l(3.0), Color32::from_gray(200)),
        );
        p.text(
            c + vec2(0.0, x.l(40.0)),
            Align2::CENTER_CENTER,
            *label,
            FontId::proportional(x.l(13.0)),
            pal::INK,
        );
    }
    // Индикатор питания.
    p.circle_filled(
        x.p(px + 46.0, TV_H - 70.0),
        x.l(6.0),
        Color32::from_rgb(0x40, 0xE0, 0x40),
    );
    p.text(
        x.p(px + 46.0, TV_H - 48.0),
        Align2::CENTER_CENTER,
        "СЕТЬ",
        FontId::proportional(x.l(12.0)),
        pal::INK,
    );
    // Решётка динамика внизу.
    for i in 0..10 {
        let yy = TV_H - 26.0;
        let xx = 60.0 + i as f32 * 22.0;
        p.line_segment(
            [x.p(xx, yy), x.p(xx + 12.0, yy)],
            Stroke::new(x.l(3.0), Color32::from_gray(150)),
        );
    }
}

/// Ширина клавиатуры.
pub const KB_W: f32 = 1300.0;
/// Высота клавиатуры.
pub const KB_H: f32 = 420.0;

/// Ряды клавиатуры (рис. 2, 1989-05): `(сдвиг ряда в клавишах, клавиши)`; клавиша —
/// код основного символа; 0 — модификатор из [`ROW_MODS`].
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
pub fn keyboard(p: &egui::Painter, x: Xf, pressed: &dyn Pressed, hits: &mut Hits) {
    case(p, x.r(0.0, 0.0, KB_W, KB_H), x.l(28.0), pal::CASE);
    let u = 72.0; // шаг клавиш
    let ks = 64.0; // размер клавиши
    let x0 = 40.0;
    let y0 = 36.0;
    let font = x.l(22.0);
    let mut put = |p: &egui::Painter,
                   xx: f32,
                   yy: f32,
                   w: f32,
                   h: Hit,
                   color: Color32,
                   main: String,
                   top: String| {
        let r = x.r(xx, yy, w, ks);
        key(p, r, color, &main, &top, pressed.is_down(h), font);
        hits.zones.push((r, h));
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
                p,
                xx,
                yy,
                ks * 1.4,
                Hit::Kb(m),
                pal::KEY_GREY,
                keymap::kb_label(m),
                String::new(),
            );
            xx += u * 1.4 + 2.0;
        }
        for &c in codes.iter() {
            let (k, _) = KbKey::for_code(c).expect("клавиша раскладки");
            let main = if c == b'0' {
                "0".into()
            } else {
                keymap::code_label(c)
            };
            put(p, xx, yy, ks, Hit::Kb(k), pal::KEY_DARK, main, top_label(c));
            xx += u;
        }
        match ri {
            0 => put(
                p,
                xx,
                yy,
                ks * 1.4,
                Hit::Kb(KbKey::Rus),
                pal::KEY_GREY,
                "РУС".into(),
                String::new(),
            ),
            1 => {
                let (k, _) = KbKey::for_code(0x0D).expect("ВК");
                put(
                    p,
                    xx,
                    yy,
                    ks * 1.3,
                    Hit::Kb(k),
                    pal::KEY_GREY,
                    "ВК".into(),
                    String::new(),
                );
            }
            _ => {}
        }
    }
    // Пробел.
    let (sp, _) = KbKey::for_code(b' ').expect("пробел");
    let yy = y0 + 4.0 * u;
    put(
        p,
        x0 + 2.4 * u,
        yy,
        7.6 * u,
        Hit::Kb(sp),
        pal::KEY_LIGHT,
        "ПРОБЕЛ".into(),
        String::new(),
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
    for (dx, dy, c) in cursor {
        let (k, _) = KbKey::for_code(c).expect("клавиша курсора");
        put(
            p,
            cx + dx * u,
            y0 + dy * u,
            ks,
            Hit::Kb(k),
            pal::KEY_GREY,
            keymap::code_label(c),
            String::new(),
        );
    }
}
