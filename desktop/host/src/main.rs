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
    use pinrail_host::frame::{self, Frame};
    use pinrail_host::launch;
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
  connect  Bridge standard input and output to the running host's socket,
           presenting its secret in the first message, for SSH

Options:
  --data-dir <dir>       Pinrail's data folder (default: $PINRAIL_DATA_DIR,
                         else ~/.local/share/pinrail)
  --idle-exit <seconds>  With serve: exit after this long with no terminals
                         and no controller (default: 600)
  --ping-every <seconds> With serve: how often a controller hears from an
                         idle host (default: 15)
";

    struct Args {
        command: String,
        data_dir: PathBuf,
        idle_exit: Duration,
        ping_every: Duration,
        daemonize: bool,
        handshake: Option<PathBuf>,
    }

    fn parse() -> Result<Args, String> {
        let mut args = std::env::args().skip(1);
        let mut command = None;
        let mut data_dir = None;
        let mut idle_exit = server::IDLE_EXIT;
        let mut ping_every = server::PING_EVERY;
        let mut daemonize = false;
        let mut handshake = None;
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
                "--ping-every" => {
                    let seconds: u64 = args
                        .next()
                        .and_then(|s| s.parse().ok())
                        .filter(|&s| s > 0)
                        .ok_or("--ping-every needs a number of seconds")?;
                    ping_every = Duration::from_secs(seconds);
                }
                "--daemonize" => daemonize = true,
                "--handshake" => {
                    handshake = Some(PathBuf::from(
                        args.next().ok_or("--handshake needs a socket")?,
                    ))
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
            ping_every,
            daemonize,
            handshake,
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
            "serve" => {
                if let Err(error) = detach(&args) {
                    return fail(&error.to_string());
                }
                serve(args)
            }
            "status" => status(&args.data_dir),
            "stop" => stop(&args.data_dir),
            "connect" => connect(&args.data_dir),
            other => {
                eprintln!("pinrail-host: unknown command {other:?}\n");
                eprint!("{USAGE}");
                ExitCode::from(2)
            }
        }
    }

    /// Takes the environment the core sends, then leaves the core's
    /// process tree, as `--handshake` and `--daemonize` ask, while the
    /// process still has one thread.
    fn detach(args: &Args) -> std::io::Result<()> {
        let stream = match &args.handshake {
            Some(socket) => Some(launch::receive_env(socket)?),
            None => None,
        };
        if args.daemonize {
            launch::daemonize()?;
        }
        if let Some(stream) = stream {
            launch::report_detached(stream)?;
        }
        Ok(())
    }

    fn serve(args: Args) -> ExitCode {
        match server::serve(Config {
            data_dir: args.data_dir,
            idle_exit: args.idle_exit,
            ping_every: args.ping_every,
        }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(&error.to_string()),
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

    /// Bridges standard input and output to the running host's socket, for
    /// a core that reaches this computer over SSH. The first message, its
    /// `hello`, gets the host's secret, so being able to run this command
    /// as the person is what lets a remote core in.
    fn connect(data_dir: &std::path::Path) -> ExitCode {
        let Some(registration) = Registration::read(&Paths::new(data_dir).registration())
            .filter(Registration::may_be_live)
        else {
            return fail("no host is running");
        };
        let socket = match std::os::unix::net::UnixStream::connect(&registration.socket) {
            Ok(socket) => socket,
            Err(error) => return fail(&error.to_string()),
        };
        let mut stdin = std::io::stdin().lock();
        let first = match frame::read_blocking(&mut stdin) {
            Ok(Some(first)) => first,
            _ => return fail("the first message must be a hello"),
        };
        let Ok(mut hello) = serde_json::from_slice::<serde_json::Value>(&first.payload) else {
            return fail("the first message must be a hello");
        };
        hello["params"]["secret"] = json!(registration.secret);
        let mut writer = match socket.try_clone() {
            Ok(writer) => writer,
            Err(error) => return fail(&error.to_string()),
        };
        if frame::write_blocking(&mut writer, &Frame::json(&hello)).is_err() {
            return fail("the host closed the connection");
        }
        let mut reader = socket;
        // flushed after each read: stdout is line-buffered, and frames are
        // binary
        let output = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let mut stdout = std::io::stdout().lock();
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if stdout
                            .write_all(&buf[..n])
                            .and_then(|()| stdout.flush())
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
        let _ = std::io::copy(&mut stdin, &mut writer);
        let _ = writer.shutdown(std::net::Shutdown::Write);
        let _ = output.join();
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
