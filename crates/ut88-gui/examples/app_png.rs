//! Кадры окна эмулятора в PNG — проверка вида без окна ОС: кадр [`App::frame`] строится
//! стендом `ui::testing::Harness` в памяти.
//!
//! Запуск: `cargo run --release -p ut88-gui --example app_png -- ПАПКА [МАСШТАБ]`.
//! В папку пишутся:
//! - `app-minimal.png` — минимальная конфигурация, 720 × 680;
//! - `app-full.png` — полная, 1280 × 900 (машина поработала 1,5 с — на экране МОНИТОР-F);
//! - `app-full-menu.png` — то же с открытым меню «Машина»;
//! - `app-full-windows.png` — окна «Магнитофон и файлы» (F8) и «Справка: клавиши» (F11);
//! - `app-programs.png` — окно «Программы из журналов» (F9) в минимальной конфигурации;
//! - `app-debugger.png` — окно отладчика, 820 × 760.

use std::path::{Path, PathBuf};

use ut88_core::{Config, Machine};
use ut88_gui::app::App;
use ut88_gui::gfx::{Vec2, vec2};
use ut88_gui::ui::KeyCode;
use ut88_gui::ui::testing::Harness;
use ut88_gui::{GuiOptions, Speed};

/// Приложение без звука и без папки настроек.
fn app(config: Config, run_secs: f64) -> App {
    let mut m = Machine::new(config);
    m.run_seconds(run_secs);
    App::new(GuiOptions {
        machine: m,
        speed: Speed::X1,
        sound: false,
        programs_dir: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../programs")),
        kvazidisk_file: None,
        settings_dir: None,
    })
}

/// Стенд главного окна.
fn main_window(config: Config, run_secs: f64, size: Vec2, ppp: f32) -> Harness<App> {
    Harness::with_ppp(size, ppp, app(config, run_secs), |ctx, app: &mut App| {
        app.frame(ctx)
    })
}

/// Записать последний кадр стенда.
fn save(h: &Harness<App>, dir: &Path, name: &str) -> anyhow::Result<()> {
    let p = dir.join(name);
    h.ctx.with_canvas(|c| c.pixmap().save_png(&p))?;
    println!("{}", p.display());
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let ppp: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    std::fs::create_dir_all(&dir)?;

    let mut h = main_window(Config::minimal(), 0.5, vec2(720.0, 680.0), ppp);
    h.settle();
    save(&h, &dir, "app-minimal.png")?;

    h.key(KeyCode::F9);
    h.settle();
    save(&h, &dir, "app-programs.png")?;

    let mut h = main_window(Config::full(), 1.5, vec2(1280.0, 900.0), ppp);
    h.settle();
    save(&h, &dir, "app-full.png")?;

    h.click("Машина");
    save(&h, &dir, "app-full-menu.png")?;
    h.key(KeyCode::Escape);

    h.key(KeyCode::F8);
    h.key(KeyCode::F11);
    h.settle();
    save(&h, &dir, "app-full-windows.png")?;

    let mut d = Harness::with_ppp(
        vec2(820.0, 760.0),
        ppp,
        app(Config::full(), 1.5),
        |ctx, app: &mut App| app.debugger_frame(ctx),
    );
    d.settle();
    save(&d, &dir, "app-debugger.png")?;
    Ok(())
}
