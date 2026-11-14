//! Окно эмулятора: рисунок компьютера, меню, синхронизация с реальным временем.
//!
//! Кадр окна строит [`App::frame`] в контексте виджетов [`Ctx`] и не зависит от winit:
//! окно ОС и цикл кадров — в [`crate::platform`], а тесты гоняют кадры стендом
//! [`crate::ui::testing::Harness`] без окна.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ut88_audio::Audio;
use ut88_core::{
    CPU_HZ, Config, CpmDisk, FRAME_CYCLES, KbKey, Machine, PadKey, System, TapeFormat, screen,
};

use crate::debugger::{DbgAction, Debugger};
use crate::dialogs::{FileDialogs, RfdDialogs};
use crate::gfx::{Canvas, Color, Pos2, Rect};
use crate::keymap;
use crate::loader::{self, ProgramInfo, parse_hex_u16};
use crate::picture::{self, Hit, Hits, Layout};
use crate::settings::{self, Settings};
use crate::ui::{
    Button, CentralPanel, Ctx, CursorIcon, Event, Frame, Grid, KeyCode, MenuBar, Order, RichText,
    Sense, Slider, TextEdit, TopBottomPanel, Ui, Window,
};
use crate::{GuiOptions, Speed};

/// Минимальное время удержания клавиши после щелчка мышью, с (эмулированного времени).
const MOUSE_MIN_HOLD: f64 = 0.3;
/// Минимальное время удержания клавиши ПК, с (нажатие и отпускание могли прийти в одном
/// кадре окна).
const PC_MIN_HOLD: f64 = 0.06;
/// Бюджет эмуляции на кадр окна при конечной скорости (догон отставания), реального
/// времени.
const SPEED_BUDGET: Duration = Duration::from_millis(30);
/// Бюджет эмуляции на кадр окна на «максимуме»: начальный, наименьший и наибольший.
/// Подстраивается так, чтобы окно успевало к каждому обновлению экрана ПК (≈ 60 Гц).
const MAX_BUDGET_START: Duration = Duration::from_millis(12);
const MAX_BUDGET_MIN: Duration = Duration::from_millis(8);
const MAX_BUDGET_MAX: Duration = Duration::from_millis(14);
/// Кадр окна дольше этого — пропущено обновление экрана ПК: бюджет уменьшается.
const MAX_FRAME_MISSED: Duration = Duration::from_millis(22);
/// Шаг уменьшения бюджета при пропуске и увеличения — при успехе.
const MAX_BUDGET_DOWN: Duration = Duration::from_millis(1);
const MAX_BUDGET_UP: Duration = Duration::from_micros(50);
/// Порция эмуляции на «максимуме» — около миллисекунды реального времени, в кадрах
/// дисплея: наибольшая (наименьшая — один кадр).
const MAX_CHUNK_FRAMES: u64 = 64;

/// Следующий бюджет эмуляции на «максимуме» по длительности прошлого кадра окна:
/// медленно растёт, пока окно успевает, и заметно падает при пропуске.
fn next_max_budget(budget: Duration, frame: Duration) -> Duration {
    let b = if frame > MAX_FRAME_MISSED {
        budget.saturating_sub(MAX_BUDGET_DOWN)
    } else {
        budget + MAX_BUDGET_UP
    };
    b.clamp(MAX_BUDGET_MIN, MAX_BUDGET_MAX)
}

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

    /// Цвет свечения стекла телевизора.
    fn glow(self) -> Color {
        let (fg, _) = self.colors();
        Color::from_rgb(fg[0], fg[1], fg[2])
    }
}

/// От чего зависит кэш-слой рисунка компьютера (кроме нажатых клавиш).
#[derive(Debug, Clone, Copy, PartialEq)]
struct LayerKey {
    /// Прямоугольник рисунка, логические точки.
    rect: Rect,
    /// Размер холста кадра, физические пиксели.
    size: (u32, u32),
    /// Масштаб HiDPI.
    ppp: f32,
    /// Полная конфигурация (телевизор и клавиатура).
    full: bool,
    /// Люминофор (цвет свечения погасшего экрана).
    phosphor: Phosphor,
}

/// Кэш-слой статической части рисунка компьютера ([`picture::computer_static`]):
/// холст размером с кадр, рисунок на своём месте. Слой перерисовывается целиком при смене
/// [`LayerKey`], а при нажатии и отпускании клавиши — только эта клавиша
/// ([`picture::redraw_key`]). Каждый кадр слой копируется в кадр, а поверх рисуется
/// живая часть ([`picture::computer_live`]).
#[derive(Default)]
struct PictureCache {
    /// Слой (`None` — ещё не рисовался).
    canvas: Option<Canvas>,
    /// Для чего нарисован слой.
    key: Option<LayerKey>,
    /// Раскладка рисунка.
    layout: Option<Layout>,
    /// Зоны нажатия и вид клавиш слоя.
    hits: Hits,
    /// Клавиши, нарисованные на слое нажатыми.
    down: HashSet<Hit>,
    /// Сколько раз слой перерисован целиком.
    full_redraws: u64,
    /// Сколько раз перерисована одна клавиша.
    key_redraws: u64,
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
    /// Машина собирает перепады для звука.
    capturing: bool,
    /// Момент (такт 16 МГц) последней ресинхронизации генератора звука.
    resync_at: u64,
    /// В тестах: считать звуковой вывод открытым и включённым.
    #[cfg(test)]
    fake_sound: bool,
    /// Бюджет эмуляции на кадр окна на «максимуме».
    max_budget: Duration,
    /// Порция эмуляции на «максимуме», такты.
    max_chunk: u64,
    /// Сегменты индикаторов (обновляются при изменении).
    segs: [u8; 6],
    holds: HashMap<Hit, Hold>,
    mouse_down: Option<Hit>,
    /// Клавиатура ПК управляет модулем 1 даже в полной конфигурации.
    pc_to_pad: bool,
    /// Изображение экрана дисплейного модуля есть в [`App::screen_buf`].
    screen_ok: bool,
    /// Изображение экрана, RGBA 384 × 280 без умножения на альфу.
    screen_buf: Vec<u8>,
    /// Люминофор, которым нарисован экран (`None` — перерисовать).
    screen_phosphor: Option<Phosphor>,
    /// Кэш-слой рисунка компьютера.
    pic: PictureCache,
    /// Попросили выйти (F10): окно сохраняет квазидиск ([`App::on_exit`]) и закрывается.
    quit_requested: bool,
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
    /// Окно «Диск CP/M».
    cpm_open: bool,
    /// Файл образа квазидиска (сохраняется при выходе).
    kvazidisk_file: Option<PathBuf>,
    /// Файловые диалоги (в тестах — подставные).
    dialogs: Box<dyn FileDialogs>,
    /// Файл быстрого снимка (F6/F7).
    quick_snapshot: PathBuf,
    /// Как открыть звук, когда его включают в меню «Вид» (в тестах — без звуковой карты).
    open_audio: fn() -> Audio,
    /// Запоминаемые настройки (громкость, звук вкл/выкл).
    settings: Settings,
    /// Папка настроек (`None` — не запоминать).
    settings_dir: Option<PathBuf>,
}

impl App {
    /// Создать приложение.
    pub fn new(opts: GuiOptions) -> Self {
        let (settings, settings_warnings) = match opts.settings_dir.as_deref() {
            Some(d) => Settings::load(d),
            None => (Settings::default(), Vec::new()),
        };
        // Ключ `--no-sound` действует только на этот запуск: сохранённое `sound` не
        // меняется, пока звук не переключат в меню.
        let sound = opts.sound && settings.sound;
        let mut audio = if sound { Audio::new() } else { Audio::silent() };
        audio.set_volume(settings.volume);
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
            sound_wanted: sound,
            audio,
            capturing: false,
            resync_at: 0,
            #[cfg(test)]
            fake_sound: false,
            max_budget: MAX_BUDGET_START,
            max_chunk: FRAME_CYCLES,
            segs: [0; 6],
            holds: HashMap::new(),
            mouse_down: None,
            pc_to_pad: false,
            screen_ok: false,
            screen_buf: vec![0; screen::WIDTH * screen::HEIGHT * 4],
            screen_phosphor: None,
            pic: PictureCache::default(),
            quit_requested: false,
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
            cpm_open: false,
            kvazidisk_file: opts.kvazidisk_file,
            dialogs: Box::new(RfdDialogs::default()),
            quick_snapshot: PathBuf::from(QUICK_SNAPSHOT),
            open_audio: Audio::new,
            settings,
            settings_dir: opts.settings_dir,
        };
        if let Some(e) = app.audio.error()
            && sound
        {
            app.say(format!("Звук недоступен: {e}"));
        }
        for w in &settings_warnings {
            eprintln!("ut88: настройка взята по умолчанию: {w}");
        }
        if !settings_warnings.is_empty() {
            app.say(format!(
                "Настройка взята по умолчанию: {}",
                settings_warnings.join("; ")
            ));
        }
        app.sync_sound_capture();
        app
    }

    /// Заменить файловые диалоги (для тестов окна).
    pub fn set_file_dialogs(&mut self, d: Box<dyn FileDialogs>) {
        self.dialogs = d;
    }

    /// Задать файл быстрого снимка F6/F7 (по умолчанию [`QUICK_SNAPSHOT`] в текущей папке).
    pub fn set_quick_snapshot(&mut self, p: PathBuf) {
        self.quick_snapshot = p;
    }

    /// Задать, как открывать звук при включении в меню (по умолчанию [`Audio::new`]).
    pub fn set_audio_opener(&mut self, f: fn() -> Audio) {
        self.open_audio = f;
    }

    /// Записать файл параметра `name` в папку настроек (ошибка — в строку состояния и
    /// stderr).
    fn save_param(&mut self, name: &str, text: &str) {
        let Some(dir) = self.settings_dir.clone() else {
            return;
        };
        if let Err(e) = settings::write_param(&dir, name, text) {
            let msg = format!("Настройка не сохранена в {}: {e}", dir.join(name).display());
            eprintln!("ut88: {msg}");
            self.say(msg);
        }
    }

    /// Запомнить и сразу записать громкость (файл не трогается, если запись не меняется).
    fn set_volume_setting(&mut self, v: f32) {
        let old = settings::volume_text(self.settings.volume);
        self.settings.volume = v;
        let new = settings::volume_text(v);
        if new != old {
            self.save_param(settings::VOLUME_FILE, &new);
        }
    }

    /// Запомнить и сразу записать «звук вкл/выкл».
    fn set_sound_setting(&mut self, on: bool) {
        self.settings.sound = on;
        self.save_param(settings::SOUND_FILE, &settings::sound_text(on));
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

    /// Звук выводится (устройство открыто и звук включён).
    fn sound_out(&self) -> bool {
        #[cfg(test)]
        if self.fake_sound {
            return true;
        }
        self.audio.is_available() && self.audio.enabled()
    }

    /// Нужно ли собирать перепады для звука: звук выводится и скорость не «максимум»
    /// (на «максимуме» звук не выводится).
    fn want_capture(&self) -> bool {
        self.sound_out() && self.speed != Speed::MAX
    }

    /// Включить или выключить сбор перепадов по [`App::want_capture`] и начать звук
    /// заново с текущего момента (без щелчков и скачков от накопленного).
    fn sync_sound_capture(&mut self) {
        let on = self.want_capture();
        self.m.set_sound_capture(on);
        self.capturing = on;
        self.resync_audio();
    }

    fn resync_audio(&mut self) {
        self.resync_at = self.m.master_ticks();
        self.audio.resync(self.resync_at, self.m.tape_out_level());
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
        self.screen_phosphor = None;
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

    /// Обработка мыши по зонам клавиш (зоны — кэш-слоя рисунка). Нажатие действует,
    /// только если над рисунком нет окна или меню и оно не закрывает открытое меню.
    fn mouse_input(&mut self, ui: &Ui) {
        let (pos, down, pressed) =
            ui.input(|i| (i.interact_pos(), i.primary_down(), i.primary_pressed()));
        let ctx = ui.ctx();
        let on_picture = pos.is_some_and(|p| {
            ctx.layer_id_at(p)
                .is_none_or(|l| l.order == Order::Background)
        });
        let over = pos.and_then(|p| self.pic.hits.at(p)).filter(|_| on_picture);
        if over.is_some() {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }
        if pressed
            && let Some(h) = over
            && !ctx.press_blocked()
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

    /// Клавиатура ПК: физические коды клавиш (раскладка не важна). Пока фокус в поле
    /// ввода, клавиши не нажимают кнопок ЮТ-88; горячие клавиши действуют всегда.
    fn keyboard_input(&mut self, ctx: &Ctx) {
        let events = ctx.input(|i| i.events.clone());
        let typing = ctx.wants_keyboard_input();
        let to_kb = self.has_display() && !self.pc_to_pad;
        for ev in events {
            match ev {
                Event::Key(k) => {
                    if k.pressed && !k.repeat && self.hotkey(k.key, k.modifiers.shift) {
                        continue;
                    }
                    let hit = if to_kb {
                        keymap::kb_key(k.key).map(Hit::Kb)
                    } else {
                        keymap::pad_key(k.key).map(Hit::Pad)
                    };
                    let Some(h) = hit else { continue };
                    if k.pressed {
                        if !k.repeat && !typing {
                            self.press(h, false);
                        }
                    } else {
                        self.release(h, false);
                    }
                }
                Event::WindowFocused(false) => self.release_all_pc(),
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
    fn hotkey(&mut self, key: KeyCode, shift: bool) -> bool {
        match key {
            KeyCode::F2 => self.press(Hit::Pad(PadKey::Reset), false),
            KeyCode::F3 => {
                let c = self.m.cold_start_config();
                self.cold_start(c);
            }
            KeyCode::F4 => self.toggle_pause(),
            KeyCode::F5 => self.change_speed(!shift),
            // Некоторые раскладки присылают Shift+F5 как F17.
            KeyCode::F17 => self.change_speed(false),
            KeyCode::F6 => self.save_snapshot(&self.quick_snapshot.clone()),
            KeyCode::F7 => self.load_snapshot(&self.quick_snapshot.clone()),
            KeyCode::F8 => self.tape.open = !self.tape.open,
            KeyCode::F9 => self.programs_open = !self.programs_open,
            KeyCode::F10 => self.quit_requested = true,
            KeyCode::F11 => self.help_open = !self.help_open,
            KeyCode::F12 => self.dbg.open = !self.dbg.open,
            _ => return false,
        }
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

    fn tape_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let ins = self.m.tape_inserted();
            let label = if self.m.tape_playing() {
                "⏹ Стоп"
            } else {
                "▶ Воспроизведение"
            };
            if ui.add_enabled(ins, Button::new(label)).clicked() {
                self.tape_play_toggle();
            }
            if ui.add_enabled(ins, Button::new("⏮ В начало")).clicked() {
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
        self.resync_audio();
        self.say("Выполнение");
    }

    /// Прогнать эмуляцию за прошедшее реальное время.
    fn emulate(&mut self) {
        let now = Instant::now();
        let frame = now.duration_since(self.last_tick);
        let dt = frame.min(Duration::from_millis(100));
        self.last_tick = now;
        if self.capturing != self.want_capture() {
            // Переход на «максимум» и обратно, включение и выключение звука.
            self.sync_sound_capture();
        }
        if self.paused {
            self.apply_holds();
            return;
        }
        let start_cycles = self.m.cycles();
        let wall_start = Instant::now();
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
                    if wall_start.elapsed() > SPEED_BUDGET {
                        // Не успеваем — отбрасываем долг, чтобы окно не зависло.
                        self.debt = 0.0;
                        break;
                    }
                }
            }
            None => {
                self.max_budget = next_max_budget(self.max_budget, frame);
                while wall_start.elapsed() < self.max_budget {
                    self.apply_holds();
                    // Крупные порции — когда не нужны покомандные проверки отладчика и
                    // не держатся клавиши (их удержание отмеряется кадрами, как на ×1).
                    let n = if self.holds.is_empty() && !self.dbg.active() {
                        self.max_chunk
                    } else {
                        FRAME_CYCLES
                    };
                    if let Some(r) = self.dbg.run(&mut self.m, n) {
                        self.pause(&r);
                        self.dbg.open = true;
                        break;
                    }
                }
                // Порция — около миллисекунды при достигнутой скорости.
                let secs = wall_start.elapsed().as_secs_f64();
                if secs > 0.0 {
                    let rate = (self.m.cycles() - start_cycles) as f64 / secs;
                    self.max_chunk = ((rate / 1000.0) as u64)
                        .clamp(FRAME_CYCLES, MAX_CHUNK_FRAMES * FRAME_CYCLES);
                }
            }
        }
        self.apply_holds();
        let ran = (self.m.cycles() - start_cycles) as f64;
        if dt.as_secs_f64() > 0.0 {
            let inst = ran / CPU_HZ / dt.as_secs_f64();
            self.actual_speed = self.actual_speed * 0.9 + inst * 0.1;
        }
        // На «максимуме» и без звука перепады не собираются (буфер пуст).
        if self.capturing {
            let edges = self.m.drain_sound_edges();
            self.audio.set_speed(self.speed.factor());
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

    fn menu(&mut self, ui: &mut Ui) {
        MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Машина", |ui| {
                if ui.button("УСТ — сброс (F2)").clicked() {
                    self.press(Hit::Pad(PadKey::Reset), false);
                    ui.close();
                }
                if ui.button("Холодный старт — выкл./вкл. питания (F3)").clicked() {
                    let c = self.m.cold_start_config();
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
                // Щелчок по переключателю или флажку закрывает меню, как и по кнопке.
                for s in Speed::all() {
                    let r = ui.radio_value(&mut self.speed, s, s.label());
                    if r.changed() {
                        self.debt = 0.0;
                    }
                    if r.clicked() {
                        ui.close();
                    }
                }
                ui.separator();
                ui.label("Конфигурация (холодный старт):");
                let cur = self.config_kind();
                for (kind, label, mk) in [
                    (0, "минимальная (модуль 1)", Config::minimal as fn() -> Config),
                    (1, "полная (МОНИТОР-F, часы в нём)", Config::full),
                    (2, "полная + МОНИТОР модуля 1", Config::full_with_monitor0),
                    (3, "CP/M и ОС ЮТ-88 (полная + квазидиск 256К)", Config::cpm),
                ] {
                    if ui.radio(cur == kind, label).clicked() {
                        self.switch_config(mk());
                        ui.close();
                    }
                }
                let mut irq = self.m.config().clock_interrupts;
                let r = ui
                    .checkbox(&mut irq, "Прерывания от часов")
                    .on_hover_text("тумблер из doc/25: при работе с магнитофоном через МОНИТОР модуля 1 их отключают");
                if r.changed() {
                    self.m.set_clock_interrupts(irq);
                }
                if r.clicked() {
                    ui.close();
                }
                if self.has_display() {
                    let mut rom = self.m.config().monitor_f_rom;
                    let r = ui
                        .checkbox(&mut rom, "ПЗУ МОНИТОРА-F (сегмент F000H–FFFFH)")
                        .on_hover_text(
                            "тумблер из doc/30: выключенное ПЗУ отдаёт F000H–FFFFH модулю ОЗУ — \
                             так работает ОС ЮТ-88",
                        );
                    if r.changed() {
                        self.m.set_monitor_f_rom(rom);
                    }
                    if r.clicked() {
                        ui.close();
                    }
                    if ui
                        .checkbox(&mut self.pc_to_pad, "Клавиатура ПК → кнопки модуля 1")
                        .clicked()
                    {
                        ui.close();
                    }
                }
            });
            ui.menu_button("Магнитофон", |ui| {
                if ui.button("Вставить кассету (WAV)…").clicked() {
                    ui.close();
                    if let Some(p) = self.dialogs.pick_open("WAV", &["wav"]) {
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
                if ui.add_enabled(ins, Button::new(label)).clicked() {
                    self.tape_play_toggle();
                    ui.close();
                }
                if ui.add_enabled(ins, Button::new("Перемотать в начало")).clicked() {
                    self.tape_rewind();
                    ui.close();
                }
                if ui.add_enabled(ins, Button::new("Вынуть кассету")).clicked() {
                    self.m.tape_eject();
                    self.say("Кассета вынута");
                    ui.close();
                }
                ui.separator();
                if self.m.tape_recording() {
                    if ui.button("Остановить запись и сохранить WAV…").clicked() {
                        ui.close();
                        if let Some(sig) = self.m.tape_record_stop() {
                            match self.dialogs.pick_save("WAV", &["wav"], "record.wav") {
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
                    if let Some(p) = self.dialogs.pick_open("Программы", &["bin", "hex", "ihx"]) {
                        self.load_any_file(&p, false);
                    }
                }
                if ui.button("Загрузить и запустить…").clicked() {
                    ui.close();
                    if let Some(p) = self.dialogs.pick_open("Программы", &["bin", "hex", "ihx"]) {
                        self.load_any_file(&p, true);
                    }
                }
                ui.separator();
                if ui.button("Быстрый снимок (F6)").clicked() {
                    self.save_snapshot(&self.quick_snapshot.clone());
                    ui.close();
                }
                if ui.button("Загрузить быстрый снимок (F7)").clicked() {
                    self.load_snapshot(&self.quick_snapshot.clone());
                    ui.close();
                }
                if ui.button("Сохранить снимок…").clicked() {
                    ui.close();
                    if let Some(p) = self.dialogs.pick_save("Снимок ЮТ-88", &["snap"], "ut88.snap") {
                        self.save_snapshot(&p);
                    }
                }
                if ui.button("Загрузить снимок…").clicked() {
                    ui.close();
                    if let Some(p) = self.dialogs.pick_open("Снимок ЮТ-88", &["snap"]) {
                        self.load_snapshot(&p);
                    }
                }
            });
            ui.menu_button("CP/M", |ui| self.cpm_menu(ui));
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
                let r = ui.checkbox(&mut on, "Звук");
                if r.clicked() {
                    ui.close();
                }
                if r.changed() {
                    if on && !self.audio.is_available() {
                        self.audio = (self.open_audio)();
                        if let Some(e) = self.audio.error() {
                            let e = e.to_string();
                            self.fail(format!("Звук недоступен: {e}"));
                        }
                    }
                    self.audio.set_volume(self.settings.volume);
                    self.sound_wanted = on;
                    self.audio.set_enabled(on);
                    self.sync_sound_capture();
                    self.set_sound_setting(on);
                }
                let mut vol = self.audio.volume();
                if ui
                    .add(Slider::new(&mut vol, 0.0..=1.0).text("громкость"))
                    .changed()
                {
                    self.audio.set_volume(vol);
                    self.set_volume_setting(self.audio.volume());
                }
                ui.separator();
                ui.label("Люминофор:");
                for (ph, label) in [(Phosphor::Green, "зелёный"), (Phosphor::White, "белый")] {
                    if ui.radio_value(&mut self.phosphor, ph, label).clicked() {
                        ui.close();
                    }
                }
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
    fn programs_list(&mut self, ui: &mut Ui, in_menu: bool) {
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
            let mut buttons = |ui: &mut Ui| {
                let mut clicked = false;
                if ui.add_enabled(ok, Button::new("Загрузить")).clicked() {
                    self.load_program(p, false);
                    clicked = true;
                }
                if ui
                    .add_enabled(
                        ok && p.start.is_some(),
                        Button::new("Загрузить и запустить"),
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
                            RichText::new("нужна полная конфигурация").color(Color::LIGHT_RED),
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
                            RichText::new("нужна полная конфигурация").color(Color::LIGHT_RED),
                        );
                    }
                });
            }
        }
    }

    fn programs_window(&mut self, ctx: &Ctx) {
        let mut open = self.programs_open;
        Window::new("Программы из журналов")
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| self.programs_list(ui, false));
        self.programs_open = open && self.programs_open;
    }

    fn config_kind(&self) -> u8 {
        let c = self.m.config();
        match (c.display.is_some(), c.rom1_enabled, c.kvazidisk > 0) {
            (false, _, _) => 0,
            (true, false, false) => 1,
            (true, true, _) => 2,
            (true, false, true) => 3,
        }
    }

    fn config_name(&self) -> &'static str {
        match self.config_kind() {
            0 => "минимальная",
            1 => "полная",
            2 => "полная + МОНИТОР-0",
            _ => "CP/M (квазидиск)",
        }
    }

    // ----- CP/M и ОС ЮТ-88 -----

    fn cpm_menu(&mut self, ui: &mut Ui) {
        for (sys, keep, label) in [
            (
                System::Cpm64,
                true,
                "Загрузить CP/M-64 (файлы на квазидиске сохраняются)",
            ),
            (
                System::Cpm64,
                false,
                "Загрузить CP/M-64 с чистым квазидиском",
            ),
            (
                System::Cpm35,
                true,
                "Загрузить CP/M-35 (диск в ОЗУ 5000H–DBFFH)",
            ),
            (System::Ut88Os, true, "Загрузить ОС ЮТ-88"),
        ] {
            if ui.button(label).clicked() {
                ui.close();
                self.boot(sys, keep);
            }
        }
        ui.separator();
        if ui.button("Диск CP/M: файлы…").clicked() {
            ui.close();
            self.cpm_open = true;
        }
        if ui.button("Записать файл на диск CP/M…").clicked() {
            ui.close();
            self.cpm_put_dialog();
        }
        ui.separator();
        let has_kd = self.m.has_kvazidisk();
        if ui
            .add_enabled(has_kd, Button::new("Сохранить образ квазидиска…"))
            .clicked()
        {
            ui.close();
            let def = self.kvazidisk_name();
            if let Some(p) = self.dialogs.pick_save("Образ квазидиска", &["img"], &def)
            {
                self.save_kvazidisk(&p);
            }
        }
        if ui
            .add_enabled(has_kd, Button::new("Загрузить образ квазидиска…"))
            .clicked()
        {
            ui.close();
            if let Some(p) = self.dialogs.pick_open("Образ квазидиска", &["img"]) {
                match self.m.load_kvazidisk_file(&p) {
                    Ok(()) => {
                        self.kvazidisk_file = Some(p.clone());
                        self.say(format!(
                            "Образ квазидиска загружен: {} — запустите CP/M-64 с сохранением файлов",
                            p.display()
                        ));
                    }
                    Err(e) => self.fail(format!("Образ не загружен: {e}")),
                }
            }
        }
    }

    fn kvazidisk_name(&self) -> String {
        self.kvazidisk_file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "kvazidisk.img".into())
    }

    fn save_kvazidisk(&mut self, p: &Path) {
        match self.m.save_kvazidisk_file(p) {
            Ok(()) => {
                self.kvazidisk_file = Some(p.to_path_buf());
                self.say(format!("Образ квазидиска сохранён: {}", p.display()));
            }
            Err(e) => self.fail(format!("Образ не сохранён: {e}")),
        }
    }

    /// Загрузить систему; при неподходящей конфигурации — холодный старт в «CP/M».
    fn boot(&mut self, sys: System, keep: bool) {
        if let Some(c) = sys.config_for(self.m.config()) {
            self.cold_start(c);
        }
        self.m.release_all_keys();
        self.holds.clear();
        match self.m.boot_system(sys, keep) {
            Ok(msg) => {
                self.resume();
                self.say(msg);
            }
            Err(e) => self.fail(format!("{} не загружена: {e}", sys.title())),
        }
    }

    fn cpm_put_dialog(&mut self) {
        let Some(p) = self.dialogs.pick_open(
            "Файлы CP/M",
            &["com", "COM", "txt", "TXT", "asm", "ASM", "bas", "BAS"],
        ) else {
            return;
        };
        match self.m.cpm_put_file(&p, None) {
            Ok(msg) => self.say(msg),
            Err(e) => self.fail(format!("Файл не записан: {e}")),
        }
    }

    fn cpm_window(&mut self, ctx: &Ctx) {
        let mut open = self.cpm_open;
        let disk = self.m.cpm_default_disk();
        let list = self.m.cpm_list(disk);
        let mut extract: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut put = false;
        Window::new("Диск CP/M")
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(match (disk, self.m.cpm_running()) {
                    (CpmDisk::Kvazidisk, Some(_)) => "Квазидиск работающей CP/M-64",
                    (CpmDisk::Kvazidisk, None) => "Квазидиск (CP/M-64 не запущена)",
                    (CpmDisk::Ram35, Some(_)) => "Диск работающей CP/M-35 (ОЗУ 5000H–DBFFH)",
                    (CpmDisk::Ram35, None) => "Диск CP/M-35 в ОЗУ (CP/M не запущена)",
                });
                match &list {
                    Ok(files) if files.is_empty() => {
                        ui.label("Файлов нет.");
                    }
                    Ok(files) => {
                        Grid::new("cpm-files").striped(true).show(ui, |ui| {
                            for f in files {
                                ui.monospace(format!("{:<12}", f.name));
                                ui.monospace(format!("{:>6} байт", f.bytes()));
                                if ui.small_button("Сохранить на ПК…").clicked() {
                                    extract = Some(f.name.clone());
                                }
                                if ui.small_button("Удалить").clicked() {
                                    delete = Some(f.name.clone());
                                }
                                ui.end_row();
                            }
                        });
                    }
                    Err(e) => {
                        ui.label(format!("Нет диска: {e}"));
                    }
                }
                ui.separator();
                if ui.button("Записать файл с ПК…").clicked() {
                    put = true;
                }
                ui.label(
                    "Файл записывается под своим именем (8.3, заглавными буквами); \
                     работающая CP/M после этого перезапускается, чтобы перечитать каталог.",
                );
            });
        self.cpm_open = open;
        if put {
            self.cpm_put_dialog();
        }
        if let Some(name) = extract
            && let Some(p) = self.dialogs.pick_save("Все файлы", &["*"], &name)
        {
            match self.m.cpm_read_file(disk, &name) {
                Ok(data) => match std::fs::write(&p, data) {
                    Ok(()) => self.say(format!("{name} сохранён в {}", p.display())),
                    Err(e) => self.fail(format!("{}: {e}", p.display())),
                },
                Err(e) => self.fail(e),
            }
        }
        if let Some(name) = delete {
            match self.m.cpm_delete_file(disk, &name) {
                Ok(_) => {
                    let restarted = self.m.cpm_warm_boot();
                    self.say(format!(
                        "{name} удалён{}",
                        if restarted {
                            "; CP/M перезапущена"
                        } else {
                            ""
                        }
                    ));
                }
                Err(e) => self.fail(e),
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let state = if self.paused {
                RichText::new("⏸ пауза").color(Color::YELLOW)
            } else {
                RichText::new("▶ работа").color(Color::LIGHT_GREEN)
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
    fn fkey_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            for &(label, key, shift, hint) in keymap::FKEY_BAR {
                let text = RichText::new(label).monospace();
                if ui.button(text).on_hover_text(hint).clicked() {
                    self.hotkey(key, shift);
                }
            }
        });
    }

    fn help_window(&mut self, ctx: &Ctx) {
        Window::new("Справка: клавиши")
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
                Grid::new("hot").striped(true).show(ui, |ui| {
                    for (k, what) in keymap::HOTKEYS {
                        ui.monospace(*k);
                        ui.label(*what);
                        ui.end_row();
                    }
                });
                ui.heading("Модуль 1 (минимальная конфигурация)");
                Grid::new("pad").striped(true).show(ui, |ui| {
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
                Grid::new("kb").striped(true).show(ui, |ui| {
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

    fn tape_window(&mut self, ctx: &Ctx) {
        let mut open = self.tape.open;
        let mut actions: Vec<u8> = Vec::new();
        Window::new("Магнитофон и файлы")
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
                let d = &mut *self.dialogs;
                ui.heading("Быстрая загрузка .bin");
                path_row(ui, d, &mut t.load_path, true, "bin");
                ui.horizontal(|ui| {
                    ui.label("адрес:");
                    ui.add(TextEdit::singleline(&mut t.load_addr).desired_width(60.0));
                    if ui.button("Загрузить").clicked() {
                        actions.push(1);
                    }
                    if ui.button("Загрузить и запустить").clicked() {
                        actions.push(2);
                    }
                });
                ui.separator();
                ui.heading("Сохранить диапазон памяти в .bin");
                path_row(ui, d, &mut t.save_path, false, "bin");
                ui.horizontal(|ui| {
                    ui.label("с");
                    ui.add(TextEdit::singleline(&mut t.save_start).desired_width(60.0));
                    ui.label("по");
                    ui.add(TextEdit::singleline(&mut t.save_end).desired_width(60.0));
                    if ui.button("Сохранить").clicked() {
                        actions.push(3);
                    }
                });
                ui.separator();
                ui.heading("WAV из .bin (запись для директив A / I)");
                ui.label("файл .bin:");
                path_row(ui, d, &mut t.gen_bin, true, "bin");
                ui.label("файл .wav:");
                path_row(ui, d, &mut t.gen_wav, false, "wav");
                ui.horizontal(|ui| {
                    ui.label("адрес начала:");
                    ui.add(TextEdit::singleline(&mut t.gen_addr).desired_width(60.0));
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
                anyhow::ensure!(
                    data.len() <= 0x10000,
                    "файл больше 64 Кбайт ({} байт)",
                    data.len()
                );
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
                    addr.wrapping_add((data.len() - 1) as u16)
                ))
            }
            5 => {
                let p = PathBuf::from(t.gen_wav.trim());
                self.insert_tape(&p)
            }
            _ => Ok(String::new()),
        }
    }

    /// Обновить изображение экрана и сегменты индикаторов — только если они изменились
    /// (не чаще кадра окна).
    fn update_screen(&mut self) {
        if self.m.take_indicators_changed() {
            self.segs = self.m.indicator_segments();
        }
        let changed = self.m.take_screen_changed();
        if !changed
            && self.screen_phosphor == Some(self.phosphor)
            && (self.screen_ok || !self.has_display())
        {
            return;
        }
        self.screen_phosphor = Some(self.phosphor);
        let (fg, bg) = self.phosphor.colors();
        self.screen_ok = self.m.render_screen(&mut self.screen_buf, fg, bg);
    }

    /// Клавиши, которые рисуются нажатыми.
    fn drawn_down(&self) -> HashSet<Hit> {
        self.holds
            .iter()
            .filter(|(_, x)| x.applied || x.pc || x.mouse || x.sticky)
            .map(|(&h, _)| h)
            .collect()
    }

    /// Привести кэш-слой рисунка в соответствие с прямоугольником `rect`, масштабом
    /// и нажатыми клавишами. `size` — размер холста кадра в пикселях.
    fn update_picture_layer(&mut self, rect: Rect, size: (u32, u32), ppp: f32) {
        let key = LayerKey {
            rect,
            size,
            ppp,
            full: self.has_display(),
            phosphor: self.phosphor,
        };
        let down = self.drawn_down();
        let pic = &mut self.pic;
        let mut whole = pic.key != Some(key) || pic.canvas.is_none() || pic.layout.is_none();
        if !whole && down != pic.down {
            let (Some(c), Some(layout)) = (pic.canvas.as_mut(), pic.layout.as_ref()) else {
                unreachable!()
            };
            for &h in down.symmetric_difference(&pic.down) {
                if picture::redraw_key(c, layout, &pic.hits, h, down.contains(&h)).is_none() {
                    whole = true;
                    break;
                }
                pic.key_redraws += 1;
            }
            pic.down = down.clone();
        }
        if whole {
            let c = pic
                .canvas
                .get_or_insert_with(|| Canvas::new(size.0, size.1));
            c.resize(size.0, size.1);
            c.set_scale(ppp);
            c.clear(Color::TRANSPARENT);
            let layout = Layout::new(rect, key.full);
            let mut hits = Hits::default();
            picture::computer_static(
                c,
                &layout,
                self.phosphor.glow(),
                &PressedSet(&down),
                &mut hits,
            );
            pic.hits = hits;
            pic.layout = Some(layout);
            pic.key = Some(key);
            pic.down = down;
            pic.full_redraws += 1;
        }
    }

    /// Рисунок компьютера: кэш-слой статической части и живая часть поверх.
    fn computer(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, Sense::hover());
        let ppp = ui.ctx().pixels_per_point();
        let size = ui.ctx().with_canvas(|c| (c.width(), c.height()));
        self.update_picture_layer(rect, size, ppp);
        let screen = self.screen_ok.then_some((
            self.screen_buf.as_slice(),
            screen::WIDTH as u32,
            screen::HEIGHT as u32,
        ));
        let glow = self.phosphor.glow();
        let segs = self.segs;
        let pic = &self.pic;
        if let (Some(layer), Some(layout)) = (pic.canvas.as_ref(), pic.layout.as_ref()) {
            ui.canvas_at(rect, |c| {
                c.draw_canvas(layer, Pos2::ZERO);
                picture::computer_live(c, layout, segs, screen, glow);
            });
        }
        self.mouse_input(ui);
    }
}

/// Нажатые клавиши для рисования.
struct PressedSet<'a>(&'a HashSet<Hit>);

impl picture::Pressed for PressedSet<'_> {
    fn is_down(&self, h: Hit) -> bool {
        self.0.contains(&h)
    }
}

fn path_row(ui: &mut Ui, dialogs: &mut dyn FileDialogs, path: &mut String, open: bool, ext: &str) {
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(path)
                .desired_width(380.0)
                .hint_text("путь к файлу"),
        );
        if ui.button("Обзор…").clicked() {
            let p = if open {
                dialogs.pick_open(ext, &[ext])
            } else {
                dialogs.pick_save(ext, &[ext], &format!("ut88.{ext}"))
            };
            if let Some(p) = p {
                *path = p.display().to_string();
            }
        }
    });
}

impl App {
    /// Кадр главного окна: клавиатура, эмуляция за прошедшее время, меню, строка
    /// состояния, панель F-клавиш, рисунок компьютера и плавающие окна.
    pub fn frame(&mut self, ctx: &Ctx) {
        self.keyboard_input(ctx);
        self.emulate();
        self.update_screen();

        TopBottomPanel::top("menu").show(ctx, |ui| self.menu(ui));
        TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));
        TopBottomPanel::bottom("fkeys").show(ctx, |ui| self.fkey_bar(ui));
        CentralPanel::default()
            .frame(Frame::NONE)
            .show(ctx, |ui| self.computer(ui));

        if self.help_open {
            self.help_window(ctx);
        }
        if self.tape.open {
            self.tape_window(ctx);
        }
        if self.programs_open {
            self.programs_window(ctx);
        }
        if self.cpm_open {
            self.cpm_window(ctx);
        }
        if self.about_open {
            Window::new("О программе")
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
            Window::new("Ошибка")
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
    }

    /// Окно отладчика открыто (F12, меню «Вид», останов в точке).
    pub fn debugger_open(&self) -> bool {
        self.dbg.open
    }

    /// Закрыть окно отладчика (крестик окна ОС).
    pub fn close_debugger(&mut self) {
        self.dbg.open = false;
    }

    /// Кадр окна отладчика в своём контексте `ctx` (отдельное окно ОС или стенд тестов).
    /// Эмуляция в нём не идёт — только в [`App::frame`].
    pub fn debugger_frame(&mut self, ctx: &Ctx) {
        let paused = self.paused;
        let a = self.dbg.frame(ctx, &mut self.m, paused);
        self.do_debug_action(a);
    }

    /// Попросили выйти (F10 или кнопка «F10 Выход»).
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Перед выходом: сохранить образ квазидиска в его файл (если он задан).
    pub fn on_exit(&mut self) {
        if let Some(p) = self.kvazidisk_file.clone()
            && self.m.has_kvazidisk()
        {
            match self.m.save_kvazidisk_file(&p) {
                Ok(()) => eprintln!("ut88: образ квазидиска сохранён в {}", p.display()),
                Err(e) => eprintln!("ut88: образ квазидиска не сохранён: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod gui_tests;
