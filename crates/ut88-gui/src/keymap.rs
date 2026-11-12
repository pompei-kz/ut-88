//! Соответствие клавиш ПК клавишам ЮТ-88.
//!
//! Клавиши берутся по физическому положению (как на американской раскладке), поэтому
//! переключатель раскладки ПК не мешает. Клавиша ЮТ-88 держится нажатой, пока нажата
//! клавиша ПК («Тетрис» опрашивает клавиатуру редко).

use eframe::egui::Key;
use ut88_core::{KbKey, PadKey};

/// Клавиатура модуля 1 (минимальная конфигурация): клавиша ПК → кнопка.
pub const PAD_KEYS: &[(Key, PadKey)] = &[
    (Key::Num0, PadKey::Hex(0)),
    (Key::Num1, PadKey::Hex(1)),
    (Key::Num2, PadKey::Hex(2)),
    (Key::Num3, PadKey::Hex(3)),
    (Key::Num4, PadKey::Hex(4)),
    (Key::Num5, PadKey::Hex(5)),
    (Key::Num6, PadKey::Hex(6)),
    (Key::Num7, PadKey::Hex(7)),
    (Key::Num8, PadKey::Hex(8)),
    (Key::Num9, PadKey::Hex(9)),
    (Key::A, PadKey::Hex(0xA)),
    (Key::B, PadKey::Hex(0xB)),
    (Key::C, PadKey::Hex(0xC)),
    (Key::D, PadKey::Hex(0xD)),
    (Key::E, PadKey::Hex(0xE)),
    (Key::F, PadKey::Hex(0xF)),
    (Key::Backspace, PadKey::StepBack),
];

/// Полная клавиатура дисплейного модуля: клавиша ПК → код КОИ-7 основного символа
/// клавиши ЮТ-88 (клавиша матрицы находится через [`KbKey::for_code`]).
pub const KB_KEYS: &[(Key, u8)] = &[
    (Key::Num0, b'0'),
    (Key::Num1, b'1'),
    (Key::Num2, b'2'),
    (Key::Num3, b'3'),
    (Key::Num4, b'4'),
    (Key::Num5, b'5'),
    (Key::Num6, b'6'),
    (Key::Num7, b'7'),
    (Key::Num8, b'8'),
    (Key::Num9, b'9'),
    (Key::A, b'A'),
    (Key::B, b'B'),
    (Key::C, b'C'),
    (Key::D, b'D'),
    (Key::E, b'E'),
    (Key::F, b'F'),
    (Key::G, b'G'),
    (Key::H, b'H'),
    (Key::I, b'I'),
    (Key::J, b'J'),
    (Key::K, b'K'),
    (Key::L, b'L'),
    (Key::M, b'M'),
    (Key::N, b'N'),
    (Key::O, b'O'),
    (Key::P, b'P'),
    (Key::Q, b'Q'),
    (Key::R, b'R'),
    (Key::S, b'S'),
    (Key::T, b'T'),
    (Key::U, b'U'),
    (Key::V, b'V'),
    (Key::W, b'W'),
    (Key::X, b'X'),
    (Key::Y, b'Y'),
    (Key::Z, b'Z'),
    (Key::Semicolon, b';'),
    (Key::Quote, b':'),
    (Key::Minus, b'-'),
    (Key::Equals, b'_'),
    (Key::Comma, b','),
    (Key::Period, b'.'),
    (Key::Slash, b'/'),
    (Key::Backslash, b'\\'),
    (Key::OpenBracket, b'['),
    (Key::CloseBracket, b']'),
    (Key::Backtick, b'@'),
    (Key::Insert, b'^'),
    (Key::Space, b' '),
    (Key::Enter, 0x0D),
    (Key::Backspace, 0x08),
    (Key::ArrowLeft, 0x08),
    (Key::ArrowRight, 0x18),
    (Key::ArrowUp, 0x19),
    (Key::ArrowDown, 0x1A),
    (Key::Home, 0x0C),
    (Key::Delete, 0x1F),
    (Key::End, 0x1F),
];

/// Клавиши-модификаторы ПК → модификаторы ЮТ-88 (для таблицы «Справка»).
pub const MODIFIERS: &[(&str, KbKey)] = &[
    ("Shift", KbKey::Ss),
    ("Ctrl", KbKey::Us),
    ("Alt", KbKey::Rus),
];

/// Горячие клавиши эмулятора (для таблицы «Справка»). Схема общая с TUI
/// (`ut88_tui::keymap::HOTKEYS`).
pub const HOTKEYS: &[(&str, &str)] = &[
    ("F1", "окно «Справка» (клавиши)"),
    ("F2", "кнопка «УСТ» (сброс процессора)"),
    ("F3", "холодный старт (выключить и включить питание)"),
    ("F4", "пауза / продолжить"),
    ("F5", "скорость выше"),
    ("Shift+F5", "скорость ниже"),
    ("F6", "быстрый снимок состояния (ut88-quick.snap)"),
    ("F7", "загрузить быстрый снимок"),
    ("F8", "окно «Магнитофон и файлы»"),
    ("F9", "окно «Программы из журналов»"),
    ("F10", "выход"),
    ("F12", "окно отладчика"),
];

/// Клавиша полной клавиатуры для клавиши ПК.
pub fn kb_key(key: Key) -> Option<KbKey> {
    KB_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|&(_, c)| KbKey::for_code(c))
        .map(|(k, _)| k)
}

/// Кнопка модуля 1 для клавиши ПК.
pub fn pad_key(key: Key) -> Option<PadKey> {
    PAD_KEYS.iter().find(|(k, _)| *k == key).map(|&(_, p)| p)
}

/// Подпись клавиши ЮТ-88 по коду её основного символа.
pub fn code_label(code: u8) -> String {
    match code {
        b' ' => "ПРОБЕЛ".into(),
        0x0D => "ВК".into(),
        0x08 => "←".into(),
        0x18 => "→".into(),
        0x19 => "↑".into(),
        0x1A => "↓".into(),
        0x0C => "↖".into(),
        0x1F => "СТР".into(),
        c => char::from(c).to_string(),
    }
}

/// Название кнопки модуля 1.
pub fn pad_label(k: PadKey) -> String {
    match k {
        PadKey::Hex(d) => format!("{d:X}"),
        PadKey::StepBack => "ШАГ НАЗАД".into(),
        PadKey::Reset => "УСТ".into(),
    }
}

/// Название модификатора.
pub fn kb_label(k: KbKey) -> String {
    match k {
        KbKey::Rus => "РУС".into(),
        KbKey::Us => "УС".into(),
        KbKey::Ss => "СС".into(),
        KbKey::Matrix(_) => k.base_code().map(code_label).unwrap_or_default(),
    }
}

/// Название клавиши ПК.
pub fn pc_label(key: Key) -> String {
    match key {
        Key::Semicolon => ";".into(),
        Key::Quote => "'".into(),
        Key::Minus => "-".into(),
        Key::Equals => "=".into(),
        Key::Comma => ",".into(),
        Key::Period => ".".into(),
        Key::Slash => "/".into(),
        Key::Backslash => "\\".into(),
        Key::OpenBracket => "[".into(),
        Key::CloseBracket => "]".into(),
        Key::Backtick => "`".into(),
        k => k.name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kb_mapping_resolves_to_plain_key() {
        for &(k, code) in KB_KEYS {
            let (kb, modifier) =
                KbKey::for_code(code).unwrap_or_else(|| panic!("{k:?} → {code:02X}"));
            assert!(
                modifier.is_none(),
                "{k:?}: код {code:02X} требует модификатора"
            );
            assert_eq!(kb.base_code(), Some(code));
        }
    }

    #[test]
    fn all_matrix_keys_reachable_from_pc() {
        let mut seen = [false; 56];
        for &(k, _) in KB_KEYS {
            if let Some(KbKey::Matrix(n)) = kb_key(k) {
                seen[usize::from(n)] = true;
            }
        }
        let missing: Vec<usize> = (0..56).filter(|&n| !seen[n]).collect();
        assert!(missing.is_empty(), "без клавиши ПК: {missing:?}");
    }

    #[test]
    fn pad_mapping() {
        assert_eq!(pad_key(Key::Num0), Some(PadKey::Hex(0)));
        assert_eq!(pad_key(Key::F), Some(PadKey::Hex(15)));
        assert_eq!(pad_key(Key::Backspace), Some(PadKey::StepBack));
        assert_eq!(pad_key(Key::G), None);
        let mut hex: Vec<u8> = PAD_KEYS
            .iter()
            .filter_map(|(_, p)| match p {
                PadKey::Hex(d) => Some(*d),
                _ => None,
            })
            .collect();
        hex.sort();
        assert_eq!(hex, (0..16).collect::<Vec<_>>());
    }

    #[test]
    fn no_duplicate_pc_keys() {
        let mut keys: Vec<Key> = KB_KEYS.iter().map(|k| k.0).collect();
        keys.sort_by_key(|k| *k as u32);
        keys.dedup();
        // ← задаётся двумя клавишами ПК, СТР — двумя: дубликатов клавиш ПК быть не должно.
        assert_eq!(keys.len(), KB_KEYS.len());
    }
}
