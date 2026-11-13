//! Препроцессор: INCLUDE, IF/IFDEF/IFNDEF/ELSE/ENDIF, MACRO/ENDM/LOCAL/EXITM, REPT.

use std::path::{Path, PathBuf};

use asm_k580vm80a::ir::Placement;
use asm_k580vm80a::{Assembly, Options, assemble, assemble_file};

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("asm-k580-pp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn p(&self, f: &str) -> PathBuf {
        self.0.join(f)
    }
    fn write(&self, f: &str, text: &str) -> PathBuf {
        let p = self.p(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn join(e: &[asm_k580vm80a::AsmError]) -> String {
    e.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn asm_opts(text: &str, o: &Options) -> Assembly {
    assemble("t.asm", text, o).unwrap_or_else(|e| panic!("{}", join(&e)))
}

fn bin_opts(text: &str, o: &Options) -> Vec<u8> {
    asm_opts(text, o)
        .to_image(&Placement::default())
        .unwrap()
        .to_bin(0)
}

fn bin(text: &str) -> Vec<u8> {
    bin_opts(text, &Options::default())
}

fn errors_opts(text: &str, o: &Options) -> Vec<String> {
    match assemble("t.asm", text, o) {
        Ok(_) => panic!("ожидалась ошибка:\n{text}"),
        Err(e) => e.iter().map(ToString::to_string).collect(),
    }
}

fn errors(text: &str) -> Vec<String> {
    errors_opts(text, &Options::default())
}

fn defines(d: &[(&str, i64)]) -> Options {
    Options {
        defines: d.iter().map(|(n, v)| ((*n).to_string(), *v)).collect(),
        ..Options::default()
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

// ---------- INCLUDE ----------

#[test]
fn include_nested_and_search_path() {
    let d = Dir::new("inc");
    // main.asm → sub/a.inc (по относительному пути) → b.inc (из папки -I lib)
    d.write(
        "main.asm",
        "        ORG 100H\n        INCLUDE \"sub/a.inc\"\n        DB 3\n",
    );
    d.write("sub/a.inc", "        DB 1\n        INCLUDE 'b.inc'\n");
    d.write("lib/b.inc", "\u{feff}        DB 2\nB_DONE  EQU 1\n");
    let o = Options {
        include_dirs: vec![d.p("lib")],
        ..Options::default()
    };
    let a = assemble_file(&d.p("main.asm"), &o).unwrap_or_else(|e| panic!("{}", join(&e)));
    let img = a.to_image(&Placement::default()).unwrap();
    assert_eq!(img.to_dump(), "0100 01 02 03\n");
    // глубина включения строк
    let depth: Vec<(usize, String)> = a
        .lines
        .iter()
        .map(|l| (l.src.include_depth, l.src.text.trim().to_string()))
        .collect();
    assert_eq!(depth[2], (1, "DB 1".to_string()));
    assert_eq!(depth[4], (2, "DB 2".to_string()));
    assert_eq!(depth[6], (0, "DB 3".to_string()));

    // без -I файл b.inc не найден
    let e = assemble_file(&d.p("main.asm"), &Options::default()).unwrap_err();
    assert_eq!(
        e[0].to_string(),
        format!(
            "{}:2:17: ошибка: INCLUDE: файл «b.inc» не найден\n    примечание: включено из {}:2",
            s(&d.p("sub/a.inc")),
            s(&d.p("main.asm"))
        )
    );
}

#[test]
fn include_error_positions() {
    let d = Dir::new("incpos");
    d.write("main.asm", "  NOP\n  INCLUDE \"a.inc\"\n");
    d.write("a.inc", "  NOP\n\n  INCLUDE \"b.inc\"\n");
    d.write("b.inc", "  MVI A,UNKNOWN\n");
    let e = assemble_file(&d.p("main.asm"), &Options::default()).unwrap_err();
    assert_eq!(
        join(&e),
        format!(
            "{}:1:9: ошибка: неизвестный символ UNKNOWN\n    \
             примечание: включено из {}:3\n    примечание: включено из {}:2",
            s(&d.p("b.inc")),
            s(&d.p("a.inc")),
            s(&d.p("main.asm"))
        )
    );
}

#[test]
fn include_recursion_is_error() {
    let d = Dir::new("increc");
    d.write("main.asm", "  INCLUDE \"a.inc\"\n");
    d.write("a.inc", "  NOP\n  INCLUDE \"main.asm\"\n");
    let e = assemble_file(&d.p("main.asm"), &Options::default()).unwrap_err();
    assert_eq!(e.len(), 1);
    let msg = e[0].to_string();
    assert!(
        msg.starts_with(&format!(
            "{}:2:11: ошибка: INCLUDE: рекурсивное включение файла",
            s(&d.p("a.inc"))
        )),
        "{msg}"
    );
    // самовключение
    d.write("self.inc", "  INCLUDE \"self.inc\"\n");
    d.write("m2.asm", "  INCLUDE \"self.inc\"\n");
    let e = assemble_file(&d.p("m2.asm"), &Options::default()).unwrap_err();
    assert!(e[0].msg.contains("рекурсивное включение"), "{}", e[0]);
}

#[test]
fn include_syntax_errors() {
    assert_eq!(
        errors("  INCLUDE nofile\n")[0],
        "t.asm:1:11: ошибка: INCLUDE: ожидается имя файла в кавычках"
    );
    assert_eq!(
        errors("  INCLUDE \"nofile.inc\"\n")[0],
        "t.asm:1:11: ошибка: INCLUDE: файл «nofile.inc» не найден"
    );
}

#[test]
fn include_unclosed_if_in_file() {
    let d = Dir::new("incif");
    d.write("main.asm", "  INCLUDE \"a.inc\"\n  ENDIF\n");
    d.write("a.inc", "  IF 1\n  NOP\n");
    let e = assemble_file(&d.p("main.asm"), &Options::default()).unwrap_err();
    let e: Vec<String> = e.iter().map(ToString::to_string).collect();
    assert_eq!(
        e,
        vec![
            format!("{}:2:3: ошибка: ENDIF без IF", s(&d.p("main.asm"))),
            format!(
                "{}:1:3: ошибка: IF без ENDIF до конца включённого файла\n    \
                 примечание: включено из {}:1",
                s(&d.p("a.inc")),
                s(&d.p("main.asm"))
            ),
        ]
    );
}

// ---------- IF ----------

#[test]
fn if_else_with_defines() {
    let src = "\
        IFDEF   DEBUG
        DB      1
        ELSE
        DB      2
        ENDIF
        IFNDEF  DEBUG
        DB      3
        ENDIF
        IF      LEVEL GT 1
        DB      4
        ELSE
        DB      5
        ENDIF
";
    assert_eq!(bin_opts(src, &defines(&[("LEVEL", 0)])), vec![2, 3, 5]);
    assert_eq!(
        bin_opts(src, &defines(&[("debug", 1), ("LEVEL", 2)])),
        vec![1, 4]
    );
}

#[test]
fn if_nested_and_symbols_above() {
    let src = "\
N       EQU     2
        IF      N
          IF    N = 1
            DB  1
          ELSE
            IF  0
              DB 0FFH
              IF 1
                DB 0FEH
              ELSE
                DB 0FDH
              ENDIF
            ELSE
              DB 2
            ENDIF
          ENDIF
        ELSE
          DB    0FCH
          IF 1
            DB  0FBH
          ENDIF
        ENDIF
LB:
        IFDEF   LB
        DB      3
        ENDIF
        IFDEF   LATER
        DB      0FAH
        ENDIF
LATER:  DB      4
";
    assert_eq!(bin(src), vec![2, 3, 4]);
}

#[test]
fn if_errors() {
    let cases = [
        (
            "  IF FWD\n  ENDIF\nFWD EQU 1\n",
            "t.asm:1:6: ошибка: IF: значение должно быть известно до этой строки (ссылка вперёд)",
        ),
        ("  IF\n  ENDIF\n", "t.asm:1:5: ошибка: IF: нужен операнд"),
        ("  ELSE\n", "t.asm:1:3: ошибка: ELSE без IF"),
        ("  ENDIF\n", "t.asm:1:3: ошибка: ENDIF без IF"),
        (
            "  IF 1\n  ELSE\n  ELSE\n  ENDIF\n",
            "t.asm:3:3: ошибка: второй ELSE для IF в t.asm:1",
        ),
        (
            "  IFDEF 1+2\n  ENDIF\n",
            "t.asm:1:9: ошибка: IFDEF: ожидается имя символа",
        ),
        (
            "  IF 1\n  NOP\n",
            "t.asm:1:3: ошибка: IF без ENDIF до конца текста",
        ),
        (
            "  IFDEF X\n  IF 1\n  ENDIF\n",
            "t.asm:1:3: ошибка: IFDEF без ENDIF до конца текста",
        ),
        (
            "  IF 1\n  ENDIF 5\n",
            "t.asm:2:9: ошибка: у ENDIF нет операндов",
        ),
        (
            "  IF 1\nX: ELSE\n  ENDIF\n",
            "t.asm:2:1: ошибка: метка перед ELSE не допускается",
        ),
        (
            "  CSEG\nLL: NOP\n  IF LL\n  ENDIF\n",
            "t.asm:3:6: ошибка: IF: нужно абсолютное значение",
        ),
    ];
    for (src, want) in cases {
        let e = errors(src);
        assert_eq!(e[0], want, "{src}");
    }
    // END внутри IF — тоже незакрытый IF
    let e = errors("  IF 1\n  END\n  ENDIF\n");
    assert_eq!(e, vec!["t.asm:1:3: ошибка: IF без ENDIF"]);
}

#[test]
fn false_branch_is_not_parsed() {
    // В ложной ветке не проверяются ни команды, ни макросы, ни INCLUDE.
    let src = "\
        IF 0
        это не команда !!!
        INCLUDE \"нет такого файла\"
        UNDEFINED_MACRO 1,2
        ENDM
        ELSE
        DB 7
        ENDIF
";
    let a = asm_opts(src, &Options::default());
    assert_eq!(
        a.to_image(&Placement::default()).unwrap().to_bin(0),
        vec![7]
    );
}

// ---------- MACRO ----------

#[test]
fn macro_params_local_nested() {
    let src = "\
        ORG     100H
; задержка: B раз по кругу
DELAY   MACRO   N
        LOCAL   LOOP
        MVI     B,N
LOOP:   DCR     B
        JNZ     LOOP
        ENDM
; вывод в порт с задержкой — вложенный вызов
OUTD    MACRO   PORT, VAL, n
        MVI     A,VAL
        OUT     PORT
        DELAY   n
        ENDM
START:  OUTD    0A0H, 'A'+1, 2
        delay   3
        JMP     START
";
    let a = asm_opts(src, &Options::default());
    assert_eq!(
        a.to_image(&Placement::default()).unwrap().to_dump(),
        "0100 3E 42 D3 A0 06 02 05 C2 06 01 06 03 05 C2 0C 01\n0110 C3 00 01\n"
    );
    // LOCAL дают разные имена на каждое раскрытие
    let locals: Vec<&str> = a
        .module
        .symbols
        .iter()
        .map(|s| s.name.as_str())
        .filter(|n| n.starts_with("??"))
        .collect();
    assert_eq!(locals, vec!["??0001", "??0002"]);
    // строки раскрытия помечены
    assert_eq!(a.lines.iter().filter(|l| l.src.expanded).count(), 11);
}

#[test]
fn macro_args_forms() {
    let src = "\
STR     MACRO   TEXT, TAIL
        DB      TEXT, TAIL
        ENDM
LBL     MACRO   N
L&N:    DB      N
        DW      L&N
        DB      '&N'      ; N в комментарии не меняется
        ENDM
OPT     MACRO   A1, A2
        DB      1 A2
        ENDM
        STR     'a,b', 0
        STR     <1, 2>, 3
        STR     (2+2)*2, 9
        LBL     5
        OPT     x
";
    let a = asm_opts(src, &Options::default());
    assert_eq!(
        a.to_image(&Placement::default()).unwrap().to_bin(0),
        vec![b'a', b',', b'b', 0, 1, 2, 3, 8, 9, 5, 9, 0, b'5', 1]
    );
    assert!(a.module.symbols.iter().any(|s| s.name == "L5"));
    let texts: Vec<&str> = a
        .lines
        .iter()
        .filter(|l| l.src.expanded)
        .map(|l| l.src.text.as_str())
        .collect();
    assert!(texts.contains(&"        DB      '5'      ; N в комментарии не меняется"));
}

#[test]
fn macro_exitm_and_recursion() {
    // Рекурсивный макрос с выходом по условию: DB N, N-1, …, 1.
    let src = "\
DOWN    MACRO   N
        IF      N = 0
        EXITM
        ENDIF
        DB      N
        DOWN    N-1
        ENDM
        DOWN    4
        DB      0FFH
";
    assert_eq!(bin(src), vec![4, 3, 2, 1, 0xFF]);

    // Бесконечная рекурсия — ошибка, а не зависание.
    let e = errors("R MACRO\n  R\n  ENDM\n  R\n");
    assert_eq!(e.len(), 1);
    assert!(
        e[0].starts_with(
            "t.asm:2:3: ошибка: слишком глубокая вложенность макросов и REPT (больше 64)"
        ),
        "{}",
        e[0]
    );
    assert!(e[0].contains("уровней вложенности"), "{}", e[0]);
    assert!(
        e[0].ends_with("из макроса R, вызванного в t.asm:4"),
        "{}",
        e[0]
    );
}

#[test]
fn macro_error_notes() {
    let src = "\
INNER   MACRO   X
        MVI     A,X
        ENDM
OUTER   MACRO
        INNER   NOSUCH
        ENDM
        NOP
        OUTER
";
    assert_eq!(
        errors(src),
        vec![
            "t.asm:2:19: ошибка: неизвестный символ NOSUCH\n    \
             примечание: из макроса INNER, вызванного в t.asm:5\n    \
             примечание: из макроса OUTER, вызванного в t.asm:8"
        ]
    );
}

#[test]
fn macro_label_on_call_and_redefinition() {
    let src = "\
        ORG     10H
TWO     MACRO
        NOP
        NOP
        ENDM
HERE:   TWO
THERE   TWO
TWO     MACRO
        HLT
        ENDM
        TWO
        DW      HERE, THERE
";
    assert_eq!(bin(src), vec![0, 0, 0, 0, 0x76, 0x10, 0, 0x12, 0]);
}

#[test]
fn macro_errors() {
    let cases = [
        (
            "  MACRO\n  ENDM\n",
            "t.asm:1:3: ошибка: MACRO без имени макроса",
        ),
        (
            "MOV MACRO\n  ENDM\n",
            "t.asm:1:1: ошибка: имя макроса MOV совпадает с командой или директивой",
        ),
        (
            "M MACRO A,A\n  ENDM\n",
            "t.asm:1:11: ошибка: MACRO: имя A повторяется",
        ),
        (
            "M MACRO 1X\n  ENDM\n",
            "t.asm:1:9: ошибка: MACRO: ожидается имя, а не «1X»",
        ),
        (
            "M MACRO A\n  DB A\n  ENDM\n  M 1,2\n",
            "t.asm:4:5: ошибка: у макроса M 1 параметр(ов), а передано 2 аргумент(ов)",
        ),
        ("  ENDM\n", "t.asm:1:3: ошибка: ENDM без MACRO или REPT"),
        ("  EXITM\n", "t.asm:1:3: ошибка: EXITM вне макроса или REPT"),
        (
            "  LOCAL X\n",
            "t.asm:1:3: ошибка: LOCAL вне макроса или REPT",
        ),
        (
            "M MACRO\n  NOP\n",
            "t.asm:1:3: ошибка: MACRO без ENDM до конца текста",
        ),
        (
            "M MACRO\n  LOCAL\n  ENDM\n  M\n",
            "t.asm:2:3: ошибка: LOCAL: нужен список имён\n    \
             примечание: из макроса M, вызванного в t.asm:4",
        ),
        (
            "M MACRO A\n  DB A\n  ENDM\n  M <1\n",
            "t.asm:4:5: ошибка: не закрыта угловая скобка «<»",
        ),
    ];
    for (src, want) in cases {
        let e = errors(src);
        assert_eq!(e[0], want, "{src}");
    }
}

// ---------- REPT ----------

#[test]
fn rept_counter_nested_local() {
    let src = "\
N       SET     0
        REPT    4
N       SET     N+1
        DB      N*N
        ENDM
        REPT    0
        DB      0FFH
        ENDM
        REPT    2
        REPT    3
        DB      7
        ENDM
        DB      8
        ENDM
CNT     EQU     2
        REPT    CNT
        LOCAL   L
L:      DW      L
        ENDM
";
    let a = asm_opts(src, &Options::default());
    assert_eq!(
        a.to_image(&Placement::default()).unwrap().to_bin(0),
        vec![1, 4, 9, 16, 7, 7, 7, 8, 7, 7, 7, 8, 12, 0, 14, 0]
    );
}

#[test]
fn rept_exitm_and_errors() {
    let src = "\
I       SET     0
        REPT    100
I       SET     I+1
        IF      I > 3
        EXITM
        ENDIF
        DB      I
        ENDM
        DB      0
";
    assert_eq!(bin(src), vec![1, 2, 3, 0]);

    let e = errors("  REPT 2\n  DB X\n  ENDM\n");
    assert_eq!(
        e[0],
        "t.asm:2:6: ошибка: неизвестный символ X\n    \
         примечание: в повторении 1 из 2 (REPT в t.asm:1)"
    );
    assert_eq!(
        e[1],
        "t.asm:2:6: ошибка: неизвестный символ X\n    \
         примечание: в повторении 2 из 2 (REPT в t.asm:1)"
    );
    assert_eq!(
        errors("  REPT N\n  ENDM\nN EQU 1\n")[0],
        "t.asm:1:8: ошибка: REPT: значение должно быть известно до этой строки (ссылка вперёд)"
    );
    assert_eq!(
        errors("  REPT -1\n  ENDM\n")[0],
        "t.asm:1:8: ошибка: REPT: неверное число повторений -1"
    );
    assert_eq!(
        errors("  REPT 2\n  NOP\n")[0],
        "t.asm:1:3: ошибка: REPT без ENDM до конца текста"
    );
    // IF, открытый в теле REPT, должен закрыться в нём же
    let e = errors("  REPT 1\n  IF 1\n  ENDM\n  ENDIF\n");
    assert_eq!(
        e,
        vec![
            "t.asm:4:3: ошибка: ENDIF без IF",
            "t.asm:2:3: ошибка: IF без ENDIF до конца тела REPT\n    \
             примечание: в повторении 1 из 1 (REPT в t.asm:1)",
        ]
    );
}

#[test]
fn rept_label_and_macro_inside() {
    let src = "\
        ORG     20H
TBL:    REPT    2
        DB      1
        ENDM
P       MACRO   V
        REPT    V
        DB      V
        ENDM
        ENDM
        P       3
        DW      TBL
";
    assert_eq!(bin(src), vec![1, 1, 3, 3, 3, 0x20, 0]);
}
