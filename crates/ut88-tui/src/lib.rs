//! Терминальный интерфейс эмулятора ЮТ-88 (ratatui + crossterm).
//!
//! - минимальная конфигурация — шесть семисегментных индикаторов крупными цифрами и
//!   подсказка клавиатуры модуля 1;
//! - полная конфигурация — экран дисплейного модуля 64 × 28 текстом;
//! - горячие клавиши F2–F12 (см. [`keymap::HOTKEYS`]; схема общая с GUI), меню F11
//!   дублирует их буквами; нижняя строка — панель кнопок этих клавиш в стиле Midnight
//!   Commander, щелчок мышью по кнопке равен нажатию (меню и справка доступны и тогда,
//!   когда терминал забирает F11 себе);
//! - отладчик (F12) — отдельный экран.
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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            speed: 1.0,
            sound: false,
            programs_dir: None,
            cold_start: None,
            quick_snapshot: PathBuf::from(QUICK_SNAPSHOT),
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
    let frame = frame_duration();
    // Опорная точка реального времени: (момент, такт машины).
    let mut base = (Instant::now(), app.m.cycles(), app.speed, app.running());

    while !app.quit {
        term.draw(|f| app.draw(f))?;
        let deadline = Instant::now() + frame;
        // События до конца кадра.
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if !event::poll(left)? {
                break;
            }
            match event::read()? {
                Event::Key(k) => app.on_key(k),
                Event::Mouse(me) => app.on_mouse(me),
                Event::FocusLost => app.input.clear(&mut app.m),
                _ => {}
            }
            if app.quit {
                break;
            }
        }
        if app.quit {
            break;
        }
        // Смена скорости, паузы или машины — новая опорная точка.
        let running = app.running();
        if base.2 != app.speed || base.3 != running || app.m.cycles() < base.1 {
            base = (Instant::now(), app.m.cycles(), app.speed, running);
        }
        if !running {
            continue;
        }
        if app.speed > 0.0 {
            let elapsed = base.0.elapsed().as_secs_f64();
            let mut target = base.1 + (elapsed * app.speed * CPU_HZ) as u64;
            let lag = target.saturating_sub(app.m.cycles());
            if lag as f64 > MAX_LAG * app.speed * CPU_HZ {
                // Не догоняем большое отставание (машина медленнее реального времени
                // или процесс спал): продолжаем с текущего момента.
                target = app.m.cycles() + FRAME_CYCLES;
                base = (Instant::now(), target, app.speed, running);
            }
            app.run_to_cycle(target);
        } else {
            // Максимум: считать кадрами, пока не истечёт ≈ 3/4 кадра реального времени.
            let until = Instant::now() + frame * 3 / 4;
            while Instant::now() < until && app.running() {
                let t = app.m.cycles() + FRAME_CYCLES;
                app.run_to_cycle(t);
            }
        }
        app.update_meter();
    }
    Ok(())
}
