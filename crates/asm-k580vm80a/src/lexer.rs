//! Лексер: разбор строки на метку, мнемонику (директиву), операнды и комментарий.
//!
//! Строка: `[метка[:]] [мнемоника|директива [операнды]] [; комментарий]`.
//! Метка — слово с двоеточием в любом месте строки или слово без двоеточия с первого
//! столбца (если это не известная мнемоника/директива); имя перед `EQU`/`SET`/`MACRO` —
//! тоже метка, даже с отступом.

use crate::error::{AsmError, AsmResult, Loc};
use crate::source::SourceLine;

/// Лексема операндов.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// Имя: символ, регистр или слово-оператор (`MOD`, `HIGH`…), как написано.
    Ident(String),
    /// Число.
    Num(i64),
    /// Строка в кавычках `'…'` или `"…"` (удвоенная кавычка внутри — сама кавычка).
    Str(String),
    /// `$` — текущий адрес.
    Dollar,
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// `,`.
    Comma,
    /// Знак операции: `+ - * / = <> < <= > >=`.
    Op(&'static str),
}

/// Лексема с номером столбца (с 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// Лексема.
    pub tok: Tok,
    /// Столбец начала.
    pub col: usize,
}

/// Слово строки с номером столбца.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    /// Текст как написан.
    pub text: String,
    /// Столбец начала (с 1).
    pub col: usize,
}

/// Разобранная строка.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedLine {
    /// Метка (без двоеточия).
    pub label: Option<Word>,
    /// Мнемоника или директива.
    pub op: Option<Word>,
    /// Текст операндов (без комментария, без концевых пробелов).
    pub args_text: String,
    /// Столбец начала операндов.
    pub args_col: usize,
    /// Операнды, разделённые запятыми верхнего уровня (вне скобок).
    pub args: Vec<Vec<Token>>,
    /// Столбец начала каждого операнда (для пустого — столбец после запятой).
    pub arg_cols: Vec<usize>,
}

/// Может ли символ начинать имя.
pub fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '_' | '?' | '@' | '.')
}

/// Может ли символ продолжать имя.
pub fn is_ident_char(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

/// Индекс начала комментария `;` вне кавычек (или длина строки).
fn comment_start(chars: &[char]) -> usize {
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) => {
                if c == q {
                    if chars.get(i + 1) == Some(&q) {
                        i += 1;
                    } else {
                        quote = None;
                    }
                }
            }
            None => match c {
                ';' => return i,
                '\'' | '"' => quote = Some(c),
                _ => {}
            },
        }
        i += 1;
    }
    chars.len()
}

fn read_word(chars: &[char], start: usize) -> usize {
    let mut e = start;
    while e < chars.len() && is_ident_char(chars[e]) {
        e += 1;
    }
    e
}

/// Разобрать строку. `is_op` — известна ли мнемоника/директива (без учёта регистра).
pub fn parse_line(src: &SourceLine, is_op: &dyn Fn(&str) -> bool) -> AsmResult<ParsedLine> {
    let all: Vec<char> = src.text.chars().collect();
    let end = comment_start(&all);
    let chars = &all[..end];
    let mut out = ParsedLine::default();
    let skip_ws = |mut i: usize| {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        i
    };
    let mut i = skip_ws(0);
    if i < chars.len() && is_ident_start(chars[i]) {
        let s = i;
        let e = read_word(chars, s);
        let w: String = chars[s..e].iter().collect();
        if chars.get(e) == Some(&':') {
            out.label = Some(Word {
                text: w,
                col: s + 1,
            });
            i = e + 1;
            if chars.get(i) == Some(&':') {
                i += 1;
            }
        } else if !is_op(&w) {
            // Имя перед EQU/SET/MACRO — метка даже с отступом.
            let j = skip_ws(e);
            let next_end = read_word(chars, j);
            let next: String = chars[j..next_end].iter().collect();
            let defines = matches!(next.to_ascii_uppercase().as_str(), "EQU" | "SET" | "MACRO");
            if s == 0 || defines {
                out.label = Some(Word {
                    text: w,
                    col: s + 1,
                });
                i = e;
            }
        }
    } else if i < chars.len() && i == 0 && !chars[i].is_whitespace() {
        return Err(AsmError::at(
            &src.loc(1),
            format!("неверное начало строки «{}»", chars[0]),
        ));
    }
    i = skip_ws(i);
    if i < chars.len() {
        if !is_ident_start(chars[i]) {
            return Err(AsmError::at(
                &src.loc(i + 1),
                "ожидается мнемоника команды или директива",
            ));
        }
        let e = read_word(chars, i);
        out.op = Some(Word {
            text: chars[i..e].iter().collect(),
            col: i + 1,
        });
        i = skip_ws(e);
    }
    out.args_col = i + 1;
    out.args_text = chars[i.min(chars.len())..]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_string();
    let toks = tokenize(&chars[i.min(chars.len())..], i, &src.loc(1))?;
    (out.args, out.arg_cols) = split_args_cols(toks, out.args_col);
    Ok(out)
}

/// Разделить лексемы по запятым верхнего уровня. Пустой список — нет операндов.
pub fn split_args(toks: Vec<Token>) -> Vec<Vec<Token>> {
    split_args_cols(toks, 1).0
}

/// То же, плюс столбец начала каждого операнда (`first_col` — для первого, если он пуст).
pub fn split_args_cols(toks: Vec<Token>, first_col: usize) -> (Vec<Vec<Token>>, Vec<usize>) {
    if toks.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut args = vec![Vec::new()];
    let mut cols = vec![first_col];
    let mut depth = 0i32;
    for t in toks {
        match t.tok {
            Tok::LParen => depth += 1,
            Tok::RParen => depth -= 1,
            Tok::Comma if depth == 0 => {
                args.push(Vec::new());
                cols.push(t.col + 1);
                continue;
            }
            _ => {}
        }
        let last = args.last_mut().expect("есть");
        if last.is_empty() {
            *cols.last_mut().expect("есть") = t.col;
        }
        last.push(t);
    }
    (args, cols)
}

/// Разбить текст операндов на лексемы. `offset` — индекс первого символа в строке,
/// `loc` — позиция строки (столбец берётся из лексемы).
pub fn tokenize(chars: &[char], offset: usize, loc: &Loc) -> AsmResult<Vec<Token>> {
    let mut out = Vec::new();
    let mut i = 0;
    let at = |i: usize| loc.with_col(offset + i + 1);
    while i < chars.len() {
        let c = chars[i];
        let col = offset + i + 1;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let s = i;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let text: String = chars[s..i].iter().collect();
            let v = parse_number(&text).map_err(|m| AsmError::at(&at(s), m))?;
            out.push(Token {
                tok: Tok::Num(v),
                col,
            });
            continue;
        }
        if c == '$' {
            let s = i;
            i += 1;
            if i < chars.len() && chars[i].is_ascii_hexdigit() {
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let text: String = chars[s + 1..i].iter().collect();
                let v = parse_radix(&text, 16).map_err(|m| AsmError::at(&at(s), m))?;
                out.push(Token {
                    tok: Tok::Num(v),
                    col,
                });
            } else {
                out.push(Token {
                    tok: Tok::Dollar,
                    col,
                });
            }
            continue;
        }
        if is_ident_start(c) {
            let e = read_word(chars, i);
            out.push(Token {
                tok: Tok::Ident(chars[i..e].iter().collect()),
                col,
            });
            i = e;
            continue;
        }
        if c == '\'' || c == '"' {
            let q = c;
            let start = i;
            let mut s = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(AsmError::at(&at(start), "не закрыта кавычка")),
                    Some(&ch) if ch == q => {
                        if chars.get(i + 1) == Some(&q) {
                            s.push(q);
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    }
                    Some(&ch) => {
                        s.push(ch);
                        i += 1;
                    }
                }
            }
            out.push(Token {
                tok: Tok::Str(s),
                col,
            });
            continue;
        }
        let next = chars.get(i + 1).copied();
        let (tok, len) = match (c, next) {
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            (',', _) => (Tok::Comma, 1),
            ('+', _) => (Tok::Op("+"), 1),
            ('-', _) => (Tok::Op("-"), 1),
            ('*', _) => (Tok::Op("*"), 1),
            ('/', _) => (Tok::Op("/"), 1),
            ('=', _) => (Tok::Op("="), 1),
            ('<', Some('>')) => (Tok::Op("<>"), 2),
            ('<', Some('=')) => (Tok::Op("<="), 2),
            ('<', _) => (Tok::Op("<"), 1),
            ('>', Some('=')) => (Tok::Op(">="), 2),
            ('>', _) => (Tok::Op(">"), 1),
            _ => return Err(AsmError::at(&at(i), format!("неожиданный символ «{c}»"))),
        };
        out.push(Token { tok, col });
        i += len;
    }
    Ok(out)
}

fn parse_radix(digits: &str, radix: u32) -> Result<i64, String> {
    if digits.is_empty() {
        return Err("нет цифр в числе".to_string());
    }
    let mut v: i64 = 0;
    for ch in digits.chars() {
        let d = ch
            .to_digit(radix)
            .ok_or_else(|| format!("недопустимая цифра «{ch}» в числе с основанием {radix}"))?;
        v = v * i64::from(radix) + i64::from(d);
        if v > 0xFFFF_FFFF {
            return Err("слишком большое число".to_string());
        }
    }
    Ok(v)
}

/// Разобрать число: `0C000H`, `0FFh`, `1010B`, `17O`/`17Q`, `99`/`99D`, `0x1F`.
pub fn parse_number(text: &str) -> Result<i64, String> {
    let lower = text.to_ascii_lowercase();
    if let Some(h) = lower.strip_prefix("0x") {
        return parse_radix(h, 16).map_err(|m| format!("«{text}»: {m}"));
    }
    let (body, radix) = match lower.chars().last() {
        Some('h') => (&lower[..lower.len() - 1], 16),
        Some('b') => (&lower[..lower.len() - 1], 2),
        Some('o') | Some('q') => (&lower[..lower.len() - 1], 8),
        Some('d') => (&lower[..lower.len() - 1], 10),
        _ => (lower.as_str(), 10),
    };
    parse_radix(body, radix).map_err(|m| format!("неверное число «{text}»: {m}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn line(t: &str) -> ParsedLine {
        let ops = ["MOV", "MVI", "EQU", "NOP", "DB", "ORG", "SET"];
        let is_op = |w: &str| ops.contains(&w.to_ascii_uppercase().as_str());
        parse_line(&SourceLine::new(Arc::from("t.asm"), 1, t), &is_op).unwrap()
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("0C000H"), Ok(0xC000));
        assert_eq!(parse_number("0ffh"), Ok(0xFF));
        assert_eq!(parse_number("1010B"), Ok(10));
        assert_eq!(parse_number("17O"), Ok(15));
        assert_eq!(parse_number("17q"), Ok(15));
        assert_eq!(parse_number("99"), Ok(99));
        assert_eq!(parse_number("99D"), Ok(99));
        assert_eq!(parse_number("0x1F"), Ok(31));
        assert!(parse_number("12B").is_err());
        assert!(parse_number("0FF").is_err());
        assert!(parse_number("19O").is_err());
    }

    #[test]
    fn lines() {
        let l = line("START:  MOV A,B ; комментарий; 'x'");
        assert_eq!(l.label.unwrap().text, "START");
        assert_eq!(l.op.unwrap().text, "MOV");
        assert_eq!(l.args.len(), 2);
        assert_eq!(l.args_text, "A,B");

        let l = line("LOOP NOP");
        assert_eq!(l.label.unwrap().text, "LOOP");
        let l = line("NOP");
        assert!(l.label.is_none());
        let l = line("   X EQU 5");
        assert_eq!(l.label.unwrap().text, "X");
        assert_eq!(l.op.unwrap().col, 6);
        let l = line("  DB ';', 'a''b', (1,2)");
        assert_eq!(l.args.len(), 3);
        assert_eq!(l.args[0][0].tok, Tok::Str(";".into()));
        assert_eq!(l.args[1][0].tok, Tok::Str("a'b".into()));
        assert_eq!(l.args[1][0].col, 11);
        let l = line("  MVI A,$FF");
        assert_eq!(l.args[1][0].tok, Tok::Num(0xFF));
        let l = line("; только комментарий");
        assert!(l.op.is_none() && l.label.is_none());
    }

    #[test]
    fn errors() {
        let is_op = |_: &str| true;
        let e =
            parse_line(&SourceLine::new(Arc::from("t.asm"), 3, "  DB 'abc"), &is_op).unwrap_err();
        assert_eq!(e.to_string(), "t.asm:3:6: ошибка: не закрыта кавычка");
        let e = parse_line(
            &SourceLine::new(Arc::from("t.asm"), 1, "  MVI A,#1"),
            &is_op,
        )
        .unwrap_err();
        assert_eq!(e.to_string(), "t.asm:1:9: ошибка: неожиданный символ «#»");
    }
}
