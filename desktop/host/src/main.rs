//! `pinrail-host`: runs the terminals of the agents Pinrail starts, in a
//! process that outlives the app.

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    cli::main()
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("pinrail-host runs only on macOS and Linux");
    std::process::ExitCode::FAILURE
}

#[cfg(unix)]
mod cli {
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::time::Duration;

    use pinrail_host::client::Client;
    use pinrail_host::paths::{self, Paths};
    use pinrail_host::registration::Registration;
    use pinrail_host::server::{self, Config};
    use serde_json::json;

    const USAGE: &str = "\
Usage: pinrail-host <command> [--data-dir <dir>]

Commands:
  serve    Run the host until it is stopped or idle for 10 minutes
  status   Print the running host's registration and terminal count as JSON
  stop     Stop the running host and its terminals

Options:
  --data-dir <dir>       Pinrail's data folder (default: $PINRAIL_DATA_DIR,
                         else ~/.local/share/pinrail)
  --idle-exit <seconds>  With serve: exit after this long with no terminals
                         and no controller (default: 600)
";

    struct Args {
        command: String,
        data_dir: PathBuf,
        idle_exit: Duration,
    }

    fn parse() -> Result<Args, String> {
        let mut args = std::env::args().skip(1);
        let mut command = None;
        let mut data_dir = None;
        let mut idle_exit = server::IDLE_EXIT;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--data-dir" => {
                    data_dir = Some(PathBuf::from(
                        args.next().ok_or("--data-dir needs a folder")?,
                    ))
                }
                "--idle-exit" => {
                    let seconds: u64 = args
                        .next()
                        .and_then(|s| s.parse().ok())
                        .ok_or("--idle-exit needs a number of seconds")?;
                    idle_exit = Duration::from_secs(seconds);
                }
                "-h" | "--help" => return Err(String::new()),
                "-V" | "--version" => {
                    println!("{}", pinrail_host::name());
                    std::process::exit(0);
                }
                _ if command.is_none() && !arg.starts_with('-') => command = Some(arg),
                _ => return Err(format!("unexpected argument {arg:?}")),
            }
        }
        Ok(Args {
            command: command.ok_or("")?,
            data_dir: data_dir.unwrap_or_else(paths::default_data_dir),
            idle_exit,
        })
    }

    pub fn main() -> ExitCode {
        let args = match parse() {
            Ok(args) => args,
            Err(message) => {
                if !message.is_empty() {
                    eprintln!("pinrail-host: {message}\n");
                }
                eprint!("{USAGE}");
                return ExitCode::from(2);
            }
        };
        match args.command.as_str() {
            "serve" => match server::serve(Config {
                data_dir: args.data_dir,
                idle_exit: args.idle_exit,
            }) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => fail(&error.to_string()),
            },
            "status" => status(&args.data_dir),
            "stop" => stop(&args.data_dir),
            other => {
                eprintln!("pinrail-host: unknown command {other:?}\n");
                eprint!("{USAGE}");
                ExitCode::from(2)
            }
        }
    }

    fn fail(message: &str) -> ExitCode {
        eprintln!("pinrail-host: {message}");
        ExitCode::FAILURE
    }

    /// The running host's registration and a connection to it, or `None`
    /// when no host of this boot answers.
    fn running(data_dir: &std::path::Path) -> Option<(Registration, Client)> {
        let registration = Registration::read(&Paths::new(data_dir).registration())?;
        if !registration.may_be_live() {
            return None;
        }
        let client = Client::connect(&registration).ok()?;
        Some((registration, client))
    }

    fn status(data_dir: &std::path::Path) -> ExitCode {
        let Some((registration, mut client)) = running(data_dir) else {
            println!("{}", json!({"running": false}));
            return ExitCode::from(1);
        };
        let info = match client.call("host.info", json!({})) {
            Ok(info) => info,
            Err(error) => return fail(&error.to_string()),
        };
        let mut out = registration.to_json();
        let map = out.as_object_mut().expect("an object");
        map.remove("secret");
        map.insert("running".to_string(), json!(true));
        map.insert("terminals".to_string(), info["terminals"].clone());
        println!("{out}");
        ExitCode::SUCCESS
    }

    fn stop(data_dir: &std::path::Path) -> ExitCode {
        let Some((_, mut client)) = running(data_dir) else {
            eprintln!("pinrail-host: no host is running");
            return ExitCode::SUCCESS;
        };
        if let Err(error) = client.call("host.stop", json!({"stop_terminals": true})) {
            return fail(&error.to_string());
        }
        client.wait_closed();
        // the host removes its registration as it exits
        let registration = Paths::new(data_dir).registration();
        for _ in 0..150 {
            if !registration.exists() {
                return ExitCode::SUCCESS;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        fail("the host did not exit within 15 seconds")
    }
}
