//! Проверки TUI без терминала: очередь нажатий на настоящих МОНИТОРАХ, отрисовка кадров
//! через TestBackend, отладчик в приложении.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ut88_core::{CPU_HZ, Config, KbKey, Machine, PadKey};
use ut88_tui::app::{App, View};
use ut88_tui::debugger::{Debugger, run_until};
use ut88_tui::input::{InputQueue, Target};
use ut88_tui::widgets::buffer_text;
use ut88_tui::{Options, keymap};

fn run_secs(m: &mut Machine, q: &mut InputQueue, secs: f64) {
    let mut d = Debugger::new();
    let t = m.cycles() + (secs * CPU_HZ) as u64;
    assert_eq!(run_until(m, t, &mut d, q, false), None);
}

/// Нажатия «набраны» с интервалом 150 мс реального времени (не автоповтор), но все
/// сразу — очередь сама разносит их во времени машины.
fn type_all(q: &mut InputQueue, keys: &[(Target, String)]) {
    let t0 = Instant::now();
    for (i, (t, id)) in keys.iter().enumerate() {
        q.press(*t, id, false, t0 + Duration::from_millis(150 * i as u64));
    }
}

#[test]
fn pad_queue_types_into_monitor0() {
    let mut m = Machine::new(Config::minimal());
    let mut q = InputQueue::new();
    run_secs(&mut m, &mut q, 0.1);
    // Директива 8: контрольная сумма 0000–01FF (у monitor-fixed.bin — 0171H).
    let keys: Vec<_> = "8000001FF"
        .chars()
        .map(|c| {
            let d = c.to_digit(16).unwrap() as u8;
            (Target::Pad(PadKey::Hex(d)), c.to_string())
        })
        .collect();
    type_all(&mut q, &keys);
    run_secs(&mut m, &mut q, 6.0);
    assert_eq!(q.pending(), 0);
    assert_eq!(m.indicator_bytes(), [0x11, 0x71, 0x01]);
}

fn kb_target(c: char) -> (Target, String) {
    let code = keymap::char_code(c).unwrap_or(if c == '\r' { 0x0D } else { 0 });
    let (k, md) = KbKey::for_code(code).unwrap();
    (Target::Kb(k, md), c.to_lowercase().collect())
}

#[test]
fn kb_queue_types_into_monitor_f() {
    let mut m = Machine::new(Config::full());
    let mut q = InputQueue::new();
    run_secs(&mut m, &mut q, 0.5);
    let keys: Vec<_> = "DF800,F80F\r".chars().map(kb_target).collect();
    type_all(&mut q, &keys);
    run_secs(&mut m, &mut q, 4.0);
    let s = m.screen().unwrap();
    assert_eq!(s.line(2), " =>DF800,F80F");
    assert_eq!(
        s.line(3).trim(),
        "F800 C3 36 F8 C3 57 FD C3 71 FB C3 43 FC C3 EE FB C3"
    );
    // Русские буквы — с РУС.
    let keys: Vec<_> = "ют".chars().map(kb_target).collect();
    type_all(&mut q, &keys);
    run_secs(&mut m, &mut q, 2.0);
    assert_eq!(m.screen().unwrap().line(4), " =>ЮТ");
}

#[test]
fn single_key_hold_starts_tetris() {
    let mut m = Machine::new(Config::full());
    let mut q = InputQueue::new();
    run_secs(&mut m, &mut q, 0.5);
    m.load_bin_file(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs/tetris.bin"),
        0x3000,
    )
    .unwrap();
    let keys: Vec<_> = "G3000\r".chars().map(kb_target).collect();
    type_all(&mut q, &keys);
    run_secs(&mut m, &mut q, 3.0);
    type_all(&mut q, &[kb_target('3')]);
    run_secs(&mut m, &mut q, 1.0);
    let text = m.screen().unwrap().text();
    assert!(text.contains("НАЖМИТЕ ЛЮБУЮ КЛАВИШУ"), "{text}");
    // Одно нажатие пробела (без автоповтора) — игра начинается.
    type_all(&mut q, &[kb_target(' ')]);
    run_secs(&mut m, &mut q, 3.0);
    let text = m.screen().unwrap().text();
    assert!(text.contains("ЧИСЛО ПОЛНЫХ СТРОК   00"), "{text}");
}

#[test]
fn autorepeat_extends_and_release_works() {
    let mut m = Machine::new(Config::full());
    let mut q = InputQueue::new();
    let t = kb_target('A');
    let t0 = Instant::now();
    q.press(t.0, &t.1, false, t0);
    run_secs(&mut m, &mut q, 0.01);
    assert_eq!(q.current(), Some(t.0));
    // Автоповтор (через 30 мс) не ставит новое нажатие в очередь.
    q.press(t.0, &t.1, false, t0 + Duration::from_millis(30));
    assert_eq!(q.pending(), 0);
    run_secs(&mut m, &mut q, 0.5);
    assert_eq!(q.current(), None, "отпущена по времени");
    // Kitty: держится, пока не отпустят.
    q.press(t.0, &t.1, true, t0 + Duration::from_secs(5));
    run_secs(&mut m, &mut q, 2.0);
    assert_eq!(q.current(), Some(t.0));
    q.release(&t.1, m.cycles());
    run_secs(&mut m, &mut q, 0.01);
    assert_eq!(q.current(), None);
}

fn render(app: &App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    buffer_text(term.backend().buffer())
}

#[test]
fn frame_minimal_shows_11() {
    let mut app = App::new(Machine::new(Config::minimal()), Options::default());
    let t = app.m.cycles() + (0.1 * CPU_HZ) as u64;
    app.run_to_cycle(t);
    let text = render(&app, 80, 24);
    println!("{text}");
    let lines: Vec<&str> = text.lines().collect();
    // Два правых разряда — «1»: вертикальные сегменты b, c справа в цифрах 5 и 6.
    // Разряды 5 и 6 начинаются со столбцов 2 + 4·10 + 3 и +10; у «1» горит только
    // правый столбец цифры (x + 7) во всех семи строках.
    let x5 = 2 + 4 * 10 + 3;
    for (y, line) in lines.iter().enumerate().take(8).skip(1) {
        let chars: Vec<char> = line
            .chars()
            .chain(std::iter::repeat(' '))
            .take(80)
            .collect();
        for x0 in [x5, x5 + 10] {
            let digit: String = chars[x0..x0 + 8].iter().collect();
            assert_eq!(digit, "       █", "строка {y}");
        }
    }
    // Адрес 0000 — нули: верхняя строка цифры горит целиком.
    assert!(lines[1].starts_with("  ████████  ████████"), "{}", lines[1]);
    assert!(text.contains("ШАГ НАЗАД"));
    assert!(text.contains("│ C │ D │ E │ F │"));
    assert!(lines[23].contains("минимальная"));
}

#[test]
fn frame_full_shows_prompt() {
    let mut app = App::new(Machine::new(Config::full()), Options::default());
    let t = app.m.cycles() + (0.5 * CPU_HZ) as u64;
    app.run_to_cycle(t);
    let text = render(&app, 80, 32);
    println!("{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[2],
        "│*ЮТ/88*                                                         │"
    );
    assert!(lines[3].starts_with("│ =>"));
    assert!(lines[31].contains("полная"));
    // Курсор — инверсное знакоместо.
    let mut term = Terminal::new(TestBackend::new(80, 32)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let cell = term.backend().buffer().cell((4, 3)).unwrap();
    assert_eq!(cell.bg, ratatui::style::Color::White);
}

fn key(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

#[test]
fn app_hotkeys_and_debugger() {
    let mut app = App::new(Machine::new(Config::minimal()), Options::default());
    app.on_key(key(KeyCode::F(12)));
    assert_eq!(app.view, View::Debugger);
    assert!(!app.running());
    let pc0 = app.m.cpu.pc;
    app.on_key(key(KeyCode::Char('s')));
    assert_ne!(app.m.cpu.pc, pc0, "шаг");
    // Точка останова через строку ввода.
    app.on_key(key(KeyCode::Char('b')));
    for _ in 0..4 {
        app.on_key(key(KeyCode::Backspace));
    }
    for c in "005F".chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    assert!(app.dbg.breakpoints.contains(&0x005F));
    app.on_key(key(KeyCode::Char('g')));
    assert!(app.running());
    let t = app.m.cycles() + CPU_HZ as u64;
    app.run_to_cycle(t);
    assert!(!app.running());
    assert_eq!(app.m.cpu.pc, 0x005F);
    let text = render(&app, 100, 30);
    println!("{text}");
    assert!(text.contains("PC=005F"));
    assert!(text.contains("Останов: точка останова 005F"));
    // Выход из отладчика, пауза, скорость, УСТ, холодный старт.
    app.on_key(key(KeyCode::Esc));
    assert_eq!(app.view, View::Machine);
    app.on_key(key(KeyCode::F(4)));
    assert!(!app.running());
    app.on_key(key(KeyCode::F(5)));
    assert_eq!(app.speed, 2.0);
    app.on_key(key(KeyCode::F(2)));
    assert_eq!(app.m.cpu.pc, 0);
    app.m.poke(0xC000, 0x55);
    app.on_key(key(KeyCode::F(3)));
    assert_eq!(app.m.peek(0xC000), 0);
    // Меню: «q» — выход.
    app.on_key(key(KeyCode::F(1)));
    app.on_key(key(KeyCode::Char('q')));
    assert!(app.quit);
}

#[test]
fn snapshot_roundtrip_via_prompts() {
    let path = std::env::temp_dir().join(format!("ut88-tui-{}.snap", std::process::id()));
    let p = path.to_string_lossy().to_string();
    let mut app = App::new(Machine::new(Config::minimal()), Options::default());
    app.m.poke(0xC000, 0xAB);
    let type_path = |app: &mut App| {
        for _ in 0..20 {
            app.on_key(key(KeyCode::Backspace));
        }
        for c in p.chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
    };
    app.on_key(key(KeyCode::F(6)));
    type_path(&mut app);
    app.m.poke(0xC000, 0);
    app.on_key(key(KeyCode::F(7)));
    type_path(&mut app);
    assert_eq!(app.m.peek(0xC000), 0xAB);
    let _ = std::fs::remove_file(path);
}

#[test]
fn programs_menu_loads_and_runs() {
    let mut app = App::new(Machine::new(Config::minimal()), Options::default());
    app.on_key(key(KeyCode::F(9)));
    // Выбрать «reaction».
    for _ in 0..20 {
        if let Some(ut88_tui::app::Overlay::Programs(l, s)) = &app.overlay
            && l[*s].name == "reaction"
        {
            break;
        }
        app.on_key(key(KeyCode::Down));
    }
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.m.cpu.pc, 0xC000);
    let data = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../programs/reaction.bin"
    ))
    .unwrap();
    assert_eq!(app.m.save_bin(0xC000, 0xC000 + data.len() as u16 - 1), data);
}
