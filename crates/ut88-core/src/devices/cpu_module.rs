//! Модуль 1 (процессорный): ПЗУ МОНИТОРА, ОЗУ 1 Кбайт, индикаторы, клавиатура,
//! магнитофон (doc/02, doc/04).

use serde::{Deserialize, Serialize};

use crate::keys::PadKey;
use crate::roms::SEGMENTS;

/// Состояние модуля 1 (без процессора и часов — они в [`crate::Machine`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuModule {
    /// ПЗУ МОНИТОРА (DD10, DD11), 1 Кбайт.
    pub rom: Vec<u8>,
    /// Статическое ОЗУ C000H–C3FFH (DD13, DD14).
    pub ram: Vec<u8>,
    /// Регистровая память индикаторов DD18/DD19: 4 ячейки по A0–A1 (видны первые три).
    pub indicators: [u8; 4],
    /// Нажатые кнопки клавиатуры (кроме «УСТ»).
    pub pressed: Vec<PadKey>,
    /// Выход на магнитофон (триггер DD4.2).
    pub tape_out: bool,
}

impl CpuModule {
    /// Новый модуль с образом ПЗУ `rom` (дополняется FFH до 1 Кбайта).
    pub fn new(rom: &[u8]) -> Self {
        let mut r = vec![0xFF; 1024];
        let n = rom.len().min(1024);
        r[..n].copy_from_slice(&rom[..n]);
        Self {
            rom: r,
            ram: vec![0; 1024],
            indicators: [0; 4],
            pressed: Vec::new(),
            tape_out: false,
        }
    }

    /// Байт, читаемый из порта клавиатуры A0H.
    ///
    /// Одна нажатая кнопка — её код; несколько — объединение кодов по ИЛИ (упрощение:
    /// реальные ПЗУ DD15/DD16 при нескольких нажатиях выдают 0, а DD17 добавляет свои
    /// разряды).
    pub fn keyboard(&self) -> u8 {
        self.pressed.iter().fold(0, |a, k| a | k.code())
    }

    /// Шесть цифр индикатора слева направо (H, L, A).
    pub fn digits(&self) -> [u8; 6] {
        let [a, l, h, _] = self.indicators;
        [h >> 4, h & 15, l >> 4, l & 15, a >> 4, a & 15]
    }

    /// Сегменты шести разрядов слева направо (бит 0 — a, …, бит 6 — g).
    pub fn segments(&self) -> [u8; 6] {
        self.digits().map(|d| SEGMENTS[usize::from(d)])
    }
}
