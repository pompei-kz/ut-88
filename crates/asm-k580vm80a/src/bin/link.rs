//! `link-k580vm80a` — компоновщик объектных файлов `.link` ассемблера КР580ВМ80А для
//! ЮТ-88. Справка: `link-k580vm80a -h`.

use std::ffi::OsString;

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let code = asm_k580vm80a::link_cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
