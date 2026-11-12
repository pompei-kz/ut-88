//! Проверки TUI без терминала: очередь нажатий на настоящих МОНИТОРАХ, отрисовка кадров
//! через TestBackend, отладчик в приложении.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ut88_core::{CPU_HZ, Config, KbKey, Machine, PadKey};
use ut88_tui::app::{App, Overlay, View};
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
    assert!(lines[22].contains("минимальная"));
    assert!(lines[23].starts_with("2УСТ3Холод4Пауза"), "{}", lines[23]);
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
    assert!(lines[30].contains("полная"));
    assert!(lines[31].contains("11Справка12Отладка"), "{}", lines[31]);
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
    app.on_key(key(KeyCode::F(11)));
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
    // Меню F11: «a» — сохранить снимок в файл, «o» — загрузить из файла.
    app.on_key(key(KeyCode::F(11)));
    app.on_key(key(KeyCode::Char('a')));
    type_path(&mut app);
    app.m.poke(0xC000, 0);
    app.on_key(key(KeyCode::F(11)));
    app.on_key(key(KeyCode::Char('o')));
    type_path(&mut app);
    assert_eq!(app.m.peek(0xC000), 0xAB);
    let _ = std::fs::remove_file(path);
}

#[test]
fn quick_snapshot_f6_f7() {
    let path = std::env::temp_dir().join(format!("ut88-tui-quick-{}.snap", std::process::id()));
    let opts = Options {
        quick_snapshot: path.clone(),
        ..Options::default()
    };
    let mut app = App::new(Machine::new(Config::minimal()), opts);
    app.m.poke(0xC000, 0x5A);
    app.on_key(key(KeyCode::F(6)));
    assert!(app.overlay.is_none(), "F6 сохраняет без вопросов");
    app.m.poke(0xC000, 0);
    app.on_key(key(KeyCode::F(7)));
    assert_eq!(app.m.peek(0xC000), 0x5A);
    let _ = std::fs::remove_file(path);
}

#[test]
fn speeds_shared_with_gui() {
    let opts = Options {
        speed: 16.0,
        ..Options::default()
    };
    let mut app = App::new(Machine::new(Config::minimal()), opts);
    assert_eq!(app.speed, 16.0);
    app.on_key(key(KeyCode::F(5)));
    assert_eq!(app.speed, 0.0, "после ×16 — максимум");
    for _ in 0..10 {
        app.on_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::SHIFT));
    }
    assert_eq!(app.speed, 0.25, "самая медленная — ×0,25");
}

#[test]
fn programs_menu_checks_config() {
    let mut app = App::new(Machine::new(Config::minimal()), Options::default());
    app.on_key(key(KeyCode::F(9)));
    for _ in 0..20 {
        if let Some(ut88_tui::app::Overlay::Programs(l, s)) = &app.overlay
            && l[*s].name == "tetris"
        {
            break;
        }
        app.on_key(key(KeyCode::Down));
    }
    let text = render(&app, 100, 30);
    assert!(text.contains("нужен дисплейный модуль"), "{text}");
    let pc = app.m.cpu.pc;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.m.cpu.pc, pc, "«Тетрис» не запущен в минимальной");
    assert_eq!(app.m.peek(0x3000), 0xFF, "и не загружен");
    assert!(matches!(
        app.overlay,
        Some(ut88_tui::app::Overlay::Programs(..))
    ));
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

fn click(app: &mut App, column: u16, row: u16) {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

/// Щелчок по кнопке панели с действием `hk` (координаты — из последней отрисовки).
fn click_hotkey(app: &mut App, hk: keymap::Hotkey) {
    render(app, 80, 32);
    let (r, _) = app
        .keybar_buttons()
        .into_iter()
        .find(|b| b.1 == hk)
        .unwrap_or_else(|| panic!("нет кнопки {hk:?}"));
    // Щёлкаем по последней клетке кнопки — по подписи.
    click(app, r.right() - 1, r.y);
}

#[test]
fn keybar_fits_80_and_covers_hotkeys() {
    let w: usize = keymap::KEYBAR
        .iter()
        .map(|(n, l, _)| n.chars().count() + l.chars().count())
        .sum();
    assert!(w <= 80, "панель шириной {w}");
    for (k, hk, _) in keymap::HOTKEYS {
        assert!(
            keymap::KEYBAR.iter().any(|b| b.2 == *hk),
            "{k} нет на панели"
        );
    }
    // Номер на кнопке совпадает с клавишей.
    for (n, _, hk) in keymap::KEYBAR {
        let k = keymap::HOTKEYS.iter().find(|h| h.1 == *hk).unwrap().0;
        assert_eq!(k.replace("Shift+F", "⇧").replace('F', ""), *n);
    }
    // В 80 столбцах нарисованы все кнопки.
    let app = App::new(Machine::new(Config::full()), Options::default());
    render(&app, 80, 32);
    assert_eq!(app.keybar_buttons().len(), keymap::KEYBAR.len());
}

#[test]
fn keybar_mouse_clicks() {
    use keymap::Hotkey;
    let mut app = App::new(Machine::new(Config::minimal()), Options::default());
    // F11 — меню; «11Справка» мышью открывает и закрывает меню.
    app.on_key(key(KeyCode::F(11)));
    assert!(matches!(app.overlay, Some(Overlay::Menu(_))));
    app.on_key(key(KeyCode::F(11)));
    assert!(app.overlay.is_none());
    click_hotkey(&mut app, Hotkey::Menu);
    assert!(matches!(app.overlay, Some(Overlay::Menu(_))));
    let text = render(&app, 80, 32);
    assert!(text.contains("справка: все клавиши"), "{text}");
    click_hotkey(&mut app, Hotkey::Menu);
    assert!(app.overlay.is_none());
    // F1 больше ничего не делает.
    app.on_key(key(KeyCode::F(1)));
    assert!(app.overlay.is_none());
    // Полная справка из меню: «?».
    click_hotkey(&mut app, Hotkey::Menu);
    app.on_key(key(KeyCode::Char('?')));
    assert!(matches!(app.overlay, Some(Overlay::Help)));
    let text = render(&app, 80, 32);
    assert!(text.contains("F11       меню и справка"), "{text}");
    assert!(text.contains("11Справка"), "{text}");
    click_hotkey(&mut app, Hotkey::Menu);
    assert!(app.overlay.is_none());
    // Пауза, скорость, магнитофон, отладчик, выход — щелчками.
    click_hotkey(&mut app, Hotkey::Pause);
    assert!(!app.running());
    click_hotkey(&mut app, Hotkey::Pause);
    assert!(app.running());
    click_hotkey(&mut app, Hotkey::SpeedUp);
    assert_eq!(app.speed, 2.0);
    click_hotkey(&mut app, Hotkey::SpeedDown);
    assert_eq!(app.speed, 1.0);
    click_hotkey(&mut app, Hotkey::Tape);
    assert!(matches!(app.overlay, Some(Overlay::Tape(_))));
    // Щелчок по другой кнопке закрывает окно и выполняет действие.
    click_hotkey(&mut app, Hotkey::Debugger);
    assert!(app.overlay.is_none());
    assert_eq!(app.view, View::Debugger);
    click_hotkey(&mut app, Hotkey::Debugger);
    assert_eq!(app.view, View::Machine);
    app.m.poke(0xC000, 0x55);
    click_hotkey(&mut app, Hotkey::ColdStart);
    assert_eq!(app.m.peek(0xC000), 0);
    // Щелчок мимо кнопок и правой кнопкой — ничего.
    render(&app, 80, 32);
    click(&mut app, 79, 31);
    click(&mut app, 10, 5);
    let r = app.keybar_buttons()[0].0;
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: r.x,
        row: r.y,
        modifiers: KeyModifiers::NONE,
    });
    assert!(app.overlay.is_none() && !app.quit);
    click_hotkey(&mut app, Hotkey::Exit);
    assert!(app.quit);
}

#[test]
fn cpm_menu_boots_and_puts_files() {
    let mut app = App::new(Machine::new(Config::minimal()), Options::default());
    app.on_key(key(KeyCode::F(11)));
    app.on_key(key(KeyCode::Char('c')));
    assert!(matches!(app.overlay, Some(Overlay::Cpm(_))));
    let text = render(&app, 80, 32);
    assert!(text.contains("загрузить CP/M-64"), "{text}");
    // CP/M-64 из минимальной конфигурации: холодный старт в «CP/M (квазидиск)».
    app.on_key(key(KeyCode::Char('6')));
    assert!(app.overlay.is_none());
    assert!(app.m.has_kvazidisk());
    assert_eq!(app.m.cpu.pc, 0xDA00);
    let text = render(&app, 80, 32);
    assert!(text.contains("CP/M-64 загружена"), "{text}");
    // Записать файл на диск через строку ввода.
    let dir = std::env::temp_dir().join(format!("ut88-tui-cpm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("hello.txt");
    std::fs::write(&f, b"HI\r\n").unwrap();
    app.on_key(key(KeyCode::F(11)));
    app.on_key(key(KeyCode::Char('c')));
    app.on_key(key(KeyCode::Char('w')));
    for c in f.display().to_string().chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    let files = app.m.cpm_list(ut88_core::CpmDisk::Kvazidisk).unwrap();
    assert_eq!(files[0].name, "HELLO.TXT");
    // ОС ЮТ-88: ПЗУ МОНИТОРА-F выключено, тумблер — пункт «r» меню.
    app.on_key(key(KeyCode::F(11)));
    app.on_key(key(KeyCode::Char('c')));
    app.on_key(key(KeyCode::Char('o')));
    assert!(!app.m.config().monitor_f_rom);
    app.on_key(key(KeyCode::F(11)));
    app.on_key(key(KeyCode::Char('r')));
    assert!(app.m.config().monitor_f_rom);
    // Холодный старт возвращает тумблер.
    app.m.set_monitor_f_rom(false);
    app.on_key(key(KeyCode::F(3)));
    assert!(app.m.config().monitor_f_rom);
    assert!(app.m.has_kvazidisk());
}
