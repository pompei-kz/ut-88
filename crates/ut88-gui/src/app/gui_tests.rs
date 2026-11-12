//! Тесты окна через `egui_kittest`: виджеты ищутся по подписи (AccessKit) и «щёлкаются»
//! мышью внутри тестового контекста egui — настоящее окно не открывается, события в
//! X-экран не посылаются. Файловые диалоги подменены [`ScriptedDialogs`], звук —
//! [`Audio::silent`].

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ut88_audio::Audio;
use ut88_core::{Config, Machine};

use super::*;
use crate::dialogs::ScriptedDialogs;

/// Окно приложения в тестовом контексте.
struct Win {
    h: Harness<'static, App>,
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

impl Win {
    fn new(name: &str, config: Config) -> Self {
        let dir = tmp_dir(name);
        let dlg = ScriptedDialogs::default();
        let opts = GuiOptions {
            machine: Machine::new(config),
            speed: Speed::X1,
            sound: false,
            programs_dir: Some(programs_dir()),
            kvazidisk_file: None,
        };
        let d2 = dlg.clone();
        let quick = dir.join("quick.snap");
        let h = Harness::builder()
            .with_size(egui::vec2(1280.0, 900.0))
            .build_eframe(move |_cc| {
                let mut app = App::new(opts);
                app.set_file_dialogs(Box::new(d2));
                app.set_quick_snapshot(quick);
                app.set_audio_opener(Audio::silent);
                app
            });
        let mut w = Self { h, dlg, dir };
        w.h.step();
        w
    }

    fn app(&self) -> &App {
        self.h.state()
    }

    fn app_mut(&mut self) -> &mut App {
        self.h.state_mut()
    }

    fn m(&mut self) -> &mut Machine {
        &mut self.h.state_mut().m
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Следующий файловый диалог вернёт этот путь.
    fn answer(&self, p: &Path) {
        self.dlg.push(Some(p.to_path_buf()));
    }

    /// Обработать очередь событий (кадр на событие) и нарисовать ещё кадр.
    fn settle(&mut self) {
        self.h.step();
        self.h.step();
    }

    /// Щёлкнуть мышью по единственному виджету с подписью `label`.
    fn click(&mut self, label: &str) {
        self.h.get_by_label(label).click();
        self.settle();
    }

    /// Щёлкнуть `n`-й (по порядку дерева) виджет с подписью `label`.
    fn click_nth(&mut self, label: &str, n: usize) {
        self.h
            .get_all_by_label(label)
            .nth(n)
            .unwrap_or_else(|| panic!("нет {n}-го «{label}»"))
            .click();
        self.settle();
    }

    /// Пункт меню: `top` → `item`.
    fn menu(&mut self, top: &str, item: &str) {
        self.click(top);
        self.click(item);
    }

    fn key(&mut self, k: egui::Key) {
        self.h.key_press(k);
        self.settle();
    }

    fn has(&self, label: &str) -> bool {
        self.h.query_all_by_label(label).next().is_some()
    }

    fn has_containing(&self, part: &str) -> bool {
        self.h.query_all_by_label_contains(part).next().is_some()
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
    fn program_row_button(&mut self, name: &str, button: &str) {
        let prefix = format!("{name} — ");
        let y = self
            .h
            .get_by(|n| {
                n.role() == egui::accesskit::Role::Label
                    && n.value().is_some_and(|l| l.starts_with(&prefix))
            })
            .rect()
            .center()
            .y;
        let node = self
            .h
            .get_all_by_label(button)
            .find(|b| (b.rect().center().y - y).abs() < 4.0)
            .unwrap_or_else(|| panic!("нет «{button}» в строке {name}"));
        assert!(
            !node.accesskit_node().is_disabled(),
            "{name}: «{button}» выключена"
        );
        node.click();
        self.settle();
    }

    fn program_row_disabled(&self, name: &str, button: &str) -> bool {
        let prefix = format!("{name} — ");
        let y = self
            .h
            .get_by(|n| {
                n.role() == egui::accesskit::Role::Label
                    && n.value().is_some_and(|l| l.starts_with(&prefix))
            })
            .rect()
            .center()
            .y;
        self.h
            .get_all_by_label(button)
            .find(|b| (b.rect().center().y - y).abs() < 4.0)
            .unwrap_or_else(|| panic!("нет «{button}» в строке {name}"))
            .accesskit_node()
            .is_disabled()
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
    w.key(egui::Key::F6);
    w.no_error();
    assert!(quick.is_file());
    w.m().poke(0xC010, 0x00);
    w.key(egui::Key::F7);
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
    w.key(egui::Key::F8);
    assert!(w.app().tape.open);
    assert!(w.has("Магнитофон и файлы"));
    // Без кассеты кнопки ленты выключены.
    assert!(
        w.h.get_by_label("▶ Воспроизведение")
            .accesskit_node()
            .is_disabled()
    );

    // Быстрая загрузка .bin по адресу.
    let bin = w.path("prog.bin");
    std::fs::write(&bin, [0x01, 0x02, 0x03, 0x04]).unwrap();
    w.answer(&bin);
    w.click_nth("Обзор…", 0);
    assert_eq!(w.app().tape.load_path, bin.display().to_string());
    // Адрес набирается в поле с клавиатуры.
    let field = w.h.get_all_by_value("C000").next().unwrap();
    field.click();
    w.settle();
    w.h.key_press(egui::Key::End);
    for _ in 0..4 {
        w.h.key_press(egui::Key::Backspace);
    }
    w.h.event(egui::Event::Text("C100".into()));
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
    w.key(egui::Key::F8);
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
    w.key(egui::Key::F9);
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
    w.key(egui::Key::F9);
    assert!(!w.has("Программы из журналов"));
    w.m().poke(0xC03E, 0);
    w.click("Программы");
    w.click("gamma — Программа «Гамма» ⏵");
    w.click("Загрузить и запустить");
    w.no_error();
    assert_eq!(w.mem(0xC000, 0x83), program_bin("gamma"));
    assert_eq!(w.m().cpu.pc, 0xC03E);

    w.click("Программы");
    w.click("tetris — Игра «Тетрис» ⏵");
    assert!(w.has("нужна полная конфигурация"));
    assert!(w.h.get_by_label("Загрузить").accesskit_node().is_disabled());
    w.key(egui::Key::Escape);

    w.menu("Программы", "Окно «Программы из журналов» (F9)");
    assert!(w.app().programs_open);

    // Полная конфигурация — «Тетрис» доступен и загружается.
    w.app_mut().programs_open = false;
    w.settle();
    w.menu("Машина", "полная (МОНИТОР-F, часы в нём)");
    w.pause();
    w.key(egui::Key::F9);
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
        w.key(egui::Key::Escape);
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
    w.key(egui::Key::Escape);
    let rom = w.m().config().monitor_f_rom;
    w.menu("Машина", "ПЗУ МОНИТОРА-F (сегмент F000H–FFFFH)");
    assert_eq!(w.m().config().monitor_f_rom, !rom);
    w.key(egui::Key::Escape);
    w.menu("Машина", "Клавиатура ПК → кнопки модуля 1");
    assert!(w.app().pc_to_pad);
    w.key(egui::Key::Escape);
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
    w.key(egui::Key::Escape);
    w.menu("Вид", "зелёный");
    assert_eq!(w.app().phosphor, Phosphor::Green);
    w.key(egui::Key::Escape);
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

    // F10 — команда окну закрыться.
    w.h.get_by_label("F10 Выход").click();
    w.h.step();
    let close =
        w.h.output()
            .viewport_output
            .values()
            .any(|v| v.commands.contains(&egui::ViewportCommand::Close));
    assert!(close, "F10 не закрывает окно");
}

#[test]
fn fkeys_from_keyboard() {
    let mut w = Win::new("fkeys-kb", Config::minimal());
    w.key(egui::Key::F4);
    assert!(w.app().paused);
    w.m().jump(0xC123);
    w.key(egui::Key::F2);
    assert_eq!(w.m().cpu.pc, 0);
    w.key(egui::Key::F5);
    assert_eq!(w.app().speed, Speed::X1.step(true));
    w.h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F5);
    w.settle();
    assert_eq!(w.app().speed, Speed::X1);
    w.key(egui::Key::F11);
    assert!(w.has("Справка: клавиши"));
    w.key(egui::Key::F12);
    assert!(w.app().dbg.open);
    // Нажатие и отпускание — разные кадры; команда уходит в кадре нажатия.
    w.h.key_down(egui::Key::F10);
    w.h.step();
    assert!(
        w.h.output()
            .viewport_output
            .values()
            .any(|v| v.commands.contains(&egui::ViewportCommand::Close))
    );
}
