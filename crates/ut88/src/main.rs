//! Исполняемый файл эмулятора ЮТ-88: разбор ключей командной строки и выбор интерфейса.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use ut88_core::{Config, Machine};
use ut88_gui::loader::{self, Image, ProgramInfo, StartSource, parse_hex_u16, parse_load_arg};
use ut88_gui::{GuiOptions, Speed};

/// Конфигурация машины.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ConfigKind {
    /// Только процессорный модуль (модуль 1): индикаторы и 16 кнопок.
    Minimal,
    /// Модуль 1 + дисплейный модуль + ОЗУ 64 Кбайт; МОНИТОР-F, часы в нём.
    Full,
    /// Полная, но с включённым ПЗУ МОНИТОРА модуля 1 (МОНИТОР-F — директивой 7 F800).
    #[value(name = "full-monitor0")]
    FullMonitor0,
}

/// Эмулятор компьютера ЮТ-88 (журнал «ЮТ для умелых рук», 1989–1990).
#[derive(Parser, Debug)]
#[command(name = "ut88", version, about)]
struct Args {
    /// Конфигурация машины.
    #[arg(long, value_enum, default_value = "full")]
    config: ConfigKind,

    /// Образ ПЗУ МОНИТОРА модуля 1 (до 1 Кбайта) вместо встроенного monitor-fixed.bin.
    #[arg(long, value_name = "ФАЙЛ")]
    rom: Option<PathBuf>,

    /// Подключить ПЗУ библиотеки калькулятора (0800H–0FFFH).
    #[arg(long)]
    calculator: bool,

    /// Терминальный интерфейс вместо окна.
    #[arg(long)]
    tui: bool,

    /// Загрузить программу: .bin (адрес обязателен или берётся из programs/README.md по
    /// имени), Intel HEX (.ihx/.hex) или текстовый дамп «AAAA XX XX …» (.hex). Можно
    /// повторять.
    #[arg(long, value_name = "ФАЙЛ[@АДРЕС]")]
    load: Vec<String>,

    /// Запустить программу с адреса (после загрузки).
    #[arg(long, value_name = "АДРЕС", value_parser = parse_addr)]
    run: Option<u16>,

    /// Вставить «кассету» — WAV-файл для чтения директивой A / I.
    #[arg(long, value_name = "ФАЙЛ.wav")]
    tape: Option<PathBuf>,

    /// Восстановить состояние из снимка (конфигурация берётся из снимка).
    #[arg(long, value_name = "ФАЙЛ")]
    snapshot: Option<PathBuf>,

    /// Скорость (ряд общий для окна и терминала): 0.25, 0.5, 1, 2, 4, 8, 16 или max
    /// (0 — тоже максимум).
    #[arg(long, value_name = "N", default_value = "1", value_parser = parse_speed)]
    speed: f64,

    /// Без звука.
    #[arg(long)]
    no_sound: bool,

    /// Папка с программами (по умолчанию ищется programs/ рядом).
    #[arg(long, value_name = "ПАПКА")]
    programs: Option<PathBuf>,
}

fn parse_addr(s: &str) -> Result<u16, String> {
    parse_hex_u16(s).map_err(|e| e.to_string())
}

fn parse_speed(s: &str) -> Result<f64, String> {
    ut88_core::speed::parse_speed(s)
}

/// Конфигурация по ключам.
fn make_config(args: &Args) -> Result<Config> {
    let mut c = match args.config {
        ConfigKind::Minimal => Config::minimal(),
        ConfigKind::Full => Config::full(),
        ConfigKind::FullMonitor0 => Config::full_with_monitor0(),
    };
    if let Some(p) = &args.rom {
        let rom = std::fs::read(p).with_context(|| format!("не читается ПЗУ {}", p.display()))?;
        if rom.len() > 1024 {
            bail!("ПЗУ {} больше 1 Кбайта ({} байт)", p.display(), rom.len());
        }
        c = c.with_monitor_rom(&rom);
    }
    if args.calculator {
        c = c.with_calculator();
    }
    Ok(c)
}

/// Эмулированное время, за которое МОНИТОР успевает проинициализироваться перед загрузкой
/// программ с ключом `--load` / `--run`.
const BOOT_SECONDS: f64 = 1.0;

/// Собрать машину по ключам: конфигурация или снимок, загрузка программ, пуск, лента.
fn build_machine(args: &Args, table: &[ProgramInfo]) -> Result<Machine> {
    let mut m = match &args.snapshot {
        Some(p) => {
            let data = std::fs::read(p).with_context(|| format!("не читается {}", p.display()))?;
            let s = Machine::parse_snapshot(&data)?;
            Machine::from_snapshot(&s)?
        }
        None => Machine::new(make_config(args)?),
    };
    if args.snapshot.is_none() && (!args.load.is_empty() || args.run.is_some()) {
        // Дать МОНИТОРУ установить стек и рабочие ячейки, иначе он затрёт программу или
        // программа уйдёт в незаданный стек.
        m.run_seconds(BOOT_SECONDS);
    }
    for l in &args.load {
        let (path, addr) = parse_load_arg(l)?;
        let img = loader::load_file(&path, addr, table)?;
        img.load_into(&mut m);
        eprintln!("ut88: {}", load_message(&path, &img));
    }
    if let Some(a) = args.run {
        m.jump(a);
        eprintln!("ut88: пуск с {a:04X} (--run)");
    } else if !args.load.is_empty() {
        eprintln!(
            "ut88: программа загружена, но не запущена (нет --run): \
             запустите директивой МОНИТОРА или ключом --run АДРЕС"
        );
    }
    if let Some(t) = &args.tape {
        m.tape_insert_wav(t)
            .with_context(|| format!("не читается WAV {}", t.display()))?;
        eprintln!(
            "ut88: кассета {} вставлена, лента стоит — тронется, когда программа начнёт \
             читать магнитофон (директива A / I), или кнопкой «Воспроизведение» (F8)",
            t.display()
        );
    }
    Ok(m)
}

/// Сообщение о загруженном образе: сколько байт, куда и что известно об адресе пуска.
fn load_message(path: &std::path::Path, img: &Image) -> String {
    let mut s = format!(
        "{} — {} байт с {:04X}",
        path.display(),
        img.len(),
        img.first_addr().unwrap_or(0)
    );
    match (img.start, img.start_source) {
        (Some(a), Some(StartSource::Table)) => {
            s += &format!(", адрес пуска по programs/README.md {a:04X}");
        }
        (Some(a), _) => s += &format!(", адрес пуска из файла {a:04X}"),
        (None, _) => {}
    }
    if let Some(from) = img.relocated_from {
        s += &format!(
            " (сдвинуто с {from:04X}: адрес пуска неизвестен, программа может не работать \
             на новом месте)"
        );
    }
    s
}

fn main() -> Result<()> {
    let args = Args::parse();
    let programs_dir = args.programs.clone().or_else(loader::find_programs_dir);
    let table = programs_dir
        .as_deref()
        .map(loader::load_programs_table)
        .unwrap_or_default();
    let machine = build_machine(&args, &table)?;
    if args.tui {
        let opts = ut88_tui::Options {
            speed: args.speed,
            sound: !args.no_sound,
            programs_dir: programs_dir.clone(),
            ..Default::default()
        };
        ut88_tui::run(machine, opts)
    } else {
        ut88_gui::run(GuiOptions {
            machine,
            speed: Speed::from_factor(args.speed),
            sound: !args.no_sound,
            programs_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(p: &str) -> String {
        format!("{}/../../{p}", env!("CARGO_MANIFEST_DIR"))
    }

    fn table() -> Vec<ProgramInfo> {
        loader::load_programs_table(std::path::Path::new(&repo("programs")))
    }

    #[test]
    fn cli_parses() {
        let a = Args::try_parse_from([
            "ut88",
            "--config",
            "full-monitor0",
            "--load",
            "a.bin@C000",
            "--load",
            "b.hex",
            "--run",
            "C000",
            "--speed",
            "max",
            "--no-sound",
            "--tui",
        ])
        .unwrap();
        assert_eq!(a.config, ConfigKind::FullMonitor0);
        assert_eq!(a.load.len(), 2);
        assert_eq!(a.run, Some(0xC000));
        assert_eq!(a.speed, 0.0);
        assert!(a.no_sound && a.tui);
        let a = Args::try_parse_from(["ut88"]).unwrap();
        assert_eq!(a.config, ConfigKind::Full);
        assert_eq!(a.speed, 1.0);
        assert!(Args::try_parse_from(["ut88", "--run", "XYZ"]).is_err());
        assert!(Args::try_parse_from(["ut88", "--speed", "fast"]).is_err());
        assert!(Args::try_parse_from(["ut88", "--speed", "3"]).is_err());
        let a = Args::try_parse_from(["ut88", "--speed", "16"]).unwrap();
        assert_eq!(Speed::from_factor(a.speed).factor(), Some(16.0));
        let a = Args::try_parse_from(["ut88", "--speed", "0.25"]).unwrap();
        assert_eq!(a.speed, 0.25);
        assert!(Args::try_parse_from(["ut88", "--config", "huge"]).is_err());
    }

    #[test]
    fn load_and_run_minimal() {
        let gamma = repo("programs/gamma.bin");
        let a = Args::try_parse_from([
            "ut88", "--config", "minimal", "--load", &gamma, "--run", "C03E",
        ])
        .unwrap();
        let m = build_machine(&a, &table()).unwrap();
        assert_eq!(m.cpu.pc, 0xC03E);
        assert_eq!(m.save_bin(0xC000, 0xC082), std::fs::read(&gamma).unwrap());
    }

    #[test]
    fn bin_without_address_needs_table() {
        let a = Args::try_parse_from([
            "ut88",
            "--config",
            "minimal",
            "--load",
            "/tmp/nonexistent-x.bin",
        ])
        .unwrap();
        assert!(build_machine(&a, &table()).is_err());
    }

    #[test]
    fn full_config_tetris_from_hex_dump() {
        let a = Args::try_parse_from([
            "ut88",
            "--load",
            &repo("programs/tetris.hex"),
            "--load",
            &format!("{}@C000", repo("programs/fill-const.bin")),
        ])
        .unwrap();
        let m = build_machine(&a, &table()).unwrap();
        assert!(m.config().display.is_some());
        assert_eq!(m.peek(0x3000), 0x3E);
        assert_eq!(
            m.peek(0xC000),
            std::fs::read(repo("programs/fill-const.bin")).unwrap()[0]
        );
    }

    #[test]
    fn load_messages() {
        let t = table();
        let gamma = std::path::PathBuf::from(repo("programs/gamma.bin"));
        let img = loader::load_file(&gamma, None, &t).unwrap();
        let msg = load_message(&gamma, &img);
        assert!(
            msg.contains("адрес пуска по programs/README.md C03E"),
            "{msg}"
        );
        let img = loader::load_file(&gamma, Some(0xC100), &t).unwrap();
        let msg = load_message(&gamma, &img);
        assert!(msg.contains("с C100"), "{msg}");
        assert!(!msg.contains("C03E"), "{msg}");
        assert!(msg.contains("сдвинуто"), "{msg}");
    }

    /// Окно и терминал — одна схема горячих клавиш.
    #[test]
    fn hotkeys_same_in_gui_and_tui() {
        let gui: Vec<&str> = ut88_gui::keymap::HOTKEYS.iter().map(|h| h.0).collect();
        let tui: Vec<&str> = ut88_tui::keymap::HOTKEYS.iter().map(|h| h.0).collect();
        assert_eq!(gui, tui);
        assert!(!gui.contains(&"F1"), "F1 свободна");
        for (k, what) in [
            ("F2", "УСТ"),
            ("F12", "отладчик"),
            ("F8", "агнитофон"),
            ("F11", "правка"),
        ] {
            let g = ut88_gui::keymap::HOTKEYS.iter().find(|h| h.0 == k).unwrap();
            let t = ut88_tui::keymap::HOTKEYS.iter().find(|h| h.0 == k).unwrap();
            assert!(
                g.1.contains(what) && t.2.contains(what),
                "{k}: {} / {}",
                g.1,
                t.2
            );
        }
    }

    #[test]
    fn rom_too_big_rejected() {
        let a = Args::try_parse_from(["ut88", "--rom", &repo("roms/monitor-f.bin")]).unwrap();
        assert!(make_config(&a).is_err());
    }
}
