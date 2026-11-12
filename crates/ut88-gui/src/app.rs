//! Окно эмулятора: рисунок компьютера, меню, синхронизация с реальным временем.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions};
use ut88_audio::Audio;
use ut88_core::{CPU_HZ, Config, FRAME_CYCLES, KbKey, Machine, PadKey, TapeFormat, screen};

use crate::debugger::{DbgAction, Debugger};
use crate::draw::{self, Hit, Hits, Xf};
use crate::keymap;
use crate::loader::{self, ProgramInfo, parse_hex_u16};
use crate::{GuiOptions, Speed};

/// Минимальное время удержания клавиши после щелчка мышью, с (эмулированного времени).
const MOUSE_MIN_HOLD: f64 = 0.3;
/// Минимальное время удержания клавиши ПК, с (нажатие и отпускание могли прийти в одном
/// кадре окна).
const PC_MIN_HOLD: f64 = 0.06;
/// Файл быстрого снимка (F6/F7).
pub const QUICK_SNAPSHOT: &str = "ut88-quick.snap";

/// Цвет люминофора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phosphor {
    /// Зелёный.
    Green,
    /// Белый.
    White,
}

impl Phosphor {
    fn colors(self) -> ([u8; 4], [u8; 4]) {
        match self {
            Phosphor::Green => ([0x66, 0xFF, 0x88, 0xFF], [0x06, 0x14, 0x08, 0xFF]),
            Phosphor::White => ([0xE8, 0xEC, 0xF0, 0xFF], [0x10, 0x10, 0x12, 0xFF]),
        }
    }
}

/// Кто держит клавишу.
#[derive(Debug, Default, Clone, Copy)]
struct Hold {
    /// Клавиша ПК.
    pc: bool,
    /// Кнопка мыши.
    mouse: bool,
    /// Модификатор, «залипший» после щелчка мышью.
    sticky: bool,
    /// Держать не меньше, чем до этого такта.
    until: u64,
    /// Нажатие передано машине.
    applied: bool,
}

/// Окно «Магнитофон и файлы».
#[derive(Default)]
struct TapeUi {
    open: bool,
    load_path: String,
    load_addr: String,
    save_path: String,
    save_start: String,
    save_end: String,
    gen_bin: String,
    gen_addr: String,
    gen_wav: String,
    gen_format_f: bool,
}

/// Приложение.
pub struct App {
    m: Machine,
    speed: Speed,
    paused: bool,
    last_tick: Instant,
    debt: f64,
    /// Фактическая скорость (доля реального времени), сглаженная.
    actual_speed: f64,
    audio: Audio,
    sound_wanted: bool,
    holds: HashMap<Hit, Hold>,
    mouse_down: Option<Hit>,
    /// Клавиатура ПК управляет модулем 1 даже в полной конфигурации.
    pc_to_pad: bool,
    screen_tex: Option<TextureHandle>,
    screen_buf: Vec<u8>,
    phosphor: Phosphor,
    dbg: Debugger,
    help_open: bool,
    about_open: bool,
    programs_open: bool,
    tape: TapeUi,
    programs_dir: Option<PathBuf>,
    programs: Vec<ProgramInfo>,
    status: String,
    status_time: Instant,
    error_popup: Option<String>,
}

impl App {
    /// Создать приложение.
    pub fn new(opts: GuiOptions) -> Self {
        let audio = if opts.sound {
            Audio::new()
        } else {
            Audio::silent()
        };
        let programs_dir = opts.programs_dir.or_else(loader::find_programs_dir);
        let programs = programs_dir
            .as_deref()
            .map(loader::load_programs_table)
            .unwrap_or_default();
        let mut app = Self {
            m: opts.machine,
            speed: opts.speed,
            paused: false,
            last_tick: Instant::now(),
            debt: 0.0,
            actual_speed: 1.0,
            sound_wanted: opts.sound,
            audio,
            holds: HashMap::new(),
            mouse_down: None,
            pc_to_pad: false,
            screen_tex: None,
            screen_buf: vec![0; screen::WIDTH * screen::HEIGHT * 4],
            phosphor: Phosphor::Green,
            dbg: Debugger::default(),
            help_open: false,
            about_open: false,
            programs_open: false,
            tape: TapeUi {
                load_addr: "C000".into(),
                save_start: "C000".into(),
                save_end: "C0FF".into(),
                gen_addr: "C000".into(),
                ..Default::default()
            },
            programs_dir,
            programs,
            status: String::new(),
            status_time: Instant::now(),
            error_popup: None,
        };
        if let Some(e) = app.audio.error()
            && opts.sound
        {
            app.say(format!("Звук недоступен: {e}"));
        }
        app.sync_sound_capture();
        app
    }

    fn has_display(&self) -> bool {
        self.m.config().display.is_some()
    }

    fn say(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.status_time = Instant::now();
    }

    fn fail(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.say(s.clone());
        self.error_popup = Some(s);
    }

    fn sync_sound_capture(&mut self) {
        let on = self.audio.is_available() && self.audio.enabled();
        self.m.set_sound_capture(on);
        self.audio
            .resync(self.m.master_ticks(), self.m.tape_out_level());
    }

    /// Заменить машину (холодный старт, снимок, смена конфигурации).
    fn replace_machine(&mut self, m: Machine) {
        self.m = m;
        self.holds.clear();
        self.mouse_down = None;
        self.debt = 0.0;
        self.dbg.run_to = None;
        self.dbg.stop_reason = None;
        self.dbg.mark_cycles = self.m.cycles();
        self.sync_sound_capture();
    }

    // ----- клавиши -----

    fn hold_mut(&mut self, h: Hit) -> &mut Hold {
        self.holds.entry(h).or_default()
    }

    fn press(&mut self, h: Hit, mouse: bool) {
        if h == Hit::Pad(PadKey::Reset) {
            self.m.pad_down(PadKey::Reset);
            self.say("УСТ — сброс");
            return;
        }
        let min = if mouse { MOUSE_MIN_HOLD } else { PC_MIN_HOLD };
        let until = self.m.cycles() + (min * CPU_HZ) as u64;
        let hold = self.hold_mut(h);
        if mouse {
            hold.mouse = true;
        } else {
            hold.pc = true;
        }
        hold.until = hold.until.max(until);
    }

    fn release(&mut self, h: Hit, mouse: bool) {
        if let Some(hold) = self.holds.get_mut(&h) {
            if mouse {
                hold.mouse = false;
            } else {
                hold.pc = false;
            }
        }
    }

    /// Передать машине изменения в нажатых клавишах.
    fn apply_holds(&mut self) {
        let now = self.m.cycles();
        let mut done = Vec::new();
        for (&h, hold) in self.holds.iter_mut() {
            let active = hold.pc || hold.mouse || hold.sticky || now < hold.until;
            if active && !hold.applied {
                match h {
                    Hit::Pad(k) => self.m.pad_down(k),
                    Hit::Kb(k) => self.m.kb_down(k),
                }
                hold.applied = true;
            } else if !active {
                if hold.applied {
                    match h {
                        Hit::Pad(k) => self.m.pad_up(k),
                        Hit::Kb(k) => self.m.kb_up(k),
                    }
                }
                done.push(h);
            }
        }
        for h in done {
            self.holds.remove(&h);
        }
    }

    fn release_all_pc(&mut self) {
        for hold in self.holds.values_mut() {
            hold.pc = false;
        }
    }

    fn is_modifier(h: Hit) -> bool {
        matches!(h, Hit::Kb(KbKey::Rus | KbKey::Us | KbKey::Ss))
    }

    /// Обработка мыши по зонам клавиш.
    fn mouse_input(&mut self, ui: &egui::Ui, hits: &Hits) {
        let (pos, down, pressed) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_down(),
                i.pointer.primary_pressed(),
            )
        });
        let over = pos.and_then(|p| {
            hits.zones
                .iter()
                .rev()
                .find(|z| z.0.contains(p))
                .map(|z| z.1)
        });
        if over.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if pressed
            && let Some(h) = over
            && ui
                .ctx()
                .layer_id_at(pos.unwrap_or_default())
                .is_none_or(|l| l.order == egui::Order::Background)
        {
            if Self::is_modifier(h) {
                // РУС/УС/СС мышью — «залипают» до следующей клавиши.
                let hold = self.hold_mut(h);
                hold.sticky = !hold.sticky;
            } else {
                self.press(h, true);
                self.mouse_down = Some(h);
            }
        }
        if !down && let Some(h) = self.mouse_down.take() {
            self.release(h, true);
            // Залипшие модификаторы отпускаются вместе с клавишей.
            let until = self.holds.get(&h).map_or(0, |x| x.until);
            for (k, hold) in self.holds.iter_mut() {
                if Self::is_modifier(*k) && hold.sticky {
                    hold.sticky = false;
                    hold.until = hold.until.max(until);
                }
            }
        }
    }

    /// Клавиатура ПК.
    fn keyboard_input(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        let typing = ctx.wants_keyboard_input();
        let to_kb = self.has_display() && !self.pc_to_pad;
        for ev in events {
            match ev {
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } => {
                    let key = physical_key.unwrap_or(key);
                    if pressed && !repeat && self.hotkey(ctx, key, modifiers.shift) {
                        continue;
                    }
                    let hit = if to_kb {
                        keymap::kb_key(key).map(Hit::Kb)
                    } else {
                        keymap::pad_key(key).map(Hit::Pad)
                    };
                    let Some(h) = hit else { continue };
                    if pressed {
                        if !repeat && !typing {
                            self.press(h, false);
                        }
                    } else {
                        self.release(h, false);
                    }
                }
                // Ctrl+C/X/V egui превращает в команды буфера обмена и не присылает
                // нажатие клавиши (отпускание приходит обычным событием) — а это УС+C,
                // которым, например, заканчивают «Тетрис».
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) if !typing => {
                    let key = match ev {
                        egui::Event::Copy => egui::Key::C,
                        egui::Event::Cut => egui::Key::X,
                        _ => egui::Key::V,
                    };
                    let hit = if to_kb {
                        keymap::kb_key(key).map(Hit::Kb)
                    } else {
                        keymap::pad_key(key).map(Hit::Pad)
                    };
                    if let Some(h) = hit {
                        self.press(h, false);
                    }
                }
                egui::Event::WindowFocused(false) => self.release_all_pc(),
                _ => {}
            }
        }
        // Модификаторы ПК → РУС/УС/СС.
        if to_kb {
            let mods = ctx.input(|i| i.modifiers);
            for (on, k) in [
                (mods.shift && !typing, KbKey::Ss),
                (mods.ctrl && !typing, KbKey::Us),
                (mods.alt && !typing, KbKey::Rus),
            ] {
                let h = Hit::Kb(k);
                let held = self.holds.get(&h).is_some_and(|x| x.pc);
                if on && !held {
                    self.press(h, false);
                } else if !on && held {
                    self.release(h, false);
                }
            }
        }
    }

    /// Горячие клавиши — общая схема с TUI (см. [`keymap::HOTKEYS`]).
    fn hotkey(&mut self, ctx: &egui::Context, key: egui::Key, shift: bool) -> bool {
        match key {
            egui::Key::F2 => self.press(Hit::Pad(PadKey::Reset), false),
            egui::Key::F3 => {
                let c = self.m.config().clone();
                self.cold_start(c);
            }
            egui::Key::F4 => self.toggle_pause(),
            egui::Key::F5 => self.change_speed(!shift),
            // Некоторые раскладки присылают Shift+F5 как F17.
            egui::Key::F17 => self.change_speed(false),
            egui::Key::F6 => self.save_snapshot(Path::new(QUICK_SNAPSHOT)),
            egui::Key::F7 => self.load_snapshot(Path::new(QUICK_SNAPSHOT)),
            egui::Key::F8 => self.tape.open = !self.tape.open,
            egui::Key::F9 => self.programs_open = !self.programs_open,
            egui::Key::F10 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            egui::Key::F11 => self.help_open = !self.help_open,
            egui::Key::F12 => self.dbg.open = !self.dbg.open,
            _ => return false,
        }
        ctx.request_repaint();
        true
    }

    fn change_speed(&mut self, up: bool) {
        self.speed = self.speed.step(up);
        self.debt = 0.0;
        self.say(format!("Скорость: {}", self.speed.label()));
    }

    // ----- магнитофон -----

    fn insert_tape(&mut self, p: &Path) -> anyhow::Result<String> {
        self.m.tape_insert_wav(p)?;
        Ok(format!(
            "Кассета {} вставлена; лента тронется, когда программа начнёт читать \
             (директива A / I), или кнопкой «Воспроизведение»",
            p.display()
        ))
    }

    fn tape_play_toggle(&mut self) {
        if !self.m.tape_inserted() {
            self.fail("Кассета не вставлена (Магнитофон → Вставить кассету)");
        } else if self.m.tape_playing() {
            self.m.tape_pause();
            self.say("Лента остановлена");
        } else {
            self.m.tape_start();
            self.say("Воспроизведение");
        }
    }

    fn tape_rewind(&mut self) {
        if self.m.tape_rewind() {
            self.say("Лента перемотана в начало");
        }
    }

    /// Состояние магнитофона для строки состояния и окна.
    fn tape_status(&self) -> String {
        let mut s = match self.m.tape_position() {
            None => "кассеты нет".to_string(),
            Some(p) if self.m.tape_finished() => format!("лента кончилась ({p:.1} с)"),
            Some(p) if self.m.tape_playing() => format!("▶ лента {p:.1} с"),
            Some(p) if self.m.tape_auto_start() => {
                format!("⏹ лента {p:.1} с — тронется при чтении")
            }
            Some(p) => format!("⏹ лента {p:.1} с — стоп"),
        };
        if self.m.tape_recording() {
            s += " ● запись";
        }
        s
    }

    fn tape_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let ins = self.m.tape_inserted();
            let label = if self.m.tape_playing() {
                "⏹ Стоп"
            } else {
                "▶ Воспроизведение"
            };
            if ui.add_enabled(ins, egui::Button::new(label)).clicked() {
                self.tape_play_toggle();
            }
            if ui
                .add_enabled(ins, egui::Button::new("⏮ В начало"))
                .clicked()
            {
                self.tape_rewind();
            }
            ui.label(self.tape_status());
        });
    }

    // ----- эмуляция -----

    fn toggle_pause(&mut self) {
        if self.paused {
            self.resume();
        } else {
            self.pause("пауза");
        }
    }

    fn pause(&mut self, why: &str) {
        self.paused = true;
        self.debt = 0.0;
        self.dbg.mark_cycles = self.m.cycles();
        self.say(format!("Остановлено: {why}"));
    }

    fn resume(&mut self) {
        self.paused = false;
        self.dbg.skip_once = true;
        self.dbg.stop_reason = None;
        self.last_tick = Instant::now();
        self.audio
            .resync(self.m.master_ticks(), self.m.tape_out_level());
        self.say("Выполнение");
    }

    /// Прогнать эмуляцию за прошедшее реальное время.
    fn emulate(&mut self) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_tick)
            .min(Duration::from_millis(100));
        self.last_tick = now;
        if self.paused {
            self.apply_holds();
            return;
        }
        let start_cycles = self.m.cycles();
        let wall_start = Instant::now();
        let budget = Duration::from_millis(30);
        match self.speed.factor() {
            Some(k) => {
                self.debt += dt.as_secs_f64() * CPU_HZ * k;
                while self.debt > 0.0 {
                    self.apply_holds();
                    let n = (self.debt as u64).clamp(1, FRAME_CYCLES);
                    let before = self.m.cycles();
                    let stop = self.dbg.run(&mut self.m, n);
                    self.debt -= (self.m.cycles() - before) as f64;
                    if let Some(r) = stop {
                        self.pause(&r);
                        self.dbg.open = true;
                        break;
                    }
                    if wall_start.elapsed() > budget {
                        // Не успеваем — отбрасываем долг, чтобы окно не зависло.
                        self.debt = 0.0;
                        break;
                    }
                }
            }
            None => {
                while wall_start.elapsed() < budget {
                    self.apply_holds();
                    if let Some(r) = self.dbg.run(&mut self.m, FRAME_CYCLES) {
                        self.pause(&r);
                        self.dbg.open = true;
                        break;
                    }
                }
            }
        }
        self.apply_holds();
        let ran = (self.m.cycles() - start_cycles) as f64;
        if dt.as_secs_f64() > 0.0 {
            let inst = ran / CPU_HZ / dt.as_secs_f64();
            self.actual_speed = self.actual_speed * 0.9 + inst * 0.1;
        }
        let edges = self.m.drain_sound_edges();
        if self.speed == Speed::MAX {
            self.audio
                .resync(self.m.master_ticks(), self.m.tape_out_level());
        } else {
            self.audio.push_edges(&edges, self.m.master_ticks());
        }
    }

    fn do_debug_action(&mut self, a: DbgAction) {
        match a {
            DbgAction::None => {}
            DbgAction::Pause => self.pause("отладчик"),
            DbgAction::Continue => self.resume(),
            DbgAction::Step => {
                if !self.paused {
                    self.pause("шаг");
                }
                self.m.step();
                self.dbg.stop_reason = None;
            }
            DbgAction::StepOver => {
                if !self.paused {
                    self.pause("шаг");
                }
                if self.dbg.step_over(&mut self.m) {
                    self.paused = false;
                    self.last_tick = Instant::now();
                }
            }
            DbgAction::RunTo(a) => {
                self.dbg.run_to = Some(a);
                self.resume();
            }
        }
    }

    // ----- файлы -----

    fn save_snapshot(&mut self, path: &Path) {
        match self.m.save_snapshot(path) {
            Ok(()) => self.say(format!("Снимок сохранён: {}", path.display())),
            Err(e) => self.fail(format!("Снимок не сохранён: {e}")),
        }
    }

    fn load_snapshot(&mut self, path: &Path) {
        let r = std::fs::read(path)
            .map_err(ut88_core::SnapshotError::from)
            .and_then(|d| Machine::parse_snapshot(&d))
            .and_then(|s| Machine::from_snapshot(&s));
        match r {
            Ok(m) => {
                self.replace_machine(m);
                self.say(format!("Снимок загружен: {}", path.display()));
            }
            Err(e) => self.fail(format!("Снимок не загружен ({}): {e}", path.display())),
        }
    }

    fn cold_start(&mut self, config: Config) {
        let m = Machine::new(config);
        self.replace_machine(m);
        self.say("Холодный старт");
    }

    fn switch_config(&mut self, mut c: Config) {
        let cur = self.m.config();
        c.monitor_rom = cur.monitor_rom.clone();
        c.calculator_rom = cur.calculator_rom.clone();
        self.cold_start(c);
    }

    fn load_program(&mut self, p: &ProgramInfo, run: bool) {
        let Some(dir) = self.programs_dir.clone() else {
            return;
        };
        if p.needs_display() && !self.has_display() {
            self.fail(format!(
                "«{}» требует дисплейного модуля: переключите конфигурацию на полную",
                p.title
            ));
            return;
        }
        let mut loaded = vec![p.clone()];
        loaded.extend(
            self.programs
                .iter()
                .filter(|q| q.data_for() == Some(&p.name))
                .cloned(),
        );
        for q in &loaded {
            let path = dir.join(format!("{}.bin", q.name));
            match loader::load_file(&path, Some(q.load), &self.programs) {
                Ok(img) => img.load_into(&mut self.m),
                Err(e) => {
                    self.fail(e.to_string());
                    return;
                }
            }
        }
        let names: Vec<String> = loaded.iter().map(|q| q.name.clone()).collect();
        if run && let Some(s) = p.start {
            self.m.release_all_keys();
            self.m.jump(s);
            self.say(format!(
                "Загружено и запущено с {s:04X}: {}",
                names.join(", ")
            ));
        } else {
            self.say(format!(
                "Загружено {} с {:04X}{} — не запущено",
                names.join(", "),
                p.load,
                p.start
                    .map(|s| format!(", адрес пуска {s:04X}"))
                    .unwrap_or_default()
            ));
        }
    }

    fn load_any_file(&mut self, path: &Path, run: bool) {
        match loader::load_file(path, None, &self.programs) {
            Ok(img) => {
                img.load_into(&mut self.m);
                let first = img.first_addr().unwrap_or(0);
                if run && let Some(s) = img.start.or(img.first_addr()) {
                    self.m.jump(s);
                }
                self.say(format!(
                    "Загружено {} байт с {first:04X} из {}",
                    img.len(),
                    path.display()
                ));
            }
            Err(e) => {
                // .bin без адреса — предложить окно «Магнитофон и файлы».
                self.tape.open = true;
                self.tape.load_path = path.display().to_string();
                self.fail(format!("{e:#}"));
            }
        }
    }

    // ----- меню -----

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Машина", |ui| {
                if ui.button("УСТ — сброс (F2)").clicked() {
                    self.press(Hit::Pad(PadKey::Reset), false);
                    ui.close();
                }
                if ui.button("Холодный старт — выкл./вкл. питания (F3)").clicked() {
                    let c = self.m.config().clone();
                    self.cold_start(c);
                    ui.close();
                }
                let label = if self.paused { "Продолжить (F4)" } else { "Пауза (F4)" };
                if ui.button(label).clicked() {
                    self.toggle_pause();
                    ui.close();
                }
                ui.separator();
                ui.label("Скорость (F5 — выше, Shift+F5 — ниже):");
                for s in Speed::all() {
                    if ui.radio_value(&mut self.speed, s, s.label()).changed() {
                        self.debt = 0.0;
                    }
                }
                ui.separator();
                ui.label("Конфигурация (холодный старт):");
                let cur = self.config_kind();
                for (kind, label, mk) in [
                    (0, "минимальная (модуль 1)", Config::minimal as fn() -> Config),
                    (1, "полная (МОНИТОР-F, часы в нём)", Config::full),
                    (2, "полная + МОНИТОР модуля 1", Config::full_with_monitor0),
                ] {
                    if ui.radio(cur == kind, label).clicked() {
                        self.switch_config(mk());
                        ui.close();
                    }
                }
                let mut irq = self.m.config().clock_interrupts;
                if ui
                    .checkbox(&mut irq, "Прерывания от часов")
                    .on_hover_text("тумблер из doc/25: при работе с магнитофоном через МОНИТОР модуля 1 их отключают")
                    .changed()
                {
                    self.m.set_clock_interrupts(irq);
                }
                if self.has_display() {
                    ui.checkbox(&mut self.pc_to_pad, "Клавиатура ПК → кнопки модуля 1");
                }
            });
            ui.menu_button("Магнитофон", |ui| {
                if ui.button("Вставить кассету (WAV)…").clicked() {
                    ui.close();
                    if let Some(p) = pick_open("WAV", &["wav"]) {
                        match self.insert_tape(&p) {
                            Ok(msg) => self.say(msg),
                            Err(e) => self.fail(format!("WAV не читается: {e}")),
                        }
                    }
                }
                let ins = self.m.tape_inserted();
                let label = if self.m.tape_playing() {
                    "Стоп"
                } else {
                    "Воспроизведение"
                };
                if ui.add_enabled(ins, egui::Button::new(label)).clicked() {
                    self.tape_play_toggle();
                    ui.close();
                }
                if ui.add_enabled(ins, egui::Button::new("Перемотать в начало")).clicked() {
                    self.tape_rewind();
                    ui.close();
                }
                if ui.add_enabled(ins, egui::Button::new("Вынуть кассету")).clicked() {
                    self.m.tape_eject();
                    self.say("Кассета вынута");
                    ui.close();
                }
                ui.separator();
                if self.m.tape_recording() {
                    if ui.button("Остановить запись и сохранить WAV…").clicked() {
                        ui.close();
                        if let Some(sig) = self.m.tape_record_stop() {
                            match pick_save("WAV", &["wav"], "record.wav") {
                                Some(p) => match sig.save_wav(&p) {
                                    Ok(()) => self.say(format!(
                                        "Записано {:.1} с в {}",
                                        sig.duration(),
                                        p.display()
                                    )),
                                    Err(e) => self.fail(format!("WAV не записан: {e}")),
                                },
                                None => self.say("Запись отброшена"),
                            }
                        }
                    }
                } else if ui.button("Начать запись").clicked() {
                    self.m.tape_record_start();
                    self.say("Идёт запись выхода магнитофона");
                    ui.close();
                }
                ui.separator();
                if ui.button("Магнитофон и файлы… (F8)").clicked() {
                    self.tape.open = true;
                    ui.close();
                }
            });
            ui.menu_button("Файл", |ui| {
                if ui.button("Загрузить программу (.bin/.hex/.ihx)…").clicked() {
                    ui.close();
                    if let Some(p) = pick_open("Программы", &["bin", "hex", "ihx"]) {
                        self.load_any_file(&p, false);
                    }
                }
                if ui.button("Загрузить и запустить…").clicked() {
                    ui.close();
                    if let Some(p) = pick_open("Программы", &["bin", "hex", "ihx"]) {
                        self.load_any_file(&p, true);
                    }
                }
                ui.separator();
                if ui.button("Быстрый снимок (F6)").clicked() {
                    self.save_snapshot(Path::new(QUICK_SNAPSHOT));
                    ui.close();
                }
                if ui.button("Загрузить быстрый снимок (F7)").clicked() {
                    self.load_snapshot(Path::new(QUICK_SNAPSHOT));
                    ui.close();
                }
                if ui.button("Сохранить снимок…").clicked() {
                    ui.close();
                    if let Some(p) = pick_save("Снимок ЮТ-88", &["snap"], "ut88.snap") {
                        self.save_snapshot(&p);
                    }
                }
                if ui.button("Загрузить снимок…").clicked() {
                    ui.close();
                    if let Some(p) = pick_open("Снимок ЮТ-88", &["snap"]) {
                        self.load_snapshot(&p);
                    }
                }
            });
            ui.menu_button("Программы", |ui| {
                if ui.button("Окно «Программы из журналов» (F9)").clicked() {
                    self.programs_open = true;
                    ui.close();
                }
                ui.separator();
                self.programs_list(ui, true);
            });
            ui.menu_button("Вид", |ui| {
                let mut on = self.audio.enabled() && self.audio.is_available();
                if ui.checkbox(&mut on, "Звук").changed() {
                    if on && !self.audio.is_available() {
                        self.audio = Audio::new();
                        if let Some(e) = self.audio.error() {
                            let e = e.to_string();
                            self.fail(format!("Звук недоступен: {e}"));
                        }
                    }
                    self.sound_wanted = on;
                    self.audio.set_enabled(on);
                    self.sync_sound_capture();
                }
                let mut vol = self.audio.volume();
                if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).text("громкость")).changed() {
                    self.audio.set_volume(vol);
                }
                ui.separator();
                ui.label("Люминофор:");
                ui.radio_value(&mut self.phosphor, Phosphor::Green, "зелёный");
                ui.radio_value(&mut self.phosphor, Phosphor::White, "белый");
                ui.separator();
                if ui.button("Отладчик (F12)").clicked() {
                    self.dbg.open = true;
                    ui.close();
                }
            });
            ui.menu_button("Справка", |ui| {
                if ui.button("Клавиши (F11)").clicked() {
                    self.help_open = true;
                    ui.close();
                }
                if ui.button("О программе").clicked() {
                    self.about_open = true;
                    ui.close();
                }
            });
        });
    }

    /// Список программ из `programs/` с кнопками загрузки (в меню и в окне F9).
    fn programs_list(&mut self, ui: &mut egui::Ui, in_menu: bool) {
        if self.programs.is_empty() {
            ui.label("папка programs/ не найдена");
        }
        let progs = self.programs.clone();
        for p in progs.iter().filter(|p| p.data_for().is_none()) {
            let ok = !p.needs_display() || self.has_display();
            let info = format!(
                "{}; {}\nзагрузка {:04X}, пуск {}",
                p.issue,
                p.config,
                p.load,
                p.start.map(|s| format!("{s:04X}")).unwrap_or("—".into())
            );
            let mut buttons = |ui: &mut egui::Ui| {
                let mut clicked = false;
                if ui.add_enabled(ok, egui::Button::new("Загрузить")).clicked() {
                    self.load_program(p, false);
                    clicked = true;
                }
                if ui
                    .add_enabled(
                        ok && p.start.is_some(),
                        egui::Button::new("Загрузить и запустить"),
                    )
                    .clicked()
                {
                    self.load_program(p, true);
                    clicked = true;
                }
                clicked
            };
            if in_menu {
                ui.menu_button(format!("{} — {}", p.name, p.title), |ui| {
                    ui.label(&info);
                    if !ok {
                        ui.label(
                            RichText::new("нужна полная конфигурация").color(Color32::LIGHT_RED),
                        );
                    }
                    if buttons(ui) {
                        ui.close();
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    buttons(ui);
                    ui.label(RichText::new(format!("{} — {}", p.name, p.title)).strong())
                        .on_hover_text(&info);
                    if !ok {
                        ui.label(
                            RichText::new("нужна полная конфигурация").color(Color32::LIGHT_RED),
                        );
                    }
                });
            }
        }
    }

    fn programs_window(&mut self, ctx: &egui::Context) {
        let mut open = self.programs_open;
        egui::Window::new("Программы из журналов")
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| self.programs_list(ui, false));
        self.programs_open = open && self.programs_open;
    }

    fn config_kind(&self) -> u8 {
        let c = self.m.config();
        match (c.display.is_some(), c.rom1_enabled) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => 2,
        }
    }

    fn config_name(&self) -> &'static str {
        match self.config_kind() {
            0 => "минимальная",
            1 => "полная",
            _ => "полная + МОНИТОР-0",
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let state = if self.paused {
                RichText::new("⏸ пауза").color(Color32::YELLOW)
            } else {
                RichText::new("▶ работа").color(Color32::LIGHT_GREEN)
            };
            ui.label(state);
            ui.separator();
            ui.label(format!("Конфигурация: {}", self.config_name()));
            ui.separator();
            ui.label(format!(
                "Скорость: {} (факт. {:.0} %)",
                self.speed.label(),
                if self.paused {
                    0.0
                } else {
                    self.actual_speed * 100.0
                }
            ));
            ui.separator();
            ui.label(format!("Время: {:.1} с", self.m.time()));
            ui.separator();
            ui.label(self.tape_status());
            ui.separator();
            ui.label(if self.audio.is_available() && self.audio.enabled() {
                "🔊"
            } else {
                "🔇"
            });
            if self.status_time.elapsed() < Duration::from_secs(8) {
                ui.separator();
                ui.label(&self.status);
            }
        });
    }

    /// Панель кнопок горячих клавиш (как в Norton/Midnight Commander): щелчок — то же,
    /// что нажатие клавиши.
    fn fkey_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            for &(label, key, shift, hint) in keymap::FKEY_BAR {
                let text = RichText::new(label).monospace();
                if ui.button(text).on_hover_text(hint).clicked() {
                    self.hotkey(&ctx, key, shift);
                }
            }
        });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Справка: клавиши")
            .open(&mut self.help_open)
            .default_size([520.0, 420.0])
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(
                    "Клавиша ЮТ-88 держится, пока нажата клавиша ПК (не меньше 0,06 с). \
                     Щелчок мышью по нарисованной клавише держит её не меньше 0,3 с. \
                     РУС, УС, СС мышью «залипают» до следующей клавиши.",
                );
                ui.label(
                    "Это окно открывается клавишей F11, пунктом меню «Справка → Клавиши» \
                     или кнопкой «F11 Справка» на панели над строкой состояния. Каждая \
                     горячая клавиша есть на этой панели кнопкой — щелчок мышью равен \
                     нажатию клавиши.",
                );
                ui.heading("Горячие клавиши");
                egui::Grid::new("hot").striped(true).show(ui, |ui| {
                    for (k, what) in keymap::HOTKEYS {
                        ui.monospace(*k);
                        ui.label(*what);
                        ui.end_row();
                    }
                });
                ui.heading("Модуль 1 (минимальная конфигурация)");
                egui::Grid::new("pad").striped(true).show(ui, |ui| {
                    ui.strong("ПК");
                    ui.strong("ЮТ-88");
                    ui.end_row();
                    for (k, p) in keymap::PAD_KEYS {
                        ui.monospace(keymap::pc_label(*k));
                        ui.label(keymap::pad_label(*p));
                        ui.end_row();
                    }
                    ui.monospace("F2");
                    ui.label("УСТ");
                    ui.end_row();
                });
                ui.heading("Полная клавиатура (дисплейный модуль)");
                egui::Grid::new("kb").striped(true).show(ui, |ui| {
                    ui.strong("ПК");
                    ui.strong("ЮТ-88");
                    ui.end_row();
                    for (name, k) in keymap::MODIFIERS {
                        ui.monospace(*name);
                        ui.label(keymap::kb_label(*k));
                        ui.end_row();
                    }
                    for (k, code) in keymap::KB_KEYS {
                        ui.monospace(keymap::pc_label(*k));
                        ui.label(keymap::code_label(*code));
                        ui.end_row();
                    }
                });
            });
    }

    fn tape_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tape.open;
        let mut actions: Vec<u8> = Vec::new();
        egui::Window::new("Магнитофон и файлы")
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.heading("Кассета");
                self.tape_controls(ui);
                ui.label(
                    "Вставленная кассета стоит; лента трогается сама, когда программа \
                     начинает читать вход магнитофона (директива A МОНИТОРА модуля 1, I \
                     МОНИТОРА-F), или кнопкой «Воспроизведение».",
                );
                ui.separator();
                let t = &mut self.tape;
                ui.heading("Быстрая загрузка .bin");
                path_row(ui, &mut t.load_path, true, "bin");
                ui.horizontal(|ui| {
                    ui.label("адрес:");
                    ui.add(egui::TextEdit::singleline(&mut t.load_addr).desired_width(60.0));
                    if ui.button("Загрузить").clicked() {
                        actions.push(1);
                    }
                    if ui.button("Загрузить и запустить").clicked() {
                        actions.push(2);
                    }
                });
                ui.separator();
                ui.heading("Сохранить диапазон памяти в .bin");
                path_row(ui, &mut t.save_path, false, "bin");
                ui.horizontal(|ui| {
                    ui.label("с");
                    ui.add(egui::TextEdit::singleline(&mut t.save_start).desired_width(60.0));
                    ui.label("по");
                    ui.add(egui::TextEdit::singleline(&mut t.save_end).desired_width(60.0));
                    if ui.button("Сохранить").clicked() {
                        actions.push(3);
                    }
                });
                ui.separator();
                ui.heading("WAV из .bin (запись для директив A / I)");
                ui.label("файл .bin:");
                path_row(ui, &mut t.gen_bin, true, "bin");
                ui.label("файл .wav:");
                path_row(ui, &mut t.gen_wav, false, "wav");
                ui.horizontal(|ui| {
                    ui.label("адрес начала:");
                    ui.add(egui::TextEdit::singleline(&mut t.gen_addr).desired_width(60.0));
                    ui.radio_value(&mut t.gen_format_f, false, "МОНИТОР модуля 1");
                    ui.radio_value(&mut t.gen_format_f, true, "МОНИТОР-F");
                });
                if ui.button("Создать WAV").clicked() {
                    actions.push(4);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Вставить WAV как кассету").clicked() {
                        actions.push(5);
                    }
                    ui.label("(файл .wav из строки выше)");
                });
            });
        self.tape.open = open;
        for a in actions {
            let r = self.tape_action(a);
            match r {
                Ok(msg) => self.say(msg),
                Err(e) => self.fail(format!("{e:#}")),
            }
        }
    }

    fn tape_action(&mut self, a: u8) -> anyhow::Result<String> {
        let t = &self.tape;
        match a {
            1 | 2 => {
                let addr = parse_hex_u16(&t.load_addr)?;
                let path = PathBuf::from(t.load_path.trim());
                let n = self.m.load_bin_file(&path, addr)?;
                if a == 2 {
                    self.m.jump(addr);
                }
                Ok(format!("Загружено {n} байт с {addr:04X}"))
            }
            3 => {
                let s = parse_hex_u16(&t.save_start)?;
                let e = parse_hex_u16(&t.save_end)?;
                anyhow::ensure!(e >= s, "конец раньше начала");
                let path = PathBuf::from(t.save_path.trim());
                anyhow::ensure!(!t.save_path.trim().is_empty(), "не задан файл");
                self.m.save_bin_file(&path, s, e)?;
                Ok(format!("Сохранено {s:04X}–{e:04X} в {}", path.display()))
            }
            4 => {
                let addr = parse_hex_u16(&t.gen_addr)?;
                let data = std::fs::read(t.gen_bin.trim())?;
                anyhow::ensure!(!data.is_empty(), "пустой файл");
                anyhow::ensure!(!t.gen_wav.trim().is_empty(), "не задан файл .wav");
                let fmt = if t.gen_format_f {
                    TapeFormat::MonitorF
                } else {
                    TapeFormat::Monitor0
                };
                let sig =
                    ut88_core::tape::block_signal(fmt, addr, &data, ut88_core::tape::WAV_RATE);
                sig.save_wav(t.gen_wav.trim())?;
                Ok(format!(
                    "WAV создан: {} ({:.1} с), блок {addr:04X}–{:04X}",
                    t.gen_wav,
                    sig.duration(),
                    addr.wrapping_add(data.len() as u16 - 1)
                ))
            }
            5 => {
                let p = PathBuf::from(t.gen_wav.trim());
                self.insert_tape(&p)
            }
            _ => Ok(String::new()),
        }
    }

    fn update_screen_texture(&mut self, ctx: &egui::Context) {
        let (fg, bg) = self.phosphor.colors();
        if !self.m.render_screen(&mut self.screen_buf, fg, bg) {
            self.screen_tex = None;
            return;
        }
        let img = egui::ColorImage::from_rgba_unmultiplied(
            [screen::WIDTH, screen::HEIGHT],
            &self.screen_buf,
        );
        match &mut self.screen_tex {
            Some(t) => t.set(img, TextureOptions::LINEAR),
            None => {
                self.screen_tex = Some(ctx.load_texture("screen", img, TextureOptions::LINEAR));
            }
        }
    }

    /// Рисунок компьютера.
    fn computer(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        // Стол.
        painter.rect_filled(rect, 0.0, Color32::from_rgb(0x2C, 0x2A, 0x2E));
        let segs = self.m.indicator_segments();
        let mut hits = Hits::default();
        let pressed = PressedView(&self.holds);
        if self.has_display() {
            let (w, h) = (draw::KB_W + 60.0, draw::TV_H + draw::KB_H + 50.0);
            let x = Xf::fit(rect.shrink(8.0), w, h);
            let glow = {
                let (fg, _) = self.phosphor.colors();
                Color32::from_rgb(fg[0], fg[1], fg[2])
            };
            draw::tv(
                &painter,
                x.sub(20.0, 10.0, 1.0),
                self.screen_tex.as_ref(),
                glow,
            );
            let k = 0.86;
            draw::cpu_module(
                &painter,
                x.sub(draw::TV_W + 50.0, 40.0, k),
                segs,
                &pressed,
                &mut hits,
            );
            draw::keyboard(
                &painter,
                x.sub(30.0, draw::TV_H + 40.0, 1.0),
                &pressed,
                &mut hits,
            );
        } else {
            let x = Xf::fit(
                rect.shrink(12.0),
                draw::MODULE_W + 20.0,
                draw::MODULE_H + 20.0,
            );
            draw::cpu_module(&painter, x.sub(10.0, 6.0, 1.0), segs, &pressed, &mut hits);
        }
        self.mouse_input(ui, &hits);
    }
}

struct PressedView<'a>(&'a HashMap<Hit, Hold>);

impl draw::Pressed for PressedView<'_> {
    fn is_down(&self, h: Hit) -> bool {
        self.0
            .get(&h)
            .is_some_and(|x| x.applied || x.pc || x.mouse || x.sticky)
    }
}

fn path_row(ui: &mut egui::Ui, path: &mut String, open: bool, ext: &str) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(path)
                .desired_width(380.0)
                .hint_text("путь к файлу"),
        );
        if ui.button("Обзор…").clicked() {
            let p = if open {
                pick_open(ext, &[ext])
            } else {
                pick_save(ext, &[ext], &format!("ut88.{ext}"))
            };
            if let Some(p) = p {
                *path = p.display().to_string();
            }
        }
    });
}

fn pick_open(name: &str, exts: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(name, exts)
        .add_filter("Все файлы", &["*"])
        .set_directory(std::env::current_dir().unwrap_or_default())
        .pick_file()
}

fn pick_save(name: &str, exts: &[&str], default: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(name, exts)
        .set_file_name(default)
        .set_directory(std::env::current_dir().unwrap_or_default())
        .save_file()
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.keyboard_input(ctx);
        self.emulate();
        self.update_screen_texture(ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));
        egui::TopBottomPanel::bottom("fkeys").show(ctx, |ui| self.fkey_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| self.computer(ui));

        let paused = self.paused;
        if self.dbg.open {
            let a = self.dbg.window(ctx, &mut self.m, paused);
            self.do_debug_action(a);
        }
        if self.help_open {
            self.help_window(ctx);
        }
        if self.tape.open {
            self.tape_window(ctx);
        }
        if self.programs_open {
            self.programs_window(ctx);
        }
        if self.about_open {
            egui::Window::new("О программе")
                .open(&mut self.about_open)
                .show(ctx, |ui| {
                    ui.label("Эмулятор микро-ЭВМ «ЮТ-88»");
                    ui.label("по журналу «ЮТ для умелых рук», 1989–1990.");
                    ui.label(format!(
                        "Процессор КР580ВМ80А, {:.4} МГц (16 МГц / 9).",
                        CPU_HZ / 1e6
                    ));
                });
        }
        if let Some(e) = self.error_popup.clone() {
            let mut open = true;
            egui::Window::new("Ошибка")
                .collapsible(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(&e);
                    if ui.button("OK").clicked() {
                        self.error_popup = None;
                    }
                });
            if !open {
                self.error_popup = None;
            }
        }
        // Окно перерисовывается непрерывно — от этого зависит ход эмуляции.
        ctx.request_repaint();
    }
}
