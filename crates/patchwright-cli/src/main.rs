#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    patchwright_cli::main_entry()
}
