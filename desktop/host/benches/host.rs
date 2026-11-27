//! `mise run bench:host`: the terminal host's performance budgets.
//!
//! - Parse throughput of one terminal with no viewer, through the query
//!   filter and the screen, on the `tui` and `plain` workloads: at least
//!   250 and 500 MB/s.
//! - Memory of a terminal holding 10,000 lines of 120 columns: at most
//!   15 MiB.
//! - CPU of the host with 10 idle agents: at most 1 % of one core.
//! - First paint of 5,000 lines: at most 10 ms.
//!
//! These have no budget, and are compared with the baseline only:
//!
//! - Output to viewers, through the PTY, the host and its frames, as a
//!   viewer that returns credit reads it: one viewer, and three of the same
//!   terminal.
//! - Output through the viewers' frames and outboxes alone, in this
//!   process and to a sink: one viewer, and three.
//! - Attach with the binary snapshot of 10,000 lines: from the request to
//!   the snapshot's end.
//! - First paint of 5,000 lines with a full-screen program on the
//!   alternate screen.
//! - The host's resident memory with 10 terminals of 10,000 lines each.
//!
//! It prints the results as JSON and exits with 1 when a budget fails.
//! With `--baseline <file>`, it also fails when a result is more than 15 %
//! worse than the baseline's, beyond a small margin for the measures whose
//! values are too small for a ratio to mean much, and `--write-baseline
//! <file>` writes this run's results as the baseline. `--skip-budgets`
//! checks the baseline only, for machines other than the reference ones,
//! such as CI's, and `--no-fail` reports the failures without exiting with
//! 1. Each result is the best of three runs.
//!
//!     cargo bench -p pinrail-host --bench host -- [--baseline <file>] [--write-baseline <file>] [--skip-budgets] [--no-fail]

#[cfg(unix)]
#[path = "host/unix.rs"]
mod unix;

#[cfg(unix)]
fn main() {
    unix::main();
}

/// The host runs only on Unix, so there is nothing to measure elsewhere.
#[cfg(not(unix))]
fn main() {}
