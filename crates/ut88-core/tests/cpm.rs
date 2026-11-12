//! Квазидиск (doc/32), CP/M-64, CP/M-35, ОС ЮТ-88 (распечатки 1990-02).

mod common;

use common::*;
use ut88_core::{CPU_HZ, Config, CpmDisk, Machine, roms};

/// Выполнять, пока на экране не появится `text` (не дольше `secs` с).
fn wait_text(m: &mut Machine, text: &str, secs: f64) {
    let limit = m.cycles() + (secs * CPU_HZ) as u64;
    while !screen_text(m).contains(text) {
        assert!(
            m.cycles() < limit,
            "на экране нет «{text}»:\n{}",
            screen_text(m)
        );
        m.run_seconds(0.05);
    }
}

/// Последняя непустая строка экрана.
fn last_line(m: &Machine) -> String {
    let t = screen_text(m);
    // Курсор ОС ЮТ-88 мигает (инверсией байта за курсором) — его не учитываем.
    t.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim_end_matches(['■', ' '])
        .to_string()
}

/// Набрать команду и дождаться следующего приглашения `prompt`.
fn command(m: &mut Machine, cmd: &str, prompt: &str) -> String {
    kb_type(m, cmd);
    kb_type(m, "\r");
    let limit = m.cycles() + (5.0 * CPU_HZ) as u64;
    while last_line(m) != prompt {
        assert!(m.cycles() < limit, "нет приглашения:\n{}", screen_text(m));
        m.run_seconds(0.05);
    }
    screen_text(m)
}

// ----------------------------------------------------------------- квазидиск

/// Машина конфигурации cpm с программой `code` в 2000H; МОНИТОР-F успел стартовать.
fn kd_machine(code: &[u8]) -> Machine {
    let mut m = Machine::new(Config::cpm());
    m.run_seconds(0.3);
    m.load_bin(0x2000, code);
    m.jump(0x2000);
    m
}

/// Выполнять, пока процессор не остановится (HLT).
fn run_to_halt(m: &mut Machine) {
    let limit = m.cycles() + 100_000;
    while !m.cpu.halted {
        assert!(m.cycles() < limit, "нет HLT, PC={:04X}", m.cpu.pc);
        m.step();
    }
}

fn kd(m: &Machine, block: usize, addr: u16) -> u8 {
    m.kvazidisk_image().unwrap()[block * 0x10000 + usize::from(addr)]
}

#[test]
fn kvazidisk_push_pop_blocks() {
    #[rustfmt::skip]
    let code = [
        0x31, 0x00, 0x10,       // LXI SP,1000H
        0x3E, 0xFE, 0xD3, 0x40, // MVI A,FEH; OUT 40H — блок 1
        0x01, 0x34, 0x12,       // LXI B,1234H
        0xC5,                   // PUSH B
        0x3E, 0xFD, 0xD3, 0x40, // блок 2
        0x31, 0x00, 0x10,       // LXI SP,1000H
        0x11, 0x78, 0x56,       // LXI D,5678H
        0xD5,                   // PUSH D
        0x3E, 0xFE, 0xD3, 0x40, // снова блок 1
        0x31, 0xFE, 0x0F,       // LXI SP,0FFEH
        0xE1,                   // POP H → 1234H
        0x3E, 0xFF, 0xD3, 0x40, // закрыть
        0x31, 0xFE, 0x0F,       // LXI SP,0FFEH
        0xC1,                   // POP B — из основного ОЗУ
        0x76,                   // HLT
    ];
    let mut m = kd_machine(&code);
    m.poke(0x0FFE, 0xAA);
    m.poke(0x0FFF, 0xBB);
    run_to_halt(&mut m);
    assert_eq!(m.cpu.hl(), 0x1234);
    assert_eq!(m.cpu.bc(), 0xBBAA, "основное ОЗУ не тронуто");
    assert_eq!((kd(&m, 0, 0x0FFE), kd(&m, 0, 0x0FFF)), (0x34, 0x12));
    assert_eq!((kd(&m, 1, 0x0FFE), kd(&m, 1, 0x0FFF)), (0x78, 0x56));
    assert_eq!(kd(&m, 2, 0x0FFE), 0);
    assert_eq!(m.bus.st.kvazidisk.as_ref().unwrap().port, 0xFF);
}

#[test]
fn kvazidisk_call_ret_and_normal_accesses() {
    #[rustfmt::skip]
    let code = [
        0x31, 0x00, 0x80,       // 2000 LXI SP,8000H
        0x3E, 0xFB, 0xD3, 0x40, // 2003 блок 3
        0xCD, 0x10, 0x20,       // 2007 CALL 2010H
        0x3E, 0xFF, 0xD3, 0x40, // 200A закрыть
        0x76,                   // 200E HLT
        0x00,
        0x3A, 0x00, 0x30,       // 2010 LDA 3000H — обычное чтение
        0x32, 0xFE, 0x7F,       // 2013 STA 7FFEH — обычная запись
        0xE3,                   // 2016 XTHL — со стеком квазидиска
        0xE3,                   // 2017 XTHL
        0xC9,                   // 2018 RET
    ];
    let mut m = kd_machine(&code);
    m.poke(0x3000, 0x5A);
    m.cpu.h = 0x11;
    m.cpu.l = 0x22;
    run_to_halt(&mut m);
    assert_eq!(m.cpu.pc, 0x200F, "RET вернулся из квазидиска");
    assert_eq!(m.cpu.a, 0xFF);
    assert_eq!(m.cpu.hl(), 0x1122);
    // Адрес возврата 200AH — в блоке 3, а STA попала в основное ОЗУ.
    assert_eq!((kd(&m, 2, 0x7FFE), kd(&m, 2, 0x7FFF)), (0x0A, 0x20));
    assert_eq!(m.peek(0x7FFE), 0x5A);
    assert_eq!(m.peek(0x7FFF), 0x00);
}

#[test]
fn kvazidisk_interrupt_pushes_into_disk() {
    #[rustfmt::skip]
    let code = [
        0x31, 0x00, 0x80,       // LXI SP,8000H
        0x3E, 0xFE, 0xD3, 0x40, // блок 1
        0xFB,                   // EI
        0x00,                   // 2008 NOP
        0xC3, 0x08, 0x20,       // 2009 JMP 2008H
    ];
    let mut m = kd_machine(&code);
    // Обработчик RST 7: закрыть квазидиск и остановиться.
    m.load_bin(0x0038, &[0x3E, 0xFF, 0xD3, 0x40, 0x76]);
    for _ in 0..5 {
        m.step();
    }
    m.clock.irq_pending = true;
    run_to_halt(&mut m);
    let ret = u16::from_le_bytes([kd(&m, 0, 0x7FFE), kd(&m, 0, 0x7FFF)]);
    assert!((0x2008..=0x2009).contains(&ret), "{ret:04X}");
    assert_eq!(m.peek(0x7FFE), 0);
}

#[test]
fn kvazidisk_sizes_and_closed_states() {
    let mut m = Machine::new(Config::cpm().with_kvazidisk(1));
    assert_eq!(m.kvazidisk_image().unwrap().len(), 0x10000);
    assert_eq!(m.cpm64_dsm(), Some(0x39));
    // LXI SP,0100H; MVI A,FDH; OUT 40H (блок 2 не установлен); POP B; HLT
    m.load_bin(
        0x2000,
        &[0x31, 0x00, 0x01, 0x3E, 0xFD, 0xD3, 0x40, 0xC1, 0x76],
    );
    m.poke(0x0100, 0x12);
    m.jump(0x2000);
    run_to_halt(&mut m);
    assert_eq!(m.cpu.bc(), 0xFFFF, "открыт не установленный блок");
    for (blocks, dsm) in [(2, 0x79), (3, 0xB9), (4, 0xF9)] {
        let m = Machine::new(Config::cpm().with_kvazidisk(blocks));
        assert_eq!(m.cpm64_dsm(), Some(dsm));
    }
    assert!(!Machine::new(Config::full()).has_kvazidisk());
}

#[test]
fn kvazidisk_in_snapshot_and_image_file() {
    let mut m = Machine::new(Config::cpm());
    m.bus.st.kvazidisk.as_mut().unwrap().mem[0x3_1234] = 0x77;
    let s = Machine::parse_snapshot(&m.snapshot_bytes()).unwrap();
    let m2 = Machine::from_snapshot(&s).unwrap();
    assert_eq!(m2.kvazidisk_image().unwrap()[0x3_1234], 0x77);

    let path = temp("kvazidisk.img");
    m.save_kvazidisk_file(&path).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 256 * 1024);
    let mut m3 = Machine::new(Config::cpm());
    m3.load_kvazidisk_file(&path).unwrap();
    assert_eq!(m3.kvazidisk_image(), m.kvazidisk_image());
    let mut small = Machine::new(Config::cpm().with_kvazidisk(1));
    assert!(small.load_kvazidisk_file(&path).is_err());
}

// ----------------------------------------------------------------- CP/M-64

/// «Горячий старт» CP/M после записи файлов эмулятором; ждать нового приглашения.
fn warm_boot(m: &mut Machine) {
    assert!(m.cpm_warm_boot());
    m.run_seconds(0.5);
    assert_eq!(last_line(m), "A>");
}

fn cpm64() -> Machine {
    let mut m = Machine::new(Config::cpm());
    m.run_seconds(0.3);
    m.boot_cpm64(true).unwrap();
    wait_text(&mut m, "A>", 3.0);
    m
}

#[test]
fn cpm64_boots_and_dir_is_empty() {
    let mut m = cpm64();
    let t = screen_text(&m);
    assert!(t.contains("CPM V - 2.2  DISK RAM - 256K."), "{t}");
    assert_eq!(m.peek(0x4A50), 0xF9, "DSM для 256 Кбайт");
    assert_eq!(m.cpm_running(), Some(CpmDisk::Kvazidisk));
    let t = command(&mut m, "DIR", "A>");
    assert!(t.contains("A>DIR\nNO FILE\nA>"), "{t}");
    let t = command(&mut m, "BB", "A>");
    assert!(t.contains("BB?"), "{t}");
    // Система на квазидиске, каталог пуст.
    assert!(m.kvazidisk_has_cpm());
    assert!(m.cpm_list(CpmDisk::Kvazidisk).unwrap().is_empty());
}

#[test]
fn cpm64_emulator_writes_file_dir_shows_it() {
    let mut m = cpm64();
    m.cpm_write_file(CpmDisk::Kvazidisk, "CH.COM", roms::CPM_CH_COM)
        .unwrap();
    m.cpm_write_file(
        CpmDisk::Kvazidisk,
        "HELLO.TXT",
        b"HELLO, CP/M!\r\nLINE 2\r\n",
    )
    .unwrap();
    warm_boot(&mut m);
    let t = command(&mut m, "DIR", "A>");
    assert!(t.contains("A: CH       COM : HELLO    TXT"), "{t}");
    let t = command(&mut m, "TYPE HELLO.TXT", "A>");
    assert!(t.contains("HELLO, CP/M!\nLINE 2\n"), "{t}");
    // CH.COM без имени файла.
    let t = command(&mut m, "CH", "A>");
    assert!(t.contains("CHANGER VERS 1.1"), "{t}");
    assert!(t.contains("NO SOURCE FILE PRESENT."), "{t}");
}

#[test]
fn cpm64_save_reads_back_through_emulator() {
    let mut m = cpm64();
    // Файл 50 Кбайт эмулятором — затем SAVE заставит CP/M писать за пределы первых 64 К.
    let big: Vec<u8> = (0..50 * 1024).map(|i| (i * 7 + i / 300) as u8).collect();
    m.cpm_write_file(CpmDisk::Kvazidisk, "BIG.DAT", &big)
        .unwrap();
    warm_boot(&mut m);
    let tpa: Vec<u8> = (0..40 * 256).map(|i| (i * 13 + 1) as u8).collect();
    m.load_bin(0x0100, &tpa);
    command(&mut m, "SAVE 40 X.COM", "A>");
    let t = command(&mut m, "DIR", "A>");
    assert!(t.contains("BIG      DAT : X        COM"), "{t}");
    assert_eq!(m.cpm_read_file(CpmDisk::Kvazidisk, "X.COM").unwrap(), tpa);
    assert_eq!(m.cpm_read_file(CpmDisk::Kvazidisk, "BIG.DAT").unwrap(), big);
    // Часть X.COM лежит во втором блоке квазидиска (дорожки 64…).
    let img = m.kvazidisk_image().unwrap();
    assert!(
        img[0x10000..0x20000]
            .chunks(128)
            .any(|c| tpa.chunks(128).any(|t| t == c))
    );
    let files = m.cpm_list(CpmDisk::Kvazidisk).unwrap();
    assert_eq!(files[1].name, "X.COM");
    assert_eq!(files[1].records, 80);
}

#[test]
fn cpm64_disk_survives_restart_via_image() {
    let mut m = cpm64();
    command(&mut m, "SAVE 2 KEEP.COM", "A>");
    let path = temp("cpm64-disk.img");
    m.save_kvazidisk_file(&path).unwrap();

    let mut m2 = Machine::new(Config::cpm());
    m2.run_seconds(0.3);
    m2.load_kvazidisk_file(&path).unwrap();
    assert!(m2.kvazidisk_has_cpm());
    m2.boot_cpm64(false).unwrap();
    wait_text(&mut m2, "A>", 3.0);
    let t = command(&mut m2, "DIR", "A>");
    assert!(t.contains("A: KEEP     COM"), "{t}");
    // Горячий старт читает систему с дорожек 0–5 квазидиска.
    kb_code(&mut m2, 0x03);
    m2.run_seconds(0.5);
    let t = command(&mut m2, "DIR", "A>");
    assert!(t.contains("A: KEEP     COM"), "{t}");
}

/// CH.COM: файл → магнитофон (с проверкой записи) → ERA → обратно с ленты.
#[test]
fn cpm64_ch_com_tape_round_trip() {
    let mut m = cpm64();
    m.cpm_write_file(CpmDisk::Kvazidisk, "CH.COM", roms::CPM_CH_COM)
        .unwrap();
    m.cpm_write_file(CpmDisk::Kvazidisk, "T.TXT", b"TAPE TEST\r\n")
        .unwrap();
    warm_boot(&mut m);
    kb_type(&mut m, "CH T.TXT\r");
    wait_text(&mut m, "READY TR FOR OUTPUT, PRESS CR.", 2.0);
    m.tape_record_start();
    kb_type(&mut m, "\r");
    wait_text(&mut m, "READY TR FOR VERIFY, PRESS CR.", 30.0);
    let signal = m.tape_record_stop().unwrap();
    m.tape_insert(signal.clone());
    kb_type(&mut m, "\r");
    let limit = m.cycles() + (30.0 * CPU_HZ) as u64;
    while last_line(&m) != "A>" {
        assert!(m.cycles() < limit, "{}", screen_text(&m));
        m.run_seconds(0.1);
    }
    assert!(!screen_text(&m).contains("ERROR"));
    command(&mut m, "ERA T.TXT", "A>");
    assert!(m.cpm_read_file(CpmDisk::Kvazidisk, "T.TXT").is_err());
    kb_type(&mut m, "CH T.TXT\r");
    wait_text(&mut m, "READY TR FOR INPUT, PRESS CR.", 2.0);
    m.tape_insert(signal);
    kb_type(&mut m, "\r");
    let limit = m.cycles() + (30.0 * CPU_HZ) as u64;
    while last_line(&m) != "A>" {
        assert!(m.cycles() < limit, "{}", screen_text(&m));
        m.run_seconds(0.1);
    }
    let data = m.cpm_read_file(CpmDisk::Kvazidisk, "T.TXT").unwrap();
    assert!(data.starts_with(b"TAPE TEST\r\n\x1A"));
}

// ----------------------------------------------------------------- CP/M-35

#[test]
fn cpm35_boots_dir_save() {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.3);
    m.boot_cpm35(true).unwrap();
    wait_text(&mut m, "35K CP/M VERS 2.2", 3.0);
    wait_text(&mut m, "A>", 1.0);
    assert!(!m.config().clock_interrupts);
    assert_eq!(m.cpm_running(), Some(CpmDisk::Ram35));
    let t = command(&mut m, "DIR", "A>");
    assert!(t.contains("NO FILE"), "{t}");
    m.load_bin(0x0100, &[0x55; 256]);
    command(&mut m, "SAVE 1 X.COM", "A>");
    m.cpm_write_file(CpmDisk::Ram35, "CH.COM", roms::CPM_CH_COM)
        .unwrap();
    // Перезапуск эмулятором — через BOOT (4A00H), каталог в ОЗУ сохраняется.
    warm_boot(&mut m);
    let t = command(&mut m, "DIR", "A>");
    assert!(t.contains("A: X        COM : CH       COM"), "{t}");
    assert_eq!(
        m.cpm_read_file(CpmDisk::Ram35, "X.COM").unwrap(),
        vec![0x55; 256]
    );
    // WBOOT CP/M-35 (0000H → 4A03H → JMP F86CH) выходит в МОНИТОР-F — и после
    // завершения программы, и по УС+C; обратно — G4A00.
    let t = command(&mut m, "CH", " =>");
    assert!(t.contains("CHANGER VERS 1.1"), "{t}");
    command(&mut m, "G4A00", "A>");
    let t = command(&mut m, "DIR", "A>");
    assert!(t.contains("A: X        COM : CH       COM"), "{t}");
}

// ----------------------------------------------------------------- ОС ЮТ-88

#[test]
fn ut88_os_starts() {
    let mut m = Machine::new(Config::cpm());
    m.run_seconds(0.3);
    m.boot_ut88_os().unwrap();
    let t = screen_text(&m);
    assert!(t.contains("СТАРТЕР \"ОС ЮТ*88\""), "{t}");
    assert!(
        t.contains("ВЫКЛЮЧИТЕ ПЗУ МОНИТОРА И НАЖМИТЕ ЛЮБУЮ КЛАВИШУ"),
        "{t}"
    );
    assert!(!m.config().monitor_f_rom);
    assert!(!m.config().clock_interrupts);
    kb_type(&mut m, " ");
    m.run_seconds(0.5);
    let t = screen_text(&m);
    assert!(t.contains("**** ОС   *   ЮT-88 ****"), "{t}");
    assert_eq!(last_line(&m), "ЮT*88>");
    // Монитор ОС — в ОЗУ под бывшим ПЗУ (с правкой 3320H → FA70H).
    let mut mon = roms::UT88_OS[0xB0..0x8B0].to_vec();
    assert_eq!(mon[0x270], 0x4D);
    mon[0x270] = 0x68;
    assert_eq!(m.save_bin(0xF800, 0xFFFF), mon);
    assert_eq!(m.save_bin(0xC000, 0xDFFF), roms::UT88_OS[0x8B0..0x28B0]);
    assert!((0xF800..=0xFFFF).contains(&m.cpu.pc) || m.cpu.pc < 0xE000);

    // Три дампа по 16 строк — экран прокручивается (с напечатанным байтом 3320H ОС
    // рушилась на первой прокрутке).
    for _ in 0..3 {
        command(&mut m, "D0000", "ЮT*88>");
    }
    let t = screen_text(&m);
    assert!(!t.contains("**** ОС"), "экран прокручен: {t}");
    assert!(t.contains("0000 C3 43 00 F8"), "{t}");
    assert_eq!(m.peek(0), 0xC3);
}
