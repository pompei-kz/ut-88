//! Тесты окна на стенде [`Harness`]: кадры [`App::frame`] строятся без окна ОС, виджеты
//! ищутся по подписи в реестре кадра и «щёлкаются» мышью — настоящее окно не
//! открывается, события в X-экран не посылаются. Файловые диалоги подменены
//! [`ScriptedDialogs`], звук — [`Audio::silent`].

use std::path::{Path, PathBuf};

use ut88_audio::Audio;
use ut88_core::{Config, Machine};

use super::*;
use crate::dialogs::ScriptedDialogs;
use crate::gfx::{FontKind, missing_glyphs, vec2};
use crate::ui::testing::Harness;
use crate::ui::{Modifiers, WidgetKind};

/// Окно приложения на стенде.
struct Win {
    h: Harness<App>,
    dlg: ScriptedDialogs,
    dir: PathBuf,
}

/// Папка `programs/` репозитория.
fn programs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../programs")
}

/// Чистая временная папка теста.
fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ut88-gui-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Приложение для тестов: подставные диалоги, быстрый снимок в папке теста, звук без
/// звуковой карты.
fn test_app(
    dir: &Path,
    machine: Machine,
    speed: Speed,
    sound: bool,
    cfg: PathBuf,
) -> (App, ScriptedDialogs) {
    let dlg = ScriptedDialogs::default();
    let opts = GuiOptions {
        machine,
        speed,
        sound,
        programs_dir: Some(programs_dir()),
        kvazidisk_file: None,
        settings_dir: Some(cfg),
    };
    let mut app = App::new(opts);
    app.set_file_dialogs(Box::new(dlg.clone()));
    app.set_quick_snapshot(dir.join("quick.snap"));
    app.set_audio_opener(Audio::silent);
    (app, dlg)
}

impl Win {
    fn new(name: &str, config: Config) -> Self {
        Self::with_machine(name, Machine::new(config), Speed::X1)
    }

    /// Окно с готовой машиной и скоростью.
    fn with_machine(name: &str, machine: Machine, speed: Speed) -> Self {
        let dir = tmp_dir(name);
        let cfg = dir.join("config");
        Self::launch(dir, machine, speed, false, cfg)
    }

    /// Окно мини-конфигурации с ключом звука `sound` (`false` — как `--no-sound`) и
    /// папкой настроек `cfg`; `name` — имя временной папки теста.
    fn with_settings(name: &str, sound: bool, cfg: &Path) -> Self {
        let dir = tmp_dir(name);
        Self::launch(
            dir,
            Machine::new(Config::minimal()),
            Speed::X1,
            sound,
            cfg.to_path_buf(),
        )
    }

    fn launch(dir: PathBuf, machine: Machine, speed: Speed, sound: bool, cfg: PathBuf) -> Self {
        let (app, dlg) = test_app(&dir, machine, speed, sound, cfg);
        let h = Harness::new(vec2(1280.0, 900.0), app, |ctx, app: &mut App| {
            app.frame(ctx)
        });
        Self { h, dlg, dir }
    }

    fn app(&self) -> &App {
        &self.h.state
    }

    fn app_mut(&mut self) -> &mut App {
        &mut self.h.state
    }

    fn m(&mut self) -> &mut Machine {
        &mut self.h.state.m
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Следующий файловый диалог вернёт этот путь.
    fn answer(&self, p: &Path) {
        self.dlg.push(Some(p.to_path_buf()));
    }

    /// Обработать события и нарисовать ещё кадр.
    fn settle(&mut self) {
        self.h.settle();
    }

    /// Щёлкнуть мышью по единственному виджету с подписью `label`.
    fn click(&mut self, label: &str) {
        self.h.click(label);
    }

    /// Щёлкнуть `n`-й (по порядку кадра) виджет с подписью `label`.
    fn click_nth(&mut self, label: &str, n: usize) {
        self.h.click_nth(label, n);
    }

    /// Пункт меню: `top` → `item`.
    fn menu(&mut self, top: &str, item: &str) {
        self.click(top);
        self.click(item);
    }

    fn key(&mut self, k: KeyCode) {
        self.h.key(k);
        self.settle();
    }

    fn has(&self, label: &str) -> bool {
        self.h.has(label)
    }

    fn has_containing(&self, part: &str) -> bool {
        self.h.has_containing(part)
    }

    /// Виджет `label` доступен (паника, если его нет или их несколько).
    fn enabled(&self, label: &str) -> bool {
        self.h.get(label).enabled
    }

    /// Остановить эмуляцию (меню «Машина → Пауза»), чтобы память и PC не менялись.
    fn pause(&mut self) {
        if !self.app().paused {
            self.menu("Машина", "Пауза (F4)");
        }
        assert!(self.app().paused);
    }

    fn no_error(&self) {
        assert_eq!(
            self.app().error_popup,
            None,
            "статус: {}",
            self.app().status
        );
    }

    fn status(&self) -> String {
        self.app().status.clone()
    }

    fn mem(&mut self, addr: u16, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| self.m().peek(addr.wrapping_add(i as u16)))
            .collect()
    }

    /// Кнопка `button` в строке окна F9, где подпись программы начинается с `name — `.
    fn row_widget(&self, name: &str, button: &str) -> crate::ui::WidgetInfo {
        let prefix = format!("{name} — ");
        self.h
            .registry()
            .in_row_with_prefix(&prefix, button)
            .cloned()
            .unwrap_or_else(|| panic!("нет «{button}» в строке {name}"))
    }

    fn program_row_button(&mut self, name: &str, button: &str) {
        let w = self.row_widget(name, button);
        assert!(w.enabled, "{name}: «{button}» выключена");
        self.h.click_widget(&w);
    }

    fn program_row_disabled(&self, name: &str, button: &str) -> bool {
        !self.row_widget(name, button).enabled
    }
}

impl Drop for Win {
    fn drop(&mut self) {
        // Папку оставляем, если тест упал, — чтобы посмотреть файлы.
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

fn program_bin(name: &str) -> Vec<u8> {
    std::fs::read(programs_dir().join(format!("{name}.bin"))).unwrap()
}

// ------------------------------------------------------------------ снимки

#[test]
fn file_menu_snapshot_save_and_load() {
    let mut w = Win::new("snap", Config::minimal());
    w.pause();
    w.m().poke(0xC000, 0x12);
    let snap = w.path("state.snap");

    w.answer(&snap);
    w.menu("Файл", "Сохранить снимок…");
    w.no_error();
    assert!(snap.is_file(), "снимок не записан");
    assert_eq!(w.dlg.requests.borrow()[0].default, "ut88.snap");
    assert!(!w.dlg.requests.borrow()[0].open);

    w.m().poke(0xC000, 0x34);
    w.answer(&snap);
    w.menu("Файл", "Загрузить снимок…");
    w.no_error();
    assert_eq!(w.m().peek(0xC000), 0x12);
    assert!(w.status().starts_with("Снимок загружен"));

    // «Отмена» в диалоге — ничего не происходит.
    w.m().poke(0xC000, 0x56);
    w.dlg.push(None);
    w.menu("Файл", "Загрузить снимок…");
    w.no_error();
    assert_eq!(w.m().peek(0xC000), 0x56);

    // Битый файл — сообщение об ошибке, машина прежняя.
    let bad = w.path("bad.snap");
    std::fs::write(&bad, b"garbage").unwrap();
    w.answer(&bad);
    w.menu("Файл", "Загрузить снимок…");
    assert!(w.app().error_popup.is_some());
    assert!(w.has("Ошибка"));
    w.click("OK");
    w.no_error();
    assert!(!w.has("Ошибка"));
    assert_eq!(w.m().peek(0xC000), 0x56);
    assert_eq!(w.dlg.pending(), 0);
}

#[test]
fn quick_snapshots_keys_buttons_menu() {
    let mut w = Win::new("quick", Config::minimal());
    w.pause();
    let quick = w.path("quick.snap");

    // Клавиши F6/F7.
    w.m().poke(0xC010, 0xA1);
    w.key(KeyCode::F6);
    w.no_error();
    assert!(quick.is_file());
    w.m().poke(0xC010, 0x00);
    w.key(KeyCode::F7);
    w.no_error();
    assert_eq!(w.m().peek(0xC010), 0xA1);

    // Кнопки панели.
    w.m().poke(0xC010, 0xB2);
    w.click("F6 Снимок");
    w.no_error();
    w.m().poke(0xC010, 0x00);
    w.click("F7 Загр");
    w.no_error();
    assert_eq!(w.m().peek(0xC010), 0xB2);

    // Пункты меню «Файл».
    w.m().poke(0xC010, 0xC3);
    w.menu("Файл", "Быстрый снимок (F6)");
    w.m().poke(0xC010, 0x00);
    w.menu("Файл", "Загрузить быстрый снимок (F7)");
    w.no_error();
    assert_eq!(w.m().peek(0xC010), 0xC3);
    // Диалоги не вызывались, в текущей папке ничего не записано.
    assert!(w.dlg.requests.borrow().is_empty());
}

#[test]
fn file_menu_load_program() {
    let mut w = Win::new("loadprog", Config::minimal());
    w.pause();
    let gamma = programs_dir().join("gamma.hex");
    w.answer(&gamma);
    w.menu("Файл", "Загрузить программу (.bin/.hex/.ihx)…");
    w.no_error();
    assert_eq!(w.mem(0xC000, 0x83), program_bin("gamma"));

    // .bin без адреса — открывается окно «Магнитофон и файлы» с этим путём.
    let bin = w.path("noaddr.bin");
    std::fs::write(&bin, [1, 2, 3]).unwrap();
    w.answer(&bin);
    w.menu("Файл", "Загрузить и запустить…");
    assert!(w.app().error_popup.is_some());
    w.click("OK");
    assert!(w.app().tape.open);
    assert_eq!(w.app().tape.load_path, bin.display().to_string());
    assert!(w.has("Магнитофон и файлы"));
}

// ------------------------------------------------------------------ магнитофон

#[test]
fn tape_window_files() {
    let mut w = Win::new("tape", Config::minimal());
    w.pause();
    w.key(KeyCode::F8);
    assert!(w.app().tape.open);
    assert!(w.has("Магнитофон и файлы"));
    // Без кассеты кнопки ленты выключены.
    assert!(!w.enabled("▶ Воспроизведение"));

    // Быстрая загрузка .bin по адресу.
    let bin = w.path("prog.bin");
    std::fs::write(&bin, [0x01, 0x02, 0x03, 0x04]).unwrap();
    w.answer(&bin);
    w.click_nth("Обзор…", 0);
    assert_eq!(w.app().tape.load_path, bin.display().to_string());
    // Адрес набирается в поле с клавиатуры.
    let field = w.h.get_by_value("C000");
    w.h.click_widget(&field);
    w.h.key(KeyCode::End);
    for _ in 0..4 {
        w.h.key(KeyCode::Backspace);
    }
    w.h.type_text("C100");
    w.settle();
    assert_eq!(w.app().tape.load_addr, "C100");
    // Набор в поле не нажимает клавиш ЮТ-88.
    assert!(w.app().holds.is_empty());
    w.click("Загрузить");
    w.no_error();
    assert_eq!(w.mem(0xC100, 4), [1, 2, 3, 4]);
    w.m().poke(0xC100, 0);
    w.click("Загрузить и запустить");
    w.no_error();
    assert_eq!(w.m().peek(0xC100), 1);
    assert_eq!(w.m().cpu.pc, 0xC100);

    // Сохранение диапазона в .bin.
    let out = w.path("out.bin");
    w.answer(&out);
    w.click_nth("Обзор…", 1);
    w.app_mut().tape.save_start = "C100".into();
    w.app_mut().tape.save_end = "C103".into();
    w.click("Сохранить");
    w.no_error();
    assert_eq!(std::fs::read(&out).unwrap(), [1, 2, 3, 4]);
    // Конец раньше начала — ошибка.
    w.app_mut().tape.save_end = "C0FF".into();
    w.click("Сохранить");
    assert!(w.app().error_popup.is_some());
    w.click("OK");

    // WAV из .bin в обоих форматах.
    let wav0 = w.path("m0.wav");
    let wavf = w.path("mf.wav");
    w.answer(&bin);
    w.click_nth("Обзор…", 2);
    w.answer(&wav0);
    w.click_nth("Обзор…", 3);
    w.app_mut().tape.gen_addr = "C100".into();
    assert!(!w.app().tape.gen_format_f);
    w.click("Создать WAV");
    w.no_error();
    w.click("МОНИТОР-F");
    assert!(w.app().tape.gen_format_f);
    w.answer(&wavf);
    w.click_nth("Обзор…", 3);
    w.click("Создать WAV");
    w.no_error();
    let data = std::fs::read(&bin).unwrap();
    for (fmt, wav) in [(TapeFormat::Monitor0, &wav0), (TapeFormat::MonitorF, &wavf)] {
        let reference = w.path("ref.wav");
        ut88_core::tape::block_signal(fmt, 0xC100, &data, ut88_core::tape::WAV_RATE)
            .save_wav(&reference)
            .unwrap();
        assert_eq!(
            std::fs::read(wav).unwrap(),
            std::fs::read(&reference).unwrap(),
            "{fmt:?}"
        );
    }
    assert_ne!(std::fs::read(&wav0).unwrap(), std::fs::read(&wavf).unwrap());

    // Блок ровно в 64 Кбайт (раньше в отладочной сборке — переполнение при подсчёте
    // конца блока) и больше 64 Кбайт (ошибка).
    let big = w.path("big.bin");
    std::fs::write(&big, vec![0u8; 0x10000]).unwrap();
    w.answer(&big);
    w.click_nth("Обзор…", 2);
    w.app_mut().tape.gen_wav = w.path("big.wav").display().to_string();
    w.app_mut().tape.gen_addr = "0000".into();
    w.click("Создать WAV");
    w.no_error();
    assert!(w.status().contains("блок 0000–FFFF"), "{}", w.status());
    std::fs::write(&big, vec![0u8; 0x10001]).unwrap();
    w.click("Создать WAV");
    assert!(
        w.app()
            .error_popup
            .as_deref()
            .is_some_and(|e| e.contains("64 Кбайт"))
    );
    w.click("OK");
    w.app_mut().tape.gen_wav = wavf.display().to_string();

    // Вставить WAV как кассету, воспроизведение, стоп, перемотка.
    w.click("Вставить WAV как кассету");
    w.no_error();
    assert!(w.m().tape_inserted());
    assert!(!w.m().tape_playing());
    w.click("▶ Воспроизведение");
    assert!(w.m().tape_playing());
    w.m().run_seconds(0.5);
    assert!(w.m().tape_position().unwrap() > 0.4);
    w.click("⏹ Стоп");
    assert!(!w.m().tape_playing());
    w.click("⏮ В начало");
    assert_eq!(w.m().tape_position(), Some(0.0));
    assert!(w.status().contains("перемотана"));

    // Закрыть окно клавишей F8.
    w.key(KeyCode::F8);
    assert!(!w.app().tape.open);
    assert!(!w.has("Магнитофон и файлы"));
}

#[test]
fn tape_menu_insert_record() {
    let mut w = Win::new("tapemenu", Config::minimal());
    w.pause();
    let wav = w.path("in.wav");
    ut88_core::tape::block_signal(
        TapeFormat::Monitor0,
        0xC000,
        &[1, 2, 3],
        ut88_core::tape::WAV_RATE,
    )
    .save_wav(&wav)
    .unwrap();

    w.answer(&wav);
    w.menu("Магнитофон", "Вставить кассету (WAV)…");
    w.no_error();
    assert!(w.m().tape_inserted());
    w.menu("Магнитофон", "Воспроизведение");
    assert!(w.m().tape_playing());
    w.menu("Магнитофон", "Стоп");
    assert!(!w.m().tape_playing());
    w.m().tape_start();
    w.m().run_seconds(0.3);
    w.menu("Магнитофон", "Перемотать в начало");
    assert_eq!(w.m().tape_position(), Some(0.0));
    w.menu("Магнитофон", "Вынуть кассету");
    assert!(!w.m().tape_inserted());

    // Запись выхода магнитофона в WAV.
    w.menu("Магнитофон", "Начать запись");
    assert!(w.m().tape_recording());
    w.m().run_seconds(0.5);
    let rec = w.path("rec.wav");
    w.answer(&rec);
    w.menu("Магнитофон", "Остановить запись и сохранить WAV…");
    w.no_error();
    assert!(!w.m().tape_recording());
    let bytes = std::fs::read(&rec).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert!(w.status().starts_with("Записано"));
    // Отказ от сохранения записи.
    w.menu("Магнитофон", "Начать запись");
    w.dlg.push(None);
    w.menu("Магнитофон", "Остановить запись и сохранить WAV…");
    assert!(!w.m().tape_recording());
    assert_eq!(w.status(), "Запись отброшена");

    w.menu("Магнитофон", "Магнитофон и файлы… (F8)");
    assert!(w.app().tape.open);
}

// ------------------------------------------------------------------ программы

#[test]
fn programs_window_and_menu() {
    let mut w = Win::new("progs", Config::minimal());
    w.pause();
    w.key(KeyCode::F9);
    assert!(w.has("Программы из журналов"));

    // «Гамма»: загрузить и запустить.
    w.program_row_button("gamma", "Загрузить и запустить");
    w.no_error();
    assert_eq!(w.mem(0xC000, 0x83), program_bin("gamma"));
    assert_eq!(w.m().cpu.pc, 0xC03E);

    // «Лабиринт»: поле подгружается вместе с программой.
    w.program_row_button("labyrinth", "Загрузить");
    w.no_error();
    assert_eq!(w.mem(0xC000, 0x100), program_bin("labyrinth"));
    assert_eq!(w.mem(0xC200, 0x100), program_bin("labyrinth-field"));
    assert!(w.status().contains("labyrinth, labyrinth-field"));
    w.m().poke(0xC200, 0xEE);
    w.program_row_button("labyrinth", "Загрузить и запустить");
    assert_eq!(w.mem(0xC200, 0x100), program_bin("labyrinth-field"));
    assert_eq!(w.m().cpu.pc, 0xC000);

    // Данные к программе отдельной строкой не показываются.
    assert!(!w.has_containing("labyrinth-field — "));

    // В минимальной конфигурации дисплейные программы заблокированы.
    for name in ["display-test", "tetris", "programmator"] {
        assert!(w.program_row_disabled(name, "Загрузить"), "{name}");
        assert!(
            w.program_row_disabled(name, "Загрузить и запустить"),
            "{name}"
        );
    }
    assert!(w.has("нужна полная конфигурация"));
    assert!(!w.program_row_disabled("reaction", "Загрузить"));

    // Закрыть окно и пройти тем же путём через меню «Программы».
    w.key(KeyCode::F9);
    assert!(!w.has("Программы из журналов"));
    w.m().poke(0xC03E, 0);
    w.click("Программы");
    w.click("gamma — Программа «Гамма»");
    w.click("Загрузить и запустить");
    w.no_error();
    assert_eq!(w.mem(0xC000, 0x83), program_bin("gamma"));
    assert_eq!(w.m().cpu.pc, 0xC03E);

    w.click("Программы");
    w.click("tetris — Игра «Тетрис»");
    assert!(w.has("нужна полная конфигурация"));
    assert!(!w.enabled("Загрузить"));
    w.key(KeyCode::Escape);

    w.menu("Программы", "Окно «Программы из журналов» (F9)");
    assert!(w.app().programs_open);

    // Полная конфигурация — «Тетрис» доступен и загружается.
    w.app_mut().programs_open = false;
    w.settle();
    w.menu("Машина", "полная (МОНИТОР-F, часы в нём)");
    w.pause();
    w.key(KeyCode::F9);
    assert!(!w.has("нужна полная конфигурация"));
    w.program_row_button("tetris", "Загрузить");
    w.no_error();
    assert_eq!(w.mem(0x3000, 0x660), program_bin("tetris"));
}

// ------------------------------------------------------------------ меню «Машина», «Вид», «Справка»

#[test]
fn machine_menu() {
    let mut w = Win::new("machine", Config::minimal());
    w.pause();
    assert!(w.has("⏸ пауза"));
    w.click("Машина");
    assert!(w.has("Продолжить (F4)"));
    w.click("Продолжить (F4)");
    assert!(!w.app().paused);
    w.pause();

    // УСТ.
    w.m().jump(0xC123);
    w.menu("Машина", "УСТ — сброс (F2)");
    assert_eq!(w.m().cpu.pc, 0);

    // Холодный старт.
    w.m().run_seconds(0.1);
    assert!(w.m().cycles() > 0);
    w.menu("Машина", "Холодный старт — выкл./вкл. питания (F3)");
    assert_eq!(w.m().cycles(), 0);
    assert_eq!(w.status(), "Холодный старт");

    // Скорость.
    for s in Speed::all() {
        w.menu("Машина", &s.label());
        assert_eq!(w.app().speed, s, "{}", s.label());
        w.key(KeyCode::Escape);
    }

    // Конфигурации.
    for (label, kind) in [
        ("полная (МОНИТОР-F, часы в нём)", 1),
        ("полная + МОНИТОР модуля 1", 2),
        ("CP/M и ОС ЮТ-88 (полная + квазидиск 256К)", 3),
        ("минимальная (модуль 1)", 0),
    ] {
        w.menu("Машина", label);
        assert_eq!(w.app().config_kind(), kind, "{label}");
        w.no_error();
    }
    w.menu("Машина", "полная (МОНИТОР-F, часы в нём)");
    assert!(w.has("Конфигурация: полная"));

    // Тумблеры.
    let irq = w.m().config().clock_interrupts;
    w.menu("Машина", "Прерывания от часов");
    assert_eq!(w.m().config().clock_interrupts, !irq);
    w.key(KeyCode::Escape);
    let rom = w.m().config().monitor_f_rom;
    w.menu("Машина", "ПЗУ МОНИТОРА-F (сегмент F000H–FFFFH)");
    assert_eq!(w.m().config().monitor_f_rom, !rom);
    w.key(KeyCode::Escape);
    w.menu("Машина", "Клавиатура ПК → кнопки модуля 1");
    assert!(w.app().pc_to_pad);
    w.key(KeyCode::Escape);
}

#[test]
fn view_and_help_menus() {
    let mut w = Win::new("view", Config::full());
    // Звук: в тестах «звуковой карты нет» — включение сообщает об этом.
    assert!(w.has("🔇"));
    w.menu("Вид", "Звук");
    assert!(w.app().sound_wanted);
    assert!(
        w.app()
            .error_popup
            .as_deref()
            .is_some_and(|e| e.starts_with("Звук недоступен"))
    );
    w.click("OK");
    w.no_error();
    assert!(w.has("🔇"));

    w.menu("Вид", "белый");
    assert_eq!(w.app().phosphor, Phosphor::White);
    w.key(KeyCode::Escape);
    w.menu("Вид", "зелёный");
    assert_eq!(w.app().phosphor, Phosphor::Green);
    w.key(KeyCode::Escape);
    w.menu("Вид", "Отладчик (F12)");
    assert!(w.app().dbg.open);
    w.app_mut().dbg.open = false;
    w.settle();

    w.menu("Справка", "Клавиши (F11)");
    assert!(w.has("Справка: клавиши"));
    assert!(w.has("Горячие клавиши"));
    w.app_mut().help_open = false;
    w.settle();
    w.menu("Справка", "О программе");
    assert!(w.has("О программе"));
    assert!(w.has("Эмулятор микро-ЭВМ «ЮТ-88»"));
}

/// Громкость меняется ползунком в меню «Вид» (стрелками с клавиатуры).
fn volume_down(w: &mut Win, steps: usize) {
    w.click("Вид");
    let slider =
        w.h.registry()
            .all_by_kind(WidgetKind::Slider)
            .next()
            .cloned()
            .expect("ползунок громкости");
    w.h.ctx.request_focus(slider.id);
    w.h.step();
    for _ in 0..steps {
        w.h.key(KeyCode::ArrowLeft);
        w.settle();
    }
    w.key(KeyCode::Escape);
}

#[test]
fn volume_saved_and_applied_on_next_start() {
    let cfg = tmp_dir("volume-cfg").join("ut-88");
    {
        // Первый запуск: файлов нет — громкость по умолчанию, ничего не записано.
        let mut w = Win::with_settings("volume-1", false, &cfg);
        assert_eq!(w.app().audio.volume(), Settings::default().volume);
        assert!(!cfg.exists());
        volume_down(&mut w, 3);
        let v = w.app().audio.volume();
        assert!(v < Settings::default().volume, "{v}");
        // Записано сразу, без выхода из программы.
        let text = std::fs::read_to_string(cfg.join(settings::VOLUME_FILE)).unwrap();
        assert_eq!(text, settings::volume_text(v));
        // --no-sound не записывает sound.txt.
        assert!(!cfg.join(settings::SOUND_FILE).exists());
    }
    // Второй запуск: громкость из файла.
    let w = Win::with_settings("volume-2", false, &cfg);
    let v =
        settings::parse_volume(&std::fs::read_to_string(cfg.join(settings::VOLUME_FILE)).unwrap())
            .unwrap();
    assert_eq!(w.app().audio.volume(), v);
    drop(w);
    let _ = std::fs::remove_dir_all(cfg.parent().unwrap());
}

#[test]
fn no_sound_key_keeps_saved_sound() {
    let cfg = tmp_dir("nosound-cfg");
    settings::save_sound(&cfg, true).unwrap();
    {
        // --no-sound: звук выключен на этот запуск, sound.txt не тронут.
        let mut w = Win::with_settings("nosound-1", false, &cfg);
        assert!(!w.app().sound_wanted);
        volume_down(&mut w, 1);
        w.app_mut().on_exit();
        assert_eq!(
            std::fs::read_to_string(cfg.join(settings::SOUND_FILE)).unwrap(),
            "on\n"
        );
    }
    // sound.txt = off: запуск без --no-sound начинается без звука (звуковая карта не
    // открывается); включение в меню сразу записывает on.
    settings::save_sound(&cfg, false).unwrap();
    let mut w = Win::with_settings("nosound-2", true, &cfg);
    assert!(!w.app().sound_wanted);
    assert!(!w.app().audio.is_available());
    w.menu("Вид", "Звук");
    w.click("OK");
    assert!(w.app().sound_wanted);
    assert_eq!(
        std::fs::read_to_string(cfg.join(settings::SOUND_FILE)).unwrap(),
        "on\n"
    );
    drop(w);
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn broken_settings_use_defaults() {
    let cfg = tmp_dir("broken-cfg");
    std::fs::write(cfg.join(settings::VOLUME_FILE), "громко").unwrap();
    std::fs::write(cfg.join(settings::SOUND_FILE), "может быть").unwrap();
    let w = Win::with_settings("broken", false, &cfg);
    assert_eq!(w.app().audio.volume(), Settings::default().volume);
    assert!(w.status().contains("по умолчанию"), "{}", w.status());
    drop(w);
    let _ = std::fs::remove_dir_all(&cfg);
}

// ------------------------------------------------------------------ CP/M

/// Прогнать машину, пока на экране не появится `text`.
fn wait_screen(w: &mut Win, text: &str, secs: f64) {
    let limit = w.m().cycles() + (secs * ut88_core::CPU_HZ) as u64;
    loop {
        let t = w.m().screen().map(|s| s.text()).unwrap_or_default();
        if t.contains(text) {
            return;
        }
        assert!(w.m().cycles() < limit, "на экране нет «{text}»:\n{t}");
        // Эмуляцию в тестах гоняем сами: окно считает такты по реальному времени.
        w.m().run_seconds(0.05);
        w.h.step();
    }
}

#[test]
fn cpm_menu_and_disk_window() {
    let mut w = Win::new("cpm", Config::minimal());
    w.menu("CP/M", "Загрузить CP/M-64 с чистым квазидиском");
    w.no_error();
    assert_eq!(w.app().config_kind(), 3);
    assert!(w.status().starts_with("CP/M-64 загружена"));
    wait_screen(&mut w, "A>", 5.0);
    w.pause();

    w.menu("CP/M", "Диск CP/M: файлы…");
    assert!(w.has("Диск CP/M"));
    assert!(w.has("Файлов нет."));

    // Запись файла с ПК.
    let src = w.path("hello.txt");
    std::fs::write(&src, b"HELLO, CP/M\r\n").unwrap();
    w.answer(&src);
    w.click("Записать файл с ПК…");
    w.no_error();
    assert!(w.status().starts_with("HELLO.TXT"));
    assert!(w.has("HELLO.TXT   "));
    assert!(!w.has("Файлов нет."));

    // Сохранение на ПК.
    let back = w.path("back.txt");
    w.answer(&back);
    w.click("Сохранить на ПК…");
    w.no_error();
    let data = std::fs::read(&back).unwrap();
    assert!(data.starts_with(b"HELLO, CP/M\r\n"));
    assert_eq!(w.dlg.requests.borrow().last().unwrap().default, "HELLO.TXT");

    // Удаление.
    w.click("Удалить");
    w.no_error();
    assert!(w.has("Файлов нет."));

    // Пункт меню «Записать файл на диск CP/M…».
    w.answer(&src);
    w.menu("CP/M", "Записать файл на диск CP/M…");
    w.no_error();
    assert!(w.has("HELLO.TXT   "));

    // Образ квазидиска: сохранить, испортить, загрузить.
    let img = w.path("kd.img");
    w.answer(&img);
    w.menu("CP/M", "Сохранить образ квазидиска…");
    w.no_error();
    assert!(img.is_file());
    assert_eq!(w.app().kvazidisk_file.as_deref(), Some(img.as_path()));
    let disk = w.m().cpm_default_disk();
    w.m().cpm_delete_file(disk, "HELLO.TXT").unwrap();
    w.settle();
    assert!(w.has("Файлов нет."));
    w.answer(&img);
    w.menu("CP/M", "Загрузить образ квазидиска…");
    w.no_error();
    w.settle();
    assert!(w.has("HELLO.TXT   "));

    // Перезагрузка CP/M с сохранением файлов.
    w.menu(
        "CP/M",
        "Загрузить CP/M-64 (файлы на квазидиске сохраняются)",
    );
    w.no_error();
    assert!(w.status().contains("файлы на диске сохранены"));
    wait_screen(&mut w, "A>", 5.0);
}

// ------------------------------------------------------------------ панель F2–F12

#[test]
fn fkey_bar_buttons() {
    let mut w = Win::new("fkeys", Config::minimal());

    w.click("F4 Пауза");
    assert!(w.app().paused);
    w.click("F4 Пауза");
    assert!(!w.app().paused);
    w.click("F4 Пауза");

    w.m().jump(0xC123);
    w.click("F2 УСТ");
    assert_eq!(w.m().cpu.pc, 0);

    w.m().run_seconds(0.1);
    w.click("F3 Холод");
    assert_eq!(w.m().cycles(), 0);

    assert_eq!(w.app().speed, Speed::X1);
    w.click("F5 Скор+");
    assert_eq!(w.app().speed, Speed::X1.step(true));
    w.click("Shift+F5 Скор-");
    w.click("Shift+F5 Скор-");
    assert_eq!(w.app().speed, Speed::X1.step(false));

    w.m().poke(0xC000, 0x77);
    w.click("F6 Снимок");
    w.m().poke(0xC000, 0);
    w.click("F7 Загр");
    assert_eq!(w.m().peek(0xC000), 0x77);
    w.no_error();

    for (label, title) in [
        ("F8 Лента", "Магнитофон и файлы"),
        ("F9 Прогр", "Программы из журналов"),
        ("F11 Справка", "Справка: клавиши"),
    ] {
        w.click(label);
        assert!(w.has(title), "{label}");
        w.click(label);
        assert!(!w.has(title), "{label}");
    }
    w.click("F12 Отладчик");
    assert!(w.app().dbg.open);
    w.app_mut().dbg.open = false;
    w.settle();

    // F10 — просьба окну закрыться.
    assert!(!w.app().quit_requested());
    w.click("F10 Выход");
    assert!(w.app().quit_requested(), "F10 не закрывает окно");
}

#[test]
fn fkeys_from_keyboard() {
    let mut w = Win::new("fkeys-kb", Config::minimal());
    w.key(KeyCode::F4);
    assert!(w.app().paused);
    w.m().jump(0xC123);
    w.key(KeyCode::F2);
    assert_eq!(w.m().cpu.pc, 0);
    w.key(KeyCode::F5);
    assert_eq!(w.app().speed, Speed::X1.step(true));
    w.h.key_mods(Modifiers::SHIFT, KeyCode::F5);
    w.settle();
    assert_eq!(w.app().speed, Speed::X1);
    w.key(KeyCode::F11);
    assert!(w.has("Справка: клавиши"));
    w.key(KeyCode::F12);
    assert!(w.app().dbg.open);
    // Нажатие и отпускание — разные кадры; выход просится в кадре нажатия.
    w.h.key_down(KeyCode::F10);
    w.h.step();
    assert!(w.app().quit_requested());
}

// ------------------------------------------------------------------ «максимум»

/// Нажать и отпустить клавишу полной клавиатуры с кодом `code` (прямо в машине).
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
fn kb_accept(m: &mut Machine, code: u8) {
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

fn machine_wait_text(m: &mut Machine, text: &str, secs: f64) {
    let limit = m.cycles() + (secs * CPU_HZ) as u64;
    while !m.screen().unwrap().text().contains(text) {
        assert!(m.cycles() < limit, "на экране нет «{text}»");
        m.run_seconds(0.05);
    }
}

/// МОНИТОР-F ждёт клавишу.
fn bench_full() -> Machine {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    m
}

/// «Тетрис» (programs/tetris.bin с 3000H) идёт на уровне 0.
fn bench_tetris() -> Machine {
    let mut m = bench_full();
    m.load_bin(0x3000, &program_bin("tetris"));
    for b in b"G3000\r" {
        kb_code(&mut m, *b);
    }
    machine_wait_text(&mut m, "ВАШ УРОВЕНЬ(0-7) ?", 3.0);
    kb_accept(&mut m, b'0');
    machine_wait_text(&mut m, "НАЖМИТЕ ЛЮБУЮ КЛАВИШУ", 3.0);
    kb_accept(&mut m, b' ');
    m.run_seconds(0.5);
    m
}

/// Замер «максимума» окна (кадры через `Harness::step` с рисованием, без вывода в окно и
/// вертикальной синхронизации): эмулированные МГц и кадров окна в секунду.
///
/// ```text
/// cargo test --release -p ut88-gui bench_max -- --ignored --nocapture
/// ```
#[test]
#[ignore = "замер скорости, запускать вручную в release"]
fn bench_max_speed() {
    let secs: f64 = std::env::var("UT88_BENCH_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3.0);
    type Load = (&'static str, &'static str, fn() -> Machine);
    let loads: [Load; 2] = [
        ("full", "МОНИТОР-F, простой", bench_full),
        ("tetris", "«Тетрис», уровень 0", bench_tetris),
    ];
    println!("GUI, «максимум», прогон {secs} с");
    println!();
    println!(
        "| {:<8} | {:<20} | {:>9} | {:>12} |",
        "Нагрузка", "Что", "МГц", "Кадров/с"
    );
    println!("|{:-<10}|{:-<22}|{:->11}|{:->14}|", "", "", ":", ":");
    for (name, what, make) in loads {
        let mut w = Win::with_machine(name, make(), Speed::MAX);
        let c0 = w.m().cycles();
        let t0 = Instant::now();
        let mut frames = 0u64;
        while t0.elapsed().as_secs_f64() < secs {
            w.h.step();
            frames += 1;
        }
        let dt = t0.elapsed().as_secs_f64();
        let mhz = (w.m().cycles() - c0) as f64 / dt / 1e6;
        let fps = frames as f64 / dt;
        println!("| {name:<8} | {what:<20} | {mhz:>9.1} | {fps:>12.1} |");
    }
}

/// Программа в C000H: бесконечно переключает выход на магнитофон (порт A1H).
const TOGGLE_A1: [u8; 11] = [
    0x3E, 0x01, 0xD3, 0xA1, // MVI A,1; OUT A1H
    0x3E, 0x00, 0xD3, 0xA1, // MVI A,0; OUT A1H
    0xC3, 0x00, 0xC0, // JMP C000H
];

/// Машина собирает перепады: прогнать программу и посмотреть, накопились ли они.
fn collects_edges(w: &mut Win) -> bool {
    w.m().load_bin(0xC000, &TOGGLE_A1);
    w.m().jump(0xC000);
    w.m().drain_sound_edges();
    w.m().run_cycles(1000);
    !w.m().drain_sound_edges().is_empty()
}

#[test]
fn sound_resumes_after_max_speed() {
    let mut w = Win::new("sound-max", Config::full());
    w.app_mut().fake_sound = true;
    w.settle();
    assert!(w.app().capturing);
    assert!(collects_edges(&mut w));

    // «Максимум»: звук не выводится, перепады не собираются.
    w.menu("Машина", &Speed::MAX.label());
    assert_eq!(w.app().speed, Speed::MAX);
    assert!(!w.app().capturing);
    assert!(!collects_edges(&mut w));
    w.h.step();

    // Обратно ×1: сбор перепадов включён, генератор начат заново с текущего момента.
    let before = w.app().resync_at;
    let ticks = w.m().master_ticks();
    w.menu("Машина", &Speed::X1.label());
    assert_eq!(w.app().speed, Speed::X1);
    assert!(w.app().capturing);
    assert!(w.app().resync_at > before);
    assert!(w.app().resync_at >= ticks);
    assert!(collects_edges(&mut w));

    // Звук выключен — перепады не собираются и на ×1.
    w.app_mut().fake_sound = false;
    w.h.step();
    assert!(!w.app().capturing);
    assert!(!collects_edges(&mut w));
}

#[test]
fn breakpoint_stops_at_max_speed() {
    // C000: INX H; MOV A,H; ORA L; JNZ C000; C006: JMP C000 — C006H раз в 65 536 кругов.
    let code = [0x23, 0x7C, 0xB5, 0xC2, 0x00, 0xC0, 0xC3, 0x00, 0xC0];
    let mut m = bench_full();
    m.load_bin(0xC000, &code);
    m.jump(0xC000);
    let mut w = Win::with_machine("bp-max", m, Speed::MAX);
    // Без точек — крупные порции, машина идёт.
    for _ in 0..3 {
        w.h.step();
    }
    assert!(!w.app().paused);
    assert!(w.app().max_chunk >= FRAME_CYCLES);
    w.app_mut().dbg.breakpoints.push(0xC006);
    for _ in 0..200 {
        w.h.step();
        if w.app().paused {
            break;
        }
    }
    let c = w.app().m.cpu.clone();
    assert!(w.app().paused, "не остановился: {c:?}");
    assert_eq!(w.m().cpu.pc, 0xC006);
    assert_eq!((w.m().cpu.h, w.m().cpu.l), (0, 0));
    assert_eq!(
        w.app().dbg.stop_reason.as_deref(),
        Some("точка останова C006")
    );
}

#[test]
fn screen_and_indicators_redrawn_on_change() {
    let mut w = Win::new("redraw", Config::full());
    w.m().run_seconds(0.5);
    w.h.step();
    w.pause();
    w.settle();
    assert!(w.app().screen_ok);
    // Без изменений изображение не перерисовывается: испорченный буфер остаётся как есть.
    w.app_mut().screen_buf.fill(7);
    w.settle();
    assert!(w.app().screen_buf.iter().all(|&b| b == 7));
    // Изменение экранного ОЗУ — перерисовка.
    w.m().poke(0xE800, 0x41);
    w.settle();
    assert!(w.app().screen_buf.iter().any(|&b| b != 7));
    // Смена люминофора — тоже.
    w.app_mut().screen_buf.fill(7);
    w.app_mut().phosphor = Phosphor::White;
    w.settle();
    assert!(w.app().screen_buf.iter().any(|&b| b != 7));

    // Индикаторы.
    let old = w.app().segs;
    w.m().poke(0x9000, 0x88);
    w.settle();
    assert_ne!(w.app().segs, old);
    let segs = w.m().indicator_segments();
    assert_eq!(w.app().segs, segs);
}

#[test]
fn max_budget_adapts_to_frame_rate() {
    let ms = Duration::from_millis;
    // Окно успевает — бюджет медленно растёт до предела.
    let mut b = MAX_BUDGET_START;
    for _ in 0..1000 {
        b = next_max_budget(b, ms(17));
    }
    assert_eq!(b, MAX_BUDGET_MAX);
    // Пропуск обновления экрана — бюджет падает, но не ниже предела.
    assert_eq!(next_max_budget(b, ms(33)), b - MAX_BUDGET_DOWN);
    for _ in 0..100 {
        b = next_max_budget(b, ms(33));
    }
    assert_eq!(b, MAX_BUDGET_MIN);
}

// ------------------------------------------------------------------ отладчик

/// Окно отладчика на своём стенде — как отдельное окно ОС со своим контекстом.
fn debugger_harness(name: &str, config: Config) -> (Harness<App>, PathBuf) {
    let dir = tmp_dir(name);
    let (mut app, _) = test_app(
        &dir,
        Machine::new(config),
        Speed::X1,
        false,
        dir.join("config"),
    );
    app.dbg.open = true;
    app.paused = true;
    let h = Harness::new(vec2(820.0, 760.0), app, |ctx, app: &mut App| {
        app.debugger_frame(ctx)
    });
    (h, dir)
}

#[test]
fn debugger_window_without_os_window() {
    let (mut h, dir) = debugger_harness("dbg", Config::minimal());
    for label in [
        "Регистры",
        "Точки останова",
        "Дизассемблер",
        "Память",
        "Шаг",
        "Пуск до",
    ] {
        assert!(h.has(label), "нет «{label}»");
    }
    assert!(h.has("▶ Продолжить"));

    // Шаг выполняет одну команду.
    let pc = h.state.m.cpu.pc;
    h.click("Шаг");
    assert_ne!(h.state.m.cpu.pc, pc);
    assert!(h.state.paused);

    // Точка останова по адресу: поле «адрес» + «+».
    let edits: Vec<_> = h
        .registry()
        .all_by_kind(WidgetKind::TextEdit)
        .cloned()
        .collect();
    // Порядок полей: «до адреса», 6 регистров, адрес точки, порт, адрес памяти, запись.
    let bp_field = edits[7].clone();
    h.click_widget(&bp_field);
    h.type_text("0005");
    h.click_nth("+", 0);
    assert_eq!(h.state.dbg.breakpoints, vec![0x0005]);
    assert!(h.has("0005"));
    h.click("✖");
    assert!(h.state.dbg.breakpoints.is_empty());

    // Точка останова по порту: поле, выбор направления в списке, «+».
    let port_field = edits[8].clone();
    h.click_widget(&port_field);
    h.type_text("A0");
    h.click("IN/OUT");
    h.click("IN");
    h.click_nth("+", 1);
    assert_eq!(
        h.state.dbg.port_breaks,
        vec![(0xA0, crate::debugger::PortDir::In)]
    );
    assert!(h.has("IN A0H"));

    // Правка регистра PC при паузе.
    let pc_field = edits[6].clone();
    assert!(pc_field.enabled);
    h.click_widget(&pc_field);
    h.key(KeyCode::End);
    for _ in 0..4 {
        h.key(KeyCode::Backspace);
    }
    h.type_text("C000");
    h.key(KeyCode::Enter);
    h.settle();
    assert_eq!(h.state.m.cpu.pc, 0xC000);

    // Запись байтов в память.
    let addr = edits[10].clone();
    h.click_widget(&addr);
    h.type_text("C100");
    let bytes = edits[11].clone();
    h.click_widget(&bytes);
    h.type_text("3E 55 C9");
    h.click("Записать");
    assert_eq!(
        (0..3)
            .map(|i| h.state.m.peek(0xC100 + i))
            .collect::<Vec<_>>(),
        [0x3E, 0x55, 0xC9]
    );
    assert!(h.has("записано 3 байт с C100"));

    // Продолжить.
    h.click("▶ Продолжить");
    assert!(!h.state.paused);
    assert!(h.has("⏸ Пауза"));
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------------ шрифт

/// Подписи кадра, которые выводятся текстом (значки `ui::icon` рисуются фигурами).
fn frame_texts(h: &Harness<App>, out: &mut Vec<String>) {
    for w in h.registry().widgets {
        out.push(w.label.clone());
        out.extend(w.value.clone());
        out.extend(w.hover_text.clone());
    }
}

/// Все подписи интерфейса (меню, окна, строка состояния, панель F-клавиш, отладчик)
/// рисуются вшитым шрифтом: в DejaVu Sans и Sans Mono есть все их символы, кроме значков,
/// которые `ui::icon` рисует фигурами.
#[test]
fn all_labels_have_glyphs() {
    let mut texts = Vec::new();
    for config in [Config::minimal(), Config::cpm()] {
        let mut w = Win::new("glyphs", config);
        frame_texts(&w.h, &mut texts);
        // Меню, в том числе вложенные меню программ.
        for top in [
            "Машина",
            "Магнитофон",
            "Файл",
            "CP/M",
            "Программы",
            "Вид",
            "Справка",
        ] {
            w.click(top);
            frame_texts(&w.h, &mut texts);
            if top == "Программы" {
                let subs: Vec<_> =
                    w.h.registry()
                        .all_by_kind(WidgetKind::MenuButton)
                        .filter(|m| m.label.contains(" — "))
                        .cloned()
                        .collect();
                for m in subs {
                    w.h.move_to(m.rect.center());
                    w.h.step();
                    frame_texts(&w.h, &mut texts);
                }
            }
            w.key(KeyCode::Escape);
        }
        // Все окна разом, пауза и работа, запись на ленту.
        w.app_mut().tape.open = true;
        w.app_mut().programs_open = true;
        w.app_mut().help_open = true;
        w.app_mut().about_open = true;
        w.app_mut().cpm_open = true;
        w.app_mut().error_popup = Some("Ошибка: «пример» — №1…".into());
        w.m().tape_record_start();
        w.settle();
        frame_texts(&w.h, &mut texts);
        w.pause();
        frame_texts(&w.h, &mut texts);
    }
    let (h, dir) = debugger_harness("glyphs-dbg", Config::minimal());
    frame_texts(&h, &mut texts);
    let _ = std::fs::remove_dir_all(dir);

    texts.sort();
    texts.dedup();
    let mut bad = Vec::new();
    for t in &texts {
        let plain: String = t
            .chars()
            .filter(|&c| crate::ui::Icon::from_char(c).is_none())
            .collect();
        for kind in [FontKind::Sans, FontKind::Mono] {
            let miss = missing_glyphs(&plain, kind);
            if !miss.is_empty() {
                bad.push(format!("{kind:?}: «{t}» — {miss:?}"));
            }
        }
    }
    assert!(bad.is_empty(), "нет в шрифте:\n{}", bad.join("\n"));
}

// ------------------------------------------------------------------ кэш-слой рисунка

/// Кадр окна (в пределах рисунка компьютера) совпадает с прямым рисованием всего
/// рисунка с теми же нажатыми клавишами, индикаторами и экраном.
fn assert_picture_matches_direct(w: &Win, what: &str) {
    let app = w.app();
    let layout = app.pic.layout.expect("рисунок нарисован");
    let rect = layout.rect;
    let (cw, ch, ppp) = w.h.ctx.with_canvas(|c| (c.width(), c.height(), c.scale()));
    let mut direct = Canvas::new(cw, ch);
    direct.set_scale(ppp);
    direct.clear(w.h.ctx.theme().panel_fill);
    direct.push_clip(rect);
    let screen = app.screen_ok.then_some((
        app.screen_buf.as_slice(),
        screen::WIDTH as u32,
        screen::HEIGHT as u32,
    ));
    picture::computer(
        &mut direct,
        rect,
        app.has_display(),
        app.segs,
        screen,
        app.phosphor.glow(),
        &PressedSet(&app.drawn_down()),
        &mut Hits::default(),
    );
    direct.pop_clip();
    let inner = rect.shrink(1.0);
    let (x0, y0) = (
        (inner.min.x * ppp).ceil() as u32,
        (inner.min.y * ppp).ceil() as u32,
    );
    let (x1, y1) = ((inner.max.x * ppp) as u32, (inner.max.y * ppp) as u32);
    for y in y0..y1 {
        for x in x0..x1 {
            let f = w.h.pixel(x, y);
            let d = direct.pixel(x, y);
            assert_eq!(f, d, "{what}: пиксель ({x}, {y})");
        }
    }
}

#[test]
fn picture_layer_redraws_pressed_key_only() {
    for (config, hit) in [
        (Config::minimal(), Hit::Pad(PadKey::Hex(5))),
        (Config::full(), Hit::Kb(KbKey::for_code(b'A').unwrap().0)),
    ] {
        let mut w = Win::new("layer", config);
        w.settle();
        assert_picture_matches_direct(&w, "до нажатия");
        let full0 = w.app().pic.full_redraws;
        let keys0 = w.app().pic.key_redraws;

        // Щелчок мышью по клавише: перерисована только она.
        let zone = w
            .app()
            .pic
            .hits
            .zones
            .iter()
            .find(|z| z.1 == hit)
            .expect("зона клавиши")
            .0;
        w.h.click_at(zone.center());
        w.h.step();
        assert!(w.app().drawn_down().contains(&hit), "{hit:?} не нажата");
        assert!(w.app().pic.down.contains(&hit));
        assert_eq!(w.app().pic.full_redraws, full0, "слой перерисован целиком");
        assert!(w.app().pic.key_redraws > keys0);
        assert_picture_matches_direct(&w, "нажата");

        // Время удержания вышло — клавиша отпущена и перерисована.
        w.m().run_seconds(0.5);
        w.h.run(3);
        assert!(!w.app().drawn_down().contains(&hit), "{hit:?} не отпущена");
        assert!(!w.app().pic.down.contains(&hit));
        assert_eq!(w.app().pic.full_redraws, full0);
        assert_picture_matches_direct(&w, "отпущена");

        // Смена люминофора — слой целиком.
        w.app_mut().phosphor = Phosphor::White;
        w.h.step();
        assert_eq!(w.app().pic.full_redraws, full0 + 1);
        assert_picture_matches_direct(&w, "белый люминофор");
    }
}

/// Нажатие мыши мимо открытого меню над рисунком только закрывает меню, клавиша ЮТ-88
/// не нажимается; над окном — тоже не нажимается.
#[test]
fn mouse_on_picture_blocked_by_menu_and_window() {
    let mut w = Win::new("mouse-block", Config::minimal());
    w.pause();
    let hit = Hit::Pad(PadKey::Hex(0));
    let zone = w
        .app()
        .pic
        .hits
        .zones
        .iter()
        .find(|z| z.1 == hit)
        .unwrap()
        .0;
    w.click("Машина");
    assert!(w.h.ctx.any_popup_open());
    w.h.click_at(zone.center());
    assert!(!w.h.ctx.any_popup_open());
    assert!(w.app().holds.is_empty(), "нажатие прошло сквозь меню");
    // Без меню щелчок нажимает клавишу.
    w.h.click_at(zone.center());
    assert!(w.app().holds.contains_key(&hit));
}
