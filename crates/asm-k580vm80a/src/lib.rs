//! Ассемблер КР580ВМ80А (Intel 8080) для компьютера ЮТ-88.
//!
//! Схема работы: [`source`] (строки исходного текста; сюда встраивается препроцессор) →
//! [`lexer`] (метка, мнемоника, операнды) → [`expr`] (выражения со значениями с признаком
//! перемещаемости) → [`assembler`] (два прохода, команды из [`opcodes`], директивы) →
//! [`ir`] (модуль: сегменты + перемещения + символы) → [`output`] (bin, дамп журнала,
//! Intel HEX).
//!
//! ```
//! use asm_k580vm80a::{assemble, Options, ir::Placement};
//! let a = assemble("t.asm", "  ORG 0C000H\nS: JMP S\n", &Options::default()).unwrap();
//! let img = a.to_image(&Placement::default()).unwrap();
//! assert_eq!(img.to_dump(), "C000 C3 00 C0\n");
//! ```

pub mod assembler;
pub mod charset;
pub mod cli;
pub mod error;
pub mod expr;
pub mod ir;
pub mod lexer;
pub mod opcodes;
pub mod output;
pub mod source;

use std::path::{Path, PathBuf};

pub use assembler::{Assembly, LineRecord, assemble_with};
pub use charset::Charset;
pub use error::{AsmError, Loc};
pub use output::{Format, Image};

/// Настройки сборки.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Кодировка строк (`--charset`).
    pub charset: Charset,
    /// Символы из командной строки (`-D ИМЯ[=ЗНАЧЕНИЕ]`).
    pub defines: Vec<(String, i64)>,
    /// Папки поиска файлов INCLUDE/INCBIN (`-I`), после папки исходного файла.
    pub include_dirs: Vec<PathBuf>,
}

/// Собрать текст `text` (имя файла `file` — для сообщений и поиска INCBIN).
pub fn assemble(file: &str, text: &str, opts: &Options) -> Result<Assembly, Vec<AsmError>> {
    assemble_with(&source::TextProvider::new(file, text), opts)
}

/// Собрать файл.
pub fn assemble_file(path: &Path, opts: &Options) -> Result<Assembly, Vec<AsmError>> {
    let provider = source::TextProvider::from_file(path).map_err(|e| vec![e])?;
    assemble_with(&provider, opts)
}
