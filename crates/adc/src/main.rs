mod cli;

use std::process;

fn main() {
    if let Err(err) = cli::run(std::env::args().skip(1)) {
        eprintln!("{err}");
        process::exit(2);
    }
}
