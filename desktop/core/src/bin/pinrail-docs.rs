//! Prints one of the docs' generated reference pages to stdout:
//! `pinrail-docs settings` or `pinrail-docs manifest`.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("settings") => print!("{}", pinrail_core::docs::settings_page()),
        Some("manifest") => print!("{}", pinrail_core::docs::manifest_page()),
        _ => {
            eprintln!("usage: pinrail-docs settings|manifest");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}
