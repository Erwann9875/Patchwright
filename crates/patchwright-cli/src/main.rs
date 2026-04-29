#![forbid(unsafe_code)]

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();

    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("patchwright {}", patchwright_core::VERSION);
        return Ok(());
    }

    match args[0].as_str() {
        "status" => {
            println!("patchwright: ready");
            Ok(())
        }
        "config" if args.get(1).map(String::as_str) == Some("check") => {
            println!("config: no config file required for this command");
            Ok(())
        }
        "bench" if args.get(1).map(String::as_str) == Some("startup") => {
            println!("startup benchmark command registered");
            Ok(())
        }
        "solve" => Err("solve is not wired yet".to_owned()),
        "verify" => Err("verify is not wired yet".to_owned()),
        other => Err(format!("unknown command: {other}")),
    }
}

fn print_help() {
    println!(
        "patchwright\n\nUSAGE:\n    patchwright --version\n    patchwright status\n    patchwright config check\n    patchwright bench startup\n    patchwright solve --repo <path> --task <text>\n    patchwright verify --repo <path>"
    );
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn version_route_returns_before_heavy_commands() {
        let result = run(["--version".to_owned()]);
        assert!(result.is_ok());
    }

    #[test]
    fn unknown_command_is_an_error() {
        let result = run(["unknown".to_owned()]);
        assert_eq!(result, Err("unknown command: unknown".to_owned()));
    }
}
