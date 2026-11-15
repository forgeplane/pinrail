//! The snapshots the terminal benchmark's attach case restores, made by
//! the host's own terminal: the files given are fed to a terminal of 120
//! columns and 40 rows that keeps 5,000 lines of history, then its `vt`
//! first paint and its binary snapshot are written to the folder given.
//!
//! ```sh
//! cargo run -q --release -p pinrail-screen --example bench_snapshot -- <out> <name> <file>...
//! ```
//!
//! It writes `<name>.vt` and `<name>.bin`, and prints the binary format.

#[cfg(ghostty_built)]
fn main() {
    use pinrail_screen::ghostty::Terminal;
    use pinrail_screen::{Options, Screen};

    const HISTORY: usize = 5_000;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [out, name, files @ ..] = args.as_slice() else {
        eprintln!("usage: bench_snapshot <out> <name> <file>...");
        std::process::exit(2);
    };
    let mut terminal = Terminal::new(&Options {
        cols: 120,
        rows: 40,
        scrollback_lines: HISTORY,
    })
    .expect("a terminal");
    for file in files {
        let bytes = std::fs::read(file).unwrap_or_else(|e| panic!("{file}: {e}"));
        for chunk in bytes.chunks(64 * 1024) {
            terminal.write(chunk);
        }
    }
    let out = std::path::Path::new(out);
    std::fs::create_dir_all(out).expect("the output folder");
    std::fs::write(
        out.join(format!("{name}.vt")),
        terminal.first_paint(HISTORY),
    )
    .expect("the vt paint");
    let binary = terminal.binary_snapshot().expect("a binary snapshot");
    std::fs::write(out.join(format!("{name}.bin")), &binary.bytes).expect("the binary snapshot");
    println!("{}", binary.format);
}

#[cfg(not(ghostty_built))]
fn main() {
    eprintln!("bench_snapshot needs libghostty-vt, which this build stubbed out");
    std::process::exit(1);
}
