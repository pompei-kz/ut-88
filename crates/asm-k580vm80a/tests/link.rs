//! Объектные файлы `.link` и компоновщик `link-k580vm80a`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use asm_k580vm80a::ir::{Module, Placement};
use asm_k580vm80a::linker::{LinkInput, LinkOptions, link};
use asm_k580vm80a::{AsmError, Image, assemble, link_cli, object};

// ---------- общие помощники ----------

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("asm-k580-link-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn p(&self, f: &str) -> PathBuf {
        self.0.join(f)
    }
    fn write(&self, f: &str, text: &str) -> String {
        let p = self.p(f);
        std::fs::write(&p, text).unwrap();
        s(&p).to_string()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn module(file: &str, text: &str) -> Module {
    match assemble(file, text, &Default::default()) {
        Ok(a) => a.module,
        Err(e) => panic!("{file}: {}", errs(&e)),
    }
}

fn input(file: &str, text: &str) -> LinkInput {
    LinkInput {
        file: file.replace(".asm", ".link"),
        module: module(file, text),
    }
}

fn errs(e: &[AsmError]) -> String {
    e.iter().map(|e| format!("{e}\n")).collect()
}

fn link_err(inputs: &[LinkInput], opts: &LinkOptions) -> String {
    match link(inputs, opts) {
        Ok(_) => panic!("компоновка должна завершиться ошибкой"),
        Err(e) => errs(&e),
    }
}

fn base(a: u16) -> LinkOptions {
    LinkOptions {
        cseg: Some(a),
        ..Default::default()
    }
}

// ---------- программа из нескольких модулей ----------

/// Главный модуль: PUBLIC/EXTRN, CSEG/DSEG, HIGH/LOW внешних символов, ± константа.
const MAIN: &str = "\
        NAME    MAIN
        EXTRN   PRINT, BUF, BUFLEN, COUNT
        PUBLIC  START, MSG
        CSEG
START:  LXI     SP,STACK
        LXI     H,MSG
        CALL    PRINT
        MVI     A,LOW BUF
        MVI     B,HIGH BUF
        MVI     C,LOW (BUF+3)
        MVI     D,HIGH (BUF+100H)
        LXI     D,BUF+7
        LXI     H,BUF-1
        LDA     COUNT
        MVI     E,LOW BUFLEN
        LXI     B,MSG+2
        MVI     A,HIGH (MSG-1)
        DW      PRINT, MSG, $+3, BUF+2
        DB      LOW PRINT, HIGH PRINT, LOW (STACK+1)
        JMP     START+3
        DSEG
MSG:    DB      'HELLO',0
        DS      20
STACK:  DW      START, BUF
        END     START
";

/// Библиотека: свои CSEG и DSEG, PUBLIC метки и абсолютного символа.
const LIB: &str = "\
        NAME    LIB
        PUBLIC  PRINT, BUF, BUFLEN, COUNT
        EXTRN   MSG
BUFLEN  EQU     8
        CSEG
PRINT:  MOV     A,M
        ORA     A
        RZ
        STA     COUNT
        INX     H
        JMP     PRINT
        LXI     H,MSG
        DSEG
COUNT:  DB      0
BUF:    DS      BUFLEN
        DW      MSG+1, PRINT
";

/// Третий модуль: только DSEG и ASEG.
const TAB: &str = "\
        NAME    TAB
        EXTRN   PRINT
        DSEG
TABLE:  DW      PRINT, TABLE
        ASEG
        ORG     0F000H
        JMP     PRINT
";

/// Тот же код одним файлом: NAME/EXTRN/END убраны, порядок частей сохранён.
fn monolithic(parts: &[&str]) -> String {
    let mut out = String::new();
    for p in parts {
        for line in p.lines() {
            let t = line.trim_start().to_ascii_uppercase();
            if t.starts_with("NAME") || t.starts_with("EXTRN") || t.starts_with("END") {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        // Каждый модуль начинается в CSEG (как отдельный файл).
        out.push_str("        CSEG\n");
    }
    out
}

fn mono_image(parts: &[&str], cseg: u16, dseg: Option<u16>, entry: Option<u16>) -> Image {
    let m = module("mono.asm", &monolithic(parts));
    let mut img = m
        .to_image(&Placement {
            cseg: Some(cseg),
            dseg,
        })
        .unwrap_or_else(|e| panic!("{}", errs(&e)));
    img.entry = entry;
    img
}

#[test]
fn linked_equals_monolithic() {
    let parts = [MAIN, LIB, TAB];
    for (cseg, dseg) in [(0xC000, None), (0x0100, None), (0x3000, Some(0xD000))] {
        let inputs: Vec<LinkInput> = [("main.asm", MAIN), ("lib.asm", LIB), ("tab.asm", TAB)]
            .iter()
            .map(|(f, t)| input(f, t))
            .collect();
        let opts = LinkOptions {
            cseg: Some(cseg),
            dseg,
            entry: None,
        };
        let linked = link(&inputs, &opts).unwrap_or_else(|e| panic!("{}", errs(&e)));
        let mono = mono_image(&parts, cseg, dseg, Some(cseg));
        assert_eq!(linked.image, mono, "cseg {cseg:04X} dseg {dseg:?}");
        assert_eq!(linked.image.to_bin(0xFF), mono.to_bin(0xFF));
        assert_eq!(linked.image.entry, Some(cseg));
    }
}

#[test]
fn linked_through_object_files_equals_monolithic() {
    // Модули проходят запись в .link и чтение обратно — результат тот же.
    let inputs: Vec<LinkInput> = [("main.asm", MAIN), ("lib.asm", LIB), ("tab.asm", TAB)]
        .iter()
        .map(|(f, t)| {
            let text = object::write(&module(f, t));
            let file = f.replace(".asm", ".link");
            LinkInput {
                module: object::read(&file, &text).unwrap_or_else(|e| panic!("{}", errs(&e))),
                file,
            }
        })
        .collect();
    let linked = link(&inputs, &base(0xC000)).unwrap();
    assert_eq!(
        linked.image,
        mono_image(&[MAIN, LIB, TAB], 0xC000, None, Some(0xC000))
    );
}

#[test]
fn default_base_is_c000_and_entry_option() {
    let inputs = vec![input("main.asm", MAIN), input("lib.asm", LIB)];
    let l = link(&inputs, &LinkOptions::default()).unwrap();
    assert_eq!(l.image.chunks[0].0, 0xC000);
    let l = link(
        &inputs,
        &LinkOptions {
            entry: Some("print".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let print = l.symbols.iter().find(|y| y.name == "PRINT").unwrap();
    assert_eq!(l.image.entry, Some(print.value as u16));
    assert_eq!(l.entry_symbol.as_deref(), Some("PRINT"));
    let e = link_err(
        &inputs,
        &LinkOptions {
            entry: Some("NOPE".into()),
            ..Default::default()
        },
    );
    assert!(e.contains("-e NOPE: символ не найден"), "{e}");
}

#[test]
fn public_via_extern_chain() {
    let a = input(
        "a.asm",
        "  EXTRN Y\n  PUBLIC X, A1\nX EQU Y+2\n  CSEG\nA1: DW X\n",
    );
    let b = input("b.asm", "  PUBLIC Y\n  EXTRN X\n  CSEG\n  NOP\nY: DW X\n");
    let l = link(&[a, b], &base(0x100)).unwrap();
    // A: DW X (2 байта) по 0100H, B: NOP по 0102H, Y = 0103H, X = 0105H.
    assert_eq!(l.image.to_dump(), "0100 05 01 00 05 01\n");
    let names: Vec<_> = l
        .symbols
        .iter()
        .map(|y| (y.name.as_str(), y.value))
        .collect();
    assert_eq!(names, [("A1", 0x100), ("X", 0x105), ("Y", 0x103)]);
}

// ---------- ошибки компоновки ----------

#[test]
fn unresolved_extern() {
    let m = input("main.asm", MAIN);
    let e = link_err(&[m], &base(0xC000));
    assert!(
        e.contains(
            "ошибка: модуль MAIN (main.link): внешний символ PRINT не найден ни в одном модуле \
             (нет PUBLIC PRINT); EXTRN в main.asm:2:17"
        ),
        "{e}"
    );
    assert!(
        e.contains("примечание: используется: CSEG смещение 0007H, main.asm:7:17"),
        "{e}"
    );
    assert!(e.contains("внешний символ BUF не найден"), "{e}");
}

#[test]
fn duplicate_public() {
    let a = input("a.asm", "  NAME A\n  PUBLIC P\n  CSEG\nP: RET\n");
    let b = input("b.asm", "  NAME B\n  PUBLIC p\n  CSEG\n  NOP\np: RET\n");
    let e = link_err(&[a, b], &base(0));
    assert!(
        e.contains(
            "символ p определён повторно: модуль A (a.link), a.asm:2:10 и модуль B (b.link), \
             b.asm:2:10"
        ),
        "{e}"
    );
}

#[test]
fn overlap_and_address_space() {
    let a = input("a.asm", "  NAME A\n  ASEG\n  ORG 0C002H\n  DB 1,2\n");
    let b = input("b.asm", "  NAME B\n  CSEG\n  DS 4\n");
    let e = link_err(&[a.clone(), b.clone()], &base(0xC000));
    assert!(
        e.contains(
            "перекрытие областей памяти: модуль B (b.link) CSEG (C000H–C003H) и модуль A (a.link) \
             ASEG (C002H–C003H)"
        ),
        "{e}"
    );
    assert!(link(&[a.clone(), b.clone()], &base(0xC004)).is_ok());
    // Два ASEG.
    let c = input("c.asm", "  NAME C\n  ORG 0C003H\n  DB 3\n");
    let e = link_err(&[a.clone(), c], &base(0));
    assert!(e.contains("перекрытие"), "{e}");
    // DSEG по --dseg на код.
    let d = input("d.asm", "  NAME D\n  CSEG\n  DS 10H\n  DSEG\n  DB 1\n");
    let e = link_err(
        std::slice::from_ref(&d),
        &LinkOptions {
            cseg: Some(0x100),
            dseg: Some(0x10F),
            entry: None,
        },
    );
    assert!(e.contains("модуль D (d.link) CSEG (0100H–010FH)"), "{e}");
    // Выход за FFFFH.
    let e = link_err(&[d], &base(0xFFF8));
    assert!(
        e.contains("выходит за пределы адресного пространства"),
        "{e}"
    );
}

#[test]
fn low_high_overflow() {
    let a = input(
        "a.asm",
        "  NAME A\n  EXTRN X\n  CSEG\n  MVI A,HIGH (X+2000H)\n  MVI A,LOW (X-10H)\n",
    );
    let b = input("b.asm", "  PUBLIC X\n  CSEG\nX: NOP\n");
    // X = F004H: X+2000H = 11004H; X-10H — в пределах.
    let e = link_err(&[a.clone(), b.clone()], &base(0xF000));
    assert!(
        e.contains(
            "a.asm:4:9: ошибка: модуль A: переполнение: HIGH от адреса 11004H (X +2000H) — адрес \
             вне 0000H–FFFFH, байт неоднозначен"
        ),
        "{e}"
    );
    assert!(!e.contains("LOW"), "{e}");
    // X = 0004H: X-10H < 0.
    let e = link_err(&[a.clone(), b.clone()], &base(0));
    assert!(
        e.contains("переполнение: LOW от адреса -CH (X -10H)"),
        "{e}"
    );
    let l = link(&[a, b], &base(0x1000)).unwrap();
    assert_eq!(l.image.to_dump(), "1000 3E 30 3E F4 00\n");
}

#[test]
fn several_entries() {
    let a = input("a.asm", "  NAME A\n  CSEG\nS: NOP\n  END S\n");
    let b = input("b.asm", "  NAME B\n  CSEG\nT: NOP\n  END T\n");
    let e = link_err(&[a.clone(), b.clone()], &base(0));
    assert!(
        e.contains("адрес пуска (END адрес) задан в нескольких модулях: модуль A (a.link), модуль B (b.link)"),
        "{e}"
    );
}

// ---------- объектный файл ----------

#[test]
fn object_format_example() {
    let m = module(
        "hello.asm",
        "        NAME    hello
        EXTRN   PRINT
        PUBLIC  START, LIMIT
LIMIT   EQU     0FFH
        CSEG
START:  LXI     H,MSG
        CALL    PRINT
        MVI     A,LOW (PRINT+5)
        JMP     START
        DSEG
MSG:    DB      1,2
        DS      14
        END     START
",
    );
    assert_eq!(
        object::write(&m),
        "UT88LINK 1
; объектный модуль ассемблера КР580ВМ80А (ЮТ-88), читает link-k580vm80a
MODULE hello
SEGMENT CSEG 000B
DATA CSEG 0000 21 00 00 CD 00 00 3E 05 C3 00 00
SEGMENT DSEG 0010
DATA DSEG 0000 01 02
EXTERN PRINT AT hello.asm:2:17
PUBLIC START CSEG 0000 AT hello.asm:3:17
PUBLIC LIMIT ASEG 00FF AT hello.asm:3:24
RELOC CSEG 0001 WORD DSEG +0000 AT hello.asm:6:19
RELOC CSEG 0004 WORD EXT PRINT +0000 AT hello.asm:7:17
RELOC CSEG 0007 LOW EXT PRINT +0005 AT hello.asm:8:19
RELOC CSEG 0009 WORD CSEG +0000 AT hello.asm:9:17
ENTRY CSEG 0000
END
"
    );
}

#[test]
fn object_roundtrip_is_identical() {
    for (f, t) in [("main.asm", MAIN), ("lib.asm", LIB), ("tab.asm", TAB)] {
        let m = module(f, t);
        let text = object::write(&m);
        let back = object::read("x.link", &text).unwrap_or_else(|e| panic!("{}", errs(&e)));
        assert_eq!(object::write(&back), text, "{f}");
        let mut m2 = m.clone();
        m2.symbols.clear();
        assert_eq!(back, m2, "{f}");
    }
    // Длинные данные — по 16 байт в строке, читаются в один кусок.
    let m = module("big.asm", "  CSEG\n  DS 40, 0AAH\n  DB 1\n");
    let text = object::write(&m);
    assert_eq!(text.lines().filter(|l| l.starts_with("DATA")).count(), 3);
    let back = object::read("big.link", &text).unwrap();
    assert_eq!(back.segments[1].chunks.len(), 1);
    assert_eq!(object::write(&back), text);
}

fn read_err(text: &str) -> String {
    match object::read("t.link", text) {
        Ok(_) => panic!("ожидалась ошибка чтения:\n{text}"),
        Err(e) => errs(&e),
    }
}

#[test]
fn object_read_errors() {
    let e = read_err("UT88LINK 2\nEND\n");
    assert!(
        e.contains(
            "t.link:1:10: ошибка: неподдерживаемая версия формата .link «2» (поддерживается 1)"
        ),
        "{e}"
    );
    let e = read_err("HELLO\n");
    assert!(
        e.contains("t.link:1:1: ошибка: это не объектный файл .link"),
        "{e}"
    );
    let e = read_err("");
    assert!(e.contains("пустой файл"), "{e}");
    let e = read_err("UT88LINK 1\nSEGMENT CSEG 0002\nDATA CSEG 0000 01 02\n");
    assert!(
        e.contains("t.link:4:1: ошибка: файл обрезан: нет завершающей строки END"),
        "{e}"
    );
    let e = read_err("UT88LINK 1\nSEGMENT CSEG 0002\nDATA CSEG 0000 01 0X\nEND\n");
    assert!(e.contains("t.link:3:19: ошибка: неверный байт «0X»"), "{e}");
    let e = read_err("UT88LINK 1\nSEGMENT CSEG 0001\nDATA CSEG 0000 01 02\nEND\n");
    assert!(
        e.contains("t.link:3:11: ошибка: данные 0000…0001 выходят за размер"),
        "{e}"
    );
    let e = read_err("UT88LINK 1\nDATA CSEG 0000 01\nEND\n");
    assert!(e.contains("DATA CSEG до строки SEGMENT CSEG"), "{e}");
    let e = read_err("UT88LINK 1\nFOO 1\nEND\n");
    assert!(
        e.contains("t.link:2:1: ошибка: неизвестная строка «FOO»"),
        "{e}"
    );
    let e = read_err("UT88LINK 1\nSEGMENT XSEG 0001\nEND\n");
    assert!(
        e.contains("t.link:2:9: ошибка: неизвестный сегмент «XSEG»"),
        "{e}"
    );
    let e = read_err(
        "UT88LINK 1\nSEGMENT CSEG 0002\nDATA CSEG 0000 00 00\nRELOC CSEG 0000 BYTE CSEG +0000\nEND\n",
    );
    assert!(e.contains("неизвестный вид перемещения «BYTE»"), "{e}");
    let e = read_err(
        "UT88LINK 1\nSEGMENT CSEG 0002\nDATA CSEG 0000 00 00\nRELOC CSEG 0001 WORD CSEG +0000\nEND\n",
    );
    assert!(
        e.contains("RELOC CSEG 0001: место перемещения не покрыто строками DATA"),
        "{e}"
    );
    let e = read_err(
        "UT88LINK 1\nSEGMENT CSEG 0002\nDATA CSEG 0000 00 00\nRELOC CSEG 0000 WORD EXT X +0000\nEND\n",
    );
    assert!(e.contains("символ X не объявлен строкой EXTERN"), "{e}");
    let e = read_err(
        "UT88LINK 1\nSEGMENT CSEG 0002\nDATA CSEG 0000 00 00\nRELOC CSEG 0000 WORD CSEG 0000\nEND\n",
    );
    assert!(e.contains("ожидается число со знаком"), "{e}");
    let e = read_err("UT88LINK 1\nEXTERN X AT nowhere\nEND\n");
    assert!(e.contains("AT: неверное место «nowhere»"), "{e}");
    let e = read_err("UT88LINK 1\nEND\nEXTERN X\n");
    assert!(e.contains("t.link:3:1: ошибка: строки после END"), "{e}");
    let e = read_err("UT88LINK 1\nENTRY CSEG 0000 1\nEND\n");
    assert!(
        e.contains("t.link:2:17: ошибка: лишнее «1» в конце строки"),
        "{e}"
    );
    let e = read_err("UT88LINK 1\nEXTERN X\nEXTERN x\nEND\n");
    assert!(e.contains("повторный EXTERN x"), "{e}");
    // Комментарии и пустые строки допустимы, \r\n тоже.
    let m = object::read(
        "t.link",
        "; c\r\nUT88LINK 1\r\n\r\n; x\r\nMODULE m\r\nEND\r\n",
    )
    .unwrap();
    assert_eq!(m.name.as_deref(), Some("m"));
}

// ---------- карта ----------

#[test]
fn map_file() {
    let inputs = vec![
        input("main.asm", MAIN),
        input("lib.asm", LIB),
        input("tab.asm", TAB),
    ];
    let l = link(&inputs, &base(0xC000)).unwrap();
    let map = l.map();
    let main = inputs[0]
        .module
        .segment(asm_k580vm80a::ir::SegKind::Code)
        .size;
    assert!(
        map.starts_with("КАРТА КОМПОНОВКИ link-k580vm80a\n"),
        "{map}"
    );
    assert!(
        map.contains("  Модуль  Файл       Сегмент  Начало  Конец  Размер\n"),
        "{map}"
    );
    assert!(
        map.contains(&format!(
            "  MAIN    main.link  CSEG     C000H   {:04X}H  {main:04X}H\n",
            0xC000 + main - 1
        )),
        "{map}"
    );
    assert!(
        map.contains(&format!(
            "  LIB     lib.link   CSEG     {:04X}H",
            0xC000 + main
        )),
        "{map}"
    );
    assert!(
        map.contains("  TAB     tab.link   ASEG     F000H   F002H  0003H\n"),
        "{map}"
    );
    let by_name = map.split("СИМВОЛЫ PUBLIC ПО ИМЕНИ").nth(1).unwrap();
    let by_name = by_name.split("СИМВОЛЫ PUBLIC ПО АДРЕСУ").next().unwrap();
    let names: Vec<&str> = by_name
        .lines()
        .skip(3)
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(names, ["BUF", "BUFLEN", "COUNT", "MSG", "PRINT", "START"]);
    assert!(
        by_name.contains("  BUFLEN  0008H     ASEG     LIB\n"),
        "{map}"
    );
    assert!(
        by_name.contains(&format!(
            "  PRINT   {:04X}H     CSEG     LIB\n",
            0xC000 + main
        )),
        "{map}"
    );
    let by_addr = map.split("СИМВОЛЫ PUBLIC ПО АДРЕСУ").nth(1).unwrap();
    let addrs: Vec<&str> = by_addr
        .lines()
        .skip(3)
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(addrs, ["BUFLEN", "START", "PRINT", "MSG", "COUNT", "BUF"]);
    assert!(
        map.ends_with("АДРЕС ПУСКА: C000H (END в модуле MAIN)\n"),
        "{map}"
    );
}

// ---------- командная строка ----------

fn call(args: &[&str]) -> (i32, String, String) {
    let a: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = link_cli::run(&a, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn help_languages_and_hint() {
    let (code, out, _) = call(&[]);
    assert_eq!((code, out.as_str()), (0, link_cli::HINT));
    assert_eq!(call(&["-h"]).1, link_cli::HELP_RU);
    assert_eq!(call(&["--help"]).1, link_cli::HELP_RU);
    assert_eq!(call(&["-he"]).1, link_cli::HELP_EN);
    assert_eq!(call(&["a.link", "-o", "x.hex", "-he"]).1, link_cli::HELP_EN);
    assert!(link_cli::HELP_RU.contains("ИСПОЛЬЗОВАНИЕ"));
    assert!(link_cli::HELP_EN.contains("USAGE"));
    let (code, out, _) = call(&["-V"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("link-k580vm80a "));
}

/// Справка компоновщика описывает все ключи, раскладку, ошибки, карту и формат .link.
#[test]
fn help_covers_all_options() {
    let keys = [
        "-o ", "-t ", "-m ", "-e ", "--base", "--cseg", "--dseg", "--fill", "-h", "-he", "-V",
    ];
    for help in [link_cli::HELP_RU, link_cli::HELP_EN] {
        for k in keys {
            assert!(help.contains(k), "ключ {k} не описан");
        }
        for w in [
            "UT88LINK",
            "SEGMENT",
            "DATA",
            "EXTERN",
            "PUBLIC",
            "RELOC",
            "ENTRY",
            "WORD",
            "LOW",
            "HIGH",
            "CSEG",
            "DSEG",
            "ASEG",
            "EXTRN",
            ".map",
            ".bin",
            ".hex",
            ".ihx",
            "C000",
            "link-k580vm80a main.link",
        ] {
            assert!(help.contains(w), "{w} не описан");
        }
        assert!(help.contains("КР580ВМ80А"));
        assert!(!help.contains("KR580"));
    }
    // Ключи из command() и справки совпадают: каждый ключ справки принимается.
    for k in ["-m", "-e", "--base", "--cseg", "--dseg", "--fill", "-t"] {
        let (code, _, err) = call(&["x.link", "-o", "x.hex", k]);
        assert_eq!(code, 2);
        assert!(err.contains("нет значения"), "{k}: {err}");
    }
}

/// Пример из справки компоновщика (оба языка) собирается в тот же hello.hex и карту.
#[test]
fn help_example_links() {
    for help in [link_cli::HELP_RU, link_cli::HELP_EN] {
        let ex = help.split("\n  main.asm:\n").nth(1).unwrap();
        let (main, rest) = ex.split_once("\n  print.asm:\n").unwrap();
        let print = rest.split("\n\n").next().unwrap();
        let unindent = |t: &str| -> String {
            t.lines()
                .map(|l| l.strip_prefix("  ").unwrap_or(l))
                .map(|l| format!("{l}\n"))
                .collect()
        };
        let a = input("main.asm", &unindent(main));
        let b = input("print.asm", &unindent(print));
        let l = link(&[a.clone(), b.clone()], &base(0x3000)).unwrap();
        let want = help.split("\n  hello.hex:\n").nth(1).unwrap();
        let want: String = want
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(l.image.to_dump(), want);
        // Объектный файл в справке — тот, что пишет ассемблер (без строки-комментария).
        let obj = object::write(&a.module);
        for line in obj.lines().filter(|l| !l.starts_with(';')) {
            assert!(help.contains(&format!("    {line}")), "{line}");
        }
        // Карта в справке совпадает с настоящей.
        let mut inputs = [a, b];
        inputs[0].file = "main.link".into();
        inputs[1].file = "print.link".into();
        let map = link(&inputs, &base(0x3000)).unwrap().map();
        for line in map.lines().filter(|l| !l.is_empty()) {
            assert!(help.contains(&format!("  {line}\n")), "{line}");
        }
    }
}

#[test]
fn cli_link_end_to_end() {
    let d = Dir::new("cli");
    let main = d.write("main.asm", MAIN);
    let lib = d.write("lib.asm", LIB);
    let tab = d.write("tab.asm", TAB);
    let mono = d.write("mono.asm", &monolithic(&[MAIN, LIB, TAB]));
    let asm = env!("CARGO_BIN_EXE_asm-k580vm80a");
    let lnk = env!("CARGO_BIN_EXE_link-k580vm80a");
    for (src, out) in [(&main, "main.link"), (&lib, "lib.obj"), (&tab, "tab.link")] {
        let mut c = Command::new(asm);
        c.args(["-f", src, "-o", s(&d.p(out))]);
        if out.ends_with(".obj") {
            c.args(["-t", "link"]);
        }
        let o = c.output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    assert!(
        std::fs::read_to_string(d.p("lib.obj"))
            .unwrap()
            .starts_with("UT88LINK 1\n")
    );
    for (ext, fill) in [("bin", "FF"), ("hex", "00"), ("ihx", "00")] {
        let out = d.p(&format!("prog.{ext}"));
        let map = d.p("prog.map");
        let o = Command::new(lnk)
            .args([
                s(&d.p("main.link")),
                s(&d.p("lib.obj")),
                s(&d.p("tab.link")),
                "-o",
                s(&out),
                "--base",
                "C000",
                "-m",
                s(&map),
                "--fill",
                fill,
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let mono_out = d.p(&format!("mono.{ext}"));
        let o = Command::new(asm)
            .args([
                "-f",
                &mono,
                "-o",
                s(&mono_out),
                "--base",
                "C000",
                "--fill",
                fill,
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let linked = std::fs::read(&out).unwrap();
        let mono = std::fs::read(&mono_out).unwrap();
        if ext == "ihx" {
            // В монолитной сборке END убран — адрес пуска есть только у компоновщика.
            let text = String::from_utf8(linked).unwrap();
            assert!(text.contains(":040000030000C0"), "{text}");
            let without: String = text
                .lines()
                .filter(|l| !l.starts_with(":04000003"))
                .map(|l| format!("{l}\n"))
                .collect();
            assert_eq!(without.into_bytes(), mono);
        } else {
            assert_eq!(linked, mono, "{ext}");
        }
        assert!(
            std::fs::read_to_string(&map)
                .unwrap()
                .contains("АДРЕС ПУСКА: C000H")
        );
    }
}

#[test]
fn cli_errors() {
    let d = Dir::new("clierr");
    let main = d.write("main.asm", MAIN);
    let ml = d.p("main.link");
    assert_eq!(
        asm_k580vm80a::cli::run(
            &["-f", main.as_str(), "-o", s(&ml)].map(OsString::from),
            &mut Vec::new(),
            &mut Vec::new()
        ),
        0
    );
    // EXTRN при сборке сразу в hex — ошибка, в .link — нет.
    let mut err = Vec::new();
    let code = asm_k580vm80a::cli::run(
        &["-f", main.as_str(), "-o", s(&d.p("main.hex"))].map(OsString::from),
        &mut Vec::new(),
        &mut err,
    );
    assert_eq!(code, 1);
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("внешний символ PRINT не определён")
    );
    // --base с -t link — ошибка командной строки.
    let mut err = Vec::new();
    let code = asm_k580vm80a::cli::run(
        &["-f", main.as_str(), "-o", s(&ml), "--base", "C000"].map(OsString::from),
        &mut Vec::new(),
        &mut err,
    );
    assert_eq!(code, 2);
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("--base не применяется")
    );

    let out = d.p("x.hex");
    let map = d.p("x.map");
    let (code, _, err) = call(&[s(&ml), "-o", s(&out), "-m", s(&map)]);
    assert_eq!(code, 1);
    assert!(err.contains("внешний символ PRINT не найден"), "{err}");
    assert!(
        err.contains("link-k580vm80a: ошибок: 4, выходной файл не создан"),
        "{err}"
    );
    assert!(!out.exists() && !map.exists());

    let (code, _, err) = call(&["-o", s(&out)]);
    assert_eq!(code, 2);
    assert!(err.contains("не указаны объектные файлы"), "{err}");
    let (code, _, err) = call(&[s(&ml)]);
    assert_eq!(code, 2);
    assert!(err.contains("не указан выходной файл"), "{err}");
    let (code, _, err) = call(&[s(&ml), "-o", s(&d.p("x.link"))]);
    assert_eq!(code, 2);
    assert!(
        err.contains("компоновщик пишет только bin, hex или ihex"),
        "{err}"
    );
    let (code, _, err) = call(&[s(&ml), "-o", s(&d.p("x.txt"))]);
    assert_eq!(code, 2);
    assert!(err.contains("не понять тип выходного файла"), "{err}");
    let (code, _, err) = call(&[s(&ml), "-o", s(&out), "--cseg", "XYZ"]);
    assert_eq!(code, 2);
    assert!(err.contains("--cseg XYZ: нужен адрес"), "{err}");
    let (code, _, err) = call(&[s(&ml), "-o", s(&out), "-q"]);
    assert_eq!(code, 2);
    assert!(err.contains("неизвестный ключ «-q»"), "{err}");
    let (code, _, err) = call(&[s(&d.p("none.link")), "-o", s(&out)]);
    assert_eq!(code, 1);
    assert!(err.contains("none.link: не читается"), "{err}");
    let bad = d.write("bad.link", "UT88LINK 7\nEND\n");
    let (code, _, err) = call(&[&bad, "-o", s(&out)]);
    assert_eq!(code, 1);
    assert!(
        err.contains(&format!("{bad}:1:10: ошибка: неподдерживаемая версия")),
        "{err}"
    );
    std::fs::write(d.p("bin.link"), [0xFF, 0xFE, 0]).unwrap();
    let (code, _, err) = call(&[s(&d.p("bin.link")), "-o", s(&out)]);
    assert_eq!(code, 1);
    assert!(err.contains("не текст UTF-8"), "{err}");
    assert!(!out.exists());
}
