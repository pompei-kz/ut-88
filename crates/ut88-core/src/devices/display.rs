//! Дисплейный модуль: экранное ОЗУ, ОЗУ и ПЗУ МОНИТОРА-«F», знакогенератор, клавиатура на
//! КР580ВВ55А (doc/20–29, главный источник — doc/29).

use serde::{Deserialize, Serialize};

use super::ppa::{Ppa, PpaInputs};
use crate::config::DisplayRoms;
use crate::keys::KbKey;

/// Состояние дисплейного модуля.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayModule {
    /// Экранное ОЗУ 2 Кбайта (адрес = A & 07FFH, окно E000H–EFFFH).
    pub vram: Vec<u8>,
    /// ОЗУ МОНИТОРА F400H–F7FFH.
    pub ram: Vec<u8>,
    /// ПЗУ МОНИТОРА-«F» F800H–FFFFH.
    pub rom: Vec<u8>,
    /// Знакогенератор (2 Кбайта; используется первый килобайт).
    pub chargen: Vec<u8>,
    /// ППА клавиатуры DD19 (порты 04H–07H).
    pub kb_ppa: Ppa,
    /// ППА дополнительного интерфейса (порты F8H–FBH) — заглушка, входы = FFH.
    pub ext_ppa: Ppa,
    /// Нажатые клавиши.
    pub pressed: Vec<KbKey>,
}

fn padded(src: &[u8], len: usize) -> Vec<u8> {
    let mut v = vec![0xFF; len];
    let n = src.len().min(len);
    v[..n].copy_from_slice(&src[..n]);
    v
}

impl DisplayModule {
    /// Новый модуль с образами ПЗУ.
    pub fn new(roms: &DisplayRoms) -> Self {
        Self {
            vram: vec![0; 2048],
            ram: vec![0; 1024],
            rom: padded(&roms.monitor_f, 2048),
            chargen: padded(&roms.chargen, 2048),
            kb_ppa: Ppa::default(),
            ext_ppa: Ppa::default(),
            pressed: Vec::new(),
        }
    }

    /// Сброс (сигнал RESET ППА).
    pub fn reset(&mut self) {
        self.kb_ppa.reset();
        self.ext_ppa.reset();
    }

    /// Входы канала B: строки матрицы (0 — замкнута) для столбцов, выбранных нулём на
    /// канале A; бит 7 читается как 0 (doc/29).
    fn rows(&self) -> u8 {
        let cols = self.kb_ppa.port_a_out();
        let mut rows = 0x7F;
        for k in &self.pressed {
            if let Some((c, r)) = k.position()
                && cols & (1 << c) == 0
            {
                rows &= !(1 << r);
            }
        }
        rows
    }

    /// Входы канала C: бит 0 — РУС, 1 — УС, 2 — СС (0 — нажата).
    fn modifiers(&self) -> u8 {
        let mut v = 0xFF;
        for k in &self.pressed {
            match k {
                KbKey::Rus => v &= !1,
                KbKey::Us => v &= !2,
                KbKey::Ss => v &= !4,
                KbKey::Matrix(_) => {}
            }
        }
        v
    }

    /// Чтение порта; `None` — порт не принадлежит модулю.
    pub fn port_read(&self, port: u8) -> Option<u8> {
        match port {
            // Адрес ППА «вывернут»: регистр = 3 − (порт & 3) (doc/29).
            0x04..=0x07 => {
                let inputs = PpaInputs {
                    a: 0xFF,
                    b: self.rows(),
                    c: self.modifiers(),
                };
                Some(self.kb_ppa.read(3 - (port & 3), inputs))
            }
            0xF8..=0xFB => {
                let inputs = PpaInputs {
                    a: 0xFF,
                    b: 0xFF,
                    c: 0xFF,
                };
                Some(self.ext_ppa.read(port & 3, inputs))
            }
            _ => None,
        }
    }

    /// Запись в порт; `false` — порт не принадлежит модулю.
    pub fn port_write(&mut self, port: u8, v: u8) -> bool {
        match port {
            0x04..=0x07 => {
                self.kb_ppa.write(3 - (port & 3), v);
                true
            }
            0xF8..=0xFB => {
                self.ext_ppa.write(port & 3, v);
                true
            }
            _ => false,
        }
    }
}
