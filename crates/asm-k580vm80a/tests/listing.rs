//! Листинг: формат строк, перенос длинных DB, пометки раскрытий, таблица символов.

use std::path::PathBuf;

use asm_k580vm80a::{Options, assemble, assemble_file};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Листинг monitor.asm совпадает с monitor/monitor.lst строка в строку, дальше — таблица
/// символов.
#[test]
fn monitor_listing_is_monitor_lst() {
    let a = assemble_file(&root().join("monitor/monitor.asm"), &Options::default()).unwrap();
    let ours = a.listing();
    let lst = std::fs::read_to_string(root().join("monitor/monitor.lst")).unwrap();
    assert!(ours.starts_with(&lst), "листинг отличается от monitor.lst");
    let tail = &ours[lst.len()..];
    let mut lines = tail.lines();
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some("СИМВОЛЫ (106)"));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(
        lines.next(),
        Some("ИМЯ       ЗНАЧ  СЕГМЕНТ  ВИД    ГДЕ ОПРЕДЕЛЁН")
    );
    let rows: Vec<&str> = lines.collect();
    assert_eq!(rows.len(), 106);
    let names: Vec<&str> = rows
        .iter()
        .map(|r| r.split_whitespace().next().unwrap())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert!(
        rows.iter()
            .any(|r| r.starts_with("ADJUST    02E5  абс.     метка  ")
                && r.ends_with("monitor/monitor.asm:698"))
    );
    assert!(
        rows.iter()
            .any(|r| r.starts_with("CLK_H     C3FF  абс.     EQU    "))
    );
}

const SRC: &str = "\
OUTA    MACRO   V
        MVI     A,V
        OUT     0A0H
        ENDM
N       SET     1
        ORG     0C000H
START:  OUTA    5
        IF      N
        MVI     B,1
        ELSE
        MVI     B,2
        ENDIF
EMPTY:
BUF:    DS      3
MSG:    DB      'HELLO, WORLD', 0
        REPT    2
        NOP
        ENDM
        CSEG
CODE:   LXI     H,CODE
        DSEG
VAR:    DS      2
VPTR    EQU     VAR+1
        END     START
";

const LST: &str = r#"
                    OUTA    MACRO   V
                            MVI     A,V
                            OUT     0A0H
                            ENDM
     =0001          N       SET     1
                            ORG     0C000H
C000                START:  OUTA    5
C000  3E 05        +        MVI     A,5
C002  D3 A0        +        OUT     0A0H
                            IF      N
C004  06 01                 MVI     B,1
                            ELSE
                   -        MVI     B,2
                            ENDIF
C006                EMPTY:
C006                BUF:    DS      3
C009  48 45 4C 4C   MSG:    DB      'HELLO, WORLD', 0
C00D  4F 2C 20 57
C011  4F 52 4C 44
C015  00
                            REPT    2
                            NOP
                            ENDM
C016  00           +        NOP
C017  00           +        NOP
                            CSEG
0000' 21 00 00      CODE:   LXI     H,CODE
                            DSEG
0000"               VAR:    DS      2
     =0001"         VPTR    EQU     VAR+1
                            END     START

СИМВОЛЫ (9)

ИМЯ    ЗНАЧ  СЕГМЕНТ  ВИД    ГДЕ ОПРЕДЕЛЁН
BUF    C006  абс.     метка  t.asm:14
CODE   0000  CSEG     метка  t.asm:20
DEBUG  0001  абс.     -D     командная строка
EMPTY  C006  абс.     метка  t.asm:13
MSG    C009  абс.     метка  t.asm:15
N      0001  абс.     SET    t.asm:5
START  C000  абс.     метка  t.asm:7
VAR    0000  DSEG     метка  t.asm:22
VPTR   0001  DSEG     EQU    t.asm:23
"#;

#[test]
fn listing_format() {
    let o = Options {
        defines: vec![("DEBUG".into(), 1)],
        ..Options::default()
    };
    let a = assemble("t.asm", SRC, &o).unwrap();
    assert_eq!(a.listing(), &LST[1..]);
}

#[test]
fn listing_include_extern_public() {
    let d = std::env::temp_dir().join(format!("asm-k580-lst-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("main.asm"),
        "        NAME    M\n        EXTRN   PRINT\n        PUBLIC  GO\n        CSEG\n\
         GO:     CALL    PRINT\n        INCLUDE \"a.inc\"\nHI      EQU     HIGH GO\nP2      EQU     PRINT+2\n",
    )
    .unwrap();
    std::fs::write(d.join("a.inc"), "        DB 1,2,3,4,5,6,7,8\n").unwrap();
    let a = assemble_file(&d.join("main.asm"), &Options::default()).unwrap();
    let main = d.join("main.asm").display().to_string();
    let lst = a.listing().replace(&main, "main.asm");
    let _ = std::fs::remove_dir_all(&d);
    let want = r#"
                            NAME    M
                            EXTRN   PRINT
                            PUBLIC  GO
                            CSEG
0000' CD 00 00      GO:     CALL    PRINT
                            INCLUDE "a.inc"
0003' 01 02 03 04 1         DB 1,2,3,4,5,6,7,8
0007' 05 06 07 08
     =0000'         HI      EQU     HIGH GO
     =0002*         P2      EQU     PRINT+2

СИМВОЛЫ (4)

ИМЯ    ЗНАЧ  СЕГМЕНТ       ВИД           ГДЕ ОПРЕДЕЛЁН
GO     0000  CSEG          метка PUBLIC  main.asm:5
HI     0000  HIGH CSEG     EQU           main.asm:7
P2     0002  внешн. PRINT  EQU           main.asm:8
PRINT  0000  внешн. PRINT  EXTRN         main.asm:2
"#;
    assert_eq!(lst, &want[1..]);
}
