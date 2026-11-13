//! Командная строка `asm-k580vm80a`.
//!
//! `-h` / `-he` разбираются до clap (у clap встроенная справка отключена), подробная
//! справка лежит в `help_ru.txt` / `help_en.txt`.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{Arg, ArgAction, Command};

use crate::ir::Placement;
use crate::output::Format;
use crate::{Charset, Options};

/// Подробная справка на русском (`-h`).
pub const HELP_RU: &str = include_str!("help_ru.txt");
/// Подробная справка на английском (`-he`).
pub const HELP_EN: &str = include_str!("help_en.txt");

/// Краткая подсказка при запуске без аргументов.
pub const HINT: &str = "\
asm-k580vm80a — ассемблер КР580ВМ80А (Intel 8080) для ЮТ-88

Использование: asm-k580vm80a -f ВХОД.asm -o ВЫХОД.hex|.bin|.ihx [-t bin|hex|ihex] [ключи]

  asm-k580vm80a -h     подробная справка на русском
  asm-k580vm80a -he    detailed help in English
";

fn command() -> Command {
    Command::new("asm-k580vm80a")
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(Arg::new("file").short('f').num_args(1))
        .arg(Arg::new("out").short('o').num_args(1))
        .arg(Arg::new("type").short('t').num_args(1))
        .arg(Arg::new("listing").short('l').num_args(1))
        .arg(
            Arg::new("define")
                .short('D')
                .num_args(1)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("include")
                .short('I')
                .num_args(1)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("version")
                .short('V')
                .long("version")
                .action(ArgAction::SetTrue),
        )
        .arg(Arg::new("fill").long("fill").num_args(1))
        .arg(Arg::new("charset").long("charset").num_args(1))
        .arg(Arg::new("base").long("base").num_args(1))
}

fn clap_error_ru(e: &clap::Error) -> String {
    let arg = match e.get(ContextKind::InvalidArg) {
        Some(ContextValue::String(s)) => s.clone(),
        _ => String::new(),
    };
    let arg = arg.split_whitespace().next().unwrap_or("").to_string();
    match e.kind() {
        ErrorKind::UnknownArgument => format!("неизвестный ключ «{arg}»"),
        ErrorKind::InvalidValue | ErrorKind::NoEquals => {
            format!("у ключа {arg} нет значения")
        }
        ErrorKind::ArgumentConflict => format!("ключ {arg} указан дважды"),
        _ => format!("неверные аргументы командной строки ({})", e.kind()),
    }
}

/// Шестнадцатеричное число: `FF`, `0FFH`, `0xFF`, `$FF`.
pub fn parse_hex_arg(s: &str) -> Option<u32> {
    let t = s.trim();
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .or_else(|| t.strip_prefix('$'))
        .unwrap_or(t);
    let t = t
        .strip_suffix('h')
        .or_else(|| t.strip_suffix('H'))
        .unwrap_or(t);
    if t.is_empty() {
        return None;
    }
    u32::from_str_radix(t, 16).ok()
}

/// `ИМЯ` или `ИМЯ=ЗНАЧЕНИЕ` (значение — число в синтаксисе ассемблера, можно со знаком
/// минус; без значения — 1).
pub fn parse_define(s: &str) -> Result<(String, i64), String> {
    let (name, val) = match s.split_once('=') {
        Some((n, v)) => (n.trim(), Some(v.trim())),
        None => (s.trim(), None),
    };
    if name.is_empty() {
        return Err(format!("-D {s}: нет имени символа"));
    }
    let v = match val {
        None => 1,
        Some(v) => {
            let (neg, digits) = match v.strip_prefix('-') {
                Some(d) => (true, d),
                None => (false, v),
            };
            let n = if let Some(h) = digits.strip_prefix('$') {
                i64::from_str_radix(h, 16).map_err(|e| e.to_string())
            } else {
                crate::lexer::parse_number(digits)
            }
            .map_err(|e| format!("-D {s}: неверное значение: {e}"))?;
            if neg { -n } else { n }
        }
    };
    Ok((name.to_string(), v))
}

fn version() -> String {
    format!("asm-k580vm80a {}\n", env!("CARGO_PKG_VERSION"))
}

/// Выполнить команду с аргументами `args` (без имени программы). Возвращает код выхода:
/// 0 — успех, 1 — ошибки сборки, 2 — ошибки в командной строке.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = out.write_all(HINT.as_bytes());
        return 0;
    }
    if args.iter().any(|a| a == "-he") {
        let _ = out.write_all(HELP_EN.as_bytes());
        return 0;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        let _ = out.write_all(HELP_RU.as_bytes());
        return 0;
    }
    let usage_err = |err: &mut dyn Write, msg: &str| {
        let _ = writeln!(
            err,
            "asm-k580vm80a: ошибка: {msg}\nподробная справка: asm-k580vm80a -h"
        );
        2
    };
    let m = match command().try_get_matches_from(args) {
        Ok(m) => m,
        Err(e) => return usage_err(err, &clap_error_ru(&e)),
    };
    if m.get_flag("version") {
        let _ = out.write_all(version().as_bytes());
        return 0;
    }
    let Some(input) = m.get_one::<String>("file") else {
        return usage_err(err, "не указан исходный файл (-f ВХОД.asm)");
    };
    let Some(output) = m.get_one::<String>("out") else {
        return usage_err(err, "не указан выходной файл (-o ВЫХОД)");
    };
    let output = PathBuf::from(output);
    let format = match Format::choose(m.get_one::<String>("type").map(String::as_str), &output) {
        Ok(f) => f,
        Err(e) => return usage_err(err, &e),
    };
    let mut opts = Options::default();
    if let Some(c) = m.get_one::<String>("charset") {
        match Charset::from_name(c) {
            Some(c) => opts.charset = c,
            None => {
                return usage_err(
                    err,
                    &format!("неизвестная кодировка «{c}» (допустимо: koi7, ascii)"),
                );
            }
        }
    }
    for d in m.get_many::<String>("define").into_iter().flatten() {
        match parse_define(d) {
            Ok(x) => opts.defines.push(x),
            Err(e) => return usage_err(err, &e),
        }
    }
    opts.include_dirs = m
        .get_many::<String>("include")
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .collect();
    let fill = match m.get_one::<String>("fill") {
        None => 0,
        Some(s) => match parse_hex_arg(s).filter(|&v| v <= 0xFF) {
            Some(v) => v as u8,
            None => {
                return usage_err(
                    err,
                    &format!("--fill {s}: нужен байт в шестнадцатеричном виде, например FF"),
                );
            }
        },
    };
    let mut placement = Placement::default();
    if let Some(s) = m.get_one::<String>("base") {
        match parse_hex_arg(s).filter(|&v| v <= 0xFFFF) {
            Some(v) => placement.cseg = Some(v as u16),
            None => {
                return usage_err(
                    err,
                    &format!("--base {s}: нужен адрес в шестнадцатеричном виде, например C000"),
                );
            }
        }
    }
    if format == Format::Link {
        return usage_err(
            err,
            "тип выхода link (объектный файл .link) пока не реализован",
        );
    }

    let asm = match crate::assemble_file(std::path::Path::new(input), &opts) {
        Ok(a) => a,
        Err(errors) => return report(err, &errors),
    };
    let img = match asm.to_image(&placement) {
        Ok(i) => i,
        Err(errors) => return report(err, &errors),
    };
    let data = match format {
        Format::Bin => img.to_bin(fill),
        Format::Hex => img.to_dump().into_bytes(),
        Format::Ihex => img.to_ihex().into_bytes(),
        Format::Link => unreachable!("проверено выше"),
    };
    if let Some(lst) = m.get_one::<String>("listing")
        && let Err(e) = std::fs::write(lst, asm.listing())
    {
        let _ = writeln!(
            err,
            "asm-k580vm80a: ошибка: не записывается листинг {lst}: {e}"
        );
        return 1;
    }
    if let Err(e) = std::fs::write(&output, data) {
        let _ = writeln!(
            err,
            "asm-k580vm80a: ошибка: не записывается {}: {e}",
            output.display()
        );
        return 1;
    }
    0
}

fn report(err: &mut dyn Write, errors: &[crate::AsmError]) -> i32 {
    for e in errors {
        let _ = writeln!(err, "{e}");
    }
    let n = errors.len();
    let _ = writeln!(err, "asm-k580vm80a: ошибок: {n}, выходной файл не создан");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_args() {
        assert_eq!(parse_hex_arg("FF"), Some(0xFF));
        assert_eq!(parse_hex_arg("0FFH"), Some(0xFF));
        assert_eq!(parse_hex_arg("0xC000"), Some(0xC000));
        assert_eq!(parse_hex_arg("$C000"), Some(0xC000));
        assert_eq!(parse_hex_arg("xyz"), None);
        assert_eq!(parse_hex_arg(""), None);
    }

    #[test]
    fn defines() {
        assert_eq!(parse_define("DEBUG"), Ok(("DEBUG".into(), 1)));
        assert_eq!(parse_define("N=10"), Ok(("N".into(), 10)));
        assert_eq!(parse_define("N=0C000H"), Ok(("N".into(), 0xC000)));
        assert_eq!(parse_define("N=-2"), Ok(("N".into(), -2)));
        assert_eq!(parse_define("N=$FF"), Ok(("N".into(), 0xFF)));
        assert!(parse_define("=1").is_err());
        assert!(parse_define("N=zz").is_err());
    }
}
