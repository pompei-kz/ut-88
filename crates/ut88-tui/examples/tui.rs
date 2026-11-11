//! Запуск TUI без бинарника `ut88`: `cargo run -p ut88-tui --example tui [-- full|full0]`.

use ut88_core::{Config, Machine};

fn main() -> anyhow::Result<()> {
    let config = match std::env::args().nth(1).as_deref() {
        Some("full") => Config::full(),
        Some("full0") => Config::full_with_monitor0(),
        _ => Config::minimal(),
    };
    ut88_tui::run(Machine::new(config), ut88_tui::Options::default())
}
