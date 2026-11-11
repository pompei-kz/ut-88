//! Машина ЮТ-88: процессор + шина + часы; тактирование, клавиши, магнитофон, снимки.

use std::io;
use std::path::Path;

use i8080::Cpu;
use serde::{Deserialize, Serialize};

use crate::bus::{Bus, BusState, Expansion};
use crate::config::Config;
use crate::keys::{KbKey, PadKey};
use crate::screen::{self, Screen};
use crate::tape::{
    self, CPU_DIV, MASTER_HZ, TapeFormat, TapePlayer, TapeRecorder, TapeSignal, WAV_RATE,
};

/// Тактовая частота процессора, Гц (16 МГц / 9).
pub const CPU_HZ: f64 = MASTER_HZ as f64 / CPU_DIV as f64;

/// Период кадровой развёртки дисплейного модуля: 20 160 мкс = 35 840 тактов процессора.
pub const FRAME_CYCLES: u64 = 20_160 * 16 / CPU_DIV;

/// Код, который процессор получает по INTA (никто не отвечает — на шине FFH = `RST 7`).
const INTA_OPCODE: u8 = 0xFF;

/// Зеркало состояния процессора для снимков (крейт `i8080` не зависит от serde).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuState {
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    /// Регистр признаков в формате PSW.
    pub flags: u8,
    pub inte: bool,
    pub halted: bool,
    pub ei_delay: bool,
    pub cycles: u64,
}

impl From<&Cpu> for CpuState {
    fn from(c: &Cpu) -> Self {
        Self {
            a: c.a,
            b: c.b,
            c: c.c,
            d: c.d,
            e: c.e,
            h: c.h,
            l: c.l,
            sp: c.sp,
            pc: c.pc,
            flags: c.flags(),
            inte: c.inte,
            halted: c.halted,
            ei_delay: c.ei_delay,
            cycles: c.cycles,
        }
    }
}

impl From<&CpuState> for Cpu {
    fn from(s: &CpuState) -> Self {
        let mut c = Cpu::new();
        c.a = s.a;
        c.b = s.b;
        c.c = s.c;
        c.d = s.d;
        c.e = s.e;
        c.h = s.h;
        c.l = s.l;
        c.sp = s.sp;
        c.pc = s.pc;
        c.set_flags(s.flags);
        c.inte = s.inte;
        c.halted = s.halted;
        c.ei_delay = s.ei_delay;
        c.cycles = s.cycles;
        c
    }
}

/// Часы: датчик секундных импульсов К176ИЕ5 и триггер запроса прерывания.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    /// Такт 16 МГц следующего секундного импульса.
    pub next_tick: u64,
    /// Запрос прерывания ждёт приёма процессором.
    pub irq_pending: bool,
}

/// Снимок полного состояния машины.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Версия формата.
    pub version: u32,
    /// Процессор.
    pub cpu: CpuState,
    /// Память, устройства, магнитофон, конфигурация.
    pub bus: BusState,
    /// Часы.
    pub clock: ClockState,
    /// Состояния устройств расширения: `(имя, данные)`.
    pub expansions: Vec<(String, Vec<u8>)>,
}

const SNAPSHOT_VERSION: u32 = 1;
const SNAPSHOT_MAGIC: &[u8; 8] = b"UT88SNAP";

/// Ошибка снимка.
#[derive(Debug)]
pub enum SnapshotError {
    /// Ввод-вывод.
    Io(io::Error),
    /// Файл не является снимком или испорчен.
    Format(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::Io(e) => write!(f, "ошибка ввода-вывода: {e}"),
            SnapshotError::Format(s) => write!(f, "неверный снимок: {s}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl From<io::Error> for SnapshotError {
    fn from(e: io::Error) -> Self {
        SnapshotError::Io(e)
    }
}

/// Машина ЮТ-88.
pub struct Machine {
    /// Процессор.
    pub cpu: Cpu,
    /// Шина со всеми модулями.
    pub bus: Bus,
    /// Часы.
    pub clock: ClockState,
}

impl Machine {
    /// Включить питание: собрать машину по конфигурации, процессор в состоянии сброса.
    pub fn new(config: Config) -> Self {
        let bus = Bus::new(config);
        let mut m = Self {
            cpu: Cpu::new(),
            bus,
            clock: ClockState {
                next_tick: MASTER_HZ,
                irq_pending: false,
            },
        };
        m.install_boot_stub();
        m
    }

    fn install_boot_stub(&mut self) {
        let c = &self.bus.st.config;
        if c.boot_stub && !c.rom1_enabled && c.calculator_rom.is_none() && c.ram64 {
            // DI; JMP F800H и JMP FFC1H по адресу 0038H (doc/25).
            for (i, b) in [0xF3, 0xC3, 0x00, 0xF8].into_iter().enumerate() {
                self.bus.poke(i as u16, b);
            }
            for (i, b) in [0xC3, 0xC1, 0xFF].into_iter().enumerate() {
                self.bus.poke(0x38 + i as u16, b);
            }
        }
    }

    /// Конфигурация.
    pub fn config(&self) -> &Config {
        &self.bus.st.config
    }

    /// Тумблер запроса прерывания от часов (doc/25: при работе с магнитофоном через
    /// МОНИТОР-«0» прерывания рекомендуется отключать). Выключение сбрасывает и ждущий
    /// запрос.
    pub fn set_clock_interrupts(&mut self, on: bool) {
        self.bus.st.config.clock_interrupts = on;
        if !on {
            self.clock.irq_pending = false;
        }
    }

    /// Подключить устройство расширения (например, квазидиск).
    pub fn add_expansion(&mut self, dev: Box<dyn Expansion>) {
        self.bus.expansions.push(dev);
    }

    // ----- время -----

    /// Тактов процессора с включения.
    pub fn cycles(&self) -> u64 {
        self.cpu.cycles
    }

    /// Текущее время в тактах кварца 16 МГц.
    pub fn master_ticks(&self) -> u64 {
        self.cpu.cycles * CPU_DIV
    }

    /// Эмулированное время с включения, с.
    pub fn time(&self) -> f64 {
        self.master_ticks() as f64 / MASTER_HZ as f64
    }

    /// Кнопка «УСТ»: сброс процессора и ППА; память и часы не меняются.
    pub fn reset(&mut self) {
        self.cpu.reset();
        if let Some(d) = &mut self.bus.st.display {
            d.reset();
        }
        for e in &mut self.bus.expansions {
            e.reset();
        }
    }

    /// Выполнить одну команду (или принять прерывание). Возвращает такты.
    pub fn step(&mut self) -> u32 {
        self.bus.st.now = self.master_ticks();
        let t = if self.clock.irq_pending && self.cpu.interrupts_enabled() {
            self.set_stack_window(true);
            let t = self.cpu.interrupt(&mut self.bus, INTA_OPCODE).unwrap_or(0);
            self.clock.irq_pending = false;
            t
        } else {
            self.set_stack_window(false);
            self.cpu.step(&mut self.bus)
        };
        self.bus.stack_window = None;
        // Секундный импульс К176ИЕ5 взводит триггер запроса прерывания.
        while self.master_ticks() >= self.clock.next_tick {
            self.clock.next_tick += MASTER_HZ;
            if self.bus.st.config.clock_interrupts {
                self.clock.irq_pending = true;
            }
        }
        t
    }

    fn set_stack_window(&mut self, interrupt: bool) {
        if self.bus.expansions.is_empty() {
            return;
        }
        let stack = interrupt || is_stack_opcode(self.bus.peek(self.cpu.pc));
        self.bus.stack_window = stack.then_some(self.cpu.sp);
    }

    /// Выполнять команды, пока не пройдёт не меньше `n` тактов процессора. Возвращает
    /// число выполненных тактов.
    pub fn run_cycles(&mut self, n: u64) -> u64 {
        let start = self.cpu.cycles;
        let end = start + n;
        while self.cpu.cycles < end {
            self.step();
        }
        self.cpu.cycles - start
    }

    /// Выполнить один кадр дисплея (20,16 мс = 35 840 тактов).
    pub fn run_frame(&mut self) -> u64 {
        self.run_cycles(FRAME_CYCLES)
    }

    /// Выполнить `secs` секунд эмулированного времени.
    pub fn run_seconds(&mut self, secs: f64) -> u64 {
        self.run_cycles((secs * CPU_HZ) as u64)
    }

    // ----- память -----

    /// Прочитать байт памяти без побочных эффектов (как видит процессор, без устройств
    /// расширения).
    pub fn peek(&self, addr: u16) -> u8 {
        self.bus.peek(addr)
    }

    /// Записать байт в память (ПЗУ не меняется).
    pub fn poke(&mut self, addr: u16, v: u8) {
        self.bus.poke(addr, v);
    }

    /// Загрузить данные в память с адреса `addr`.
    pub fn load_bin(&mut self, addr: u16, data: &[u8]) {
        for (i, &b) in data.iter().enumerate() {
            self.bus.poke(addr.wrapping_add(i as u16), b);
        }
    }

    /// Загрузить файл `.bin` в память с адреса `addr`. Возвращает длину.
    pub fn load_bin_file(&mut self, path: impl AsRef<Path>, addr: u16) -> io::Result<usize> {
        let data = std::fs::read(path)?;
        self.load_bin(addr, &data);
        Ok(data.len())
    }

    /// Прочитать диапазон памяти `start..=end`.
    pub fn save_bin(&self, start: u16, end: u16) -> Vec<u8> {
        let len = usize::from(end.wrapping_sub(start)) + 1;
        (0..len)
            .map(|i| self.bus.peek(start.wrapping_add(i as u16)))
            .collect()
    }

    /// Сохранить диапазон памяти `start..=end` в файл.
    pub fn save_bin_file(&self, path: impl AsRef<Path>, start: u16, end: u16) -> io::Result<()> {
        std::fs::write(path, self.save_bin(start, end))
    }

    /// Передать управление на `addr` (как директива пуска).
    pub fn jump(&mut self, addr: u16) {
        self.cpu.pc = addr;
        self.cpu.halted = false;
    }

    // ----- модуль 1: индикаторы и кнопки -----

    /// Байты индикаторов: `[9000H (A), 9001H (L), 9002H (H)]`.
    pub fn indicator_bytes(&self) -> [u8; 3] {
        let i = self.bus.st.cpu_module.indicators;
        [i[0], i[1], i[2]]
    }

    /// Шесть шестнадцатеричных цифр индикатора слева направо.
    pub fn indicator_digits(&self) -> [u8; 6] {
        self.bus.st.cpu_module.digits()
    }

    /// Сегменты шести разрядов слева направо (через знакогенератор К155РЕ3: бит 0 — a,
    /// …, бит 6 — g).
    pub fn indicator_segments(&self) -> [u8; 6] {
        self.bus.st.cpu_module.segments()
    }

    /// Нажать кнопку модуля 1. «УСТ» — сброс в момент нажатия.
    pub fn pad_down(&mut self, key: PadKey) {
        if key == PadKey::Reset {
            self.reset();
            return;
        }
        let p = &mut self.bus.st.cpu_module.pressed;
        if !p.contains(&key) {
            p.push(key);
        }
    }

    /// Отпустить кнопку модуля 1.
    pub fn pad_up(&mut self, key: PadKey) {
        self.bus.st.cpu_module.pressed.retain(|&k| k != key);
    }

    // ----- дисплейный модуль -----

    /// Нажать клавишу полной клавиатуры (без дисплейного модуля — ничего).
    pub fn kb_down(&mut self, key: KbKey) {
        if let Some(d) = &mut self.bus.st.display
            && !d.pressed.contains(&key)
        {
            d.pressed.push(key);
        }
    }

    /// Отпустить клавишу полной клавиатуры.
    pub fn kb_up(&mut self, key: KbKey) {
        if let Some(d) = &mut self.bus.st.display {
            d.pressed.retain(|&k| k != key);
        }
    }

    /// Отпустить все клавиши (обеих клавиатур).
    pub fn release_all_keys(&mut self) {
        self.bus.st.cpu_module.pressed.clear();
        if let Some(d) = &mut self.bus.st.display {
            d.pressed.clear();
        }
    }

    /// Текстовый экран (`None` без дисплейного модуля).
    pub fn screen(&self) -> Option<Screen> {
        self.bus
            .st
            .display
            .as_ref()
            .map(|d| Screen::from_vram(&d.vram))
    }

    /// Нарисовать экран в буфер RGBA [`screen::WIDTH`] × [`screen::HEIGHT`]. Возвращает
    /// `false` без дисплейного модуля.
    pub fn render_screen(&self, rgba: &mut [u8], fg: [u8; 4], bg: [u8; 4]) -> bool {
        match &self.bus.st.display {
            Some(d) => {
                screen::render(&d.vram, &d.chargen, rgba, fg, bg);
                true
            }
            None => false,
        }
    }

    // ----- магнитофон -----

    /// Текущий уровень выхода на магнитофон (бит 0 порта A1H).
    pub fn tape_out_level(&self) -> bool {
        self.bus.st.cpu_module.tape_out
    }

    /// Включить/выключить сбор перепадов выхода для звука.
    pub fn set_sound_capture(&mut self, on: bool) {
        self.bus.sound_capture = on;
        if !on {
            self.bus.sound_edges.clear();
        }
    }

    /// Забрать накопленные перепады выхода магнитофона `(такт 16 МГц, уровень)`.
    pub fn drain_sound_edges(&mut self) -> Vec<(u64, bool)> {
        std::mem::take(&mut self.bus.sound_edges)
    }

    /// Начать запись выхода магнитофона.
    pub fn tape_record_start(&mut self) {
        self.bus.st.recorder = Some(TapeRecorder {
            start_tick: self.master_ticks(),
            start_level: self.tape_out_level(),
            edges: Vec::new(),
        });
    }

    /// Идёт запись.
    pub fn tape_recording(&self) -> bool {
        self.bus.st.recorder.is_some()
    }

    /// Остановить запись и получить сигнал (44 100 Гц).
    pub fn tape_record_stop(&mut self) -> Option<TapeSignal> {
        let now = self.master_ticks();
        self.bus
            .st
            .recorder
            .take()
            .map(|r| r.to_signal(now, WAV_RATE))
    }

    /// Вставить «кассету» и начать воспроизведение с текущего момента.
    pub fn tape_play(&mut self, signal: TapeSignal) {
        self.bus.st.player = Some(TapePlayer {
            signal,
            start_tick: self.master_ticks(),
            threshold: 0,
            invert: false,
        });
    }

    /// Воспроизвести WAV-файл.
    pub fn tape_play_wav(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let s = TapeSignal::load_wav(path)?;
        self.tape_play(s);
        Ok(())
    }

    /// Порог и инверсия входа (для записей с постоянной составляющей).
    pub fn tape_set_input(&mut self, threshold: i16, invert: bool) {
        if let Some(p) = &mut self.bus.st.player {
            p.threshold = threshold;
            p.invert = invert;
        }
    }

    /// Вынуть кассету.
    pub fn tape_stop(&mut self) {
        self.bus.st.player = None;
    }

    /// Позиция воспроизведения, с (`None` — кассеты нет).
    pub fn tape_position(&self) -> Option<f64> {
        let now = self.master_ticks();
        self.bus.st.player.as_ref().map(|p| p.position(now))
    }

    /// Воспроизведение закончилось (или кассеты нет).
    pub fn tape_finished(&self) -> bool {
        let now = self.master_ticks();
        self.bus.st.player.as_ref().is_none_or(|p| p.finished(now))
    }

    /// Сигнал блока памяти `start..=end` в формате МОНИТОРА — для записи в WAV.
    pub fn tape_block_signal(&self, format: TapeFormat, start: u16, end: u16) -> TapeSignal {
        tape::block_signal(format, start, &self.save_bin(start, end), WAV_RATE)
    }

    // ----- снимки -----

    /// Снимок состояния.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: SNAPSHOT_VERSION,
            cpu: CpuState::from(&self.cpu),
            bus: self.bus.st.clone(),
            clock: self.clock.clone(),
            expansions: self
                .bus
                .expansions
                .iter()
                .map(|e| (e.name().to_string(), e.save_state()))
                .collect(),
        }
    }

    /// Восстановить состояние из снимка. Устройства расширения должны быть подключены
    /// те же и в том же порядке.
    pub fn restore(&mut self, s: &Snapshot) -> Result<(), SnapshotError> {
        if s.version != SNAPSHOT_VERSION {
            return Err(SnapshotError::Format(format!("версия {}", s.version)));
        }
        if s.expansions.len() != self.bus.expansions.len() {
            return Err(SnapshotError::Format(
                "другой набор устройств расширения".into(),
            ));
        }
        for ((name, data), e) in s.expansions.iter().zip(&mut self.bus.expansions) {
            if name != e.name() {
                return Err(SnapshotError::Format(format!(
                    "устройство {name} вместо {}",
                    e.name()
                )));
            }
            e.load_state(data).map_err(SnapshotError::Format)?;
        }
        self.cpu = Cpu::from(&s.cpu);
        self.bus.st = s.bus.clone();
        self.bus.sound_edges.clear();
        self.clock = s.clock.clone();
        Ok(())
    }

    /// Машина из снимка (без устройств расширения).
    pub fn from_snapshot(s: &Snapshot) -> Result<Self, SnapshotError> {
        let mut m = Machine::new(s.bus.config.clone());
        m.restore(s)?;
        Ok(m)
    }

    /// Снимок в байты.
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        let mut out = SNAPSHOT_MAGIC.to_vec();
        out.extend(bincode::serialize(&self.snapshot()).expect("сериализация снимка"));
        out
    }

    /// Разобрать снимок из байтов.
    pub fn parse_snapshot(data: &[u8]) -> Result<Snapshot, SnapshotError> {
        let body = data
            .strip_prefix(SNAPSHOT_MAGIC)
            .ok_or_else(|| SnapshotError::Format("нет сигнатуры UT88SNAP".into()))?;
        bincode::deserialize(body).map_err(|e| SnapshotError::Format(e.to_string()))
    }

    /// Сохранить снимок в файл.
    pub fn save_snapshot(&self, path: impl AsRef<Path>) -> Result<(), SnapshotError> {
        std::fs::write(path, self.snapshot_bytes())?;
        Ok(())
    }

    /// Загрузить снимок из файла в эту машину.
    pub fn load_snapshot(&mut self, path: impl AsRef<Path>) -> Result<(), SnapshotError> {
        let data = std::fs::read(path)?;
        let s = Self::parse_snapshot(&data)?;
        self.restore(&s)
    }
}

/// Команды, обращающиеся к стеку.
fn is_stack_opcode(op: u8) -> bool {
    matches!(op & 0xC7, 0xC0 | 0xC4 | 0xC7) // Rcc, Ccc, RST
        || matches!(op & 0xCF, 0xC1 | 0xC5 | 0xCD) // POP, PUSH, CALL/*CALL
        || matches!(op, 0xC9 | 0xD9) // RET/*RET
        || op == 0xE3 // XTHL
}
