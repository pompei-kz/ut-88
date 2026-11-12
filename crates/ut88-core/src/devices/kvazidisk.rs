//! Электронный квазидиск (doc/32): 1–4 блока динамического ОЗУ по 64 Кбайт, которые
//! подключаются вместо основного ОЗУ в стековых машинных циклах.
//!
//! - Порт **40H** (только запись): разряды D0–D3 открывают блоки 1–4, активный уровень 0
//!   (код CP/M пишет FEH, FDH, FBH, F7H); FFH — квазидиск закрыт. После включения порт
//!   считается равным FFH.
//! - Пока хотя бы один из разрядов D0–D3 равен 0, **все** обращения к памяти в циклах с
//!   битом STACK слова состояния (`PUSH`, `POP`, `CALL`, `RET`, `RST`, `XTHL`, приём
//!   прерывания) идут в квазидиск по 16-битному адресу шины (значению SP); основное ОЗУ,
//!   ПЗУ и экран в этих циклах не выбираются.
//! - Если открыты сразу несколько блоков (журнал: «записано 00»), запись идёт во все, чтение —
//!   из блока с меньшим номером. Открытый, но не установленный блок читается как FFH.
//! - «УСТ» регистр порта не сбрасывает (в описании схемы сброс к триггерам не подведён),
//!   содержимое сохраняется.

use serde::{Deserialize, Serialize};

/// Объём одного блока квазидиска (64 Кбайт).
pub const BLOCK_SIZE: usize = 0x10000;

/// Порт управления квазидиском.
pub const PORT: u8 = 0x40;

/// Значение порта «квазидиск закрыт».
pub const CLOSED: u8 = 0xFF;

/// Квазидиск.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kvazidisk {
    /// Регистр порта 40H (разряды D0–D3).
    pub port: u8,
    /// Содержимое блоков подряд: блок 1 — `mem[0..64K]`, блок 2 — следующие 64 Кбайт, …
    pub mem: Vec<u8>,
}

impl Kvazidisk {
    /// Квазидиск из `blocks` блоков (1–4), содержимое — нули, порт закрыт.
    pub fn new(blocks: u8) -> Self {
        let blocks = usize::from(blocks.clamp(1, 4));
        Self {
            port: CLOSED,
            mem: vec![0; blocks * BLOCK_SIZE],
        }
    }

    /// Число установленных блоков.
    pub fn blocks(&self) -> usize {
        self.mem.len() / BLOCK_SIZE
    }

    /// Объём, Кбайт.
    pub fn size_kb(&self) -> usize {
        self.mem.len() / 1024
    }

    /// Квазидиск открыт: стековые циклы идут в него.
    pub fn is_open(&self) -> bool {
        self.port & 0x0F != 0x0F
    }

    /// Открытые блоки (номера 0–3).
    fn open_blocks(&self) -> impl Iterator<Item = usize> + '_ {
        (0..4).filter(move |b| self.port & (1 << b) == 0)
    }

    /// Чтение в стековом цикле (квазидиск должен быть открыт).
    pub fn read(&self, addr: u16) -> u8 {
        match self.open_blocks().next() {
            Some(b) => self
                .mem
                .get(b * BLOCK_SIZE + usize::from(addr))
                .copied()
                .unwrap_or(0xFF),
            None => 0xFF,
        }
    }

    /// Запись в стековом цикле (квазидиск должен быть открыт).
    pub fn write(&mut self, addr: u16, v: u8) {
        for b in 0..4 {
            if self.port & (1 << b) == 0
                && let Some(x) = self.mem.get_mut(b * BLOCK_SIZE + usize::from(addr))
            {
                *x = v;
            }
        }
    }
}
