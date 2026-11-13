//! `asm-k580vm80a` — ассемблер КР580ВМ80А для ЮТ-88. Справка: `asm-k580vm80a -h`.

use std::ffi::OsString;

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let code = asm_k580vm80a::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
