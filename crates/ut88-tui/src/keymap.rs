//! Соответствие клавиш ПК кнопкам ЮТ-88 и горячим клавишам эмулятора.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ut88_core::{KbKey, PadKey};

/// Действие эмулятора по горячей клавише.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// Меню всех действий и справка.
    Menu,
    /// «УСТ» — сброс процессора.
    Reset,
    /// Холодный старт — новая машина (выключить и включить питание).
    ColdStart,
    /// Пауза / продолжить.
    Pause,
    /// Скорость выше.
    SpeedUp,
    /// Скорость ниже.
    SpeedDown,
    /// Сохранить снимок.
    SaveSnapshot,
    /// Загрузить снимок.
    LoadSnapshot,
    /// Меню магнитофона.
    Tape,
    /// Список программ из журналов.
    Programs,
    /// Выход.
    Exit,
    /// Отладчик.
    Debugger,
}

/// Таблица горячих клавиш: клавиша, действие, описание (для меню и справки).
pub const HOTKEYS: &[(&str, Hotkey, &str)] = &[
    ("F1", Hotkey::Menu, "меню и справка"),
    ("F2", Hotkey::Reset, "«УСТ» (сброс процессора)"),
    ("F3", Hotkey::ColdStart, "холодный старт (питание)"),
    ("F4", Hotkey::Pause, "пауза / продолжить"),
    ("F5", Hotkey::SpeedUp, "скорость выше"),
    ("Shift+F5", Hotkey::SpeedDown, "скорость ниже"),
    ("F6", Hotkey::SaveSnapshot, "сохранить снимок"),
    ("F7", Hotkey::LoadSnapshot, "загрузить снимок"),
    ("F8", Hotkey::Tape, "магнитофон"),
    ("F9", Hotkey::Programs, "программы из журналов"),
    ("F10", Hotkey::Exit, "выход"),
    ("F12", Hotkey::Debugger, "отладчик"),
];

/// Горячая клавиша по событию.
pub fn hotkey(ev: &KeyEvent) -> Option<Hotkey> {
    let shift = ev.modifiers.contains(KeyModifiers::SHIFT);
    Some(match ev.code {
        KeyCode::F(1) => Hotkey::Menu,
        KeyCode::F(2) => Hotkey::Reset,
        KeyCode::F(3) => Hotkey::ColdStart,
        KeyCode::F(4) => Hotkey::Pause,
        KeyCode::F(5) if shift => Hotkey::SpeedDown,
        KeyCode::F(5) => Hotkey::SpeedUp,
        // Некоторые терминалы передают Shift+F5 как F17.
        KeyCode::F(17) => Hotkey::SpeedDown,
        KeyCode::F(6) => Hotkey::SaveSnapshot,
        KeyCode::F(7) => Hotkey::LoadSnapshot,
        KeyCode::F(8) => Hotkey::Tape,
        KeyCode::F(9) => Hotkey::Programs,
        KeyCode::F(10) => Hotkey::Exit,
        KeyCode::F(12) => Hotkey::Debugger,
        _ => return None,
    })
}

/// Кнопка модуля 1 по клавише ПК: цифры 0–9, латинские A–F (любой регистр),
/// Backspace — «ШАГ НАЗАД». «УСТ» — горячая клавиша F2.
pub fn pad_key(ev: &KeyEvent) -> Option<PadKey> {
    if ev
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    match ev.code {
        KeyCode::Char(c) => c.to_digit(16).map(|d| PadKey::Hex(d as u8)),
        KeyCode::Backspace => Some(PadKey::StepBack),
        _ => None,
    }
}

/// Русские буквы КОИ-7 Н2 с кода 60H (порядок как в знакогенераторе ЮТ-88).
const RUS: [char; 32] = [
    'Ю', 'А', 'Б', 'Ц', 'Д', 'Е', 'Ф', 'Г', 'Х', 'И', 'Й', 'К', 'Л', 'М', 'Н', 'О', 'П', 'Я', 'Р',
    'С', 'Т', 'У', 'Ж', 'В', 'Ь', 'Ы', 'З', 'Ш', 'Э', 'Щ', 'Ч', 'Ъ',
];

/// Код КОИ-7 символа, набранного на клавиатуре ПК.
///
/// Латинские строчные становятся прописными (у ЮТ-88 латиница только прописная),
/// русские буквы любого регистра — коды 60H–7FH (набираются с РУС).
pub fn char_code(c: char) -> Option<u8> {
    match c {
        ' '..='_' => Some(c as u8),
        'a'..='z' => Some(c.to_ascii_uppercase() as u8),
        'Ё' | 'ё' => Some(0x65),
        _ => {
            let up = c.to_uppercase().next()?;
            RUS.iter().position(|&r| r == up).map(|i| 0x60 + i as u8)
        }
    }
}

/// Код КОИ-7 для клавиши ПК в полной конфигурации.
///
/// - печатные символы — по [`char_code`];
/// - Ctrl+буква — УС (код AND 1FH); Alt+латинская буква — РУС (код + 20H);
/// - Enter — ВК (0DH), стрелки — 08H/18H/19H/1AH, Backspace — ← (08H), Home — ↖ (0CH),
///   Delete и End — СТР (1FH), Tab — 09H, Esc — 1BH.
pub fn kb_code(ev: &KeyEvent) -> Option<u8> {
    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
    let alt = ev.modifiers.contains(KeyModifiers::ALT);
    match ev.code {
        KeyCode::Char(c) if ctrl => {
            let u = c.to_ascii_uppercase();
            ('@'..='_').contains(&u).then_some(u as u8 & 0x1F)
        }
        KeyCode::Char(c) if alt => {
            let u = c.to_ascii_uppercase();
            ('@'..='_').contains(&u).then(|| u as u8 + 0x20)
        }
        KeyCode::Char(c) => char_code(c),
        KeyCode::Enter => Some(0x0D),
        KeyCode::Left | KeyCode::Backspace => Some(0x08),
        KeyCode::Right => Some(0x18),
        KeyCode::Up => Some(0x19),
        KeyCode::Down => Some(0x1A),
        KeyCode::Home => Some(0x0C),
        KeyCode::Delete | KeyCode::End => Some(0x1F),
        KeyCode::Tab => Some(0x09),
        KeyCode::Esc => Some(0x1B),
        _ => None,
    }
}

/// Клавиша (и модификатор) полной клавиатуры для клавиши ПК.
pub fn kb_key(ev: &KeyEvent) -> Option<(KbKey, Option<KbKey>)> {
    kb_code(ev).and_then(KbKey::for_code)
}

/// Подпись кнопки модуля 1 на подсказке.
pub fn pad_label(k: PadKey) -> String {
    match k {
        PadKey::Hex(d) => format!("{d:X}"),
        PadKey::StepBack => "ШАГ НАЗАД".into(),
        PadKey::Reset => "УСТ".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    #[test]
    fn pad_mapping() {
        let n = KeyModifiers::NONE;
        assert_eq!(pad_key(&ev(KeyCode::Char('0'), n)), Some(PadKey::Hex(0)));
        assert_eq!(pad_key(&ev(KeyCode::Char('a'), n)), Some(PadKey::Hex(10)));
        assert_eq!(pad_key(&ev(KeyCode::Char('F'), n)), Some(PadKey::Hex(15)));
        assert_eq!(pad_key(&ev(KeyCode::Char('g'), n)), None);
        assert_eq!(pad_key(&ev(KeyCode::Backspace, n)), Some(PadKey::StepBack));
        assert_eq!(
            pad_key(&ev(KeyCode::Char('1'), KeyModifiers::CONTROL)),
            None
        );
    }

    #[test]
    fn char_codes() {
        assert_eq!(char_code('d'), Some(b'D'));
        assert_eq!(char_code('D'), Some(b'D'));
        assert_eq!(char_code('7'), Some(b'7'));
        assert_eq!(char_code('ю'), Some(0x60));
        assert_eq!(char_code('Т'), Some(0x74));
        assert_eq!(char_code('ч'), Some(0x7E));
        assert_eq!(char_code('`'), None);
        assert_eq!(char_code('ё'), Some(0x65));
        // Обратное соответствие со знакогенератором.
        for code in 0x60..0x7F {
            let c = ut88_core::screen::koi7_to_char(code);
            assert_eq!(char_code(c), Some(code), "{c}");
        }
    }

    #[test]
    fn special_keys() {
        let n = KeyModifiers::NONE;
        assert_eq!(kb_code(&ev(KeyCode::Enter, n)), Some(0x0D));
        assert_eq!(kb_code(&ev(KeyCode::Left, n)), Some(0x08));
        assert_eq!(kb_code(&ev(KeyCode::Right, n)), Some(0x18));
        assert_eq!(kb_code(&ev(KeyCode::Up, n)), Some(0x19));
        assert_eq!(kb_code(&ev(KeyCode::Down, n)), Some(0x1A));
        assert_eq!(kb_code(&ev(KeyCode::Delete, n)), Some(0x1F));
        assert_eq!(kb_code(&ev(KeyCode::Home, n)), Some(0x0C));
        let c = KeyModifiers::CONTROL;
        assert_eq!(kb_code(&ev(KeyCode::Char('c'), c)), Some(0x03));
        assert_eq!(
            kb_key(&ev(KeyCode::Char('c'), c)),
            Some((KbKey::Matrix(19), Some(KbKey::Us)))
        );
        let a = KeyModifiers::ALT;
        assert_eq!(kb_code(&ev(KeyCode::Char('a'), a)), Some(0x61));
        assert_eq!(
            kb_key(&ev(KeyCode::Char('a'), a)),
            Some((KbKey::Matrix(17), Some(KbKey::Rus)))
        );
        // Каждый печатный символ набирается.
        for c in ' '..='_' {
            assert!(kb_key(&ev(KeyCode::Char(c), n)).is_some(), "{c}");
        }
        for &c in &RUS[..31] {
            assert!(kb_key(&ev(KeyCode::Char(c), n)).is_some(), "{c}");
        }
    }

    #[test]
    fn hotkeys_do_not_overlap_input() {
        for (_, hk, _) in HOTKEYS {
            let _ = hk;
        }
        let f = |n| ev(KeyCode::F(n), KeyModifiers::NONE);
        assert_eq!(hotkey(&f(2)), Some(Hotkey::Reset));
        assert_eq!(
            hotkey(&ev(KeyCode::F(5), KeyModifiers::SHIFT)),
            Some(Hotkey::SpeedDown)
        );
        for n in 1..=12 {
            assert_eq!(kb_code(&f(n)), None);
            assert_eq!(pad_key(&f(n)), None);
        }
    }
}
