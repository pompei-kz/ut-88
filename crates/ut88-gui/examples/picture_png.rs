//! Рисунок компьютера (модуль `picture`) в PNG-файлы и замер времени кадра.
//!
//! Запуск: `cargo run --release -p ut88-gui --example picture_png -- ПАПКА [МАСШТАБ]`.
//! В папку пишутся `picture-minimal.png` (минимальная конфигурация, 720 × 680) и
//! `picture-full.png` (полная, 1280 × 900, экран — с настоящей машины после 1,5 с
//! эмуляции). Окно не открывается.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ut88_core::{Config, Machine, screen};
use ut88_gui::gfx::{Canvas, Color, Pos2};
use ut88_gui::picture::{self, Hit, Hits, Layout, Pressed};

/// Цвета зелёного люминофора (как `Phosphor::Green` в окне).
const FG: [u8; 4] = [0x66, 0xFF, 0x88, 0xFF];
const BG: [u8; 4] = [0x06, 0x14, 0x08, 0xFF];

/// Для наглядности нажата клавиша «5» модуля.
struct Five;

impl Pressed for Five {
    fn is_down(&self, h: Hit) -> bool {
        h == Hit::Pad(ut88_core::PadKey::Hex(5))
    }
}

/// Холст `w × h` логических точек с масштабом `ppp`.
fn canvas(w: f32, h: f32, ppp: f32) -> Canvas {
    let mut c = Canvas::new((w * ppp).round() as u32, (h * ppp).round() as u32);
    c.set_scale(ppp);
    c
}

/// Среднее время `n` повторов `f`.
fn time(n: u32, mut f: impl FnMut()) -> Duration {
    f();
    let t = Instant::now();
    for _ in 0..n {
        f();
    }
    t.elapsed() / n
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let ppp: f32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(1.0);
    let glow = Color::from_rgb(FG[0], FG[1], FG[2]);

    // Минимальная конфигурация.
    let mut m = Machine::new(Config::minimal());
    m.run_seconds(1.0);
    let mut c = canvas(720.0, 680.0, ppp);
    let rect = c.rect_points();
    picture::computer(
        &mut c,
        rect,
        false,
        m.indicator_segments(),
        None,
        glow,
        &Five,
        &mut Hits::default(),
    );
    let p = dir.join("picture-minimal.png");
    c.pixmap().save_png(&p)?;
    println!("{}", p.display());

    // Полная конфигурация: экран и индикаторы с настоящей машины.
    let mut m = Machine::new(Config::full());
    m.run_seconds(1.5);
    let mut buf = vec![0u8; screen::WIDTH * screen::HEIGHT * 4];
    let has = m.render_screen(&mut buf, FG, BG);
    assert!(has, "в полной конфигурации есть дисплей");
    let scr = Some((&buf[..], screen::WIDTH as u32, screen::HEIGHT as u32));
    let segs = m.indicator_segments();
    let mut c = canvas(1280.0, 900.0, ppp);
    let rect = c.rect_points();
    let mut hits = Hits::default();
    picture::computer(&mut c, rect, true, segs, scr, glow, &Five, &mut hits);
    let p = dir.join("picture-full.png");
    c.pixmap().save_png(&p)?;
    println!("{} (зон нажатия: {})", p.display(), hits.zones.len());

    // Замер: весь рисунок, статическая часть (кэш-слой) и кадр из кэша.
    let n = 100;
    let full = time(n, || {
        picture::computer(
            &mut c,
            rect,
            true,
            segs,
            scr,
            glow,
            &Five,
            &mut Hits::default(),
        );
    });
    let layout = Layout::new(rect, true);
    let mut layer = canvas(1280.0, 900.0, ppp);
    let stat = time(n, || {
        picture::computer_static(&mut layer, &layout, glow, &Five, &mut Hits::default());
    });
    let live = time(n, || {
        c.draw_canvas(&layer, Pos2::ZERO);
        picture::computer_live(&mut c, &layout, segs, scr, glow);
    });
    let minimal = {
        let mut c = canvas(720.0, 680.0, ppp);
        let rect = c.rect_points();
        time(n, || {
            picture::computer(
                &mut c,
                rect,
                false,
                segs,
                None,
                glow,
                &Five,
                &mut Hits::default(),
            );
        })
    };
    // Части статического слоя по отдельности.
    let tv_t = time(n, || {
        picture::tv(&mut layer, layout.tv.unwrap(), None, glow)
    });
    let mod_t = time(n, || {
        picture::cpu_module(
            &mut layer,
            layout.module,
            [0; 6],
            &Five,
            &mut Hits::default(),
        )
    });
    let kb_t = time(n, || {
        picture::keyboard(
            &mut layer,
            layout.keyboard.unwrap(),
            &Five,
            &mut Hits::default(),
        )
    });
    // Перерисовка одной нажатой клавиши в кэш-слое вместо всего слоя.
    let mut hits = Hits::default();
    picture::computer_static(&mut layer, &layout, glow, &Five, &mut hits);
    let mut down = false;
    let key_t = time(n, || {
        down = !down;
        picture::redraw_key(
            &mut layer,
            &layout,
            &hits,
            Hit::Pad(ut88_core::PadKey::Hex(5)),
            down,
        )
        .expect("клавиша перерисовывается");
    });
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!("масштаб {ppp}, среднее по {n} кадрам:");
    println!("  полный рисунок 1280×900:       {:6.2} мс", ms(full));
    println!("  статическая часть (кэш-слой):  {:6.2} мс", ms(stat));
    println!("  кадр из кэша + живая часть:    {:6.2} мс", ms(live));
    println!(
        "    из них телевизор {:.2}, модуль {:.2}, клавиатура {:.2} мс",
        ms(tv_t),
        ms(mod_t),
        ms(kb_t)
    );
    println!("  перерисовка одной клавиши:     {:6.3} мс", ms(key_t));
    println!("  минимальный рисунок 720×680:   {:6.2} мс", ms(minimal));
    Ok(())
}
