//! Ошибки ассемблера с позицией в исходном тексте.

use std::fmt;
use std::sync::Arc;

/// Позиция в исходном тексте: файл, строка и столбец (с единицы, в символах).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Loc {
    /// Имя файла, как его увидит пользователь (путь из командной строки, из INCLUDE и т. п.).
    pub file: Arc<str>,
    /// Номер строки, с 1.
    pub line: usize,
    /// Номер столбца, с 1 (в символах; табуляция — один символ).
    pub col: usize,
}

impl Loc {
    /// Позиция `файл:строка:столбец`.
    pub fn new(file: Arc<str>, line: usize, col: usize) -> Self {
        Self { file, line, col }
    }

    /// Та же строка, другой столбец.
    pub fn with_col(&self, col: usize) -> Self {
        Self {
            file: self.file.clone(),
            line: self.line,
            col,
        }
    }
}

impl fmt::Display for Loc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

/// Ошибка сборки. Выводится как `файл:строка:столбец: ошибка: текст`, затем, если есть,
/// примечание с новой строки (например, «в макросе X, вызванном в a.asm:10» — его заполняет
/// препроцессор).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsmError {
    /// Где ошибка; `None` — ошибка не привязана к строке (например, файл не читается).
    pub loc: Option<Loc>,
    /// Текст ошибки (по-русски, со строчной буквы, без точки в конце).
    pub msg: String,
    /// Дополнительные пояснения, каждое выводится отдельной строкой.
    pub notes: Vec<String>,
}

impl AsmError {
    /// Ошибка в позиции `loc`.
    pub fn at(loc: &Loc, msg: impl Into<String>) -> Self {
        Self {
            loc: Some(loc.clone()),
            msg: msg.into(),
            notes: Vec::new(),
        }
    }

    /// Ошибка без позиции.
    pub fn plain(msg: impl Into<String>) -> Self {
        Self {
            loc: None,
            msg: msg.into(),
            notes: Vec::new(),
        }
    }

    /// Добавить примечание.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.loc {
            Some(loc) => write!(f, "{loc}: ошибка: {}", self.msg)?,
            None => write!(f, "ошибка: {}", self.msg)?,
        }
        for n in &self.notes {
            write!(f, "\n    примечание: {n}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AsmError {}

/// Результат с одной ошибкой.
pub type AsmResult<T> = Result<T, AsmError>;
