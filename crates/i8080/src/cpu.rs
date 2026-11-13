//! Процессор КР580ВМ80А: регистры, флаги, выполнение команд.

use crate::Bus;

/// Сколько тактов «тратит» [`Cpu::step`], когда процессор стоит в состоянии HLT.
///
/// В состоянии останова настоящий 8080 не выполняет команд и ждёт прерывания или сброса;
/// эмулятору же нужно, чтобы время шло. Поэтому каждый вызов `step` в состоянии HLT
/// продвигает счётчик тактов на эту величину (длительность самой короткой команды).
pub const HALT_STEP_CYCLES: u32 = 4;

/// Биты регистра признаков (PSW) в формате 8080: `S Z 0 AC 0 P 1 C`.
const FLAG_S: u8 = 0x80;
const FLAG_Z: u8 = 0x40;
const FLAG_AC: u8 = 0x10;
const FLAG_P: u8 = 0x04;
const FLAG_C: u8 = 0x01;

/// Состояние процессора КР580ВМ80А.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cpu {
    /// Аккумулятор.
    pub a: u8,
    /// Регистр B.
    pub b: u8,
    /// Регистр C.
    pub c: u8,
    /// Регистр D.
    pub d: u8,
    /// Регистр E.
    pub e: u8,
    /// Регистр H.
    pub h: u8,
    /// Регистр L.
    pub l: u8,
    /// Указатель стека.
    pub sp: u16,
    /// Счётчик команд.
    pub pc: u16,
    /// Признак знака S.
    pub flag_s: bool,
    /// Признак нуля Z.
    pub flag_z: bool,
    /// Признак вспомогательного переноса AC (перенос из бита 3).
    pub flag_ac: bool,
    /// Признак чётности P (1 — чётное число единиц).
    pub flag_p: bool,
    /// Признак переноса C (CY).
    pub flag_c: bool,
    /// Триггер разрешения прерываний INTE.
    pub inte: bool,
    /// Процессор остановлен командой HLT.
    pub halted: bool,
    /// Только что выполнена команда EI: прерывание будет принято лишь после следующей команды.
    pub ei_delay: bool,
    /// Счётчик тактов с момента создания (сброс его не обнуляет).
    pub cycles: u64,
}

#[inline]
fn parity(v: u8) -> bool {
    v.count_ones().is_multiple_of(2)
}

impl Cpu {
    /// Новый процессор в состоянии после сброса: PC = 0, прерывания запрещены.
    pub fn new() -> Self {
        Self::default()
    }

    /// Сигнал СБРОС: PC = 0, INTE = 0, выход из HLT. Остальные регистры не меняются,
    /// как и у настоящего 8080.
    pub fn reset(&mut self) {
        self.pc = 0;
        self.inte = false;
        self.ei_delay = false;
        self.halted = false;
    }

    // ----- регистровые пары и PSW -----

    /// Пара BC.
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    /// Пара DE.
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    /// Пара HL.
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    /// Записать пару BC.
    pub fn set_bc(&mut self, v: u16) {
        [self.b, self.c] = v.to_be_bytes();
    }
    /// Записать пару DE.
    pub fn set_de(&mut self, v: u16) {
        [self.d, self.e] = v.to_be_bytes();
    }
    /// Записать пару HL.
    pub fn set_hl(&mut self, v: u16) {
        [self.h, self.l] = v.to_be_bytes();
    }

    /// Регистр признаков в формате 8080: `S Z 0 AC 0 P 1 C`.
    pub fn flags(&self) -> u8 {
        let mut f = 0x02;
        if self.flag_s {
            f |= FLAG_S;
        }
        if self.flag_z {
            f |= FLAG_Z;
        }
        if self.flag_ac {
            f |= FLAG_AC;
        }
        if self.flag_p {
            f |= FLAG_P;
        }
        if self.flag_c {
            f |= FLAG_C;
        }
        f
    }

    /// Установить признаки из байта (биты 5, 3, 1 игнорируются).
    pub fn set_flags(&mut self, f: u8) {
        self.flag_s = f & FLAG_S != 0;
        self.flag_z = f & FLAG_Z != 0;
        self.flag_ac = f & FLAG_AC != 0;
        self.flag_p = f & FLAG_P != 0;
        self.flag_c = f & FLAG_C != 0;
    }

    /// Слово состояния процессора PSW = A·F.
    pub fn psw(&self) -> u16 {
        u16::from_be_bytes([self.a, self.flags()])
    }

    /// Записать PSW.
    pub fn set_psw(&mut self, v: u16) {
        let [a, f] = v.to_be_bytes();
        self.a = a;
        self.set_flags(f);
    }

    /// Можно ли сейчас принять прерывание (INTE = 1 и не идёт задержка после EI).
    pub fn interrupts_enabled(&self) -> bool {
        self.inte && !self.ei_delay
    }

    // ----- прерывания -----

    /// Запрос прерывания: если прерывания разрешены, процессор принимает по INTA
    /// код команды `opcode` и выполняет его (для ЮТ-88 это `RST 7` = `0FFH`).
    ///
    /// При приёме прерывания INTE сбрасывается, процессор выходит из HLT, PC не
    /// увеличивается при «выборке» кода. Возвращает число тактов цикла прерывания
    /// или `None`, если прерывание не принято (запрос нужно держать и повторить
    /// после следующей команды).
    ///
    /// Поддерживаются однобайтовые коды (RST n и т. п.); у многобайтовых команд
    /// операнды читались бы из памяти по PC.
    pub fn interrupt<B: Bus + ?Sized>(&mut self, bus: &mut B, opcode: u8) -> Option<u32> {
        if !self.interrupts_enabled() {
            return None;
        }
        self.inte = false;
        self.halted = false;
        let t = self.execute(bus, opcode);
        self.cycles += u64::from(t);
        Some(t)
    }

    // ----- выполнение -----

    /// Выполнить одну команду. Возвращает число тактов.
    ///
    /// В состоянии HLT команда не выполняется, возвращается [`HALT_STEP_CYCLES`].
    #[inline]
    pub fn step<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u32 {
        self.ei_delay = false;
        if self.halted {
            self.cycles += u64::from(HALT_STEP_CYCLES);
            return HALT_STEP_CYCLES;
        }
        let op = self.fetch(bus);
        let t = self.execute(bus, op);
        self.cycles += u64::from(t);
        t
    }

    #[inline(always)]
    fn fetch<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    #[inline(always)]
    fn fetch16<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch(bus);
        let hi = self.fetch(bus);
        u16::from_le_bytes([lo, hi])
    }

    #[inline(always)]
    fn read16<B: Bus + ?Sized>(bus: &mut B, addr: u16) -> u16 {
        let lo = bus.read(addr);
        let hi = bus.read(addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    #[inline(always)]
    fn write16<B: Bus + ?Sized>(bus: &mut B, addr: u16, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        bus.write(addr, lo);
        bus.write(addr.wrapping_add(1), hi);
    }

    // Обращения к стеку идут через `read_stack`/`write_stack`: у КР580ВМ80А в этих
    // машинных циклах в слове состояния установлен бит STACK (D2), по нему, например,
    // квазидиск ЮТ-88 перехватывает стек.

    #[inline(always)]
    fn push<B: Bus + ?Sized>(&mut self, bus: &mut B, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        self.sp = self.sp.wrapping_sub(1);
        bus.write_stack(self.sp, hi);
        self.sp = self.sp.wrapping_sub(1);
        bus.write_stack(self.sp, lo);
    }

    #[inline(always)]
    fn pop<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 {
        let lo = bus.read_stack(self.sp);
        let hi = bus.read_stack(self.sp.wrapping_add(1));
        self.sp = self.sp.wrapping_add(2);
        u16::from_le_bytes([lo, hi])
    }

    /// Регистр по 3-битному коду: 0 B, 1 C, 2 D, 3 E, 4 H, 5 L, 6 M, 7 A.
    #[inline(always)]
    fn get_reg<B: Bus + ?Sized>(&mut self, bus: &mut B, r: u8) -> u8 {
        match r {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => bus.read(self.hl()),
            _ => self.a,
        }
    }

    #[inline(always)]
    fn set_reg<B: Bus + ?Sized>(&mut self, bus: &mut B, r: u8, v: u8) {
        match r {
            0 => self.b = v,
            1 => self.c = v,
            2 => self.d = v,
            3 => self.e = v,
            4 => self.h = v,
            5 => self.l = v,
            6 => bus.write(self.hl(), v),
            _ => self.a = v,
        }
    }

    /// Регистровая пара по 2-битному коду (0 B, 1 D, 2 H, 3 SP).
    fn get_rp(&self, rp: u8) -> u16 {
        match rp {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.sp,
        }
    }

    fn set_rp(&mut self, rp: u8, v: u16) {
        match rp {
            0 => self.set_bc(v),
            1 => self.set_de(v),
            2 => self.set_hl(v),
            _ => self.sp = v,
        }
    }

    /// Условие по 3-битному коду: NZ Z NC C PO PE P M.
    fn cond(&self, cc: u8) -> bool {
        match cc {
            0 => !self.flag_z,
            1 => self.flag_z,
            2 => !self.flag_c,
            3 => self.flag_c,
            4 => !self.flag_p,
            5 => self.flag_p,
            6 => !self.flag_s,
            _ => self.flag_s,
        }
    }

    fn set_szp(&mut self, v: u8) {
        self.flag_s = v & 0x80 != 0;
        self.flag_z = v == 0;
        self.flag_p = parity(v);
    }

    // ----- арифметика и логика -----

    fn add(&mut self, v: u8, carry: bool) {
        let c = u16::from(carry);
        let res = u16::from(self.a) + u16::from(v) + c;
        self.flag_ac = (self.a & 0x0F) + (v & 0x0F) + c as u8 > 0x0F;
        self.flag_c = res > 0xFF;
        self.a = res as u8;
        self.set_szp(self.a);
    }

    /// Вычитание `A - v - borrow`; на 8080 выполняется как `A + !v + !borrow`,
    /// AC — перенос из бита 3 этого сложения, CY — инверсия переноса (заём).
    fn sub_result(&mut self, v: u8, borrow: bool) -> u8 {
        let nv = !v;
        let c = u16::from(!borrow);
        let res = u16::from(self.a) + u16::from(nv) + c;
        self.flag_ac = (self.a & 0x0F) + (nv & 0x0F) + c as u8 > 0x0F;
        self.flag_c = res <= 0xFF;
        let r = res as u8;
        self.set_szp(r);
        r
    }

    fn alu(&mut self, op: u8, v: u8) {
        match op {
            0 => self.add(v, false),
            1 => self.add(v, self.flag_c),
            2 => self.a = self.sub_result(v, false),
            3 => self.a = self.sub_result(v, self.flag_c),
            4 => {
                // ANA: AC = OR битов 3 операндов (особенность 8080).
                self.flag_ac = (self.a | v) & 0x08 != 0;
                self.a &= v;
                self.flag_c = false;
                self.set_szp(self.a);
            }
            5 => {
                self.a ^= v;
                self.flag_ac = false;
                self.flag_c = false;
                self.set_szp(self.a);
            }
            6 => {
                self.a |= v;
                self.flag_ac = false;
                self.flag_c = false;
                self.set_szp(self.a);
            }
            _ => {
                self.sub_result(v, false);
            }
        }
    }

    fn daa(&mut self) {
        let a = self.a;
        let lo = a & 0x0F;
        let hi = a >> 4;
        let mut corr = 0u8;
        let mut cy = self.flag_c;
        if self.flag_ac || lo > 9 {
            corr |= 0x06;
        }
        if cy || hi > 9 || (hi >= 9 && lo > 9) {
            corr |= 0x60;
            cy = true;
        }
        self.flag_ac = (a & 0x0F) + (corr & 0x0F) > 0x0F;
        self.a = a.wrapping_add(corr);
        self.flag_c = cy;
        self.set_szp(self.a);
    }

    /// Выполнить команду с кодом `op` (код уже выбран). Возвращает число тактов.
    fn execute<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) -> u32 {
        match op {
            // NOP и недокументированные NOP
            0x00 | 0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38 => 4,

            // LXI rp,d16
            0x01 | 0x11 | 0x21 | 0x31 => {
                let v = self.fetch16(bus);
                self.set_rp(op >> 4, v);
                10
            }
            // STAX B / STAX D
            0x02 | 0x12 => {
                let addr = self.get_rp(op >> 4);
                bus.write(addr, self.a);
                7
            }
            // LDAX B / LDAX D
            0x0A | 0x1A => {
                let addr = self.get_rp(op >> 4);
                self.a = bus.read(addr);
                7
            }
            // INX rp
            0x03 | 0x13 | 0x23 | 0x33 => {
                let rp = op >> 4;
                self.set_rp(rp, self.get_rp(rp).wrapping_add(1));
                5
            }
            // DCX rp
            0x0B | 0x1B | 0x2B | 0x3B => {
                let rp = op >> 4;
                self.set_rp(rp, self.get_rp(rp).wrapping_sub(1));
                5
            }
            // DAD rp
            0x09 | 0x19 | 0x29 | 0x39 => {
                let res = u32::from(self.hl()) + u32::from(self.get_rp(op >> 4));
                self.flag_c = res > 0xFFFF;
                self.set_hl(res as u16);
                10
            }
            // INR r
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
                let r = (op >> 3) & 7;
                let v = self.get_reg(bus, r).wrapping_add(1);
                self.flag_ac = v & 0x0F == 0;
                self.set_szp(v);
                self.set_reg(bus, r, v);
                if r == 6 { 10 } else { 5 }
            }
            // DCR r
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
                let r = (op >> 3) & 7;
                let v = self.get_reg(bus, r).wrapping_sub(1);
                self.flag_ac = v & 0x0F != 0x0F;
                self.set_szp(v);
                self.set_reg(bus, r, v);
                if r == 6 { 10 } else { 5 }
            }
            // MVI r,d8
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
                let r = (op >> 3) & 7;
                let v = self.fetch(bus);
                self.set_reg(bus, r, v);
                if r == 6 { 10 } else { 7 }
            }
            // RLC
            0x07 => {
                self.flag_c = self.a & 0x80 != 0;
                self.a = self.a.rotate_left(1);
                4
            }
            // RRC
            0x0F => {
                self.flag_c = self.a & 0x01 != 0;
                self.a = self.a.rotate_right(1);
                4
            }
            // RAL
            0x17 => {
                let c = self.flag_c;
                self.flag_c = self.a & 0x80 != 0;
                self.a = (self.a << 1) | u8::from(c);
                4
            }
            // RAR
            0x1F => {
                let c = self.flag_c;
                self.flag_c = self.a & 0x01 != 0;
                self.a = (self.a >> 1) | (u8::from(c) << 7);
                4
            }
            // SHLD a16
            0x22 => {
                let addr = self.fetch16(bus);
                Self::write16(bus, addr, self.hl());
                16
            }
            // LHLD a16
            0x2A => {
                let addr = self.fetch16(bus);
                let v = Self::read16(bus, addr);
                self.set_hl(v);
                16
            }
            // DAA
            0x27 => {
                self.daa();
                4
            }
            // CMA
            0x2F => {
                self.a = !self.a;
                4
            }
            // STA a16
            0x32 => {
                let addr = self.fetch16(bus);
                bus.write(addr, self.a);
                13
            }
            // LDA a16
            0x3A => {
                let addr = self.fetch16(bus);
                self.a = bus.read(addr);
                13
            }
            // STC
            0x37 => {
                self.flag_c = true;
                4
            }
            // CMC
            0x3F => {
                self.flag_c = !self.flag_c;
                4
            }
            // HLT
            0x76 => {
                self.halted = true;
                7
            }
            // MOV r1,r2
            0x40..=0x7F => {
                let dst = (op >> 3) & 7;
                let src = op & 7;
                let v = self.get_reg(bus, src);
                self.set_reg(bus, dst, v);
                if dst == 6 || src == 6 { 7 } else { 5 }
            }
            // ADD/ADC/SUB/SBB/ANA/XRA/ORA/CMP r
            0x80..=0xBF => {
                let src = op & 7;
                let v = self.get_reg(bus, src);
                self.alu((op >> 3) & 7, v);
                if src == 6 { 7 } else { 4 }
            }
            // ADI/ACI/SUI/SBI/ANI/XRI/ORI/CPI d8
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                let v = self.fetch(bus);
                self.alu((op >> 3) & 7, v);
                7
            }
            // Rcc
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
                if self.cond((op >> 3) & 7) {
                    self.pc = self.pop(bus);
                    11
                } else {
                    5
                }
            }
            // RET (и недокументированный D9)
            0xC9 | 0xD9 => {
                self.pc = self.pop(bus);
                10
            }
            // POP rp
            0xC1 | 0xD1 | 0xE1 => {
                let v = self.pop(bus);
                self.set_rp((op >> 4) & 3, v);
                10
            }
            // POP PSW
            0xF1 => {
                let v = self.pop(bus);
                self.set_psw(v);
                10
            }
            // PUSH rp
            0xC5 | 0xD5 | 0xE5 => {
                let v = self.get_rp((op >> 4) & 3);
                self.push(bus, v);
                11
            }
            // PUSH PSW
            0xF5 => {
                let v = self.psw();
                self.push(bus, v);
                11
            }
            // Jcc a16
            0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => {
                let addr = self.fetch16(bus);
                if self.cond((op >> 3) & 7) {
                    self.pc = addr;
                }
                10
            }
            // JMP (и недокументированный CB)
            0xC3 | 0xCB => {
                self.pc = self.fetch16(bus);
                10
            }
            // Ccc a16
            0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => {
                let addr = self.fetch16(bus);
                if self.cond((op >> 3) & 7) {
                    self.push(bus, self.pc);
                    self.pc = addr;
                    17
                } else {
                    11
                }
            }
            // CALL (и недокументированные DD, ED, FD)
            0xCD | 0xDD | 0xED | 0xFD => {
                let addr = self.fetch16(bus);
                self.push(bus, self.pc);
                self.pc = addr;
                17
            }
            // RST n
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                self.push(bus, self.pc);
                self.pc = u16::from(op & 0x38);
                11
            }
            // OUT d8
            0xD3 => {
                let port = self.fetch(bus);
                bus.output(port, self.a);
                10
            }
            // IN d8
            0xDB => {
                let port = self.fetch(bus);
                self.a = bus.input(port);
                10
            }
            // XTHL
            0xE3 => {
                // Порядок циклов, как у КР580ВМ80А: чтение SP, SP+1, запись SP+1 (H), SP (L).
                let sp1 = self.sp.wrapping_add(1);
                let lo = bus.read_stack(self.sp);
                let hi = bus.read_stack(sp1);
                bus.write_stack(sp1, self.h);
                bus.write_stack(self.sp, self.l);
                self.set_hl(u16::from_le_bytes([lo, hi]));
                18
            }
            // PCHL
            0xE9 => {
                self.pc = self.hl();
                5
            }
            // XCHG
            0xEB => {
                std::mem::swap(&mut self.h, &mut self.d);
                std::mem::swap(&mut self.l, &mut self.e);
                4
            }
            // DI
            0xF3 => {
                self.inte = false;
                4
            }
            // EI
            0xFB => {
                self.inte = true;
                self.ei_delay = true;
                4
            }
            // SPHL
            0xF9 => {
                self.sp = self.hl();
                5
            }
        }
    }
}
