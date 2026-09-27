//! `Pinrail --headless`: the server without a window or tray, for
//! tests and machines without a display. Exits on SIGTERM or Ctrl-C.

use std::path::PathBuf;
use std::sync::Arc;

use pinrail_core::Config;
use pinrail_core::{Pinrail, api};

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

/// The core's configuration: the environment's, with the flags over it.
fn config(options: Options, exe: &std::path::Path) -> Config {
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
    if config.sdk_dir.is_none() {
        config.sdk_dir = crate::sdk_dir(exe);
    }
    config
}

pub fn run(options: Options) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    let config = config(options, &exe);

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(error) => {
            eprintln!("pinrail: cannot start: {error}");
            return 1;
        }
    };
    runtime.block_on(async {
        let state = match Pinrail::open(config) {
            Ok(state) => Arc::new(state),
            Err(error) => {
                eprintln!("pinrail: cannot open the data directory: {error}");
                return 1;
            }
        };
        eprintln!(
            "pinrail: serving on {} (data in {}, SDK from {})",
            state.config().url(),
            state.config().data_dir.display(),
            state
                .config()
                .sdk_dir
                .as_deref()
                .map_or_else(|| "nowhere".into(), |d| d.display().to_string())
        );
        match api::serve(state, shutdown_signal()).await {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("pinrail: the server stopped: {error}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headless_server_serves_the_sdk_its_bundle_ships() {
        // the packaged app run with --headless, as CI and PINRAIL_SERVER_CMD
        // do: no --sdk-dir, and plugin views still need the SDK
        let root = tempfile::tempdir().unwrap();
        let (exe, sdk) = if cfg!(target_os = "macos") {
            let contents = root.path().join("Pinrail.app/Contents");
            (
                contents.join("MacOS/Pinrail"),
                contents.join("Resources/sdk/v1"),
            )
        } else {
            let usr = root.path().join("usr");
            (
                usr.join("bin/pinrail-desktop"),
                usr.join("lib/Pinrail/sdk/v1"),
            )
        };
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        std::fs::create_dir_all(&sdk).unwrap();
        std::fs::write(sdk.join("pinrail-plugin.js"), "").unwrap();

        let config = config(Options::default(), &exe);
        assert_eq!(
            config.sdk_dir.map(|d| d.canonicalize().unwrap()),
            Some(sdk.canonicalize().unwrap())
        );
    }
}
