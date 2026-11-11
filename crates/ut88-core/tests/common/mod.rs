//! Общие помощники интеграционных тестов.
#![allow(dead_code)]

use std::path::PathBuf;

use ut88_core::{CPU_HZ, KbKey, Machine, PadKey};

/// Путь к файлу относительно корня репозитория.
pub fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// Уникальный временный файл.
pub fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ut88-core-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

// ----- модуль 1 -----

/// Цикл ожидания нажатия в подпрограмме GETKEY МОНИТОРА (005FH–0065H).
fn in_getkey(m: &Machine) -> bool {
    (0x005F..=0x0065).contains(&m.cpu.pc)
}

/// Выполнять, пока МОНИТОР не встанет в ожидание клавиши (не дольше `secs` с).
pub fn wait_getkey(m: &mut Machine, secs: f64) {
    let limit = m.cycles() + (secs * CPU_HZ) as u64;
    while !in_getkey(m) {
        assert!(
            m.cycles() < limit,
            "МОНИТОР не ждёт клавишу, PC={:04X}",
            m.cpu.pc
        );
        m.step();
    }
}

/// Нажать кнопку модуля 1: дождаться GETKEY, нажать, дождаться приёма кода, отпустить.
pub fn pad(m: &mut Machine, key: PadKey) {
    wait_getkey(m, 5.0);
    m.pad_down(key);
    let limit = m.cycles() + CPU_HZ as u64;
    while m.cpu.pc != 0x0066 {
        assert!(m.cycles() < limit, "клавиша не принята");
        m.step();
    }
    m.pad_up(key);
}

/// Набрать шестнадцатеричные цифры на клавиатуре модуля 1.
pub fn pad_hex(m: &mut Machine, digits: &str) {
    for c in digits.chars() {
        pad(m, PadKey::Hex(c.to_digit(16).unwrap() as u8));
    }
}

/// HL на индикаторах.
pub fn hl(m: &Machine) -> u16 {
    let [_, l, h] = m.indicator_bytes();
    u16::from_be_bytes([h, l])
}

// ----- полная клавиатура -----

/// Нажать и отпустить клавишу с кодом `code` (с нужным модификатором).
pub fn kb_code(m: &mut Machine, code: u8) {
    let (key, modifier) = KbKey::for_code(code).unwrap_or_else(|| panic!("код {code:02X}"));
    if let Some(md) = modifier {
        m.kb_down(md);
    }
    m.kb_down(key);
    m.run_seconds(0.06);
    m.kb_up(key);
    if let Some(md) = modifier {
        m.kb_up(md);
    }
    m.run_seconds(0.3);
}

/// Набрать строку (символы КОИ-7; `\r` — ВК).
pub fn kb_type(m: &mut Machine, text: &str) {
    for b in text.bytes() {
        kb_code(m, if b == b'\n' { b'\r' } else { b });
    }
}

/// Текст экрана.
pub fn screen_text(m: &Machine) -> String {
    m.screen().expect("нет дисплейного модуля").text()
}
