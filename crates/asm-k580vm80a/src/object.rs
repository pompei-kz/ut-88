//! Объектный файл `.link`: запись [`Module`] в текст и чтение обратно.
//!
//! Формат текстовый, построчный, читается глазами. Первая строка — заголовок с версией
//! `UT88LINK 1`, последняя — `END`. Пустые строки и строки, начинающиеся с `;`, —
//! комментарии. Поля разделяются пробелами, числа — шестнадцатеричные без суффикса.
//!
//! ```text
//! UT88LINK 1
//! ; объектный модуль ассемблера КР580ВМ80А (ЮТ-88)
//! MODULE hello
//! SEGMENT CSEG 0009
//! DATA CSEG 0000 21 00 00 CD 00 00 C3 00 00
//! SEGMENT DSEG 0010
//! EXTERN PRINT AT hello.asm:3:15
//! PUBLIC START CSEG 0000 AT hello.asm:4:16
//! PUBLIC LIMIT ASEG 00FF AT hello.asm:4:22
//! RELOC CSEG 0001 WORD DSEG +0000 AT hello.asm:6:19
//! RELOC CSEG 0004 WORD EXT PRINT +0000 AT hello.asm:7:19
//! RELOC CSEG 0007 WORD CSEG +0000 AT hello.asm:8:19
//! ENTRY CSEG 0000
//! END
//! ```
//!
//! Строки:
//!
//! * `MODULE имя` — имя модуля (`NAME`), необязательна;
//! * `SEGMENT ASEG|CSEG|DSEG размер` — сегмент и его размер (с резервом `DS`);
//! * `DATA сегмент смещение XX XX …` — байты с указанного смещения (до 16 в строке);
//! * `EXTERN имя [AT место]` — импорт;
//! * `PUBLIC имя сегмент смещение [AT место]` — экспорт (`ASEG` — абсолютное значение);
//! * `PUBLIC имя EXT другое ±n [AT место]` — экспорт значения «внешний символ ± n»;
//! * `RELOC сегмент смещение WORD|LOW|HIGH цель ±n [AT место]` — перемещение, цель —
//!   `CSEG`, `DSEG` или `EXT имя`;
//! * `ENTRY сегмент смещение` — адрес пуска (из `END адрес`);
//! * `END` — конец файла (без неё файл считается обрезанным).
//!
//! `AT файл:строка:столбец` — место в исходном тексте (для сообщений компоновщика), до
//! конца строки. В данных на месте перемещения лежит значение при базе 0.

use std::fmt::Write as _;
use std::sync::Arc;

use crate::error::{AsmError, Loc};
use crate::expr::{Part, Rel, RelocTarget, Value};
use crate::ir::{Chunk, Extern, Module, Public, Reloc, SegKind};

/// Подпись формата в первой строке.
pub const MAGIC: &str = "UT88LINK";
/// Версия формата, которую пишет и читает этот модуль.
pub const VERSION: u32 = 1;

const BYTES_PER_LINE: usize = 16;

/// Записать модуль в текст `.link`.
pub fn write(m: &Module) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{MAGIC} {VERSION}");
    s.push_str("; объектный модуль ассемблера КР580ВМ80А (ЮТ-88), читает link-k580vm80a\n");
    if let Some(n) = &m.name {
        let _ = writeln!(s, "MODULE {n}");
    }
    for seg in &m.segments {
        if seg.is_empty() {
            continue;
        }
        let k = seg.kind.name();
        let _ = writeln!(s, "SEGMENT {k} {:04X}", seg.size);
        for ch in &seg.chunks {
            for (i, part) in ch.bytes.chunks(BYTES_PER_LINE).enumerate() {
                let off = usize::from(ch.offset) + i * BYTES_PER_LINE;
                let _ = write!(s, "DATA {k} {off:04X}");
                for b in part {
                    let _ = write!(s, " {b:02X}");
                }
                s.push('\n');
            }
        }
    }
    for e in &m.externs {
        let _ = writeln!(s, "EXTERN {}{}", e.name, at(&e.loc));
    }
    for p in &m.publics {
        let _ = writeln!(
            s,
            "PUBLIC {} {}{}",
            p.name,
            value_text(&p.value),
            at(&p.loc)
        );
    }
    for r in &m.relocs {
        let target = match &r.target {
            RelocTarget::Segment(k) => k.name().to_string(),
            RelocTarget::Extern(n) => format!("EXT {n}"),
        };
        let _ = writeln!(
            s,
            "RELOC {} {:04X} {} {target} {}{}",
            r.seg.name(),
            r.offset,
            r.part.name(),
            signed(r.addend),
            at(&r.loc)
        );
    }
    if let Some(e) = &m.entry {
        let _ = writeln!(s, "ENTRY {}", value_text(e));
    }
    s.push_str("END\n");
    s
}

fn at(loc: &Option<Loc>) -> String {
    match loc {
        Some(l) => format!(" AT {l}"),
        None => String::new(),
    }
}

/// Число: `0000`…`FFFF` четырьмя цифрами, иначе как есть (со знаком минус).
fn number(n: i64) -> String {
    if n < 0 {
        format!("-{:04X}", n.unsigned_abs())
    } else {
        format!("{n:04X}")
    }
}

/// Константа со знаком: `+0005`, `-0003`.
fn signed(n: i64) -> String {
    if n < 0 {
        number(n)
    } else {
        format!("+{}", number(n))
    }
}

fn value_text(v: &Value) -> String {
    match &v.rel {
        None => format!("ASEG {}", number(v.num)),
        Some(Rel {
            target: RelocTarget::Segment(k),
            ..
        }) => format!("{} {}", k.name(), number(v.num)),
        Some(Rel {
            target: RelocTarget::Extern(n),
            ..
        }) => format!("EXT {n} {}", signed(v.num)),
    }
}

/// Слово строки с позицией (в байтах) в строке.
#[derive(Clone, Copy)]
struct Field<'a> {
    text: &'a str,
    start: usize,
}

struct LineReader<'a> {
    file: &'a Arc<str>,
    line_no: usize,
    line: &'a str,
    fields: Vec<Field<'a>>,
    pos: usize,
}

impl<'a> LineReader<'a> {
    fn new(file: &'a Arc<str>, line_no: usize, line: &'a str) -> Self {
        let mut fields = Vec::new();
        let mut start = None;
        for (i, c) in line.char_indices() {
            if c.is_whitespace() {
                if let Some(s) = start.take() {
                    fields.push(Field {
                        text: &line[s..i],
                        start: s,
                    });
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        if let Some(s) = start {
            fields.push(Field {
                text: &line[s..],
                start: s,
            });
        }
        Self {
            file,
            line_no,
            line,
            fields,
            pos: 0,
        }
    }

    fn loc_at(&self, byte: usize) -> Loc {
        let col = self.line[..byte.min(self.line.len())].chars().count() + 1;
        Loc::new(self.file.clone(), self.line_no, col)
    }

    /// Ошибка у поля `i` (или в конце строки, если поля нет).
    fn err_at(&self, i: usize, msg: impl Into<String>) -> AsmError {
        let byte = self
            .fields
            .get(i)
            .map_or(self.line.trim_end().len(), |f| f.start);
        AsmError::at(&self.loc_at(byte), msg)
    }

    fn next(&mut self, what: &str) -> Result<Field<'a>, AsmError> {
        match self.fields.get(self.pos) {
            Some(f) => {
                self.pos += 1;
                Ok(*f)
            }
            None => Err(self.err_at(self.pos, format!("ожидается {what}"))),
        }
    }

    fn last(&self) -> usize {
        self.pos - 1
    }

    fn seg(&mut self) -> Result<SegKind, AsmError> {
        let f = self.next("сегмент (ASEG, CSEG или DSEG)")?;
        SegKind::from_name(f.text).ok_or_else(|| {
            self.err_at(
                self.last(),
                format!(
                    "неизвестный сегмент «{}» (допустимо: ASEG, CSEG, DSEG)",
                    f.text
                ),
            )
        })
    }

    fn hex(&mut self, what: &str, max: i64) -> Result<i64, AsmError> {
        let f = self.next(what)?;
        let v = parse_number(f.text).ok_or_else(|| {
            self.err_at(
                self.last(),
                format!("{what}: неверное шестнадцатеричное число «{}»", f.text),
            )
        })?;
        if !(0..=max).contains(&v) {
            return Err(self.err_at(
                self.last(),
                format!("{what}: значение {} вне 0…{max:X}", f.text),
            ));
        }
        Ok(v)
    }

    fn any_number(&mut self, what: &str) -> Result<i64, AsmError> {
        let f = self.next(what)?;
        parse_number(f.text).ok_or_else(|| {
            self.err_at(
                self.last(),
                format!("{what}: неверное шестнадцатеричное число «{}»", f.text),
            )
        })
    }

    fn signed(&mut self, what: &str) -> Result<i64, AsmError> {
        let f = self.next(what)?;
        if !f.text.starts_with(['+', '-']) {
            return Err(self.err_at(
                self.last(),
                format!("{what}: ожидается число со знаком (+0005, -0003)"),
            ));
        }
        parse_number(f.text)
            .ok_or_else(|| self.err_at(self.last(), format!("{what}: неверное число «{}»", f.text)))
    }

    fn name(&mut self, what: &str) -> Result<String, AsmError> {
        Ok(self.next(what)?.text.to_string())
    }

    /// Значение: `ASEG|CSEG|DSEG смещение` или `EXT имя ±n`.
    fn value(&mut self) -> Result<Value, AsmError> {
        let f = self.next("сегмент или EXT")?;
        if f.text == "EXT" {
            let n = self.name("имя внешнего символа")?;
            let k = self.signed("константа")?;
            let mut v = Value::external(&n);
            v.num = k;
            return Ok(v);
        }
        self.pos -= 1;
        let seg = self.seg()?;
        let off = self.any_number("смещение")?;
        Ok(Value::in_segment(seg, off))
    }

    /// Необязательное `AT файл:строка:столбец` до конца строки и проверка, что лишнего нет.
    fn tail(&mut self, allow_at: bool) -> Result<Option<Loc>, AsmError> {
        let Some(f) = self.fields.get(self.pos).copied() else {
            return Ok(None);
        };
        if !allow_at || f.text != "AT" {
            return Err(self.err_at(self.pos, format!("лишнее «{}» в конце строки", f.text)));
        }
        let rest = self
            .fields
            .get(self.pos + 1)
            .ok_or_else(|| self.err_at(self.pos + 1, "AT: ожидается место файл:строка:столбец"))?;
        let text = self.line[rest.start..].trim_end();
        let bad = || {
            self.err_at(
                self.pos + 1,
                format!("AT: неверное место «{text}» (нужно файл:строка:столбец)"),
            )
        };
        let mut it = text.rsplitn(3, ':');
        let col = it.next().and_then(|c| c.parse::<usize>().ok());
        let line = it.next().and_then(|c| c.parse::<usize>().ok());
        let file = it.next().filter(|f| !f.is_empty());
        match (file, line, col) {
            (Some(f), Some(l), Some(c)) => Ok(Some(Loc::new(Arc::from(f), l, c))),
            _ => Err(bad()),
        }
    }
}

/// Шестнадцатеричное число с необязательным знаком.
fn parse_number(s: &str) -> Option<i64> {
    let (neg, d) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if d.is_empty() || !d.bytes().all(|b| b.is_ascii_hexdigit()) || d.len() > 15 {
        return None;
    }
    let v = i64::from_str_radix(d, 16).ok()?;
    Some(if neg { -v } else { v })
}

/// Прочитать текст `.link`. `file` — имя файла для сообщений (`файл:строка:столбец`).
pub fn read(file: &str, text: &str) -> Result<Module, Vec<AsmError>> {
    let file: Arc<str> = Arc::from(file);
    let mut errors = Vec::new();
    let mut m = Module::default();
    let mut header = false;
    let mut ended = false;
    let mut name_seen = false;
    let mut seg_seen = [false; 3];
    let mut entry_seen = false;
    let mut last_line = 0;
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        last_line = line_no;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') {
            continue;
        }
        let mut r = LineReader::new(&file, line_no, line);
        if ended {
            errors.push(r.err_at(0, "строки после END"));
            break;
        }
        if !header {
            let magic = r.fields[0].text;
            if magic != MAGIC {
                errors.push(r.err_at(
                    0,
                    format!(
                        "это не объектный файл .link: первая строка должна быть «{MAGIC} {VERSION}»"
                    ),
                ));
                return Err(errors);
            }
            match r.fields.get(1).map(|f| f.text) {
                Some(v) if v == VERSION.to_string() && r.fields.len() == 2 => {}
                Some(v) => {
                    errors.push(r.err_at(
                        1,
                        format!(
                            "неподдерживаемая версия формата .link «{v}» (поддерживается {VERSION}); \
                             пересоберите модуль этой версией asm-k580vm80a"
                        ),
                    ));
                    return Err(errors);
                }
                None => {
                    errors.push(r.err_at(1, "в заголовке нет версии формата"));
                    return Err(errors);
                }
            }
            header = true;
            continue;
        }
        let res = read_line(
            &mut r,
            &mut m,
            &mut name_seen,
            &mut seg_seen,
            &mut entry_seen,
        );
        match res {
            Ok(true) => ended = true,
            Ok(false) => {}
            Err(e) => errors.push(e),
        }
    }
    if !header {
        errors.push(AsmError::at(
            &Loc::new(file.clone(), 1, 1),
            format!("пустой файл: нет заголовка «{MAGIC} {VERSION}»"),
        ));
        return Err(errors);
    }
    if !ended && errors.is_empty() {
        errors.push(AsmError::at(
            &Loc::new(file.clone(), last_line + 1, 1),
            "файл обрезан: нет завершающей строки END",
        ));
    }
    if errors.is_empty() {
        check(&file, &m, &mut errors);
    }
    if errors.is_empty() {
        Ok(m)
    } else {
        Err(errors)
    }
}

/// Разобрать одну строку; `Ok(true)` — строка `END`.
fn read_line(
    r: &mut LineReader<'_>,
    m: &mut Module,
    name_seen: &mut bool,
    seg_seen: &mut [bool; 3],
    entry_seen: &mut bool,
) -> Result<bool, AsmError> {
    let kw = r.next("ключевое слово")?;
    match kw.text {
        "MODULE" => {
            if *name_seen {
                return Err(r.err_at(0, "повторная строка MODULE"));
            }
            let f = r.next("имя модуля")?;
            m.name = Some(r.line[f.start..].trim_end().to_string());
            *name_seen = true;
            return Ok(false);
        }
        "SEGMENT" => {
            let k = r.seg()?;
            if seg_seen[k.index()] {
                return Err(r.err_at(1, format!("повторная строка SEGMENT {}", k.name())));
            }
            let size = r.hex("размер сегмента", 0x10000)?;
            seg_seen[k.index()] = true;
            m.segments[k.index()].size = size as u32;
        }
        "DATA" => {
            let k = r.seg()?;
            if !seg_seen[k.index()] {
                return Err(r.err_at(1, format!("DATA {0} до строки SEGMENT {0}", k.name())));
            }
            let off = r.hex("смещение", 0xFFFF)?;
            let mut bytes = Vec::new();
            while r.pos < r.fields.len() {
                let f = r.next("байт")?;
                match (f.text.len(), u8::from_str_radix(f.text, 16)) {
                    (2, Ok(b)) => bytes.push(b),
                    _ => {
                        return Err(r.err_at(
                            r.last(),
                            format!(
                                "неверный байт «{}» (нужны две шестнадцатеричные цифры)",
                                f.text
                            ),
                        ));
                    }
                }
            }
            if bytes.is_empty() {
                return Err(r.err_at(r.pos, "DATA без байтов"));
            }
            let seg = &mut m.segments[k.index()];
            let end = off as usize + bytes.len();
            if end > 0x10000 {
                return Err(r.err_at(2, "данные выходят за 64 Кбайт"));
            }
            if k != SegKind::Abs && end > seg.size as usize {
                return Err(r.err_at(
                    2,
                    format!(
                        "данные {:04X}…{:04X} выходят за размер сегмента {} ({:04X})",
                        off,
                        end - 1,
                        k.name(),
                        seg.size
                    ),
                ));
            }
            match seg.chunks.last_mut() {
                Some(c) if usize::from(c.offset) + c.bytes.len() == off as usize => {
                    c.bytes.extend_from_slice(&bytes);
                }
                Some(c) if usize::from(c.offset) + c.bytes.len() > off as usize => {
                    return Err(r.err_at(
                        2,
                        format!(
                            "DATA {} {:04X}: смещения должны идти по возрастанию без перекрытий",
                            k.name(),
                            off
                        ),
                    ));
                }
                _ => seg.chunks.push(Chunk {
                    offset: off as u16,
                    bytes,
                }),
            }
        }
        "EXTERN" => {
            let name = r.name("имя внешнего символа")?;
            if m.externs.iter().any(|e| e.name.eq_ignore_ascii_case(&name)) {
                return Err(r.err_at(1, format!("повторный EXTERN {name}")));
            }
            let loc = r.tail(true)?;
            m.externs.push(Extern { name, loc });
            return Ok(false);
        }
        "PUBLIC" => {
            let name = r.name("имя символа")?;
            if m.publics.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                return Err(r.err_at(1, format!("повторный PUBLIC {name}")));
            }
            let value = r.value()?;
            let loc = r.tail(true)?;
            m.publics.push(Public { name, value, loc });
            return Ok(false);
        }
        "RELOC" => {
            let seg = r.seg()?;
            let offset = r.hex("смещение", 0xFFFF)? as u16;
            let f = r.next("вид перемещения (WORD, LOW, HIGH)")?;
            let part = match f.text {
                "WORD" => Part::Word,
                "LOW" => Part::Low,
                "HIGH" => Part::High,
                t => {
                    return Err(r.err_at(
                        r.last(),
                        format!("неизвестный вид перемещения «{t}» (допустимо: WORD, LOW, HIGH)"),
                    ));
                }
            };
            let f = r.next("цель перемещения (CSEG, DSEG или EXT имя)")?;
            let target = match f.text {
                "EXT" => RelocTarget::Extern(r.name("имя внешнего символа")?),
                "CSEG" => RelocTarget::Segment(SegKind::Code),
                "DSEG" => RelocTarget::Segment(SegKind::Data),
                t => {
                    return Err(r.err_at(
                        r.last(),
                        format!("неверная цель перемещения «{t}» (допустимо: CSEG, DSEG, EXT имя)"),
                    ));
                }
            };
            let addend = r.signed("константа")?;
            let loc = r.tail(true)?;
            m.relocs.push(Reloc {
                seg,
                offset,
                part,
                target,
                addend,
                loc,
            });
            return Ok(false);
        }
        "ENTRY" => {
            if *entry_seen {
                return Err(r.err_at(0, "повторная строка ENTRY"));
            }
            let v = r.value()?;
            if matches!(
                v.rel,
                Some(Rel {
                    target: RelocTarget::Extern(_),
                    ..
                })
            ) {
                return Err(r.err_at(1, "ENTRY: адрес пуска не может быть внешним символом"));
            }
            m.entry = Some(v);
            *entry_seen = true;
        }
        "END" => {
            r.tail(false)?;
            return Ok(true);
        }
        t => {
            return Err(r.err_at(
                0,
                format!(
                    "неизвестная строка «{t}» (ожидается MODULE, SEGMENT, DATA, EXTERN, PUBLIC, \
                     RELOC, ENTRY или END)"
                ),
            ));
        }
    }
    r.tail(false)?;
    Ok(false)
}

/// Проверки модуля целиком: перемещения лежат на данных и ссылаются на объявленные EXTERN.
fn check(file: &Arc<str>, m: &Module, errors: &mut Vec<AsmError>) {
    let whole = |msg: String| AsmError::plain(format!("{file}: {msg}"));
    for r in &m.relocs {
        let seg = m.segment(r.seg);
        let start = usize::from(r.offset);
        let end = start + usize::from(r.width());
        let covered = seg.chunks.iter().any(|c| {
            let cs = usize::from(c.offset);
            cs <= start && end <= cs + c.bytes.len()
        });
        if !covered {
            errors.push(whole(format!(
                "RELOC {} {:04X}: место перемещения не покрыто строками DATA",
                r.seg.name(),
                r.offset
            )));
        }
        if let RelocTarget::Extern(n) = &r.target
            && !m.externs.iter().any(|e| e.name.eq_ignore_ascii_case(n))
        {
            errors.push(whole(format!(
                "RELOC {} {:04X}: символ {n} не объявлен строкой EXTERN",
                r.seg.name(),
                r.offset
            )));
        }
    }
    for p in &m.publics {
        if let Some(Rel {
            target: RelocTarget::Extern(n),
            ..
        }) = &p.value.rel
            && !m.externs.iter().any(|e| e.name.eq_ignore_ascii_case(n))
        {
            errors.push(whole(format!(
                "PUBLIC {}: символ {n} не объявлен строкой EXTERN",
                p.name
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(number(5), "0005");
        assert_eq!(number(0x12345), "12345");
        assert_eq!(number(-3), "-0003");
        assert_eq!(signed(5), "+0005");
        assert_eq!(signed(-3), "-0003");
        assert_eq!(parse_number("+0005"), Some(5));
        assert_eq!(parse_number("-0003"), Some(-3));
        assert_eq!(parse_number("C000"), Some(0xC000));
        assert_eq!(parse_number("x"), None);
        assert_eq!(parse_number("-"), None);
    }

    #[test]
    fn at_loc_with_spaces_and_colons() {
        let text = "UT88LINK 1\nEXTERN X AT my dir/a:b.asm:12:7\nEND\n";
        let m = read("t.link", text).unwrap();
        let l = m.externs[0].loc.as_ref().unwrap();
        assert_eq!(&*l.file, "my dir/a:b.asm");
        assert_eq!((l.line, l.col), (12, 7));
        assert_eq!(
            write(&m).lines().nth(2),
            Some("EXTERN X AT my dir/a:b.asm:12:7")
        );
    }
}
