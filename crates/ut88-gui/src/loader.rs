//! Загрузка программ: двоичные образы `.bin`, Intel HEX (`.ihx`, `.hex` с записями
//! `:LLAAAATT…`), текстовые дампы проекта (`AAAA XX XX …`, как `programs/*.hex` и
//! `monitor/monitor.hex`), а также таблица программ из `programs/README.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

/// Образ для загрузки в память: куски `(адрес, байты)` и, если известен, адрес пуска.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Image {
    /// Куски памяти.
    pub chunks: Vec<(u16, Vec<u8>)>,
    /// Адрес пуска (из таблицы программ или записи 03/05 Intel HEX).
    pub start: Option<u16>,
}

impl Image {
    /// Первый адрес образа.
    pub fn first_addr(&self) -> Option<u16> {
        self.chunks.first().map(|c| c.0)
    }

    /// Общее число байт.
    pub fn len(&self) -> usize {
        self.chunks.iter().map(|c| c.1.len()).sum()
    }

    /// Пустой образ.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Положить образ в память машины.
    pub fn load_into(&self, m: &mut ut88_core::Machine) {
        for (addr, data) in &self.chunks {
            m.load_bin(*addr, data);
        }
    }
}

/// Разобрать шестнадцатеричное число: `C000`, `C000H`, `0xC000`, `$C000`.
pub fn parse_hex_u16(s: &str) -> Result<u16> {
    let t = s.trim();
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .or_else(|| t.strip_prefix('$'))
        .unwrap_or(t);
    let t = t
        .strip_suffix('H')
        .or_else(|| t.strip_suffix('h'))
        .unwrap_or(t);
    if t.is_empty() {
        bail!("пустой адрес");
    }
    u16::from_str_radix(t, 16).map_err(|_| anyhow!("неверный шестнадцатеричный адрес «{s}»"))
}

/// Аргумент `файл[@адрес]` ключа `--load`.
pub fn parse_load_arg(s: &str) -> Result<(PathBuf, Option<u16>)> {
    match s.rsplit_once('@') {
        Some((file, addr)) if !file.is_empty() => {
            Ok((PathBuf::from(file), Some(parse_hex_u16(addr)?)))
        }
        _ => Ok((PathBuf::from(s), None)),
    }
}

/// Формат файла программы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Двоичный образ.
    Bin,
    /// Intel HEX.
    IntelHex,
    /// Текстовый дамп «AAAA XX XX …».
    Dump,
}

/// Определить формат по расширению и содержимому.
pub fn detect_format(path: &Path, data: &[u8]) -> Format {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "ihx" | "hex" | "txt" => {
            let text = String::from_utf8_lossy(data);
            let first = text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with(['#', ';']));
            if first.is_some_and(|l| l.starts_with(':')) {
                Format::IntelHex
            } else {
                Format::Dump
            }
        }
        _ => Format::Bin,
    }
}

/// Разобрать Intel HEX.
pub fn parse_intel_hex(text: &str) -> Result<Image> {
    let mut img = Image::default();
    let mut base: u32 = 0;
    for (no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rec = line
            .strip_prefix(':')
            .ok_or_else(|| anyhow!("строка {}: нет «:»", no + 1))?;
        if rec.len() % 2 != 0 || rec.len() < 10 {
            bail!("строка {}: неверная длина записи", no + 1);
        }
        let bytes = (0..rec.len() / 2)
            .map(|i| u8::from_str_radix(&rec[2 * i..2 * i + 2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|_| anyhow!("строка {}: не шестнадцатеричные цифры", no + 1))?;
        let n = usize::from(bytes[0]);
        if bytes.len() != n + 5 {
            bail!("строка {}: длина не совпадает с полем LL", no + 1);
        }
        let sum = bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        if sum != 0 {
            bail!("строка {}: неверная контрольная сумма", no + 1);
        }
        let addr = u16::from_be_bytes([bytes[1], bytes[2]]);
        let data = &bytes[4..4 + n];
        match bytes[3] {
            0x00 => {
                let a = (base + u32::from(addr)) as u16;
                push_chunk(&mut img.chunks, a, data);
            }
            0x01 => break,
            0x02 if n == 2 => base = u32::from(u16::from_be_bytes([data[0], data[1]])) << 4,
            0x04 if n == 2 => base = u32::from(u16::from_be_bytes([data[0], data[1]])) << 16,
            0x03 if n == 4 => img.start = Some(u16::from_be_bytes([data[2], data[3]])),
            0x05 if n == 4 => img.start = Some(u16::from_be_bytes([data[2], data[3]])),
            t => bail!("строка {}: неизвестная запись {t:02X}", no + 1),
        }
    }
    Ok(img)
}

/// Разобрать текстовый дамп: `AAAA XX XX …` в строке; пустые строки и комментарии
/// (`#`, `;`) пропускаются.
pub fn parse_dump(text: &str) -> Result<Image> {
    let mut img = Image::default();
    for (no, line) in text.lines().enumerate() {
        let line = line.split(['#', ';']).next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let addr_s = it.next().unwrap_or("").trim_end_matches(':');
        let addr = parse_hex_u16(addr_s).with_context(|| format!("строка {}", no + 1))?;
        let data = it
            .map(|b| {
                u8::from_str_radix(b, 16)
                    .map_err(|_| anyhow!("строка {}: неверный байт «{b}»", no + 1))
            })
            .collect::<Result<Vec<u8>>>()?;
        push_chunk(&mut img.chunks, addr, &data);
    }
    Ok(img)
}

fn push_chunk(chunks: &mut Vec<(u16, Vec<u8>)>, addr: u16, data: &[u8]) {
    if let Some((a, d)) = chunks.last_mut()
        && usize::from(*a) + d.len() == usize::from(addr)
    {
        d.extend_from_slice(data);
        return;
    }
    chunks.push((addr, data.to_vec()));
}

/// Загрузить файл программы.
///
/// Для `.bin` адрес берётся из `addr`, иначе из таблицы программ по имени файла
/// (`programs/README.md`), иначе — ошибка. Для HEX-форматов адреса заданы в файле;
/// `addr` (если указан) сдвигает образ так, чтобы первый кусок лёг по этому адресу.
pub fn load_file(path: &Path, addr: Option<u16>, table: &[ProgramInfo]) -> Result<Image> {
    let data = std::fs::read(path).with_context(|| format!("не читается {}", path.display()))?;
    let info = find_program(table, path);
    let mut img = match detect_format(path, &data) {
        Format::Bin => {
            let a = addr.or(info.map(|i| i.load)).ok_or_else(|| {
                anyhow!(
                    "для {} нужен адрес загрузки: --load файл@АДРЕС",
                    path.display()
                )
            })?;
            Image {
                chunks: vec![(a, data)],
                start: None,
            }
        }
        Format::IntelHex => parse_intel_hex(&String::from_utf8_lossy(&data))?,
        Format::Dump => parse_dump(&String::from_utf8_lossy(&data))?,
    };
    if let (Some(a), Some(first)) = (addr, img.first_addr())
        && a != first
    {
        let delta = a.wrapping_sub(first);
        for c in &mut img.chunks {
            c.0 = c.0.wrapping_add(delta);
        }
    }
    if img.start.is_none()
        && let Some(i) = info
    {
        img.start = i.start;
    }
    Ok(img)
}

/// Запись таблицы программ (`programs/README.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramInfo {
    /// Имя файла без расширения.
    pub name: String,
    /// Название.
    pub title: String,
    /// Выпуск журнала, страница.
    pub issue: String,
    /// Нужная конфигурация (текстом).
    pub config: String,
    /// Адрес загрузки.
    pub load: u16,
    /// Конечный адрес.
    pub end: Option<u16>,
    /// Адрес пуска.
    pub start: Option<u16>,
}

impl ProgramInfo {
    /// Программа для дисплейного модуля (полной конфигурации).
    pub fn needs_display(&self) -> bool {
        self.config.contains("дисплейный")
    }

    /// Данные к другой программе: имя основной программы.
    pub fn data_for(&self) -> Option<&str> {
        let rest = self.config.strip_prefix("данные к")?;
        Some(rest.trim().trim_matches('`'))
    }
}

/// Разобрать «Сводную таблицу» из `programs/README.md`.
pub fn parse_programs_readme(text: &str) -> Vec<ProgramInfo> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("| `") {
            continue;
        }
        let cols: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cols.len() < 7 {
            continue;
        }
        let name = cols[0].trim_matches('`').to_string();
        let Ok(load) = parse_hex_u16(cols[4]) else {
            continue;
        };
        out.push(ProgramInfo {
            name,
            title: cols[1].to_string(),
            issue: cols[2].to_string(),
            config: cols[3].to_string(),
            load,
            end: parse_hex_u16(cols[5]).ok(),
            start: parse_hex_u16(cols[6]).ok(),
        });
    }
    out
}

/// Найти программу таблицы по имени файла.
pub fn find_program<'a>(table: &'a [ProgramInfo], path: &Path) -> Option<&'a ProgramInfo> {
    let stem = path.file_stem()?.to_str()?;
    table.iter().find(|p| p.name == stem)
}

/// Найти папку `programs/`: рядом с текущим каталогом, рядом с исполняемым файлом (и
/// выше по дереву), в исходниках проекта.
pub fn find_programs_dir() -> Option<PathBuf> {
    let mut cands = vec![PathBuf::from("programs")];
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors().skip(1).take(4) {
            cands.push(anc.join("programs"));
        }
    }
    cands.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../programs"));
    cands.into_iter().find(|p| p.join("README.md").is_file())
}

/// Прочитать таблицу программ из папки (пусто, если README.md нет).
pub fn load_programs_table(dir: &Path) -> Vec<ProgramInfo> {
    std::fs::read_to_string(dir.join("README.md"))
        .map(|t| parse_programs_readme(&t))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_numbers() {
        assert_eq!(parse_hex_u16("C000").unwrap(), 0xC000);
        assert_eq!(parse_hex_u16("c03eH").unwrap(), 0xC03E);
        assert_eq!(parse_hex_u16("0x3000").unwrap(), 0x3000);
        assert_eq!(parse_hex_u16("$F800").unwrap(), 0xF800);
        assert!(parse_hex_u16("—").is_err());
        assert!(parse_hex_u16("10000").is_err());
    }

    #[test]
    fn load_args() {
        assert_eq!(
            parse_load_arg("a.bin@C000").unwrap(),
            (PathBuf::from("a.bin"), Some(0xC000))
        );
        assert_eq!(
            parse_load_arg("a.hex").unwrap(),
            (PathBuf::from("a.hex"), None)
        );
        assert!(parse_load_arg("a.bin@XYZ").is_err());
    }

    #[test]
    fn intel_hex() {
        let text = ":03C000003E01C935\n:02C0030076FFC6\n:0400000500003000C7\n:00000001FF\n";
        let img = parse_intel_hex(text).unwrap();
        assert_eq!(
            img.chunks,
            vec![(0xC000, vec![0x3E, 0x01, 0xC9, 0x76, 0xFF])]
        );
        assert_eq!(img.start, Some(0x3000));
        assert!(parse_intel_hex(":03C000003E01C9B4\n").is_err());
    }

    #[test]
    fn dump() {
        let img = parse_dump("# комментарий\nC000 3E 55 32\nC003 00 F4\n\nC100 01\n").unwrap();
        assert_eq!(
            img.chunks,
            vec![
                (0xC000, vec![0x3E, 0x55, 0x32, 0x00, 0xF4]),
                (0xC100, vec![1])
            ]
        );
        assert!(parse_dump("C000 3G").is_err());
    }

    #[test]
    fn detect() {
        assert_eq!(detect_format(Path::new("a.bin"), b""), Format::Bin);
        assert_eq!(
            detect_format(Path::new("a.ihx"), b":00000001FF"),
            Format::IntelHex
        );
        assert_eq!(detect_format(Path::new("a.hex"), b"C000 01"), Format::Dump);
    }

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn programs_readme() {
        let table = load_programs_table(&repo().join("programs"));
        assert!(table.len() >= 9, "{table:?}");
        let gamma = table.iter().find(|p| p.name == "gamma").unwrap();
        assert_eq!((gamma.load, gamma.start), (0xC000, Some(0xC03E)));
        let field = table.iter().find(|p| p.name == "labyrinth-field").unwrap();
        assert_eq!(field.start, None);
        assert_eq!(field.data_for(), Some("labyrinth"));
        let tetris = table.iter().find(|p| p.name == "tetris").unwrap();
        assert!(tetris.needs_display());
        assert_eq!(tetris.load, 0x3000);
    }

    #[test]
    fn bin_and_hex_files_agree() {
        let dir = repo().join("programs");
        let table = load_programs_table(&dir);
        for p in &table {
            let bin = load_file(&dir.join(format!("{}.bin", p.name)), None, &table).unwrap();
            let hex = load_file(&dir.join(format!("{}.hex", p.name)), None, &table).unwrap();
            assert_eq!(bin.first_addr(), Some(p.load), "{}", p.name);
            assert_eq!(hex.first_addr(), Some(p.load), "{}", p.name);
            assert_eq!(bin.start, p.start);
            assert_eq!(bin.chunks[0].1, hex.chunks.concat_bytes(), "{}", p.name);
        }
        assert!(
            load_file(&dir.join("gamma.bin"), Some(0xC100), &table)
                .unwrap()
                .first_addr()
                == Some(0xC100)
        );
    }

    trait Concat {
        fn concat_bytes(&self) -> Vec<u8>;
    }
    impl Concat for Vec<(u16, Vec<u8>)> {
        fn concat_bytes(&self) -> Vec<u8> {
            self.iter().flat_map(|c| c.1.clone()).collect()
        }
    }
}
