//! Юнит-тесты команд КР580ВМ80А: группы команд, флаги, DAA, такты, прерывания,
//! недокументированные коды, дизассемблер.

mod common;

use common::RamBus;
use i8080::{Cpu, HALT_STEP_CYCLES, disasm, instruction_len};

/// Процессор и шина с программой `code`, загруженной с адреса 0000H; SP = 8000H.
fn setup(code: &[u8]) -> (Cpu, RamBus) {
    let mut bus = RamBus::new();
    bus.load(0, code);
    let mut cpu = Cpu::new();
    cpu.sp = 0x8000;
    (cpu, bus)
}

/// Выполнить `n` команд, вернуть сумму тактов.
fn run(cpu: &mut Cpu, bus: &mut RamBus, n: usize) -> u32 {
    (0..n).map(|_| cpu.step(bus)).sum()
}

/// Выполнить одну команду `code` при A = `a` и заданных признаках, вернуть процессор.
fn alu(code: &[u8], a: u8, flags: u8) -> Cpu {
    let (mut cpu, mut bus) = setup(code);
    cpu.a = a;
    cpu.set_flags(flags);
    cpu.step(&mut bus);
    cpu
}

const S: u8 = 0x80;
const Z: u8 = 0x40;
const AC: u8 = 0x10;
const P: u8 = 0x04;
const CY: u8 = 0x01;
/// Всегда установленный бит 1 регистра признаков.
const ONE: u8 = 0x02;

// ---------------------------------------------------------------- регистры и PSW

#[test]
fn psw_format() {
    let mut cpu = Cpu::new();
    cpu.set_flags(0xFF);
    assert_eq!(cpu.flags(), 0xD7, "биты 5 и 3 всегда 0, бит 1 всегда 1");
    cpu.set_flags(0x00);
    assert_eq!(cpu.flags(), 0x02);
    cpu.set_psw(0x12FF);
    assert_eq!(cpu.a, 0x12);
    assert_eq!(cpu.psw(), 0x12D7);
}

#[test]
fn register_pairs() {
    let mut cpu = Cpu::new();
    cpu.set_bc(0x1234);
    cpu.set_de(0x5678);
    cpu.set_hl(0x9ABC);
    assert_eq!(
        (cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l),
        (0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC)
    );
    assert_eq!((cpu.bc(), cpu.de(), cpu.hl()), (0x1234, 0x5678, 0x9ABC));
}

// ---------------------------------------------------------------- пересылки

#[test]
fn data_transfer() {
    // LXI B,1234H; LXI D,5678H; LXI H,0C000H; LXI SP,0F000H; MVI A,42H; MOV B,A; MVI M,99H;
    // MOV C,M; STA 0C001H; LDA 0C000H; STAX D; LDAX B
    let code = [
        0x01, 0x34, 0x12, 0x11, 0x78, 0x56, 0x21, 0x00, 0xC0, 0x31, 0x00, 0xF0, 0x3E, 0x42, 0x47,
        0x36, 0x99, 0x4E, 0x32, 0x01, 0xC0, 0x3A, 0x00, 0xC0, 0x12, 0x0A,
    ];
    let (mut cpu, mut bus) = setup(&code);
    run(&mut cpu, &mut bus, 4);
    assert_eq!(
        (cpu.bc(), cpu.de(), cpu.hl(), cpu.sp),
        (0x1234, 0x5678, 0xC000, 0xF000)
    );
    run(&mut cpu, &mut bus, 2);
    assert_eq!((cpu.a, cpu.b), (0x42, 0x42));
    run(&mut cpu, &mut bus, 2);
    assert_eq!(bus.mem[0xC000], 0x99);
    assert_eq!(cpu.c, 0x99);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(bus.mem[0xC001], 0x42);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.a, 0x99);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(bus.mem[0x5678], 0x99);
    bus.mem[0x4299] = 0x77; // BC = 4299H после MOV B,A и MOV C,M
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.a, 0x77);
}

#[test]
fn lhld_shld_xchg_xthl_sphl_pchl() {
    // LHLD 0100H; SHLD 0200H; XCHG; XTHL; SPHL; PCHL
    let code = [0x2A, 0x00, 0x01, 0x22, 0x00, 0x02, 0xEB, 0xE3, 0xF9, 0xE9];
    let (mut cpu, mut bus) = setup(&code);
    bus.load(0x0100, &[0x34, 0x12]);
    bus.load(0x8000, &[0xCD, 0xAB]);
    cpu.set_de(0x5678);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.hl(), 0x1234);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(&bus.mem[0x200..0x202], &[0x34, 0x12]);
    run(&mut cpu, &mut bus, 1);
    assert_eq!((cpu.hl(), cpu.de()), (0x5678, 0x1234));
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.hl(), 0xABCD);
    assert_eq!(&bus.mem[0x8000..0x8002], &[0x78, 0x56]);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.sp, 0xABCD);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.pc, 0xABCD);
}

#[test]
fn push_pop() {
    // PUSH B; PUSH PSW; POP D; POP H; POP PSW
    let code = [0xC5, 0xF5, 0xD1, 0xE1, 0xF1];
    let (mut cpu, mut bus) = setup(&code);
    cpu.set_bc(0x1234);
    cpu.a = 0x56;
    cpu.set_flags(S | CY);
    run(&mut cpu, &mut bus, 2);
    assert_eq!(cpu.sp, 0x7FFC);
    assert_eq!(&bus.mem[0x7FFC..0x8000], &[S | CY | ONE, 0x56, 0x34, 0x12]);
    run(&mut cpu, &mut bus, 2);
    assert_eq!((cpu.de(), cpu.hl()), (0x5683, 0x1234));
    // POP PSW со стека 0FFH 0FFH: признаки = 0D7H
    bus.load(0x8000, &[0xFF, 0xFF]);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.psw(), 0xFFD7);
}

// ---------------------------------------------------------------- арифметика

#[test]
fn add_flags() {
    let c = alu(&[0xC6, 0x01], 0x0F, 0); // ADI 1
    assert_eq!((c.a, c.flags()), (0x10, AC | ONE));
    let c = alu(&[0xC6, 0x01], 0xFF, 0);
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | CY | ONE));
    let c = alu(&[0xC6, 0x01], 0x7F, 0);
    assert_eq!((c.a, c.flags()), (0x80, S | AC | ONE));
    let c = alu(&[0xCE, 0x00], 0xFF, CY); // ACI 0 с переносом
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | CY | ONE));
    let c = alu(&[0xCE, 0x10], 0x20, CY);
    assert_eq!((c.a, c.flags()), (0x31, ONE));
}

#[test]
fn sub_flags() {
    // SUI: AC — перенос из бита 3 при сложении с дополнением.
    let c = alu(&[0xD6, 0x01], 0x00, 0);
    assert_eq!((c.a, c.flags()), (0xFF, S | P | CY | ONE));
    let c = alu(&[0xD6, 0x05], 0x05, 0);
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | ONE));
    let c = alu(&[0xD6, 0x01], 0x10, 0);
    assert_eq!((c.a, c.flags()), (0x0F, P | ONE));
    let c = alu(&[0x97], 0x3E, CY); // SUB A
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | ONE));
    let c = alu(&[0xDE, 0x01], 0x02, CY); // SBI 1 с заёмом
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | ONE));
    let c = alu(&[0xDE, 0x00], 0x00, CY);
    assert_eq!((c.a, c.flags()), (0xFF, S | P | CY | ONE));
}

#[test]
fn cmp_keeps_a() {
    let c = alu(&[0xFE, 0x40], 0x30, 0); // CPI 40H
    assert_eq!(c.a, 0x30);
    assert!(c.flag_c && !c.flag_z);
    let c = alu(&[0xFE, 0x30], 0x30, 0);
    assert!(!c.flag_c && c.flag_z);
    let c = alu(&[0xFE, 0x20], 0x30, 0);
    assert!(!c.flag_c && !c.flag_z);
}

#[test]
fn logic_flags() {
    // ANA: AC = OR битов 3 операндов, CY = 0.
    let c = alu(&[0xE6, 0x00], 0x08, CY);
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | ONE));
    let c = alu(&[0xE6, 0x08], 0x00, CY);
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | ONE));
    let c = alu(&[0xE6, 0xF0], 0xF7, CY);
    assert_eq!((c.a, c.flags()), (0xF0, S | P | ONE));
    // XRA A: всё обнуляется.
    let c = alu(&[0xAF], 0x5A, CY | AC | S);
    assert_eq!((c.a, c.flags()), (0x00, Z | P | ONE));
    // ORI
    let c = alu(&[0xF6, 0x01], 0x80, CY | AC);
    assert_eq!((c.a, c.flags()), (0x81, S | P | ONE));
    let c = alu(&[0xF6, 0x00], 0x07, 0);
    assert_eq!((c.a, c.flags()), (0x07, ONE));
}

#[test]
fn inr_dcr() {
    // INR A: CY не меняется
    let c = alu(&[0x3C], 0xFF, CY);
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | CY | ONE));
    let c = alu(&[0x3C], 0x0E, 0);
    assert_eq!((c.a, c.flags()), (0x0F, P | ONE));
    // DCR A: AC = 1, если не было заёма из бита 4
    let c = alu(&[0x3D], 0x00, 0);
    assert_eq!((c.a, c.flags()), (0xFF, S | P | ONE));
    let c = alu(&[0x3D], 0x01, 0);
    assert_eq!((c.a, c.flags()), (0x00, Z | AC | P | ONE));
    let c = alu(&[0x3D], 0x10, CY);
    assert_eq!((c.a, c.flags()), (0x0F, P | CY | ONE));
    // INR M / DCR M
    let (mut cpu, mut bus) = setup(&[0x34, 0x35, 0x35]);
    cpu.set_hl(0x4000);
    bus.mem[0x4000] = 0x7F;
    assert_eq!(cpu.step(&mut bus), 10);
    assert_eq!(bus.mem[0x4000], 0x80);
    run(&mut cpu, &mut bus, 2);
    assert_eq!(bus.mem[0x4000], 0x7E);
}

#[test]
fn inx_dcx_dad() {
    // INX B; DCX D; INX SP; DAD B; DAD H
    let (mut cpu, mut bus) = setup(&[0x03, 0x1B, 0x33, 0x09, 0x29]);
    cpu.set_bc(0xFFFF);
    cpu.set_de(0x0000);
    cpu.sp = 0xFFFF;
    cpu.set_hl(0x8001);
    run(&mut cpu, &mut bus, 3);
    assert_eq!((cpu.bc(), cpu.de(), cpu.sp), (0x0000, 0xFFFF, 0x0000));
    assert_eq!(cpu.flags(), ONE, "INX/DCX не меняют признаков");
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.hl(), 0x8001);
    assert!(!cpu.flag_c);
    run(&mut cpu, &mut bus, 1);
    assert_eq!(cpu.hl(), 0x0002);
    assert_eq!(cpu.flags(), ONE | CY, "DAD меняет только CY");
}

#[test]
fn daa() {
    // 09H + 08H = 11H (AC=1) → DAA → 17H
    let (mut cpu, mut bus) = setup(&[0xC6, 0x08, 0x27]);
    cpu.a = 0x09;
    run(&mut cpu, &mut bus, 2);
    assert_eq!(cpu.a, 0x17);
    assert!(!cpu.flag_c);
    // 99H + 01H = 9AH → DAA → 00H, CY=1, AC=1
    let (mut cpu, mut bus) = setup(&[0xC6, 0x01, 0x27]);
    cpu.a = 0x99;
    run(&mut cpu, &mut bus, 2);
    assert_eq!((cpu.a, cpu.flags()), (0x00, Z | AC | P | CY | ONE));
    // 9BH → DAA → 01H, CY=1 (пример из документации Intel)
    let c = alu(&[0x27], 0x9B, 0);
    assert_eq!((c.a, c.flags()), (0x01, AC | CY | ONE));
    // 45H + 38H = 7DH → 83H
    let c = alu(&[0x27], 0x7D, 0);
    assert_eq!((c.a, c.flags()), (0x83, S | AC | ONE));
    // CY сохраняется: 15H с CY → 75H
    let c = alu(&[0x27], 0x15, CY);
    assert_eq!((c.a, c.flags()), (0x75, CY | ONE));
    // Все суммы двух BCD-чисел дают правильный BCD-результат.
    for x in 0..100u32 {
        for y in 0..100u32 {
            let bcd = |v: u32| (((v / 10) << 4) | (v % 10)) as u8;
            let (mut cpu, mut bus) = setup(&[0xC6, bcd(y), 0x27]);
            cpu.a = bcd(x);
            run(&mut cpu, &mut bus, 2);
            assert_eq!(cpu.a, bcd((x + y) % 100), "{x}+{y}");
            assert_eq!(cpu.flag_c, x + y >= 100, "{x}+{y}");
        }
    }
}

#[test]
fn rotates_and_misc() {
    let c = alu(&[0x07], 0x81, 0); // RLC
    assert_eq!((c.a, c.flag_c), (0x03, true));
    let c = alu(&[0x0F], 0x81, 0); // RRC
    assert_eq!((c.a, c.flag_c), (0xC0, true));
    let c = alu(&[0x17], 0x80, 0); // RAL
    assert_eq!((c.a, c.flag_c), (0x00, true));
    let c = alu(&[0x17], 0x00, CY);
    assert_eq!((c.a, c.flag_c), (0x01, false));
    let c = alu(&[0x1F], 0x01, 0); // RAR
    assert_eq!((c.a, c.flag_c), (0x00, true));
    let c = alu(&[0x1F], 0x00, CY);
    assert_eq!((c.a, c.flag_c), (0x80, false));
    let c = alu(&[0x07], 0x00, Z | S | P | AC);
    assert_eq!(c.flags(), Z | S | P | AC | ONE, "сдвиги меняют только CY");
    let c = alu(&[0x2F], 0x5A, 0); // CMA
    assert_eq!((c.a, c.flags()), (0xA5, ONE));
    let c = alu(&[0x37], 0, 0); // STC
    assert_eq!(c.flags(), CY | ONE);
    let c = alu(&[0x3F], 0, CY); // CMC
    assert_eq!(c.flags(), ONE);
}

// ---------------------------------------------------------------- переходы

#[test]
fn jumps_calls_returns() {
    // 0000: CALL 0010H; 0003: HLT; 0010: RST 1 (→0008); 0008: RET; 0011: RET
    let (mut cpu, mut bus) = setup(&[0xCD, 0x10, 0x00, 0x76]);
    bus.mem[0x08] = 0xC9;
    bus.mem[0x10] = 0xCF;
    bus.mem[0x11] = 0xC9;
    assert_eq!(cpu.step(&mut bus), 17);
    assert_eq!((cpu.pc, cpu.sp), (0x0010, 0x7FFE));
    assert_eq!(&bus.mem[0x7FFE..0x8000], &[0x03, 0x00]);
    assert_eq!(cpu.step(&mut bus), 11);
    assert_eq!(cpu.pc, 0x0008);
    assert_eq!(cpu.step(&mut bus), 10);
    assert_eq!(cpu.pc, 0x0011);
    cpu.step(&mut bus);
    assert_eq!((cpu.pc, cpu.sp), (0x0003, 0x8000));
    assert_eq!(cpu.step(&mut bus), 7);
    assert!(cpu.halted);
    assert_eq!(cpu.step(&mut bus), HALT_STEP_CYCLES);
    assert_eq!(cpu.pc, 0x0004);
}

#[test]
fn conditions() {
    // Для каждого из 8 условий: Jcc, Ccc, Rcc при истинном и ложном условии.
    // cc: NZ Z NC C PO PE P M; флаг и значение, при котором условие истинно.
    let cases: [(u8, bool); 8] = [
        (Z, false),
        (Z, true),
        (CY, false),
        (CY, true),
        (P, false),
        (P, true),
        (S, false),
        (S, true),
    ];
    for (cc, &(flag, val)) in cases.iter().enumerate() {
        let cc = cc as u8;
        for taken in [true, false] {
            let flags = if val == taken { flag } else { 0 };
            // Jcc 1234H
            let (mut cpu, mut bus) = setup(&[0xC2 | cc << 3, 0x34, 0x12]);
            cpu.set_flags(flags);
            assert_eq!(cpu.step(&mut bus), 10);
            assert_eq!(cpu.pc, if taken { 0x1234 } else { 3 }, "J cc={cc}");
            // Ccc 1234H
            let (mut cpu, mut bus) = setup(&[0xC4 | cc << 3, 0x34, 0x12]);
            cpu.set_flags(flags);
            assert_eq!(cpu.step(&mut bus), if taken { 17 } else { 11 }, "C cc={cc}");
            assert_eq!(cpu.pc, if taken { 0x1234 } else { 3 });
            assert_eq!(cpu.sp, if taken { 0x7FFE } else { 0x8000 });
            // Rcc
            let (mut cpu, mut bus) = setup(&[0xC0 | cc << 3]);
            bus.load(0x8000, &[0x34, 0x12]);
            cpu.set_flags(flags);
            assert_eq!(cpu.step(&mut bus), if taken { 11 } else { 5 }, "R cc={cc}");
            assert_eq!(cpu.pc, if taken { 0x1234 } else { 1 });
        }
    }
}

// ---------------------------------------------------------------- ввод-вывод

#[test]
fn in_out() {
    // MVI A,55H; OUT 0A1H; IN 0A0H
    let (mut cpu, mut bus) = setup(&[0x3E, 0x55, 0xD3, 0xA1, 0xDB, 0xA0]);
    bus.input_value = 0x3C;
    run(&mut cpu, &mut bus, 2);
    assert_eq!(bus.outputs, vec![(0xA1, 0x55)]);
    assert_eq!(cpu.step(&mut bus), 10);
    assert_eq!(cpu.a, 0x3C);
}

// ---------------------------------------------------------------- такты

/// Такты всех 256 кодов по документации Intel 8080 (для условных CALL/RET — условие
/// не выполнено; недокументированные коды — как их документированные двойники).
#[rustfmt::skip]
const CYCLES: [u32; 256] = [
//  0   1   2   3   4   5   6   7   8   9   A   B   C   D   E   F
    4, 10,  7,  5,  5,  5,  7,  4,  4, 10,  7,  5,  5,  5,  7,  4, // 0
    4, 10,  7,  5,  5,  5,  7,  4,  4, 10,  7,  5,  5,  5,  7,  4, // 1
    4, 10, 16,  5,  5,  5,  7,  4,  4, 10, 16,  5,  5,  5,  7,  4, // 2
    4, 10, 13,  5, 10, 10, 10,  4,  4, 10, 13,  5,  5,  5,  7,  4, // 3
    5,  5,  5,  5,  5,  5,  7,  5,  5,  5,  5,  5,  5,  5,  7,  5, // 4
    5,  5,  5,  5,  5,  5,  7,  5,  5,  5,  5,  5,  5,  5,  7,  5, // 5
    5,  5,  5,  5,  5,  5,  7,  5,  5,  5,  5,  5,  5,  5,  7,  5, // 6
    7,  7,  7,  7,  7,  7,  7,  7,  5,  5,  5,  5,  5,  5,  7,  5, // 7
    4,  4,  4,  4,  4,  4,  7,  4,  4,  4,  4,  4,  4,  4,  7,  4, // 8
    4,  4,  4,  4,  4,  4,  7,  4,  4,  4,  4,  4,  4,  4,  7,  4, // 9
    4,  4,  4,  4,  4,  4,  7,  4,  4,  4,  4,  4,  4,  4,  7,  4, // A
    4,  4,  4,  4,  4,  4,  7,  4,  4,  4,  4,  4,  4,  4,  7,  4, // B
    5, 10, 10, 10, 11, 11,  7, 11,  5, 10, 10, 10, 11, 17,  7, 11, // C
    5, 10, 10, 10, 11, 11,  7, 11,  5, 10, 10, 10, 11, 17,  7, 11, // D
    5, 10, 10, 18, 11, 11,  7, 11,  5,  5, 10,  4, 11, 17,  7, 11, // E
    5, 10, 10,  4, 11, 11,  7, 11,  5,  5, 10,  4, 11, 17,  7, 11, // F
];

#[test]
fn cycle_table() {
    for op in 0..=255u8 {
        let (mut cpu, mut bus) = setup(&[op, 0x00, 0x40]);
        cpu.set_hl(0x4000);
        // Для условных команд (Rcc, Ccc) делаем условие ложным.
        if op >= 0xC0 && (op & 7 == 0 || op & 7 == 4) {
            let cc = (op >> 3) & 7;
            let flag = [Z, CY, P, S][usize::from(cc >> 1)];
            cpu.set_flags(if cc & 1 == 0 { flag } else { 0 });
        }
        assert_eq!(cpu.step(&mut bus), CYCLES[usize::from(op)], "код {op:02X}H");
    }
}

#[test]
fn cycle_counter() {
    let (mut cpu, mut bus) = setup(&[0x00, 0x41, 0xC3, 0x00, 0x00]); // NOP; MOV B,C; JMP 0
    let t = run(&mut cpu, &mut bus, 6);
    assert_eq!(t, 2 * (4 + 5 + 10));
    assert_eq!(cpu.cycles, u64::from(t));
}

// ---------------------------------------------------------------- недокументированные коды

#[test]
fn undocumented_opcodes() {
    for op in [0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0x38] {
        let (mut cpu, mut bus) = setup(&[op]);
        let before = cpu.clone();
        assert_eq!(cpu.step(&mut bus), 4);
        assert_eq!(cpu.pc, 1);
        assert_eq!(
            (cpu.a, cpu.flags(), cpu.sp),
            (before.a, before.flags(), before.sp)
        );
    }
    // CB = JMP
    let (mut cpu, mut bus) = setup(&[0xCB, 0x34, 0x12]);
    assert_eq!(cpu.step(&mut bus), 10);
    assert_eq!(cpu.pc, 0x1234);
    // D9 = RET
    let (mut cpu, mut bus) = setup(&[0xD9]);
    bus.load(0x8000, &[0x78, 0x56]);
    assert_eq!(cpu.step(&mut bus), 10);
    assert_eq!((cpu.pc, cpu.sp), (0x5678, 0x8002));
    // DD, ED, FD = CALL
    for op in [0xDD, 0xED, 0xFD] {
        let (mut cpu, mut bus) = setup(&[op, 0x34, 0x12]);
        assert_eq!(cpu.step(&mut bus), 17);
        assert_eq!((cpu.pc, cpu.sp), (0x1234, 0x7FFE));
        assert_eq!(&bus.mem[0x7FFE..0x8000], &[0x03, 0x00]);
    }
}

// ---------------------------------------------------------------- прерывания

#[test]
fn interrupt_disabled() {
    let (mut cpu, mut bus) = setup(&[0x00]);
    assert!(!cpu.inte, "после создания прерывания запрещены");
    assert_eq!(cpu.interrupt(&mut bus, 0xFF), None);
    assert_eq!(cpu.pc, 0);
}

#[test]
fn ei_delay_and_rst7() {
    // EI; NOP; NOP
    let (mut cpu, mut bus) = setup(&[0xFB, 0x00, 0x00]);
    cpu.step(&mut bus);
    assert!(cpu.inte);
    assert_eq!(
        cpu.interrupt(&mut bus, 0xFF),
        None,
        "сразу после EI прерывание не принимается"
    );
    cpu.step(&mut bus);
    assert_eq!(cpu.interrupt(&mut bus, 0xFF), Some(11));
    assert_eq!(cpu.pc, 0x0038);
    assert!(!cpu.inte, "приём прерывания сбрасывает INTE");
    assert_eq!(
        &bus.mem[0x7FFE..0x8000],
        &[0x02, 0x00],
        "в стеке адрес следующей команды"
    );
    assert_eq!(cpu.interrupt(&mut bus, 0xFF), None);
}

#[test]
fn di_blocks_interrupt() {
    // EI; NOP; DI; NOP
    let (mut cpu, mut bus) = setup(&[0xFB, 0x00, 0xF3, 0x00]);
    run(&mut cpu, &mut bus, 3);
    assert!(!cpu.inte);
    assert_eq!(cpu.interrupt(&mut bus, 0xFF), None);
}

#[test]
fn interrupt_wakes_from_hlt() {
    // EI; HLT
    let (mut cpu, mut bus) = setup(&[0xFB, 0x76]);
    run(&mut cpu, &mut bus, 2);
    assert!(cpu.halted);
    assert_eq!(cpu.step(&mut bus), HALT_STEP_CYCLES);
    assert_eq!(cpu.pc, 2);
    let cycles = cpu.cycles;
    assert_eq!(cpu.interrupt(&mut bus, 0xFF), Some(11));
    assert_eq!(cpu.cycles, cycles + 11);
    assert!(!cpu.halted);
    assert_eq!(cpu.pc, 0x0038);
    assert_eq!(
        &bus.mem[0x7FFE..0x8000],
        &[0x02, 0x00],
        "возврат — на команду после HLT"
    );
}

#[test]
fn reset() {
    let (mut cpu, mut bus) = setup(&[0xFB, 0x76]);
    run(&mut cpu, &mut bus, 2);
    cpu.reset();
    assert_eq!(cpu.pc, 0);
    assert!(!cpu.inte && !cpu.halted);
}

// ---------------------------------------------------------------- дизассемблер

#[test]
fn disassembler() {
    let cases: &[(&[u8], &str, usize)] = &[
        (&[0x00], "NOP", 1),
        (&[0x78], "MOV A,B", 1),
        (&[0x77], "MOV M,A", 1),
        (&[0x76], "HLT", 1),
        (&[0x21, 0x34, 0x12], "LXI H,1234H", 3),
        (&[0x31, 0x00, 0xC4], "LXI SP,0C400H", 3),
        (&[0xC3, 0x00, 0xC0], "JMP 0C000H", 3),
        (&[0xC3, 0x00, 0x00], "JMP 0000H", 3),
        (&[0x3E, 0xFF], "MVI A,0FFH", 2),
        (&[0x36, 0x05], "MVI M,05H", 2),
        (&[0xFE, 0x0A], "CPI 0AH", 2),
        (&[0xDB, 0xA0], "IN 0A0H", 2),
        (&[0xD3, 0xA1], "OUT 0A1H", 2),
        (&[0xF5], "PUSH PSW", 1),
        (&[0xC1], "POP B", 1),
        (&[0xFF], "RST 7", 1),
        (&[0xC8], "RZ", 1),
        (&[0xFC, 0x00, 0x90], "CM 9000H", 3),
        (&[0xEA, 0x34, 0x12], "JPE 1234H", 3),
        (&[0x96], "SUB M", 1),
        (&[0x32, 0x02, 0x90], "STA 9002H", 3),
        (&[0x1A], "LDAX D", 1),
        (&[0x39], "DAD SP", 1),
        (&[0x08], "*NOP", 1),
        (&[0xCB, 0x34, 0x12], "*JMP 1234H", 3),
        (&[0xD9], "*RET", 1),
        (&[0xFD, 0x34, 0x12], "*CALL 1234H", 3),
        (&[0xCD], "CALL 0000H", 3),
    ];
    for &(bytes, text, len) in cases {
        assert_eq!(
            disasm(bytes, 0x0100),
            (text.to_string(), len),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn disasm_lengths_match_execution() {
    // Для всех некомандных-переходов длина из дизассемблера равна приросту PC.
    for op in 0..=255u8 {
        let is_branch = matches!(op, 0xC0..=0xFF)
            && matches!(op & 7, 0 | 1 | 2 | 3 | 4 | 5 | 7)
            && !matches!(op, 0xC1 | 0xD1 | 0xE1 | 0xF1 | 0xC5 | 0xD5 | 0xE5 | 0xF5)
            && !matches!(op, 0xD3 | 0xDB | 0xE3 | 0xEB | 0xF3 | 0xFB | 0xF9);
        if is_branch || op == 0x76 {
            continue;
        }
        let (mut cpu, mut bus) = setup(&[op, 0x00, 0x40]);
        cpu.set_hl(0x4000);
        cpu.step(&mut bus);
        assert_eq!(usize::from(cpu.pc), instruction_len(op), "код {op:02X}H");
        assert_eq!(disasm(&[op, 0, 0], 0).1, instruction_len(op));
    }
}

// ---------------------------------------------------------------- стековые циклы

/// Шина, раздельно считающая обычные и стековые обращения (бит STACK слова состояния).
struct StackBus {
    ram: RamBus,
    /// Журнал: (стековое, запись, адрес).
    log: Vec<(bool, bool, u16)>,
}

impl i8080::Bus for StackBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.log.push((false, false, addr));
        self.ram.mem[usize::from(addr)]
    }
    fn write(&mut self, addr: u16, v: u8) {
        self.log.push((false, true, addr));
        self.ram.mem[usize::from(addr)] = v;
    }
    fn read_stack(&mut self, addr: u16) -> u8 {
        self.log.push((true, false, addr));
        self.ram.mem[usize::from(addr)]
    }
    fn write_stack(&mut self, addr: u16, v: u8) {
        self.log.push((true, true, addr));
        self.ram.mem[usize::from(addr)] = v;
    }
    fn input(&mut self, _port: u8) -> u8 {
        0
    }
    fn output(&mut self, _port: u8, _v: u8) {}
}

/// Стековые обращения команды `code` при SP = 8000H: список (запись, адрес).
fn stack_cycles(code: &[u8], interrupt: bool) -> (Vec<(bool, u16)>, usize) {
    let (mut cpu, ram) = setup(code);
    let mut bus = StackBus {
        ram,
        log: Vec::new(),
    };
    if interrupt {
        cpu.inte = true;
        cpu.interrupt(&mut bus, 0xFF).unwrap();
    } else {
        cpu.step(&mut bus);
    }
    let stack: Vec<(bool, u16)> = bus.log.iter().filter(|e| e.0).map(|e| (e.1, e.2)).collect();
    let normal = bus.log.iter().filter(|e| !e.0).count();
    (stack, normal)
}

#[test]
fn stack_cycles_are_marked() {
    let w = |a| (true, a);
    let r = |a| (false, a);
    // PUSH B: две записи, обычные — только выборка кода.
    assert_eq!(
        stack_cycles(&[0xC5], false),
        (vec![w(0x7FFF), w(0x7FFE)], 1)
    );
    // POP B.
    assert_eq!(
        stack_cycles(&[0xC1], false),
        (vec![r(0x8000), r(0x8001)], 1)
    );
    // CALL 1234H: выборка 3 байт обычная.
    assert_eq!(
        stack_cycles(&[0xCD, 0x34, 0x12], false),
        (vec![w(0x7FFF), w(0x7FFE)], 3)
    );
    // CZ при Z = 0 стек не трогает.
    assert_eq!(stack_cycles(&[0xCC, 0x34, 0x12], false), (vec![], 3));
    // RET, RST 7.
    assert_eq!(
        stack_cycles(&[0xC9], false),
        (vec![r(0x8000), r(0x8001)], 1)
    );
    assert_eq!(
        stack_cycles(&[0xFF], false),
        (vec![w(0x7FFF), w(0x7FFE)], 1)
    );
    // XTHL: чтение SP, SP+1, запись SP+1, SP.
    assert_eq!(
        stack_cycles(&[0xE3], false),
        (vec![r(0x8000), r(0x8001), w(0x8001), w(0x8000)], 1)
    );
    // Приём прерывания RST 7: код по INTA не читается из памяти.
    assert_eq!(stack_cycles(&[], true), (vec![w(0x7FFF), w(0x7FFE)], 0));
    // MOV M,A / SHLD / SPHL — не стековые.
    assert_eq!(stack_cycles(&[0x77], false).0, vec![]);
    assert_eq!(stack_cycles(&[0x22, 0x00, 0x90], false).0, vec![]);
    assert_eq!(stack_cycles(&[0xF9], false).0, vec![]);
}
