//! Клавиши: 17 кнопок модуля 1 и полная клавиатура дисплейного модуля.

use serde::{Deserialize, Serialize};

/// Кнопки клавиатуры модуля 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PadKey {
    /// Шестнадцатеричная цифра 0–F.
    Hex(u8),
    /// «ШАГ НАЗАД».
    StepBack,
    /// «УСТ» — сброс процессора (в порт клавиатуры не попадает).
    Reset,
}

impl PadKey {
    /// Код, который читается из порта A0H, пока нажата только эта кнопка
    /// (doc/04, табл. 3: «0» → 10H, 1–F → 01H–0FH, «ШАГ НАЗАД» → 80H).
    pub fn code(self) -> u8 {
        match self {
            PadKey::Hex(0) => 0x10,
            PadKey::Hex(d) => d & 0x0F,
            PadKey::StepBack => 0x80,
            PadKey::Reset => 0,
        }
    }
}

/// Клавиши полной клавиатуры дисплейного модуля (doc/22, рис. 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KbKey {
    /// Клавиша матрицы с номером N = 7 × столбец + строка (столбец 0–7 — канал А,
    /// строка 0–6 — канал В); N = 0–55.
    Matrix(u8),
    /// РУС (канал С, бит 0).
    Rus,
    /// УС (канал С, бит 1).
    Us,
    /// СС (канал С, бит 2).
    Ss,
}

/// Коды клавиш N = 48–55 (последний столбец матрицы), doc/27, таблица FE43H.
const SPECIAL: [u8; 8] = [0x20, 0x18, 0x08, 0x19, 0x1A, 0x0D, 0x1F, 0x0C];

impl KbKey {
    /// Клавиша по столбцу (0–7) и строке (0–6).
    pub fn at(column: u8, row: u8) -> Self {
        KbKey::Matrix(column * 7 + row)
    }

    /// Столбец и строка клавиши матрицы.
    pub fn position(self) -> Option<(u8, u8)> {
        match self {
            KbKey::Matrix(n) if n < 56 => Some((n / 7, n % 7)),
            _ => None,
        }
    }

    /// Код, который МОНИТОР-«F» выдаёт для клавиши без модификаторов (`None` для РУС,
    /// УС, СС).
    pub fn base_code(self) -> Option<u8> {
        match self {
            KbKey::Matrix(n) if n < 48 => {
                let c = n + 0x30;
                Some(if (0x3C..=0x3F).contains(&c) {
                    c & 0x2F
                } else {
                    c
                })
            }
            KbKey::Matrix(n) if n < 56 => Some(SPECIAL[usize::from(n - 48)]),
            _ => None,
        }
    }

    /// Клавиша и модификатор, которыми набирается код `code` (КОИ-7) в МОНИТОРЕ-«F».
    ///
    /// Возвращает `None`, если такой код с клавиатуры не набрать.
    pub fn for_code(code: u8) -> Option<(KbKey, Option<KbKey>)> {
        let plain = |c: u8| (0u8..56).find(|&n| KbKey::Matrix(n).base_code() == Some(c));
        if let Some(n) = plain(code) {
            return Some((KbKey::Matrix(n), None));
        }
        match code {
            // РУС: 40H–5FH → 60H–7FH.
            0x60..=0x7F => plain(code - 0x20).map(|n| (KbKey::Matrix(n), Some(KbKey::Rus))),
            // СС: 30H–3BH → 20H–2BH.
            0x20..=0x2B => plain(code + 0x10).map(|n| (KbKey::Matrix(n), Some(KbKey::Ss))),
            // СС: 2CH–2FH → 3CH–3FH.
            0x3C..=0x3F => plain(code - 0x10).map(|n| (KbKey::Matrix(n), Some(KbKey::Ss))),
            // УС: код AND 1FH (от букв 40H–5FH).
            0x00..=0x1F => plain(code | 0x40).map(|n| (KbKey::Matrix(n), Some(KbKey::Us))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_codes() {
        assert_eq!(KbKey::Matrix(0).base_code(), Some(0x30));
        assert_eq!(KbKey::Matrix(11).base_code(), Some(0x3B));
        assert_eq!(KbKey::Matrix(12).base_code(), Some(0x2C));
        assert_eq!(KbKey::Matrix(15).base_code(), Some(0x2F));
        assert_eq!(KbKey::Matrix(16).base_code(), Some(0x40));
        assert_eq!(KbKey::Matrix(47).base_code(), Some(0x5F));
        assert_eq!(KbKey::Matrix(53).base_code(), Some(0x0D));
        // «7» — столбец 1, строка 0 (doc/22).
        assert_eq!(KbKey::at(1, 0).base_code(), Some(b'7'));
        assert_eq!(KbKey::for_code(b'A'), Some((KbKey::Matrix(17), None)));
        assert_eq!(
            KbKey::for_code(0x61),
            Some((KbKey::Matrix(17), Some(KbKey::Rus)))
        );
        assert_eq!(
            KbKey::for_code(b'!'),
            Some((KbKey::Matrix(1), Some(KbKey::Ss)))
        );
        assert_eq!(
            KbKey::for_code(b'>'),
            Some((KbKey::Matrix(14), Some(KbKey::Ss)))
        );
        assert_eq!(
            KbKey::for_code(0x03),
            Some((KbKey::Matrix(19), Some(KbKey::Us)))
        );
        assert_eq!(KbKey::for_code(b' '), Some((KbKey::Matrix(48), None)));
    }
}
