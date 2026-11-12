//! Полная конфигурация: модуль 1 + дисплейный модуль (МОНИТОР-«F») + ОЗУ 64 Кбайт.

mod common;

use common::*;
use ut88_core::screen::{self, COLS};
use ut88_core::{Config, KbKey, Machine, PadKey, TapeFormat, tape};

fn boot() -> Machine {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    m
}

#[test]
fn monitor_f_prompt() {
    let m = boot();
    let s = m.screen().unwrap();
    assert_eq!(s.line(1), "*ЮТ/88*");
    assert_eq!(s.line(2), " =>");
    // Курсор после «=>» — строка 2, столбец 3; бит 7 стоит в следующей ячейке, а
    // инверсно показывается само знакоместо курсора.
    assert!(s.inverse[2 * COLS + 3]);
    assert_eq!(s.inverse.iter().filter(|&&b| b).count(), 1);
    assert_eq!(m.peek(0xE884) & 0x80, 0x80);
}

#[test]
fn render_rgba() {
    let m = boot();
    let mut buf = vec![0u8; screen::WIDTH * screen::HEIGHT * 4];
    let fg = [255, 255, 255, 255];
    let bg = [0, 0, 0, 255];
    assert!(m.render_screen(&mut buf, fg, bg));
    let px = |x: usize, y: usize| {
        let o = (y * screen::WIDTH + x) * 4;
        [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]
    };
    // Курсор (пробел, инверсия) — сплошной прямоугольник 6 × 10.
    for y in 20..30 {
        for x in 18..24 {
            assert_eq!(px(x, y), fg, "курсор {x},{y}");
        }
    }
    // Пустая строка 0 — тёмная.
    assert!((0..screen::WIDTH).all(|x| px(x, 0) == bg));
    // «*» в строке 1 что-то зажигает.
    let lit = (10..20)
        .flat_map(|y| (0..6).map(move |x| (x, y)))
        .filter(|&(x, y)| px(x, y) == fg)
        .count();
    assert!(lit > 0);
}

#[test]
fn directive_d_from_keyboard() {
    let mut m = boot();
    kb_type(&mut m, "DF800,F80F\r");
    m.run_seconds(0.5);
    let s = m.screen().unwrap();
    assert_eq!(s.line(2), " =>DF800,F80F");
    assert_eq!(
        s.line(3).trim(),
        "F800 C3 36 F8 C3 57 FD C3 71 FB C3 43 FC C3 EE FB C3"
    );
    assert_eq!(s.line(4), " =>");
}

#[test]
fn modifiers_rus() {
    let mut m = boot();
    // РУС + буквы: «ЮТ» = 60H 74H; МОНИТОР выводит эхо, директивы такой нет.
    kb_type(&mut m, "\x60\x74");
    let s = m.screen().unwrap();
    assert_eq!(s.line(2), " =>ЮТ");
}

#[test]
fn tetris_draws_screen() {
    let mut m = boot();
    m.load_bin_file(repo("programs/tetris.bin"), 0x3000)
        .unwrap();
    kb_type(&mut m, "G3000\r");
    m.run_seconds(1.5);
    let text = screen_text(&m);
    assert!(text.contains("ВАШ УРОВЕНЬ(0-7) ?"), "{text}");
    assert!(text.contains("########################"), "{text}");
    // Ввести уровень, нажать любую клавишу — фигура падает.
    kb_type(&mut m, "3");
    m.run_seconds(0.5);
    let text = screen_text(&m);
    assert!(text.contains("НАЖМИТЕ ЛЮБУЮ КЛАВИШУ"), "{text}");
    // «Тетрис» опрашивает клавиатуру (F812H) редко — клавишу надо подержать.
    m.kb_down(KbKey::Matrix(48));
    m.run_seconds(0.5);
    m.kb_up(KbKey::Matrix(48));
    m.run_seconds(3.0);
    let text = screen_text(&m);
    assert!(text.contains("ЧИСЛО ПОЛНЫХ СТРОК   00"), "{text}");
    // Фигуры: программа выводит код 5BH дважды («[[»), см. подпрограмму 33F4H.
    assert!(text.contains("[["), "{text}");
}

#[test]
fn tape_monitor_f_block_directive_i() {
    let mut m = boot();
    let data: Vec<u8> = (0..256u32).map(|i| (i * 13 + 5) as u8).collect();
    let mut src = Machine::new(Config::full());
    src.load_bin(0x4000, &data);
    let signal = src.tape_block_signal(TapeFormat::MonitorF, 0x4000, 0x40FF);

    kb_type(&mut m, "I");
    m.tape_play(signal);
    kb_type(&mut m, "\r");
    m.run_seconds(4.0);
    assert_eq!(m.save_bin(0x4000, 0x40FF), data);
    let sum = format!("{:04X}", tape::checksum(&data));
    let text = screen_text(&m);
    assert!(text.contains("4000"), "{text}");
    assert!(text.contains("40FF"), "{text}");
    assert!(text.contains(&sum), "{text}");
    assert!(!text.contains('?'), "{text}");
}

/// Кассета вставлена до набора директивы «I»: лента трогается при первом чтении порта.
#[test]
fn tape_inserted_before_directive_i() {
    let mut src = Machine::new(Config::full());
    let data: Vec<u8> = (0..128u32).map(|i| (i * 5 + 9) as u8).collect();
    src.load_bin(0x4000, &data);
    let signal = src.tape_block_signal(TapeFormat::MonitorF, 0x4000, 0x407F);

    let mut m = boot();
    m.tape_insert(signal);
    m.run_seconds(3.0);
    assert!(!m.tape_playing());
    kb_type(&mut m, "I\r");
    m.run_seconds(4.0);
    assert!(m.tape_position().unwrap() > 1.0, "лента тронулась");
    assert_eq!(m.save_bin(0x4000, 0x407F), data);
    let text = screen_text(&m);
    let sum = format!("{:04X}", tape::checksum(&data));
    assert!(text.contains(&sum), "{text}");
    assert!(!text.contains('?'), "{text}");
}

#[test]
fn directive_o_to_wav_and_back() {
    let mut m = boot();
    let data: Vec<u8> = (0..64u32).map(|i| (i * 31 + 1) as u8).collect();
    m.load_bin(0x5000, &data);
    kb_type(&mut m, "O5000,503F");
    m.tape_record_start();
    kb_type(&mut m, "\r");
    m.run_seconds(4.0);
    let signal = m.tape_record_stop().unwrap();
    let wav = temp("dir-o.wav");
    signal.save_wav(&wav).unwrap();

    m.load_bin(0x5000, &[0u8; 64]);
    kb_type(&mut m, "I");
    m.tape_insert_wav(&wav).unwrap();
    kb_type(&mut m, "\r");
    m.run_seconds(4.0);
    assert_eq!(m.save_bin(0x5000, 0x503F), data);
    let text = screen_text(&m);
    assert!(!text.contains('?'), "{text}");
}

#[test]
fn snapshot_roundtrip_full() {
    let mut m = boot();
    m.load_bin_file(repo("programs/tetris.bin"), 0x3000)
        .unwrap();
    kb_type(&mut m, "G3000\r");
    m.run_seconds(0.7);
    let path = temp("full.snap");
    m.save_snapshot(&path).unwrap();

    let data = std::fs::read(&path).unwrap();
    let snap = Machine::parse_snapshot(&data).unwrap();
    let mut m2 = Machine::from_snapshot(&snap).unwrap();
    assert_eq!(m.snapshot(), m2.snapshot());
    assert_eq!(m.cpu, m2.cpu);

    for mm in [&mut m, &mut m2] {
        kb_type(mm, "5");
        mm.run_seconds(0.5);
    }
    assert_eq!(m.snapshot_bytes(), m2.snapshot_bytes());
    assert_eq!(screen_text(&m), screen_text(&m2));
}

#[test]
fn full_with_monitor0_starts_monitor0() {
    let mut m = Machine::new(Config::full_with_monitor0());
    m.run_seconds(0.1);
    assert_eq!(m.indicator_bytes()[0], 0x11);
    // Директива 7 F800 — запуск МОНИТОРА-«F».
    pad(&mut m, PadKey::Hex(7));
    pad_hex(&mut m, "F800");
    m.run_seconds(0.5);
    assert_eq!(m.screen().unwrap().line(1), "*ЮТ/88*");
    // ОЗУ 1000H–DFFFH, сегмент 0 — ПЗУ.
    m.poke(0x1000, 0x5A);
    assert_eq!(m.peek(0x1000), 0x5A);
    m.poke(0x0100, 0x5A);
    assert_ne!(m.peek(0x0100), 0x5A);
}

#[test]
fn memory_map_full() {
    let mut m = Machine::new(Config::full());
    // Экранное ОЗУ: E000H и E800H — одна ячейка (A11 не декодируется).
    m.poke(0xE005, 0x41);
    assert_eq!(m.peek(0xE805), 0x41);
    // ПЗУ МОНИТОРА-«F» не пишется, F000H–F3FFH пусто.
    m.poke(0xF800, 0);
    assert_eq!(m.peek(0xF800), 0xC3);
    assert_eq!(m.peek(0xF000), 0xFF);
    // Индикаторы пишутся и при ОЗУ 64 Кбайт; ОЗУ под ними читается.
    m.poke(0x9002, 0x12);
    assert_eq!(m.indicator_bytes()[2], 0x12);
    assert_eq!(m.peek(0x9002), 0x12);
    // ОЗУ в 0000H с загрузочной заглушкой.
    assert_eq!(m.save_bin(0, 3), vec![0xF3, 0xC3, 0x00, 0xF8]);
    assert_eq!(m.save_bin(0x38, 0x3A), vec![0xC3, 0xC1, 0xFF]);
}

#[test]
fn key_beep_goes_to_sound_edges() {
    let mut m = boot();
    m.set_sound_capture(true);
    kb_type(&mut m, "A");
    let edges = m.drain_sound_edges();
    // FE4BH: 191 проход цикла; за проход уровень меняется один раз (OUT A, CMA, OUT A —
    // второй OUT следующего прохода повторяет уровень), т. е. 191 перепад ≈ 95 периодов.
    assert_eq!(edges.len(), 191);
    assert!(
        edges
            .windows(2)
            .all(|w| w[0].0 < w[1].0 && w[0].1 != w[1].1)
    );
}

/// Устройство расширения, которое запоминает стековые записи и отвечает на порт 40H.
struct Probe {
    stack_writes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl ut88_core::Expansion for Probe {
    fn name(&self) -> &str {
        "probe"
    }
    fn mem_write(&mut self, _addr: u16, _v: u8, access: ut88_core::MemAccess) -> bool {
        if access == ut88_core::MemAccess::Stack {
            self.stack_writes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        false
    }
    fn port_read(&mut self, port: u8) -> Option<u8> {
        (port == 0x40).then_some(0x42)
    }
}

#[test]
fn expansion_sees_stack_accesses() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut m = Machine::new(Config::full());
    m.add_expansion(Box::new(Probe {
        stack_writes: counter.clone(),
    }));
    // LXI SP,8000H; LXI B,1234H; PUSH B; IN 40H; HLT
    m.load_bin(
        0x2000,
        &[0x31, 0x00, 0x80, 0x01, 0x34, 0x12, 0xC5, 0xDB, 0x40, 0x76],
    );
    m.jump(0x2000);
    m.run_cycles(100);
    assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(m.cpu.a, 0x42);
    assert_eq!(m.save_bin(0x7FFE, 0x7FFF), vec![0x34, 0x12]);
}

// ----- часы и директива B в режиме «часы в МОНИТОРЕ-«F»» -----

/// Время МОНИТОРА-«F»: секунды, минуты, часы (F6FDH–F6FFH).
fn clock_f(m: &Machine) -> [u8; 3] {
    let v = m.save_bin(0xF6FD, 0xF6FF);
    [v[0], v[1], v[2]]
}

/// Директива B вызывает подпрограммы МОНИТОРА-«0» `RST 5` и `RST 3` (0028H, 0018H). В
/// конфигурации `full` ПЗУ модуля 1 выключено, там ОЗУ с нулями: процессор «проезжает»
/// по `NOP` до заглушки 0038H `JMP FFC1H` и дважды выполняет обработчик часов. Итог:
/// время F6FDH +2 с на индикаторах, прерывания разрешены (`EI` в конце обработчика) —
/// часы пошли. Время МОНИТОРА-«0» (C3FDH) не показывается.
#[test]
fn directive_b_in_full_runs_clock_handler() {
    let mut m = boot();
    kb_type(&mut m, "MF6FD\r00\r30\r12\r.");
    assert!(!m.cpu.inte);
    m.poke(0xC3FD, 0x11);
    m.poke(0xC3FE, 0x22);
    m.poke(0xC3FF, 0x33);
    kb_type(&mut m, "B\r");
    assert_eq!(&clock_f(&m)[1..], &[0x30, 0x12]);
    let s = clock_f(&m)[0];
    assert!((0x02..=0x04).contains(&s), "секунды {s:02X}");
    assert!(m.cpu.inte, "часы пошли");
    assert_eq!(m.indicator_bytes(), clock_f(&m));
    m.run_seconds(3.0);
    assert!(clock_f(&m)[0] >= s + 2);
    // На экране — только эхо директивы и новое приглашение.
    let t = screen_text(&m);
    let tail: Vec<_> = t.lines().skip_while(|l| *l != " =>B").collect();
    assert_eq!(tail.first(), Some(&" =>B"), "{t}");
    assert_eq!(tail.get(1), Some(&" =>"), "{t}");
}

/// Положить в 2000H программу `<op>; EI; JMP 2002H` и дать ей поработать: прерывание
/// часов всегда приходит при флаге CY, заданном `op` (STC — 1, ORA A — 0).
fn clock_with_carry(stc: bool, start: [u8; 3]) -> Machine {
    let mut m = boot();
    m.load_bin(0xF6FD, &start);
    m.load_bin(
        0x2000,
        &[if stc { 0x37 } else { 0xB7 }, 0xFB, 0xC3, 0x02, 0x20],
    );
    m.jump(0x2000);
    m
}

/// Ошибка ПЗУ (и МОНИТОРА-«F» FFCFH, и МОНИТОРА-«0» 00CFH): секунды считаются
/// `INR A; DAA`. `INR` не меняет CY, а `DAA` при CY = 1 прибавляет 60H, поэтому, если
/// прерывание пришло при CY = 1, время портится: 27 → 88, а от 59 перенос в минуты не
/// происходит. Так работает и настоящий КР580ВМ80А (процессор эмулятора проходит
/// 8080EXM, где `DAA` проверяется со всеми флагами).
#[test]
fn clock_handler_daa_carry_bug() {
    let mut m = clock_with_carry(true, [0x27, 0x30, 0x12]);
    m.run_seconds(1.0);
    assert_eq!(clock_f(&m), [0x88, 0x30, 0x12]);
    let mut m = clock_with_carry(false, [0x27, 0x30, 0x12]);
    m.run_seconds(1.0);
    assert_eq!(clock_f(&m), [0x28, 0x30, 0x12]);
    let mut m = clock_with_carry(false, [0x59, 0x59, 0x12]);
    m.run_seconds(1.0);
    assert_eq!(clock_f(&m), [0x00, 0x00, 0x13]);
}

/// Обход ошибки в ОЗУ (manual.md): по вектору 0038H вместо `JMP FFC1H` положить
/// `PUSH PSW; ORA A; CALL FFC1H; POP PSW; RET` — обработчик получает CY = 0, а флаги
/// прерванной программы сохраняются.
#[test]
fn clock_shim_clears_carry() {
    const SHIM: [u8; 7] = [0xF5, 0xB7, 0xCD, 0xC1, 0xFF, 0xF1, 0xC9];
    let mut m = clock_with_carry(true, [0x58, 0x59, 0x23]);
    m.load_bin(0x0038, &SHIM);
    m.run_seconds(1.0);
    assert_eq!(clock_f(&m), [0x59, 0x59, 0x23]);
    m.run_seconds(1.0);
    assert_eq!(clock_f(&m), [0x00, 0x00, 0x00]);
    assert_eq!(m.indicator_bytes(), [0x00, 0x00, 0x00]);
    // Программа продолжает работать с прежними флагами (CY = 1 после STC).
    assert!((0x2002..=0x2004).contains(&m.cpu.pc));
    assert!(m.cpu.flag_c);

    // Набор заглушки директивой M и пуск часов директивой B.
    let mut m = boot();
    kb_type(&mut m, "M38\rF5\rB7\rCD\rC1\rFF\rF1\rC9\r.");
    assert_eq!(m.save_bin(0x38, 0x3E), SHIM);
    kb_type(&mut m, "MF6FD\r58\r59\r23\r.");
    kb_type(&mut m, "B\r");
    m.run_seconds(120.0);
    let [s, mi, h] = clock_f(&m);
    assert_eq!(h, 0x00);
    assert_eq!(mi, 0x02);
    assert!(s <= 0x59 && s & 0x0F <= 9, "{s:02X}");
}
