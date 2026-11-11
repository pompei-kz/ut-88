//! Список программ из журналов (`programs/`): файлы `.bin` и адреса из `README.md`.

use std::path::{Path, PathBuf};

/// Программа.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// Имя файла без расширения.
    pub name: String,
    /// Название (из README).
    pub title: String,
    /// Конфигурация (из README).
    pub config: String,
    /// Путь к `.bin`.
    pub path: PathBuf,
    /// Адрес загрузки.
    pub load: Option<u16>,
    /// Адрес пуска.
    pub start: Option<u16>,
}

fn parse_hex(s: &str) -> Option<u16> {
    let s = s.trim().trim_end_matches(['H', 'h']);
    u16::from_str_radix(s, 16).ok()
}

/// Строка сводной таблицы: имя, название, конфигурация, загрузка, пуск.
pub type Row = (String, String, String, Option<u16>, Option<u16>);

/// Строки сводной таблицы README.
pub fn parse_readme(text: &str) -> Vec<Row> {
    text.lines()
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('|').map(str::trim).collect();
            // | `имя` | Название | Выпуск | Конфигурация | Загрузка | Конец | Пуск | ...
            if cols.len() < 9 {
                return None;
            }
            let name = cols[1].strip_prefix('`')?.strip_suffix('`')?;
            Some((
                name.to_string(),
                cols[2].to_string(),
                cols[4].to_string(),
                parse_hex(cols[5]),
                parse_hex(cols[7]),
            ))
        })
        .collect()
}

/// Найти папку программ: заданная, `./programs`, рядом с исполняемым файлом и выше,
/// папка исходников.
pub fn find_dir(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    let mut cands = vec![PathBuf::from("programs")];
    if let Ok(exe) = std::env::current_exe() {
        for a in exe.ancestors().skip(1) {
            cands.push(a.join("programs"));
        }
    }
    cands.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../programs"
    )));
    cands.into_iter().find(|p| p.is_dir())
}

/// Программы папки `dir`, по имени.
pub fn list(dir: &Path) -> Vec<Program> {
    let table = std::fs::read_to_string(dir.join("README.md"))
        .map(|t| parse_readme(&t))
        .unwrap_or_default();
    let mut out: Vec<Program> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .map(|path| {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let row = table.iter().find(|r| r.0 == name);
            Program {
                title: row.map(|r| r.1.clone()).unwrap_or_default(),
                config: row.map(|r| r.2.clone()).unwrap_or_default(),
                load: row.and_then(|r| r.3),
                start: row.and_then(|r| r.4),
                name,
                path,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_table() {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs"));
        let ps = list(dir);
        let t = ps.iter().find(|p| p.name == "tetris").expect("tetris");
        assert_eq!(t.load, Some(0x3000));
        assert_eq!(t.start, Some(0x3000));
        let g = ps.iter().find(|p| p.name == "gamma").expect("gamma");
        assert_eq!((g.load, g.start), (Some(0xC000), Some(0xC03E)));
        let f = ps.iter().find(|p| p.name == "labyrinth-field").unwrap();
        assert_eq!((f.load, f.start), (Some(0xC200), None));
        assert!(ps.iter().all(|p| p.load.is_some()), "{ps:?}");
    }
}
