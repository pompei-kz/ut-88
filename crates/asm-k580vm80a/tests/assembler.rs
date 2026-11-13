//! Директивы, перемещаемость, ошибки с позициями.

use asm_k580vm80a::expr::{Part, RelocTarget, Value};
use asm_k580vm80a::ir::{Placement, SegKind};
use asm_k580vm80a::{Assembly, Charset, Options, assemble};

fn asm(text: &str) -> Assembly {
    assemble("t.asm", text, &Options::default()).unwrap_or_else(|e| {
        panic!(
            "{}",
            e.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

fn dump(text: &str) -> String {
    asm(text).to_image(&Placement::default()).unwrap().to_dump()
}

fn errors(text: &str) -> Vec<String> {
    errors_opts(text, &Options::default())
}

fn errors_opts(text: &str, o: &Options) -> Vec<String> {
    match assemble("t.asm", text, o) {
        Ok(a) => match a.to_image(&Placement::default()) {
            Ok(_) => panic!("ожидалась ошибка:\n{text}"),
            Err(e) => e.iter().map(ToString::to_string).collect(),
        },
        Err(e) => e.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn org_equ_set_forward_refs() {
    let d = dump(
        "\
N       EQU     3
        ORG     0C000H
START:  LXI     H,DATA      ; ссылка вперёд
        MVI     B,N*2
        JMP     START
DATA:   DB      LEN
LEN     EQU     $-DATA+1
X       SET     1
        DB      X
X       SET     X+1
        DB      X
        end     START
",
    );
    assert_eq!(d, "C000 21 08 C0 06 06 C3 00 C0 02 01 02\n");
}

#[test]
fn label_forms_and_case() {
    // Метка с двоеточием, без двоеточия с первого столбца, регистр букв не важен.
    let d = dump("        ORG 100H\nloop:   NOP\nNEXT    jmp LOOP\n        Jmp next\n");
    assert_eq!(d, "0100 00 C3 00 01 C3 01 01\n");
}

#[test]
fn db_dw_ds() {
    let d = dump(
        "\
        ORG     0
        DB      1, 'AB', \"C\"\"D\", 'A'+1, -1, NOT 0FEH
        DEFB    'Привет'
        DW      1234H, 'AB', $
        DEFW    -1
        DS      2
        DB      7
        DS      3, 0E5H
        DEFS    1
",
    );
    assert_eq!(
        d,
        "0000 01 41 42 43 22 44 42 FF 01 70 72 69 77 65 74 34\n\
         0010 12 42 41 0F 00 FF FF\n\
         0019 07 E5 E5 E5\n"
    );
}

#[test]
fn charset_ascii_option() {
    let o = Options {
        charset: Charset::Ascii,
        ..Options::default()
    };
    let e = errors_opts("  DB 'Да'\n", &o);
    assert!(
        e[0].starts_with("t.asm:1:7: ошибка: символ «Д» не входит в ASCII"),
        "{e:?}"
    );
    let a = assemble("t.asm", "  DB 'Ok'\n", &o).unwrap();
    let img = a.to_image(&Placement::default()).unwrap();
    assert_eq!(img.to_bin(0), b"Ok");
    // Ъ в КОИ-7 ЮТ-88 нет.
    assert!(errors("  DB 'Ъ'\n")[0].contains("Ъ"));
}

#[test]
fn bin_fill_and_entry() {
    let a = asm("  ORG 10H\n  DB 1\n  ORG 13H\n  DB 2\n  END 10H\n");
    let img = a.to_image(&Placement::default()).unwrap();
    assert_eq!(img.to_bin(0), vec![1, 0, 0, 2]);
    assert_eq!(img.to_bin(0xFF), vec![1, 0xFF, 0xFF, 2]);
    assert_eq!(img.entry, Some(0x10));
    assert!(img.to_ihex().contains(":0400000300000010E9\n"));
}

#[test]
fn end_stops_assembly() {
    assert_eq!(dump("  NOP\n  END\n  this is not assembled\n"), "0000 00\n");
}

#[test]
fn incbin() {
    let dir = std::env::temp_dir().join(format!("asm-k580-incbin-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("inc")).unwrap();
    std::fs::write(dir.join("inc/data.bin"), [1u8, 2, 3]).unwrap();
    let src = dir.join("main.asm");
    let text = "  ORG 100H\n  INCBIN \"data.bin\"\nAFTER: DB AFTER-100H\n";
    std::fs::write(&src, text).unwrap();
    let o = Options {
        include_dirs: vec![dir.join("inc")],
        ..Options::default()
    };
    let a = asm_k580vm80a::assemble_file(&src, &o).unwrap();
    let img = a.to_image(&Placement::default()).unwrap();
    assert_eq!(img.to_dump(), "0100 01 02 03 03\n");
    // Без -I файла не найти.
    let e = asm_k580vm80a::assemble_file(&src, &Options::default()).unwrap_err();
    assert!(e[0].msg.contains("не найден"), "{}", e[0]);
    assert_eq!(e[0].loc.as_ref().unwrap().col, 10);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn defines_from_command_line() {
    let o = Options {
        defines: vec![("N".into(), 5)],
        ..Options::default()
    };
    let a = assemble("t.asm", "  DB N\n", &o).unwrap();
    assert_eq!(
        a.to_image(&Placement::default()).unwrap().to_bin(0),
        vec![5]
    );
    let e = errors_opts("N EQU 1\n", &o);
    assert!(e[0].contains("повторное определение символа N (первое — в командной строке"));
}

#[test]
fn segments_and_relocations() {
    let a = asm("\
        NAME    PROG
        EXTRN   PRINT
        PUBLIC  START, BUF
        CSEG
START:  LXI     H,BUF
        CALL    PRINT
        MVI     A,HIGH (BUF+100H)
        MVI     B,LOW BUF
        JMP     START+1
        DW      MSG-START
MSG:    DB      'X'
        DSEG
        DS      2
BUF:    DS      10
        ASEG
        ORG     0F000H
        JMP     START
        END     START
");
    let m = &a.module;
    assert_eq!(m.name.as_deref(), Some("PROG"));
    assert_eq!(m.segment(SegKind::Code).size, 16);
    assert_eq!(m.segment(SegKind::Data).size, 12);
    assert_eq!(m.segment(SegKind::Data).chunks.len(), 0);
    let seg = |k| RelocTarget::Segment(k);
    let rel: Vec<(SegKind, u16, Part, RelocTarget, i64)> = m
        .relocs
        .iter()
        .map(|r| (r.seg, r.offset, r.part, r.target.clone(), r.addend))
        .collect();
    assert_eq!(
        rel,
        vec![
            (SegKind::Code, 1, Part::Word, seg(SegKind::Data), 2),
            (
                SegKind::Code,
                4,
                Part::Word,
                RelocTarget::Extern("PRINT".into()),
                0
            ),
            (SegKind::Code, 7, Part::High, seg(SegKind::Data), 0x102),
            (SegKind::Code, 9, Part::Low, seg(SegKind::Data), 2),
            (SegKind::Code, 11, Part::Word, seg(SegKind::Code), 1),
            (SegKind::Abs, 0xF001, Part::Word, seg(SegKind::Code), 0),
        ]
    );
    assert_eq!(m.externs.len(), 1);
    assert_eq!(m.publics.len(), 2);
    assert_eq!(m.publics[1].value, Value::in_segment(SegKind::Data, 2));
    assert_eq!(m.entry, Some(Value::in_segment(SegKind::Code, 0)));
    // Данные CSEG при базе 0: разность меток абсолютна (MSG-START = 15).
    let code = &m.segment(SegKind::Code).chunks[0].bytes;
    assert_eq!(&code[13..15], &[15, 0]);

    // В bin/hex/ihex EXTRN недопустим.
    let e = a.to_image(&Placement::default()).unwrap_err();
    assert!(
        e[0].to_string()
            .starts_with("t.asm:6:17: ошибка: модуль PROG: внешний символ PRINT не определён")
    );
}

#[test]
fn relocatable_segments_placed_in_image() {
    let a = asm("\
        CSEG
START:  LXI     H,BUF
        MVI     A,HIGH BUF
        MVI     B,LOW BUF
        JMP     START
        DSEG
BUF:    DB      0AAH
        END     START
");
    let img = a
        .to_image(&Placement {
            cseg: Some(0xC000),
            dseg: None,
        })
        .unwrap();
    // CSEG 0C000H (10 байт), DSEG сразу за ним — 0C00AH.
    assert_eq!(img.to_dump(), "C000 21 0A C0 3E C0 06 0A C3 00 C0 AA\n");
    assert_eq!(img.entry, Some(0xC000));
    let img = a
        .to_image(&Placement {
            cseg: Some(0x100),
            dseg: Some(0x2FF),
        })
        .unwrap();
    assert_eq!(
        img.chunks[0].1[..7],
        [0x21, 0xFF, 0x02, 0x3E, 0x02, 0x06, 0xFF]
    );
    // Без размещения CSEG начинается с 0 — смещения совпадают с адресами.
    let img = a.to_image(&Placement::default()).unwrap();
    assert_eq!(img.chunks[0].0, 0);
}

#[test]
fn relocation_errors() {
    let cases = [
        (
            "  CSEG\nLL: DW LL*2\n",
            "t.asm:2:10: ошибка: выражение нельзя переместить",
        ),
        (
            "  CSEG\nLL: MVI A,LL\n",
            "t.asm:2:11: ошибка: перемещаемый адрес не помещается в байт: используйте HIGH или LOW",
        ),
        (
            "  CSEG\nLL: NOP\n  DSEG\nDD: DW DD-LL\n",
            "t.asm:4:10: ошибка: выражение нельзя переместить: разность адресов из разных сегментов",
        ),
        (
            "  EXTRN EX\n  DW -EX\n",
            "t.asm:2:6: ошибка: выражение нельзя переместить",
        ),
        (
            "  CSEG\nLL: NOP\n  ASEG\n  ORG LL\n",
            "t.asm:4:7: ошибка: ORG: адрес должен быть числом или адресом текущего сегмента ASEG",
        ),
        (
            "  CSEG\nLL: DS LL\n",
            "t.asm:2:8: ошибка: DS: нужно абсолютное значение",
        ),
        (
            "  EXTRN EX\n  END EX\n",
            "t.asm:2:7: ошибка: адрес пуска в END должен быть адресом в модуле или числом",
        ),
        (
            "EX: NOP\n  EXTRN EX\n",
            "t.asm:2:9: ошибка: символ EX определён в этом модуле и не может быть EXTRN",
        ),
        (
            "  PUBLIC NOPE\n",
            "t.asm:1:10: ошибка: PUBLIC: символ NOPE не определён",
        ),
    ];
    for (src, want) in cases {
        let e = errors(src);
        assert_eq!(e[0], want, "{src}");
    }
}

#[test]
fn errors_with_positions() {
    let cases = [
        (
            "  JMP NOWHERE\n",
            "t.asm:1:7: ошибка: неизвестный символ NOWHERE",
        ),
        (
            "LL: NOP\nLL: NOP\n",
            "t.asm:2:1: ошибка: повторное определение символа LL (первое — в t.asm:1)",
        ),
        (
            "  FOO A\n",
            "t.asm:1:3: ошибка: неизвестная команда или директива FOO",
        ),
        (
            "  MOV A\n",
            "t.asm:1:3: ошибка: команде MOV нужны два операнда через запятую",
        ),
        (
            "  MOV A,B,C\n",
            "t.asm:1:11: ошибка: команде MOV нужны два операнда через запятую",
        ),
        (
            "  NOP 1\n",
            "t.asm:1:7: ошибка: у команды NOP нет операндов",
        ),
        (
            "  MOV A,X\n",
            "t.asm:1:9: ошибка: ожидается регистр B, C, D, E, H, L, M или A, а не X",
        ),
        (
            "  MOV M,M\n",
            "t.asm:1:3: ошибка: команды MOV M,M нет (на её месте HLT)",
        ),
        (
            "  LXI PSW,1\n",
            "t.asm:1:7: ошибка: ожидается регистр B, D, H или SP, а не PSW",
        ),
        (
            "  PUSH SP\n",
            "t.asm:1:8: ошибка: ожидается регистр B, D, H или PSW, а не SP",
        ),
        (
            "  STAX H\n",
            "t.asm:1:8: ошибка: ожидается регистр B или D, а не H",
        ),
        (
            "  MVI A,100H\n",
            "t.asm:1:9: ошибка: значение 100H не помещается в байт",
        ),
        (
            "  MVI A,-257\n",
            "t.asm:1:9: ошибка: значение -257 не помещается в байт",
        ),
        (
            "  LXI H,10000H\n",
            "t.asm:1:9: ошибка: значение 10000H не помещается в слово",
        ),
        (
            "  RST 8\n",
            "t.asm:1:7: ошибка: номер RST должен быть числом от 0 до 7",
        ),
        (
            "  ORG 0FFFFH\n  DW 1\n",
            "t.asm:2:6: ошибка: переполнение адреса: код выходит за FFFFH",
        ),
        (
            "  ORG 10H\n  NOP\n  ORG 10H\n  NOP\n",
            "t.asm:4:3: ошибка: перекрытие: адрес 0010H уже занят",
        ),
        (
            "  ORG X\nX EQU 1\n",
            "t.asm:1:7: ошибка: ORG: значение должно быть известно до этой строки (ссылка вперёд)",
        ),
        (
            "  DS N\nN EQU 1\n",
            "t.asm:1:6: ошибка: DS: значение должно быть известно до этой строки (ссылка вперёд)",
        ),
        ("  EQU 5\n", "t.asm:1:3: ошибка: EQU без имени символа"),
        (
            "A EQU 5\n",
            "t.asm:1:1: ошибка: имя A зарезервировано (регистр или оператор)",
        ),
        (
            "X EQU 1\nX SET 2\n",
            "t.asm:2:1: ошибка: повторное определение символа X (первое — в t.asm:1)",
        ),
        ("  DB 1,,2\n", "t.asm:1:8: ошибка: пустой операнд"),
        ("  DB (1+2\n", "t.asm:1:10: ошибка: ожидается «)»"),
        ("  DB 'abc\n", "t.asm:1:6: ошибка: не закрыта кавычка"),
        (
            "  MVI A,12G\n",
            "t.asm:1:9: ошибка: неверное число «12G»: недопустимая цифра «g» в числе с основанием 10",
        ),
        (
            "  IF 1\n",
            "t.asm:1:3: ошибка: IF без ENDIF до конца текста",
        ),
        (
            "\tCSEG 100H\n",
            "t.asm:1:7: ошибка: у CSEG нет операндов (адрес задаётся ORG)",
        ),
    ];
    for (src, want) in cases {
        let e = errors(src);
        assert_eq!(e[0], want, "{src}");
    }
}

#[test]
fn several_errors_reported() {
    let e = errors("  JMP A1\n  JMP A2\n  NOP\n  CALL A3\n");
    assert_eq!(e.len(), 3);
    assert!(e[2].starts_with("t.asm:4:8:"));
}

#[test]
fn listing_records() {
    let a = asm("N EQU 5\n  ORG 100H\nLL: MVI A,N\n  DB 1,2,3\n");
    let r: Vec<(usize, u32, Vec<u8>)> = a
        .lines
        .iter()
        .map(|l| (l.src.line, l.offset, l.bytes.clone()))
        .collect();
    assert_eq!(
        r,
        vec![
            (1, 0, vec![]),
            (2, 0, vec![]),
            (3, 0x100, vec![0x3E, 5]),
            (4, 0x102, vec![1, 2, 3]),
        ]
    );
    assert_eq!(a.lines[0].value, Some(Value::abs(5)));
    let l = a.module.symbols.iter().find(|s| s.name == "LL").unwrap();
    assert_eq!(l.value, Value::abs(0x100));
}

/// Точка расширения для препроцессора: свой источник строк видит таблицу символов
/// текущего прохода и может пропускать/добавлять строки.
#[test]
fn custom_line_source() {
    use asm_k580vm80a::error::AsmResult;
    use asm_k580vm80a::source::{LineSource, PassContext, SourceLine, SourceProvider};
    use std::sync::Arc;

    /// Игрушечный препроцессор: строка `?ИМЯ текст` попадает в сборку, только если ИМЯ
    /// определено; `!выражение` повторяет `DB N` столько раз, каково значение выражения.
    struct Toy(Vec<&'static str>);
    struct ToySource {
        lines: Vec<&'static str>,
        next: usize,
        pending: usize,
    }
    impl SourceProvider for Toy {
        fn open(&self, _: &Options) -> AsmResult<Box<dyn LineSource + '_>> {
            Ok(Box::new(ToySource {
                lines: self.0.clone(),
                next: 0,
                pending: 0,
            }))
        }
    }
    impl LineSource for ToySource {
        fn next_line(&mut self, ctx: &dyn PassContext) -> AsmResult<Option<SourceLine>> {
            loop {
                let file: Arc<str> = Arc::from("toy.asm");
                if self.pending > 0 {
                    self.pending -= 1;
                    let mut l = SourceLine::new(file, self.next, "  DB N");
                    l.expanded = true;
                    l.notes.push("из повтора".into());
                    return Ok(Some(l));
                }
                let Some(t) = self.lines.get(self.next) else {
                    return Ok(None);
                };
                self.next += 1;
                let line = SourceLine::new(file, self.next, *t);
                if let Some(rest) = t.strip_prefix('?') {
                    let (name, text) = rest.split_once(' ').unwrap();
                    if ctx.is_defined(name) {
                        return Ok(Some(SourceLine::new(line.file, line.line, text)));
                    }
                    continue;
                }
                if let Some(e) = t.strip_prefix('!') {
                    let loc = line.loc(2);
                    let v = ctx
                        .eval(e, &loc)?
                        .ok_or_else(|| asm_k580vm80a::AsmError::at(&loc, "значение неизвестно"))?;
                    self.pending = v.num as usize;
                    continue;
                }
                return Ok(Some(line));
            }
        }
    }

    let o = Options {
        defines: vec![("YES".into(), 1)],
        ..Options::default()
    };
    let toy = Toy(vec![
        "N EQU 2",
        "?YES  DB 1",
        "?NO   DB 2",
        "!N+1",
        "  DB 3",
    ]);
    let a = asm_k580vm80a::assemble_with(&toy, &o).unwrap();
    let img = a.to_image(&Placement::default()).unwrap();
    assert_eq!(img.to_bin(0), vec![1, 2, 2, 2, 3]);
    assert!(a.lines.iter().filter(|l| l.src.expanded).count() == 3);

    // Примечания строки попадают в сообщение об ошибке.
    let toy = Toy(vec!["!X"]);
    let e = asm_k580vm80a::assemble_with(&toy, &o).unwrap_err();
    assert_eq!(e[0].to_string(), "toy.asm:1:2: ошибка: значение неизвестно");
    let toy = Toy(vec!["!1"]);
    let e = asm_k580vm80a::assemble_with(&toy, &o).unwrap_err();
    assert_eq!(
        e[0].to_string(),
        "toy.asm:1:6: ошибка: неизвестный символ N\n    примечание: из повтора"
    );
}
