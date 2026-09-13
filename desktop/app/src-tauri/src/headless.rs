//! `wicket-desktop --headless`: the server without a window or tray, for
//! tests and machines without a display. Exits on SIGTERM or Ctrl-C.

use std::path::PathBuf;

use wicket_core::Config;
use wicket_core::api::{self, AppState};

#[derive(Debug, Default)]
pub struct Options {
    pub port: Option<u16>,
    pub data_dir: Option<PathBuf>,
    pub sdk_dir: Option<PathBuf>,
}

/// Parses `--headless [--port N] [--data-dir D] [--sdk-dir S]`. Returns
/// `None` when `--headless` is absent, so the app starts normally.
pub fn parse(args: &[String]) -> Result<Option<Options>, String> {
    if !args.iter().any(|a| a == "--headless") {
        return Ok(None);
    }
    let mut options = Options::default();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--headless" => {}
            "--port" => {
                let value = iter.next().ok_or("--port needs a value")?;
                options.port = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--port {value} is not a number"))?,
                );
            }
            "--data-dir" => {
                options.data_dir = Some(iter.next().ok_or("--data-dir needs a value")?.into());
            }
            "--sdk-dir" => {
                options.sdk_dir = Some(iter.next().ok_or("--sdk-dir needs a value")?.into());
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Some(options))
}

pub fn run(options: Options) -> i32 {
    let mut config = Config::from_env();
    if let Some(port) = options.port {
        config.port = port;
    }
    if let Some(dir) = options.data_dir {
        config.data_dir = dir;
    }
    if let Some(dir) = options.sdk_dir {
        config.sdk_dir = Some(dir);
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(error) => {
            eprintln!("wicket: cannot start: {error}");
            return 1;
        }
    };
    runtime.block_on(async {
        let state = match AppState::open(config) {
            Ok(state) => state,
            Err(error) => {
                eprintln!("wicket: cannot open the data directory: {error}");
                return 1;
            }
        };
        eprintln!(
            "wicket: serving on {} (data in {})",
            state.config.url(),
            state.config.data_dir.display()
        );
        match api::serve(state, shutdown_signal()).await {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("wicket: the server stopped: {error}");
                1
            }
        }
    })
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
