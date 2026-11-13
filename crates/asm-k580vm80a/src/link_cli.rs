//! Командная строка компоновщика `link-k580vm80a`.
//!
//! Как у ассемблера: `-h` / `-he` разбираются до clap (встроенная справка clap
//! отключена), подробная справка лежит в `link_help_ru.txt` / `link_help_en.txt`.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use clap::{Arg, ArgAction, Command};

use crate::cli::{clap_error_ru, parse_hex_arg};
use crate::linker::{LinkInput, LinkOptions, link};
use crate::output::Format;

/// Подробная справка на русском (`-h`).
pub const HELP_RU: &str = include_str!("link_help_ru.txt");
/// Подробная справка на английском (`-he`).
pub const HELP_EN: &str = include_str!("link_help_en.txt");

/// Краткая подсказка при запуске без аргументов.
pub const HINT: &str = "\
link-k580vm80a — компоновщик объектных файлов .link ассемблера КР580ВМ80А для ЮТ-88

Использование: link-k580vm80a МОДУЛЬ.link... -o ВЫХОД.hex|.bin|.ihx [-m КАРТА.map] [ключи]

  link-k580vm80a -h     подробная справка на русском
  link-k580vm80a -he    detailed help in English
";

const NAME: &str = "link-k580vm80a";

fn command() -> Command {
    Command::new(NAME)
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(Arg::new("inputs").num_args(1..).action(ArgAction::Append))
        .arg(Arg::new("out").short('o').num_args(1))
        .arg(Arg::new("type").short('t').num_args(1))
        .arg(Arg::new("map").short('m').num_args(1))
        .arg(Arg::new("entry").short('e').num_args(1))
        .arg(Arg::new("base").long("base").num_args(1))
        .arg(Arg::new("cseg").long("cseg").num_args(1))
        .arg(Arg::new("dseg").long("dseg").num_args(1))
        .arg(Arg::new("fill").long("fill").num_args(1))
        .arg(
            Arg::new("version")
                .short('V')
                .long("version")
                .action(ArgAction::SetTrue),
        )
}

fn usage_err(err: &mut dyn Write, msg: &str) -> i32 {
    let _ = writeln!(err, "{NAME}: ошибка: {msg}\nподробная справка: {NAME} -h");
    2
}

fn addr_arg(m: &clap::ArgMatches, key: &str) -> Result<Option<u16>, String> {
    match m.get_one::<String>(key) {
        None => Ok(None),
        Some(s) => match parse_hex_arg(s).filter(|&v| v <= 0xFFFF) {
            Some(v) => Ok(Some(v as u16)),
            None => Err(format!(
                "--{key} {s}: нужен адрес в шестнадцатеричном виде, например C000"
            )),
        },
    }
}

/// Выполнить компоновку с аргументами `args` (без имени программы). Код выхода: 0 — успех,
/// 1 — ошибки чтения модулей или компоновки, 2 — ошибки в командной строке.
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
    let m = match command().try_get_matches_from(args) {
        Ok(m) => m,
        Err(e) => return usage_err(err, &clap_error_ru(&e)),
    };
    if m.get_flag("version") {
        let _ = writeln!(out, "{NAME} {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let inputs: Vec<String> = m
        .get_many::<String>("inputs")
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    if inputs.is_empty() {
        return usage_err(err, "не указаны объектные файлы (МОДУЛЬ.link...)");
    }
    let Some(output) = m.get_one::<String>("out") else {
        return usage_err(err, "не указан выходной файл (-o ВЫХОД)");
    };
    let output = PathBuf::from(output);
    let format = match Format::choose(m.get_one::<String>("type").map(String::as_str), &output) {
        Ok(Format::Link) => {
            return usage_err(
                err,
                "компоновщик пишет только bin, hex или ihex (объединение в один .link не \
                 поддерживается)",
            );
        }
        Ok(f) => f,
        Err(e) => return usage_err(err, &e),
    };
    let mut opts = LinkOptions::default();
    let base = match addr_arg(&m, "base") {
        Ok(v) => v,
        Err(e) => return usage_err(err, &e),
    };
    opts.cseg = match addr_arg(&m, "cseg") {
        Ok(v) => v.or(base),
        Err(e) => return usage_err(err, &e),
    };
    opts.dseg = match addr_arg(&m, "dseg") {
        Ok(v) => v,
        Err(e) => return usage_err(err, &e),
    };
    opts.entry = m.get_one::<String>("entry").cloned();
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

    let mut modules = Vec::new();
    let mut errors = Vec::new();
    for f in &inputs {
        let text = match std::fs::read(f) {
            Ok(b) => match String::from_utf8(b) {
                Ok(t) => t,
                Err(_) => {
                    errors.push(crate::AsmError::plain(format!(
                        "{f}: не текст UTF-8 — это не объектный файл .link"
                    )));
                    continue;
                }
            },
            Err(e) => {
                errors.push(crate::AsmError::plain(format!("{f}: не читается: {e}")));
                continue;
            }
        };
        match crate::object::read(f, &text) {
            Ok(module) => modules.push(LinkInput {
                file: f.clone(),
                module,
            }),
            Err(e) => errors.extend(e),
        }
    }
    if !errors.is_empty() {
        return report(err, &errors);
    }
    let linked = match link(&modules, &opts) {
        Ok(l) => l,
        Err(e) => return report(err, &e),
    };
    let data = match format {
        Format::Bin => linked.image.to_bin(fill),
        Format::Hex => linked.image.to_dump().into_bytes(),
        Format::Ihex => linked.image.to_ihex().into_bytes(),
        Format::Link => unreachable!("отсеяно выше"),
    };
    if let Some(map) = m.get_one::<String>("map")
        && let Err(e) = std::fs::write(map, linked.map())
    {
        let _ = writeln!(err, "{NAME}: ошибка: не записывается карта {map}: {e}");
        return 1;
    }
    if let Err(e) = std::fs::write(&output, data) {
        let _ = writeln!(
            err,
            "{NAME}: ошибка: не записывается {}: {e}",
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
    let _ = writeln!(
        err,
        "{NAME}: ошибок: {}, выходной файл не создан",
        errors.len()
    );
    1
}
