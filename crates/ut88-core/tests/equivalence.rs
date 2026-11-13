//! Эквивалентность способов прогона: одна большая порция `run_cycles`, порции по кадру
//! (`FRAME_CYCLES`) и пошаговый `step` дают одно и то же состояние машины (снимок —
//! процессор, такты, память, экран, устройства, часы, магнитофон, квазидиск), одинаковые
//! записи магнитофона, перепады звука и признаки «экран/индикаторы изменились».

mod common;

use common::*;
use ut88_core::{CPU_HZ, Config, FRAME_CYCLES, KbKey, Machine, PadKey, System, TapeSignal};

/// Способ прогона.
#[derive(Debug, Clone, Copy)]
enum Mode {
    /// Одна порция `run_cycles(n)`.
    Big,
    /// Порции по кадру.
    Frames,
    /// По одной команде.
    Steps,
}

/// Действие сценария.
#[derive(Clone)]
enum Act {
    /// Выполнить столько-то секунд эмулированного времени выбранным способом.
    Run(f64),
    /// Набрать текст на полной клавиатуре (клавиша 0,06 с, пауза 0,3 с).
    Type(&'static str),
    /// Набрать шестнадцатеричные цифры на клавиатуре модуля 1 (кнопка 0,1 с, пауза 0,3 с).
    Pad(&'static str),
    /// Положить байты в память.
    Load(u16, Vec<u8>),
    /// Передать управление.
    Jump(u16),
    /// Начать запись магнитофона.
    RecStart,
    /// Остановить запись (сигнал попадает в результат).
    RecStop,
    /// Вставить последний записанный сигнал и пустить ленту.
    PlayLast,
    /// Собирать перепады звука.
    Sound(bool),
    /// Выключить запросы прерывания от часов.
    ClockOff,
    /// Загрузить систему (как пункт меню).
    Boot(System),
}

/// Что сравнивается между способами прогона.
#[derive(Default, PartialEq)]
struct Trace {
    /// Снимки после каждого прогона.
    snapshots: Vec<Vec<u8>>,
    /// Признаки «экран / индикаторы изменились» после каждого прогона.
    changed: Vec<(bool, bool)>,
    /// Записи магнитофона.
    tapes: Vec<TapeSignal>,
    /// Перепады звука.
    sound: Vec<(u64, bool)>,
}

impl std::fmt::Debug for Trace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Trace {{ snapshots: {}, changed: {:?}, tapes: {}, sound: {} }}",
            self.snapshots.len(),
            self.changed,
            self.tapes.len(),
            self.sound.len()
        )
    }
}

fn run(m: &mut Machine, cycles: u64, mode: Mode) {
    let target = m.cycles() + cycles;
    match mode {
        Mode::Big => {
            m.run_cycles(cycles);
        }
        Mode::Frames => {
            while m.cycles() < target {
                m.run_cycles((target - m.cycles()).min(FRAME_CYCLES));
            }
        }
        Mode::Steps => {
            while m.cycles() < target {
                m.step();
            }
        }
    }
}

fn secs(s: f64) -> u64 {
    (s * CPU_HZ) as u64
}

fn play(config: Config, script: &[Act], mode: Mode) -> Trace {
    let mut m = Machine::new(config);
    let mut tr = Trace::default();
    let run_and_note = |m: &mut Machine, tr: &mut Trace, s: f64| {
        run(m, secs(s), mode);
        tr.snapshots.push(m.snapshot_bytes());
        tr.changed
            .push((m.take_screen_changed(), m.take_indicators_changed()));
        tr.sound.extend(m.drain_sound_edges());
    };
    for act in script {
        match act {
            Act::Run(s) => run_and_note(&mut m, &mut tr, *s),
            Act::Type(text) => {
                for b in text.bytes() {
                    let (key, modifier) = KbKey::for_code(b).unwrap();
                    if let Some(md) = modifier {
                        m.kb_down(md);
                    }
                    m.kb_down(key);
                    run_and_note(&mut m, &mut tr, 0.06);
                    m.kb_up(key);
                    if let Some(md) = modifier {
                        m.kb_up(md);
                    }
                    run_and_note(&mut m, &mut tr, 0.3);
                }
            }
            Act::Pad(digits) => {
                for c in digits.chars() {
                    let key = PadKey::Hex(c.to_digit(16).unwrap() as u8);
                    m.pad_down(key);
                    run_and_note(&mut m, &mut tr, 0.1);
                    m.pad_up(key);
                    run_and_note(&mut m, &mut tr, 0.3);
                }
            }
            Act::Load(addr, data) => m.load_bin(*addr, data),
            Act::Jump(addr) => m.jump(*addr),
            Act::RecStart => m.tape_record_start(),
            Act::RecStop => tr.tapes.push(m.tape_record_stop().unwrap()),
            Act::PlayLast => m.tape_play(tr.tapes.last().unwrap().clone()),
            Act::Sound(on) => m.set_sound_capture(*on),
            Act::ClockOff => m.set_clock_interrupts(false),
            Act::Boot(sys) => {
                m.boot_system(*sys, false).unwrap();
            }
        }
    }
    tr
}

/// Прогнать сценарий тремя способами и сравнить.
fn check(config: Config, script: &[Act]) -> Trace {
    let big = play(config.clone(), script, Mode::Big);
    let frames = play(config.clone(), script, Mode::Frames);
    assert!(big == frames, "порция ≠ кадры: {big:?} / {frames:?}");
    let steps = play(config, script, Mode::Steps);
    assert!(big == steps, "порция ≠ шаги: {big:?} / {steps:?}");
    big
}

/// МОНИТОР модуля 1 с часами: прерывания каждую секунду, индикаторы.
#[test]
fn clock_interrupts_minimal() {
    let tr = check(Config::minimal(), &[Act::Run(3.4)]);
    assert!(tr.changed[0].1, "индикаторы изменились");
}

/// МОНИТОР-F с часами, программа в HLT с разрешёнными прерываниями (просыпается от
/// секундного импульса) и в HLT с запрещёнными (стоит до конца прогона).
#[test]
fn clock_interrupts_and_halt_full() {
    let tr = check(
        Config::full(),
        &[
            Act::Run(0.5),
            // 2000H: EI; HLT; INR B; JMP 2000H
            Act::Load(0x2000, vec![0xFB, 0x76, 0x04, 0xC3, 0x00, 0x20]),
            Act::Jump(0x2000),
            Act::Run(2.7),
            Act::Run(FRAME_CYCLES as f64 * 1.5 / CPU_HZ),
            // 2100H: DI; HLT
            Act::Load(0x2100, vec![0xF3, 0x76]),
            Act::Jump(0x2100),
            Act::Run(1.3),
        ],
    );
    assert!(tr.changed[0].0, "экран нарисован");
}

/// МОНИТОР-F: запись блока на магнитофон (директива «O») и чтение его обратно
/// (директива «I»), со сбором звука.
#[test]
fn tape_write_and_read_full() {
    let data: Vec<u8> = (0..32u32).map(|i| (i * 31 + 1) as u8).collect();
    let tr = check(
        Config::full(),
        &[
            Act::Run(0.5),
            Act::Load(0x5000, data.clone()),
            Act::Sound(true),
            Act::Type("O5000,501F"),
            Act::RecStart,
            Act::Type("\r"),
            Act::Run(2.0),
            Act::RecStop,
            Act::Sound(false),
            Act::Load(0x5000, vec![0; 32]),
            Act::Type("I"),
            Act::PlayLast,
            Act::Type("\r"),
            Act::Run(2.5),
        ],
    );
    assert!(!tr.sound.is_empty(), "звук записи собран");
    let last = tr.snapshots.last().unwrap();
    let m = Machine::from_snapshot(&Machine::parse_snapshot(last).unwrap()).unwrap();
    assert_eq!(m.save_bin(0x5000, 0x501F), data, "блок прочитан");
}

/// МОНИТОР модуля 1 без прерываний (doc/25): запись директивой «9» и чтение директивой
/// «A».
#[test]
fn tape_write_and_read_minimal() {
    let data: Vec<u8> = (0..16u32).map(|i| (i * 7 + 3) as u8).collect();
    let tr = check(
        Config::minimal(),
        &[
            Act::Run(0.3),
            Act::ClockOff,
            Act::Load(0xC000, data.clone()),
            Act::Pad("9C000C00"),
            Act::RecStart,
            Act::Pad("F"),
            Act::Run(1.5),
            Act::RecStop,
            Act::Load(0xC000, vec![0; 16]),
            Act::Pad("A000"),
            Act::PlayLast,
            Act::Pad("0"),
            Act::Run(2.0),
        ],
    );
    let last = tr.snapshots.last().unwrap();
    let m = Machine::from_snapshot(&Machine::parse_snapshot(last).unwrap()).unwrap();
    assert_eq!(m.save_bin(0xC000, 0xC00F), data, "блок прочитан");
}

/// CP/M-64 на квазидиске: загрузка, запись файла (SAVE) и каталог (DIR).
#[test]
fn cpm64_kvazidisk() {
    let tr = check(
        Config::cpm(),
        &[
            Act::Run(0.5),
            Act::Boot(System::Cpm64),
            Act::Run(1.0),
            Act::Type("SAVE 1 A.COM\r"),
            Act::Run(0.5),
            Act::Type("DIR\r"),
            Act::Run(0.5),
        ],
    );
    let last = tr.snapshots.last().unwrap();
    let m = Machine::from_snapshot(&Machine::parse_snapshot(last).unwrap()).unwrap();
    let text = screen_text(&m);
    assert!(text.contains("A: A        COM"), "{text}");
}
