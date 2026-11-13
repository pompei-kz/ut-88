//! Настройки окна, запоминаемые между запусками: громкость и «звук вкл/выкл».
//!
//! Каждый параметр — отдельный текстовый файл в папке настроек (см. [`config_dir`]),
//! содержимое — одна строка:
//!
//! | Файл         | Содержимое                     | По умолчанию |
//! |--------------|--------------------------------|--------------|
//! | `volume.txt` | громкость, число 0.0–1.0       | `0.70`       |
//! | `sound.txt`  | звук: `on` или `off`           | `on`         |
//!
//! Файл пишется сразу при изменении параметра; папка создаётся при записи. Отсутствующий
//! файл — значение по умолчанию; битое содержимое — значение по умолчанию и
//! предупреждение. Пробелы и переводы строк вокруг значения не важны; в `sound.txt` при
//! чтении принимаются и `true`/`false`.

use std::path::{Path, PathBuf};

/// Файл громкости в папке настроек.
pub const VOLUME_FILE: &str = "volume.txt";

/// Файл «звук вкл/выкл» в папке настроек.
pub const SOUND_FILE: &str = "sound.txt";

/// Переменная окружения, задающая папку настроек вместо `~/.config/ut-88`.
pub const DIR_ENV: &str = "UT88_CONFIG_DIR";

/// Запоминаемые настройки.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Громкость 0,0–1,0.
    pub volume: f32,
    /// Звук включён.
    pub sound: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.7,
            sound: true,
        }
    }
}

/// Папка настроек: `$UT88_CONFIG_DIR`, иначе `$XDG_CONFIG_HOME/ut-88`, иначе
/// `~/.config/ut-88`; `None`, если ни одна переменная не задана.
pub fn config_dir() -> Option<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(d) = var(DIR_ENV) {
        return Some(d);
    }
    if let Some(x) = var("XDG_CONFIG_HOME") {
        return Some(x.join("ut-88"));
    }
    var("HOME").map(|h| h.join(".config").join("ut-88"))
}

/// Разобрать громкость.
pub fn parse_volume(s: &str) -> Option<f32> {
    s.trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0))
}

/// Разобрать «звук вкл/выкл».
pub fn parse_sound(s: &str) -> Option<bool> {
    match s.trim() {
        "on" | "true" => Some(true),
        "off" | "false" => Some(false),
        _ => None,
    }
}

/// Текст файла громкости.
pub fn volume_text(v: f32) -> String {
    format!("{:.2}\n", v.clamp(0.0, 1.0))
}

/// Текст файла «звук вкл/выкл».
pub fn sound_text(on: bool) -> String {
    format!("{}\n", if on { "on" } else { "off" })
}

/// Прочитать один параметр: `Ok(None)` — файла нет, `Err` — предупреждение.
fn read_param<T>(
    path: &Path,
    parse: fn(&str) -> Option<T>,
    what: &str,
) -> Result<Option<T>, String> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes)
            .ok()
            .and_then(|t| parse(&t))
            .map(Some)
            .ok_or_else(|| format!("{}: не {what}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: не прочитан: {e}", path.display())),
    }
}

impl Settings {
    /// Прочитать настройки из папки `dir`. Возвращает настройки и предупреждения (по
    /// одному на каждый битый или нечитаемый файл).
    pub fn load(dir: &Path) -> (Settings, Vec<String>) {
        let mut s = Settings::default();
        let mut warn = Vec::new();
        match read_param(&dir.join(VOLUME_FILE), parse_volume, "число 0.0–1.0") {
            Ok(Some(v)) => s.volume = v,
            Ok(None) => {}
            Err(w) => warn.push(w),
        }
        match read_param(&dir.join(SOUND_FILE), parse_sound, "on или off") {
            Ok(Some(v)) => s.sound = v,
            Ok(None) => {}
            Err(w) => warn.push(w),
        }
        (s, warn)
    }
}

/// Записать файл параметра `name` в папку `dir` (папка создаётся; запись через
/// временный файл и переименование, чтобы не оставить файл недописанным).
pub fn write_param(dir: &Path, name: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(name);
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Записать громкость.
pub fn save_volume(dir: &Path, v: f32) -> std::io::Result<()> {
    write_param(dir, VOLUME_FILE, &volume_text(v))
}

/// Записать «звук вкл/выкл».
pub fn save_sound(dir: &Path, on: bool) -> std::io::Result<()> {
    write_param(dir, SOUND_FILE, &sound_text(on))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("ut88-settings-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn save_and_load() {
        let d = tmp("save").join("sub");
        save_volume(&d, 0.25).unwrap();
        save_sound(&d, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.join(VOLUME_FILE)).unwrap(),
            "0.25\n"
        );
        assert_eq!(
            std::fs::read_to_string(d.join(SOUND_FILE)).unwrap(),
            "off\n"
        );
        let (s, w) = Settings::load(&d);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            s,
            Settings {
                volume: 0.25,
                sound: false
            }
        );
        save_sound(&d, true).unwrap();
        assert!(Settings::load(&d).0.sound);
        let _ = std::fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn missing_files_defaults() {
        let d = tmp("missing");
        assert_eq!(Settings::load(&d), (Settings::default(), vec![]));
        // Один файл есть, другого нет.
        save_volume(&d, 0.5).unwrap();
        let (s, w) = Settings::load(&d);
        assert!(w.is_empty());
        assert_eq!(s.volume, 0.5);
        assert!(s.sound);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn broken_files_defaults_with_warning() {
        let d = tmp("broken");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(VOLUME_FILE), "громко\n").unwrap();
        std::fs::write(d.join(SOUND_FILE), [0xFF, 0xFE]).unwrap();
        let (s, w) = Settings::load(&d);
        assert_eq!(s, Settings::default());
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(
            w[0].contains(VOLUME_FILE) && w[1].contains(SOUND_FILE),
            "{w:?}"
        );
        // Папка вместо файла — тоже не падаем.
        std::fs::remove_file(d.join(SOUND_FILE)).unwrap();
        std::fs::create_dir(d.join(SOUND_FILE)).unwrap();
        let (s, w) = Settings::load(&d);
        assert!(s.sound);
        assert_eq!(w.len(), 2, "{w:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn parse_values() {
        assert_eq!(parse_volume(" 0.3 \n"), Some(0.3));
        assert_eq!(parse_volume("1.5"), Some(1.0));
        assert_eq!(parse_volume("-2"), Some(0.0));
        assert_eq!(parse_volume("NaN"), None);
        assert_eq!(parse_volume(""), None);
        assert_eq!(parse_sound("on\n"), Some(true));
        assert_eq!(parse_sound("false"), Some(false));
        assert_eq!(parse_sound("1"), None);
        assert_eq!(volume_text(0.7), "0.70\n");
        assert_eq!(sound_text(false), "off\n");
    }
}
