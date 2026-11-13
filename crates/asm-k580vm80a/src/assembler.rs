//! Двухпроходная сборка: символы, сегменты, команды, директивы.
//!
//! Первый проход определяет метки и размеры (ссылки вперёд дают «неизвестно»), второй —
//! генерирует код в сегменты и записывает перемещения. Строки берутся из
//! [`SourceProvider`] — между чтением файлов и сборкой можно вставить препроцессор.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::Options;
use crate::error::{AsmError, AsmResult, Loc};
use crate::expr::{self, Lookup, Part, RelocTarget, Symbols, Value};
use crate::ir::{Chunk, Extern, Module, Public, Reloc, SegKind, Symbol, SymbolKind};
use crate::lexer::{self, ParsedLine, Tok, Token, Word};
use crate::opcodes::{self, Form};
use crate::source::{LineKind, LineSource, Pass, PassContext, SourceLine, SourceProvider};

/// Директивы, которые выполняет сам ассемблер.
pub const DIRECTIVES: &[&str] = &[
    "ORG", "EQU", "SET", "DB", "DEFB", "DW", "DEFW", "DS", "DEFS", "END", "INCBIN", "CSEG", "DSEG",
    "ASEG", "NAME", "PUBLIC", "GLOBAL", "EXTRN", "EXTERN",
];

/// Директивы препроцессора: ассемблер их знает (чтобы не путать с метками), но выполнять
/// должен препроцессор — источник строк, который до ассемблера их не пропускает.
pub const PREPROCESSOR_DIRECTIVES: &[&str] = &[
    "INCLUDE", "IF", "IFDEF", "IFNDEF", "ELSE", "ENDIF", "MACRO", "ENDM", "REPT", "LOCAL", "EXITM",
];

/// Известна ли мнемоника или директива (без учёта регистра).
pub fn is_known_op(w: &str) -> bool {
    opcodes::lookup(w).is_some()
        || DIRECTIVES.iter().any(|d| d.eq_ignore_ascii_case(w))
        || PREPROCESSOR_DIRECTIVES
            .iter()
            .any(|d| d.eq_ignore_ascii_case(w))
}

/// Запись о строке второго прохода (для листинга).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRecord {
    /// Исходная строка.
    pub src: SourceLine,
    /// Сегмент на начало строки.
    pub seg: SegKind,
    /// Смещение (адрес) на начало строки.
    pub offset: u32,
    /// Сгенерированные байты (на месте перемещений — значение при базе 0).
    pub bytes: Vec<u8>,
    /// Значение символа для EQU/SET (в листинге вместо адреса).
    pub value: Option<Value>,
    /// Адрес, который показать в листинге у строки без кодов: адрес метки этой строки или
    /// начало резерва DS (сегмент и смещение).
    pub addr: Option<(SegKind, u32)>,
}

/// Результат сборки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assembly {
    /// Модуль: сегменты, перемещения, символы.
    pub module: Module,
    /// Строки второго прохода с адресами и кодами (для листинга).
    pub lines: Vec<LineRecord>,
}

impl Assembly {
    /// Образ памяти (bin/hex/ihex) с размещением перемещаемых сегментов по `p`.
    pub fn to_image(
        &self,
        p: &crate::ir::Placement,
    ) -> Result<crate::output::Image, Vec<AsmError>> {
        self.module.to_image(p)
    }
}

#[derive(Clone, Debug)]
struct SymEntry {
    name: String,
    value: Option<Value>,
    kind: SymbolKind,
    loc: Option<Loc>,
    pass: Option<Pass>,
}

struct SegMem {
    data: Vec<u8>,
    used: Vec<bool>,
}

impl SegMem {
    fn new() -> Self {
        Self {
            data: vec![0; 0x10000],
            used: vec![false; 0x10000],
        }
    }
}

/// Ассемблер (состояние сборки одного модуля).
pub struct Assembler<'o> {
    opts: &'o Options,
    pass: Pass,
    symbols: HashMap<String, SymEntry>,
    seg: SegKind,
    pc: [u32; 3],
    size: [u32; 3],
    line_start: u32,
    mem: Vec<SegMem>,
    relocs: Vec<Reloc>,
    publics: Vec<(String, Loc)>,
    externs: Vec<Extern>,
    name: Option<String>,
    entry: Option<Value>,
    errors: Vec<AsmError>,
    lines: Vec<LineRecord>,
    cur_bytes: Vec<u8>,
    cur_value: Option<Value>,
    cur_addr: Option<(SegKind, u32)>,
    ended: bool,
}

/// Собрать модуль из строк поставщика `provider`.
pub fn assemble_with(
    provider: &dyn SourceProvider,
    opts: &Options,
) -> Result<Assembly, Vec<AsmError>> {
    let mut a = Assembler::new(opts)?;
    a.run_pass(provider, Pass::First);
    if !a.errors.is_empty() {
        return Err(a.errors);
    }
    a.run_pass(provider, Pass::Second);
    a.finish()
}

impl<'o> Assembler<'o> {
    fn new(opts: &'o Options) -> Result<Self, Vec<AsmError>> {
        let mut symbols = HashMap::new();
        let mut errors = Vec::new();
        for (name, v) in &opts.defines {
            let ok = !name.is_empty()
                && name.chars().next().is_some_and(lexer::is_ident_start)
                && name.chars().all(lexer::is_ident_char)
                && !expr::is_reserved(name);
            if !ok {
                errors.push(AsmError::plain(format!(
                    "-D: недопустимое имя символа «{name}»"
                )));
                continue;
            }
            symbols.insert(
                name.to_ascii_uppercase(),
                SymEntry {
                    name: name.clone(),
                    value: Some(Value::abs(*v)),
                    kind: SymbolKind::Define,
                    loc: None,
                    pass: None,
                },
            );
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(Self {
            opts,
            pass: Pass::First,
            symbols,
            seg: SegKind::Abs,
            pc: [0; 3],
            size: [0; 3],
            line_start: 0,
            mem: Vec::new(),
            relocs: Vec::new(),
            publics: Vec::new(),
            externs: Vec::new(),
            name: None,
            entry: None,
            errors: Vec::new(),
            lines: Vec::new(),
            cur_bytes: Vec::new(),
            cur_value: None,
            cur_addr: None,
            ended: false,
        })
    }

    fn run_pass(&mut self, provider: &dyn SourceProvider, pass: Pass) {
        self.pass = pass;
        self.seg = SegKind::Abs;
        self.pc = [0; 3];
        self.size = [0; 3];
        self.relocs.clear();
        self.publics.clear();
        self.externs.clear();
        self.lines.clear();
        self.name = None;
        self.entry = None;
        self.ended = false;
        if pass == Pass::Second {
            self.mem = (0..3).map(|_| SegMem::new()).collect();
        }
        let mut src: Box<dyn LineSource + '_> = match provider.open(self.opts) {
            Ok(s) => s,
            Err(e) => {
                self.errors.push(e);
                return;
            }
        };
        while !self.ended {
            match src.next_line(&*self) {
                Ok(Some(line)) => self.process(line),
                Ok(None) => break,
                Err(e) => {
                    self.errors.push(e);
                    break;
                }
            }
        }
        let errs = src.finish(&*self);
        self.errors.extend(errs);
    }

    fn process(&mut self, line: SourceLine) {
        let seg = self.seg;
        let start = self.pc[seg.index()];
        self.line_start = start;
        self.cur_bytes.clear();
        self.cur_value = None;
        self.cur_addr = None;
        let res = match &line.kind {
            LineKind::Code => self.process_inner(&line),
            LineKind::Preprocessed { label: Some(l) } => self.define_label(l, &line),
            LineKind::Preprocessed { label: None } | LineKind::Skipped => Ok(()),
            LineKind::Error(e) => Err((**e).clone()),
        };
        if let Err(mut e) = res {
            e.notes.extend(line.notes.iter().cloned());
            self.errors.push(e);
        }
        for k in SegKind::ALL {
            let i = k.index();
            self.size[i] = self.size[i].max(self.pc[i]);
        }
        if self.pass == Pass::Second {
            self.lines.push(LineRecord {
                src: line,
                seg,
                offset: start,
                bytes: std::mem::take(&mut self.cur_bytes),
                value: self.cur_value.take(),
                addr: self.cur_addr.take(),
            });
        }
    }

    fn process_inner(&mut self, line: &SourceLine) -> AsmResult<()> {
        let p = lexer::parse_line(line, &is_known_op)?;
        if p.args.len() > 1
            && let Some(i) = p.args.iter().position(Vec::is_empty)
        {
            return Err(AsmError::at(&line.loc(p.arg_cols[i]), "пустой операнд"));
        }
        let Some(op) = p.op.clone() else {
            if let Some(l) = &p.label {
                self.define_label(l, line)?;
            }
            return Ok(());
        };
        let up = op.text.to_ascii_uppercase();
        let oploc = line.loc(op.col);
        match up.as_str() {
            "EQU" | "SET" => {
                let Some(l) = &p.label else {
                    return Err(AsmError::at(&oploc, format!("{up} без имени символа")));
                };
                let arg = self.single_arg(&p, &up, line)?;
                let v = self.eval_at(arg, line, p.args_col)?;
                let kind = if up == "EQU" {
                    SymbolKind::Equ
                } else {
                    SymbolKind::Set
                };
                self.cur_value.clone_from(&v);
                return self.define(l, v, kind, line);
            }
            "ORG" | "CSEG" | "DSEG" | "ASEG" => {
                self.segment_directive(&up, &p, line)?;
                if let Some(l) = &p.label {
                    self.define_label(l, line)?;
                }
                return Ok(());
            }
            _ => {}
        }
        if let Some(l) = &p.label {
            self.define_label(l, line)?;
        }
        if let Some(form) = opcodes::lookup(&up) {
            return self.instruction(&up, form, &p, line);
        }
        match up.as_str() {
            "DB" | "DEFB" => self.db(&p, line),
            "DW" | "DEFW" => {
                if p.args.is_empty() {
                    return Err(AsmError::at(&oploc, format!("{up} без операндов")));
                }
                for a in &p.args {
                    let v = self.eval_at(a, line, p.args_col)?;
                    self.emit_word(v, &arg_loc(a, line, p.args_col))?;
                }
                Ok(())
            }
            "DS" | "DEFS" => self.ds(&p, line),
            "END" => {
                self.ended = true;
                match p.args.as_slice() {
                    [] => Ok(()),
                    [a] => {
                        let v = self.eval_at(a, line, p.args_col)?;
                        if let Some(v) = v {
                            if matches!(
                                v.rel,
                                Some(expr::Rel {
                                    part: Part::Word,
                                    target: RelocTarget::Segment(_)
                                }) | None
                            ) {
                                self.entry = Some(v);
                            } else {
                                return Err(AsmError::at(
                                    &arg_loc(a, line, p.args_col),
                                    "адрес пуска в END должен быть адресом в модуле или числом",
                                ));
                            }
                        }
                        Ok(())
                    }
                    _ => Err(AsmError::at(
                        &oploc,
                        "END принимает не более одного операнда",
                    )),
                }
            }
            "INCBIN" => self.incbin(&p, line),
            "NAME" => {
                let a = self.single_arg(&p, "NAME", line)?;
                match a.as_slice() {
                    [
                        Token {
                            tok: Tok::Ident(s) | Tok::Str(s),
                            ..
                        },
                    ] => {
                        self.name = Some(s.clone());
                        Ok(())
                    }
                    _ => Err(AsmError::at(
                        &arg_loc(a, line, p.args_col),
                        "NAME: ожидается имя модуля",
                    )),
                }
            }
            "PUBLIC" | "GLOBAL" => {
                for (name, loc) in names_list(&up, &p, line)? {
                    if !self
                        .publics
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case(&name))
                    {
                        self.publics.push((name, loc));
                    }
                }
                Ok(())
            }
            "EXTRN" | "EXTERN" => {
                for (name, loc) in names_list(&up, &p, line)? {
                    self.declare_extern(&name, &loc)?;
                }
                Ok(())
            }
            _ if PREPROCESSOR_DIRECTIVES.contains(&up.as_str()) => Err(AsmError::at(
                &oploc,
                format!(
                    "директива {up} выполняется препроцессором, а он не подключён к этому источнику строк"
                ),
            )),
            _ => Err(AsmError::at(
                &oploc,
                format!("неизвестная команда или директива {}", op.text),
            )),
        }
    }

    // ---------- символы ----------

    fn define_label(&mut self, l: &Word, line: &SourceLine) -> AsmResult<()> {
        let pc = self.pc[self.seg.index()];
        self.cur_addr = Some((self.seg, pc));
        let v = Value::in_segment(self.seg, i64::from(pc));
        self.define(l, Some(v), SymbolKind::Label, line)
    }

    fn define(
        &mut self,
        w: &Word,
        value: Option<Value>,
        kind: SymbolKind,
        line: &SourceLine,
    ) -> AsmResult<()> {
        let loc = line.loc(w.col);
        if expr::is_reserved(&w.text) {
            return Err(AsmError::at(
                &loc,
                format!("имя {} зарезервировано (регистр или оператор)", w.text),
            ));
        }
        let key = w.text.to_ascii_uppercase();
        let pass = self.pass;
        if let Some(e) = self.symbols.get_mut(&key) {
            let where_ = e
                .loc
                .as_ref()
                .map_or("в командной строке (-D)".to_string(), |l| {
                    format!("в {}:{}", l.file, l.line)
                });
            if e.kind == SymbolKind::Extern {
                return Err(AsmError::at(
                    &loc,
                    format!("символ {} уже объявлен внешним (EXTRN) {where_}", w.text),
                ));
            }
            if kind == SymbolKind::Set && e.kind == SymbolKind::Set {
                e.value = value;
                e.pass = Some(pass);
                return Ok(());
            }
            if e.pass == Some(pass) || e.kind == SymbolKind::Define || e.kind != kind {
                return Err(AsmError::at(
                    &loc,
                    format!(
                        "повторное определение символа {} (первое — {where_})",
                        w.text
                    ),
                ));
            }
            if kind == SymbolKind::Label && e.value.is_some() && e.value != value {
                return Err(AsmError::at(
                    &loc,
                    format!(
                        "адрес метки {} на втором проходе отличается от первого",
                        w.text
                    ),
                ));
            }
            e.value = value;
            e.pass = Some(pass);
            e.loc = Some(loc);
            return Ok(());
        }
        self.symbols.insert(
            key,
            SymEntry {
                name: w.text.clone(),
                value,
                kind,
                loc: Some(loc),
                pass: Some(pass),
            },
        );
        Ok(())
    }

    fn declare_extern(&mut self, name: &str, loc: &Loc) -> AsmResult<()> {
        if expr::is_reserved(name) {
            return Err(AsmError::at(loc, format!("имя {name} зарезервировано")));
        }
        let key = name.to_ascii_uppercase();
        let pass = self.pass;
        match self.symbols.get_mut(&key) {
            Some(e) if e.kind == SymbolKind::Extern => {
                if e.pass == Some(pass) {
                    return Ok(());
                }
                e.pass = Some(pass);
            }
            Some(_) => {
                return Err(AsmError::at(
                    loc,
                    format!("символ {name} определён в этом модуле и не может быть EXTRN"),
                ));
            }
            None => {
                self.symbols.insert(
                    key,
                    SymEntry {
                        name: name.to_string(),
                        value: Some(Value::external(name)),
                        kind: SymbolKind::Extern,
                        loc: Some(loc.clone()),
                        pass: Some(pass),
                    },
                );
            }
        }
        self.externs.push(Extern {
            name: name.to_string(),
            loc: Some(loc.clone()),
        });
        Ok(())
    }

    // ---------- выражения ----------

    fn eval_at(&self, toks: &[Token], line: &SourceLine, col: usize) -> AsmResult<Option<Value>> {
        expr::eval(toks, &arg_loc(toks, line, col), self)
    }

    /// Значение, которое должно быть известно уже на первом проходе и абсолютно.
    fn eval_now(
        &self,
        toks: &[Token],
        line: &SourceLine,
        col: usize,
        what: &str,
    ) -> AsmResult<i64> {
        let loc = arg_loc(toks, line, col);
        match self.eval_at(toks, line, col)? {
            Some(v) if v.is_abs() => Ok(v.num),
            Some(_) => Err(AsmError::at(
                &loc,
                format!("{what}: нужно абсолютное значение"),
            )),
            None => Err(AsmError::at(
                &loc,
                format!("{what}: значение должно быть известно до этой строки (ссылка вперёд)"),
            )),
        }
    }

    fn single_arg<'p>(
        &self,
        p: &'p ParsedLine,
        op: &str,
        line: &SourceLine,
    ) -> AsmResult<&'p Vec<Token>> {
        match p.args.as_slice() {
            [a] => Ok(a),
            [] => Err(AsmError::at(
                &line.loc(p.args_col),
                format!("{op}: нужен операнд"),
            )),
            [_, b, ..] => Err(AsmError::at(
                &arg_loc(b, line, p.args_col),
                format!("{op}: лишний операнд"),
            )),
        }
    }

    // ---------- вывод байтов ----------

    fn emit(&mut self, bytes: &[u8], loc: &Loc) -> AsmResult<()> {
        let i = self.seg.index();
        let pc = self.pc[i];
        let end = pc + bytes.len() as u32;
        if end > 0x10000 {
            return Err(AsmError::at(
                loc,
                format!(
                    "переполнение адреса: {} выходит за FFFFH",
                    describe_seg(self.seg)
                ),
            ));
        }
        if self.pass == Pass::Second {
            let m = &mut self.mem[i];
            let (s, e) = (pc as usize, end as usize);
            if let Some(k) = m.used[s..e].iter().position(|&u| u) {
                return Err(AsmError::at(
                    loc,
                    format!(
                        "перекрытие: {}{:04X}H уже занят",
                        seg_prefix(self.seg),
                        s + k
                    ),
                ));
            }
            m.data[s..e].copy_from_slice(bytes);
            m.used[s..e].fill(true);
            self.cur_bytes.extend_from_slice(bytes);
        }
        self.pc[i] = end;
        Ok(())
    }

    fn add_reloc(&mut self, v: &Value, loc: &Loc) {
        if self.pass != Pass::Second {
            return;
        }
        if let Some(r) = &v.rel {
            self.relocs.push(Reloc {
                seg: self.seg,
                offset: self.pc[self.seg.index()] as u16,
                part: r.part,
                target: r.target.clone(),
                addend: v.num,
                loc: Some(loc.clone()),
            });
        }
    }

    fn emit_byte(&mut self, v: Option<Value>, loc: &Loc) -> AsmResult<()> {
        let b = match &v {
            None => 0,
            Some(v) => match &v.rel {
                None => {
                    if !(-256..=255).contains(&v.num) {
                        return Err(AsmError::at(
                            loc,
                            format!("значение {} не помещается в байт", fmt_num(v.num)),
                        ));
                    }
                    v.num as u8
                }
                Some(r) if r.part == Part::Word => {
                    return Err(AsmError::at(
                        loc,
                        "перемещаемый адрес не помещается в байт: используйте HIGH или LOW",
                    ));
                }
                Some(_) => {
                    self.add_reloc(v, loc);
                    v.resolve(0) as u8
                }
            },
        };
        self.emit(&[b], loc)
    }

    fn emit_word(&mut self, v: Option<Value>, loc: &Loc) -> AsmResult<()> {
        let w = match &v {
            None => 0,
            Some(v) => {
                if v.is_abs() {
                    if !(-65535..=65535).contains(&v.num) {
                        return Err(AsmError::at(
                            loc,
                            format!("значение {} не помещается в слово", fmt_num(v.num)),
                        ));
                    }
                } else {
                    self.add_reloc(v, loc);
                }
                (v.resolve(0) & 0xFFFF) as u16
            }
        };
        self.emit(&w.to_le_bytes(), loc)
    }

    // ---------- команды ----------

    fn instruction(
        &mut self,
        mn: &str,
        form: Form,
        p: &ParsedLine,
        line: &SourceLine,
    ) -> AsmResult<()> {
        let need = match form {
            Form::None(_) => 0,
            Form::Mov | Form::Mvi | Form::Lxi => 2,
            _ => 1,
        };
        let oploc = line.loc(p.op.as_ref().map_or(1, |o| o.col));
        if p.args.len() != need {
            let msg = match need {
                0 => format!("у команды {mn} нет операндов"),
                1 => format!("команде {mn} нужен один операнд"),
                _ => format!("команде {mn} нужны два операнда через запятую"),
            };
            let loc = p
                .args
                .get(need)
                .map_or(oploc, |a| arg_loc(a, line, p.args_col));
            return Err(AsmError::at(&loc, msg));
        }
        let col = p.args_col;
        let a = |i: usize| &p.args[i];
        let reg = |i: usize| reg_arg(a(i), line, col, opcodes::reg8, "B, C, D, E, H, L, M или A");
        match form {
            Form::None(c) => self.emit(&[c], &oploc),
            Form::Dst(b) => {
                let r = reg(0)?;
                self.emit(&[b | (r << 3)], &oploc)
            }
            Form::Src(b) => {
                let r = reg(0)?;
                self.emit(&[b | r], &oploc)
            }
            Form::Mov => {
                let d = reg(0)?;
                let s = reg(1)?;
                if d == 6 && s == 6 {
                    return Err(AsmError::at(
                        &oploc,
                        "команды MOV M,M нет (на её месте HLT)",
                    ));
                }
                self.emit(&[0x40 | (d << 3) | s], &oploc)
            }
            Form::Mvi => {
                let r = reg(0)?;
                let v = self.eval_at(a(1), line, col)?;
                self.emit(&[0x06 | (r << 3)], &oploc)?;
                self.emit_byte(v, &arg_loc(a(1), line, col))
            }
            Form::Imm8(c) => {
                let v = self.eval_at(a(0), line, col)?;
                self.emit(&[c], &oploc)?;
                self.emit_byte(v, &arg_loc(a(0), line, col))
            }
            Form::Imm16(c) => {
                let v = self.eval_at(a(0), line, col)?;
                self.emit(&[c], &oploc)?;
                self.emit_word(v, &arg_loc(a(0), line, col))
            }
            Form::Lxi => {
                let r = reg_arg(a(0), line, col, opcodes::rp_sp, "B, D, H или SP")?;
                let v = self.eval_at(a(1), line, col)?;
                self.emit(&[0x01 | (r << 4)], &oploc)?;
                self.emit_word(v, &arg_loc(a(1), line, col))
            }
            Form::Rp(b) => {
                let r = reg_arg(a(0), line, col, opcodes::rp_sp, "B, D, H или SP")?;
                self.emit(&[b | (r << 4)], &oploc)
            }
            Form::RpBd(b) => {
                let r = reg_arg(
                    a(0),
                    line,
                    col,
                    |s| opcodes::rp_sp(s).filter(|&r| r < 2),
                    "B или D",
                )?;
                self.emit(&[b | (r << 4)], &oploc)
            }
            Form::RpPsw(b) => {
                let r = reg_arg(a(0), line, col, opcodes::rp_psw, "B, D, H или PSW")?;
                self.emit(&[b | (r << 4)], &oploc)
            }
            Form::Rst => {
                let loc = arg_loc(a(0), line, col);
                let n = match self.eval_at(a(0), line, col)? {
                    None => 0,
                    Some(v) if v.is_abs() && (0..8).contains(&v.num) => v.num as u8,
                    Some(_) => {
                        return Err(AsmError::at(&loc, "номер RST должен быть числом от 0 до 7"));
                    }
                };
                self.emit(&[0xC7 | (n << 3)], &oploc)
            }
        }
    }

    // ---------- директивы ----------

    fn segment_directive(&mut self, up: &str, p: &ParsedLine, line: &SourceLine) -> AsmResult<()> {
        if up == "ORG" {
            let a = self.single_arg(p, "ORG", line)?;
            let loc = arg_loc(a, line, p.args_col);
            let v = match self.eval_at(a, line, p.args_col)? {
                Some(v) => v,
                None => {
                    return Err(AsmError::at(
                        &loc,
                        "ORG: значение должно быть известно до этой строки (ссылка вперёд)",
                    ));
                }
            };
            let off = match &v.rel {
                None => v.num,
                Some(expr::Rel {
                    target: RelocTarget::Segment(k),
                    part: Part::Word,
                }) if *k == self.seg => v.num,
                Some(_) => {
                    return Err(AsmError::at(
                        &loc,
                        format!(
                            "ORG: адрес должен быть числом или адресом текущего сегмента {}",
                            self.seg.name()
                        ),
                    ));
                }
            };
            if !(0..=0xFFFF).contains(&off) {
                return Err(AsmError::at(
                    &loc,
                    format!("ORG: адрес {} вне 0000H–FFFFH", fmt_num(off)),
                ));
            }
            self.pc[self.seg.index()] = off as u32;
            return Ok(());
        }
        if let Some(a) = p.args.first() {
            return Err(AsmError::at(
                &arg_loc(a, line, p.args_col),
                format!("у {up} нет операндов (адрес задаётся ORG)"),
            ));
        }
        self.seg = SegKind::from_name(up).expect("имя сегмента");
        Ok(())
    }

    fn db(&mut self, p: &ParsedLine, line: &SourceLine) -> AsmResult<()> {
        if p.args.is_empty() {
            return Err(AsmError::at(&line.loc(p.args_col), "DB без операндов"));
        }
        for a in &p.args {
            let loc = arg_loc(a, line, p.args_col);
            if let [
                Token {
                    tok: Tok::Str(s), ..
                },
            ] = a.as_slice()
            {
                let mut bytes = Vec::with_capacity(s.len());
                for (i, c) in s.chars().enumerate() {
                    let b = self
                        .opts
                        .charset
                        .encode(c)
                        .map_err(|m| AsmError::at(&loc.with_col(loc.col + 1 + i), m))?;
                    bytes.push(b);
                }
                self.emit(&bytes, &loc)?;
            } else {
                let v = self.eval_at(a, line, p.args_col)?;
                self.emit_byte(v, &loc)?;
            }
        }
        Ok(())
    }

    fn ds(&mut self, p: &ParsedLine, line: &SourceLine) -> AsmResult<()> {
        let (count, fill) = match p.args.as_slice() {
            [n] => (n, None),
            [n, f] => (n, Some(f)),
            [] => return Err(AsmError::at(&line.loc(p.args_col), "DS: нужен размер")),
            [_, _, x, ..] => {
                return Err(AsmError::at(
                    &arg_loc(x, line, p.args_col),
                    "DS: лишний операнд",
                ));
            }
        };
        let loc = arg_loc(count, line, p.args_col);
        let n = self.eval_now(count, line, p.args_col, "DS")?;
        if !(0..=0x10000).contains(&n) {
            return Err(AsmError::at(
                &loc,
                format!("DS: неверный размер {}", fmt_num(n)),
            ));
        }
        match fill {
            Some(f) => {
                let floc = arg_loc(f, line, p.args_col);
                let v = self.eval_now(f, line, p.args_col, "DS: заполнитель")?;
                if !(-256..=255).contains(&v) {
                    return Err(AsmError::at(&floc, "DS: заполнитель не помещается в байт"));
                }
                self.emit(&vec![v as u8; n as usize], &loc)
            }
            None => {
                let i = self.seg.index();
                if self.cur_addr.is_none() {
                    self.cur_addr = Some((self.seg, self.pc[i]));
                }
                if self.pc[i] + n as u32 > 0x10000 {
                    return Err(AsmError::at(
                        &loc,
                        format!(
                            "переполнение адреса: {} выходит за FFFFH",
                            describe_seg(self.seg)
                        ),
                    ));
                }
                self.pc[i] += n as u32;
                Ok(())
            }
        }
    }

    fn incbin(&mut self, p: &ParsedLine, line: &SourceLine) -> AsmResult<()> {
        let a = self.single_arg(p, "INCBIN", line)?;
        let loc = arg_loc(a, line, p.args_col);
        let [
            Token {
                tok: Tok::Str(name),
                ..
            },
        ] = a.as_slice()
        else {
            return Err(AsmError::at(&loc, "INCBIN: ожидается имя файла в кавычках"));
        };
        let path = find_file(name, line, &self.opts.include_dirs)
            .ok_or_else(|| AsmError::at(&loc, format!("INCBIN: файл «{name}» не найден")))?;
        let data = std::fs::read(&path).map_err(|e| {
            AsmError::at(&loc, format!("INCBIN: не читается {}: {e}", path.display()))
        })?;
        self.emit(&data, &loc)
    }

    // ---------- итог ----------

    fn finish(mut self) -> Result<Assembly, Vec<AsmError>> {
        let mut module = Module {
            name: self.name.take(),
            relocs: std::mem::take(&mut self.relocs),
            externs: std::mem::take(&mut self.externs),
            entry: self.entry.take(),
            ..Module::default()
        };
        for k in SegKind::ALL {
            let i = k.index();
            let seg = &mut module.segments[i];
            seg.size = self.size[i];
            let m = &self.mem[i];
            let mut a = 0usize;
            while a < 0x10000 {
                if !m.used[a] {
                    a += 1;
                    continue;
                }
                let s = a;
                while a < 0x10000 && m.used[a] {
                    a += 1;
                }
                seg.chunks.push(Chunk {
                    offset: s as u16,
                    bytes: m.data[s..a].to_vec(),
                });
            }
        }
        for (name, loc) in std::mem::take(&mut self.publics) {
            let e = self.symbols.get(&name.to_ascii_uppercase());
            match e {
                None => self.errors.push(AsmError::at(
                    &loc,
                    format!("PUBLIC: символ {name} не определён"),
                )),
                Some(e) if e.kind == SymbolKind::Extern => self.errors.push(AsmError::at(
                    &loc,
                    format!("PUBLIC: символ {name} внешний (EXTRN)"),
                )),
                Some(e) => match &e.value {
                    Some(v) if v.rel.as_ref().is_none_or(|r| r.part == Part::Word) => {
                        module.publics.push(Public {
                            name: e.name.clone(),
                            value: v.clone(),
                            loc: Some(loc),
                        });
                    }
                    _ => self.errors.push(AsmError::at(
                        &loc,
                        format!("PUBLIC: значение символа {name} нельзя экспортировать"),
                    )),
                },
            }
        }
        let publics: Vec<String> = module
            .publics
            .iter()
            .map(|p| p.name.to_ascii_uppercase())
            .collect();
        let mut symbols: Vec<Symbol> = self
            .symbols
            .iter()
            .filter_map(|(key, e)| {
                Some(Symbol {
                    name: e.name.clone(),
                    value: e.value.clone()?,
                    kind: e.kind,
                    loc: e.loc.clone(),
                    public: publics.contains(key),
                })
            })
            .collect();
        symbols.sort_by(|a, b| {
            a.name
                .to_ascii_uppercase()
                .cmp(&b.name.to_ascii_uppercase())
        });
        module.symbols = symbols;
        if !self.errors.is_empty() {
            return Err(self.errors);
        }
        Ok(Assembly {
            module,
            lines: self.lines,
        })
    }
}

impl Symbols for Assembler<'_> {
    fn lookup(&self, name: &str) -> Lookup {
        match self.symbols.get(&name.to_ascii_uppercase()) {
            Some(SymEntry { value: Some(v), .. }) => Lookup::Value(v.clone()),
            Some(_) => Lookup::Unknown,
            None => Lookup::Undefined,
        }
    }

    fn here(&self) -> Value {
        Value::in_segment(self.seg, i64::from(self.line_start))
    }

    fn charset(&self) -> crate::Charset {
        self.opts.charset
    }

    fn final_pass(&self) -> bool {
        self.pass == Pass::Second
    }
}

impl PassContext for Assembler<'_> {
    fn pass(&self) -> Pass {
        self.pass
    }

    fn eval(&self, text: &str, loc: &Loc) -> AsmResult<Option<Value>> {
        let chars: Vec<char> = text.chars().collect();
        let toks = lexer::tokenize(&chars, loc.col.saturating_sub(1), loc)?;
        expr::eval(&toks, loc, self)
    }

    fn is_defined(&self, name: &str) -> bool {
        self.symbols
            .get(&name.to_ascii_uppercase())
            .is_some_and(|e| e.kind == SymbolKind::Define || e.pass == Some(self.pass))
    }

    fn options(&self) -> &Options {
        self.opts
    }
}

/// Найти файл: абсолютный путь — как есть, иначе в папке текущего файла, затем в папках
/// `dirs` (`-I`), затем относительно текущего каталога.
pub fn find_file(name: &str, line: &SourceLine, dirs: &[PathBuf]) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute() {
        return p.exists().then(|| p.to_path_buf());
    }
    let mut cands = Vec::new();
    if let Some(d) = line.dir() {
        cands.push(d.join(p));
    }
    cands.extend(dirs.iter().map(|d| d.join(p)));
    cands.push(p.to_path_buf());
    cands.into_iter().find(|c| c.is_file())
}

/// Позиция операнда (по первой лексеме) или `col`, если операнд пуст.
fn arg_loc(toks: &[Token], line: &SourceLine, col: usize) -> Loc {
    line.loc(toks.first().map_or(col, |t| t.col))
}

fn reg_arg(
    toks: &[Token],
    line: &SourceLine,
    col: usize,
    map: impl Fn(&str) -> Option<u8>,
    allowed: &str,
) -> AsmResult<u8> {
    let loc = arg_loc(toks, line, col);
    match toks {
        [
            Token {
                tok: Tok::Ident(s), ..
            },
        ] => map(s)
            .ok_or_else(|| AsmError::at(&loc, format!("ожидается регистр {allowed}, а не {s}"))),
        _ => Err(AsmError::at(&loc, format!("ожидается регистр {allowed}"))),
    }
}

fn names_list(op: &str, p: &ParsedLine, line: &SourceLine) -> AsmResult<Vec<(String, Loc)>> {
    if p.args.is_empty() {
        return Err(AsmError::at(
            &line.loc(p.args_col),
            format!("{op}: нужен список имён"),
        ));
    }
    p.args
        .iter()
        .map(|a| match a.as_slice() {
            [
                Token {
                    tok: Tok::Ident(s),
                    col,
                },
            ] => Ok((s.clone(), line.loc(*col))),
            _ => Err(AsmError::at(
                &arg_loc(a, line, p.args_col),
                format!("{op}: ожидается имя символа"),
            )),
        })
        .collect()
}

fn fmt_num(n: i64) -> String {
    if n < 0 {
        format!("{n}")
    } else {
        format!("{n:X}H")
    }
}

fn seg_prefix(k: SegKind) -> String {
    match k {
        SegKind::Abs => "адрес ".to_string(),
        k => format!("{} смещение ", k.name()),
    }
}

fn describe_seg(k: SegKind) -> String {
    match k {
        SegKind::Abs => "код".to_string(),
        k => format!("сегмент {}", k.name()),
    }
}
