//! Встроенные образы ПЗУ (файлы репозитория, вкомпилированные в крейт).

/// МОНИТОР модуля 1 в том виде, как напечатан в журнале (`monitor/monitor.bin`, 1 Кбайт).
pub const MONITOR: &[u8; 1024] = include_bytes!("../../../monitor/monitor.bin");

/// МОНИТОР модуля 1 с исправленным обработчиком «ШАГ НАЗАД»
/// (`monitor/monitor-fixed.bin`, 1 Кбайт) — используется по умолчанию.
pub const MONITOR_FIXED: &[u8; 1024] = include_bytes!("../../../monitor/monitor-fixed.bin");

/// МОНИТОР-«F» дисплейного модуля (`roms/monitor-f.bin`, 2 Кбайта, адреса F800H–FFFFH).
pub const MONITOR_F: &[u8; 2048] = include_bytes!("../../../roms/monitor-f.bin");

/// Знакогенератор дисплейного модуля (`roms/chargen.bin`, 2 Кбайта; на экран попадает
/// только первый килобайт — символы 00H–7FH).
pub const CHARGEN: &[u8; 2048] = include_bytes!("../../../roms/chargen.bin");

/// Библиотека калькулятора (`roms/calculator.bin`, 2 Кбайта, адреса 0800H–0FFFH).
pub const CALCULATOR: &[u8; 2048] = include_bytes!("../../../roms/calculator.bin");

/// Знакогенератор семисегментного индикатора К155РЕ3 (DD23), таблица 2 из
/// `doc/04-elektricheskaya-shema.md`: бит 0 — сегмент a, …, бит 6 — сегмент g.
pub const SEGMENTS: [u8; 16] = [
    0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F, 0x77, 0x7C, 0x39, 0x5E, 0x79, 0x71,
];

/// ОС ЮТ-88 (`roms/ut88-os.bin`, загрузочный модуль 3000H–58AFH, пуск 3000H; doc/30).
pub const UT88_OS: &[u8] = include_bytes!("../../../roms/ut88-os.bin");

/// CP/M-64 для квазидиска (`roms/cpm64.bin`, 3100H–4FFFH, пуск 3100H; doc/31, doc/32).
pub const CPM64: &[u8] = include_bytes!("../../../roms/cpm64.bin");

/// CP/M-35 без квазидиска (`roms/cpm35.bin`, 3400H–4C55H, пуск 4A00H; doc/33).
pub const CPM35: &[u8] = include_bytes!("../../../roms/cpm35.bin");

/// Программа CH.COM для CP/M (`roms/cpm-ch-com.bin`, 0100H–03FFH; doc/33).
pub const CPM_CH_COM: &[u8] = include_bytes!("../../../roms/cpm-ch-com.bin");
