//! Компоновщик: раскладка сегментов нескольких модулей, сведение PUBLIC/EXTERN,
//! применение перемещений, адрес пуска и карта `.map`.
//!
//! Раскладка: все CSEG подряд в порядке модулей с адреса `cseg` (по умолчанию
//! [`DEFAULT_BASE`]), затем все DSEG подряд с адреса `dseg` или сразу после кода; ASEG —
//! на свои адреса. Имена символов, как и в ассемблере, не зависят от регистра букв.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::error::{AsmError, Loc};
use crate::expr::{Rel, RelocTarget};
use crate::ir::{Module, SegBases, SegKind, resolve_value};
use crate::output::{Image, ImageBuilder};

/// Начало CSEG по умолчанию — начало ОЗУ пользователя ЮТ-88.
pub const DEFAULT_BASE: u16 = 0xC000;

/// Модуль на входе компоновщика.
#[derive(Clone, Debug)]
pub struct LinkInput {
    /// Файл, из которого прочитан модуль (для сообщений и карты).
    pub file: String,
    /// Модуль.
    pub module: Module,
}

impl LinkInput {
    /// Имя модуля: из `NAME`, иначе имя файла без папки и расширения.
    pub fn label(&self) -> String {
        if let Some(n) = &self.module.name {
            return n.clone();
        }
        let p = std::path::Path::new(&self.file);
        p.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.file.clone())
    }

    fn describe(&self) -> String {
        format!("модуль {} ({})", self.label(), self.file)
    }
}

/// Настройки компоновки.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkOptions {
    /// Начало CSEG первого модуля (`--cseg`/`--base`); `None` — [`DEFAULT_BASE`].
    pub cseg: Option<u16>,
    /// Начало DSEG первого модуля (`--dseg`); `None` — сразу после кода.
    pub dseg: Option<u16>,
    /// Символ адреса пуска (`-e`); `None` — единственный ENTRY модулей, если есть.
    pub entry: Option<String>,
}

/// Размещённый сегмент модуля (для карты).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedSegment {
    /// Номер модуля во входном списке.
    pub module: usize,
    /// Вид сегмента.
    pub kind: SegKind,
    /// Начальный адрес.
    pub start: u16,
    /// Размер в байтах (ASEG — по строке на каждый непрерывный кусок данных).
    pub size: u32,
}

/// Символ PUBLIC после компоновки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedSymbol {
    /// Имя.
    pub name: String,
    /// Значение (адрес или константа).
    pub value: i64,
    /// Номер модуля, где определён.
    pub module: usize,
    /// Сегмент значения (`None` — внешний символ ± константа).
    pub seg: Option<SegKind>,
}

/// Результат компоновки.
#[derive(Clone, Debug)]
pub struct Linked {
    /// Образ памяти (с адресом пуска).
    pub image: Image,
    /// Имена модулей и файлы, по порядку входа.
    pub modules: Vec<(String, String)>,
    /// Сегменты модулей по порядку модулей: ASEG, CSEG, DSEG.
    pub segments: Vec<PlacedSegment>,
    /// Все символы PUBLIC по алфавиту.
    pub symbols: Vec<LinkedSymbol>,
    /// Символ, давший адрес пуска (`-e`), если есть.
    pub entry_symbol: Option<String>,
    /// Модуль, чей `END адрес` дал адрес пуска (без `-e`), если есть.
    pub entry_module: Option<usize>,
}

fn place_text(loc: &Option<Loc>) -> String {
    match loc {
        Some(l) => format!(", {l}"),
        None => String::new(),
    }
}

/// Скомпоновать модули.
pub fn link(inputs: &[LinkInput], opts: &LinkOptions) -> Result<Linked, Vec<AsmError>> {
    let mut errors = Vec::new();
    if inputs.is_empty() {
        return Err(vec![AsmError::plain("нет модулей для компоновки")]);
    }

    // 1. Раскладка.
    let mut bases: Vec<SegBases> = vec![[0; 3]; inputs.len()];
    let mut segments = Vec::new();
    let mut code = i64::from(opts.cseg.unwrap_or(DEFAULT_BASE));
    for (i, inp) in inputs.iter().enumerate() {
        bases[i][SegKind::Code.index()] = code;
        code += i64::from(inp.module.segment(SegKind::Code).size);
    }
    let mut data = opts.dseg.map_or(code, i64::from);
    for (i, inp) in inputs.iter().enumerate() {
        bases[i][SegKind::Data.index()] = data;
        data += i64::from(inp.module.segment(SegKind::Data).size);
    }

    // 2. Области памяти: перемещаемые сегменты целиком (с резервом DS), ASEG — куски данных.
    struct Area {
        start: i64,
        end: i64,
        who: String,
    }
    let mut areas = Vec::new();
    for (i, inp) in inputs.iter().enumerate() {
        for seg in &inp.module.segments {
            if seg.kind == SegKind::Abs {
                for ch in &seg.chunks {
                    let start = i64::from(ch.offset);
                    areas.push(Area {
                        start,
                        end: start + ch.bytes.len() as i64,
                        who: format!("{} ASEG", inp.describe()),
                    });
                }
                for ch in &seg.chunks {
                    segments.push(PlacedSegment {
                        module: i,
                        kind: SegKind::Abs,
                        start: ch.offset,
                        size: ch.bytes.len() as u32,
                    });
                }
                continue;
            }
            if seg.size == 0 {
                continue;
            }
            let start = bases[i][seg.kind.index()];
            let end = start + i64::from(seg.size);
            if end > 0x10000 {
                errors.push(AsmError::plain(format!(
                    "{}: сегмент {} {:04X}H–{:X}H выходит за пределы адресного пространства \
                     0000H–FFFFH",
                    inp.describe(),
                    seg.kind.name(),
                    start,
                    end - 1
                )));
                continue;
            }
            areas.push(Area {
                start,
                end,
                who: format!("{} {}", inp.describe(), seg.kind.name()),
            });
            segments.push(PlacedSegment {
                module: i,
                kind: seg.kind,
                start: start as u16,
                size: seg.size,
            });
        }
    }
    areas.sort_by_key(|a| (a.start, a.end));
    for (j, b) in areas.iter().enumerate() {
        for a in &areas[..j] {
            if a.end > b.start && b.end > a.start {
                errors.push(AsmError::plain(format!(
                    "перекрытие областей памяти: {} ({:04X}H–{:04X}H) и {} ({:04X}H–{:04X}H)",
                    a.who,
                    a.start,
                    a.end - 1,
                    b.who,
                    b.start,
                    b.end - 1
                )));
            }
        }
    }

    // 3. Символы PUBLIC.
    let mut publics: HashMap<String, (usize, usize)> = HashMap::new();
    for (i, inp) in inputs.iter().enumerate() {
        for (j, p) in inp.module.publics.iter().enumerate() {
            let key = p.name.to_ascii_uppercase();
            if let Some(&(pi, pj)) = publics.get(&key) {
                let first = &inputs[pi].module.publics[pj];
                errors.push(AsmError::plain(format!(
                    "символ {} определён повторно: {}{} и {}{}",
                    p.name,
                    inputs[pi].describe(),
                    place_text(&first.loc),
                    inp.describe(),
                    place_text(&p.loc)
                )));
            } else {
                publics.insert(key, (i, j));
            }
        }
    }

    // 4. Неразрешённые внешние ссылки.
    for inp in inputs {
        for e in &inp.module.externs {
            if publics.contains_key(&e.name.to_ascii_uppercase()) {
                continue;
            }
            let mut err = AsmError::plain(format!(
                "{}: внешний символ {} не найден ни в одном модуле (нет PUBLIC {}){}",
                inp.describe(),
                e.name,
                e.name,
                e.loc
                    .as_ref()
                    .map_or(String::new(), |l| format!("; EXTRN в {l}"))
            ));
            for r in &inp.module.relocs {
                if matches!(&r.target, RelocTarget::Extern(n) if n.eq_ignore_ascii_case(&e.name)) {
                    err = err.note(format!(
                        "используется: {} смещение {:04X}H{}",
                        r.seg.name(),
                        r.offset,
                        place_text(&r.loc)
                    ));
                }
            }
            errors.push(err);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // 5. Значения PUBLIC (в том числе «внешний символ ± константа» — по цепочке).
    let mut values: HashMap<String, i64> = HashMap::new();
    let mut keys: Vec<&String> = publics.keys().collect();
    keys.sort();
    for key in keys {
        let mut chain = Vec::new();
        match resolve_public(key, inputs, &publics, &bases, &mut values, &mut chain) {
            Ok(_) => {}
            Err(msg) => {
                errors.push(AsmError::plain(msg));
                break;
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let lookup = |n: &str| values.get(&n.to_ascii_uppercase()).copied();

    // 6. Данные и перемещения.
    let mut img = ImageBuilder::new();
    for (i, inp) in inputs.iter().enumerate() {
        for e in inp.module.place(&bases[i], &lookup, &mut img) {
            errors.push(e.note(inp.describe()));
        }
    }

    // 7. Адрес пуска.
    let mut entry = None;
    let mut entry_symbol = None;
    let mut entry_module = None;
    if let Some(sym) = &opts.entry {
        match lookup(sym) {
            Some(v) if (0..=0xFFFF).contains(&v) => {
                entry = Some(v as u16);
                let (pi, pj) = publics[&sym.to_ascii_uppercase()];
                entry_symbol = Some(inputs[pi].module.publics[pj].name.clone());
            }
            Some(v) => errors.push(AsmError::plain(format!(
                "-e {sym}: значение {v:X}H не годится в адрес пуска"
            ))),
            None => errors.push(AsmError::plain(format!(
                "-e {sym}: символ не найден среди PUBLIC модулей"
            ))),
        }
    } else {
        let with_entry: Vec<usize> = (0..inputs.len())
            .filter(|&i| inputs[i].module.entry.is_some())
            .collect();
        match with_entry.as_slice() {
            [] => {}
            [i] => {
                let v = inputs[*i].module.entry.as_ref().expect("отобран по entry");
                let a = resolve_value(v, &bases[*i], &lookup).expect("ENTRY не внешний");
                entry = Some((a & 0xFFFF) as u16);
                entry_module = Some(*i);
            }
            many => {
                let list: Vec<String> = many.iter().map(|&i| inputs[i].describe()).collect();
                errors.push(AsmError::plain(format!(
                    "адрес пуска (END адрес) задан в нескольких модулях: {}; оставьте один или \
                     укажите -e СИМВОЛ",
                    list.join(", ")
                )));
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut symbols: Vec<LinkedSymbol> = publics
        .iter()
        .map(|(key, &(i, j))| {
            let p = &inputs[i].module.publics[j];
            let seg = match &p.value.rel {
                None => Some(SegKind::Abs),
                Some(Rel {
                    target: RelocTarget::Segment(k),
                    ..
                }) => Some(*k),
                Some(_) => None,
            };
            LinkedSymbol {
                name: p.name.clone(),
                value: values[key],
                module: i,
                seg,
            }
        })
        .collect();
    symbols.sort_by(|a, b| {
        a.name
            .to_ascii_uppercase()
            .cmp(&b.name.to_ascii_uppercase())
    });
    Ok(Linked {
        image: img.finish(entry),
        modules: inputs.iter().map(|m| (m.label(), m.file.clone())).collect(),
        segments,
        symbols,
        entry_symbol,
        entry_module,
    })
}

fn resolve_public(
    key: &str,
    inputs: &[LinkInput],
    publics: &HashMap<String, (usize, usize)>,
    bases: &[SegBases],
    values: &mut HashMap<String, i64>,
    chain: &mut Vec<String>,
) -> Result<i64, String> {
    if let Some(&v) = values.get(key) {
        return Ok(v);
    }
    let (i, j) = publics[key];
    let p = &inputs[i].module.publics[j];
    if chain.iter().any(|c| c == key) {
        chain.push(key.to_string());
        return Err(format!(
            "символы PUBLIC определены друг через друга по кругу: {}",
            chain.join(" → ")
        ));
    }
    chain.push(key.to_string());
    let v = match &p.value.rel {
        Some(Rel {
            target: RelocTarget::Extern(n),
            ..
        }) => {
            let k = n.to_ascii_uppercase();
            if !publics.contains_key(&k) {
                return Err(format!(
                    "{}: символ {} = {n} + константа, а {n} не найден",
                    inputs[i].describe(),
                    p.name
                ));
            }
            resolve_public(&k, inputs, publics, bases, values, chain)? + p.value.num
        }
        _ => resolve_value(&p.value, &bases[i], &|_| None).expect("не внешний"),
    };
    chain.pop();
    values.insert(key.to_string(), v);
    Ok(v)
}

fn addr_text(v: i64) -> String {
    if (0..=0xFFFF).contains(&v) {
        format!("{v:04X}H")
    } else if v < 0 {
        format!("-{:X}H", v.unsigned_abs())
    } else {
        format!("{v:X}H")
    }
}

/// Таблица с выравниванием столбцов.
fn table(out: &mut String, head: &[&str], rows: &[Vec<String>]) {
    let mut w: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let mut line = |cells: &[String]| {
        let mut s = String::from(" ");
        for (i, c) in cells.iter().enumerate() {
            s.push(' ');
            s.push_str(c);
            if i + 1 < cells.len() {
                s.push_str(&" ".repeat(w[i] - c.chars().count() + 1));
            }
        }
        let _ = writeln!(out, "{}", s.trim_end());
    };
    line(&head.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    for r in rows {
        line(r);
    }
}

impl Linked {
    /// Карта компоновки `.map`: сегменты модулей, символы по имени и по адресу, адрес
    /// пуска.
    pub fn map(&self) -> String {
        let mut s = String::from("КАРТА КОМПОНОВКИ link-k580vm80a\n\nСЕГМЕНТЫ МОДУЛЕЙ\n\n");
        let rows: Vec<Vec<String>> = self
            .segments
            .iter()
            .map(|g| {
                let (name, file) = &self.modules[g.module];
                let end = if g.size == 0 {
                    "-".to_string()
                } else {
                    format!("{:04X}H", u32::from(g.start) + g.size - 1)
                };
                vec![
                    name.clone(),
                    file.clone(),
                    g.kind.name().to_string(),
                    format!("{:04X}H", g.start),
                    end,
                    format!("{:04X}H", g.size),
                ]
            })
            .collect();
        table(
            &mut s,
            &["Модуль", "Файл", "Сегмент", "Начало", "Конец", "Размер"],
            &rows,
        );
        let sym_row = |y: &LinkedSymbol| {
            vec![
                y.name.clone(),
                addr_text(y.value),
                y.seg.map_or("EXT", |k| k.name()).to_string(),
                self.modules[y.module].0.clone(),
            ]
        };
        let head = ["Символ", "Значение", "Сегмент", "Модуль"];
        s.push_str("\nСИМВОЛЫ PUBLIC ПО ИМЕНИ\n\n");
        let rows: Vec<Vec<String>> = self.symbols.iter().map(sym_row).collect();
        if rows.is_empty() {
            s.push_str("  (нет)\n");
        } else {
            table(&mut s, &head, &rows);
        }
        s.push_str("\nСИМВОЛЫ PUBLIC ПО АДРЕСУ\n\n");
        let mut by_addr: Vec<&LinkedSymbol> = self.symbols.iter().collect();
        by_addr.sort_by_key(|y| (y.value, y.name.to_ascii_uppercase()));
        let rows: Vec<Vec<String>> = by_addr.into_iter().map(sym_row).collect();
        if rows.is_empty() {
            s.push_str("  (нет)\n");
        } else {
            table(&mut s, &head, &rows);
        }
        s.push_str("\nАДРЕС ПУСКА: ");
        match (self.image.entry, &self.entry_symbol, self.entry_module) {
            (Some(e), Some(n), _) => {
                let _ = writeln!(s, "{e:04X}H ({n}, ключ -e)");
            }
            (Some(e), None, Some(i)) => {
                let _ = writeln!(s, "{e:04X}H (END в модуле {})", self.modules[i].0);
            }
            (Some(e), None, None) => {
                let _ = writeln!(s, "{e:04X}H");
            }
            (None, _, _) => s.push_str("не задан\n"),
        }
        s
    }
}
