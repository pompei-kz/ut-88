//! Командная строка: выбор формата по расширению и -t, ключи, справка -h/-he.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use asm_k580vm80a::assembler::DIRECTIVES;
use asm_k580vm80a::cli::{HELP_EN, HELP_RU, HINT, run};
use asm_k580vm80a::opcodes::TABLE;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("asm-k580-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn p(&self, f: &str) -> PathBuf {
        self.0.join(f)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn call(args: &[&str]) -> (i32, String, String) {
    let a: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(&a, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

const PROG: &str = "        ORG 0C000H\nSTART:  MVI A,11H\n        JMP START\n        END START\n";

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn format_by_extension() {
    let d = Dir::new("ext");
    let src = d.p("p.asm");
    std::fs::write(&src, PROG).unwrap();
    for (name, want) in [
        ("p.hex", b"C000 3E 11 C3 00 C0\n".to_vec()),
        ("p.bin", vec![0x3E, 0x11, 0xC3, 0x00, 0xC0]),
        ("p.rom", vec![0x3E, 0x11, 0xC3, 0x00, 0xC0]),
        (
            "p.ihx",
            b":05C000003E11C300C069\n:040000030000C00039\n:00000001FF\n".to_vec(),
        ),
        (
            "p.ihex",
            b":05C000003E11C300C069\n:040000030000C00039\n:00000001FF\n".to_vec(),
        ),
    ] {
        let out = d.p(name);
        let (code, _, err) = call(&["-f", s(&src), "-o", s(&out)]);
        assert_eq!(code, 0, "{name}: {err}");
        assert_eq!(std::fs::read(&out).unwrap(), want, "{name}");
    }
}

#[test]
fn type_option_overrides_extension() {
    let d = Dir::new("type");
    let src = d.p("p.asm");
    std::fs::write(&src, PROG).unwrap();
    let out = d.p("p.bin");
    assert_eq!(call(&["-f", s(&src), "-o", s(&out), "-t", "hex"]).0, 0);
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "C000 3E 11 C3 00 C0\n"
    );
    let out = d.p("p.txt");
    assert_eq!(call(&["-t", "bin", "-f", s(&src), "-o", s(&out)]).0, 0);
    assert_eq!(std::fs::read(&out).unwrap().len(), 5);
    let out = d.p("p.dat");
    assert_eq!(call(&["-f", s(&src), "-o", s(&out), "-t", "ihex"]).0, 0);
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .starts_with(":05C000")
    );

    // Неизвестное расширение без -t, неизвестный -t, link — ошибка, файла нет.
    let out = d.p("p.out");
    let (code, _, err) = call(&["-f", s(&src), "-o", s(&out)]);
    assert_eq!(code, 2);
    assert!(err.contains("не понять тип выходного файла"), "{err}");
    assert!(err.contains("-t bin|hex|ihex|link"), "{err}");
    let (code, _, err) = call(&["-f", s(&src), "-o", s(&out), "-t", "elf"]);
    assert_eq!(code, 2);
    assert!(err.contains("неизвестный тип выхода «elf»"), "{err}");
    let (code, _, err) = call(&["-f", s(&src), "-o", s(&d.p("p.link"))]);
    assert_eq!(code, 2);
    assert!(err.contains("link"), "{err}");
    assert!(!out.exists() && !d.p("p.link").exists());
}

#[test]
fn errors_do_not_create_output() {
    let d = Dir::new("err");
    let src = d.p("bad.asm");
    std::fs::write(&src, "  ORG 0\n  JMP NOWHERE\n  MVI A\n").unwrap();
    let out = d.p("bad.bin");
    let (code, _, err) = call(&["-f", s(&src), "-o", s(&out)]);
    assert_eq!(code, 1);
    let first = format!("{}:2:7: ошибка: неизвестный символ NOWHERE", s(&src));
    let second = format!(
        "{}:3:3: ошибка: команде MVI нужны два операнда через запятую",
        s(&src)
    );
    assert!(err.contains(&second), "{err}");
    // Ошибки первого прохода (синтаксис) сообщаются раньше, сборка дальше не идёт.
    assert!(!err.contains(&first), "{err}");
    assert!(!out.exists());

    std::fs::write(&src, "  ORG 0\n  JMP NOWHERE\n").unwrap();
    let (code, _, err) = call(&["-f", s(&src), "-o", s(&out)]);
    assert_eq!(code, 1);
    assert!(err.contains(&first), "{err}");
    assert!(!out.exists());
}

#[test]
fn options_d_fill_charset_base() {
    let d = Dir::new("opts");
    let src = d.p("p.asm");
    std::fs::write(
        &src,
        "        CSEG\n        DB N, 'Я'\n        ORG 4\n        DB LOW $\n",
    )
    .unwrap();
    let out = d.p("p.bin");
    let (code, _, err) = call(&[
        "-f",
        s(&src),
        "-o",
        s(&out),
        "-D",
        "N=0x7",
        "--fill",
        "FF",
        "--base",
        "C000",
    ]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        std::fs::read(&out).unwrap(),
        vec![7, 0x71, 0xFF, 0xFF, 0x04]
    );
    let (code, _, err) = call(&[
        "-f",
        s(&src),
        "-o",
        s(&out),
        "-D",
        "N",
        "--charset",
        "ascii",
    ]);
    assert_eq!(code, 1);
    assert!(err.contains("не входит в ASCII"), "{err}");
    let (code, _, err) = call(&["-f", s(&src), "-o", s(&out), "--charset", "ebcdic"]);
    assert_eq!(code, 2);
    assert!(err.contains("неизвестная кодировка"), "{err}");
    let (code, _, err) = call(&["-f", s(&src), "-o", s(&out), "--fill", "100"]);
    assert_eq!(code, 2);
    assert!(err.contains("--fill"), "{err}");
}

#[test]
fn usage_errors() {
    let (code, _, err) = call(&["-o", "x.bin"]);
    assert_eq!(code, 2);
    assert!(err.contains("не указан исходный файл"), "{err}");
    let (code, _, err) = call(&["-f", "x.asm"]);
    assert_eq!(code, 2);
    assert!(err.contains("не указан выходной файл"), "{err}");
    let (code, _, err) = call(&["-f", "x.asm", "-o", "x.bin", "-q"]);
    assert_eq!(code, 2);
    assert!(err.contains("неизвестный ключ «-q»"), "{err}");
    assert!(err.contains("asm-k580vm80a -h"), "{err}");
    let (code, _, err) = call(&["-f"]);
    assert_eq!(code, 2);
    assert!(err.contains("у ключа -f нет значения"), "{err}");
    let (code, out, _) = call(&["-V"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("asm-k580vm80a "));
}

#[test]
fn no_arguments_prints_hint() {
    let (code, out, _) = call(&[]);
    assert_eq!(code, 0);
    assert_eq!(out, HINT);
    assert!(out.contains("-h") && out.contains("-he"));
}

#[test]
fn help_languages() {
    let (code, out, _) = call(&["-h"]);
    assert_eq!(code, 0);
    assert_eq!(out, HELP_RU);
    assert!(out.contains("ИСПОЛЬЗОВАНИЕ"));
    let (_, out, _) = call(&["-he"]);
    assert_eq!(out, HELP_EN);
    assert!(out.contains("USAGE"));
    // -h/-he разбираются раньше остальных ключей.
    assert_eq!(call(&["-f", "x.asm", "-he"]).1, HELP_EN);
    assert_eq!(call(&["--unknown", "-h"]).1, HELP_RU);
}

/// Справка описывает все ключи, директивы, форматы и команды.
#[test]
fn help_covers_everything() {
    let keys = [
        "-f ",
        "-o ",
        "-t ",
        "-l ",
        "-D ",
        "-I ",
        "-h",
        "-he",
        "-V",
        "--fill",
        "--charset",
        "--base",
    ];
    for help in [HELP_RU, HELP_EN] {
        let words: Vec<&str> = help
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .collect();
        for k in keys {
            assert!(help.contains(k), "ключ {k} не описан");
        }
        for d in DIRECTIVES {
            assert!(words.contains(d), "директива {d} не описана");
        }
        for (m, _) in TABLE {
            assert!(words.contains(m), "команда {m} не описана");
        }
        for f in [
            ".bin", ".rom", ".hex", ".ihx", ".ihex", ".link", "koi7", "ascii",
        ] {
            assert!(help.contains(f), "{f} не описан");
        }
        assert!(help.contains("КР580ВМ80А"));
        assert!(!help.contains("KR580"));
    }
}

/// Примеры из справки собираются.
#[test]
fn help_examples_assemble() {
    let src = "\
TABLE:  DS 20
BUF:    DS 2
SIZE    EQU 4
START:
END_ADDR:
ADDR    EQU 1234H
        MVI   A,HIGH (TABLE+10)
        LXI   H,BUF+2*SIZE
        CPI   'Z'+1
        ANI   NOT 80H
        DB    (END_ADDR-START) MOD 256
MSG:    DB 'ПРИВЕТ', 0DH, 0AH, 0
        DB 'A'+80H, LOW ADDR, HIGH ADDR
        DB 'It''s'
N       SET 0
N       SET N+1
LOOP:   DW START, LOOP, 0
        DS 16, 0FFH
";
    let a = asm_k580vm80a::assemble("ex.asm", src, &Default::default());
    assert!(a.is_ok(), "{:?}", a.err());
}
