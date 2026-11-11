//! Графический интерфейс эмулятора ЮТ-88 (eframe/egui).
//!
//! Окно рисует компьютер «как на фотографиях журнала»: корпус процессорного модуля с
//! шестью красными семисегментными индикаторами и клавиатурой 4 × 4 + «ШАГ НАЗАД» +
//! «УСТ», а для полной конфигурации — ещё телевизор-монитор с экраном дисплейного модуля и
//! полную клавиатуру по рис. 2 из 1989-05. Меню: сброс, холодный старт, пауза, скорость,
//! магнитофон, снимки, программы из `programs/`, звук, конфигурация; отдельное окно
//! отладчика.

pub mod app;
pub mod debugger;
pub mod draw;
pub mod keymap;
pub mod loader;

use std::path::PathBuf;

use ut88_core::Machine;

/// Скорость эмуляции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    /// Реальное время.
    X1,
    /// ×2.
    X2,
    /// ×4.
    X4,
    /// ×8.
    X8,
    /// Максимум (насколько хватает процессора ПК).
    Max,
}

impl Speed {
    /// Все варианты.
    pub const ALL: [Speed; 5] = [Speed::X1, Speed::X2, Speed::X4, Speed::X8, Speed::Max];

    /// Множитель (`None` — максимум).
    pub fn factor(self) -> Option<f64> {
        match self {
            Speed::X1 => Some(1.0),
            Speed::X2 => Some(2.0),
            Speed::X4 => Some(4.0),
            Speed::X8 => Some(8.0),
            Speed::Max => None,
        }
    }

    /// Подпись.
    pub fn label(self) -> &'static str {
        match self {
            Speed::X1 => "×1",
            Speed::X2 => "×2",
            Speed::X4 => "×4",
            Speed::X8 => "×8",
            Speed::Max => "максимум",
        }
    }

    /// Ближайшая скорость к множителю (`0` — максимум).
    pub fn from_factor(k: f64) -> Speed {
        if k <= 0.0 || k > 8.0 {
            return Speed::Max;
        }
        *Speed::ALL[..4]
            .iter()
            .min_by(|a, b| {
                let da = (a.factor().unwrap_or(0.0) - k).abs();
                let db = (b.factor().unwrap_or(0.0) - k).abs();
                da.total_cmp(&db)
            })
            .unwrap_or(&Speed::X1)
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
        assert_eq!(Speed::from_factor(3.9), Speed::X4);
        assert_eq!(Speed::from_factor(0.0), Speed::Max);
        assert_eq!(Speed::from_factor(100.0), Speed::Max);
    }
}
