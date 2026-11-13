//! Выражения и значения с признаком перемещаемости.
//!
//! Значение выражения — [`Value`]: число и, если оно зависит от места размещения,
//! признак перемещаемости [`Rel`] — относительно начала сегмента (CSEG/DSEG) или
//! внешнего символа (EXTRN), целиком (`WORD`) или только старший/младший байт
//! (`HIGH`/`LOW`).
//!
//! Допустимые операции над перемещаемыми значениями: `перемещаемое ± константа`,
//! `константа + перемещаемое`, разность двух перемещаемых с одной базой (результат
//! абсолютный), `HIGH`/`LOW` от перемещаемого слова. Всё остальное — ошибка
//! «выражение нельзя переместить».
//!
//! Приоритеты (от высшего к низшему): скобки; унарные `+ - HIGH LOW`;
//! `* / MOD SHL SHR`; `+ -`; сравнения `EQ NE LT LE GT GE = <> < <= > >=`; `NOT`; `AND`;
//! `OR XOR`. Арифметика целочисленная, результат сравнения — 0FFFFH (истина) или 0.

use crate::charset::Charset;
use crate::error::{AsmError, AsmResult, Loc};
use crate::ir::SegKind;
use crate::lexer::{Tok, Token};

/// На что указывает перемещаемое значение.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RelocTarget {
    /// Начало перемещаемого сегмента (только [`SegKind::Code`] или [`SegKind::Data`]).
    Segment(SegKind),
    /// Внешний символ (EXTRN), имя как написано в EXTRN.
    Extern(String),
}

/// Какая часть адреса нужна.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// Весь 16-битный адрес.
    Word,
    /// Младший байт (`LOW`).
    Low,
    /// Старший байт (`HIGH`).
    High,
}

impl Part {
    /// Имя вида перемещения: `WORD`, `LOW`, `HIGH`.
    pub fn name(self) -> &'static str {
        match self {
            Part::Word => "WORD",
            Part::Low => "LOW",
            Part::High => "HIGH",
        }
    }

    /// Применить к полному адресу.
    pub fn apply(self, addr: i64) -> i64 {
        match self {
            Part::Word => addr & 0xFFFF,
            Part::Low => addr & 0xFF,
            Part::High => (addr >> 8) & 0xFF,
        }
    }
}

/// Признак перемещаемости.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rel {
    /// База.
    pub target: RelocTarget,
    /// Часть адреса.
    pub part: Part,
}

/// Значение выражения.
///
/// Для абсолютного значения (`rel == None`) `num` — само число. Для перемещаемого —
/// смещение от базы (полное, даже при `Part::Low/High`): итоговое значение
/// `part.apply(база + num)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Value {
    /// Число или смещение от базы.
    pub num: i64,
    /// Признак перемещаемости; `None` — абсолютное значение.
    pub rel: Option<Rel>,
}

const NOT_RELOC: &str = "выражение нельзя переместить";

impl Value {
    /// Абсолютное значение.
    pub fn abs(num: i64) -> Self {
        Self { num, rel: None }
    }

    /// Адрес в сегменте `seg` со смещением `off` (в ASEG — абсолютный).
    pub fn in_segment(seg: SegKind, off: i64) -> Self {
        match seg {
            SegKind::Abs => Self::abs(off),
            s => Self {
                num: off,
                rel: Some(Rel {
                    target: RelocTarget::Segment(s),
                    part: Part::Word,
                }),
            },
        }
    }

    /// Внешний символ `name` (+0).
    pub fn external(name: &str) -> Self {
        Self {
            num: 0,
            rel: Some(Rel {
                target: RelocTarget::Extern(name.to_string()),
                part: Part::Word,
            }),
        }
    }

    /// Абсолютное ли значение.
    pub fn is_abs(&self) -> bool {
        self.rel.is_none()
    }

    /// Значение при базе `base` (для абсолютного — само число).
    pub fn resolve(&self, base: i64) -> i64 {
        match &self.rel {
            None => self.num,
            Some(r) => r.part.apply(base + self.num),
        }
    }

    fn word_rel(&self) -> Option<&RelocTarget> {
        match &self.rel {
            Some(Rel {
                target,
                part: Part::Word,
            }) => Some(target),
            _ => None,
        }
    }
}

/// Результат поиска символа.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// Значение известно.
    Value(Value),
    /// Символ есть, но значение пока неизвестно (ссылка вперёд на первом проходе).
    Unknown,
    /// Символа нет.
    Undefined,
}

/// Таблица символов для вычисления выражений.
pub trait Symbols {
    /// Найти символ по имени.
    fn lookup(&self, name: &str) -> Lookup;
    /// Текущий адрес `$`.
    fn here(&self) -> Value;
    /// Кодировка символьных констант.
    fn charset(&self) -> Charset;
    /// Последний ли проход: тогда неизвестный символ — ошибка, иначе значение `None`.
    fn final_pass(&self) -> bool;
}

/// Регистры — их нельзя использовать в выражениях и как имена символов.
pub const REGISTERS: [&str; 10] = ["A", "B", "C", "D", "E", "H", "L", "M", "SP", "PSW"];

/// Слова-операторы выражений.
pub const OPERATOR_WORDS: [&str; 15] = [
    "MOD", "SHL", "SHR", "AND", "OR", "XOR", "NOT", "EQ", "NE", "LT", "LE", "GT", "GE", "HIGH",
    "LOW",
];

/// Зарезервировано ли слово (регистр или оператор).
pub fn is_reserved(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    REGISTERS.contains(&up.as_str()) || OPERATOR_WORDS.contains(&up.as_str())
}

/// Вычислить выражение из лексем `toks`. `loc` — позиция строки (столбец — начало
/// выражения, для пустого выражения). `Ok(None)` — значение пока неизвестно.
pub fn eval(toks: &[Token], loc: &Loc, syms: &dyn Symbols) -> AsmResult<Option<Value>> {
    if toks.is_empty() {
        return Err(AsmError::at(loc, "ожидается выражение"));
    }
    let mut p = Parser {
        toks,
        pos: 0,
        loc,
        syms,
    };
    let v = p.or_expr()?;
    if let Some(t) = toks.get(p.pos) {
        return Err(AsmError::at(
            &loc.with_col(t.col),
            format!("лишнее в выражении: {}", describe(&t.tok)),
        ));
    }
    Ok(v)
}

/// Как назвать лексему в сообщении.
pub fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("«{s}»"),
        Tok::Num(n) => format!("число {n}"),
        Tok::Str(s) => format!("строка '{s}'"),
        Tok::Dollar => "«$»".to_string(),
        Tok::LParen => "«(»".to_string(),
        Tok::RParen => "«)»".to_string(),
        Tok::Comma => "«,»".to_string(),
        Tok::Op(o) => format!("«{o}»"),
    }
}

/// Значение символьной константы `'A'` / `'AB'`.
pub fn char_const(s: &str, charset: Charset) -> Result<i64, String> {
    let codes = s
        .chars()
        .map(|c| charset.encode(c))
        .collect::<Result<Vec<u8>, String>>()?;
    match codes.as_slice() {
        [a] => Ok(i64::from(*a)),
        [a, b] => Ok((i64::from(*a) << 8) | i64::from(*b)),
        [] => Err("пустая символьная константа".to_string()),
        _ => Err(format!(
            "символьная константа '{s}' длиннее двух символов (строки — только в DB)"
        )),
    }
}

struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
    loc: &'a Loc,
    syms: &'a dyn Symbols,
}

type R = AsmResult<Option<Value>>;

impl Parser<'_> {
    fn peek_word(&self) -> Option<String> {
        match self.toks.get(self.pos).map(|t| &t.tok) {
            Some(Tok::Ident(s)) => Some(s.to_ascii_uppercase()),
            Some(Tok::Op(o)) => Some((*o).to_string()),
            _ => None,
        }
    }

    fn col(&self) -> usize {
        self.toks
            .get(self.pos)
            .map(|t| t.col)
            .unwrap_or_else(|| self.toks.last().map_or(self.loc.col, |t| t.col + 1))
    }

    fn err(&self, col: usize, msg: impl Into<String>) -> AsmError {
        AsmError::at(&self.loc.with_col(col), msg)
    }

    fn binary(
        &mut self,
        ops: &[&str],
        next: fn(&mut Self) -> R,
        apply: fn(&str, Value, Value) -> Result<Value, String>,
    ) -> R {
        let mut left = next(self)?;
        while let Some(w) = self.peek_word() {
            let Some(op) = ops.iter().find(|o| **o == w) else {
                break;
            };
            let col = self.col();
            self.pos += 1;
            let right = next(self)?;
            left = match (left, right) {
                (Some(a), Some(b)) => Some(apply(op, a, b).map_err(|m| self.err(col, m))?),
                _ => None,
            };
        }
        Ok(left)
    }

    fn or_expr(&mut self) -> R {
        self.binary(&["OR", "XOR"], Self::and_expr, logic)
    }

    fn and_expr(&mut self) -> R {
        self.binary(&["AND"], Self::not_expr, logic)
    }

    fn not_expr(&mut self) -> R {
        if self.peek_word().as_deref() == Some("NOT") {
            let col = self.col();
            self.pos += 1;
            let v = self.not_expr()?;
            return match v {
                Some(v) if v.is_abs() => Ok(Some(Value::abs(!v.num))),
                Some(_) => Err(self.err(col, NOT_RELOC)),
                None => Ok(None),
            };
        }
        self.cmp_expr()
    }

    fn cmp_expr(&mut self) -> R {
        self.binary(
            &[
                "EQ", "NE", "LT", "LE", "GT", "GE", "=", "<>", "<", "<=", ">", ">=",
            ],
            Self::add_expr,
            compare,
        )
    }

    fn add_expr(&mut self) -> R {
        self.binary(&["+", "-"], Self::mul_expr, add_sub)
    }

    fn mul_expr(&mut self) -> R {
        self.binary(&["*", "/", "MOD", "SHL", "SHR"], Self::unary, mul_div)
    }

    fn unary(&mut self) -> R {
        let col = self.col();
        match self.peek_word().as_deref() {
            Some("+") => {
                self.pos += 1;
                self.unary()
            }
            Some("-") => {
                self.pos += 1;
                match self.unary()? {
                    Some(v) if v.is_abs() => Ok(Some(Value::abs(v.num.wrapping_neg()))),
                    Some(_) => Err(self.err(col, NOT_RELOC)),
                    None => Ok(None),
                }
            }
            Some(w @ ("HIGH" | "LOW")) => {
                let part = if w == "HIGH" { Part::High } else { Part::Low };
                self.pos += 1;
                match self.unary()? {
                    Some(v) if v.is_abs() => Ok(Some(Value::abs(part.apply(v.num)))),
                    Some(v) => match v.word_rel() {
                        Some(t) => Ok(Some(Value {
                            num: v.num,
                            rel: Some(Rel {
                                target: t.clone(),
                                part,
                            }),
                        })),
                        None => Err(self.err(col, NOT_RELOC)),
                    },
                    None => Ok(None),
                }
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> R {
        let col = self.col();
        let Some(t) = self.toks.get(self.pos) else {
            return Err(self.err(col, "выражение оборвано: ожидается операнд"));
        };
        self.pos += 1;
        match &t.tok {
            Tok::Num(n) => Ok(Some(Value::abs(*n))),
            Tok::Dollar => Ok(Some(self.syms.here())),
            Tok::Str(s) => char_const(s, self.syms.charset())
                .map(|v| Some(Value::abs(v)))
                .map_err(|m| self.err(col, m)),
            Tok::LParen => {
                let v = self.or_expr()?;
                match self.toks.get(self.pos) {
                    Some(Token {
                        tok: Tok::RParen, ..
                    }) => {
                        self.pos += 1;
                        Ok(v)
                    }
                    _ => Err(self.err(self.col(), "ожидается «)»")),
                }
            }
            Tok::Ident(name) => {
                let up = name.to_ascii_uppercase();
                if REGISTERS.contains(&up.as_str()) {
                    return Err(
                        self.err(col, format!("регистр {up} нельзя использовать в выражении"))
                    );
                }
                if OPERATOR_WORDS.contains(&up.as_str()) {
                    return Err(self.err(col, format!("ожидается операнд, а не «{name}»")));
                }
                match self.syms.lookup(name) {
                    Lookup::Value(v) => Ok(Some(v)),
                    Lookup::Unknown if self.syms.final_pass() => {
                        Err(self.err(col, format!("значение символа {name} не определено")))
                    }
                    Lookup::Undefined if self.syms.final_pass() => {
                        Err(self.err(col, format!("неизвестный символ {name}")))
                    }
                    _ => Ok(None),
                }
            }
            other => Err(self.err(col, format!("ожидается операнд, а не {}", describe(other)))),
        }
    }
}

fn both_abs(a: &Value, b: &Value) -> Result<(i64, i64), String> {
    if a.is_abs() && b.is_abs() {
        Ok((a.num, b.num))
    } else {
        Err(NOT_RELOC.to_string())
    }
}

fn logic(op: &str, a: Value, b: Value) -> Result<Value, String> {
    let (x, y) = both_abs(&a, &b)?;
    Ok(Value::abs(match op {
        "AND" => x & y,
        "OR" => x | y,
        _ => x ^ y,
    }))
}

fn compare(op: &str, a: Value, b: Value) -> Result<Value, String> {
    // Сравнивать можно и два перемещаемых с одной базой.
    let (x, y) = if a.rel.is_some() && a.rel == b.rel && a.word_rel().is_some() {
        (a.num, b.num)
    } else {
        both_abs(&a, &b)?
    };
    let r = match op {
        "EQ" | "=" => x == y,
        "NE" | "<>" => x != y,
        "LT" | "<" => x < y,
        "LE" | "<=" => x <= y,
        "GT" | ">" => x > y,
        _ => x >= y,
    };
    Ok(Value::abs(if r { 0xFFFF } else { 0 }))
}

fn add_sub(op: &str, a: Value, b: Value) -> Result<Value, String> {
    let plus = op == "+";
    match (a.is_abs(), b.is_abs()) {
        (true, true) => Ok(Value::abs(if plus {
            a.num.wrapping_add(b.num)
        } else {
            a.num.wrapping_sub(b.num)
        })),
        (false, true) if a.word_rel().is_some() => Ok(Value {
            num: if plus {
                a.num.wrapping_add(b.num)
            } else {
                a.num.wrapping_sub(b.num)
            },
            rel: a.rel,
        }),
        (true, false) if plus && b.word_rel().is_some() => Ok(Value {
            num: a.num.wrapping_add(b.num),
            rel: b.rel,
        }),
        (false, false) if !plus && a.word_rel().is_some() && a.rel == b.rel => {
            Ok(Value::abs(a.num.wrapping_sub(b.num)))
        }
        (false, false) if !plus && a.word_rel().is_some() && b.word_rel().is_some() => {
            Err(format!("{NOT_RELOC}: разность адресов из разных сегментов"))
        }
        _ => Err(NOT_RELOC.to_string()),
    }
}

fn mul_div(op: &str, a: Value, b: Value) -> Result<Value, String> {
    let (x, y) = both_abs(&a, &b)?;
    let v = match op {
        "*" => x.wrapping_mul(y),
        "/" | "MOD" if y == 0 => return Err("деление на ноль".to_string()),
        "/" => x.wrapping_div(y),
        "MOD" => x.wrapping_rem(y),
        "SHL" => {
            if !(0..32).contains(&y) {
                0
            } else {
                (x << y) & 0xFFFF
            }
        }
        _ => {
            if !(0..32).contains(&y) {
                0
            } else {
                (x & 0xFFFF) >> y
            }
        }
    };
    Ok(Value::abs(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct T {
        syms: HashMap<String, Value>,
        here: Value,
        final_pass: bool,
    }

    impl Symbols for T {
        fn lookup(&self, name: &str) -> Lookup {
            match self.syms.get(&name.to_ascii_uppercase()) {
                Some(v) => Lookup::Value(v.clone()),
                None => Lookup::Undefined,
            }
        }
        fn here(&self) -> Value {
            self.here.clone()
        }
        fn charset(&self) -> Charset {
            Charset::Koi7
        }
        fn final_pass(&self) -> bool {
            self.final_pass
        }
    }

    fn table() -> T {
        let mut syms = HashMap::new();
        syms.insert("FIVE".to_string(), Value::abs(5));
        syms.insert("C1".to_string(), Value::in_segment(SegKind::Code, 0x10));
        syms.insert("C2".to_string(), Value::in_segment(SegKind::Code, 0x30));
        syms.insert("D1".to_string(), Value::in_segment(SegKind::Data, 0x4));
        syms.insert("EXT".to_string(), Value::external("EXT"));
        T {
            syms,
            here: Value::in_segment(SegKind::Code, 0x100),
            final_pass: true,
        }
    }

    fn ev(text: &str) -> AsmResult<Option<Value>> {
        let t = table();
        let loc = Loc::new(Arc::from("e.asm"), 1, 1);
        let chars: Vec<char> = text.chars().collect();
        let toks = tokenize(&chars, 0, &loc)?;
        eval(&toks, &loc, &t)
    }

    fn num(text: &str) -> i64 {
        let v = ev(text).unwrap().unwrap();
        assert!(v.is_abs(), "{text}: {v:?}");
        v.num
    }

    #[test]
    fn arithmetic() {
        assert_eq!(num("1+2*3"), 7);
        assert_eq!(num("(1+2)*3"), 9);
        assert_eq!(num("10 MOD 3"), 1);
        assert_eq!(num("17/5"), 3);
        assert_eq!(num("1 SHL 4"), 16);
        assert_eq!(num("8000H SHR 15"), 1);
        assert_eq!(num("0F0H AND 3CH OR 1"), 0x31);
        assert_eq!(num("0FH XOR 3"), 0x0C);
        assert_eq!(num("NOT 0") & 0xFFFF, 0xFFFF);
        assert_eq!(num("-FIVE+1"), -4);
        assert_eq!(num("HIGH 1234H"), 0x12);
        assert_eq!(num("LOW 1234H"), 0x34);
        assert_eq!(num("HIGH (1234H+100H)"), 0x13);
        assert_eq!(num("'A'"), 0x41);
        assert_eq!(num("'AB'"), 0x4142);
        assert_eq!(num("'Я'"), 0x71);
        assert_eq!(num("5 EQ FIVE"), 0xFFFF);
        assert_eq!(num("5 <> FIVE"), 0);
        assert_eq!(num("1 < 2 AND 3 >= 3"), 0xFFFF);
        assert_eq!(num("2 LE 1"), 0);
        assert_eq!(num("$10+0x10+10"), 42);
    }

    #[test]
    fn relocatable() {
        let v = ev("C1+5").unwrap().unwrap();
        assert_eq!(v, Value::in_segment(SegKind::Code, 0x15));
        let v = ev("2+C1-1").unwrap().unwrap();
        assert_eq!(v.num, 0x11);
        assert_eq!(num("C2-C1"), 0x20);
        assert_eq!(num("$-C1"), 0xF0);
        let v = ev("HIGH (EXT+300H)").unwrap().unwrap();
        assert_eq!(
            v.rel,
            Some(Rel {
                target: RelocTarget::Extern("EXT".into()),
                part: Part::High
            })
        );
        assert_eq!(v.num, 0x300);
        let v = ev("LOW D1").unwrap().unwrap();
        assert_eq!(v.rel.as_ref().unwrap().part, Part::Low);
        assert_eq!(v.resolve(0x1FF), 0x03);
        for bad in [
            "C1+C2",
            "C1*2",
            "-C1",
            "D1-C1",
            "FIVE-C1",
            "NOT C1",
            "C1 AND 0FFH",
            "LOW C1 + 1",
            "HIGH LOW C1",
            "C1 SHR 8",
            "EXT-C1",
        ] {
            let e = ev(bad).unwrap_err();
            assert!(e.msg.contains("нельзя переместить"), "{bad}: {e}");
        }
        assert_eq!(num("C1 = C1"), 0xFFFF);
    }

    #[test]
    fn errors_with_columns() {
        let e = ev("1+UNKNOWN").unwrap_err();
        assert_eq!(
            e.to_string(),
            "e.asm:1:3: ошибка: неизвестный символ UNKNOWN"
        );
        let e = ev("(1+2").unwrap_err();
        assert_eq!(e.to_string(), "e.asm:1:5: ошибка: ожидается «)»");
        let e = ev("1/0").unwrap_err();
        assert_eq!(e.to_string(), "e.asm:1:2: ошибка: деление на ноль");
        let e = ev("1 2").unwrap_err();
        assert_eq!(e.msg, "лишнее в выражении: число 2");
        assert!(ev("A+1").unwrap_err().msg.contains("регистр A"));
        assert!(ev("'ABC'").is_err());
        assert!(ev("1+").is_err());

        let mut t = table();
        t.final_pass = false;
        let loc = Loc::new(Arc::from("e.asm"), 1, 1);
        let toks = tokenize(&"FWD+1".chars().collect::<Vec<_>>(), 0, &loc).unwrap();
        assert_eq!(eval(&toks, &loc, &t), Ok(None));
    }
}
