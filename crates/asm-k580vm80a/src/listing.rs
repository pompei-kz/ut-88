//! Листинг (`-l ФАЙЛ.lst`) в формате `monitor/monitor.lst`.
//!
//! Каждая строка исходного текста (после препроцессора — с раскрытиями макросов и
//! включёнными файлами) выводится так:
//!
//! ```text
//! АДРЕС КОДЫ             МП ИСХОДНАЯ СТРОКА
//! 0000  31 EE C3            START:      LXI     SP,STACK
//! 03FA  FF FF FF FF                     DB      0FFH,0FFH,0FFH,0FFH,0FFH,0FFH
//! 03FE  FF FF
//!      =9000                DISP_D      EQU     9000H
//! ```
//!
//! * столбцы 1–4 — адрес (у строки без кодов — только если на ней метка или DS), 5-й —
//!   признак сегмента: пробел — абсолютный адрес, `'` — смещение в CSEG, `"` — в DSEG;
//! * дальше — до 4 байт кода; длинные DB/DW/DS переносятся на следующие строки
//!   (адрес и коды, без текста);
//! * у EQU/SET вместо адреса — `=ЗНАЧЕНИЕ` (с тем же признаком сегмента, `*` — внешний
//!   символ);
//! * 19-й столбец — глубина INCLUDE (пусто — основной файл, `1`…`9`), 20-й — `+` у строк
//!   раскрытия макроса/REPT, `-` у строк ложной ветки IF (они не собираются);
//! * исходная строка — с 21-го столбца, как написана.
//!
//! В конце — таблица символов по алфавиту: имя, значение, сегмент (абсолютное, CSEG,
//! DSEG, внешнее), вид и где определён.

use std::fmt::Write as _;

use crate::assembler::{Assembly, LineRecord};
use crate::expr::{Part, RelocTarget, Value};
use crate::ir::{SegKind, SymbolKind};
use crate::source::LineKind;

/// Сколько байт кода в строке листинга.
pub const BYTES_PER_LINE: usize = 4;
/// Ширина поля перед исходной строкой.
const PREFIX: usize = 20;

fn seg_mark(k: SegKind) -> char {
    match k {
        SegKind::Abs => ' ',
        SegKind::Code => '\'',
        SegKind::Data => '"',
    }
}

fn value_mark(v: &Value) -> char {
    match &v.rel {
        None => ' ',
        Some(r) => match &r.target {
            RelocTarget::Segment(k) => seg_mark(*k),
            RelocTarget::Extern(_) => '*',
        },
    }
}

fn codes(seg: SegKind, addr: u32, bytes: &[u8]) -> String {
    let mut s = format!("{:04X}{}", addr & 0xFFFF, seg_mark(seg));
    for b in bytes {
        let _ = write!(s, " {b:02X}");
    }
    s.trim_end().to_string()
}

fn markers(r: &LineRecord) -> String {
    let inc = match r.src.include_depth {
        0 => ' ',
        d @ 1..=9 => char::from_digit(d as u32, 10).unwrap_or('*'),
        _ => '*',
    };
    let kind = if r.src.kind == LineKind::Skipped {
        '-'
    } else if r.src.expanded {
        '+'
    } else {
        ' '
    };
    format!("{inc}{kind}")
}

fn line_out(out: &mut String, head: &str, r: &LineRecord) {
    let pad = (PREFIX - 2).saturating_sub(head.chars().count());
    let _ = writeln!(out, "{head}{}{}{}", " ".repeat(pad), markers(r), r.src.text);
}

/// Значение для таблицы символов: число и «сегмент».
fn describe(v: &Value) -> (String, String) {
    let num = format!("{:04X}", v.num & 0xFFFF);
    let seg = match &v.rel {
        None => "абс.".to_string(),
        Some(r) => {
            let base = match &r.target {
                RelocTarget::Segment(k) => k.name().to_string(),
                RelocTarget::Extern(n) => format!("внешн. {n}"),
            };
            match r.part {
                Part::Word => base,
                p => format!("{} {base}", p.name()),
            }
        }
    };
    (num, seg)
}

fn kind_name(k: SymbolKind) -> &'static str {
    match k {
        SymbolKind::Label => "метка",
        SymbolKind::Equ => "EQU",
        SymbolKind::Set => "SET",
        SymbolKind::Extern => "EXTRN",
        SymbolKind::Define => "-D",
    }
}

/// Текст листинга.
pub fn listing(a: &Assembly) -> String {
    let mut out = String::new();
    for r in &a.lines {
        if let Some(v) = &r.value {
            let head = format!("     ={:04X}{}", v.num & 0xFFFF, value_mark(v));
            line_out(&mut out, &head, r);
            continue;
        }
        if r.bytes.is_empty() {
            let head = r.addr.map_or(String::new(), |(seg, a)| codes(seg, a, &[]));
            line_out(&mut out, &head, r);
            continue;
        }
        let mut chunks = r.bytes.chunks(BYTES_PER_LINE);
        let first = chunks.next().unwrap_or(&[]);
        line_out(&mut out, &codes(r.seg, r.offset, first), r);
        let mut addr = r.offset + first.len() as u32;
        for c in chunks {
            let _ = writeln!(out, "{}", codes(r.seg, addr, c));
            addr += c.len() as u32;
        }
    }
    symbol_table(a, &mut out);
    out
}

fn symbol_table(a: &Assembly, out: &mut String) {
    let syms = &a.module.symbols;
    let _ = writeln!(out);
    let _ = writeln!(out, "СИМВОЛЫ ({})", syms.len());
    if syms.is_empty() {
        return;
    }
    let _ = writeln!(out);
    let rows: Vec<[String; 5]> = syms
        .iter()
        .map(|s| {
            let (num, seg) = describe(&s.value);
            let mut kind = kind_name(s.kind).to_string();
            if s.public {
                kind.push_str(" PUBLIC");
            }
            let at = s.loc.as_ref().map_or("командная строка".to_string(), |l| {
                format!("{}:{}", l.file, l.line)
            });
            [s.name.clone(), num, seg, kind, at]
        })
        .collect();
    let header = ["ИМЯ", "ЗНАЧ", "СЕГМЕНТ", "ВИД", "ГДЕ ОПРЕДЕЛЁН"].map(String::from);
    let mut w = [0usize; 5];
    for row in rows.iter().chain(std::iter::once(&header)) {
        for (i, c) in row.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let fmt_row = |row: &[String; 5]| {
        let mut s = String::new();
        for (i, c) in row.iter().enumerate() {
            s.push_str(c);
            if i + 1 < row.len() {
                s.push_str(&" ".repeat(w[i] - c.chars().count() + 2));
            }
        }
        s.trim_end().to_string()
    };
    let _ = writeln!(out, "{}", fmt_row(&header));
    for row in &rows {
        let _ = writeln!(out, "{}", fmt_row(row));
    }
}

impl Assembly {
    /// Листинг: адреса, коды, исходные строки и таблица символов.
    pub fn listing(&self) -> String {
        listing(self)
    }
}
