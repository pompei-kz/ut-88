//! Примеры `examples/asm/`: программы журнала собираются байт-в-байт (тест 3 плана),
//! примеры выполняются в эмуляторе ut88-core (тест 5 плана).

use std::path::{Path, PathBuf};

use asm_k580vm80a::ir::Placement;
use asm_k580vm80a::linker::{LinkInput, LinkOptions, link};
use asm_k580vm80a::{Image, Options, assemble_file, object};
use ut88_core::{CPU_HZ, Config, Machine, PadKey};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example(name: &str) -> PathBuf {
    root().join("examples/asm").join(name)
}

/// Собрать файл сразу в образ памяти (ASEG, CSEG с 0000H).
fn build(path: &Path) -> Image {
    let a = assemble_file(path, &Options::default())
        .unwrap_or_else(|e| panic!("{}: {}", path.display(), e[0]));
    a.to_image(&Placement::default()).unwrap()
}

fn load(m: &mut Machine, img: &Image) {
    for (addr, bytes) in &img.chunks {
        m.load_bin(*addr, bytes);
    }
}

/// Пример из двух модулей: сборка в объектные файлы (через текст .link, как на диске),
/// компоновка с 3000H.
fn build_hello_link() -> Image {
    let inputs: Vec<LinkInput> = ["main.asm", "print.asm"]
        .iter()
        .map(|f| {
            let path = example("hello-link").join(f);
            let a = assemble_file(&path, &Options::default())
                .unwrap_or_else(|e| panic!("{f}: {}", e[0]));
            let text = object::write(&a.module);
            let file = f.replace(".asm", ".link");
            let module = object::read(&file, &text).unwrap_or_else(|e| panic!("{}", e[0]));
            LinkInput { file, module }
        })
        .collect();
    let opts = LinkOptions {
        cseg: Some(0x3000),
        ..LinkOptions::default()
    };
    link(&inputs, &opts)
        .unwrap_or_else(|e| panic!("{}", e[0]))
        .image
}

// ---------- тест 3: программы журнала байт-в-байт ----------

#[test]
fn ones_matches_magazine() {
    let img = build(&example("ones.asm"));
    // Коды из doc/05-programmirovanie.md плюс RST 0 (C7) для возврата в МОНИТОР.
    let expected = vec![
        0x3E, 0x11, 0x32, 0x00, 0x90, 0x21, 0x01, 0x90, 0x77, 0x11, 0x02, 0x90,
        0x12, // журнал
        0xC7, // RST 0
    ];
    assert_eq!(img.chunks, vec![(0xC000, expected)]);
    assert_eq!(img.entry, Some(0xC000));
}

#[test]
fn fill_matches_programs_bin() {
    let img = build(&example("fill.asm"));
    let bin = std::fs::read(root().join("programs/fill-const.bin")).unwrap();
    assert_eq!(img.chunks.len(), 1);
    assert_eq!(img.chunks[0].0, 0xC000);
    assert_eq!(img.to_bin(0), bin);
}

#[test]
fn gamma_matches_programs_bin() {
    let img = build(&example("gamma.asm"));
    let bin = std::fs::read(root().join("programs/gamma.bin")).unwrap();
    assert_eq!(img.chunks.len(), 1);
    assert_eq!(img.chunks[0].0, 0xC000);
    assert_eq!(img.to_bin(0), bin);
    // Лишний байт журнала INX B сохранён.
    assert_eq!(bin[0x69], 0x03);
    assert_eq!(img.entry, Some(0xC03E));
}

// ---------- тест 5: сквозной, в эмуляторе ----------

/// Все примеры собираются без ошибок: examples/asm/*.asm — сразу в образ, модули
/// примера hello-link — через компоновщик.
#[test]
fn all_examples_assemble() {
    let mut n = 0;
    for e in std::fs::read_dir(root().join("examples/asm")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "asm") {
            let img = build(&p);
            assert!(!img.chunks.is_empty(), "{}", p.display());
            n += 1;
        }
    }
    assert!(n >= 5, "найдено примеров: {n}");
    assert!(!build_hello_link().chunks.is_empty());
}

/// Выполнять, пока PC не станет `pc` (не дольше `secs` секунд машинного времени).
fn run_to(m: &mut Machine, pc: u16, secs: f64) {
    let limit = m.cycles() + (secs * CPU_HZ) as u64;
    while m.cpu.pc != pc {
        assert!(
            m.cycles() < limit,
            "не дошли до {pc:04X}, PC={:04X}",
            m.cpu.pc
        );
        m.step();
    }
}

fn boot_minimal() -> Machine {
    let mut m = Machine::new(Config::minimal());
    m.run_seconds(0.1);
    m
}

/// Нажать кнопку модуля 1, когда МОНИТОР ждёт клавишу (GETKEY 005FH–0065H).
fn pad(m: &mut Machine, key: PadKey) {
    let limit = m.cycles() + (5.0 * CPU_HZ) as u64;
    while !(0x005F..=0x0065).contains(&m.cpu.pc) {
        assert!(
            m.cycles() < limit,
            "МОНИТОР не ждёт клавишу, PC={:04X}",
            m.cpu.pc
        );
        m.step();
    }
    m.pad_down(key);
    run_to(m, 0x0066, 1.0);
    m.pad_up(key);
}

fn pad_hex(m: &mut Machine, digits: &str) {
    for c in digits.chars() {
        pad(m, PadKey::Hex(c.to_digit(16).unwrap() as u8));
    }
}

#[test]
fn ones_runs() {
    let img = build(&example("ones.asm"));
    let mut m = boot_minimal();
    load(&mut m, &img);
    m.jump(img.entry.unwrap());
    run_to(&mut m, 0xC00D, 0.1); // перед RST 0
    assert_eq!(m.indicator_bytes(), [0x11, 0x11, 0x11]);
    assert_eq!(m.indicator_digits(), [1; 6]);
    // RST 0 — возврат в МОНИТОР.
    m.step();
    assert_eq!(m.cpu.pc, 0x0000);
    m.run_seconds(0.1);
    assert_eq!(m.indicator_bytes()[0], 0x11);
}

#[test]
fn fill_runs() {
    let img = build(&example("fill.asm"));
    let mut m = boot_minimal();
    load(&mut m, &img);
    // Пуск директивой «6» (с C000H), ввод параметров с клавиатуры модуля 1.
    pad(&mut m, PadKey::Hex(6));
    pad_hex(&mut m, "C100C17FA5");
    run_to(&mut m, 0x0000, 1.0); // RST 0 — выход в МОНИТОР
    assert!(m.save_bin(0xC100, 0xC17F).iter().all(|&b| b == 0xA5));
    assert_eq!(m.peek(0xC0FF), 0);
    assert_eq!(m.peek(0xC180), 0);
    m.run_seconds(0.1);
    assert_eq!(m.indicator_bytes()[0], 0x11, "приглашение МОНИТОРА");
}

#[test]
fn macro_runs() {
    let img = build(&example("macro.asm"));
    let mut m = boot_minimal();
    load(&mut m, &img);
    m.jump(img.entry.unwrap());
    // После вывода суммы начинается задержка (MVI C,10).
    let delay = img.chunks[0].1.iter().position(|&b| b == 0x0E).unwrap() as u16;
    run_to(&mut m, 0xC000 + delay, 0.1);
    // N = 5: «05 00 37» (1 + 4 + 9 + 16 + 25 = 55 = 37H).
    assert_eq!(m.indicator_bytes(), [0x37, 0x00, 0x05]);
}

/// Полная конфигурация: загрузить программу, пустить, дождаться возврата в МОНИТОР-F.
fn run_full(img: &Image, start: u16) -> String {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    load(&mut m, img);
    m.jump(start);
    run_to(&mut m, 0xF86C, 2.0);
    m.run_seconds(0.2);
    m.screen().expect("нет дисплейного модуля").text()
}

#[test]
fn hello_runs_on_monitor_f() {
    let img = build(&example("hello.asm"));
    assert_eq!(img.entry, Some(0x3000));
    let text = run_full(&img, 0x3000);
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let i = lines
        .iter()
        .position(|l| *l == "HELLO, WORLD!")
        .unwrap_or_else(|| panic!("нет строки на экране:\n{text}"));
    assert_eq!(lines[i + 1], "ПРИВЕТ, ЮТ-88!");
    assert_eq!(lines[i + 2], " =>", "приглашение МОНИТОРА-F");
}

#[test]
fn hello_link_runs_on_monitor_f() {
    let img = build_hello_link();
    assert_eq!(img.entry, Some(0x3000));
    let text = run_full(&img, 0x3000);
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let i = lines
        .iter()
        .position(|l| *l == "HELLO FROM TWO MODULES!")
        .unwrap_or_else(|| panic!("нет строки на экране:\n{text}"));
    assert_eq!(lines[i + 1], "ПРИВЕТ ИЗ ДВУХ МОДУЛЕЙ!");
}

#[test]
fn draw_paints_screen() {
    let img = build(&example("draw.asm"));
    assert_eq!(img.entry, Some(0x3000));
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    load(&mut m, &img);
    m.jump(0x3000);
    // Программа рисует и ждёт клавишу в F803H.
    m.run_seconds(0.5);
    let s = m.screen().expect("нет дисплейного модуля");
    let row = |r: usize| s.line(r).trim_end().to_string();
    // Рамка с уголками «+».
    assert_eq!(row(0), format!("+{}+", "*".repeat(62)));
    assert_eq!(row(25), format!("+{}+", "*".repeat(62)));
    for r in 1..25 {
        let line: Vec<char> = s.line(r).chars().collect();
        assert_eq!(
            (line[0], line[63]),
            ('*', '*'),
            "боковые стороны рамки, строка {r}"
        );
    }
    // Диагонали вне прямоугольника.
    assert_eq!(s.line(1).chars().nth(8), Some('\\'));
    assert_eq!(s.line(1).chars().nth(55), Some('/'));
    assert_eq!(s.line(24).chars().nth(54), Some('\\'));
    assert_eq!(s.line(24).chars().nth(9), Some('/'));
    // Инверсный прямоугольник: строки 10–14, знакоместа 23–39 (бит 7 в ячейках 24–40).
    for r in 10..15 {
        for c in 0..64 {
            assert_eq!(
                s.inverse[r * 64 + c],
                (23..=39).contains(&c),
                "инверсия {r}:{c}"
            );
        }
    }
    // Надпись.
    assert!(row(20).contains("ЮТ-88 РИСУЕТ!"), "{}", row(20));
}
