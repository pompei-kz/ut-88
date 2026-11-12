//! CP/M и ОС ЮТ-88: загрузка систем из распечаток журнала 1990-02 и работа с файлами на
//! диске CP/M средствами эмулятора.
//!
//! Диски обеих CP/M ЮТ-88 устроены одинаково (по таблицам БСВВ, doc/32, doc/35): дорожка —
//! 8 секторов по 128 байт (1 Кбайт), блок — 1 Кбайт, каталог — блок 0 (32 записи), номера
//! блоков однобайтовые, EXM = 0 (одна запись каталога — 16 блоков, 128 записей по 128 байт).
//! Дорожки лежат в памяти подряд, байт i сектора S дорожки T — по смещению
//! `T × 400H + (S − 1) × 80H + i`:
//!
//! - **CP/M-64** — квазидиск, 6 системных дорожек, `DSM = 64 × блоков − 7` (39H, 79H, B9H,
//!   F9H для 64…256 Кбайт); дорожка T — в блоке квазидиска T div 64, то есть смещение в
//!   содержимом квазидиска (блоки подряд) — ровно `T × 400H`;
//! - **CP/M-35** — «квазидиск в основном ОЗУ» 5000H–DBFFH: 35 дорожек без системных,
//!   `DSM = 22H`.

use crate::config::Config;
use crate::devices::kvazidisk::{self, BLOCK_SIZE};
use crate::machine::Machine;
use crate::roms;

/// Какой диск CP/M.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpmDisk {
    /// Квазидиск CP/M-64.
    Kvazidisk,
    /// Диск CP/M-35 в основном ОЗУ (5000H–DBFFH).
    Ram35,
}

/// Начало диска CP/M-35 в ОЗУ.
pub const CPM35_DISK: u16 = 0x5000;
/// Число дорожек (= блоков) диска CP/M-35.
const CPM35_TRACKS: usize = 35;
/// Системных дорожек CP/M-64.
const CPM64_OFF: usize = 6;

/// Адрес загрузки CP/M-64 и её загрузчика.
pub const CPM64_LOAD: u16 = 0x3100;
/// Ячейка младшего байта DSM в распечатке CP/M-64 (doc/32: поправка по объёму квазидиска).
pub const CPM64_DSM_CELL: u16 = 0x4A50;
/// Адрес загрузки CP/M-35.
pub const CPM35_LOAD: u16 = 0x3400;
/// Пуск CP/M-35 (BOOT БСВВ).
pub const CPM35_START: u16 = 0x4A00;
/// Адрес загрузки и пуска ОС ЮТ-88.
pub const UT88_OS_LOAD: u16 = 0x3000;

/// Исправления распечатки ОС ЮТ-88, которые эмулятор вносит при загрузке:
/// `(адрес в загрузочном модуле, байт)`.
///
/// - **3320H: 4DH → 68H.** Цикл прокрутки экрана монитора ОС (FA68H–FA6FH после переноса:
///   `LDAX D / MOV M,A / INX D / INX H / MOV A,D / CPI EFH / JNZ …`) по распечатке
///   переходит на FA4DH — в середину команды `JMP FA39H`; по смыслу — на начало цикла
///   FA68H. Прокрутка включена по умолчанию (F77AH = FFH), и с напечатанным байтом ОС
///   рушится при первой же прокрутке экрана. Сумма блока 3000H–33FFH после правки
///   отличается от журнальной на 3 вместо 24 (doc/35).
pub const UT88_OS_PATCHES: &[(u16, u8)] = &[(0x3320, 0x68)];

/// Размер записи CP/M.
const RECORD: usize = 128;
/// Размер блока CP/M.
const BLS: usize = 1024;
/// Записей каталога.
const DIR_ENTRIES: usize = 32;
/// Свободная запись каталога.
const EMPTY: u8 = 0xE5;

/// Файл в каталоге диска CP/M.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpmFile {
    /// Номер пользователя.
    pub user: u8,
    /// Имя в виде `ИМЯ.РАС`.
    pub name: String,
    /// Размер в записях по 128 байт.
    pub records: usize,
    /// Только чтение (бит 7 первого байта расширения).
    pub read_only: bool,
}

impl CpmFile {
    /// Размер в байтах (кратен 128).
    pub fn bytes(&self) -> usize {
        self.records * RECORD
    }
}

/// Имя файла CP/M «ИМЯ.РАС» → 11 байт (8 + 3, дополнены пробелами, заглавные буквы).
pub fn parse_cpm_name(name: &str) -> Result<[u8; 11], String> {
    let name = name.trim();
    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return Err(format!(
            "имя CP/M «{name}»: нужно ИМЯ.РАС, не больше 8 и 3 символов"
        ));
    }
    let mut out = [b' '; 11];
    for (i, c) in base
        .chars()
        .chain(std::iter::repeat_n(' ', 8 - base.len()))
        .enumerate()
    {
        out[i] = cpm_char(c, name)?;
    }
    for (i, c) in ext.chars().enumerate() {
        out[8 + i] = cpm_char(c, name)?;
    }
    Ok(out)
}

fn cpm_char(c: char, name: &str) -> Result<u8, String> {
    let c = c.to_ascii_uppercase();
    if c == ' ' || (c.is_ascii_graphic() && !"<>.,;:=?*[]_%|()/\\".contains(c)) {
        Ok(c as u8)
    } else {
        Err(format!("недопустимый символ «{c}» в имени CP/M «{name}»"))
    }
}

/// Разобрать «ФАЙЛ[=ИМЯ.РАС]»: путь к файлу ПК и, если задано, имя на диске CP/M.
pub fn split_put_arg(s: &str) -> (std::path::PathBuf, Option<String>) {
    match s.rsplit_once('=') {
        Some((p, n)) if !p.is_empty() && !n.is_empty() && !n.contains(['/', '\\']) => {
            (p.into(), Some(n.to_string()))
        }
        _ => (s.into(), None),
    }
}

/// Имя файла CP/M из каталога: «ИМЯ.РАС».
fn format_cpm_name(raw: &[u8]) -> String {
    let part = |b: &[u8]| -> String {
        b.iter()
            .map(|&c| char::from(c & 0x7F))
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    let base = part(&raw[0..8]);
    let ext = part(&raw[8..11]);
    if ext.is_empty() {
        base
    } else {
        format!("{base}.{ext}")
    }
}

/// Файловая система CP/M на линейном образе диска (дорожка 0 — с начала `data`).
pub struct CpmFs<'a> {
    data: &'a mut [u8],
    /// Системных дорожек (OFF).
    off: usize,
    /// Номер последнего блока (DSM).
    dsm: usize,
}

impl<'a> CpmFs<'a> {
    /// Файловая система: `off` системных дорожек, последний блок `dsm` (< 256).
    pub fn new(data: &'a mut [u8], off: usize, dsm: usize) -> Self {
        assert!(dsm < 256, "поддержаны только однобайтовые номера блоков");
        assert!(data.len() >= (off + dsm + 1) * BLS, "образ меньше диска");
        Self { data, off, dsm }
    }

    fn block(&self, n: usize) -> usize {
        (self.off + n) * BLS
    }

    fn entry(&self, i: usize) -> &[u8] {
        let o = self.block(0) + i * 32;
        &self.data[o..o + 32]
    }

    fn entry_mut(&mut self, i: usize) -> &mut [u8] {
        let o = self.block(0) + i * 32;
        &mut self.data[o..o + 32]
    }

    /// Каталог похож на каталог CP/M (все записи — свободные или с номером пользователя
    /// 0–31 и печатными символами имени).
    pub fn looks_formatted(&self) -> bool {
        (0..DIR_ENTRIES).all(|i| {
            let e = self.entry(i);
            e[0] == EMPTY
                || (e[0] < 32
                    && e[1..12].iter().all(|&c| (0x20..0x7F).contains(&(c & 0x7F)))
                    && e[16..].iter().all(|&b| usize::from(b) <= self.dsm))
        })
    }

    /// Очистить каталог (все записи E5H).
    pub fn format(&mut self) {
        let o = self.block(0);
        self.data[o..o + DIR_ENTRIES * 32].fill(EMPTY);
    }

    /// Файлы каталога (экстенты собраны по имени), по порядку первого появления.
    pub fn list(&self) -> Vec<CpmFile> {
        let mut out: Vec<(CpmFile, [u8; 12])> = Vec::new();
        for i in 0..DIR_ENTRIES {
            let e = self.entry(i);
            if e[0] >= 32 {
                continue;
            }
            let mut key = [0u8; 12];
            key[0] = e[0];
            for (k, &c) in key[1..].iter_mut().zip(&e[1..12]) {
                *k = c & 0x7F;
            }
            // Записей до конца этого экстента: (EX + 32 × S2) × 128 + RC.
            let ext = usize::from(e[12]) + 32 * usize::from(e[14]);
            let recs = ext * 128 + usize::from(e[15]);
            match out.iter_mut().find(|(_, k)| *k == key) {
                Some((f, _)) => f.records = f.records.max(recs),
                None => out.push((
                    CpmFile {
                        user: e[0],
                        name: format_cpm_name(&e[1..12]),
                        records: recs,
                        read_only: e[9] & 0x80 != 0,
                    },
                    key,
                )),
            }
        }
        out.into_iter().map(|(f, _)| f).collect()
    }

    fn matches(&self, i: usize, user: u8, name: &[u8; 11]) -> bool {
        let e = self.entry(i);
        e[0] == user && e[1..12].iter().zip(name).all(|(&a, &b)| a & 0x7F == b)
    }

    /// Прочитать файл пользователя `user`.
    pub fn read(&self, user: u8, name: &str) -> Result<Vec<u8>, String> {
        let raw = parse_cpm_name(name)?;
        let mut exts: Vec<(usize, usize)> = (0..DIR_ENTRIES)
            .filter(|&i| self.matches(i, user, &raw))
            .map(|i| {
                let e = self.entry(i);
                (usize::from(e[12]) + 32 * usize::from(e[14]), i)
            })
            .collect();
        if exts.is_empty() {
            return Err(format!("на диске CP/M нет файла {name}"));
        }
        exts.sort();
        let mut out = Vec::new();
        for (ext, i) in exts {
            let e = self.entry(i).to_vec();
            out.resize(ext * 128 * RECORD, 0x1A);
            let mut left = usize::from(e[15]) * RECORD;
            for &b in &e[16..] {
                if left == 0 || b == 0 {
                    break;
                }
                let n = left.min(BLS);
                let o = self.block(usize::from(b));
                out.extend_from_slice(&self.data[o..o + n]);
                left -= n;
            }
        }
        Ok(out)
    }

    /// Удалить файл; `false` — такого нет.
    pub fn delete(&mut self, user: u8, name: &str) -> Result<bool, String> {
        let raw = parse_cpm_name(name)?;
        let mut found = false;
        for i in 0..DIR_ENTRIES {
            if self.matches(i, user, &raw) {
                self.entry_mut(i)[0] = EMPTY;
                found = true;
            }
        }
        Ok(found)
    }

    /// Свободных блоков данных и свободных записей каталога.
    pub fn free(&self) -> (usize, usize) {
        let used = self.used_blocks();
        let blocks = used.iter().filter(|&&u| !u).count();
        let entries = (0..DIR_ENTRIES).filter(|&i| self.entry(i)[0] >= 32).count();
        (blocks, entries)
    }

    fn used_blocks(&self) -> Vec<bool> {
        let mut used = vec![false; self.dsm + 1];
        used[0] = true; // каталог
        for i in 0..DIR_ENTRIES {
            let e = self.entry(i);
            if e[0] < 32 {
                for &b in &e[16..] {
                    if let Some(u) = used.get_mut(usize::from(b))
                        && b != 0
                    {
                        *u = true;
                    }
                }
            }
        }
        used
    }

    /// Записать файл пользователя `user` (существующий с тем же именем заменяется).
    /// Последняя запись дополняется кодом 1AH (конец текста CP/M).
    pub fn write(&mut self, user: u8, name: &str, data: &[u8]) -> Result<(), String> {
        let raw = parse_cpm_name(name)?;
        let records = data.len().div_ceil(RECORD);
        let nblocks = data.len().div_ceil(BLS);
        let nexts = records.div_ceil(128).max(1);
        // Проверить место, не трогая диск.
        let mut used = self.used_blocks();
        let mut free_entries: Vec<usize> = Vec::new();
        for i in 0..DIR_ENTRIES {
            if self.entry(i)[0] >= 32 || self.matches(i, user, &raw) {
                free_entries.push(i);
            }
            if self.matches(i, user, &raw) {
                for &b in &self.entry(i)[16..] {
                    if b != 0 {
                        used[usize::from(b)] = false;
                    }
                }
            }
        }
        let free_blocks: Vec<usize> = (1..=self.dsm).filter(|&b| !used[b]).collect();
        if free_entries.len() < nexts {
            return Err("в каталоге диска CP/M нет места (NO DIRECTORY SPACE)".into());
        }
        if free_blocks.len() < nblocks {
            return Err(format!(
                "на диске CP/M нет места: нужно {nblocks} К, свободно {} К (DISK FULL)",
                free_blocks.len()
            ));
        }
        self.delete(user, name)?;
        for (k, &b) in free_blocks[..nblocks].iter().enumerate() {
            let o = self.block(b);
            let chunk = &data[k * BLS..data.len().min((k + 1) * BLS)];
            self.data[o..o + chunk.len()].copy_from_slice(chunk);
            let pad_end = chunk.len().div_ceil(RECORD) * RECORD;
            self.data[o + chunk.len()..o + pad_end].fill(0x1A);
        }
        for (x, &slot) in free_entries.iter().enumerate().take(nexts) {
            let e = self.entry_mut(slot);
            e.fill(0);
            e[0] = user;
            e[1..12].copy_from_slice(&raw);
            e[12] = (x % 32) as u8;
            e[14] = (x / 32) as u8;
            e[15] = (records - x * 128).min(128) as u8;
            for (j, &b) in free_blocks
                .iter()
                .take(nblocks)
                .skip(x * 16)
                .take(16)
                .enumerate()
            {
                e[16 + j] = b as u8;
            }
        }
        Ok(())
    }
}

/// Система, которую эмулятор умеет загрузить из распечаток журнала.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    /// CP/M-64 с квазидиском.
    Cpm64,
    /// CP/M-35 (диск в основном ОЗУ).
    Cpm35,
    /// ОС ЮТ-88.
    Ut88Os,
}

impl System {
    /// Все системы.
    pub const ALL: [System; 3] = [System::Cpm64, System::Cpm35, System::Ut88Os];

    /// Название.
    pub fn title(self) -> &'static str {
        match self {
            System::Cpm64 => "CP/M-64",
            System::Cpm35 => "CP/M-35",
            System::Ut88Os => "ОС ЮТ-88",
        }
    }

    /// Конфигурация подходит для системы.
    pub fn suits(self, c: &Config) -> bool {
        c.display.is_some()
            && c.ram64
            && !c.rom1_enabled
            && c.calculator_rom.is_none()
            && (self != System::Cpm64 || c.kvazidisk > 0)
    }

    /// Если конфигурация не подходит — подходящая ([`Config::cpm`] с тем же ПЗУ модуля 1),
    /// на которую нужно переключиться холодным стартом.
    pub fn config_for(self, c: &Config) -> Option<Config> {
        (!self.suits(c)).then(|| Config {
            monitor_rom: c.monitor_rom.clone(),
            ..Config::cpm()
        })
    }
}

/// Ошибка, если машина не подходит для системы.
fn need(ok: bool, what: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.to_string()) }
}

impl Machine {
    /// DSM диска CP/M-64 для текущего объёма квазидиска.
    pub fn cpm64_dsm(&self) -> Option<u8> {
        self.bus
            .st
            .kvazidisk
            .as_ref()
            .map(|q| (q.blocks() * 64 - CPM64_OFF - 1) as u8)
    }

    /// Какой CP/M сейчас загружена (по вектору BDOS в 0005H: `JMP CC06H` — CP/M-64,
    /// `JMP 3C06H` — CP/M-35).
    pub fn cpm_running(&self) -> Option<CpmDisk> {
        match [self.peek(5), self.peek(6), self.peek(7)] {
            [0xC3, 0x06, 0xCC] if self.bus.st.kvazidisk.is_some() => Some(CpmDisk::Kvazidisk),
            [0xC3, 0x06, 0x3C] => Some(CpmDisk::Ram35),
            _ => None,
        }
    }

    /// Диск CP/M, с которым работают команды эмулятора: загруженной системы, а если
    /// CP/M не запущена — квазидиск (если есть), иначе диск CP/M-35.
    pub fn cpm_default_disk(&self) -> CpmDisk {
        self.cpm_running()
            .unwrap_or(if self.bus.st.kvazidisk.is_some() {
                CpmDisk::Kvazidisk
            } else {
                CpmDisk::Ram35
            })
    }

    /// Выполнить `f` над файловой системой диска CP/M.
    pub fn with_cpm_fs<R>(
        &mut self,
        disk: CpmDisk,
        f: impl FnOnce(&mut CpmFs) -> R,
    ) -> Result<R, String> {
        match disk {
            CpmDisk::Kvazidisk => {
                let dsm = usize::from(self.cpm64_dsm().ok_or("нет квазидиска")?);
                let q = self.bus.st.kvazidisk.as_mut().unwrap();
                Ok(f(&mut CpmFs::new(&mut q.mem, CPM64_OFF, dsm)))
            }
            CpmDisk::Ram35 => {
                let ram = self
                    .bus
                    .st
                    .ram64
                    .as_mut()
                    .ok_or("нет модуля ОЗУ 64 Кбайт")?;
                let start = usize::from(CPM35_DISK);
                let end = start + CPM35_TRACKS * BLS;
                Ok(f(&mut CpmFs::new(
                    &mut ram[start..end],
                    0,
                    CPM35_TRACKS - 1,
                )))
            }
        }
    }

    /// Файлы на диске CP/M.
    pub fn cpm_list(&mut self, disk: CpmDisk) -> Result<Vec<CpmFile>, String> {
        self.with_cpm_fs(disk, |fs| fs.list())
    }

    /// Записать файл на диск CP/M (пользователь 0). Если CP/M работает, после записи нужен
    /// «горячий старт» ([`Machine::cpm_warm_boot`]): БДОС держит карту занятости в ОЗУ.
    pub fn cpm_write_file(&mut self, disk: CpmDisk, name: &str, data: &[u8]) -> Result<(), String> {
        self.with_cpm_fs(disk, |fs| fs.write(0, name, data))?
    }

    /// Прочитать файл с диска CP/M (пользователь 0); длина кратна 128.
    pub fn cpm_read_file(&mut self, disk: CpmDisk, name: &str) -> Result<Vec<u8>, String> {
        self.with_cpm_fs(disk, |fs| fs.read(0, name))?
    }

    /// Удалить файл с диска CP/M (пользователь 0); `false` — файла не было.
    pub fn cpm_delete_file(&mut self, disk: CpmDisk, name: &str) -> Result<bool, String> {
        self.with_cpm_fs(disk, |fs| fs.delete(0, name))?
    }

    /// Перезапустить работающую CP/M без потери файлов, чтобы БДОС заново прочитала
    /// каталог: CP/M-64 — «горячий старт», как УС+C (переход на 0000H, WBOOT); CP/M-35 —
    /// BOOT (4A00H), потому что её WBOOT выходит в МОНИТОР-«F». Квазидиск закрывается.
    /// `false` — CP/M не запущена.
    pub fn cpm_warm_boot(&mut self) -> bool {
        let Some(disk) = self.cpm_running() else {
            return false;
        };
        if let Some(q) = &mut self.bus.st.kvazidisk {
            q.port = kvazidisk::CLOSED;
        }
        self.cpu.inte = false;
        self.release_all_keys();
        self.jump(match disk {
            CpmDisk::Kvazidisk => 0x0000,
            CpmDisk::Ram35 => CPM35_START,
        });
        true
    }

    /// Перезапустить МОНИТОР-«F» (как «УСТ» с переходом на F800H) и дать ему
    /// проинициализироваться: ПЗУ МОНИТОРА-«F» включается, квазидиск закрывается.
    fn restart_monitor_f(&mut self) {
        self.bus.st.config.monitor_f_rom = true;
        self.reset();
        if let Some(q) = &mut self.bus.st.kvazidisk {
            q.port = kvazidisk::CLOSED;
        }
        self.release_all_keys();
        self.jump(0xF800);
        self.run_seconds(0.5);
    }

    fn check_cpm_machine(&self) -> Result<(), String> {
        let c = self.config();
        need(c.display.is_some(), "нужен дисплейный модуль")?;
        need(c.ram64, "нужен модуль ОЗУ 64 Кбайт")?;
        need(
            !c.rom1_enabled && c.calculator_rom.is_none(),
            "нужно ОЗУ в 0000H–0FFFH (выключить ПЗУ модуля 1 и калькулятора)",
        )
    }

    /// Загрузить CP/M-64 (doc/32): МОНИТОР-«F» перезапускается, распечатка кладётся в
    /// 3100H, в ячейку 4A50H — DSM по объёму квазидиска (поправка из журнала).
    ///
    /// `format = true` — пуск загрузчика с 3100H, как в журнале: система и **пустой
    /// каталог** записываются на квазидиск; функция возвращается, когда загрузчик дошёл
    /// до BOOT (PC = DA00H). `format = false` — сохранить файлы на
    /// квазидиске: эмулятор делает то же, что загрузчик (система в C400H–DBFFH и на
    /// дорожки 0–5, драйвер в F500H), но каталог и данные не трогает, и запускает BOOT
    /// (DA00H).
    pub fn boot_cpm64(&mut self, format: bool) -> Result<(), String> {
        self.check_cpm_machine()?;
        let dsm = self
            .cpm64_dsm()
            .ok_or("нужен квазидиск (конфигурация cpm)")?;
        self.restart_monitor_f();
        self.load_bin(CPM64_LOAD, roms::CPM64);
        self.poke(CPM64_DSM_CELL, dsm);
        if format {
            // Загрузчик журнала: перенос системы, запись системы и пустого каталога на
            // квазидиск; останавливаемся на входе в BOOT.
            self.jump(CPM64_LOAD);
            let limit = self.cycles() + (2.0 * crate::CPU_HZ) as u64;
            while self.cpu.pc != 0xDA00 {
                if self.cycles() > limit {
                    return Err(format!(
                        "загрузчик CP/M-64 не дошёл до DA00H (PC={:04X})",
                        self.cpu.pc
                    ));
                }
                self.step();
            }
            return Ok(());
        }
        let sys = self.save_bin(0x3400, 0x4BFF);
        self.load_bin(0xC400, &sys);
        let q = self.bus.st.kvazidisk.as_mut().unwrap();
        q.mem[..sys.len()].copy_from_slice(&sys);
        let drv = self.save_bin(0x31E0, 0x32FF);
        self.load_bin(0xF500, &drv);
        self.cpu.inte = false;
        self.jump(0xDA00);
        Ok(())
    }

    /// Загрузить CP/M-35 (doc/33): распечатка в 3400H, пуск 4A00H. Диск — ОЗУ
    /// 5000H–DBFFH; `format = true` — очистить каталог (5000H–53FFH ← E5H). Прерывания
    /// часов выключаются: БСВВ разрешает прерывания и ставит в 0038H `JMP F800H`.
    pub fn boot_cpm35(&mut self, format: bool) -> Result<(), String> {
        self.check_cpm_machine()?;
        self.set_clock_interrupts(false);
        self.restart_monitor_f();
        self.load_bin(CPM35_LOAD, roms::CPM35);
        if format {
            self.with_cpm_fs(CpmDisk::Ram35, |fs| fs.format())?;
        }
        self.jump(CPM35_START);
        Ok(())
    }

    /// Загрузить ОС ЮТ-88 (doc/30): распечатка в 3000H (с правками
    /// [`UT88_OS_PATCHES`]), пуск стартера `G3000`. Когда
    /// стартер выведет «ВЫКЛЮЧИТЕ ПЗУ МОНИТОРА И НАЖМИТЕ ЛЮБУЮ КЛАВИШУ» и встанет в
    /// ожидание, эмулятор сам «выключает ПЗУ мониторов» ([`Machine::set_monitor_f_rom`])
    /// и прерывания часов; клавишу нажимает пользователь.
    pub fn boot_ut88_os(&mut self) -> Result<(), String> {
        self.check_cpm_machine()?;
        need(
            self.bus.st.ram64.is_some(),
            "ОС ЮТ-88 нужен модуль ОЗУ 64 Кбайт",
        )?;
        self.set_clock_interrupts(false);
        self.restart_monitor_f();
        self.load_bin(UT88_OS_LOAD, roms::UT88_OS);
        for &(addr, v) in UT88_OS_PATCHES {
            self.poke(addr, v);
        }
        self.jump(UT88_OS_LOAD);
        // Стартер: сообщение через F818H, затем цикл ожидания клавиши 300DH–3018H.
        let limit = self.cycles() + (3.0 * crate::CPU_HZ) as u64;
        while !(0x300D..=0x3018).contains(&self.cpu.pc) {
            if self.cycles() > limit {
                return Err(format!(
                    "стартер ОС ЮТ-88 не дошёл до ожидания клавиши (PC={:04X})",
                    self.cpu.pc
                ));
            }
            self.step();
        }
        self.set_monitor_f_rom(false);
        Ok(())
    }

    /// Загрузить систему. `keep_files` — сохранить файлы диска CP/M, если на нём уже
    /// есть каталог (иначе диск очищается). Возвращает сообщение для пользователя.
    /// Конфигурация должна подходить ([`System::suits`]).
    pub fn boot_system(&mut self, sys: System, keep_files: bool) -> Result<String, String> {
        match sys {
            System::Cpm64 => {
                let keep = keep_files && self.kvazidisk_has_cpm();
                self.boot_cpm64(!keep)?;
                let q = self.bus.st.kvazidisk.as_ref().unwrap();
                Ok(format!(
                    "CP/M-64 загружена: квазидиск {} Кбайт (DSM = {:02X}H), {}",
                    q.size_kb(),
                    self.cpm64_dsm().unwrap_or(0),
                    if keep {
                        "файлы на диске сохранены"
                    } else {
                        "диск очищен"
                    }
                ))
            }
            System::Cpm35 => {
                let keep = keep_files
                    && self
                        .with_cpm_fs(CpmDisk::Ram35, |fs| fs.looks_formatted())
                        .unwrap_or(false);
                self.boot_cpm35(!keep)?;
                Ok(format!(
                    "CP/M-35 загружена: диск в ОЗУ 5000H–DBFFH, {}; прерывания часов выключены",
                    if keep {
                        "файлы сохранены"
                    } else {
                        "каталог очищен"
                    }
                ))
            }
            System::Ut88Os => {
                self.boot_ut88_os()?;
                Ok(
                    "ОС ЮТ-88: ПЗУ МОНИТОРА-F и прерывания часов выключены — нажмите любую \
                    клавишу"
                        .into(),
                )
            }
        }
    }

    /// Записать файл ПК на диск CP/M под именем `name` (по умолчанию — имя файла ПК,
    /// заглавными буквами); если CP/M работает — перезапустить её
    /// ([`Machine::cpm_warm_boot`]). Возвращает сообщение.
    pub fn cpm_put_file(
        &mut self,
        path: impl AsRef<std::path::Path>,
        name: Option<&str>,
    ) -> Result<String, String> {
        let path = path.as_ref();
        let name = match name {
            Some(n) => n.to_ascii_uppercase(),
            None => path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("{}: нет имени файла", path.display()))?
                .to_ascii_uppercase(),
        };
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let disk = self.cpm_default_disk();
        self.cpm_write_file(disk, &name, &data)?;
        let restarted = self.cpm_warm_boot();
        Ok(format!(
            "{name} ({} байт) записан на {}{}",
            data.len(),
            match disk {
                CpmDisk::Kvazidisk => "квазидиск (CP/M-64)",
                CpmDisk::Ram35 => "диск CP/M-35",
            },
            if restarted {
                "; CP/M перезапущена, чтобы перечитать каталог"
            } else {
                ""
            }
        ))
    }

    /// Конфигурация для холодного старта: тумблер «ПЗУ МОНИТОРА-«F»» возвращается во
    /// включённое положение (иначе после включения питания нечему работать).
    pub fn cold_start_config(&self) -> Config {
        Config {
            monitor_f_rom: true,
            ..self.config().clone()
        }
    }

    // ----- квазидиск -----

    /// Квазидиск подключён.
    pub fn has_kvazidisk(&self) -> bool {
        self.bus.st.kvazidisk.is_some()
    }

    /// Образ квазидиска (блоки по 64 Кбайт подряд).
    pub fn kvazidisk_image(&self) -> Option<&[u8]> {
        self.bus.st.kvazidisk.as_ref().map(|q| q.mem.as_slice())
    }

    /// Загрузить образ квазидиска (не больше объёма; остаток не меняется).
    pub fn set_kvazidisk_image(&mut self, data: &[u8]) -> Result<(), String> {
        let q = self.bus.st.kvazidisk.as_mut().ok_or("нет квазидиска")?;
        if data.len() > q.mem.len() {
            return Err(format!(
                "образ квазидиска {} Кбайт больше квазидиска {} Кбайт",
                data.len().div_ceil(1024),
                q.size_kb()
            ));
        }
        q.mem[..data.len()].copy_from_slice(data);
        Ok(())
    }

    /// Сохранить образ квазидиска в файл.
    pub fn save_kvazidisk_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let img = self.kvazidisk_image().ok_or("нет квазидиска")?;
        std::fs::write(path.as_ref(), img).map_err(|e| format!("{}: {e}", path.as_ref().display()))
    }

    /// Загрузить образ квазидиска из файла.
    pub fn load_kvazidisk_file(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let data = std::fs::read(path.as_ref())
            .map_err(|e| format!("{}: {e}", path.as_ref().display()))?;
        self.set_kvazidisk_image(&data)
    }

    /// На квазидиске есть каталог CP/M-64 (система записана, каталог не испорчен).
    pub fn kvazidisk_has_cpm(&mut self) -> bool {
        let Some(dsm) = self.cpm64_dsm() else {
            return false;
        };
        let mut sys = roms::CPM64[0x300..0x1B00].to_vec();
        sys[usize::from(CPM64_DSM_CELL - 0x3400)] = dsm;
        let sys_ok = self
            .kvazidisk_image()
            .is_some_and(|m| m.len() >= BLOCK_SIZE && m[..sys.len()] == sys[..]);
        sys_ok
            && self
                .with_cpm_fs(CpmDisk::Kvazidisk, |fs| fs.looks_formatted())
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(&parse_cpm_name("ch.com").unwrap(), b"CH      COM");
        assert_eq!(&parse_cpm_name("README").unwrap(), b"README     ");
        assert!(parse_cpm_name("TOOLONGNAME.COM").is_err());
        assert!(parse_cpm_name("A.COMX").is_err());
        assert!(parse_cpm_name("A*.COM").is_err());
        assert!(parse_cpm_name(".COM").is_err());
        assert_eq!(format_cpm_name(b"CH      COM"), "CH.COM");
        assert_eq!(format_cpm_name(b"README     "), "README");
        assert_eq!(
            split_put_arg("roms/cpm-ch-com.bin=CH.COM"),
            ("roms/cpm-ch-com.bin".into(), Some("CH.COM".into()))
        );
        assert_eq!(split_put_arg("a.com"), ("a.com".into(), None));
        assert_eq!(split_put_arg("x=y/z"), ("x=y/z".into(), None));
    }

    #[test]
    fn write_read_delete_extents() {
        let mut img = vec![0u8; 64 * 1024];
        let mut fs = CpmFs::new(&mut img, 6, 57);
        assert!(!fs.looks_formatted());
        fs.format();
        assert!(fs.looks_formatted());
        assert_eq!(fs.free(), (57, 32));
        // 20 Кбайт + 1 байт: 21 блок, 161 запись — два экстента.
        let data: Vec<u8> = (0..20 * 1024 + 1).map(|i| (i % 251) as u8).collect();
        fs.write(0, "big.dat", &data).unwrap();
        assert_eq!(fs.free(), (36, 30));
        let files = fs.list();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "BIG.DAT");
        assert_eq!(files[0].records, 161);
        let back = fs.read(0, "BIG.DAT").unwrap();
        assert_eq!(back.len(), 161 * 128);
        assert_eq!(&back[..data.len()], &data[..]);
        assert!(back[data.len()..].iter().all(|&b| b == 0x1A));
        // Замена файла освобождает старые блоки.
        fs.write(0, "BIG.DAT", b"x").unwrap();
        assert_eq!(fs.free(), (56, 31));
        assert!(fs.delete(0, "BIG.DAT").unwrap());
        assert!(!fs.delete(0, "BIG.DAT").unwrap());
        assert_eq!(fs.free(), (57, 32));
        // Нет места.
        assert!(fs.write(0, "HUGE", &vec![0; 58 * 1024]).is_err());
        assert_eq!(fs.free(), (57, 32), "при ошибке диск не тронут");
        // Пустой файл — одна запись каталога без блоков.
        fs.write(0, "EMPTY", &[]).unwrap();
        assert_eq!(fs.list()[0].records, 0);
        assert_eq!(fs.read(0, "EMPTY").unwrap(), Vec::<u8>::new());
    }
}
