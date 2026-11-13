//! Источник строк исходного текста — точка расширения для препроцессора.
//!
//! Ассемблер не читает файлы сам: на каждом проходе он просит у [`SourceProvider`] новый
//! [`LineSource`] и берёт из него строки по одной. Простейший источник — [`TextSource`]
//! (строки одного текста). Препроцессор (INCLUDE, MACRO/REPT, IF/ELSE/ENDIF) реализуется
//! как ещё один [`LineSource`], который оборачивает чтение файлов и отдаёт ассемблеру уже
//! раскрытые строки, каждую — со своей позицией (файл:строка) и, при раскрытии макроса,
//! с примечанием о месте вызова. Для условной сборки ему передаётся [`PassContext`]: через
//! него можно вычислить выражение по текущей таблице символов и узнать, определён ли символ.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AsmError, AsmResult, Loc};
use crate::expr::Value;
use crate::lexer::Word;

/// Одна строка исходного текста с её происхождением.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLine {
    /// Имя файла (для сообщений).
    pub file: Arc<str>,
    /// Номер строки в файле, с 1.
    pub line: usize,
    /// Текст строки без перевода строки.
    pub text: String,
    /// Примечания к ошибкам в этой строке (например, «из макроса M, вызванного в a.asm:12»).
    pub notes: Vec<String>,
    /// Строка получена раскрытием макроса или REPT (для листинга).
    pub expanded: bool,
    /// Глубина включения INCLUDE: 0 — основной файл (для листинга).
    pub include_depth: usize,
    /// Что ассемблеру делать со строкой.
    pub kind: LineKind,
}

/// Что ассемблеру делать со строкой, полученной от источника.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LineKind {
    /// Обычная строка: ассемблер её собирает.
    #[default]
    Code,
    /// Строку уже выполнил препроцессор (директива INCLUDE/IF/MACRO/REPT…, строка тела
    /// определяемого макроса, вызов макроса). Ассемблер только определяет метку `label`
    /// (если есть) и показывает строку в листинге без кодов.
    Preprocessed {
        /// Метка в начале строки (например, `L: REPT 3` или `L: MYMACRO 1`).
        label: Option<Word>,
    },
    /// Строка ложной ветки IF: не собирается, в листинге — без кодов, с пометкой.
    Skipped,
    /// Ошибка препроцессора в этой строке: ассемблер добавляет её к ошибкам прохода
    /// (вместе с примечаниями строки).
    Error(Box<AsmError>),
}

impl SourceLine {
    /// Строка `text` из файла `file` под номером `line`.
    pub fn new(file: Arc<str>, line: usize, text: impl Into<String>) -> Self {
        Self {
            file,
            line,
            text: text.into(),
            notes: Vec::new(),
            expanded: false,
            include_depth: 0,
            kind: LineKind::Code,
        }
    }

    /// Позиция столбца `col` этой строки.
    pub fn loc(&self, col: usize) -> Loc {
        Loc::new(self.file.clone(), self.line, col)
    }

    /// Папка файла этой строки (для INCBIN/INCLUDE по относительному пути).
    pub fn dir(&self) -> Option<PathBuf> {
        Path::new(&*self.file).parent().map(Path::to_path_buf)
    }
}

/// Номер прохода сборки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// Первый проход: собираются метки, ссылки вперёд ещё неизвестны.
    First,
    /// Второй проход: генерируется код, все символы известны.
    Second,
}

/// То, что ассемблер даёт источнику строк во время прохода.
pub trait PassContext {
    /// Текущий проход.
    fn pass(&self) -> Pass;
    /// Вычислить выражение `text` (например, условие IF). `loc` — позиция начала выражения
    /// (для сообщений об ошибках). `Ok(None)` — на первом проходе выражение ссылается на ещё
    /// не определённый символ.
    fn eval(&self, text: &str, loc: &Loc) -> AsmResult<Option<Value>>;
    /// Определён ли символ (метка, EQU, SET, EXTRN или `-D`) к этому моменту прохода.
    fn is_defined(&self, name: &str) -> bool;
    /// Настройки сборки (папки `-I` и т. п.).
    fn options(&self) -> &crate::Options;
}

/// Поток строк одного прохода.
pub trait LineSource {
    /// Следующая строка; `Ok(None)` — текст закончился.
    fn next_line(&mut self, ctx: &dyn PassContext) -> AsmResult<Option<SourceLine>>;

    /// Вызывается после директивы END (или конца текста): здесь препроцессор может
    /// проверить незакрытые IF/MACRO/REPT и вернуть ошибки.
    fn finish(&mut self, _ctx: &dyn PassContext) -> Vec<AsmError> {
        Vec::new()
    }
}

/// Создаёт свежий [`LineSource`] на каждый проход.
pub trait SourceProvider {
    /// Начать проход: вернуть источник строк с первой строки.
    fn open(&self, opts: &crate::Options) -> AsmResult<Box<dyn LineSource + '_>>;
}

/// Строки одного текста (без препроцессора).
#[derive(Clone, Debug)]
pub struct TextSource {
    file: Arc<str>,
    lines: Vec<String>,
    next: usize,
}

impl TextSource {
    /// Источник из текста `text` с именем файла `file`.
    pub fn new(file: impl Into<Arc<str>>, text: &str) -> Self {
        Self {
            file: file.into(),
            lines: text.lines().map(str::to_string).collect(),
            next: 0,
        }
    }
}

impl LineSource for TextSource {
    fn next_line(&mut self, _ctx: &dyn PassContext) -> AsmResult<Option<SourceLine>> {
        let Some(text) = self.lines.get(self.next) else {
            return Ok(None);
        };
        self.next += 1;
        Ok(Some(SourceLine::new(self.file.clone(), self.next, text)))
    }
}

/// Поставщик строк одного текста в памяти.
#[derive(Clone, Debug)]
pub struct TextProvider {
    /// Имя файла для сообщений.
    pub file: Arc<str>,
    /// Текст.
    pub text: String,
}

impl TextProvider {
    /// Поставщик для текста `text` с именем `file`.
    pub fn new(file: impl Into<Arc<str>>, text: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            text: text.into(),
        }
    }

    /// Прочитать файл (UTF-8; первая метка BOM отбрасывается).
    pub fn from_file(path: &Path) -> AsmResult<Self> {
        let bytes = std::fs::read(path)
            .map_err(|e| AsmError::plain(format!("не читается файл {}: {e}", path.display())))?;
        let text = String::from_utf8(bytes).map_err(|_| {
            AsmError::plain(format!("файл {} не в кодировке UTF-8", path.display()))
        })?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();
        Ok(Self::new(path.display().to_string(), text))
    }
}

impl SourceProvider for TextProvider {
    fn open(&self, _opts: &crate::Options) -> AsmResult<Box<dyn LineSource + '_>> {
        Ok(Box::new(TextSource::new(self.file.clone(), &self.text)))
    }
}
