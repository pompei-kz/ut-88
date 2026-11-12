//! Машина ЮТ-88: шина, карта памяти, устройства (модуль 1, дисплейный модуль, модуль
//! ОЗУ 64 Кбайт), часы, магнитофон, снимки состояния.
//!
//! Ядро не знает об интерфейсе: GUI/TUI создают [`Machine`] по [`Config`], крутят
//! [`Machine::run_frame`] / [`Machine::run_cycles`], нажимают клавиши и читают индикаторы
//! ([`Machine::indicator_segments`]) и экран ([`Machine::screen`],
//! [`Machine::render_screen`]). Время — только в тактах (16 МГц / 9 на такт процессора).
//!
//! Квазидиск (doc/32) — часть шины ([`Config::kvazidisk`]), загрузка CP/M и ОС ЮТ-88 и
//! работа с файлами диска CP/M — модуль [`cpm`]. Другие расширения подключаются через
//! трейт [`Expansion`].

pub mod bus;
pub mod config;
pub mod cpm;
pub mod devices;
pub mod keys;
pub mod machine;
pub mod roms;
pub mod screen;
pub mod speed;
pub mod tape;

pub use bus::{Expansion, MemAccess};
pub use config::{Config, DisplayRoms};
pub use cpm::{CpmDisk, CpmFile, System};
pub use devices::kvazidisk::Kvazidisk;
pub use keys::{KbKey, PadKey};
pub use machine::{CPU_HZ, CpuState, FRAME_CYCLES, Machine, Snapshot, SnapshotError};
pub use screen::Screen;
pub use tape::{TapeFormat, TapeSignal};
