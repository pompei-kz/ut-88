//! Замер скорости ядра: эмулированные такты в секунду реального времени.
//!
//! ```text
//! cargo run --release -p ut88-core --example bench [-- СЕКУНДЫ [НАГРУЗКА…]]
//! ```
//!
//! Нагрузки: `minimal` (МОНИТОР модуля 1 ждёт клавишу), `full` (МОНИТОР-F ждёт клавишу),
//! `tetris` («Тетрис» идёт на уровне 0), `cpm` (CP/M-64 ждёт команду на «A>»). Каждая
//! прогоняется не меньше заданного времени (по умолчанию 2 с) одним вызовом
//! `run_cycles` крупной порцией и порциями по `FRAME_CYCLES`, с одного и того же снимка.

use std::time::{Duration, Instant};

use ut88_core::{CPU_HZ, Config, FRAME_CYCLES, KbKey, Machine, System};

/// Путь к файлу относительно корня репозитория.
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

/// Держать клавишу, пока программа её не примет (щелчок подпрограммы F803H слышен как
/// перепад выхода магнитофона), затем отпустить и дать программе дорисовать экран.
fn press(m: &mut Machine, code: u8) {
    let (key, modifier) = KbKey::for_code(code).expect("код клавиши");
    m.set_sound_capture(true);
    m.drain_sound_edges();
    if let Some(md) = modifier {
        m.kb_down(md);
    }
    m.kb_down(key);
    let limit = m.cycles() + 3 * CPU_HZ as u64;
    while m.cycles() < limit && m.drain_sound_edges().is_empty() {
        m.run_seconds(0.005);
    }
    m.kb_up(key);
    if let Some(md) = modifier {
        m.kb_up(md);
    }
    m.set_sound_capture(false);
    m.run_seconds(0.3);
}

fn kb_type(m: &mut Machine, text: &str) {
    for b in text.bytes() {
        kb_code(m, b);
    }
}

/// Выполнять, пока на экране не появится `text` (не дольше `secs` эмулированных секунд).
fn wait_text(m: &mut Machine, text: &str, secs: f64) {
    let limit = m.cycles() + (secs * CPU_HZ) as u64;
    while !screen_text(m).contains(text) {
        assert!(m.cycles() < limit, "на экране нет «{text}»");
        m.run_seconds(0.05);
    }
}

fn minimal() -> Machine {
    let mut m = Machine::new(Config::minimal());
    m.run_seconds(0.5);
    m
}

fn full() -> Machine {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    m
}

fn tetris() -> Machine {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    m.load_bin_file(repo("programs/tetris.bin"), 0x3000)
        .expect("programs/tetris.bin");
    kb_type(&mut m, "G3000\r");
    wait_text(&mut m, "ВАШ УРОВЕНЬ(0-7) ?", 3.0);
    press(&mut m, b'0');
    wait_text(&mut m, "НАЖМИТЕ ЛЮБУЮ КЛАВИШУ", 3.0);
    press(&mut m, b' ');
    m.run_seconds(0.5);
    assert!(!screen_text(&m).contains("НАЖМИТЕ ЛЮБУЮ КЛАВИШУ"));
    m
}

fn cpm() -> Machine {
    let mut m = Machine::new(Config::cpm());
    m.run_seconds(0.5);
    m.boot_system(System::Cpm64, false).expect("CP/M-64");
    wait_text(&mut m, "A>", 10.0);
    m.run_seconds(0.5);
    m
}

/// Скорость: такты/с.
fn speed(cycles: u64, d: Duration) -> f64 {
    cycles as f64 / d.as_secs_f64()
}

/// Прогон одной большой порцией (размер — по пробному прогону) и порциями по кадру.
fn measure(m0: &Machine, secs: f64) -> (f64, f64) {
    let snap = m0.snapshot();
    let fresh = || {
        let mut m = Machine::from_snapshot(&snap).expect("снимок");
        // Квазидиск — часть шины, расширений нет; звук как в GUI на «максимуме» не
        // собирается.
        m.set_sound_capture(false);
        m
    };

    // Порции по кадру.
    let mut m = fresh();
    let t0 = Instant::now();
    let mut n = 0;
    while t0.elapsed().as_secs_f64() < secs {
        n += m.run_cycles(FRAME_CYCLES);
    }
    let frames = speed(n, t0.elapsed());

    // Одна большая порция: столько тактов, чтобы при скорости по кадрам заняло ≈ secs·1,1.
    let mut m = fresh();
    let big = (frames * secs * 1.1) as u64;
    let t0 = Instant::now();
    let n = m.run_cycles(big);
    let one = speed(n, t0.elapsed());
    (one, frames)
}

/// Нагрузка: имя, описание, подготовка машины.
type Load = (&'static str, &'static str, fn() -> Machine);

fn main() {
    let mut args = std::env::args().skip(1);
    let secs: f64 = args
        .next()
        .map(|s| s.parse().expect("секунды"))
        .unwrap_or(2.0);
    let only: Vec<String> = args.collect();

    let loads: [Load; 4] = [
        ("minimal", "МОНИТОР модуля 1, простой", minimal),
        ("full", "МОНИТОР-F, простой", full),
        ("tetris", "«Тетрис», уровень 0", tetris),
        ("cpm", "CP/M-64, простой на «A>»", cpm),
    ];
    println!("Скорость ядра (release), прогон ≥ {secs} с; 1 МГц = 10⁶ тактов/с, ЮТ-88 — 1,78 МГц");
    println!();
    println!(
        "| {:<10} | {:<27} | {:>18} | {:>9} | {:>18} | {:>9} |",
        "Нагрузка", "Что", "Порция: такты/с", "МГц", "Кадры: такты/с", "МГц"
    );
    println!(
        "|{:-<12}|{:-<29}|{:->20}|{:->11}|{:->20}|{:->11}|",
        "", "", ":", ":", ":", ":"
    );
    for (name, what, make) in loads {
        if !only.is_empty() && !only.iter().any(|o| o == name) {
            continue;
        }
        let m = make();
        let (one, frames) = measure(&m, secs);
        println!(
            "| {:<10} | {:<27} | {:>18.0} | {:>9.1} | {:>18.0} | {:>9.1} |",
            name,
            what,
            one,
            one / 1e6,
            frames,
            frames / 1e6
        );
    }
}
