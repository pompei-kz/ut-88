//! Дизассемблер КР580ВМ80А в мнемонике Intel.
//!
//! Числа выводятся шестнадцатеричными с суффиксом `H`; если число начинается с буквы,
//! впереди ставится `0` (`0C000H`, `0FFH`). Недокументированные коды выводятся
//! мнемоникой команды, которую они на самом деле выполняют, со звёздочкой впереди
//! (`*NOP`, `*JMP 1234H`, `*CALL 1234H`, `*RET`).

const REG: [&str; 8] = ["B", "C", "D", "E", "H", "L", "M", "A"];
const RP: [&str; 4] = ["B", "D", "H", "SP"];
const RP_PUSH: [&str; 4] = ["B", "D", "H", "PSW"];
const COND: [&str; 8] = ["NZ", "Z", "NC", "C", "PO", "PE", "P", "M"];
const ALU: [&str; 8] = ["ADD", "ADC", "SUB", "SBB", "ANA", "XRA", "ORA", "CMP"];
const ALU_IMM: [&str; 8] = ["ADI", "ACI", "SUI", "SBI", "ANI", "XRI", "ORI", "CPI"];

/// Длина команды с кодом `op` в байтах (1, 2 или 3).
pub fn instruction_len(op: u8) -> usize {
    match op {
        // MVI, ADI…CPI, IN, OUT
        0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => 2,
        0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE | 0xD3 | 0xDB => 2,
        // LXI, SHLD, LHLD, STA, LDA
        0x01 | 0x11 | 0x21 | 0x31 | 0x22 | 0x2A | 0x32 | 0x3A => 3,
        // Jcc, JMP, Ccc, CALL (вместе с недокументированными)
        0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA | 0xC3 | 0xCB => 3,
        0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => 3,
        0xCD | 0xDD | 0xED | 0xFD => 3,
        _ => 1,
    }
}

fn hex8(v: u8) -> String {
    let s = format!("{v:02X}H");
    if s.as_bytes()[0].is_ascii_alphabetic() {
        format!("0{s}")
    } else {
        s
    }
}

fn hex16(v: u16) -> String {
    let s = format!("{v:04X}H");
    if s.as_bytes()[0].is_ascii_alphabetic() {
        format!("0{s}")
    } else {
        s
    }
}

/// Дизассемблировать одну команду из `bytes` (первый байт — код команды, расположенный
/// по адресу `addr`). Возвращает текст команды и её длину в байтах.
///
/// Если байтов меньше, чем нужно команде, недостающие считаются нулями.
/// `addr` сейчас не влияет на текст (у 8080 нет относительных переходов) и оставлен для
/// единообразия с отладчиком.
pub fn disasm(bytes: &[u8], addr: u16) -> (String, usize) {
    let _ = addr;
    let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let op = byte(0);
    let len = instruction_len(op);
    let d8 = hex8(byte(1));
    let a16 = hex16(u16::from_le_bytes([byte(1), byte(2)]));
    let r_dst = REG[usize::from((op >> 3) & 7)];
    let r_src = REG[usize::from(op & 7)];
    let rp = usize::from((op >> 4) & 3);
    let cc = COND[usize::from((op >> 3) & 7)];

    let text = match op {
        0x00 => "NOP".to_string(),
        0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38 => "*NOP".to_string(),
        0x01 | 0x11 | 0x21 | 0x31 => format!("LXI {},{a16}", RP[rp]),
        0x02 | 0x12 => format!("STAX {}", RP[rp]),
        0x0A | 0x1A => format!("LDAX {}", RP[rp]),
        0x03 | 0x13 | 0x23 | 0x33 => format!("INX {}", RP[rp]),
        0x0B | 0x1B | 0x2B | 0x3B => format!("DCX {}", RP[rp]),
        0x09 | 0x19 | 0x29 | 0x39 => format!("DAD {}", RP[rp]),
        0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => format!("INR {r_dst}"),
        0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => format!("DCR {r_dst}"),
        0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => format!("MVI {r_dst},{d8}"),
        0x07 => "RLC".to_string(),
        0x0F => "RRC".to_string(),
        0x17 => "RAL".to_string(),
        0x1F => "RAR".to_string(),
        0x22 => format!("SHLD {a16}"),
        0x2A => format!("LHLD {a16}"),
        0x27 => "DAA".to_string(),
        0x2F => "CMA".to_string(),
        0x32 => format!("STA {a16}"),
        0x3A => format!("LDA {a16}"),
        0x37 => "STC".to_string(),
        0x3F => "CMC".to_string(),
        0x76 => "HLT".to_string(),
        0x40..=0x7F => format!("MOV {r_dst},{r_src}"),
        0x80..=0xBF => format!("{} {r_src}", ALU[usize::from((op >> 3) & 7)]),
        0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
            format!("{} {d8}", ALU_IMM[usize::from((op >> 3) & 7)])
        }
        0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => format!("R{cc}"),
        0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => format!("J{cc} {a16}"),
        0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => format!("C{cc} {a16}"),
        0xC1 | 0xD1 | 0xE1 | 0xF1 => format!("POP {}", RP_PUSH[rp]),
        0xC5 | 0xD5 | 0xE5 | 0xF5 => format!("PUSH {}", RP_PUSH[rp]),
        0xC9 => "RET".to_string(),
        0xD9 => "*RET".to_string(),
        0xC3 => format!("JMP {a16}"),
        0xCB => format!("*JMP {a16}"),
        0xCD => format!("CALL {a16}"),
        0xDD | 0xED | 0xFD => format!("*CALL {a16}"),
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => format!("RST {}", (op >> 3) & 7),
        0xD3 => format!("OUT {d8}"),
        0xDB => format!("IN {d8}"),
        0xE3 => "XTHL".to_string(),
        0xE9 => "PCHL".to_string(),
        0xEB => "XCHG".to_string(),
        0xF3 => "DI".to_string(),
        0xFB => "EI".to_string(),
        0xF9 => "SPHL".to_string(),
    };
    (text, len)
}
