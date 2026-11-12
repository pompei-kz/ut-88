//! Минимальная конфигурация (модуль 1) с МОНИТОРОМ `monitor-fixed.bin`.

mod common;

use common::*;
use ut88_core::{Config, Machine, PadKey, TapeFormat, TapeSignal};

fn boot() -> Machine {
    let mut m = Machine::new(Config::minimal());
    m.run_seconds(0.1);
    m
}

#[test]
fn monitor_shows_11() {
    let m = boot();
    assert_eq!(m.indicator_bytes()[0], 0x11);
    let d = m.indicator_digits();
    assert_eq!(&d[4..], &[1, 1]);
    // «1» на семисегментном индикаторе — сегменты b, c (06H).
    assert_eq!(&m.indicator_segments()[4..], &[0x06, 0x06]);
}

#[test]
fn rom_mirror_and_unmapped() {
    let m = boot();
    assert_eq!(m.peek(0x0000), 0x31);
    assert_eq!(m.peek(0x0400), 0x31, "A10 не декодируется");
    assert_eq!(m.peek(0x0800), 0xFF, "0800H–0FFFH пусто без калькулятора");
    assert_eq!(m.peek(0x1000), 0xFF);
    let m = Machine::new(Config::minimal().with_calculator());
    assert_eq!(
        m.save_bin(0x0800, 0x0FFF),
        ut88_core::roms::CALCULATOR.to_vec()
    );
}

#[test]
fn directive_4_ram_test() {
    let mut m = boot();
    pad(&mut m, PadKey::Hex(4));
    wait_getkey(&mut m, 5.0);
    assert_eq!(hl(&m), 0xC400);
}

fn checksum_directive(m: &mut Machine, range: &str) -> u16 {
    pad(m, PadKey::Hex(8));
    pad_hex(m, range);
    wait_getkey(m, 5.0);
    assert_eq!(m.indicator_bytes()[0], 0x11);
    hl(m)
}

/// Суммы из журнала (0181H/18CCH) — для дампа, как он напечатан (`monitor.bin`).
#[test]
fn directive_8_checksums_original_rom() {
    let mut m = Machine::new(Config::minimal().with_monitor_rom(ut88_core::roms::MONITOR));
    m.run_seconds(0.1);
    assert_eq!(checksum_directive(&mut m, "000001FF"), 0x0181);
    assert_eq!(checksum_directive(&mut m, "020003FF"), 0x18CC);
}

/// В исправленном образе 006FH–0070H: 3B 3B → 33 33, сумма первой половины меньше на 10H.
#[test]
fn directive_8_checksums_fixed_rom() {
    let mut m = boot();
    assert_eq!(checksum_directive(&mut m, "000001FF"), 0x0171);
    assert_eq!(checksum_directive(&mut m, "020003FF"), 0x18CC);
}

#[test]
fn fill_const_program() {
    let mut m = boot();
    let n = m
        .load_bin_file(repo("programs/fill-const.bin"), 0xC000)
        .unwrap();
    assert_eq!(n, 14);
    pad(&mut m, PadKey::Hex(6));
    pad_hex(&mut m, "C100C17FA5");
    wait_getkey(&mut m, 5.0);
    assert_eq!(m.indicator_bytes()[0], 0x11, "выход в МОНИТОР");
    assert!(m.save_bin(0xC100, 0xC17F).iter().all(|&b| b == 0xA5));
    assert_eq!(m.peek(0xC180), 0);
    assert_eq!(m.peek(0xC0FF), 0);
}

#[test]
fn clock_counts_seconds() {
    let mut m = boot();
    assert_eq!(m.peek(0xC3FD), 0);
    m.run_seconds(3.5);
    assert_eq!(m.peek(0xC3FD), 0x03);
    m.poke(0xC3FD, 0x59);
    m.run_seconds(1.0);
    assert_eq!(m.peek(0xC3FD), 0x00, "секунды в BCD по модулю 60");
    assert_eq!(m.peek(0xC3FE), 0x01, "минуты");
}

/// Ввести директиву «A» со смещением 0000 и подать сигнал на вход перед последней цифрой.
fn read_tape(m: &mut Machine, signal: TapeSignal) {
    pad(m, PadKey::Hex(0xA));
    pad_hex(m, "000");
    wait_getkey(m, 5.0);
    m.tape_play(signal);
    pad(m, PadKey::Hex(0));
    wait_getkey(m, 30.0);
}

/// Запись и чтение МОНИТОРОМ-«0» при выключенном запросе прерывания — так советует
/// doc/25: драйверы МОНИТОРА-«0» не запрещают прерывания, и секундный импульс посреди
/// блока сбивает фазу чтения (в эмуляторе это тоже воспроизводится).
#[test]
fn tape_directive_9_then_a() {
    let mut m = boot();
    m.set_clock_interrupts(false);
    m.load_bin_file(repo("programs/labyrinth.bin"), 0xC000)
        .unwrap();
    let original = m.save_bin(0xC000, 0xC0FF);

    pad(&mut m, PadKey::Hex(9));
    pad_hex(&mut m, "C000C0F");
    wait_getkey(&mut m, 5.0);
    m.tape_record_start();
    pad(&mut m, PadKey::Hex(0xF));
    wait_getkey(&mut m, 30.0);
    m.run_seconds(0.2);
    let signal = m.tape_record_stop().unwrap();
    assert_eq!(hl(&m), 0x9094, "контрольная сумма записи");

    let wav = temp("dir9.wav");
    signal.save_wav(&wav).unwrap();
    let signal = TapeSignal::load_wav(&wav).unwrap();
    assert_eq!(signal.rate, 44_100);

    m.load_bin(0xC000, &[0u8; 256]);
    read_tape(&mut m, signal);
    assert_eq!(hl(&m), 0x9094, "контрольная сумма чтения");
    assert_eq!(m.save_bin(0xC000, 0xC0FF), original);
}

#[test]
fn tape_generated_monitor0_block() {
    let mut src = Machine::new(Config::minimal());
    let data: Vec<u8> = (0..200u32).map(|i| (i * 7 + 3) as u8).collect();
    src.load_bin(0xC100, &data);
    let signal = src.tape_block_signal(TapeFormat::Monitor0, 0xC100, 0xC1C7);

    let mut m = boot();
    read_tape(&mut m, signal);
    assert_eq!(m.save_bin(0xC100, 0xC1C7), data);
    assert_eq!(hl(&m), ut88_core::tape::checksum(&data));
}

/// Кассета вставлена до набора директивы «A» (как `--tape` при запуске): лента стоит,
/// пока МОНИТОР не начнёт читать порт магнитофона, поэтому пилот-тон не теряется.
#[test]
fn tape_inserted_before_directive_a() {
    let mut src = Machine::new(Config::minimal());
    let data: Vec<u8> = (0..100u32).map(|i| (i * 11 + 7) as u8).collect();
    src.load_bin(0xC100, &data);
    let signal = src.tape_block_signal(TapeFormat::Monitor0, 0xC100, 0xC163);

    let mut m = boot();
    m.tape_insert(signal);
    assert!(m.tape_inserted() && !m.tape_playing() && m.tape_auto_start());
    // Пользователь «думает» 3 с — дольше, чем длится вся запись.
    m.run_seconds(3.0);
    assert!(!m.tape_playing());
    assert_eq!(m.tape_position(), Some(0.0));
    pad(&mut m, PadKey::Hex(0xA));
    pad_hex(&mut m, "0000");
    wait_getkey(&mut m, 30.0);
    assert_eq!(m.save_bin(0xC100, 0xC163), data);
    assert_eq!(hl(&m), ut88_core::tape::checksum(&data));
    // Лента кончилась: «Воспроизведение» перематывает её в начало.
    m.run_seconds(2.0);
    assert!(m.tape_finished() && !m.tape_playing());
    m.tape_start();
    assert!(m.tape_playing());
    assert!(m.tape_position().unwrap() < 0.01);
    // «Стоп» сохраняет позицию и снимает автопуск.
    m.run_seconds(0.5);
    m.tape_pause();
    let pos = m.tape_position().unwrap();
    m.run_seconds(0.5);
    assert_eq!(m.tape_position(), Some(pos));
    assert!(!m.tape_auto_start());
}

#[test]
fn snapshot_roundtrip_minimal() {
    let mut m = boot();
    pad(&mut m, PadKey::Hex(1));
    pad_hex(&mut m, "5");
    let path = temp("min.snap");
    m.save_snapshot(&path).unwrap();
    let mut m2 = Machine::new(Config::minimal());
    m2.load_snapshot(&path).unwrap();
    assert_eq!(m.snapshot(), m2.snapshot());
    m.run_seconds(1.3);
    m2.run_seconds(1.3);
    assert_eq!(m.snapshot_bytes(), m2.snapshot_bytes());
}
