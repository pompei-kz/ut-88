//! Описание конфигурации машины: из каких модулей она собрана.

use serde::{Deserialize, Serialize};

use crate::roms;

/// Образы ПЗУ дисплейного модуля.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayRoms {
    /// МОНИТОР-«F», 2 Кбайта (F800H–FFFFH).
    pub monitor_f: Vec<u8>,
    /// Знакогенератор, 2 Кбайта (используется первый килобайт).
    pub chargen: Vec<u8>,
}

impl Default for DisplayRoms {
    fn default() -> Self {
        Self {
            monitor_f: roms::MONITOR_F.to_vec(),
            chargen: roms::CHARGEN.to_vec(),
        }
    }
}

/// Конфигурация машины.
///
/// Готовые варианты — [`Config::minimal`] и [`Config::full`]; поля можно менять.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Образ ПЗУ МОНИТОРА модуля 1 (до 1 Кбайта; по умолчанию `monitor-fixed.bin`).
    pub monitor_rom: Vec<u8>,
    /// ПЗУ МОНИТОРА модуля 1 включено (0000H–07FFH: 1 Кбайт с повторением по A10).
    ///
    /// Выключение соответствует режиму «часы в МОНИТОРЕ-«F»» (doc/25) и третьей строке
    /// таблицы doc/15: сегмент 0000H–0FFFH отдаётся модулю ОЗУ.
    pub rom1_enabled: bool,
    /// ПЗУ библиотеки калькулятора в 0800H–0FFFH (2 Кбайта), если задано.
    pub calculator_rom: Option<Vec<u8>>,
    /// Дисплейный модуль (экранное ОЗУ E000H–EFFFH, ОЗУ F400H–F7FFH, ПЗУ F800H–FFFFH,
    /// клавиатурный ППА в портах 04H–07H).
    pub display: Option<DisplayRoms>,
    /// Модуль динамического ОЗУ 64 Кбайт (doc/15). Статическое ОЗУ модуля 1 при этом
    /// отключено, диапазоны других модулей блокируются.
    pub ram64: bool,
    /// Запросы прерывания от датчика секунд К176ИЕ5 подаются на процессор (тумблер из
    /// doc/25: при работе с магнитофоном прерывания можно отключить).
    pub clock_interrupts: bool,
    /// При включении питания положить в ОЗУ по адресу 0000H `DI; JMP F800H`, а по адресу
    /// 0038H — `JMP FFC1H` (как велит doc/25 для режима часов в МОНИТОРЕ-«F»). Действует,
    /// только если ПЗУ модуля 1 выключено и есть ОЗУ в 0000H.
    pub boot_stub: bool,
}

impl Config {
    /// Минимальная конфигурация: только модуль 1 (МОНИТОР `monitor-fixed.bin`).
    pub fn minimal() -> Self {
        Self {
            monitor_rom: roms::MONITOR_FIXED.to_vec(),
            rom1_enabled: true,
            calculator_rom: None,
            display: None,
            ram64: false,
            clock_interrupts: true,
            boot_stub: false,
        }
    }

    /// Полная конфигурация: модуль 1 + дисплейный модуль + ОЗУ 64 Кбайт.
    ///
    /// По умолчанию — режим «часы в МОНИТОРЕ-«F»»: ПЗУ модуля 1 выключено, ОЗУ занимает
    /// 0000H–DFFFH (56 Кбайт), в 0000H лежит `DI; JMP F800H`, так что «УСТ» запускает
    /// МОНИТОР-«F». Вариант с МОНИТОРОМ-«0» — [`Config::full_with_monitor0`].
    pub fn full() -> Self {
        Self {
            monitor_rom: roms::MONITOR_FIXED.to_vec(),
            rom1_enabled: false,
            calculator_rom: None,
            display: Some(DisplayRoms::default()),
            ram64: true,
            clock_interrupts: true,
            boot_stub: true,
        }
    }

    /// Полная конфигурация с включённым ПЗУ модуля 1 (первый режим из doc/25): сброс
    /// запускает МОНИТОР-«0», МОНИТОР-«F» запускают директивой «7 F800». ОЗУ — 1000H–DFFFH.
    pub fn full_with_monitor0() -> Self {
        Self {
            rom1_enabled: true,
            boot_stub: false,
            ..Self::full()
        }
    }

    /// Заменить образ ПЗУ МОНИТОРА модуля 1.
    pub fn with_monitor_rom(mut self, rom: &[u8]) -> Self {
        self.monitor_rom = rom.to_vec();
        self
    }

    /// Подключить ПЗУ калькулятора (встроенный образ `roms/calculator.bin`).
    pub fn with_calculator(mut self) -> Self {
        self.calculator_rom = Some(roms::CALCULATOR.to_vec());
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::minimal()
    }
}
