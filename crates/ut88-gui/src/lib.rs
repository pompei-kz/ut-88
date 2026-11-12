//! Графический интерфейс эмулятора ЮТ-88 (eframe/egui).
//!
//! Окно рисует компьютер «как на фотографиях журнала»: корпус процессорного модуля с
//! шестью красными семисегментными индикаторами и клавиатурой 4 × 4 + «ШАГ НАЗАД» +
//! «УСТ», а для полной конфигурации — ещё телевизор-монитор с экраном дисплейного модуля и
//! полную клавиатуру по рис. 2 из 1989-05. Меню: сброс, холодный старт, пауза, скорость,
//! магнитофон, снимки, программы из `programs/`, звук, конфигурация; отдельное окно
//! отладчика. Горячие клавиши F2–F12 — те же, что в TUI (см. [`keymap::HOTKEYS`]).

pub mod app;
pub mod debugger;
pub mod draw;
pub mod keymap;
pub mod loader;

use std::path::PathBuf;

use ut88_core::Machine;

/// Скорость эмуляции — множитель из общего ряда [`ut88_core::speed::SPEEDS`]
/// (тот же, что в TUI); `0` — максимум.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Speed(f64);

impl Speed {
    /// Реальное время.
    pub const X1: Speed = Speed(1.0);
    /// Максимум (насколько хватает процессора ПК).
    pub const MAX: Speed = Speed(0.0);

    /// Все варианты ряда.
    pub fn all() -> impl Iterator<Item = Speed> {
        ut88_core::speed::SPEEDS.into_iter().map(Speed)
    }

    /// Множитель (`None` — максимум).
    pub fn factor(self) -> Option<f64> {
        (self.0 > 0.0).then_some(self.0)
    }

    /// Подпись.
    pub fn label(self) -> String {
        ut88_core::speed::speed_label(self.0)
    }

    /// Ближайшая скорость ряда к множителю (`0` — максимум).
    pub fn from_factor(k: f64) -> Speed {
        Speed(ut88_core::speed::snap_speed(k))
    }

    /// Соседняя скорость ряда (`up` — быстрее).
    pub fn step(self, up: bool) -> Speed {
        Speed(ut88_core::speed::speed_step(self.0, up))
    }
}

/// Параметры запуска окна.
pub struct GuiOptions {
    /// Машина (уже собранная, с загруженными программами).
    pub machine: Machine,
    /// Скорость.
    pub speed: Speed,
    /// Звук.
    pub sound: bool,
    /// Папка с программами (`None` — искать автоматически).
    pub programs_dir: Option<PathBuf>,
}

/// Открыть окно эмулятора и работать до его закрытия.
pub fn run(opts: GuiOptions) -> anyhow::Result<()> {
    let full = opts.machine.config().display.is_some();
    let size = if full {
        [1280.0, 900.0]
    } else {
        [720.0, 680.0]
    };
    let native = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ЮТ-88")
            .with_inner_size(size)
            .with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ЮТ-88",
        native,
        Box::new(move |_cc| Ok(Box::new(app::App::new(opts)))),
    )
    .map_err(|e| anyhow::anyhow!("окно не открылось: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_from_factor() {
        assert_eq!(Speed::from_factor(1.0), Speed::X1);
        assert_eq!(Speed::from_factor(3.9).factor(), Some(4.0));
        assert_eq!(Speed::from_factor(16.0).factor(), Some(16.0));
        assert_eq!(Speed::from_factor(0.0), Speed::MAX);
        assert_eq!(Speed::all().count(), 8);
        assert_eq!(Speed::X1.step(false).factor(), Some(0.5));
    }
}
