//! Таблица команд КР580ВМ80А в мнемонике Intel (`doc/10-sistema-komand.md`): мнемоника →
//! вид операндов и базовый код. Всего 244 документированных кода.

/// Вид операндов команды.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Без операндов: код.
    None(u8),
    /// Регистр-приёмник `r` (биты 5–3): `INR`, `DCR`.
    Dst(u8),
    /// Регистр-источник `r` (биты 2–0): `ADD`…`CMP`.
    Src(u8),
    /// `MOV r1,r2` (кроме `MOV M,M` — это HLT).
    Mov,
    /// `MVI r,d8`.
    Mvi,
    /// Непосредственный байт: `ADI`…`CPI`, `IN`, `OUT`.
    Imm8(u8),
    /// 16-битный адрес или число: `JMP`, `CALL`, `Jcc`, `Ccc`, `LDA`, `STA`, `LHLD`, `SHLD`.
    Imm16(u8),
    /// `LXI rp,d16`, rp = B D H SP.
    Lxi,
    /// Пара B D H SP (биты 5–4): `INX`, `DCX`, `DAD`.
    Rp(u8),
    /// Пара B D: `STAX`, `LDAX`.
    RpBd(u8),
    /// Пара B D H PSW: `PUSH`, `POP`.
    RpPsw(u8),
    /// `RST n`, n = 0…7.
    Rst,
}

impl Form {
    /// Длина команды в байтах.
    pub fn size(self) -> u16 {
        match self {
            Form::Mvi | Form::Imm8(_) => 2,
            Form::Imm16(_) | Form::Lxi => 3,
            _ => 1,
        }
    }
}

/// Все мнемоники с видом операндов.
pub const TABLE: &[(&str, Form)] = &[
    ("NOP", Form::None(0x00)),
    ("HLT", Form::None(0x76)),
    ("RLC", Form::None(0x07)),
    ("RRC", Form::None(0x0F)),
    ("RAL", Form::None(0x17)),
    ("RAR", Form::None(0x1F)),
    ("DAA", Form::None(0x27)),
    ("CMA", Form::None(0x2F)),
    ("STC", Form::None(0x37)),
    ("CMC", Form::None(0x3F)),
    ("RET", Form::None(0xC9)),
    ("RNZ", Form::None(0xC0)),
    ("RZ", Form::None(0xC8)),
    ("RNC", Form::None(0xD0)),
    ("RC", Form::None(0xD8)),
    ("RPO", Form::None(0xE0)),
    ("RPE", Form::None(0xE8)),
    ("RP", Form::None(0xF0)),
    ("RM", Form::None(0xF8)),
    ("XTHL", Form::None(0xE3)),
    ("PCHL", Form::None(0xE9)),
    ("XCHG", Form::None(0xEB)),
    ("SPHL", Form::None(0xF9)),
    ("DI", Form::None(0xF3)),
    ("EI", Form::None(0xFB)),
    ("INR", Form::Dst(0x04)),
    ("DCR", Form::Dst(0x05)),
    ("ADD", Form::Src(0x80)),
    ("ADC", Form::Src(0x88)),
    ("SUB", Form::Src(0x90)),
    ("SBB", Form::Src(0x98)),
    ("ANA", Form::Src(0xA0)),
    ("XRA", Form::Src(0xA8)),
    ("ORA", Form::Src(0xB0)),
    ("CMP", Form::Src(0xB8)),
    ("MOV", Form::Mov),
    ("MVI", Form::Mvi),
    ("ADI", Form::Imm8(0xC6)),
    ("ACI", Form::Imm8(0xCE)),
    ("SUI", Form::Imm8(0xD6)),
    ("SBI", Form::Imm8(0xDE)),
    ("ANI", Form::Imm8(0xE6)),
    ("XRI", Form::Imm8(0xEE)),
    ("ORI", Form::Imm8(0xF6)),
    ("CPI", Form::Imm8(0xFE)),
    ("OUT", Form::Imm8(0xD3)),
    ("IN", Form::Imm8(0xDB)),
    ("JMP", Form::Imm16(0xC3)),
    ("JNZ", Form::Imm16(0xC2)),
    ("JZ", Form::Imm16(0xCA)),
    ("JNC", Form::Imm16(0xD2)),
    ("JC", Form::Imm16(0xDA)),
    ("JPO", Form::Imm16(0xE2)),
    ("JPE", Form::Imm16(0xEA)),
    ("JP", Form::Imm16(0xF2)),
    ("JM", Form::Imm16(0xFA)),
    ("CALL", Form::Imm16(0xCD)),
    ("CNZ", Form::Imm16(0xC4)),
    ("CZ", Form::Imm16(0xCC)),
    ("CNC", Form::Imm16(0xD4)),
    ("CC", Form::Imm16(0xDC)),
    ("CPO", Form::Imm16(0xE4)),
    ("CPE", Form::Imm16(0xEC)),
    ("CP", Form::Imm16(0xF4)),
    ("CM", Form::Imm16(0xFC)),
    ("LDA", Form::Imm16(0x3A)),
    ("STA", Form::Imm16(0x32)),
    ("LHLD", Form::Imm16(0x2A)),
    ("SHLD", Form::Imm16(0x22)),
    ("LXI", Form::Lxi),
    ("INX", Form::Rp(0x03)),
    ("DCX", Form::Rp(0x0B)),
    ("DAD", Form::Rp(0x09)),
    ("STAX", Form::RpBd(0x02)),
    ("LDAX", Form::RpBd(0x0A)),
    ("PUSH", Form::RpPsw(0xC5)),
    ("POP", Form::RpPsw(0xC1)),
    ("RST", Form::Rst),
];

/// Найти мнемонику (без учёта регистра).
pub fn lookup(mnemonic: &str) -> Option<Form> {
    TABLE
        .iter()
        .find(|(m, _)| m.eq_ignore_ascii_case(mnemonic))
        .map(|&(_, f)| f)
}

/// Номер 8-битного регистра: B C D E H L M A → 0…7.
pub fn reg8(name: &str) -> Option<u8> {
    ["B", "C", "D", "E", "H", "L", "M", "A"]
        .iter()
        .position(|r| r.eq_ignore_ascii_case(name))
        .map(|i| i as u8)
}

/// Номер пары для LXI/INX/DCX/DAD: B D H SP → 0…3.
pub fn rp_sp(name: &str) -> Option<u8> {
    ["B", "D", "H", "SP"]
        .iter()
        .position(|r| r.eq_ignore_ascii_case(name))
        .map(|i| i as u8)
}

/// Номер пары для PUSH/POP: B D H PSW → 0…3.
pub fn rp_psw(name: &str) -> Option<u8> {
    ["B", "D", "H", "PSW"]
        .iter()
        .position(|r| r.eq_ignore_ascii_case(name))
        .map(|i| i as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Число кодов, которые порождает таблица, — 244.
    #[test]
    fn covers_244_codes() {
        let mut codes = std::collections::BTreeSet::new();
        for &(_, f) in TABLE {
            match f {
                Form::None(c) | Form::Imm8(c) | Form::Imm16(c) => {
                    codes.insert(c);
                }
                Form::Dst(b) => (0..8).for_each(|r| {
                    codes.insert(b | (r << 3));
                }),
                Form::Src(b) => (0..8).for_each(|r| {
                    codes.insert(b | r);
                }),
                Form::Mov => {
                    for d in 0..8u8 {
                        for s in 0..8u8 {
                            if !(d == 6 && s == 6) {
                                codes.insert(0x40 | (d << 3) | s);
                            }
                        }
                    }
                }
                Form::Mvi => (0..8).for_each(|r| {
                    codes.insert(0x06 | (r << 3));
                }),
                Form::Lxi => (0..4).for_each(|p| {
                    codes.insert(0x01 | (p << 4));
                }),
                Form::Rp(b) | Form::RpPsw(b) => (0..4).for_each(|p| {
                    codes.insert(b | (p << 4));
                }),
                Form::RpBd(b) => (0..2).for_each(|p| {
                    codes.insert(b | (p << 4));
                }),
                Form::Rst => (0..8).for_each(|n| {
                    codes.insert(0xC7 | (n << 3));
                }),
            }
        }
        assert_eq!(codes.len(), 244);
    }
}
