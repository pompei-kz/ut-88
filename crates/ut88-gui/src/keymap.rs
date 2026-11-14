//! Соответствие клавиш ПК клавишам ЮТ-88.
//!
//! Клавиши берутся по физическому положению (коды [`KeyCode`] winit — как на американской
//! раскладке), поэтому переключатель раскладки ПК не мешает. Клавиша ЮТ-88 держится нажатой, пока нажата
//! клавиша ПК («Тетрис» опрашивает клавиатуру редко).

use ut88_core::{KbKey, PadKey};
use winit::keyboard::KeyCode;

/// Клавиатура модуля 1 (минимальная конфигурация): клавиша ПК → кнопка.
pub const PAD_KEYS: &[(KeyCode, PadKey)] = &[
    (KeyCode::Digit0, PadKey::Hex(0)),
    (KeyCode::Digit1, PadKey::Hex(1)),
    (KeyCode::Digit2, PadKey::Hex(2)),
    (KeyCode::Digit3, PadKey::Hex(3)),
    (KeyCode::Digit4, PadKey::Hex(4)),
    (KeyCode::Digit5, PadKey::Hex(5)),
    (KeyCode::Digit6, PadKey::Hex(6)),
    (KeyCode::Digit7, PadKey::Hex(7)),
    (KeyCode::Digit8, PadKey::Hex(8)),
    (KeyCode::Digit9, PadKey::Hex(9)),
    (KeyCode::KeyA, PadKey::Hex(0xA)),
    (KeyCode::KeyB, PadKey::Hex(0xB)),
    (KeyCode::KeyC, PadKey::Hex(0xC)),
    (KeyCode::KeyD, PadKey::Hex(0xD)),
    (KeyCode::KeyE, PadKey::Hex(0xE)),
    (KeyCode::KeyF, PadKey::Hex(0xF)),
    (KeyCode::Backspace, PadKey::StepBack),
];

/// Полная клавиатура дисплейного модуля: клавиша ПК → код КОИ-7 основного символа
/// клавиши ЮТ-88 (клавиша матрицы находится через [`KbKey::for_code`]).
pub const KB_KEYS: &[(KeyCode, u8)] = &[
    (KeyCode::Digit0, b'0'),
    (KeyCode::Digit1, b'1'),
    (KeyCode::Digit2, b'2'),
    (KeyCode::Digit3, b'3'),
    (KeyCode::Digit4, b'4'),
    (KeyCode::Digit5, b'5'),
    (KeyCode::Digit6, b'6'),
    (KeyCode::Digit7, b'7'),
    (KeyCode::Digit8, b'8'),
    (KeyCode::Digit9, b'9'),
    (KeyCode::KeyA, b'A'),
    (KeyCode::KeyB, b'B'),
    (KeyCode::KeyC, b'C'),
    (KeyCode::KeyD, b'D'),
    (KeyCode::KeyE, b'E'),
    (KeyCode::KeyF, b'F'),
    (KeyCode::KeyG, b'G'),
    (KeyCode::KeyH, b'H'),
    (KeyCode::KeyI, b'I'),
    (KeyCode::KeyJ, b'J'),
    (KeyCode::KeyK, b'K'),
    (KeyCode::KeyL, b'L'),
    (KeyCode::KeyM, b'M'),
    (KeyCode::KeyN, b'N'),
    (KeyCode::KeyO, b'O'),
    (KeyCode::KeyP, b'P'),
    (KeyCode::KeyQ, b'Q'),
    (KeyCode::KeyR, b'R'),
    (KeyCode::KeyS, b'S'),
    (KeyCode::KeyT, b'T'),
    (KeyCode::KeyU, b'U'),
    (KeyCode::KeyV, b'V'),
    (KeyCode::KeyW, b'W'),
    (KeyCode::KeyX, b'X'),
    (KeyCode::KeyY, b'Y'),
    (KeyCode::KeyZ, b'Z'),
    (KeyCode::Semicolon, b';'),
    (KeyCode::Quote, b':'),
    (KeyCode::Minus, b'-'),
    (KeyCode::Equal, b'_'),
    (KeyCode::Comma, b','),
    (KeyCode::Period, b'.'),
    (KeyCode::Slash, b'/'),
    (KeyCode::Backslash, b'\\'),
    (KeyCode::BracketLeft, b'['),
    (KeyCode::BracketRight, b']'),
    (KeyCode::Backquote, b'@'),
    (KeyCode::Insert, b'^'),
    (KeyCode::Space, b' '),
    (KeyCode::Enter, 0x0D),
    (KeyCode::Backspace, 0x08),
    (KeyCode::ArrowLeft, 0x08),
    (KeyCode::ArrowRight, 0x18),
    (KeyCode::ArrowUp, 0x19),
    (KeyCode::ArrowDown, 0x1A),
    (KeyCode::Home, 0x0C),
    (KeyCode::Delete, 0x1F),
    (KeyCode::End, 0x1F),
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
    ("F11", "окно «Справка» (клавиши)"),
    ("F12", "окно отладчика"),
];

/// Панель кнопок горячих клавиш внизу окна: подпись, клавиша, Shift, подсказка. Каждая
/// строка [`HOTKEYS`] есть здесь кнопкой (проверяется тестом).
pub const FKEY_BAR: &[(&str, KeyCode, bool, &str)] = &[
    (
        "F2 УСТ",
        KeyCode::F2,
        false,
        "кнопка «УСТ» (сброс процессора)",
    ),
    ("F3 Холод", KeyCode::F3, false, "холодный старт"),
    ("F4 Пауза", KeyCode::F4, false, "пауза / продолжить"),
    ("F5 Скор+", KeyCode::F5, false, "скорость выше"),
    (
        "Shift+F5 Скор-",
        KeyCode::F5,
        true,
        "скорость ниже (Shift+F5)",
    ),
    (
        "F6 Снимок",
        KeyCode::F6,
        false,
        "быстрый снимок (ut88-quick.snap)",
    ),
    ("F7 Загр", KeyCode::F7, false, "загрузить быстрый снимок"),
    ("F8 Лента", KeyCode::F8, false, "окно «Магнитофон и файлы»"),
    (
        "F9 Прогр",
        KeyCode::F9,
        false,
        "окно «Программы из журналов»",
    ),
    ("F10 Выход", KeyCode::F10, false, "выход"),
    (
        "F11 Справка",
        KeyCode::F11,
        false,
        "окно «Справка» (клавиши)",
    ),
    ("F12 Отладчик", KeyCode::F12, false, "окно отладчика"),
];

/// Клавиша полной клавиатуры для клавиши ПК.
pub fn kb_key(key: KeyCode) -> Option<KbKey> {
    KB_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|&(_, c)| KbKey::for_code(c))
        .map(|(k, _)| k)
}

/// Кнопка модуля 1 для клавиши ПК.
pub fn pad_key(key: KeyCode) -> Option<PadKey> {
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

/// Название клавиши ПК (как на клавиатуре: буквы и цифры, Backspace, Enter, Left и т. п.).
pub fn pc_label(key: KeyCode) -> String {
    match key {
        KeyCode::Semicolon => ";".into(),
        KeyCode::Quote => "'".into(),
        KeyCode::Minus => "-".into(),
        KeyCode::Equal => "=".into(),
        KeyCode::Comma => ",".into(),
        KeyCode::Period => ".".into(),
        KeyCode::Slash => "/".into(),
        KeyCode::Backslash => "\\".into(),
        KeyCode::BracketLeft => "[".into(),
        KeyCode::BracketRight => "]".into(),
        KeyCode::Backquote => "`".into(),
        k => key_name(k).to_string(),
    }
}

/// Короткое название клавиши: `A`, `0`, `F5`, `Backspace`, `Left`… (для подписей и
/// сверки панели F-клавиш с [`HOTKEYS`]).
pub fn key_name(key: KeyCode) -> &'static str {
    use KeyCode as K;
    const LETTERS: [(KeyCode, &str); 26] = [
        (K::KeyA, "A"),
        (K::KeyB, "B"),
        (K::KeyC, "C"),
        (K::KeyD, "D"),
        (K::KeyE, "E"),
        (K::KeyF, "F"),
        (K::KeyG, "G"),
        (K::KeyH, "H"),
        (K::KeyI, "I"),
        (K::KeyJ, "J"),
        (K::KeyK, "K"),
        (K::KeyL, "L"),
        (K::KeyM, "M"),
        (K::KeyN, "N"),
        (K::KeyO, "O"),
        (K::KeyP, "P"),
        (K::KeyQ, "Q"),
        (K::KeyR, "R"),
        (K::KeyS, "S"),
        (K::KeyT, "T"),
        (K::KeyU, "U"),
        (K::KeyV, "V"),
        (K::KeyW, "W"),
        (K::KeyX, "X"),
        (K::KeyY, "Y"),
        (K::KeyZ, "Z"),
    ];
    const OTHER: [(KeyCode, &str); 36] = [
        (K::Digit0, "0"),
        (K::Digit1, "1"),
        (K::Digit2, "2"),
        (K::Digit3, "3"),
        (K::Digit4, "4"),
        (K::Digit5, "5"),
        (K::Digit6, "6"),
        (K::Digit7, "7"),
        (K::Digit8, "8"),
        (K::Digit9, "9"),
        (K::Backspace, "Backspace"),
        (K::Enter, "Enter"),
        (K::Space, "Space"),
        (K::Insert, "Insert"),
        (K::Delete, "Delete"),
        (K::Home, "Home"),
        (K::End, "End"),
        (K::ArrowLeft, "Left"),
        (K::ArrowRight, "Right"),
        (K::ArrowUp, "Up"),
        (K::ArrowDown, "Down"),
        (K::Escape, "Escape"),
        (K::F1, "F1"),
        (K::F2, "F2"),
        (K::F3, "F3"),
        (K::F4, "F4"),
        (K::F5, "F5"),
        (K::F6, "F6"),
        (K::F7, "F7"),
        (K::F8, "F8"),
        (K::F9, "F9"),
        (K::F10, "F10"),
        (K::F11, "F11"),
        (K::F12, "F12"),
        (K::F17, "F17"),
        (K::Tab, "Tab"),
    ];
    LETTERS
        .iter()
        .chain(OTHER.iter())
        .find(|(k, _)| *k == key)
        .map_or("?", |&(_, n)| n)
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
        assert_eq!(pad_key(KeyCode::Digit0), Some(PadKey::Hex(0)));
        assert_eq!(pad_key(KeyCode::KeyF), Some(PadKey::Hex(15)));
        assert_eq!(pad_key(KeyCode::Backspace), Some(PadKey::StepBack));
        assert_eq!(pad_key(KeyCode::KeyG), None);
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
        let keys: std::collections::HashSet<KeyCode> = KB_KEYS.iter().map(|k| k.0).collect();
        // ← задаётся двумя клавишами ПК, СТР — двумя: дубликатов клавиш ПК быть не должно.
        assert_eq!(keys.len(), KB_KEYS.len());
    }

    #[test]
    fn pc_labels_of_mapped_keys() {
        for k in KB_KEYS
            .iter()
            .map(|e| e.0)
            .chain(PAD_KEYS.iter().map(|e| e.0))
        {
            assert_ne!(pc_label(k), "?", "{k:?}");
        }
        assert_eq!(pc_label(KeyCode::Digit7), "7");
        assert_eq!(pc_label(KeyCode::KeyQ), "Q");
        assert_eq!(pc_label(KeyCode::ArrowLeft), "Left");
        assert_eq!(pc_label(KeyCode::Equal), "=");
    }

    #[test]
    fn fkey_bar_covers_hotkeys() {
        assert_eq!(FKEY_BAR.len(), HOTKEYS.len());
        for (&(label, key, shift, _), &(hk, _)) in FKEY_BAR.iter().zip(HOTKEYS) {
            let name = if shift {
                format!("Shift+{}", key_name(key))
            } else {
                key_name(key).to_string()
            };
            assert_eq!(name, hk, "{label}");
        }
    }
}
