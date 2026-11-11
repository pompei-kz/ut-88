//! Отладчик: точки останова, пошаговое выполнение, дизассемблер и дамп памяти.
//!
//! Модель не зависит от терминала. Выполнение идёт по одной команде
//! ([`run_until`]): перед каждой командой проверяются точки останова по адресу и по
//! обращению к порту (команды IN/OUT — единственный способ обратиться к порту у
//! КР580ВМ80А, поэтому порт виден по коду команды до её выполнения).

use std::collections::BTreeSet;

use ut88_core::{CPU_HZ, Machine};

use crate::input::InputQueue;

/// Причина останова.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Точка останова по адресу.
    Breakpoint(u16),
    /// Обращение к порту: `IN` (`out == false`) или `OUT` в команде по адресу `pc`.
    Port { port: u8, out: bool, pc: u16 },
    /// Достигнут адрес «запуска до адреса» или «шага с обходом».
    Reached(u16),
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stop::Breakpoint(a) => write!(f, "точка останова {a:04X}"),
            Stop::Port { port, out, pc } => {
                let op = if *out { "OUT" } else { "IN" };
                write!(f, "{op} {port:02X}H по адресу {pc:04X}")
            }
            Stop::Reached(a) => write!(f, "достигнут адрес {a:04X}"),
        }
    }
}

/// Правка памяти в дампе: введён старший полубайт.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    /// Введённый старший полубайт.
    pub high: Option<u8>,
}

/// Состояние отладчика.
#[derive(Debug, Clone)]
pub struct Debugger {
    /// Точки останова по адресу.
    pub breakpoints: BTreeSet<u16>,
    /// Точки останова по порту.
    pub port_breaks: BTreeSet<u8>,
    /// Временная точка (запуск до адреса, шаг с обходом).
    pub temp: Option<u16>,
    /// Курсор дампа памяти.
    pub mem_cursor: u16,
    /// Начало дизассемблера (`None` — вокруг PC).
    pub disasm_at: Option<u16>,
    /// Идёт правка памяти в дампе.
    pub edit: Option<Edit>,
    /// Отметка счётчика тактов (для разности).
    pub mark: u64,
    /// Последний останов.
    pub last_stop: Option<Stop>,
}

impl Default for Debugger {
    fn default() -> Self {
        Self {
            breakpoints: BTreeSet::new(),
            port_breaks: BTreeSet::new(),
            temp: None,
            mem_cursor: 0xC000,
            disasm_at: None,
            edit: None,
            mark: 0,
            last_stop: None,
        }
    }
}

/// Порт, к которому обратится команда по адресу `pc`: `(порт, это OUT)`.
pub fn port_access(m: &Machine, pc: u16) -> Option<(u8, bool)> {
    match m.peek(pc) {
        0xDB => Some((m.peek(pc.wrapping_add(1)), false)),
        0xD3 => Some((m.peek(pc.wrapping_add(1)), true)),
        _ => None,
    }
}

/// Команда по адресу — вызов подпрограммы (CALL, Ccc, RST, недокументированные *CALL).
pub fn is_call(op: u8) -> bool {
    matches!(op, 0xCD | 0xDD | 0xED | 0xFD) || op & 0xC7 == 0xC4 || op & 0xC7 == 0xC7
}

/// Длина и текст команды по адресу `addr`.
pub fn disasm_at(m: &Machine, addr: u16) -> (String, usize) {
    let bytes = [
        m.peek(addr),
        m.peek(addr.wrapping_add(1)),
        m.peek(addr.wrapping_add(2)),
    ];
    i8080::disasm(&bytes, addr)
}

/// Строка дизассемблера.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Адрес.
    pub addr: u16,
    /// Байты команды.
    pub bytes: Vec<u8>,
    /// Мнемоника.
    pub text: String,
}

/// Дизассемблировать `count` команд с адреса `addr`.
pub fn disasm_from(m: &Machine, mut addr: u16, count: usize) -> Vec<Line> {
    (0..count)
        .map(|_| {
            let (text, len) = disasm_at(m, addr);
            let bytes = (0..len)
                .map(|i| m.peek(addr.wrapping_add(i as u16)))
                .collect();
            let l = Line { addr, bytes, text };
            addr = addr.wrapping_add(len as u16);
            l
        })
        .collect()
}

/// Адрес, с которого `before` команд подряд ровно приходят к `pc` (если найдётся);
/// иначе начало ближайшей «сходящейся» цепочки, иначе `pc`.
pub fn back_start(m: &Machine, pc: u16, before: usize) -> u16 {
    // Перебираем начала от дальнего к ближнему; берём цепочку, приходящую в pc
    // с наибольшим числом команд, но не больше `before`.
    let mut best = (0usize, pc);
    for back in (1..=(before * 3) as u16).rev() {
        let start = pc.wrapping_sub(back);
        let mut a = start;
        let mut n = 0;
        while a != pc && pc.wrapping_sub(a) <= back && n <= before * 3 {
            a = a.wrapping_add(disasm_at(m, a).1 as u16);
            n += 1;
        }
        if a == pc && n <= before && n > best.0 {
            best = (n, start);
            if n == before {
                break;
            }
        }
    }
    best.1
}

impl Debugger {
    /// Новый отладчик.
    pub fn new() -> Self {
        Self::default()
    }

    /// Есть ли что проверять перед командами.
    pub fn active(&self) -> bool {
        self.temp.is_some() || !self.breakpoints.is_empty() || !self.port_breaks.is_empty()
    }

    /// Проверить, надо ли остановиться перед командой по текущему PC.
    pub fn check(&mut self, m: &Machine) -> Option<Stop> {
        let pc = m.cpu.pc;
        if self.temp == Some(pc) {
            self.temp = None;
            return Some(Stop::Reached(pc));
        }
        if self.breakpoints.contains(&pc) {
            return Some(Stop::Breakpoint(pc));
        }
        if !self.port_breaks.is_empty()
            && let Some((port, out)) = port_access(m, pc)
            && self.port_breaks.contains(&port)
        {
            return Some(Stop::Port { port, out, pc });
        }
        None
    }

    /// Переключить точку останова по адресу. Возвращает `true`, если точка поставлена.
    pub fn toggle_breakpoint(&mut self, addr: u16) -> bool {
        if !self.breakpoints.remove(&addr) {
            self.breakpoints.insert(addr);
            true
        } else {
            false
        }
    }

    /// Переключить точку останова по порту.
    pub fn toggle_port(&mut self, port: u8) -> bool {
        if !self.port_breaks.remove(&port) {
            self.port_breaks.insert(port);
            true
        } else {
            false
        }
    }

    /// Шаг на одну команду.
    pub fn step(&mut self, m: &mut Machine, input: &mut InputQueue) {
        note_pad_read(m, input);
        m.step();
        input.tick(m);
    }

    /// Шаг с обходом CALL: для вызова подпрограммы ставит временную точку за командой и
    /// возвращает `true` (машину надо запустить); иначе делает шаг и возвращает `false`.
    pub fn step_over(&mut self, m: &mut Machine, input: &mut InputQueue) -> bool {
        let pc = m.cpu.pc;
        if is_call(m.peek(pc)) {
            let len = disasm_at(m, pc).1 as u16;
            self.temp = Some(pc.wrapping_add(len));
            true
        } else {
            self.step(m, input);
            false
        }
    }

    /// Запуск до адреса: ставит временную точку.
    pub fn run_to(&mut self, addr: u16) {
        self.temp = Some(addr);
    }

    /// Ввести шестнадцатеричную цифру при правке памяти. Возвращает `true`, если байт
    /// записан (курсор сдвинут).
    pub fn edit_digit(&mut self, m: &mut Machine, d: u8) -> bool {
        let e = self.edit.get_or_insert(Edit { high: None });
        match e.high.take() {
            None => {
                e.high = Some(d & 15);
                false
            }
            Some(h) => {
                m.poke(self.mem_cursor, (h << 4) | (d & 15));
                self.mem_cursor = self.mem_cursor.wrapping_add(1);
                true
            }
        }
    }
}

/// Учесть чтение порта кнопок модуля 1 командой по текущему PC.
fn note_pad_read(m: &Machine, input: &mut InputQueue) {
    if let Some((port, false)) = port_access(m, m.cpu.pc)
        && (0xA0..=0xAF).contains(&port)
        && port & 1 == 0
    {
        input.on_pad_read(m.cycles());
    }
}

/// Период обновления очереди нажатий, такты (≈ 1 мс).
const INPUT_TICK: u64 = (CPU_HZ / 1000.0) as u64;

/// Выполнять команды, пока счётчик тактов меньше `target` или до останова.
///
/// `skip_first` — не проверять точки перед первой командой (продолжение после
/// останова).
pub fn run_until(
    m: &mut Machine,
    target: u64,
    dbg: &mut Debugger,
    input: &mut InputQueue,
    mut skip_first: bool,
) -> Option<Stop> {
    let mut next_tick = m.cycles() + INPUT_TICK;
    input.tick(m);
    while m.cycles() < target {
        if dbg.active()
            && !skip_first
            && let Some(s) = dbg.check(m)
        {
            dbg.last_stop = Some(s);
            return Some(s);
        }
        skip_first = false;
        note_pad_read(m, input);
        m.step();
        if m.cycles() >= next_tick {
            input.tick(m);
            next_tick = m.cycles() + INPUT_TICK;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ut88_core::Config;

    fn machine_with(code: &[u8]) -> Machine {
        let mut m = Machine::new(Config::minimal());
        m.set_clock_interrupts(false);
        m.load_bin(0xC000, code);
        m.jump(0xC000);
        m
    }

    #[test]
    fn breakpoint_and_step_over() {
        // C000: CALL C010; C003: NOP; C004: JMP C004 / C010: MVI A,5; RET
        let mut code = vec![0xCD, 0x10, 0xC0, 0x00, 0xC3, 0x04, 0xC0];
        code.resize(0x10, 0);
        code.extend([0x3E, 0x05, 0xC9]);
        let mut m = machine_with(&code);
        m.cpu.sp = 0xC3F0;
        let mut d = Debugger::new();
        let mut inp = InputQueue::new();
        assert!(d.toggle_breakpoint(0xC010));
        let t = m.cycles() + 1000;
        let s = run_until(&mut m, t, &mut d, &mut inp, false);
        assert_eq!(s, Some(Stop::Breakpoint(0xC010)));
        assert_eq!(m.cpu.pc, 0xC010);
        // Продолжение с той же точки не останавливается сразу.
        d.step(&mut m, &mut inp);
        assert_eq!(m.cpu.pc, 0xC012);
        assert_eq!(m.cpu.a, 5);
        d.step(&mut m, &mut inp);
        assert_eq!(m.cpu.pc, 0xC003);
        // Шаг с обходом CALL.
        m.jump(0xC000);
        m.cpu.a = 0;
        assert!(!d.toggle_breakpoint(0xC010));
        assert!(d.step_over(&mut m, &mut inp));
        let t = m.cycles() + 1000;
        let s = run_until(&mut m, t, &mut d, &mut inp, true);
        assert_eq!(s, Some(Stop::Reached(0xC003)));
        assert_eq!(m.cpu.a, 5);
        assert_eq!(d.temp, None);
        // Шаг по обычной команде.
        assert!(!d.step_over(&mut m, &mut inp));
        assert_eq!(m.cpu.pc, 0xC004);
    }

    #[test]
    fn port_breakpoint() {
        // C000: MVI A,1; OUT A1; IN A0; JMP C000
        let code = [0x3E, 0x01, 0xD3, 0xA1, 0xDB, 0xA0, 0xC3, 0x00, 0xC0];
        let mut m = machine_with(&code);
        let mut d = Debugger::new();
        let mut inp = InputQueue::new();
        d.toggle_port(0xA0);
        let t = m.cycles() + 1000;
        let s = run_until(&mut m, t, &mut d, &mut inp, false);
        assert_eq!(
            s,
            Some(Stop::Port {
                port: 0xA0,
                out: false,
                pc: 0xC004
            })
        );
        d.toggle_port(0xA0);
        d.toggle_port(0xA1);
        let t = m.cycles() + 1000;
        let s = run_until(&mut m, t, &mut d, &mut inp, true);
        assert_eq!(
            s,
            Some(Stop::Port {
                port: 0xA1,
                out: true,
                pc: 0xC002
            })
        );
    }

    #[test]
    fn run_to_and_counts() {
        let code = [0x00, 0x00, 0x00, 0xC3, 0x00, 0xC0];
        let mut m = machine_with(&code);
        let mut d = Debugger::new();
        let mut inp = InputQueue::new();
        d.run_to(0xC003);
        let c0 = m.cycles();
        let s = run_until(&mut m, u64::MAX, &mut d, &mut inp, false);
        assert_eq!(s, Some(Stop::Reached(0xC003)));
        assert_eq!(m.cycles() - c0, 12, "три NOP по 4 такта");
        // Без точек работает до заданного такта.
        let t = m.cycles() + 1000;
        assert_eq!(run_until(&mut m, t, &mut d, &mut inp, false), None);
        assert!(m.cycles() >= t);
    }

    #[test]
    fn disasm_window() {
        let m = Machine::new(Config::minimal());
        let lines = disasm_from(&m, 0, 3);
        assert_eq!(lines[0].addr, 0);
        assert_eq!(lines[0].bytes[0], 0x31);
        assert!(lines[0].text.starts_with("LXI SP"), "{}", lines[0].text);
        // Назад от адреса третьей команды на две команды — к нулю.
        let third = lines[2].addr;
        assert_eq!(back_start(&m, third, 2), 0);
        assert_eq!(back_start(&m, 0x0000, 0), 0);
    }

    #[test]
    fn edit_memory() {
        let mut m = machine_with(&[]);
        let mut d = Debugger::new();
        d.mem_cursor = 0xC100;
        assert!(!d.edit_digit(&mut m, 0xA));
        assert!(d.edit_digit(&mut m, 0x5));
        assert_eq!(m.peek(0xC100), 0xA5);
        assert_eq!(d.mem_cursor, 0xC101);
    }

    #[test]
    fn calls() {
        assert!(is_call(0xCD));
        assert!(is_call(0xC4));
        assert!(is_call(0xFF));
        assert!(is_call(0xC7));
        assert!(!is_call(0xC3));
        assert!(!is_call(0xC9));
    }
}
