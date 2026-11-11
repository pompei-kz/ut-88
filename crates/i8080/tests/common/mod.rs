//! Общее для тестов: простая шина с 64 Кбайт ОЗУ и запуск CP/M-программ (.COM)
//! с заглушкой BDOS.

#![allow(dead_code)]

use i8080::{Bus, Cpu};

/// 64 Кбайт ОЗУ и журнал операций с портами.
pub struct RamBus {
    pub mem: Vec<u8>,
    pub outputs: Vec<(u8, u8)>,
    pub input_value: u8,
}

impl RamBus {
    pub fn new() -> Self {
        Self {
            mem: vec![0; 0x10000],
            outputs: Vec::new(),
            input_value: 0,
        }
    }

    /// Положить байты в память с адреса `addr`.
    pub fn load(&mut self, addr: u16, bytes: &[u8]) {
        let a = usize::from(addr);
        self.mem[a..a + bytes.len()].copy_from_slice(bytes);
    }
}

impl Bus for RamBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[usize::from(addr)]
    }
    fn write(&mut self, addr: u16, v: u8) {
        self.mem[usize::from(addr)] = v;
    }
    fn input(&mut self, _port: u8) -> u8 {
        self.input_value
    }
    fn output(&mut self, port: u8, v: u8) {
        self.outputs.push((port, v));
    }
}

/// Результат прогона CP/M-программы.
pub struct ComRun {
    /// Всё, что программа вывела через BDOS.
    pub output: String,
    /// Число выполненных тактов.
    pub cycles: u64,
}

/// Адрес «вершины» BDOS: на него указывает JMP по адресу 0005H, там стоит RET.
const BDOS_TOP: u16 = 0xFE00;

/// Запустить CP/M-программу `.COM` с адреса 0100H.
///
/// Заглушка BDOS: вызов `CALL 0005H` перехватывается; функция 2 выводит символ из E,
/// функция 9 — строку с адреса DE до `$`. Переход на 0000H завершает программу.
/// `max_cycles` — защита от зацикливания.
pub fn run_com(program: &[u8], max_cycles: u64) -> ComRun {
    let mut bus = RamBus::new();
    bus.load(0x0100, program);
    // 0000H: HLT (на случай, если сюда придут не прыжком); 0005H: JMP BDOS_TOP.
    bus.mem[0x0000] = 0x76;
    bus.load(0x0005, &[0xC3, BDOS_TOP as u8, (BDOS_TOP >> 8) as u8]);
    bus.mem[usize::from(BDOS_TOP)] = 0xC9; // RET

    let mut cpu = Cpu::new();
    cpu.pc = 0x0100;
    cpu.sp = BDOS_TOP;
    let mut out = String::new();

    loop {
        match cpu.pc {
            0x0000 => break,
            0x0005 => match cpu.c {
                2 => out.push(char::from(cpu.e)),
                9 => {
                    let mut addr = cpu.de();
                    loop {
                        let ch = bus.mem[usize::from(addr)];
                        if ch == b'$' {
                            break;
                        }
                        out.push(char::from(ch));
                        addr = addr.wrapping_add(1);
                    }
                }
                f => panic!("неподдерживаемая функция BDOS {f}"),
            },
            _ => {}
        }
        assert!(
            !cpu.halted,
            "процессор остановился (HLT) по адресу {:04X}",
            cpu.pc
        );
        cpu.step(&mut bus);
        assert!(
            cpu.cycles < max_cycles,
            "превышен предел тактов; вывод:\n{out}"
        );
    }
    ComRun {
        output: out,
        cycles: cpu.cycles,
    }
}
