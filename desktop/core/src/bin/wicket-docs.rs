//! Prints one of the docs' generated reference pages to stdout:
//! `wicket-docs settings`.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("settings") => print!("{}", wicket_core::docs::settings_page()),
        _ => {
            eprintln!("usage: wicket-docs settings");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}
