//! Системная шина: карта памяти и портов, собранная из модулей по [`Config`].
//!
//! Карта памяти (doc/02, doc/04, doc/15, doc/29):
//!
//! | Адреса      | Что                                                                     |
//! |-------------|-------------------------------------------------------------------------|
//! | 0000H–07FFH | ПЗУ МОНИТОРА модуля 1 (1 Кбайт, повтор по A10; DD10/DD11 выбираются A11) |
//! | 0800H–0FFFH | ПЗУ калькулятора (если подключено)                                      |
//! | 9000H–9FFFH | индикаторы (запись; A0–A1 — номер ячейки)                               |
//! | C000H–C3FFH | статическое ОЗУ модуля 1 (отключено, если есть модуль ОЗУ 64 Кбайт)      |
//! | E000H–EFFFH | экранное ОЗУ (2 Кбайта, A11 не декодируется)                             |
//! | F400H–F7FFH | ОЗУ МОНИТОРА-«F»                                                        |
//! | F800H–FFFFH | ПЗУ МОНИТОРА-«F»                                                        |
//!
//! Модуль ОЗУ 64 Кбайт отвечает везде, где не заблокирован сигналами выборки сегментов
//! других модулей: 0XXX (если включено ПЗУ модуля 1 или калькулятора), EXXX и FXXX (если
//! есть дисплейный модуль; сегмент FXXX можно отдать ОЗУ тумблером
//! [`Config::monitor_f_rom`]). Свободные адреса читаются как FFH.
//!
//! Квазидиск (doc/32): пока порт 40H открывает блок, все стековые циклы процессора
//! ([`i8080::Bus::read_stack`] / [`i8080::Bus::write_stack`]) идут в него, а не в карту
//! выше.
//!
//! Для обращений процессора шина держит таблицу страниц (256 страниц по 256 байт):
//! частые обращения к ОЗУ и ПЗУ идут по ней без разбора карты; всё необычное
//! (индикаторы, ПЗУ калькулятора, пустые адреса) — через полный разбор. Таблица строится
//! по конфигурации и перестраивается [`Bus::refresh_map`] (его вызывают
//! [`crate::Machine::step`] и [`crate::Machine::run_cycles`]), если конфигурация
//! поменялась.
//!
//! Порты: A0H–AFH (чётные) — клавиатура модуля 1, нечётные — магнитофон (бит 0;
//! дешифратор DD7 различает только старшую тетраду, магнитофон выбирается по A0 = 1);
//! 04H–07H и F8H–FBH — ППА дисплейного модуля; 40H (запись) — квазидиск.

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::devices::cpu_module::CpuModule;
use crate::devices::display::DisplayModule;
use crate::devices::kvazidisk::{self, Kvazidisk};
use crate::tape::{CPU_DIV, TapePlayer, TapeRecorder};

/// Вид обращения к памяти — для устройств расширения (квазидиск переключает именно
/// стековые обращения).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemAccess {
    /// Обычное обращение.
    Normal,
    /// Обращение к стеку: машинный цикл с битом STACK слова состояния (PUSH, POP, CALL,
    /// RET, RST, XTHL, приём прерывания).
    Stack,
}

/// Точка расширения: устройство на системной шине (например, квазидиск).
///
/// Устройства опрашиваются раньше штатных модулей; `Some`/`true` означает, что
/// устройство ответило и штатная логика не выполняется.
pub trait Expansion: Send {
    /// Имя устройства (используется при восстановлении снимков).
    fn name(&self) -> &str;
    /// Сброс «УСТ».
    fn reset(&mut self) {}
    /// Чтение памяти.
    fn mem_read(&mut self, _addr: u16, _access: MemAccess) -> Option<u8> {
        None
    }
    /// Запись в память.
    fn mem_write(&mut self, _addr: u16, _v: u8, _access: MemAccess) -> bool {
        false
    }
    /// Чтение порта.
    fn port_read(&mut self, _port: u8) -> Option<u8> {
        None
    }
    /// Запись в порт.
    fn port_write(&mut self, _port: u8, _v: u8) -> bool {
        false
    }
    /// Состояние для снимка.
    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }
    /// Восстановить состояние из снимка.
    fn load_state(&mut self, _data: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

/// Сохраняемая часть шины (всё, кроме устройств расширения).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusState {
    /// Конфигурация.
    pub config: Config,
    /// Модуль 1.
    pub cpu_module: CpuModule,
    /// Дисплейный модуль.
    pub display: Option<DisplayModule>,
    /// Модуль ОЗУ 64 Кбайт.
    pub ram64: Option<Vec<u8>>,
    /// Воспроизводимая лента.
    pub player: Option<TapePlayer>,
    /// Идущая запись выхода магнитофона.
    pub recorder: Option<TapeRecorder>,
    /// Квазидиск.
    pub kvazidisk: Option<Kvazidisk>,
    /// Такт 16 МГц начала текущей команды.
    pub now: u64,
}

/// Куда ведёт страница памяти (256 байт) в таблице страниц шины.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    /// Модуль ОЗУ 64 Кбайт (адрес как есть).
    Ram64,
    /// ПЗУ МОНИТОРА модуля 1 (адрес & 3FFH), только чтение.
    Rom1,
    /// Статическое ОЗУ модуля 1 (адрес & 3FFH).
    Ram1,
    /// Экранное ОЗУ (адрес & 7FFH).
    Vram,
    /// ОЗУ МОНИТОРА-«F» (адрес & 3FFH).
    DispRam,
    /// ПЗУ МОНИТОРА-«F» (адрес & 7FFH), только чтение.
    DispRom,
    /// Полный разбор карты ([`Bus::peek`] / [`Bus::poke`]).
    Slow,
}

/// Шина машины.
pub struct Bus {
    /// Состояние модулей.
    pub st: BusState,
    /// Устройства расширения.
    pub expansions: Vec<Box<dyn Expansion>>,
    /// Перепады выхода магнитофона для звука: `(такт 16 МГц, уровень)`.
    pub(crate) sound_edges: Vec<(u64, bool)>,
    /// Собирать перепады для звука.
    pub(crate) sound_capture: bool,
    /// Экранное ОЗУ изменилось (сбрасывает [`crate::Machine::take_screen_changed`]).
    pub(crate) screen_changed: bool,
    /// Индикаторы модуля 1 изменились (сбрасывает
    /// [`crate::Machine::take_indicators_changed`]).
    pub(crate) indicators_changed: bool,
    /// Таблица страниц для чтения.
    rmap: [Page; 256],
    /// Таблица страниц для записи.
    wmap: [Page; 256],
    /// Ключ раскладки, по которой построены таблицы ([`Bus::layout_key`]).
    map_key: u8,
}

/// Предел буфера перепадов для звука (если его никто не забирает).
const SOUND_EDGES_MAX: usize = 1 << 16;

impl Bus {
    /// Собрать шину по конфигурации (память обнулена).
    pub fn new(config: Config) -> Self {
        let cpu_module = CpuModule::new(&config.monitor_rom);
        let display = config.display.as_ref().map(DisplayModule::new);
        let ram64 = config.ram64.then(|| vec![0u8; 0x10000]);
        let kvazidisk = (config.kvazidisk > 0).then(|| Kvazidisk::new(config.kvazidisk));
        let mut bus = Self {
            st: BusState {
                config,
                cpu_module,
                display,
                ram64,
                player: None,
                recorder: None,
                kvazidisk,
                now: 0,
            },
            expansions: Vec::new(),
            sound_edges: Vec::new(),
            sound_capture: false,
            screen_changed: true,
            indicators_changed: true,
            rmap: [Page::Slow; 256],
            wmap: [Page::Slow; 256],
            map_key: u8::MAX,
        };
        bus.refresh_map();
        bus
    }

    /// Ключ раскладки памяти: всё, от чего зависят таблицы страниц.
    #[inline]
    fn layout_key(&self) -> u8 {
        let st = &self.st;
        u8::from(st.config.rom1_enabled)
            | u8::from(st.config.calculator_rom.is_some()) << 1
            | u8::from(st.display.is_some()) << 2
            | u8::from(st.ram64.is_some()) << 3
            | u8::from(st.config.monitor_f_rom) << 4
    }

    /// Перестроить таблицы страниц, если раскладка памяти поменялась (конфигурация,
    /// восстановление снимка). Дёшево, если не поменялась.
    #[inline]
    pub fn refresh_map(&mut self) {
        let key = self.layout_key();
        if key != self.map_key {
            self.build_map(key);
        }
    }

    #[cold]
    fn build_map(&mut self, key: u8) {
        let seg0 = self.segment0_blocked();
        let seg_f = self.segment_f_display();
        let st = &self.st;
        let ram64 = st.ram64.is_some();
        for p in 0..256usize {
            let addr = p << 8;
            let (r, w) = match addr >> 12 {
                0x0 if seg0 => {
                    let r = if addr & 0x0800 == 0 && st.config.rom1_enabled {
                        Page::Rom1
                    } else {
                        Page::Slow
                    };
                    (r, Page::Slow)
                }
                // Индикаторы (и ОЗУ под ними) — полный разбор при записи.
                0x9 => (if ram64 { Page::Ram64 } else { Page::Slow }, Page::Slow),
                0xC if !ram64 && addr < 0xC400 => (Page::Ram1, Page::Ram1),
                0xE if st.display.is_some() => (Page::Vram, Page::Vram),
                0xF if seg_f => match addr {
                    0xF000..=0xF3FF => (Page::Slow, Page::Slow),
                    0xF400..=0xF7FF => (Page::DispRam, Page::DispRam),
                    _ => (Page::DispRom, Page::Slow),
                },
                _ if ram64 => (Page::Ram64, Page::Ram64),
                _ => (Page::Slow, Page::Slow),
            };
            self.rmap[p] = r;
            self.wmap[p] = w;
        }
        self.map_key = key;
    }

    fn segment0_blocked(&self) -> bool {
        self.st.config.rom1_enabled || self.st.config.calculator_rom.is_some()
    }

    /// Сегмент F000H–FFFFH занят дисплейным модулем.
    fn segment_f_display(&self) -> bool {
        self.st.display.is_some() && self.st.config.monitor_f_rom
    }

    /// Чтение памяти без побочных эффектов и без устройств расширения.
    pub fn peek(&self, addr: u16) -> u8 {
        let st = &self.st;
        let a = usize::from(addr);
        match addr >> 12 {
            0x0 if self.segment0_blocked() => {
                if addr & 0x0800 == 0 {
                    if st.config.rom1_enabled {
                        st.cpu_module.rom[a & 0x3FF]
                    } else {
                        0xFF
                    }
                } else if let Some(c) = &st.config.calculator_rom {
                    c.get(a & 0x7FF).copied().unwrap_or(0xFF)
                } else {
                    0xFF
                }
            }
            0xC if st.ram64.is_none() && addr < 0xC400 => st.cpu_module.ram[a & 0x3FF],
            0xE if st.display.is_some() => st.display.as_ref().unwrap().vram[a & 0x7FF],
            0xF if self.segment_f_display() => {
                let d = st.display.as_ref().unwrap();
                match addr {
                    0xF000..=0xF3FF => 0xFF,
                    0xF400..=0xF7FF => d.ram[a & 0x3FF],
                    _ => d.rom[a & 0x7FF],
                }
            }
            _ => st.ram64.as_ref().map_or(0xFF, |r| r[a]),
        }
    }

    /// Запись в память без устройств расширения (ПЗУ не меняется).
    pub fn poke(&mut self, addr: u16, v: u8) {
        let seg0 = self.segment0_blocked();
        let seg_f = self.segment_f_display();
        let st = &mut self.st;
        let a = usize::from(addr);
        match addr >> 12 {
            0x0 if seg0 => {}
            0x9 => {
                // Индикаторы пишутся всегда; модуль ОЗУ, если есть, тоже запоминает байт.
                if st.cpu_module.indicators[a & 3] != v {
                    st.cpu_module.indicators[a & 3] = v;
                    self.indicators_changed = true;
                }
                if let Some(r) = &mut st.ram64 {
                    r[a] = v;
                }
            }
            0xC if st.ram64.is_none() && addr < 0xC400 => st.cpu_module.ram[a & 0x3FF] = v,
            0xE if st.display.is_some() => {
                let x = &mut st.display.as_mut().unwrap().vram[a & 0x7FF];
                if *x != v {
                    *x = v;
                    self.screen_changed = true;
                }
            }
            0xF if seg_f => {
                if (0xF400..=0xF7FF).contains(&addr) {
                    st.display.as_mut().unwrap().ram[a & 0x3FF] = v;
                }
            }
            _ => {
                if let Some(r) = &mut st.ram64 {
                    r[a] = v;
                }
            }
        }
    }

    /// Чтение по таблице страниц (без устройств расширения). Страница, не совпавшая с
    /// состоянием (таблица устарела), читается полным разбором.
    #[inline(always)]
    fn read_mapped(&self, addr: u16) -> u8 {
        let a = usize::from(addr);
        let st = &self.st;
        match self.rmap[a >> 8] {
            Page::Ram64 => match &st.ram64 {
                Some(r) => r[a],
                None => self.peek(addr),
            },
            Page::Rom1 => st.cpu_module.rom[a & 0x3FF],
            Page::Ram1 => st.cpu_module.ram[a & 0x3FF],
            Page::Vram => match &st.display {
                Some(d) => d.vram[a & 0x7FF],
                None => self.peek(addr),
            },
            Page::DispRam => match &st.display {
                Some(d) => d.ram[a & 0x3FF],
                None => self.peek(addr),
            },
            Page::DispRom => match &st.display {
                Some(d) => d.rom[a & 0x7FF],
                None => self.peek(addr),
            },
            Page::Slow => self.peek(addr),
        }
    }

    /// Запись по таблице страниц (без устройств расширения).
    #[inline(always)]
    fn write_mapped(&mut self, addr: u16, v: u8) {
        let a = usize::from(addr);
        match self.wmap[a >> 8] {
            Page::Ram64 => match &mut self.st.ram64 {
                Some(r) => r[a] = v,
                None => self.poke(addr, v),
            },
            Page::Ram1 => self.st.cpu_module.ram[a & 0x3FF] = v,
            Page::Vram => match &mut self.st.display {
                Some(d) => {
                    let x = &mut d.vram[a & 0x7FF];
                    if *x != v {
                        *x = v;
                        self.screen_changed = true;
                    }
                }
                None => self.poke(addr, v),
            },
            Page::DispRam => match &mut self.st.display {
                Some(d) => d.ram[a & 0x3FF] = v,
                None => self.poke(addr, v),
            },
            _ => self.poke(addr, v),
        }
    }

    #[inline(always)]
    fn mem_read(&mut self, addr: u16, acc: MemAccess) -> u8 {
        if !self.expansions.is_empty() {
            return self.mem_read_ext(addr, acc);
        }
        self.read_mapped(addr)
    }

    #[inline(never)]
    fn mem_read_ext(&mut self, addr: u16, acc: MemAccess) -> u8 {
        for e in &mut self.expansions {
            if let Some(v) = e.mem_read(addr, acc) {
                return v;
            }
        }
        self.read_mapped(addr)
    }

    #[inline(always)]
    fn mem_write(&mut self, addr: u16, v: u8, acc: MemAccess) {
        if !self.expansions.is_empty() {
            return self.mem_write_ext(addr, v, acc);
        }
        self.write_mapped(addr, v);
    }

    #[inline(never)]
    fn mem_write_ext(&mut self, addr: u16, v: u8, acc: MemAccess) {
        for e in &mut self.expansions {
            if e.mem_write(addr, v, acc) {
                return;
            }
        }
        self.write_mapped(addr, v);
    }

    /// Выход магнитофона: запомнить перепад.
    fn set_tape_out(&mut self, level: bool) {
        if self.st.cpu_module.tape_out == level {
            return;
        }
        self.st.cpu_module.tape_out = level;
        // Запись в порт происходит в последнем машинном цикле OUT (≈ 7-й такт из 10).
        let t = self.st.now + 7 * CPU_DIV;
        if let Some(rec) = &mut self.st.recorder {
            rec.edges.push((t, level));
        }
        if self.sound_capture {
            if self.sound_edges.len() >= SOUND_EDGES_MAX {
                self.sound_edges.drain(..SOUND_EDGES_MAX / 2);
            }
            self.sound_edges.push((t, level));
        }
    }

    /// Вход магнитофона.
    pub fn tape_in(&self) -> bool {
        let t = self.st.now + 7 * CPU_DIV;
        self.st
            .player
            .as_ref()
            .and_then(|p| p.level_at(t))
            .unwrap_or(false)
    }
}

impl i8080::Bus for Bus {
    #[inline(always)]
    fn read(&mut self, addr: u16) -> u8 {
        self.mem_read(addr, MemAccess::Normal)
    }

    #[inline(always)]
    fn write(&mut self, addr: u16, v: u8) {
        self.mem_write(addr, v, MemAccess::Normal);
    }

    #[inline]
    fn read_stack(&mut self, addr: u16) -> u8 {
        if let Some(q) = &self.st.kvazidisk
            && q.is_open()
        {
            return q.read(addr);
        }
        self.mem_read(addr, MemAccess::Stack)
    }

    #[inline]
    fn write_stack(&mut self, addr: u16, v: u8) {
        if let Some(q) = &mut self.st.kvazidisk
            && q.is_open()
        {
            q.write(addr, v);
            return;
        }
        self.mem_write(addr, v, MemAccess::Stack);
    }

    fn input(&mut self, port: u8) -> u8 {
        for e in &mut self.expansions {
            if let Some(v) = e.port_read(port) {
                return v;
            }
        }
        if let Some(d) = &self.st.display
            && let Some(v) = d.port_read(port)
        {
            return v;
        }
        match port {
            0xA0..=0xAF if port & 1 == 0 => self.st.cpu_module.keyboard(),
            // Магнитофон отвечает только разрядом D0 (DD12.4 с открытым коллектором),
            // остальные разряды шины читаются как 1.
            0xA0..=0xAF => {
                let t = self.st.now + 7 * CPU_DIV;
                if let Some(p) = &mut self.st.player
                    && p.auto_start
                {
                    p.start(t);
                }
                0xFE | u8::from(self.tape_in())
            }
            _ => 0xFF,
        }
    }

    fn output(&mut self, port: u8, v: u8) {
        for e in &mut self.expansions {
            if e.port_write(port, v) {
                return;
            }
        }
        if port == kvazidisk::PORT
            && let Some(q) = &mut self.st.kvazidisk
        {
            q.port = v;
            return;
        }
        if let Some(d) = &mut self.st.display
            && d.port_write(port, v)
        {
            return;
        }
        if (0xA0..=0xAF).contains(&port) && port & 1 == 1 {
            self.set_tape_out(v & 1 != 0);
        }
    }
}
