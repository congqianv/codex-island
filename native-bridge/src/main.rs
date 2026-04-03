use std::env;

use codex_island_native_bridge::run_cli;

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if let Err(error) = run_cli(&args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
