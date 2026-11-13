//! Образ памяти и выходные форматы: bin, дамп как в журнале (hex), Intel HEX (ihex).

use std::fmt::Write as _;
use std::path::Path;

/// Тип выходного файла.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Сплошной образ памяти.
    Bin,
    /// Дамп как в журнале: `AAAA XX XX …`.
    Hex,
    /// Intel HEX.
    Ihex,
    /// Объектный файл `.link`.
    Link,
}

impl Format {
    /// Имя типа для `-t`.
    pub fn name(self) -> &'static str {
        match self {
            Format::Bin => "bin",
            Format::Hex => "hex",
            Format::Ihex => "ihex",
            Format::Link => "link",
        }
    }

    /// Разобрать значение `-t`.
    pub fn from_type(t: &str) -> Result<Self, String> {
        match t.to_ascii_lowercase().as_str() {
            "bin" => Ok(Format::Bin),
            "hex" => Ok(Format::Hex),
            "ihex" | "ihx" => Ok(Format::Ihex),
            "link" => Ok(Format::Link),
            _ => Err(format!(
                "неизвестный тип выхода «{t}» (допустимо: bin, hex, ihex, link)"
            )),
        }
    }

    /// Тип по расширению выходного файла.
    pub fn from_path(p: &Path) -> Result<Self, String> {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        match ext.as_deref() {
            Some("bin" | "rom") => Ok(Format::Bin),
            Some("hex") => Ok(Format::Hex),
            Some("ihx" | "ihex") => Ok(Format::Ihex),
            Some("link") => Ok(Format::Link),
            _ => Err(format!(
                "не понять тип выходного файла по имени «{}»: расширение должно быть .bin, .rom, \
                 .hex, .ihx, .ihex или .link, либо укажите тип ключом -t bin|hex|ihex|link",
                p.display()
            )),
        }
    }

    /// Тип выхода: `-t`, если задан, иначе по расширению.
    pub fn choose(t: Option<&str>, out: &Path) -> Result<Self, String> {
        match t {
            Some(t) => Self::from_type(t),
            None => Self::from_path(out),
        }
    }
}

/// Образ памяти: непрерывные куски по возрастанию адресов и адрес пуска.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Image {
    /// Куски `(адрес, байты)`, не перекрываются и не соприкасаются.
    pub chunks: Vec<(u16, Vec<u8>)>,
    /// Адрес пуска (из `END адрес`).
    pub entry: Option<u16>,
}

impl Image {
    /// Сплошной образ от наименьшего до наибольшего занятого адреса; пропуски — `fill`.
    pub fn to_bin(&self, fill: u8) -> Vec<u8> {
        let (Some(first), Some(last)) = (self.chunks.first(), self.chunks.last()) else {
            return Vec::new();
        };
        let start = usize::from(first.0);
        let end = usize::from(last.0) + last.1.len();
        let mut out = vec![fill; end - start];
        for (a, d) in &self.chunks {
            let o = usize::from(*a) - start;
            out[o..o + d.len()].copy_from_slice(d);
        }
        out
    }

    /// Дамп как в журнале (`monitor/monitor.hex`): строки `AAAA XX XX …` по 16 байт,
    /// строки начинаются на границе 16 байт (кроме первой строки куска, если кусок
    /// начинается не на границе).
    pub fn to_dump(&self) -> String {
        let mut s = String::new();
        for (addr, data) in &self.chunks {
            let mut a = usize::from(*addr);
            let mut rest = data.as_slice();
            while !rest.is_empty() {
                let n = (16 - a % 16).min(rest.len());
                let _ = write!(s, "{a:04X}");
                for b in &rest[..n] {
                    let _ = write!(s, " {b:02X}");
                }
                s.push('\n');
                a += n;
                rest = &rest[n..];
            }
        }
        s
    }

    /// Intel HEX: записи 00 по 16 байт (по границам 16), запись 03 с адресом пуска (если
    /// есть), запись 01 в конце.
    pub fn to_ihex(&self) -> String {
        let mut s = String::new();
        let mut rec = |typ: u8, addr: u16, data: &[u8]| {
            let mut sum = data.len() as u8;
            sum = sum
                .wrapping_add((addr >> 8) as u8)
                .wrapping_add(addr as u8)
                .wrapping_add(typ);
            let _ = write!(s, ":{:02X}{addr:04X}{typ:02X}", data.len());
            for b in data {
                sum = sum.wrapping_add(*b);
                let _ = write!(s, "{b:02X}");
            }
            let _ = writeln!(s, "{:02X}", sum.wrapping_neg());
        };
        for (addr, data) in &self.chunks {
            let mut a = usize::from(*addr);
            let mut rest = data.as_slice();
            while !rest.is_empty() {
                let n = (16 - a % 16).min(rest.len());
                rec(0x00, a as u16, &rest[..n]);
                a += n;
                rest = &rest[n..];
            }
        }
        if let Some(e) = self.entry {
            rec(0x03, 0, &[0, 0, (e >> 8) as u8, e as u8]);
        }
        rec(0x01, 0, &[]);
        s
    }
}

/// Сборщик образа памяти с проверкой перекрытий и выхода за 64 Кбайт.
#[derive(Clone, Debug)]
pub struct ImageBuilder {
    mem: Vec<u8>,
    used: Vec<bool>,
}

impl Default for ImageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageBuilder {
    /// Пустой образ.
    pub fn new() -> Self {
        Self {
            mem: vec![0; 0x10000],
            used: vec![false; 0x10000],
        }
    }

    /// Положить байты с адреса `addr`. Ошибка — выход за 64 Кбайт или перекрытие уже
    /// занятых адресов.
    pub fn put(&mut self, addr: i64, bytes: &[u8]) -> Result<(), String> {
        let end = addr + bytes.len() as i64;
        if addr < 0 || end > 0x10000 {
            return Err(format!(
                "данные {}–{} выходят за пределы адресного пространства 0000H–FFFFH",
                fmt_addr(addr),
                fmt_addr(end - 1)
            ));
        }
        let a = addr as usize;
        if let Some(i) = self.used[a..a + bytes.len()].iter().position(|&u| u) {
            return Err(format!(
                "перекрытие областей памяти по адресу {:04X}H",
                a + i
            ));
        }
        self.mem[a..a + bytes.len()].copy_from_slice(bytes);
        self.used[a..a + bytes.len()].fill(true);
        Ok(())
    }

    /// Перезаписать уже положенные байты (применение перемещения); вне 64 Кбайт — пропуск.
    pub fn patch(&mut self, addr: i64, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            let a = addr + i as i64;
            if (0..0x10000).contains(&a) {
                self.mem[a as usize] = *b;
            }
        }
    }

    /// Готовый образ.
    pub fn finish(self, entry: Option<u16>) -> Image {
        let mut chunks: Vec<(u16, Vec<u8>)> = Vec::new();
        let mut a = 0usize;
        while a < 0x10000 {
            if !self.used[a] {
                a += 1;
                continue;
            }
            let s = a;
            while a < 0x10000 && self.used[a] {
                a += 1;
            }
            chunks.push((s as u16, self.mem[s..a].to_vec()));
        }
        Image { chunks, entry }
    }
}

fn fmt_addr(a: i64) -> String {
    if (0..=0xFFFF).contains(&a) {
        format!("{a:04X}H")
    } else {
        format!("{a:X}H")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_format() {
        let p = Path::new;
        assert_eq!(Format::choose(None, p("a.bin")), Ok(Format::Bin));
        assert_eq!(Format::choose(None, p("a.ROM")), Ok(Format::Bin));
        assert_eq!(Format::choose(None, p("x/a.hex")), Ok(Format::Hex));
        assert_eq!(Format::choose(None, p("a.ihx")), Ok(Format::Ihex));
        assert_eq!(Format::choose(None, p("a.ihex")), Ok(Format::Ihex));
        assert_eq!(Format::choose(None, p("a.link")), Ok(Format::Link));
        assert!(Format::choose(None, p("a.txt")).is_err());
        assert!(Format::choose(None, p("a")).is_err());
        assert_eq!(Format::choose(Some("hex"), p("a.bin")), Ok(Format::Hex));
        assert_eq!(Format::choose(Some("bin"), p("a.txt")), Ok(Format::Bin));
        assert_eq!(Format::choose(Some("IHEX"), p("a")), Ok(Format::Ihex));
        assert!(Format::choose(Some("elf"), p("a.bin")).is_err());
    }

    fn sample() -> Image {
        let mut b = ImageBuilder::new();
        b.put(0xC00E, &[1, 2, 3, 4]).unwrap();
        b.put(0xC020, &[0xAA]).unwrap();
        b.finish(Some(0xC00E))
    }

    #[test]
    fn bin_dump_ihex() {
        let img = sample();
        let bin = img.to_bin(0);
        assert_eq!(bin.len(), 0x13);
        assert_eq!(&bin[..4], &[1, 2, 3, 4]);
        assert_eq!(bin[0x12], 0xAA);
        assert_eq!(img.to_bin(0xFF)[5], 0xFF);
        assert_eq!(img.to_dump(), "C00E 01 02\nC010 03 04\nC020 AA\n");
        assert_eq!(
            img.to_ihex(),
            concat!(
                ":02C00E0001022D\n",
                ":02C010000304",
                "27\n",
                ":01C02000AA75\n",
                ":040000030000C00E2B\n",
                ":00000001FF\n"
            )
        );
    }

    #[test]
    fn overlap_and_overflow() {
        let mut b = ImageBuilder::new();
        b.put(0x100, &[1, 2]).unwrap();
        assert!(b.put(0x101, &[3]).unwrap_err().contains("0101H"));
        assert!(b.put(0xFFFF, &[1, 2]).is_err());
        assert_eq!(Image::default().to_bin(0), Vec::<u8>::new());
    }
}
