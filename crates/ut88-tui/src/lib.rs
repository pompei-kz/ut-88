//! Терминальный интерфейс эмулятора ЮТ-88 (ratatui + crossterm).
//!
//! - минимальная конфигурация — шесть семисегментных индикаторов крупными цифрами и
//!   подсказка клавиатуры модуля 1;
//! - полная конфигурация — экран дисплейного модуля 64 × 28 текстом;
//! - горячие клавиши F2–F12 (см. [`keymap::HOTKEYS`]; схема общая с GUI), меню F11
//!   дублирует их буквами; нижняя строка — панель кнопок этих клавиш в стиле Midnight
//!   Commander, щелчок мышью по кнопке равен нажатию (меню и справка доступны и тогда,
//!   когда терминал забирает F11 себе);
//! - отладчик (F12) — отдельный экран;
//! - меню CP/M (пункт `c` меню F11): загрузка CP/M-64, CP/M-35, ОС ЮТ-88, файлы на диске
//!   CP/M, образ квазидиска.
//!
//! Вход — [`run`].

pub mod app;
pub mod debugger;
pub mod input;
pub mod keymap;
pub mod programs;
pub mod widgets;

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};
use ut88_core::{CPU_HZ, FRAME_CYCLES, Machine};

pub use app::App;

/// Параметры запуска TUI.
pub struct Options {
    /// Множитель скорости относительно реального времени; 0 — максимум. Приводится к
    /// ближайшему значению общего ряда [`ut88_core::speed::SPEEDS`].
    pub speed: f64,
    /// Звук (в TUI не выводится, флаг принимается для совместимости).
    pub sound: bool,
    /// Папка с программами из журналов (`None` — искать `programs/`).
    pub programs_dir: Option<PathBuf>,
    /// Как собрать машину при «холодном старте» (`None` — `Machine::new` с той же
    /// конфигурацией, без устройств расширения).
    pub cold_start: Option<Box<dyn Fn() -> Machine>>,
    /// Файл быстрого снимка (F6/F7).
    pub quick_snapshot: PathBuf,
    /// Файл образа квазидиска: сохраняется при выходе (и по пункту меню CP/M).
    pub kvazidisk_file: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            speed: 1.0,
            sound: false,
            programs_dir: None,
            cold_start: None,
            quick_snapshot: PathBuf::from(QUICK_SNAPSHOT),
            kvazidisk_file: None,
        }
    }
}

/// Файл быстрого снимка по умолчанию — тот же, что у GUI.
pub const QUICK_SNAPSHOT: &str = "ut88-quick.snap";

/// Длительность кадра дисплея (20,16 мс).
fn frame_duration() -> Duration {
    Duration::from_secs_f64(FRAME_CYCLES as f64 / CPU_HZ)
}

/// Наибольшее отставание от реального времени, которое догоняется, с.
const MAX_LAG: f64 = 0.25;

/// Восстановление терминала при выходе (в том числе по панике).
struct TermGuard {
    kitty: bool,
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        restore_modes(self.kitty);
        ratatui::restore();
    }
}

/// Выключить захват мыши и расширенный режим клавиатуры (при выходе и по панике).
fn restore_modes(kitty: bool) {
    if kitty {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    let _ = execute!(io::stdout(), DisableMouseCapture);
}

/// Обработать событие терминала.
pub fn handle_event(app: &mut App, ev: Event) {
    match ev {
        Event::Key(k) => app.on_key(k),
        Event::Mouse(me) => app.on_mouse(me),
        Event::FocusLost => app.input.clear(&mut app.m),
        _ => {}
    }
}

/// Бюджет эмуляции за шаг главного цикла на «максимуме»: события терминала
/// обрабатываются не реже, отрисовка — по [`MAX_DRAW_PERIOD`].
const MAX_BUDGET: Duration = Duration::from_millis(10);
/// На «максимуме» терминал перерисовывается не чаще (≈ 30 Гц).
const MAX_DRAW_PERIOD: Duration = Duration::from_millis(33);
/// На «максимуме» строка состояния (такты меняются постоянно) сама по себе вызывает
/// перерисовку не чаще (≈ 4 Гц).
const MAX_STATUS_PERIOD: Duration = Duration::from_millis(250);
/// Порция эмуляции быстрого пути на «максимуме» — около миллисекунды реального времени,
/// в кадрах дисплея: наибольшая (наименьшая — один кадр).
const MAX_CHUNK_FRAMES: u64 = 64;

/// Ход главного цикла: опорная точка реального времени, когда и что перерисовывать.
pub struct Pacer {
    /// (момент, такт машины, скорость, машина идёт).
    base: (Instant, u64, f64, bool),
    /// Порция быстрого пути на «максимуме», такты.
    chunk: u64,
    /// Последняя отрисовка.
    last_draw: Option<Instant>,
    /// Последнее сравнение строки состояния.
    last_status_check: Instant,
    /// Нарисованная строка состояния.
    status: String,
    /// Нарисованное нажатие.
    pressed: Option<input::Target>,
    /// Были события терминала после отрисовки.
    dirty: bool,
}

/// Текст строки состояния.
fn status_text(app: &App) -> String {
    app.status_line()
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect()
}

/// На «максимуме» и машина идёт.
fn at_max(app: &App) -> bool {
    app.speed == 0.0 && app.running()
}

/// Можно ли прогонять машину крупными порциями без покомандных проверок: нет точек
/// останова, ничего не нажато и не ждёт в очереди (удержание кнопок модуля 1 следит
/// за чтением порта клавиатуры по командам).
fn fast_path(app: &App) -> bool {
    !app.dbg.active() && app.input.current().is_none() && app.input.pending() == 0
}

impl Pacer {
    /// Новый ход с текущего момента.
    pub fn new(app: &App) -> Self {
        Self {
            base: (Instant::now(), app.m.cycles(), app.speed, app.running()),
            chunk: FRAME_CYCLES,
            last_draw: None,
            last_status_check: Instant::now(),
            status: String::new(),
            pressed: None,
            dirty: true,
        }
    }

    /// Отметить, что интерфейс изменился (пришло событие терминала).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Пора ли перерисовать терминал: что-то изменилось (события, экран, индикаторы,
    /// нажатие, строка состояния; отладчик и окна поверх — всегда), на «максимуме» — не
    /// чаще [`MAX_DRAW_PERIOD`], а строка состояния — не чаще [`MAX_STATUS_PERIOD`].
    pub fn should_draw(&mut self, app: &mut App) -> bool {
        let now = Instant::now();
        let max = at_max(app);
        if max
            && let Some(t) = self.last_draw
            && now.duration_since(t) < MAX_DRAW_PERIOD
        {
            return false;
        }
        let screen = app.m.take_screen_changed();
        let ind = app.m.take_indicators_changed();
        let mut need = self.dirty
            || self.last_draw.is_none()
            || screen
            || ind
            || app.view != app::View::Machine
            || app.overlay.is_some()
            || app.input.current() != self.pressed;
        if !need && (!max || now.duration_since(self.last_status_check) >= MAX_STATUS_PERIOD) {
            self.last_status_check = now;
            need = status_text(app) != self.status;
        }
        need
    }

    /// Терминал перерисован.
    pub fn drawn(&mut self, app: &App) {
        let now = Instant::now();
        self.last_draw = Some(now);
        self.last_status_check = now;
        self.status = status_text(app);
        self.pressed = app.input.current();
        self.dirty = false;
    }

    /// Прогнать машину за прошедшее реальное время (на «максимуме» — сколько успеется
    /// за [`MAX_BUDGET`]).
    pub fn emulate(&mut self, app: &mut App) {
        // Смена скорости, паузы или машины — новая опорная точка.
        let running = app.running();
        let base = &mut self.base;
        if base.2 != app.speed || base.3 != running || app.m.cycles() < base.1 {
            *base = (Instant::now(), app.m.cycles(), app.speed, running);
        }
        if !running {
            return;
        }
        if app.speed > 0.0 {
            let elapsed = base.0.elapsed().as_secs_f64();
            let mut target = base.1 + (elapsed * app.speed * CPU_HZ) as u64;
            let lag = target.saturating_sub(app.m.cycles());
            if lag as f64 > MAX_LAG * app.speed * CPU_HZ {
                // Не догоняем большое отставание (машина медленнее реального времени
                // или процесс спал): продолжаем с текущего момента.
                target = app.m.cycles() + FRAME_CYCLES;
                *base = (Instant::now(), target, app.speed, running);
            }
            app.run_to_cycle(target);
        } else {
            // Максимум: сколько успеется за бюджет. Без точек останова и нажатий —
            // крупными порциями, иначе покомандно кадрами (как на ×1).
            let start = (Instant::now(), app.m.cycles());
            let until = start.0 + MAX_BUDGET;
            while Instant::now() < until && app.running() {
                if fast_path(app) {
                    app.resume = false;
                    app.m.run_cycles(self.chunk);
                } else {
                    let t = app.m.cycles() + FRAME_CYCLES;
                    app.run_to_cycle(t);
                }
            }
            let secs = start.0.elapsed().as_secs_f64();
            if secs > 0.0 {
                let rate = (app.m.cycles() - start.1) as f64 / secs;
                self.chunk =
                    ((rate / 1000.0) as u64).clamp(FRAME_CYCLES, MAX_CHUNK_FRAMES * FRAME_CYCLES);
            }
        }
        app.update_meter();
    }
}

/// Один шаг главного цикла: отрисовка (если пора), события терминала, эмуляция.
/// `events(ждать)` — следующее событие или `None`, если за это время его не было.
///
/// На ×1 и других конечных скоростях и в паузе события ждутся до конца кадра дисплея; на
/// «максимуме» — только уже пришедшие, без ожидания.
pub fn frame<B: Backend>(
    term: &mut Terminal<B>,
    app: &mut App,
    pacer: &mut Pacer,
    events: &mut impl FnMut(Duration) -> io::Result<Option<Event>>,
) -> io::Result<()> {
    if pacer.should_draw(app) {
        term.draw(|f| app.draw(f))?;
        pacer.drawn(app);
    }
    let deadline = if at_max(app) {
        Instant::now()
    } else {
        Instant::now() + frame_duration()
    };
    // События до конца кадра.
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Some(ev) = events(left)? else {
            break;
        };
        handle_event(app, ev);
        pacer.mark_dirty();
        if app.quit {
            return Ok(());
        }
    }
    pacer.emulate(app);
    Ok(())
}

/// Запустить терминальный интерфейс. Возвращается по F10 (или «q» в меню F11).
pub fn run(machine: Machine, opts: Options) -> anyhow::Result<()> {
    // ratatui::init ставит обработчик паники, восстанавливающий терминал.
    let mut term = ratatui::init();
    let kitty = matches!(terminal::supports_keyboard_enhancement(), Ok(true))
        && execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )
        .is_ok();
    // Мышь — для панели кнопок горячих клавиш внизу экрана.
    let _ = execute!(io::stdout(), EnableMouseCapture);
    let _guard = TermGuard { kitty };
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_modes(kitty);
        prev(info);
    }));

    let mut app = App::new(machine, opts);
    app.kitty = kitty;
    if app.opts.sound {
        app.say("Звук в терминальном интерфейсе не выводится");
    }
    let mut pacer = Pacer::new(&app);
    let mut events = |left: Duration| -> io::Result<Option<Event>> {
        if event::poll(left)? {
            event::read().map(Some)
        } else {
            Ok(None)
        }
    };
    while !app.quit {
        frame(&mut term, &mut app, &mut pacer, &mut events)?;
    }
    drop(_guard);
    if let Some(p) = &app.opts.kvazidisk_file
        && app.m.has_kvazidisk()
    {
        match app.m.save_kvazidisk_file(p) {
            Ok(()) => eprintln!("ut88: образ квазидиска сохранён в {}", p.display()),
            Err(e) => eprintln!("ut88: образ квазидиска не сохранён: {e}"),
        }
    }
    Ok(())
}
