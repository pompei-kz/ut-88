//! Замер скорости программной отрисовки кадра 1280 × 900 (модуль `gfx`).
//!
//! Запуск: `cargo run --release -p ut88-gui --example gfx_bench`.
//! Окно не открывается: кадр рисуется в памяти.

use std::time::{Duration, Instant};

use ut88_gui::gfx::{Align2, Canvas, Color, FontKind, Rect, pos2, vec2};

const W: u32 = 1280;
const H: u32 = 900;

/// ~200 фигур: скруглённые прямоугольники, рамки, круги, линии, многоугольники.
fn shapes(c: &mut Canvas) {
    for i in 0..40 {
        let x = 20.0 + (i % 10) as f32 * 120.0;
        let y = 20.0 + (i / 10) as f32 * 90.0;
        let r = Rect::from_min_size(pos2(x, y), vec2(100.0, 70.0));
        c.fill_rect(r, 6.0, Color::rgb(0xD9, 0xD0, 0xB8));
        c.stroke_rect(r, 6.0, 1.0, Color::rgb(0xA8, 0x9E, 0x86));
        c.fill_circle(r.center(), 12.0, Color::from_black_alpha(80));
        c.line(
            r.left_top(),
            r.right_bottom(),
            2.0,
            Color::rgb(0x4A, 0x3C, 0x30),
        );
        c.fill_polygon(
            &[
                pos2(x + 10.0, y + 10.0),
                pos2(x + 40.0, y + 6.0),
                pos2(x + 44.0, y + 14.0),
                pos2(x + 14.0, y + 18.0),
            ],
            Color::rgb(0xFF, 0x30, 0x18),
        );
    }
}

/// ~150 строк текста.
fn texts(c: &mut Canvas) {
    for i in 0..150 {
        let x = 20.0 + (i % 3) as f32 * 420.0;
        let y = 380.0 + (i / 3) as f32 * 10.0;
        let font = if i % 2 == 0 {
            FontKind::Sans
        } else {
            FontKind::Mono
        };
        c.text(
            pos2(x, y),
            Align2::LEFT_TOP,
            "Магнитофон: ▶ лента 12.5 с — запись",
            13.0,
            font,
            Color::LIGHT_GRAY,
        );
    }
}

fn main() {
    let mut canvas = Canvas::new(W, H);
    let mut out = vec![0u32; (W * H) as usize];
    let screen: Vec<u8> = (0..384 * 256)
        .flat_map(|i| {
            let on = (i / 7 + i / 384) % 3 == 0;
            if on {
                [0x40, 0xE0, 0x40, 0xFF]
            } else {
                [0x08, 0x10, 0x08, 0xFF]
            }
        })
        .collect();
    // Кэш-слой рисунка компьютера (во весь кадр, как будет в окне).
    let mut layer = Canvas::new(W, H);
    layer.clear(Color::rgb(0x2C, 0x2A, 0x2E));
    shapes(&mut layer);

    let mut t = [Duration::ZERO; 7];
    let frames = 200;
    for frame in 0..frames + 10 {
        let mut lap = Instant::now();
        let mut step = |i: usize, t: &mut [Duration; 7]| {
            let now = Instant::now();
            if frame >= 10 {
                t[i] += now - lap;
            }
            lap = now;
        };
        canvas.clear(Color::rgb(0x2C, 0x2A, 0x2E));
        step(0, &mut t);
        shapes(&mut canvas);
        step(1, &mut t);
        texts(&mut canvas);
        step(2, &mut t);
        canvas.image(
            Rect::from_min_size(pos2(240.0, 60.0), vec2(800.0, 540.0)),
            384,
            256,
            &screen,
            true,
        );
        step(3, &mut t);
        canvas.push_clip(Rect::from_min_size(pos2(100.0, 100.0), vec2(600.0, 400.0)));
        canvas.fill_circle(pos2(100.0, 100.0), 50.0, Color::YELLOW);
        canvas.pop_clip();
        step(4, &mut t);
        canvas.draw_canvas(&layer, pos2(0.0, 0.0));
        step(5, &mut t);
        canvas.to_0rgb(&mut out);
        step(6, &mut t);
    }
    let names = [
        "заливка кадра",
        "200 фигур",
        "150 строк текста",
        "экран 384×256 → 800×540 (билинейно)",
        "круг с маской отсечения",
        "наложение слоя 1280×900",
        "перевод в 0RGB",
    ];
    let ms = |d: Duration| d.as_secs_f64() * 1000.0 / frames as f64;
    for (n, d) in names.iter().zip(t) {
        println!("{n:<40} {:7.3} мс", ms(d));
    }
    let total: Duration = t.iter().sum();
    let without_layer = total - t[5];
    println!("{:<40} {:7.3} мс", "кадр без слоя", ms(without_layer));
    println!("{:<40} {:7.3} мс", "кадр со слоем", ms(total));
    println!(
        "(контрольная сумма {})",
        out.iter().fold(0u32, |a, &p| a.wrapping_add(p))
    );
}
