//! Замер режима «максимум» терминального интерфейса с отрисовкой (без терминала: кадры
//! рисуются в `TestBackend` 80 × 32, событий нет).
//!
//! ```text
//! cargo run --release -p ut88-tui --example bench_max [-- СЕКУНДЫ [НАГРУЗКА…]]
//! ```
//!
//! Нагрузки: `full` (МОНИТОР-F ждёт клавишу), `tetris` («Тетрис», programs/tetris.bin с
//! 3000H, уровень 0). Цикл — тот же [`ut88_tui::frame`], что и в настоящем терминале.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ut88_core::{CPU_HZ, Config, KbKey, Machine};
use ut88_tui::{App, Options, Pacer};

fn repo(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn screen_text(m: &Machine) -> String {
    m.screen().map(|s| s.text()).unwrap_or_default()
}

/// Нажать и отпустить клавишу полной клавиатуры с кодом `code`.
fn kb_code(m: &mut Machine, code: u8) {
    let (key, modifier) = KbKey::for_code(code).expect("код клавиши");
    if let Some(md) = modifier {
        m.kb_down(md);
    }
    m.kb_down(key);
    m.run_seconds(0.06);
    m.kb_up(key);
    if let Some(md) = modifier {
        m.kb_up(md);
    }
    m.run_seconds(0.3);
}

/// Держать клавишу, пока программа её не примет (щелчок F803H — перепад выхода
/// магнитофона), затем отпустить.
fn press(m: &mut Machine, code: u8) {
    let (key, _) = KbKey::for_code(code).expect("код клавиши");
    m.set_sound_capture(true);
    m.drain_sound_edges();
    m.kb_down(key);
    let limit = m.cycles() + 3 * CPU_HZ as u64;
    while m.cycles() < limit && m.drain_sound_edges().is_empty() {
        m.run_seconds(0.005);
    }
    m.kb_up(key);
    m.set_sound_capture(false);
    m.run_seconds(0.3);
}

fn wait_text(m: &mut Machine, text: &str, secs: f64) {
    let limit = m.cycles() + (secs * CPU_HZ) as u64;
    while !screen_text(m).contains(text) {
        assert!(m.cycles() < limit, "на экране нет «{text}»");
        m.run_seconds(0.05);
    }
}

fn full() -> Machine {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    m
}

fn tetris() -> Machine {
    let mut m = full();
    m.load_bin_file(repo("programs/tetris.bin"), 0x3000)
        .expect("programs/tetris.bin");
    for b in b"G3000\r" {
        kb_code(&mut m, *b);
    }
    wait_text(&mut m, "ВАШ УРОВЕНЬ(0-7) ?", 3.0);
    press(&mut m, b'0');
    wait_text(&mut m, "НАЖМИТЕ ЛЮБУЮ КЛАВИШУ", 3.0);
    press(&mut m, b' ');
    m.run_seconds(0.5);
    m
}

/// Прогнать главный цикл `secs` секунд на «максимуме»: (МГц, кадров терминала в секунду).
fn measure(m: Machine, secs: f64) -> (f64, f64) {
    let opts = Options {
        speed: 0.0,
        ..Options::default()
    };
    let mut app = App::new(m, opts);
    let mut term = Terminal::new(TestBackend::new(80, 32)).expect("TestBackend");
    let mut pacer = Pacer::new(&app);
    // Событий нет: ожидание события — просто сон.
    let mut events = |left: Duration| {
        std::thread::sleep(left);
        Ok(None)
    };
    let c0 = app.m.cycles();
    let draws0 = term.get_frame().count();
    let t0 = Instant::now();
    while t0.elapsed().as_secs_f64() < secs {
        ut88_tui::frame(&mut term, &mut app, &mut pacer, &mut events).expect("кадр");
    }
    let dt = t0.elapsed().as_secs_f64();
    let draws = term.get_frame().count() - draws0;
    ((app.m.cycles() - c0) as f64 / dt / 1e6, draws as f64 / dt)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let secs: f64 = args
        .next()
        .map(|s| s.parse().expect("секунды"))
        .unwrap_or(3.0);
    let only: Vec<String> = args.collect();
    type Load = (&'static str, &'static str, fn() -> Machine);
    let loads: [Load; 2] = [
        ("full", "МОНИТОР-F, простой", full),
        ("tetris", "«Тетрис», уровень 0", tetris),
    ];
    println!("TUI, «максимум» (release), прогон {secs} с");
    println!();
    println!(
        "| {:<8} | {:<20} | {:>9} | {:>14} |",
        "Нагрузка", "Что", "МГц", "Отрисовок/с"
    );
    println!("|{:-<10}|{:-<22}|{:->11}|{:->16}|", "", "", ":", ":");
    for (name, what, make) in loads {
        if !only.is_empty() && !only.iter().any(|o| o == name) {
            continue;
        }
        let (mhz, fps) = measure(make(), secs);
        println!("| {name:<8} | {what:<20} | {mhz:>9.1} | {fps:>14.1} |");
    }
}
