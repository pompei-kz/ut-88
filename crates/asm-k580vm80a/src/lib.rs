//! Ассемблер КР580ВМ80А (Intel 8080) для компьютера ЮТ-88.
//!
//! Схема работы: [`source`] (строки исходного текста) → [`preprocess`] (INCLUDE, IF,
//! MACRO, REPT) →
//! [`lexer`] (метка, мнемоника, операнды) → [`expr`] (выражения со значениями с признаком
//! перемещаемости) → [`assembler`] (два прохода, команды из [`opcodes`], директивы) →
//! [`ir`] (модуль: сегменты + перемещения + символы) → [`output`] (bin, дамп журнала,
//! Intel HEX), [`object`] (объектный файл `.link`) и [`listing`] (листинг).
//!
//! Компоновщик [`linker`] (CLI — [`link_cli`], бинарник `link-k580vm80a`) читает
//! объектные файлы, раскладывает сегменты модулей, сводит PUBLIC/EXTRN и применяет
//! перемещения тем же [`ir::Module::place`], что и ассемблер.
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
pub mod link_cli;
pub mod linker;
pub mod listing;
pub mod object;
pub mod opcodes;
pub mod output;
pub mod preprocess;
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

/// Собрать текст `text` с препроцессором (имя файла `file` — для сообщений и поиска
/// INCLUDE/INCBIN).
pub fn assemble(file: &str, text: &str, opts: &Options) -> Result<Assembly, Vec<AsmError>> {
    assemble_with(&preprocess::Preprocessor::text(file, text), opts)
}

/// Собрать файл с препроцессором.
pub fn assemble_file(path: &Path, opts: &Options) -> Result<Assembly, Vec<AsmError>> {
    let provider = preprocess::Preprocessor::file(path).map_err(|e| vec![e])?;
    assemble_with(&provider, opts)
}
