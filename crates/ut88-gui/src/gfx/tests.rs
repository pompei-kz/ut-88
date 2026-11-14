//! Тесты графики: пиксели фигур, отсечение, масштаб, текст, картинки, перевод кадра.

use super::*;

const BG: Color = Color::rgb(10, 20, 30);
const RED: Color = Color::rgb(255, 0, 0);

fn canvas(w: u32, h: u32) -> Canvas {
    let mut c = Canvas::new(w, h);
    c.clear(BG);
    c
}

/// Пиксель закрашен цветом `color` (с допуском на сглаживание).
fn is(c: &Canvas, x: u32, y: u32, color: Color) -> bool {
    let p = c.pixel(x, y);
    let d = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 2;
    d(p.r, color.r) && d(p.g, color.g) && d(p.b, color.b) && d(p.a, color.a)
}

/// Границы закрашенных (отличных от фона) пикселей: `(x0, y0, x1, y1)`, x1/y1 — не включая.
fn painted_bounds(c: &Canvas) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for y in 0..c.height() {
        for x in 0..c.width() {
            if c.pixel(x, y) != BG {
                b = Some(match b {
                    None => (x, y, x + 1, y + 1),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
                });
            }
        }
    }
    b
}

#[test]
fn fill_rect_paints_inside_only() {
    let mut c = canvas(40, 30);
    c.fill_rect(
        Rect::from_min_max(pos2(10.0, 5.0), pos2(20.0, 15.0)),
        0.0,
        RED,
    );
    assert!(is(&c, 10, 5, RED));
    assert!(is(&c, 19, 14, RED));
    assert!(is(&c, 15, 10, RED));
    assert!(is(&c, 9, 10, BG));
    assert!(is(&c, 20, 10, BG));
    assert!(is(&c, 15, 4, BG));
    assert!(is(&c, 15, 15, BG));
    assert_eq!(painted_bounds(&c), Some((10, 5, 20, 15)));
}

#[test]
fn fill_rect_half_pixel_is_antialiased() {
    let mut c = canvas(10, 10);
    c.fill_rect(
        Rect::from_min_max(pos2(2.5, 2.0), pos2(6.0, 6.0)),
        0.0,
        Color::WHITE,
    );
    let edge = c.pixel(2, 3);
    assert!(edge.r > 100 && edge.r < 160, "край: {edge:?}");
    assert!(is(&c, 3, 3, Color::WHITE));
}

#[test]
fn rounded_rect_cuts_corners() {
    let mut c = canvas(40, 40);
    c.fill_rect(
        Rect::from_min_max(pos2(5.0, 5.0), pos2(35.0, 35.0)),
        10.0,
        RED,
    );
    assert!(is(&c, 5, 5, BG), "угол не закрашен");
    assert!(is(&c, 34, 34, BG));
    assert!(is(&c, 20, 5, RED), "середина стороны закрашена");
    assert!(is(&c, 5, 20, RED));
    assert!(is(&c, 20, 20, RED));
}

#[test]
fn stroke_rect_paints_border_not_interior() {
    let mut c = canvas(40, 40);
    c.stroke_rect(
        Rect::from_min_max(pos2(10.0, 10.0), pos2(30.0, 30.0)),
        0.0,
        2.0,
        RED,
    );
    assert!(is(&c, 9, 20, RED) && is(&c, 10, 20, RED));
    assert!(is(&c, 20, 29, RED) && is(&c, 20, 30, RED));
    assert!(is(&c, 20, 20, BG));
    assert!(is(&c, 12, 20, BG));
    assert!(is(&c, 7, 20, BG));
    assert_eq!(painted_bounds(&c), Some((9, 9, 31, 31)));
}

#[test]
fn circles() {
    let mut c = canvas(40, 40);
    c.fill_circle(pos2(20.0, 20.0), 8.0, RED);
    assert!(is(&c, 20, 20, RED));
    assert!(is(&c, 13, 20, RED));
    assert!(is(&c, 13, 13, BG), "угол описанного квадрата");
    assert!(is(&c, 29, 20, BG));
    assert_eq!(painted_bounds(&c), Some((12, 12, 28, 28)));

    let mut c = canvas(40, 40);
    c.stroke_circle(pos2(20.0, 20.0), 10.0, 2.0, RED);
    assert!(is(&c, 20, 20, BG), "середина пустая");
    assert!(is(&c, 10, 20, RED) || is(&c, 9, 20, RED));
    assert!(is(&c, 20, 30, RED) || is(&c, 20, 29, RED));
}

#[test]
fn line_paints_segment() {
    let mut c = canvas(40, 20);
    c.line(pos2(5.0, 10.0), pos2(35.0, 10.0), 2.0, RED);
    assert!(is(&c, 5, 9, RED) && is(&c, 34, 10, RED));
    assert!(is(&c, 4, 10, BG) && is(&c, 35, 10, BG), "концы плоские");
    assert!(is(&c, 20, 8, BG) && is(&c, 20, 11, BG));
    assert_eq!(painted_bounds(&c), Some((5, 9, 35, 11)));
}

#[test]
fn polygons_convex_and_concave() {
    // Треугольник.
    let mut c = canvas(40, 40);
    c.fill_polygon(&[pos2(5.0, 5.0), pos2(35.0, 5.0), pos2(5.0, 35.0)], RED);
    assert!(is(&c, 8, 8, RED));
    assert!(is(&c, 30, 30, BG));
    // Буква «Г» (невыпуклый многоугольник): вырез не закрашен.
    let mut c = canvas(40, 40);
    c.fill_polygon(
        &[
            pos2(5.0, 5.0),
            pos2(35.0, 5.0),
            pos2(35.0, 15.0),
            pos2(15.0, 15.0),
            pos2(15.0, 35.0),
            pos2(5.0, 35.0),
        ],
        RED,
    );
    assert!(is(&c, 30, 10, RED) && is(&c, 10, 30, RED));
    assert!(is(&c, 25, 25, BG), "вырез");
    // Контур многоугольника.
    let mut c = canvas(40, 40);
    c.stroke_polygon(
        &[
            pos2(5.0, 5.0),
            pos2(35.0, 5.0),
            pos2(35.0, 35.0),
            pos2(5.0, 35.0),
        ],
        2.0,
        RED,
    );
    assert!(is(&c, 20, 5, RED) && is(&c, 5, 20, RED));
    assert!(is(&c, 20, 20, BG));
}

#[test]
fn alpha_blending_is_premultiplied() {
    let mut c = Canvas::new(4, 4);
    c.clear(Color::BLACK);
    c.fill_rect(c.rect_points(), 0.0, Color::from_white_alpha(128));
    let p = c.pixel(1, 1);
    assert!((p.r as i32 - 128).abs() <= 1, "{p:?}");
    assert_eq!(p.a, 255);
    // Прозрачный холст: цвет хранится с умножением, `pixel` его снимает.
    let mut c = Canvas::new(4, 4);
    c.fill_rect(c.rect_points(), 0.0, Color::rgba(200, 100, 50, 128));
    let p = c.pixel(1, 1);
    assert_eq!(p.a, 128);
    assert!((p.r as i32 - 200).abs() <= 2 && (p.g as i32 - 100).abs() <= 2);
}

#[test]
fn clip_limits_all_operations() {
    let clip = Rect::from_min_max(pos2(10.0, 10.0), pos2(20.0, 20.0));
    // Прямоугольник (геометрическое отсечение).
    let mut c = canvas(40, 40);
    c.push_clip(clip);
    c.fill_rect(c.rect_points(), 0.0, RED);
    c.pop_clip();
    assert_eq!(painted_bounds(&c), Some((10, 10, 20, 20)));
    // Круг (через маску).
    let mut c = canvas(40, 40);
    c.push_clip(clip);
    c.fill_circle(pos2(10.0, 10.0), 8.0, RED);
    c.pop_clip();
    assert!(is(&c, 12, 12, RED));
    assert_eq!(painted_bounds(&c), Some((10, 10, 18, 18)));
    // Линия, текст, картинка, слой.
    let mut c = canvas(40, 40);
    c.push_clip(clip);
    c.line(pos2(0.0, 15.0), pos2(40.0, 15.0), 3.0, RED);
    c.text(
        pos2(0.0, 0.0),
        Align2::LEFT_TOP,
        "ЖЖЖЖЖЖ",
        30.0,
        FontKind::Sans,
        RED,
    );
    c.image(c.rect_points(), 1, 1, &[0, 255, 0, 255], true);
    let mut layer = Canvas::new(40, 40);
    layer.clear(Color::WHITE);
    c.draw_canvas(&layer, pos2(0.0, 0.0));
    c.pop_clip();
    assert_eq!(painted_bounds(&c), Some((10, 10, 20, 20)));
    assert_eq!(c.clip_depth(), 0);
}

#[test]
fn nested_clip_intersects_and_pops() {
    let mut c = canvas(40, 40);
    c.push_clip(Rect::from_min_max(pos2(0.0, 0.0), pos2(20.0, 20.0)));
    c.push_clip(Rect::from_min_max(pos2(10.0, 10.0), pos2(40.0, 40.0)));
    assert_eq!(
        c.clip_rect(),
        Rect::from_min_max(pos2(10.0, 10.0), pos2(20.0, 20.0))
    );
    c.fill_circle(pos2(20.0, 20.0), 30.0, RED);
    assert_eq!(painted_bounds(&c), Some((10, 10, 20, 20)));
    c.pop_clip();
    c.fill_rect(Rect::from_min_max(pos2(0.0, 0.0), pos2(5.0, 5.0)), 1.0, RED);
    assert!(is(&c, 2, 2, RED));
    c.pop_clip();
    c.fill_rect(
        Rect::from_min_max(pos2(30.0, 0.0), pos2(40.0, 5.0)),
        0.0,
        RED,
    );
    assert!(is(&c, 35, 2, RED));
}

#[test]
fn scale_two_doubles() {
    let mut c = canvas(40, 40);
    c.set_scale(2.0);
    assert_eq!(c.size_points(), vec2(20.0, 20.0));
    c.fill_rect(Rect::from_min_max(pos2(2.0, 3.0), pos2(7.0, 8.0)), 0.0, RED);
    assert_eq!(painted_bounds(&c), Some((4, 6, 14, 16)));

    let mut c = canvas(40, 40);
    c.set_scale(2.0);
    c.push_clip(Rect::from_min_max(pos2(5.0, 5.0), pos2(10.0, 10.0)));
    c.fill_circle(pos2(10.0, 10.0), 10.0, RED);
    assert_eq!(painted_bounds(&c), Some((10, 10, 20, 20)));

    // Текст при масштабе 2 вдвое больше в физических пикселях.
    let m1 = measure_text_scaled("Магнитофон", 14.0, FontKind::Sans, 1.0);
    let m2 = measure_text_scaled("Магнитофон", 14.0, FontKind::Sans, 2.0);
    assert!(
        (m1.x - m2.x).abs() < 0.5 && (m1.y - m2.y).abs() < 0.1,
        "{m1:?} {m2:?}"
    );
}

#[test]
fn text_paints_inside_measured_rect() {
    for font in [FontKind::Sans, FontKind::Mono] {
        let mut c = canvas(300, 60);
        let s = "Пауза «ЮТ-88» — 12:34…";
        let size = c.measure_text(s, 16.0, font);
        let r = c.text(
            pos2(10.0, 10.0),
            Align2::LEFT_TOP,
            s,
            16.0,
            font,
            Color::WHITE,
        );
        assert_eq!(r.size(), size);
        assert_eq!(r.min, pos2(10.0, 10.0));
        // Высота строки DejaVu ≈ 1,16 em.
        assert!((size.y - 16.0 * 1.164).abs() < 0.2, "{size:?}");
        let (x0, y0, x1, y1) = painted_bounds(&c).expect("текст не нарисован");
        // Закрашенное — внутри прямоугольника строки (с допуском на выносные элементы).
        assert!(
            x0 as f32 >= r.min.x - 1.0 && x1 as f32 <= r.max.x + 2.0,
            "{x0}..{x1} {r:?}"
        );
        assert!(
            y0 as f32 >= r.min.y - 1.0 && y1 as f32 <= r.max.y + 1.0,
            "{y0}..{y1} {r:?}"
        );
        // И занимает почти всю ширину строки.
        assert!((x1 - x0) as f32 > size.x - 4.0, "{x0}..{x1} {size:?}");
        // Есть полностью закрашенные пиксели и немало пикселей вообще.
        let mut full = 0;
        let mut any = 0;
        for y in 0..c.height() {
            for x in 0..c.width() {
                let p = c.pixel(x, y);
                if p != BG {
                    any += 1;
                }
                if is(&c, x, y, Color::WHITE) {
                    full += 1;
                }
            }
        }
        assert!(full > 50 && any > 200, "{full} {any}");
    }
}

#[test]
fn text_alignment() {
    let mut c = canvas(200, 100);
    let r = c.text(
        pos2(100.0, 50.0),
        Align2::CENTER_CENTER,
        "Сброс",
        14.0,
        FontKind::Sans,
        Color::WHITE,
    );
    assert!((r.center().x - 100.0).abs() < 0.01 && (r.center().y - 50.0).abs() < 0.01);
    let r = c.text(
        pos2(190.0, 90.0),
        Align2::RIGHT_BOTTOM,
        "Сброс",
        14.0,
        FontKind::Sans,
        Color::WHITE,
    );
    assert_eq!(r.max, pos2(190.0, 90.0));
}

#[test]
fn mono_font_is_monospaced() {
    let a = measure_text_scaled("iiiiiiiiii", 14.0, FontKind::Mono, 1.0);
    let b = measure_text_scaled("WWWWWWWWWW", 14.0, FontKind::Mono, 1.0);
    assert!((a.x - b.x).abs() < 0.01);
    let a = measure_text_scaled("iiiiiiiiii", 14.0, FontKind::Sans, 1.0);
    assert!(a.x < b.x * 0.6);
    assert_eq!(measure_text_scaled("", 14.0, FontKind::Sans, 1.0).x, 0.0);
}

#[test]
fn glyph_coverage() {
    let common = "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя\
                  «»—–…№°×←→↑↓↖ ±0123456789";
    assert_eq!(missing_glyphs(common, FontKind::Sans), Vec::<char>::new());
    assert_eq!(missing_glyphs(common, FontKind::Mono), Vec::<char>::new());
    assert_eq!(
        missing_glyphs("a\u{E000}b\u{E000}\n", FontKind::Sans),
        vec!['\u{E000}']
    );
}

#[test]
fn image_scaling() {
    // 2 × 2: красный, зелёный / синий, белый.
    let src = [
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let mut c = canvas(20, 20);
    c.image(c.rect_points(), 2, 2, &src, false);
    assert!(is(&c, 0, 0, RED) && is(&c, 9, 9, RED));
    assert!(is(&c, 10, 0, Color::rgb(0, 255, 0)) && is(&c, 19, 9, Color::rgb(0, 255, 0)));
    assert!(is(&c, 0, 10, Color::rgb(0, 0, 255)));
    assert!(is(&c, 19, 19, Color::WHITE));
    // Билинейно: на стыке красного и зелёного — смесь.
    let mut c = canvas(20, 20);
    c.image(c.rect_points(), 2, 2, &src, true);
    assert!(is(&c, 0, 0, RED));
    let mid = c.pixel(10, 2);
    assert!(mid.r > 60 && mid.g > 60, "{mid:?}");
    // В часть холста при масштабе 2.
    let mut c = canvas(20, 20);
    c.set_scale(2.0);
    c.image(
        Rect::from_min_max(pos2(1.0, 1.0), pos2(5.0, 5.0)),
        2,
        2,
        &src,
        false,
    );
    assert_eq!(painted_bounds(&c), Some((2, 2, 10, 10)));
}

#[test]
fn draw_canvas_composites_layer() {
    let mut layer = Canvas::new(10, 10);
    layer.fill_rect(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(5.0, 10.0)),
        0.0,
        RED,
    );
    layer.fill_rect(
        Rect::from_min_max(pos2(5.0, 0.0), pos2(10.0, 10.0)),
        0.0,
        Color::from_white_alpha(128),
    );
    let mut c = Canvas::new(30, 30);
    c.clear(Color::BLACK);
    c.draw_canvas(&layer, pos2(10.0, 10.0));
    assert!(is(&c, 10, 10, RED) && is(&c, 14, 19, RED));
    let p = c.pixel(17, 15);
    assert!((p.r as i32 - 128).abs() <= 1 && p.a == 255, "{p:?}");
    assert!(is(&c, 9, 10, Color::BLACK) && is(&c, 20, 10, Color::BLACK));
    assert!(is(&c, 12, 20, Color::BLACK));
    // Слой, выходящий за край, не ломает ничего.
    c.draw_canvas(&layer, pos2(25.0, -5.0));
    assert!(is(&c, 27, 0, RED));
}

#[test]
fn to_0rgb_values() {
    let mut c = Canvas::new(3, 2);
    c.clear(Color::rgb(0x12, 0x34, 0x56));
    c.fill_rect(
        Rect::from_min_max(pos2(1.0, 0.0), pos2(2.0, 1.0)),
        0.0,
        Color::rgb(0xFF, 0x80, 0x01),
    );
    let mut out = vec![0xFFFF_FFFFu32; 6];
    c.to_0rgb(&mut out);
    assert_eq!(
        out,
        vec![0x123456, 0xFF8001, 0x123456, 0x123456, 0x123456, 0x123456]
    );
    // Полупрозрачный пиксель на прозрачном холсте — как на чёрном фоне.
    let mut c = Canvas::new(1, 1);
    c.fill_rect(c.rect_points(), 0.0, Color::from_white_alpha(128));
    let mut out = [0u32; 1];
    c.to_0rgb(&mut out);
    assert_eq!(out[0], 0x808080);
}

#[test]
fn resize_and_degenerate_input() {
    let mut c = Canvas::new(0, 0);
    assert_eq!((c.width(), c.height()), (1, 1));
    c.push_clip(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    c.resize(50, 40);
    assert_eq!((c.width(), c.height(), c.clip_depth()), (50, 40, 0));
    // Вырожденные фигуры не паникуют.
    c.fill_polygon(&[pos2(1.0, 1.0), pos2(2.0, 2.0)], RED);
    c.fill_rect(Rect::NOTHING, 3.0, RED);
    c.fill_circle(pos2(5.0, 5.0), 0.0, RED);
    c.line(pos2(5.0, 5.0), pos2(5.0, 5.0), 1.0, RED);
    c.stroke_rect(
        Rect::from_min_max(pos2(1.0, 1.0), pos2(1.0, 1.0)),
        0.0,
        1.0,
        RED,
    );
    c.text(
        pos2(0.0, 0.0),
        Align2::LEFT_TOP,
        "x",
        0.0,
        FontKind::Sans,
        RED,
    );
    c.image(c.rect_points(), 4, 4, &[0; 3], true);
    c.push_clip(Rect::NOTHING);
    c.fill_circle(pos2(5.0, 5.0), 3.0, RED);
    c.pop_clip();
}

#[test]
fn geometry_helpers() {
    let r = Rect::from_center_size(pos2(10.0, 20.0), vec2(4.0, 6.0));
    assert_eq!(r.min, pos2(8.0, 17.0));
    assert_eq!(r.shrink(1.0).size(), vec2(2.0, 4.0));
    assert_eq!(r.expand(1.0).size(), vec2(6.0, 8.0));
    assert!(r.contains(pos2(8.0, 17.0)) && !r.contains(pos2(12.0, 17.0)));
    assert_eq!(r.translate(vec2(1.0, 1.0)).center(), pos2(11.0, 21.0));
    assert!(!r.intersect(r.translate(vec2(10.0, 0.0))).is_positive());
    assert_eq!(Rect::NOTHING.union(r), r);
    assert_eq!(pos2(3.0, 4.0) - Pos2::ZERO, vec2(3.0, 4.0));
    assert_eq!(vec2(3.0, 4.0).length(), 5.0);
    assert_eq!(2.0 * vec2(1.0, 2.0), vec2(2.0, 4.0));
    let a = Vec2::angled(std::f32::consts::FRAC_PI_2);
    assert!(a.x.abs() < 1e-6 && (a.y - 1.0).abs() < 1e-6);
    assert_eq!(
        Align2::RIGHT_BOTTOM
            .anchor_size(pos2(10.0, 10.0), vec2(4.0, 2.0))
            .min,
        pos2(6.0, 8.0)
    );
}

/// Сводка: какие символы подписей нынешнего интерфейса отсутствуют в шрифтах.
/// Запуск: `cargo test -p ut88-gui gfx::tests::ui_strings_report -- --ignored --nocapture`.
#[test]
#[ignore]
fn ui_strings_report() {
    let sources = [
        include_str!("../app.rs"),
        include_str!("../debugger.rs"),
        include_str!("../keymap.rs"),
        include_str!("../draw.rs"),
    ];
    let mut chars = String::new();
    for src in sources {
        // Строковые литералы без учёта экранирования — для сводки этого достаточно.
        for (i, part) in src.split('"').enumerate() {
            if i % 2 == 1 && !part.is_ascii() {
                chars.push_str(part);
            }
        }
    }
    for font in [FontKind::Sans, FontKind::Mono] {
        let miss = missing_glyphs(&chars, font);
        let list: Vec<String> = miss
            .iter()
            .map(|c| format!("{c} U+{:04X}", *c as u32))
            .collect();
        eprintln!("{font:?}: нет {} символов: {}", miss.len(), list.join(", "));
    }
}
