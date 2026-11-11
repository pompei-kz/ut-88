//! Программируемый периферийный адаптер КР580ВВ55А — только режим 0 (doc/21).
//!
//! Внешние линии подаёт владелец ППА: [`Ppa::read`] получает уровни на входах каналов,
//! а выходные защёлки доступны через [`Ppa::port_a_out`] и т. п.

use serde::{Deserialize, Serialize};

/// Состояние ППА: защёлки выходов и управляющее слово.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ppa {
    /// Выходная защёлка канала A.
    pub a: u8,
    /// Выходная защёлка канала B.
    pub b: u8,
    /// Выходная защёлка канала C.
    pub c: u8,
    /// Управляющее слово (после сброса 9BH — все каналы на ввод).
    pub control: u8,
}

impl Default for Ppa {
    fn default() -> Self {
        Self {
            a: 0,
            b: 0,
            c: 0,
            control: 0x9B,
        }
    }
}

/// Уровни на входах каналов (то, что подаёт внешняя схема).
#[derive(Debug, Clone, Copy)]
pub struct PpaInputs {
    /// Входы канала A.
    pub a: u8,
    /// Входы канала B.
    pub b: u8,
    /// Входы канала C.
    pub c: u8,
}

impl Ppa {
    /// Сигнал RESET: все каналы на ввод, защёлки обнулены.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Канал A настроен на ввод.
    pub fn a_is_input(&self) -> bool {
        self.control & 0x10 != 0
    }
    /// Канал B настроен на ввод.
    pub fn b_is_input(&self) -> bool {
        self.control & 0x02 != 0
    }
    /// Младшая половина канала C на ввод.
    pub fn c_lo_is_input(&self) -> bool {
        self.control & 0x01 != 0
    }
    /// Старшая половина канала C на ввод.
    pub fn c_hi_is_input(&self) -> bool {
        self.control & 0x08 != 0
    }

    /// Уровни, которые ППА выдаёт на выводы канала A (линии в режиме ввода — 1, они
    /// подтянуты).
    pub fn port_a_out(&self) -> u8 {
        if self.a_is_input() { 0xFF } else { self.a }
    }

    /// Чтение регистра `reg` (0 — A, 1 — B, 2 — C, 3 — управляющее слово).
    pub fn read(&self, reg: u8, inputs: PpaInputs) -> u8 {
        match reg & 3 {
            0 => {
                if self.a_is_input() {
                    inputs.a
                } else {
                    self.a
                }
            }
            1 => {
                if self.b_is_input() {
                    inputs.b
                } else {
                    self.b
                }
            }
            2 => {
                let lo = if self.c_lo_is_input() {
                    inputs.c
                } else {
                    self.c
                } & 0x0F;
                let hi = if self.c_hi_is_input() {
                    inputs.c
                } else {
                    self.c
                } & 0xF0;
                lo | hi
            }
            // Регистр управляющего слова не читается; шина «висит».
            _ => 0xFF,
        }
    }

    /// Запись в регистр `reg`.
    pub fn write(&mut self, reg: u8, v: u8) {
        match reg & 3 {
            0 => self.a = v,
            1 => self.b = v,
            2 => self.c = v,
            _ => {
                if v & 0x80 != 0 {
                    // Установка режима: защёлки обнуляются.
                    self.control = v;
                    self.a = 0;
                    self.b = 0;
                    self.c = 0;
                } else {
                    // Установка/сброс отдельного разряда канала C.
                    let bit = (v >> 1) & 7;
                    if v & 1 != 0 {
                        self.c |= 1 << bit;
                    } else {
                        self.c &= !(1 << bit);
                    }
                }
            }
        }
    }
}
