//! Препроцессор: INCLUDE, IF/IFDEF/IFNDEF/ELSE/ENDIF, MACRO/ENDM/LOCAL/EXITM, REPT/ENDM.
//!
//! [`Preprocessor`] оборачивает любой [`SourceProvider`] (обычно [`TextProvider`] основного
//! файла) и отдаёт ассемблеру строки по одной, уже раскрытые:
//!
//! * строки включённых файлов — со своей позицией (файл:строка) и примечанием «включено из
//!   a.asm:12» (цепочкой, если включения вложены);
//! * строки раскрытых макросов и REPT — с позицией строки в теле определения, пометкой
//!   `expanded` и примечанием «из макроса X, вызванного в a.asm:12»;
//! * сами директивы, строки тел определяемых макросов и вызовы макросов — как
//!   [`LineKind::Preprocessed`] (ассемблер только показывает их в листинге и определяет
//!   метку строки);
//! * строки ложных веток IF — как [`LineKind::Skipped`];
//! * ошибки директив препроцессора — как [`LineKind::Error`] у строки с директивой, чтобы
//!   сборка продолжалась и выдала все ошибки прохода.
//!
//! Условие IF вычисляется через [`PassContext::eval`] в момент чтения строки: все символы
//! выше по тексту уже определены, ссылка вперёд — ошибка (иначе на двух проходах могли бы
//! собраться разные ветки).
//!
//! Параметры макроса подставляются по словам: имя, совпадающее с именем параметра (без
//! учёта регистра), заменяется текстом аргумента; внутри кавычек — только запись `&ИМЯ`;
//! знак `&` вне кавычек склеивает слова и удаляется (`L&N` → `L1`). Комментарий не
//! меняется. Имена из LOCAL на каждое раскрытие заменяются уникальными `??0001`, `??0002`…

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::assembler::{find_file, is_known_op};
use crate::error::{AsmError, AsmResult, Loc};
use crate::lexer::{self, Word, comment_start, is_ident_char, is_ident_start, read_word};
use crate::source::{
    LineKind, LineSource, PassContext, SourceLine, SourceProvider, TextProvider, TextSource,
};

/// Наибольшая глубина вложенных INCLUDE.
pub const MAX_INCLUDE_DEPTH: usize = 16;
/// Наибольшая глубина вложенных раскрытий макросов и REPT (защита от бесконечной рекурсии).
pub const MAX_MACRO_DEPTH: usize = 64;

/// Препроцессор поверх поставщика строк основного файла.
pub struct Preprocessor<P> {
    inner: P,
}

impl<P: SourceProvider> Preprocessor<P> {
    /// Препроцессор для строк `inner`.
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

impl Preprocessor<TextProvider> {
    /// Препроцессор для текста `text` с именем файла `file`.
    pub fn text(file: &str, text: &str) -> Self {
        Self::new(TextProvider::new(file, text))
    }

    /// Препроцессор для файла.
    pub fn file(path: &Path) -> AsmResult<Self> {
        Ok(Self::new(TextProvider::from_file(path)?))
    }
}

impl<P: SourceProvider> SourceProvider for Preprocessor<P> {
    fn open(&self, opts: &crate::Options) -> AsmResult<Box<dyn LineSource + '_>> {
        let main = self.inner.open(opts)?;
        Ok(Box::new(PreSource {
            frames: vec![Frame {
                kind: FrameKind::Source {
                    src: main,
                    path: None,
                },
                notes: Vec::new(),
                depth: 0,
                expanded: false,
            }],
            macros: HashMap::new(),
            conds: Vec::new(),
            rec: None,
            pending: Vec::new(),
            uniq: 0,
        }))
    }
}

/// Определение макроса или тело REPT.
struct Body {
    /// Имя макроса (как написано) или `REPT`.
    name: String,
    /// Параметры, заглавными.
    params: Vec<String>,
    /// Имена LOCAL верхнего уровня тела, заглавными.
    locals: Vec<String>,
    /// Строки тела (с позициями в файле определения).
    lines: Vec<SourceLine>,
}

enum FrameKind<'a> {
    /// Файл (основной или включённый).
    Source {
        src: Box<dyn LineSource + 'a>,
        /// Полный путь файла (для защиты от рекурсивного INCLUDE); `None` — ещё не
        /// выяснен (у основного файла — до первой строки).
        path: Option<Option<PathBuf>>,
    },
    /// Раскрытие макроса или REPT.
    Body {
        body: Rc<Body>,
        next: usize,
        /// Замены: имя заглавными → текст.
        map: Vec<(String, String)>,
        /// Счётчик повторов REPT (у макроса — `None`).
        rept: Option<Rept>,
    },
}

struct Rept {
    iter: usize,
    count: usize,
    loc: Loc,
}

struct Frame<'a> {
    kind: FrameKind<'a>,
    /// Примечания для ошибок в строках этого кадра.
    notes: Vec<String>,
    /// Глубина INCLUDE.
    depth: usize,
    /// Строки кадра — раскрытие макроса/REPT.
    expanded: bool,
}

/// Открытый IF.
struct Cond {
    /// Внешние условия истинны (иначе обе ветки ложны).
    parent: bool,
    /// Истинно ли условие IF.
    value: bool,
    /// Уже был ELSE.
    in_else: bool,
    /// Кадр, в котором открыт IF.
    frame: usize,
    /// Где открыт.
    loc: Loc,
    /// Директива (`IF`, `IFDEF`, `IFNDEF`).
    what: String,
    /// Примечания строки с IF.
    notes: Vec<String>,
}

impl Cond {
    fn taking(&self) -> bool {
        self.parent && (self.value != self.in_else)
    }
}

/// Записываемое определение (MACRO или REPT до ENDM).
struct Recording {
    /// Имя макроса; `None` — REPT; `Some("")` — ошибочное определение (не сохранять).
    name: Option<String>,
    params: Vec<String>,
    /// Число повторений REPT.
    count: usize,
    lines: Vec<SourceLine>,
    /// Вложенность MACRO/REPT внутри записываемого тела.
    depth: usize,
    /// Кадр, в котором начато определение.
    frame: usize,
    loc: Loc,
    /// Примечания строки с MACRO/REPT.
    notes: Vec<String>,
    /// Глубина INCLUDE строки с MACRO/REPT.
    inc_depth: usize,
}

struct PreSource<'a> {
    frames: Vec<Frame<'a>>,
    macros: HashMap<String, Rc<Body>>,
    conds: Vec<Cond>,
    rec: Option<Recording>,
    pending: Vec<AsmError>,
    uniq: u32,
}

/// Начало строки: метка, мнемоника/директива и остаток (операнды без комментария).
#[derive(Debug, Default)]
struct Head {
    label: Option<Word>,
    op: Option<Word>,
    rest: String,
    rest_col: usize,
}

impl Head {
    fn op_upper(&self) -> String {
        self.op
            .as_ref()
            .map(|w| w.text.to_ascii_uppercase())
            .unwrap_or_default()
    }
}

/// Разобрать начало строки по тем же правилам, что лексер ассемблера: метка с двоеточием
/// где угодно или без двоеточия с первого столбца (если это не известная мнемоника,
/// директива или имя макроса), имя перед EQU/SET/MACRO — метка.
fn parse_head(text: &str, is_op: &dyn Fn(&str) -> bool) -> Head {
    let all: Vec<char> = text.chars().collect();
    let chars = &all[..comment_start(&all)];
    let skip_ws = |mut i: usize| {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        i
    };
    let mut h = Head::default();
    let mut i = skip_ws(0);
    if i < chars.len() && is_ident_start(chars[i]) {
        let s = i;
        let e = read_word(chars, s);
        let w: String = chars[s..e].iter().collect();
        let j = skip_ws(e);
        let next_is_macro = chars[j..read_word(chars, j)]
            .iter()
            .collect::<String>()
            .eq_ignore_ascii_case("MACRO");
        if chars.get(e) == Some(&':') {
            h.label = Some(Word {
                text: w,
                col: s + 1,
            });
            i = e + 1;
            if chars.get(i) == Some(&':') {
                i += 1;
            }
        } else if next_is_macro {
            // имя MACRO — имя макроса, даже если совпадает с командой или макросом
            h.label = Some(Word {
                text: w,
                col: s + 1,
            });
            i = e;
        } else if !is_op(&w) {
            let j = skip_ws(e);
            let next: String = chars[j..read_word(chars, j)].iter().collect();
            let defines = matches!(next.to_ascii_uppercase().as_str(), "EQU" | "SET" | "MACRO");
            if s == 0 || defines {
                h.label = Some(Word {
                    text: w,
                    col: s + 1,
                });
                i = e;
            }
        }
    }
    i = skip_ws(i);
    if i < chars.len() && is_ident_start(chars[i]) {
        let e = read_word(chars, i);
        h.op = Some(Word {
            text: chars[i..e].iter().collect(),
            col: i + 1,
        });
        i = skip_ws(e);
    }
    let i = i.min(chars.len());
    h.rest_col = i + 1;
    h.rest = chars[i..].iter().collect::<String>().trim_end().to_string();
    h
}

/// Подставить замены `map` (имя заглавными → текст) в строку.
fn substitute(text: &str, map: &[(String, String)]) -> String {
    if map.is_empty() {
        return text.to_string();
    }
    let find = |w: &str| {
        map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(w))
            .map(|(_, v)| v.as_str())
    };
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == '&' && chars.get(i + 1).is_some_and(|&n| is_ident_start(n)) {
                let e = read_word(&chars, i + 1);
                let w: String = chars[i + 1..e].iter().collect();
                if let Some(v) = find(&w) {
                    out.push_str(v);
                    i = e;
                    continue;
                }
            }
            if c == q {
                if chars.get(i + 1) == Some(&q) {
                    out.push(c);
                    i += 1;
                } else {
                    quote = None;
                }
            }
            out.push(c);
            i += 1;
            continue;
        }
        match c {
            ';' => {
                out.extend(&chars[i..]);
                break;
            }
            '\'' | '"' => {
                quote = Some(c);
                out.push(c);
                i += 1;
            }
            '&' => i += 1,
            c if c.is_ascii_digit() => {
                let s = i;
                while i < chars.len() && is_ident_char(chars[i]) {
                    i += 1;
                }
                out.extend(&chars[s..i]);
            }
            c if is_ident_start(c) => {
                let e = read_word(&chars, i);
                let w: String = chars[i..e].iter().collect();
                out.push_str(find(&w).unwrap_or(&w));
                i = e;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Разделить аргументы вызова макроса по запятым верхнего уровня (вне кавычек, скобок и
/// угловых скобок `<…>`; угловые скобки вокруг аргумента снимаются).
fn split_macro_args(rest: &str, loc: &Loc) -> AsmResult<Vec<String>> {
    if rest.trim().is_empty() {
        return Ok(Vec::new());
    }
    let at = |i: usize| loc.with_col(loc.col + i);
    let chars: Vec<char> = rest.chars().collect();
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut bracketed = false;
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ',' && depth == 0 {
            args.push(if bracketed {
                cur.clone()
            } else {
                cur.trim().to_string()
            });
            cur.clear();
            bracketed = false;
            i += 1;
            continue;
        }
        if bracketed {
            if !c.is_whitespace() {
                return Err(AsmError::at(
                    &at(i),
                    "после аргумента в <…> ожидается запятая",
                ));
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => {
                let s = i;
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(AsmError::at(&at(s), "не закрыта кавычка")),
                        Some(&ch) if ch == c => {
                            if chars.get(i + 1) == Some(&c) {
                                i += 2;
                            } else {
                                i += 1;
                                break;
                            }
                        }
                        Some(_) => i += 1,
                    }
                }
                cur.extend(&chars[s..i]);
                continue;
            }
            '<' if depth == 0 && cur.trim().is_empty() => {
                let Some(e) = chars[i..].iter().position(|&ch| ch == '>') else {
                    return Err(AsmError::at(&at(i), "не закрыта угловая скобка «<»"));
                };
                cur = chars[i + 1..i + e].iter().collect();
                bracketed = true;
                i += e + 1;
                continue;
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        cur.push(c);
        i += 1;
    }
    args.push(if bracketed {
        cur
    } else {
        cur.trim().to_string()
    });
    Ok(args)
}

/// Список имён через запятую (параметры MACRO, LOCAL).
fn name_list(rest: &str, loc: &Loc, what: &str) -> AsmResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    if rest.trim().is_empty() {
        return Ok(out);
    }
    let mut col = loc.col;
    for part in rest.split(',') {
        let lead = part.chars().take_while(|c| c.is_whitespace()).count();
        let name = part.trim();
        let ok = name.chars().next().is_some_and(is_ident_start) && name.chars().all(is_ident_char);
        let at = loc.with_col(col + lead);
        if !ok {
            return Err(AsmError::at(
                &at,
                format!("{what}: ожидается имя, а не «{name}»"),
            ));
        }
        if out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            return Err(AsmError::at(&at, format!("{what}: имя {name} повторяется")));
        }
        out.push(name.to_ascii_uppercase());
        col += part.chars().count() + 1;
    }
    Ok(out)
}

/// Имена LOCAL верхнего уровня тела (не во вложенных определениях MACRO/REPT).
fn scan_locals(lines: &[SourceLine], is_op: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for l in lines {
        let h = parse_head(&l.text, is_op);
        match h.op_upper().as_str() {
            "MACRO" | "REPT" => depth += 1,
            "ENDM" => depth = depth.saturating_sub(1),
            "LOCAL" if depth == 0 => {
                let loc = l.loc(h.rest_col);
                if let Ok(names) = name_list(&h.rest, &loc, "LOCAL") {
                    for n in names {
                        if !out.contains(&n) {
                            out.push(n);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Длинную цепочку примечаний (глубокая рекурсия макросов) сократить: первые и последние
/// звенья и число пропущенных.
fn trim_notes(notes: &[String]) -> Vec<String> {
    const HEAD: usize = 5;
    const TAIL: usize = 3;
    if notes.len() <= HEAD + TAIL + 1 {
        return notes.to_vec();
    }
    let mut v = notes[..HEAD].to_vec();
    v.push(format!(
        "… ещё {} уровней вложенности …",
        notes.len() - HEAD - TAIL
    ));
    v.extend_from_slice(&notes[notes.len() - TAIL..]);
    v
}

fn where_(l: &SourceLine) -> String {
    format!("{}:{}", l.file, l.line)
}

impl PreSource<'_> {
    fn is_op(&self) -> impl Fn(&str) -> bool + '_ {
        |w: &str| is_known_op(w) || self.macros.contains_key(&w.to_ascii_uppercase())
    }

    /// Полная цепочка примечаний строки `line` текущего кадра (в самой строке она может
    /// быть сокращена).
    fn chain(&self, line: &SourceLine) -> Vec<String> {
        match self.frames.last() {
            Some(f) if !f.notes.is_empty() => f.notes.clone(),
            _ => line.notes.clone(),
        }
    }

    fn active(&self) -> bool {
        self.conds.iter().all(Cond::taking)
    }

    fn top(&self) -> usize {
        self.frames.len() - 1
    }

    fn frame_name(&self, i: usize) -> String {
        match &self.frames[i].kind {
            FrameKind::Source { .. } if i == 0 => "до конца текста".to_string(),
            FrameKind::Source { .. } => "до конца включённого файла".to_string(),
            FrameKind::Body { body, .. } if body.name == "REPT" => "до конца тела REPT".into(),
            FrameKind::Body { body, .. } => format!("до конца макроса {}", body.name),
        }
    }

    /// Закрыть всё, что открыто в кадрах `from..`: незакрытые IF и определения — ошибки.
    fn close_frames_from(&mut self, from: usize, report: bool) {
        if report {
            let what = self.frame_name(from.min(self.top()));
            for c in self.conds.iter().filter(|c| c.frame >= from) {
                let mut e = AsmError::at(&c.loc, format!("{} без ENDIF {what}", c.what));
                e.notes.clone_from(&c.notes);
                self.pending.push(e);
            }
            if let Some(r) = &self.rec
                && r.frame >= from
            {
                let d = if r.name.is_some() { "MACRO" } else { "REPT" };
                let mut e = AsmError::at(&r.loc, format!("{d} без ENDM {what}"));
                e.notes = trim_notes(&r.notes);
                self.pending.push(e);
            }
        }
        self.conds.retain(|c| c.frame < from);
        if self.rec.as_ref().is_some_and(|r| r.frame >= from) {
            self.rec = None;
        }
    }

    fn local_map(&mut self, body: &Body) -> Vec<(String, String)> {
        body.locals
            .iter()
            .map(|n| {
                self.uniq += 1;
                (n.clone(), format!("??{:04}", self.uniq))
            })
            .collect()
    }

    /// Следующая сырая строка из стека кадров (с подстановкой параметров).
    fn raw(&mut self, ctx: &dyn PassContext) -> AsmResult<Option<SourceLine>> {
        loop {
            let Some(top) = self.frames.len().checked_sub(1) else {
                return Ok(None);
            };
            let f = &mut self.frames[top];
            match &mut f.kind {
                FrameKind::Source { src, path } => {
                    if let Some(mut l) = src.next_line(ctx)? {
                        if path.is_none() {
                            *path = Some(Path::new(&*l.file).canonicalize().ok());
                        }
                        l.notes.extend(trim_notes(&f.notes));
                        l.include_depth = f.depth;
                        l.expanded |= f.expanded;
                        return Ok(Some(l));
                    }
                }
                FrameKind::Body {
                    body,
                    next,
                    map,
                    rept,
                } => {
                    if let Some(b) = body.lines.get(*next) {
                        *next += 1;
                        let mut l =
                            SourceLine::new(b.file.clone(), b.line, substitute(&b.text, map));
                        l.notes = trim_notes(&f.notes);
                        l.include_depth = f.depth;
                        l.expanded = true;
                        return Ok(Some(l));
                    }
                    if let Some(r) = rept
                        && r.iter + 1 < r.count
                    {
                        r.iter += 1;
                        *next = 0;
                        let note = format!(
                            "в повторении {} из {} (REPT в {}:{})",
                            r.iter + 1,
                            r.count,
                            r.loc.file,
                            r.loc.line
                        );
                        f.notes[0] = note;
                        let body = body.clone();
                        self.close_frames_from(top, true);
                        let m = self.local_map(&body);
                        if let FrameKind::Body { map, .. } = &mut self.frames[top].kind {
                            *map = m;
                        }
                        continue;
                    }
                }
            }
            self.close_frames_from(top, true);
            self.frames.pop();
        }
    }

    fn err(mut line: SourceLine, e: AsmError) -> SourceLine {
        line.kind = LineKind::Error(Box::new(e));
        line
    }

    fn done(mut line: SourceLine, label: Option<Word>) -> SourceLine {
        line.kind = LineKind::Preprocessed { label };
        line
    }

    fn no_label(line: SourceLine, h: &Head, op: &str) -> SourceLine {
        match &h.label {
            Some(l) => {
                let loc = line.loc(l.col);
                Self::err(
                    line,
                    AsmError::at(&loc, format!("метка перед {op} не допускается")),
                )
            }
            None => Self::done(line, None),
        }
    }

    fn no_args(line: SourceLine, h: &Head, op: &str) -> SourceLine {
        if h.rest.is_empty() {
            return Self::no_label(line, h, op);
        }
        let loc = line.loc(h.rest_col);
        Self::err(line, AsmError::at(&loc, format!("у {op} нет операндов")))
    }

    /// Строка внутри записываемого определения MACRO/REPT.
    fn record(&mut self, line: SourceLine, h: &Head) -> SourceLine {
        let rec = self.rec.as_mut().expect("идёт запись");
        match h.op_upper().as_str() {
            "MACRO" | "REPT" => rec.depth += 1,
            "ENDM" => {
                if rec.depth == 0 {
                    let rec = self.rec.take().expect("идёт запись");
                    return self.finish_recording(rec, line, h);
                }
                rec.depth -= 1;
            }
            _ => {}
        }
        let mut b = line.clone();
        b.notes.clear();
        b.kind = LineKind::Code;
        rec.lines.push(b);
        Self::done(line, None)
    }

    fn finish_recording(&mut self, rec: Recording, line: SourceLine, h: &Head) -> SourceLine {
        let line = Self::no_args(line, h, "ENDM");
        let is_op = |w: &str| is_known_op(w) || self.macros.contains_key(&w.to_ascii_uppercase());
        let locals = scan_locals(&rec.lines, &is_op);
        match rec.name {
            Some(name) => {
                if !name.is_empty() {
                    let body = Body {
                        name: name.clone(),
                        params: rec.params,
                        locals,
                        lines: rec.lines,
                    };
                    self.macros.insert(name.to_ascii_uppercase(), Rc::new(body));
                }
            }
            None => {
                if rec.count > 0 && !rec.lines.is_empty() {
                    if self.body_depth() >= MAX_MACRO_DEPTH {
                        return Self::err(
                            line,
                            AsmError::at(
                                &rec.loc,
                                format!(
                                    "слишком глубокая вложенность макросов и REPT (больше \
                                     {MAX_MACRO_DEPTH}) — возможна бесконечная рекурсия"
                                ),
                            ),
                        );
                    }
                    let body = Rc::new(Body {
                        name: "REPT".into(),
                        params: Vec::new(),
                        locals,
                        lines: rec.lines,
                    });
                    let map = self.local_map(&body);
                    let mut notes = vec![format!(
                        "в повторении 1 из {} (REPT в {}:{})",
                        rec.count, rec.loc.file, rec.loc.line
                    )];
                    notes.extend(rec.notes);
                    self.frames.push(Frame {
                        kind: FrameKind::Body {
                            body,
                            next: 0,
                            map,
                            rept: Some(Rept {
                                iter: 0,
                                count: rec.count,
                                loc: rec.loc,
                            }),
                        },
                        notes,
                        depth: rec.inc_depth,
                        expanded: true,
                    });
                }
            }
        }
        line
    }

    fn body_depth(&self) -> usize {
        self.frames
            .iter()
            .filter(|f| matches!(f.kind, FrameKind::Body { .. }))
            .count()
    }

    /// Вычислить условие IF/REPT: значение должно быть известно и абсолютно.
    fn eval_now(ctx: &dyn PassContext, line: &SourceLine, h: &Head, what: &str) -> AsmResult<i64> {
        let loc = line.loc(h.rest_col);
        if h.rest.is_empty() {
            return Err(AsmError::at(&loc, format!("{what}: нужен операнд")));
        }
        match ctx.eval(&h.rest, &loc)? {
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

    fn open_if(
        &mut self,
        ctx: &dyn PassContext,
        line: SourceLine,
        h: &Head,
        op: &str,
    ) -> SourceLine {
        let loc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
        let parent = self.active();
        let mut cond = Cond {
            parent,
            value: false,
            in_else: false,
            frame: self.top(),
            loc,
            what: op.to_string(),
            notes: trim_notes(&self.chain(&line)),
        };
        if !parent {
            self.conds.push(cond);
            let mut line = line;
            line.kind = LineKind::Skipped;
            return line;
        }
        let res = if op == "IF" {
            Self::eval_now(ctx, &line, h, "IF").map(|v| v != 0)
        } else {
            let loc = line.loc(h.rest_col);
            match lexer::tokenize(&h.rest.chars().collect::<Vec<_>>(), h.rest_col - 1, &loc)
                .map(|t| t.into_iter().map(|t| t.tok).collect::<Vec<_>>())
            {
                Ok(t) => match t.as_slice() {
                    [lexer::Tok::Ident(name)] => Ok(ctx.is_defined(name) == (op == "IFDEF")),
                    _ => Err(AsmError::at(&loc, format!("{op}: ожидается имя символа"))),
                },
                Err(e) => Err(e),
            }
        };
        match res {
            Ok(v) => {
                cond.value = v;
                self.conds.push(cond);
                let label = h.label.clone();
                Self::done(line, label)
            }
            Err(e) => {
                // Обе ветки — ложные, чтобы не было наведённых ошибок.
                cond.parent = false;
                self.conds.push(cond);
                Self::err(line, e)
            }
        }
    }

    fn else_endif(&mut self, line: SourceLine, h: &Head, op: &str) -> SourceLine {
        let loc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
        let top = self.top();
        let Some(c) = self.conds.last_mut().filter(|c| c.frame == top) else {
            let msg = if self.conds.is_empty() {
                format!("{op} без IF")
            } else {
                format!("{op} без IF в этом файле или макросе")
            };
            return Self::err(line, AsmError::at(&loc, msg));
        };
        if op == "ELSE" {
            if c.in_else {
                return Self::err(
                    line,
                    AsmError::at(
                        &loc,
                        format!("второй ELSE для {} в {}:{}", c.what, c.loc.file, c.loc.line),
                    ),
                );
            }
            c.in_else = true;
        } else {
            self.conds.pop();
        }
        Self::no_args(line, h, op)
    }

    fn start_macro(&mut self, line: SourceLine, h: &Head) -> SourceLine {
        let oploc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
        let mut rec = Recording {
            name: Some(String::new()),
            params: Vec::new(),
            count: 0,
            lines: Vec::new(),
            depth: 0,
            frame: self.top(),
            loc: oploc.clone(),
            notes: self.chain(&line),
            inc_depth: line.include_depth,
        };
        let res = (|| {
            let Some(name) = &h.label else {
                return Err(AsmError::at(&oploc, "MACRO без имени макроса"));
            };
            if is_known_op(&name.text) {
                return Err(AsmError::at(
                    &line.loc(name.col),
                    format!(
                        "имя макроса {} совпадает с командой или директивой",
                        name.text
                    ),
                ));
            }
            rec.params = name_list(&h.rest, &line.loc(h.rest_col), "MACRO")?;
            rec.name = Some(name.text.clone());
            Ok(())
        })();
        self.rec = Some(rec);
        match res {
            Ok(()) => Self::done(line, None),
            Err(e) => Self::err(line, e),
        }
    }

    fn start_rept(&mut self, ctx: &dyn PassContext, line: SourceLine, h: &Head) -> SourceLine {
        let oploc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
        let res = Self::eval_now(ctx, &line, h, "REPT").and_then(|n| {
            if (0..=0xFFFF).contains(&n) {
                Ok(n as usize)
            } else {
                Err(AsmError::at(
                    &line.loc(h.rest_col),
                    format!("REPT: неверное число повторений {n}"),
                ))
            }
        });
        self.rec = Some(Recording {
            name: None,
            params: Vec::new(),
            count: *res.as_ref().unwrap_or(&0),
            lines: Vec::new(),
            depth: 0,
            frame: self.top(),
            loc: oploc,
            notes: self.chain(&line),
            inc_depth: line.include_depth,
        });
        match res {
            Ok(_) => {
                let label = h.label.clone();
                Self::done(line, label)
            }
            Err(e) => Self::err(line, e),
        }
    }

    fn include(&mut self, ctx: &dyn PassContext, line: SourceLine, h: &Head) -> SourceLine {
        let loc = line.loc(h.rest_col);
        let res = (|| {
            let t = h.rest.trim();
            let name = match t.chars().next() {
                Some(q @ ('"' | '\'')) if t.len() >= 2 && t.ends_with(q) => &t[1..t.len() - 1],
                _ => {
                    return Err(AsmError::at(
                        &loc,
                        "INCLUDE: ожидается имя файла в кавычках",
                    ));
                }
            };
            let path = find_file(name, &line, &ctx.options().include_dirs)
                .ok_or_else(|| AsmError::at(&loc, format!("INCLUDE: файл «{name}» не найден")))?;
            let canon = path.canonicalize().ok();
            let recursive = canon.is_some()
                && self.frames.iter().any(|f| match &f.kind {
                    FrameKind::Source { path: Some(p), .. } => *p == canon,
                    _ => false,
                });
            if recursive {
                return Err(AsmError::at(
                    &loc,
                    format!(
                        "INCLUDE: рекурсивное включение файла {} (он уже включается)",
                        path.display()
                    ),
                ));
            }
            if line.include_depth >= MAX_INCLUDE_DEPTH {
                return Err(AsmError::at(
                    &loc,
                    format!("INCLUDE: слишком глубокая вложенность (больше {MAX_INCLUDE_DEPTH})"),
                ));
            }
            let tp = TextProvider::from_file(&path).map_err(|e| AsmError::at(&loc, e.msg))?;
            let mut notes = vec![format!("включено из {}", where_(&line))];
            notes.extend(self.chain(&line));
            Ok(Frame {
                kind: FrameKind::Source {
                    src: Box::new(TextSource::new(tp.file.clone(), &tp.text)),
                    path: Some(canon),
                },
                notes,
                depth: line.include_depth + 1,
                expanded: line.expanded,
            })
        })();
        match res {
            Ok(f) => {
                self.frames.push(f);
                let label = h.label.clone();
                Self::done(line, label)
            }
            Err(e) => Self::err(line, e),
        }
    }

    fn call_macro(&mut self, line: SourceLine, h: &Head, m: Rc<Body>) -> SourceLine {
        let loc = line.loc(h.rest_col);
        let oploc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
        let args = match split_macro_args(&h.rest, &loc) {
            Ok(a) => a,
            Err(e) => return Self::err(line, e),
        };
        if args.len() > m.params.len() {
            return Self::err(
                line,
                AsmError::at(
                    &loc,
                    format!(
                        "у макроса {} {} параметр(ов), а передано {} аргумент(ов)",
                        m.name,
                        m.params.len(),
                        args.len()
                    ),
                ),
            );
        }
        if self.body_depth() >= MAX_MACRO_DEPTH {
            return Self::err(
                line,
                AsmError::at(
                    &oploc,
                    format!(
                        "слишком глубокая вложенность макросов и REPT (больше {MAX_MACRO_DEPTH}) \
                         — возможна бесконечная рекурсия макроса {}",
                        m.name
                    ),
                ),
            );
        }
        let mut map: Vec<(String, String)> = m
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), args.get(i).cloned().unwrap_or_default()))
            .collect();
        map.extend(self.local_map(&m));
        let mut notes = vec![format!(
            "из макроса {}, вызванного в {}",
            m.name,
            where_(&line)
        )];
        notes.extend(self.chain(&line));
        self.frames.push(Frame {
            kind: FrameKind::Body {
                body: m,
                next: 0,
                map,
                rept: None,
            },
            notes,
            depth: line.include_depth,
            expanded: true,
        });
        let label = h.label.clone();
        Self::done(line, label)
    }

    fn exitm(&mut self, line: SourceLine, h: &Head) -> SourceLine {
        let Some(k) = self
            .frames
            .iter()
            .rposition(|f| matches!(f.kind, FrameKind::Body { .. }))
        else {
            let loc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
            return Self::err(line, AsmError::at(&loc, "EXITM вне макроса или REPT"));
        };
        let line = Self::no_args(line, h, "EXITM");
        if matches!(line.kind, LineKind::Error(_)) {
            return line;
        }
        self.close_frames_from(k, false);
        self.frames.truncate(k);
        line
    }

    fn local(&mut self, line: SourceLine, h: &Head) -> SourceLine {
        let oploc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
        if !matches!(
            self.frames.last().map(|f| &f.kind),
            Some(FrameKind::Body { .. })
        ) {
            return Self::err(line, AsmError::at(&oploc, "LOCAL вне макроса или REPT"));
        }
        if h.rest.is_empty() {
            return Self::err(line, AsmError::at(&oploc, "LOCAL: нужен список имён"));
        }
        if let Err(e) = name_list(&h.rest, &line.loc(h.rest_col), "LOCAL") {
            return Self::err(line, e);
        }
        Self::no_label(line, h, "LOCAL")
    }
}

impl LineSource for PreSource<'_> {
    fn next_line(&mut self, ctx: &dyn PassContext) -> AsmResult<Option<SourceLine>> {
        let Some(mut line) = self.raw(ctx)? else {
            return Ok(None);
        };
        let h = parse_head(&line.text, &self.is_op());
        if self.rec.is_some() {
            return Ok(Some(self.record(line, &h)));
        }
        let op = h.op_upper();
        let out = match op.as_str() {
            "IF" | "IFDEF" | "IFNDEF" => self.open_if(ctx, line, &h, &op),
            "ELSE" | "ENDIF" => self.else_endif(line, &h, &op),
            _ if !self.active() => {
                line.kind = LineKind::Skipped;
                line
            }
            "MACRO" => self.start_macro(line, &h),
            "REPT" => self.start_rept(ctx, line, &h),
            "ENDM" => {
                let loc = line.loc(h.op.as_ref().map_or(1, |w| w.col));
                Self::err(line, AsmError::at(&loc, "ENDM без MACRO или REPT"))
            }
            "EXITM" => self.exitm(line, &h),
            "LOCAL" => self.local(line, &h),
            "INCLUDE" => self.include(ctx, line, &h),
            _ => match self.macros.get(&op).cloned() {
                Some(m) => self.call_macro(line, &h, m),
                None => line,
            },
        };
        Ok(Some(out))
    }

    fn finish(&mut self, _ctx: &dyn PassContext) -> Vec<AsmError> {
        for c in &self.conds {
            let mut e = AsmError::at(&c.loc, format!("{} без ENDIF", c.what));
            e.notes.clone_from(&c.notes);
            self.pending.push(e);
        }
        self.conds.clear();
        if let Some(r) = self.rec.take() {
            let d = if r.name.is_some() { "MACRO" } else { "REPT" };
            let mut e = AsmError::at(&r.loc, format!("{d} без ENDM"));
            e.notes = trim_notes(&r.notes);
            self.pending.push(e);
        }
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn map(p: &[(&str, &str)]) -> Vec<(String, String)> {
        p.iter()
            .map(|(a, b)| (a.to_ascii_uppercase(), (*b).to_string()))
            .collect()
    }

    #[test]
    fn substitution() {
        let m = map(&[("x", "B"), ("n", "1")]);
        assert_eq!(substitute("  MOV A,X ; X", &m), "  MOV A,B ; X");
        assert_eq!(
            substitute("L&N: DB 'X&N', \"&x\"", &m),
            "L1: DB 'X1', \"B\""
        );
        assert_eq!(substitute("  DB 0AH,1N,XX", &m), "  DB 0AH,1N,XX");
        assert_eq!(substitute("  DB 'It''s X'", &m), "  DB 'It''s X'");
    }

    #[test]
    fn macro_args() {
        let loc = Loc::new(Arc::from("t"), 1, 1);
        let a = |s: &str| split_macro_args(s, &loc).unwrap();
        assert_eq!(a(""), Vec::<String>::new());
        assert_eq!(a("1, B"), vec!["1", "B"]);
        assert_eq!(a("(1,2), 'a,b'"), vec!["(1,2)", "'a,b'"]);
        assert_eq!(a("<1,2>, X"), vec!["1,2", "X"]);
        assert_eq!(a("X, <1,2>"), vec!["X", "1,2"]);
        assert_eq!(a("1,,3"), vec!["1", "", "3"]);
        assert_eq!(a("1,"), vec!["1", ""]);
        assert_eq!(a("A<B"), vec!["A<B"]);
        assert!(split_macro_args("<1,2", &loc).is_err());
    }

    #[test]
    fn head() {
        let is_op = |w: &str| is_known_op(w) || w.eq_ignore_ascii_case("MYM");
        let h = parse_head("MYM 1,2 ; c", &is_op);
        assert!(h.label.is_none());
        assert_eq!(h.op.unwrap().text, "MYM");
        assert_eq!((h.rest.as_str(), h.rest_col), ("1,2", 5));
        let h = parse_head("  NEW MACRO A,B", &is_op);
        assert_eq!(h.label.unwrap().text, "NEW");
        assert_eq!(h.rest, "A,B");
        let h = parse_head("L: IF X", &is_op);
        assert_eq!(h.label.as_ref().unwrap().text, "L");
        assert_eq!(h.op_upper(), "IF");
    }
}
