//! Состояние терминального интерфейса: машина, режимы, обработка клавиш, отрисовка.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ut88_core::screen::{COLS, ROWS};
use ut88_core::{CPU_HZ, FRAME_CYCLES, Machine, PadKey, TapeFormat, TapeSignal};

use crate::Options;
use crate::debugger::{self, Debugger, Stop};
use crate::input::{InputQueue, Target};
use crate::keymap::{self, HOTKEYS, Hotkey};
use crate::programs::{self, Program};
use crate::widgets::{self, INDICATORS_W, Indicators, ScreenView};

/// Ряд скоростей (0 — максимум) — общий с GUI.
pub use ut88_core::speed::SPEEDS;

/// Экран.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Машина: индикаторы или экран дисплея.
    Machine,
    /// Отладчик.
    Debugger,
}

/// Что спрашивает строка ввода.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    SaveSnapshot,
    LoadSnapshot,
    TapeWav,
    RecordSave(TapeSignal),
    FastLoadPath,
    FastLoadAddr(PathBuf),
    SaveRange,
    SaveRangePath(u16, u16),
    WavBlockRange,
    WavBlockPath(u16, u16, TapeFormat),
    Breakpoint,
    PortBreak,
    RunTo,
    MemGoto,
    DisasmGoto,
}

/// Строка ввода.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Вопрос.
    pub title: String,
    /// Введённый текст.
    pub text: String,
    /// Что спрашиваем.
    pub ask: Ask,
}

/// Пункт меню.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Hot(Hotkey),
    ToggleView,
    ToggleClock,
    SaveSnapshotAs,
    LoadSnapshotFrom,
}

/// Пункты главного меню: буква, подпись, действие.
pub const MENU: &[(char, &str, MenuAction)] = &[
    (
        'u',
        "УСТ — сброс процессора                [F2]",
        MenuAction::Hot(Hotkey::Reset),
    ),
    (
        'h',
        "холодный старт                        [F3]",
        MenuAction::Hot(Hotkey::ColdStart),
    ),
    (
        'p',
        "пауза / продолжить                    [F4]",
        MenuAction::Hot(Hotkey::Pause),
    ),
    (
        '+',
        "скорость выше                         [F5]",
        MenuAction::Hot(Hotkey::SpeedUp),
    ),
    (
        '-',
        "скорость ниже                   [Shift+F5]",
        MenuAction::Hot(Hotkey::SpeedDown),
    ),
    (
        's',
        "быстрый снимок                        [F6]",
        MenuAction::Hot(Hotkey::SaveSnapshot),
    ),
    (
        'l',
        "загрузить быстрый снимок              [F7]",
        MenuAction::Hot(Hotkey::LoadSnapshot),
    ),
    ('a', "сохранить снимок в файл…", MenuAction::SaveSnapshotAs),
    (
        'o',
        "загрузить снимок из файла…",
        MenuAction::LoadSnapshotFrom,
    ),
    (
        't',
        "магнитофон                            [F8]",
        MenuAction::Hot(Hotkey::Tape),
    ),
    (
        'g',
        "программы из журналов                 [F9]",
        MenuAction::Hot(Hotkey::Programs),
    ),
    (
        'd',
        "отладчик                             [F12]",
        MenuAction::Hot(Hotkey::Debugger),
    ),
    (
        'k',
        "вид: модуль 1 ↔ дисплейный модуль",
        MenuAction::ToggleView,
    ),
    ('i', "прерывания часов вкл/выкл", MenuAction::ToggleClock),
    (
        'q',
        "выход                                [F10]",
        MenuAction::Hot(Hotkey::Exit),
    ),
];

/// Пункты меню магнитофона.
pub const TAPE_MENU: &[(char, &str)] = &[
    ('w', "вставить кассету (WAV); лента тронется при чтении"),
    ('p', "воспроизведение / стоп"),
    ('n', "перемотать в начало"),
    ('s', "вынуть кассету"),
    ('r', "запись выхода в WAV: начать / остановить"),
    ('b', "быстро загрузить .bin по адресу"),
    ('m', "сохранить диапазон памяти в .bin"),
    ('o', "записать диапазон в WAV (блок МОНИТОРА)"),
];

/// Окно поверх экрана.
#[derive(Debug, Clone)]
pub enum Overlay {
    Menu(usize),
    Tape(usize),
    Programs(Vec<Program>, usize),
    Prompt(Prompt),
}

/// Состояние приложения.
pub struct App {
    /// Машина.
    pub m: Machine,
    /// Параметры.
    pub opts: Options,
    /// Очередь нажатий.
    pub input: InputQueue,
    /// Отладчик.
    pub dbg: Debugger,
    /// Экран.
    pub view: View,
    /// Окно поверх.
    pub overlay: Option<Overlay>,
    /// Пауза (на экране машины).
    pub paused: bool,
    /// Машина идёт в отладчике.
    pub dbg_running: bool,
    /// Индекс скорости в [`SPEEDS`] или собственная скорость.
    pub speed: f64,
    /// Показывать модуль 1 (индикаторы и кнопки) вместо дисплея.
    pub pad_view: bool,
    /// Терминал сообщает об отпускании клавиш.
    pub kitty: bool,
    /// Сообщение в строке состояния.
    pub message: Option<(String, Instant)>,
    /// Пора выходить.
    pub quit: bool,
    /// Следующая команда — продолжение после останова (не проверять точки).
    pub resume: bool,
    /// Измеренная скорость.
    pub actual_speed: f64,
    meter: (Instant, u64),
}

/// Разобрать шестнадцатеричное число (допускается суффикс H).
pub fn parse_hex(s: &str) -> Option<u16> {
    let s = s.trim().trim_end_matches(['H', 'h']);
    if s.is_empty() {
        return None;
    }
    u16::from_str_radix(s, 16).ok()
}

/// Разобрать диапазон `начало-конец` или `начало,конец`.
pub fn parse_range(s: &str) -> Option<(u16, u16)> {
    let (a, b) = s.split_once([',', '-', ' '])?;
    let (a, b) = (parse_hex(a)?, parse_hex(b)?);
    (a <= b).then_some((a, b))
}

fn key_ident(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_lowercase().collect(),
        c => format!("{c:?}"),
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

impl App {
    /// Новое приложение.
    pub fn new(m: Machine, opts: Options) -> Self {
        let pad_view = m.config().display.is_none();
        let speed = if opts.speed.is_finite() && opts.speed >= 0.0 {
            ut88_core::speed::snap_speed(opts.speed)
        } else {
            1.0
        };
        let c = m.cycles();
        Self {
            m,
            opts,
            input: InputQueue::new(),
            dbg: Debugger::new(),
            view: View::Machine,
            overlay: None,
            paused: false,
            dbg_running: false,
            speed,
            pad_view,
            kitty: false,
            message: None,
            quit: false,
            resume: false,
            actual_speed: 0.0,
            meter: (Instant::now(), c),
        }
    }

    /// Машина сейчас выполняется.
    pub fn running(&self) -> bool {
        match self.view {
            View::Machine => !self.paused,
            View::Debugger => self.dbg_running,
        }
    }

    /// Сообщение в строку состояния.
    pub fn say(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), Instant::now()));
    }

    fn config_name(&self) -> &'static str {
        let c = self.m.config();
        match (c.display.is_some(), c.rom1_enabled) {
            (false, _) => "минимальная",
            (true, false) => "полная",
            (true, true) => "полная (МОНИТОР-0)",
        }
    }

    // ----- выполнение -----

    /// Выполнить машину до такта `target` (с проверкой точек останова).
    pub fn run_to_cycle(&mut self, target: u64) {
        let skip = std::mem::take(&mut self.resume);
        if let Some(stop) =
            debugger::run_until(&mut self.m, target, &mut self.dbg, &mut self.input, skip)
        {
            self.on_stop(stop);
        }
    }

    fn on_stop(&mut self, stop: Stop) {
        self.dbg_running = false;
        self.view = View::Debugger;
        self.dbg.disasm_at = None;
        self.say(format!("Останов: {stop}"));
    }

    /// Обновить измерение скорости.
    pub fn update_meter(&mut self) {
        let (t, c) = self.meter;
        let dt = t.elapsed().as_secs_f64();
        if dt >= 1.0 {
            let dc = self.m.cycles().saturating_sub(c);
            self.actual_speed = dc as f64 / CPU_HZ / dt;
            self.meter = (Instant::now(), self.m.cycles());
        }
    }

    // ----- клавиши -----

    /// Обработать клавишу.
    pub fn on_key(&mut self, ev: KeyEvent) {
        let now = self.m.cycles();
        match ev.kind {
            KeyEventKind::Release => {
                self.input.release(&key_ident(ev.code), now);
                return;
            }
            KeyEventKind::Repeat => {
                if self.overlay.is_none() && self.view == View::Machine {
                    self.input.repeat(&key_ident(ev.code));
                    return;
                }
            }
            KeyEventKind::Press => {}
        }
        if let Some(ov) = self.overlay.take() {
            self.overlay_key(ov, ev);
            return;
        }
        if let Some(hk) = keymap::hotkey(&ev) {
            self.hotkey(hk);
            return;
        }
        match self.view {
            View::Debugger => self.debugger_key(ev),
            View::Machine => self.machine_key(ev),
        }
    }

    fn machine_key(&mut self, ev: KeyEvent) {
        let target = if self.pad_view {
            keymap::pad_key(&ev).map(Target::Pad)
        } else {
            keymap::kb_key(&ev).map(|(k, md)| Target::Kb(k, md))
        };
        if let Some(t) = target {
            self.input
                .press(t, &key_ident(ev.code), self.kitty, Instant::now());
        }
    }

    /// Выполнить действие горячей клавиши.
    pub fn hotkey(&mut self, hk: Hotkey) {
        match hk {
            Hotkey::Menu => self.overlay = Some(Overlay::Menu(0)),
            Hotkey::Reset => {
                self.input.clear(&mut self.m);
                self.m.pad_down(PadKey::Reset);
                self.say("УСТ");
            }
            Hotkey::ColdStart => {
                self.input.clear(&mut self.m);
                self.m = match &self.opts.cold_start {
                    Some(f) => f(),
                    None => Machine::new(self.m.config().clone()),
                };
                self.meter = (Instant::now(), self.m.cycles());
                self.say("Холодный старт");
            }
            Hotkey::Pause => {
                if self.view == View::Debugger {
                    self.dbg_running = !self.dbg_running;
                    self.resume = true;
                } else {
                    self.paused = !self.paused;
                    self.resume = true;
                }
            }
            Hotkey::SpeedUp | Hotkey::SpeedDown => {
                self.speed = ut88_core::speed::speed_step(self.speed, hk == Hotkey::SpeedUp);
                self.say(format!("Скорость: {}", speed_name(self.speed)));
            }
            Hotkey::SaveSnapshot => {
                let p = self.opts.quick_snapshot.display().to_string();
                self.prompt_done(Ask::SaveSnapshot, p);
            }
            Hotkey::LoadSnapshot => {
                let p = self.opts.quick_snapshot.display().to_string();
                self.prompt_done(Ask::LoadSnapshot, p);
            }
            Hotkey::Tape => self.overlay = Some(Overlay::Tape(0)),
            Hotkey::Programs => {
                let dir = programs::find_dir(self.opts.programs_dir.as_deref());
                let list = dir.map(|d| programs::list(&d)).unwrap_or_default();
                if list.is_empty() {
                    self.say("Папка programs/ не найдена или пуста");
                } else {
                    self.overlay = Some(Overlay::Programs(list, 0));
                }
            }
            Hotkey::Exit => self.quit = true,
            Hotkey::Debugger => {
                if self.view == View::Debugger {
                    self.view = View::Machine;
                    self.dbg_running = false;
                    self.resume = true;
                } else {
                    self.view = View::Debugger;
                    self.dbg_running = false;
                    self.dbg.disasm_at = None;
                    self.dbg.mark = self.m.cycles();
                }
            }
        }
    }

    fn ask(&mut self, title: &str, default: &str, ask: Ask) {
        self.overlay = Some(Overlay::Prompt(Prompt {
            title: title.into(),
            text: default.into(),
            ask,
        }));
    }

    fn overlay_key(&mut self, ov: Overlay, ev: KeyEvent) {
        let up_down = |sel: usize, n: usize| match ev.code {
            KeyCode::Up => Some(sel.checked_sub(1).unwrap_or(n - 1)),
            KeyCode::Down => Some((sel + 1) % n),
            _ => None,
        };
        match ov {
            Overlay::Menu(sel) => {
                if ev.code == KeyCode::Esc || ev.code == KeyCode::F(1) {
                    return;
                }
                if let Some(s) = up_down(sel, MENU.len()) {
                    self.overlay = Some(Overlay::Menu(s));
                    return;
                }
                let pick = match ev.code {
                    KeyCode::Enter => Some(sel),
                    KeyCode::Char(c) => MENU.iter().position(|m| m.0 == c.to_ascii_lowercase()),
                    _ => None,
                };
                match pick {
                    Some(i) => self.menu_action(MENU[i].2),
                    None => self.overlay = Some(Overlay::Menu(sel)),
                }
            }
            Overlay::Tape(sel) => {
                if ev.code == KeyCode::Esc {
                    return;
                }
                if let Some(s) = up_down(sel, TAPE_MENU.len()) {
                    self.overlay = Some(Overlay::Tape(s));
                    return;
                }
                let pick = match ev.code {
                    KeyCode::Enter => Some(TAPE_MENU[sel].0),
                    KeyCode::Char(c) => Some(c.to_ascii_lowercase()),
                    _ => None,
                };
                match pick {
                    Some(c) if TAPE_MENU.iter().any(|t| t.0 == c) => self.tape_action(c),
                    _ => self.overlay = Some(Overlay::Tape(sel)),
                }
            }
            Overlay::Programs(list, sel) => match ev.code {
                KeyCode::Esc => {}
                KeyCode::Up | KeyCode::Down => {
                    let s = up_down(sel, list.len()).unwrap_or(sel);
                    self.overlay = Some(Overlay::Programs(list, s));
                }
                KeyCode::Enter => self.load_program(&list, sel, true),
                KeyCode::Char('l' | 'L' | 'д' | 'Д') => self.load_program(&list, sel, false),
                _ => self.overlay = Some(Overlay::Programs(list, sel)),
            },
            Overlay::Prompt(mut p) => match ev.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    let text = p.text.trim().to_string();
                    self.prompt_done(p.ask, text);
                }
                KeyCode::Backspace => {
                    p.text.pop();
                    self.overlay = Some(Overlay::Prompt(p));
                }
                KeyCode::Char('u') if ev.modifiers.contains(KeyModifiers::CONTROL) => {
                    p.text.clear();
                    self.overlay = Some(Overlay::Prompt(p));
                }
                KeyCode::Char(c) => {
                    p.text.push(c);
                    self.overlay = Some(Overlay::Prompt(p));
                }
                _ => self.overlay = Some(Overlay::Prompt(p)),
            },
        }
    }

    fn menu_action(&mut self, a: MenuAction) {
        match a {
            MenuAction::Hot(h) => self.hotkey(h),
            MenuAction::ToggleView => {
                self.input.clear(&mut self.m);
                if self.m.config().display.is_none() {
                    self.pad_view = true;
                    self.say("Дисплейного модуля нет");
                } else {
                    self.pad_view = !self.pad_view;
                }
            }
            MenuAction::SaveSnapshotAs => {
                self.ask("Сохранить снимок в файл", "ut88.snap", Ask::SaveSnapshot)
            }
            MenuAction::LoadSnapshotFrom => {
                self.ask("Загрузить снимок из файла", "ut88.snap", Ask::LoadSnapshot)
            }
            MenuAction::ToggleClock => {
                let on = !self.m.config().clock_interrupts;
                self.m.set_clock_interrupts(on);
                self.say(if on {
                    "Прерывания часов включены"
                } else {
                    "Прерывания часов выключены"
                });
            }
        }
    }

    fn default_format(&self) -> TapeFormat {
        if self.m.config().display.is_some() && !self.pad_view {
            TapeFormat::MonitorF
        } else {
            TapeFormat::Monitor0
        }
    }

    fn tape_action(&mut self, c: char) {
        match c {
            'w' => self.ask("Вставить кассету — WAV-файл", "", Ask::TapeWav),
            'p' => {
                if !self.m.tape_inserted() {
                    self.say("Кассета не вставлена (F8 w)");
                } else if self.m.tape_playing() {
                    self.m.tape_pause();
                    self.say("Лента остановлена");
                } else {
                    self.m.tape_start();
                    self.say("Воспроизведение");
                }
            }
            'n' => {
                if self.m.tape_rewind() {
                    self.say("Лента перемотана в начало");
                }
            }
            's' => {
                self.m.tape_eject();
                self.say("Кассета вынута");
            }
            'r' => {
                if let Some(sig) = self.m.tape_record_stop() {
                    self.ask("Сохранить запись в WAV", "record.wav", Ask::RecordSave(sig));
                } else {
                    self.m.tape_record_start();
                    self.say("Запись выхода магнитофона начата (F8 r — остановить)");
                }
            }
            'b' => self.ask("Файл .bin для загрузки", "", Ask::FastLoadPath),
            'm' => self.ask(
                "Диапазон памяти (начало,конец)",
                "C000,C3FF",
                Ask::SaveRange,
            ),
            'o' => self.ask(
                "Диапазон для WAV (начало,конец)",
                "C000,C0FF",
                Ask::WavBlockRange,
            ),
            _ => {}
        }
    }

    fn load_program(&mut self, list: &[Program], sel: usize, run: bool) {
        let p = &list[sel];
        if let Some(why) = p.unsupported(self.m.config()) {
            // Окно списка остаётся открытым: можно выбрать другую программу.
            self.overlay = Some(Overlay::Programs(list.to_vec(), sel));
            self.say(format!("«{}» не загружена: {why}", p.title_or_name()));
            return;
        }
        let Some(load) = p.load else {
            self.say(format!("{}: адрес загрузки неизвестен", p.name));
            return;
        };
        if let Err(e) = self.m.load_bin_file(&p.path, load) {
            self.say(format!("{}: {e}", p.path.display()));
            return;
        }
        let mut msg = format!("{} загружена в {load:04X}H", p.name);
        // Данные к программе (например, поле «Лабиринта»).
        if let Some(extra) = list.iter().find(|q| q.name == format!("{}-field", p.name))
            && let Some(a) = extra.load
            && self.m.load_bin_file(&extra.path, a).is_ok()
        {
            msg += &format!(", {} — в {a:04X}H", extra.name);
        }
        match p.start {
            Some(s) if run => {
                self.input.clear(&mut self.m);
                self.m.jump(s);
                msg += &format!(", пуск с {s:04X}H");
            }
            Some(s) => msg += &format!(", не запущена (адрес пуска {s:04X}H)"),
            None => msg += ", не запущена",
        }
        if !p.config.is_empty() {
            msg += &format!(" (нужно: {})", p.config);
        }
        self.say(msg);
    }

    fn prompt_done(&mut self, ask: Ask, text: String) {
        let bad = |s: &mut Self| s.say("Неверный ввод");
        match ask {
            Ask::SaveSnapshot => match self.m.save_snapshot(&text) {
                Ok(()) => self.say(format!("Снимок сохранён: {text}")),
                Err(e) => self.say(format!("Ошибка: {e}")),
            },
            Ask::LoadSnapshot => match self.m.load_snapshot(&text) {
                Ok(()) => {
                    self.input = InputQueue::new();
                    self.m.release_all_keys();
                    self.meter = (Instant::now(), self.m.cycles());
                    self.pad_view = self.m.config().display.is_none();
                    self.say(format!("Снимок загружен: {text}"));
                }
                Err(e) => self.say(format!("Ошибка: {e}")),
            },
            Ask::TapeWav => match self.m.tape_insert_wav(&text) {
                Ok(()) => self.say(format!(
                    "Кассета {text} вставлена; лента тронется при чтении (A / I) или F8 p"
                )),
                Err(e) => self.say(format!("Ошибка: {e}")),
            },
            Ask::RecordSave(sig) => match sig.save_wav(&text) {
                Ok(()) => self.say(format!(
                    "Запись сохранена: {text} ({:.1} с)",
                    sig.duration()
                )),
                Err(e) => self.say(format!("Ошибка: {e}")),
            },
            Ask::FastLoadPath => {
                if Path::new(&text).is_file() {
                    self.ask(
                        "Адрес загрузки (hex)",
                        "C000",
                        Ask::FastLoadAddr(text.into()),
                    );
                } else {
                    self.say(format!("Нет файла {text}"));
                }
            }
            Ask::FastLoadAddr(path) => match parse_hex(&text) {
                Some(a) => match self.m.load_bin_file(&path, a) {
                    Ok(n) => self.say(format!(
                        "Загружено {n} байт в {a:04X}–{:04X}",
                        a.wrapping_add(n.max(1) as u16 - 1)
                    )),
                    Err(e) => self.say(format!("Ошибка: {e}")),
                },
                None => bad(self),
            },
            Ask::SaveRange => match parse_range(&text) {
                Some((a, b)) => self.ask(
                    "Сохранить в файл .bin",
                    "memory.bin",
                    Ask::SaveRangePath(a, b),
                ),
                None => bad(self),
            },
            Ask::SaveRangePath(a, b) => match self.m.save_bin_file(&text, a, b) {
                Ok(()) => self.say(format!("{a:04X}–{b:04X} сохранено в {text}")),
                Err(e) => self.say(format!("Ошибка: {e}")),
            },
            Ask::WavBlockRange => match parse_range(&text) {
                Some((a, b)) => {
                    let f = self.default_format();
                    self.ask(
                        &format!("Сохранить WAV ({})", format_name(f)),
                        "block.wav",
                        Ask::WavBlockPath(a, b, f),
                    )
                }
                None => bad(self),
            },
            Ask::WavBlockPath(a, b, f) => match self.m.tape_block_signal(f, a, b).save_wav(&text) {
                Ok(()) => self.say(format!("Блок {a:04X}–{b:04X} записан в {text}")),
                Err(e) => self.say(format!("Ошибка: {e}")),
            },
            Ask::Breakpoint => match parse_hex(&text) {
                Some(a) => {
                    let on = self.dbg.toggle_breakpoint(a);
                    self.say(format!(
                        "Точка {a:04X} {}",
                        if on {
                            "поставлена"
                        } else {
                            "снята"
                        }
                    ));
                }
                None => bad(self),
            },
            Ask::PortBreak => match parse_hex(&text).filter(|&p| p < 0x100) {
                Some(p) => {
                    let on = self.dbg.toggle_port(p as u8);
                    self.say(format!(
                        "Порт {p:02X} {}",
                        if on {
                            "отслеживается"
                        } else {
                            "снят"
                        }
                    ));
                }
                None => bad(self),
            },
            Ask::RunTo => match parse_hex(&text) {
                Some(a) => {
                    self.dbg.run_to(a);
                    self.dbg.mark = self.m.cycles();
                    self.dbg_running = true;
                    self.resume = true;
                }
                None => bad(self),
            },
            Ask::MemGoto => match parse_hex(&text) {
                Some(a) => self.dbg.mem_cursor = a,
                None => bad(self),
            },
            Ask::DisasmGoto => self.dbg.disasm_at = parse_hex(&text),
        }
    }

    fn debugger_key(&mut self, ev: KeyEvent) {
        // Правка памяти.
        if self.dbg.edit.is_some() {
            match ev.code {
                KeyCode::Char(c) if c.is_ascii_hexdigit() => {
                    let d = c.to_digit(16).unwrap_or(0) as u8;
                    self.dbg.edit_digit(&mut self.m, d);
                    return;
                }
                KeyCode::Esc | KeyCode::Enter => {
                    self.dbg.edit = None;
                    return;
                }
                _ => {}
            }
        }
        let cur = self.dbg.mem_cursor;
        match ev.code {
            KeyCode::Esc => self.hotkey(Hotkey::Debugger),
            KeyCode::Char('s') | KeyCode::F(11) => {
                if !self.dbg_running {
                    self.dbg.mark = self.m.cycles();
                    self.dbg.step(&mut self.m, &mut self.input);
                    self.dbg.disasm_at = None;
                }
            }
            KeyCode::Char('o') => {
                if !self.dbg_running {
                    self.dbg.mark = self.m.cycles();
                    if self.dbg.step_over(&mut self.m, &mut self.input) {
                        self.dbg_running = true;
                        self.resume = true;
                    }
                    self.dbg.disasm_at = None;
                }
            }
            KeyCode::Char('g') | KeyCode::Char(' ') => {
                self.dbg_running = !self.dbg_running;
                self.resume = true;
                if self.dbg_running {
                    self.dbg.mark = self.m.cycles();
                }
            }
            KeyCode::Char('r') => self.ask("Выполнить до адреса", "", Ask::RunTo),
            KeyCode::Char('b') => {
                let pc = format!("{:04X}", self.m.cpu.pc);
                self.ask("Точка останова по адресу", &pc, Ask::Breakpoint)
            }
            KeyCode::Char('p') => self.ask("Точка останова по порту", "A0", Ask::PortBreak),
            KeyCode::Char('x') => {
                self.dbg.breakpoints.clear();
                self.dbg.port_breaks.clear();
                self.dbg.temp = None;
                self.say("Все точки останова сняты");
            }
            KeyCode::Char('m') => self.ask("Дамп памяти с адреса", "", Ask::MemGoto),
            KeyCode::Char('a') => {
                self.ask("Дизассемблер с адреса (пусто — PC)", "", Ask::DisasmGoto)
            }
            KeyCode::Char('e') => self.dbg.edit = Some(debugger::Edit { high: None }),
            KeyCode::Char('c') => self.dbg.mark = self.m.cycles(),
            KeyCode::Left => self.dbg.mem_cursor = cur.wrapping_sub(1),
            KeyCode::Right => self.dbg.mem_cursor = cur.wrapping_add(1),
            KeyCode::Up => self.dbg.mem_cursor = cur.wrapping_sub(8),
            KeyCode::Down => self.dbg.mem_cursor = cur.wrapping_add(8),
            KeyCode::PageUp => self.dbg.mem_cursor = cur.wrapping_sub(0x80),
            KeyCode::PageDown => self.dbg.mem_cursor = cur.wrapping_add(0x80),
            _ => {}
        }
    }

    // ----- отрисовка -----

    /// Нарисовать кадр.
    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let [main, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        match self.view {
            View::Machine if self.pad_view => self.draw_pad(f, main),
            View::Machine => self.draw_display(f, main),
            View::Debugger => self.draw_debugger(f, main),
        }
        f.render_widget(Paragraph::new(self.status_line()), status);
        if let Some(ov) = &self.overlay {
            self.draw_overlay(f, main, ov);
        }
    }

    /// Строка состояния.
    pub fn status_line(&self) -> Line<'static> {
        let inv = Style::default().fg(Color::Black).bg(Color::Gray);
        let mut s = format!(
            " ЮТ-88 | {} | {}",
            self.config_name(),
            speed_name(self.speed)
        );
        if self.running() && self.actual_speed > 0.0 {
            s += &format!(" ({:.2})", self.actual_speed);
        }
        if !self.running() {
            s += " | ПАУЗА";
        }
        s += &format!(
            " | такты {} | лента: {}",
            self.m.cycles(),
            self.tape_state()
        );
        if let Some((msg, t)) = &self.message
            && t.elapsed() < Duration::from_secs(6)
        {
            s += &format!(" | {msg}");
        } else {
            s += " | F1 — меню";
        }
        Line::styled(s, inv)
    }

    fn tape_state(&self) -> String {
        let mut s = match self.m.tape_position() {
            None => "нет".to_string(),
            Some(p) if self.m.tape_finished() => format!("конец ({p:.1} с)"),
            Some(p) if self.m.tape_playing() => format!("▶ {p:.1} с"),
            Some(p) if self.m.tape_auto_start() => format!("⏹ {p:.1} с, ждёт чтения"),
            Some(p) => format!("⏹ {p:.1} с, стоп"),
        };
        if self.m.tape_recording() {
            s += " ● запись";
        }
        s
    }

    fn draw_pad(&self, f: &mut Frame, area: Rect) {
        let pressed = match self.input.current() {
            Some(Target::Pad(k)) => Some(k),
            _ => None,
        };
        let x = area.x + 2;
        let ind = Rect::new(
            x,
            area.y + 1,
            INDICATORS_W.min(area.width.saturating_sub(2)),
            widgets::DIGIT_H.min(area.height),
        );
        f.render_widget(
            Indicators {
                segments: self.m.indicator_segments(),
            },
            ind,
        );
        let label_y = ind.bottom() + 1;
        if label_y < area.bottom() {
            let lab = Rect::new(x, label_y, ind.width, 1);
            let addr_w = 4 * widgets::DIGIT_W + 3 * widgets::DIGIT_GAP;
            let text = format!(
                "{:^aw$}{:gap$}{:^dw$}",
                "АДРЕС (HL)",
                "",
                "ДАННЫЕ (A)",
                aw = addr_w as usize,
                gap = (widgets::DIGIT_GAP + widgets::GROUP_GAP) as usize,
                dw = (2 * widgets::DIGIT_W + widgets::DIGIT_GAP) as usize
            );
            f.render_widget(
                Paragraph::new(text).style(Style::default().fg(Color::DarkGray)),
                lab,
            );
        }
        let help_y = label_y + 2;
        if help_y < area.bottom() {
            let h = Rect::new(
                x,
                help_y,
                area.width.saturating_sub(2),
                area.bottom() - help_y,
            );
            f.render_widget(widgets::pad_help(pressed), h);
        }
    }

    fn draw_display(&self, f: &mut Frame, area: Rect) {
        let Some(scr) = self.m.screen() else {
            return;
        };
        let (w, h) = (COLS as u16, ROWS as u16);
        if area.width >= w + 2 && area.height > h + 2 {
            let outer = Rect::new(area.x, area.y, w + 2, h + 2);
            let hint = format!(" ЮТ-88 · {} ", self.kb_hint());
            f.render_widget(
                Block::bordered().title(" Экран 64×28 ").title_bottom(hint),
                outer,
            );
            f.render_widget(
                ScreenView { screen: &scr },
                Rect::new(area.x + 1, area.y + 1, w, h),
            );
        } else {
            f.render_widget(ScreenView { screen: &scr }, area);
        }
    }

    fn kb_hint(&self) -> String {
        match self.input.current() {
            Some(Target::Kb(k, md)) => {
                let code = k.base_code().unwrap_or(0);
                let m = match md {
                    Some(ut88_core::KbKey::Rus) => "РУС+",
                    Some(ut88_core::KbKey::Us) => "УС+",
                    Some(ut88_core::KbKey::Ss) => "СС+",
                    _ => "",
                };
                format!("нажато {m}{code:02X}H")
            }
            _ => "F1 — меню".into(),
        }
    }

    fn draw_debugger(&self, f: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Length(40), Constraint::Min(30)]).areas(area);
        let [regs, dis] = Layout::vertical([Constraint::Length(9), Constraint::Min(3)]).areas(left);
        let [mem, bps, help] = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(5),
            Constraint::Length(6),
        ])
        .areas(right);
        let c = &self.m.cpu;
        let fl = |n: &str, v: bool| format!("{n}={}", u8::from(v));
        let stack: Vec<String> = (0..4)
            .map(|i| {
                let a = c.sp.wrapping_add(i * 2);
                format!(
                    "{:02X}{:02X}",
                    self.m.peek(a.wrapping_add(1)),
                    self.m.peek(a)
                )
            })
            .collect();
        let state = if self.dbg_running {
            "выполняется"
        } else {
            "остановлен"
        };
        let lines = vec![
            Line::from(format!(
                "PC={:04X}  SP={:04X}  A={:02X}  F={:02X}",
                c.pc,
                c.sp,
                c.a,
                c.flags()
            )),
            Line::from(format!(
                "B={:02X} C={:02X} D={:02X} E={:02X} H={:02X} L={:02X}",
                c.b, c.c, c.d, c.e, c.h, c.l
            )),
            Line::from(format!(
                "BC={:04X} DE={:04X} HL={:04X}",
                c.bc(),
                c.de(),
                c.hl()
            )),
            Line::from(format!(
                "{} {} {} {} {}",
                fl("S", c.flag_s),
                fl("Z", c.flag_z),
                fl("AC", c.flag_ac),
                fl("P", c.flag_p),
                fl("C", c.flag_c)
            )),
            Line::from(format!(
                "INTE={} HLT={}  {state}",
                u8::from(c.inte),
                u8::from(c.halted)
            )),
            Line::from(format!("Такты: {}", c.cycles)),
            Line::from(format!(
                "Δ такты: {}",
                c.cycles.saturating_sub(self.dbg.mark)
            )),
            Line::from(format!("Стек: {}", stack.join(" "))),
        ];
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Регистры ")),
            regs,
        );

        // Дизассемблер.
        let rows = dis.height.saturating_sub(2) as usize;
        let pc = c.pc;
        let start = match self.dbg.disasm_at {
            Some(a) => a,
            None => debugger::back_start(&self.m, pc, (rows / 3).min(6)),
        };
        let items: Vec<Line> = debugger::disasm_from(&self.m, start, rows)
            .into_iter()
            .map(|l| {
                let bp = if self.dbg.breakpoints.contains(&l.addr) {
                    '●'
                } else {
                    ' '
                };
                let cur = if l.addr == pc { '▶' } else { ' ' };
                let bytes: String = l.bytes.iter().map(|b| format!("{b:02X}")).collect();
                let s = format!("{bp}{cur}{:04X} {bytes:<6} {}", l.addr, l.text);
                if l.addr == pc {
                    Line::styled(s, Style::default().add_modifier(Modifier::REVERSED))
                } else if bp == '●' {
                    Line::styled(s, Style::default().fg(Color::Red))
                } else {
                    Line::from(s)
                }
            })
            .collect();
        f.render_widget(
            Paragraph::new(items).block(Block::bordered().title(" Код ")),
            dis,
        );

        // Дамп.
        let rows = mem.height.saturating_sub(2) as usize;
        let cur = self.dbg.mem_cursor;
        let top = (cur & !7).wrapping_sub(8 * (rows as u16 / 2));
        let mut lines = Vec::new();
        for r in 0..rows {
            let a = top.wrapping_add(r as u16 * 8);
            let mut spans = vec![Span::raw(format!("{a:04X} "))];
            let mut ascii = String::new();
            for i in 0..8u16 {
                let ad = a.wrapping_add(i);
                let v = self.m.peek(ad);
                let txt = if ad == cur
                    && let Some(debugger::Edit { high: Some(h) }) = self.dbg.edit
                {
                    format!("{h:X}_")
                } else {
                    format!("{v:02X}")
                };
                let st = if ad == cur {
                    let s = Style::default().add_modifier(Modifier::REVERSED);
                    if self.dbg.edit.is_some() {
                        s.fg(Color::Yellow)
                    } else {
                        s
                    }
                } else {
                    Style::default()
                };
                spans.push(Span::raw(" "));
                spans.push(Span::styled(txt, st));
                ascii.push(ut88_core::screen::koi7_to_char(v));
            }
            spans.push(Span::raw(format!("  {ascii}")));
            lines.push(Line::from(spans));
        }
        let title = if self.dbg.edit.is_some() {
            " Память — правка (hex, Enter — конец) "
        } else {
            " Память "
        };
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(title)),
            mem,
        );

        let bl: Vec<String> = self
            .dbg
            .breakpoints
            .iter()
            .map(|a| format!("{a:04X}"))
            .collect();
        let pl: Vec<String> = self
            .dbg
            .port_breaks
            .iter()
            .map(|p| format!("{p:02X}"))
            .collect();
        let last = self
            .dbg
            .last_stop
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".into());
        let txt = vec![
            Line::from(format!(
                "Адреса: {}",
                if bl.is_empty() {
                    "—".into()
                } else {
                    bl.join(" ")
                }
            )),
            Line::from(format!(
                "Порты:  {}",
                if pl.is_empty() {
                    "—".into()
                } else {
                    pl.join(" ")
                }
            )),
            Line::from(format!("Останов: {last}")),
        ];
        f.render_widget(
            Paragraph::new(txt).block(Block::bordered().title(" Точки останова ")),
            bps,
        );
        let help_txt = vec![
            Line::from("s/F11 шаг  o обход CALL  g/Пробел пуск/стоп"),
            Line::from("r до адреса  b точка  p порт  x снять все"),
            Line::from("m дамп с адреса  e правка  стрелки/PgUp/PgDn"),
            Line::from("a код с адреса  c сброс Δ  Esc/F12 — выход"),
        ];
        f.render_widget(
            Paragraph::new(help_txt).block(Block::bordered().title(" Отладчик ")),
            help,
        );
    }

    fn draw_overlay(&self, f: &mut Frame, area: Rect, ov: &Overlay) {
        match ov {
            Overlay::Menu(sel) => {
                let mut items: Vec<ListItem> = MENU
                    .iter()
                    .map(|(k, label, _)| ListItem::new(format!(" {k}  {label}")))
                    .collect();
                items.push(ListItem::new(""));
                let keys = if self.pad_view {
                    " Ввод: 0–9 A–F — цифры, Backspace — ШАГ НАЗАД, F2 — УСТ"
                } else {
                    " Ввод: буквы/цифры — КОИ-7, рус. буквы и Alt+буква — РУС,"
                };
                items.push(ListItem::new(keys));
                if !self.pad_view {
                    items.push(ListItem::new(
                        " Ctrl+буква — УС, Enter — ВК, Del/End — СТР, Home — ↖",
                    ));
                }
                let r = centered(area, 62, items.len() as u16 + 2);
                self.list_popup(f, r, " Меню (буква или Enter, Esc — закрыть) ", items, *sel);
            }
            Overlay::Tape(sel) => {
                let items: Vec<ListItem> = TAPE_MENU
                    .iter()
                    .map(|(k, l)| ListItem::new(format!(" {k}  {l}")))
                    .collect();
                let r = centered(area, 52, items.len() as u16 + 2);
                self.list_popup(f, r, " Магнитофон ", items, *sel);
            }
            Overlay::Programs(list, sel) => {
                let items: Vec<ListItem> = list
                    .iter()
                    .map(|p| {
                        let ad = |a: Option<u16>| {
                            a.map(|a| format!("{a:04X}"))
                                .unwrap_or_else(|| "  — ".into())
                        };
                        let item = ListItem::new(format!(
                            " {:<16} {} → {}  {}",
                            p.name,
                            ad(p.load),
                            ad(p.start),
                            p.title
                        ));
                        if p.unsupported(self.m.config()).is_some() {
                            item.style(Style::default().fg(Color::DarkGray))
                        } else {
                            item
                        }
                    })
                    .collect();
                let mut items = items;
                let note = list
                    .get(*sel)
                    .and_then(|p| p.unsupported(self.m.config()))
                    .map(|w| format!(" ✗ {w}"))
                    .unwrap_or_default();
                items.push(ListItem::new(""));
                items.push(ListItem::new(note).style(Style::default().fg(Color::Red)));
                let r = centered(area, 76, items.len() as u16 + 2);
                self.list_popup(
                    f,
                    r,
                    " Программы: Enter — загрузить и запустить, L — только загрузить ",
                    items,
                    *sel,
                );
            }
            Overlay::Prompt(p) => {
                let r = centered(area, 60, 3);
                f.render_widget(Clear, r);
                let line = Line::from(vec![
                    Span::raw(p.text.clone()),
                    Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)),
                ]);
                f.render_widget(
                    Paragraph::new(line)
                        .block(Block::bordered().title(format!(" {} (Enter/Esc) ", p.title))),
                    r,
                );
            }
        }
    }

    fn list_popup(&self, f: &mut Frame, r: Rect, title: &str, items: Vec<ListItem>, sel: usize) {
        f.render_widget(Clear, r);
        let list = List::new(items)
            .block(Block::bordered().title(title.to_string()))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut st = ListState::default().with_selected(Some(sel));
        f.render_stateful_widget(list, r, &mut st);
    }
}

/// Название скорости.
pub fn speed_name(s: f64) -> String {
    if s == 0.0 {
        "макс.".into()
    } else {
        format!("×{s}")
    }
}

fn format_name(f: TapeFormat) -> &'static str {
    match f {
        TapeFormat::Monitor0 => "формат МОНИТОРА-0",
        TapeFormat::MonitorF => "формат МОНИТОРА-F",
    }
}

/// Тактов в кадре (для цикла реального времени).
pub const FRAME: u64 = FRAME_CYCLES;

/// Описание горячих клавиш (для справки).
pub fn hotkey_table() -> Vec<String> {
    HOTKEYS
        .iter()
        .map(|(k, _, d)| format!("{k:<9} {d}"))
        .collect()
}
