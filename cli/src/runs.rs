//! `pinrail start`, `runs`, `stop` and `host status`: the agents Pinrail
//! runs in its terminal host, for the person at their own terminal. These
//! act as the person, with the person token the app writes to the data
//! folder while the setting `/runtime/cli_control` is on.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::Output;
use crate::api::{Client, segment};

#[derive(Args, Debug)]
pub struct StartArgs {
    /// The agent to start, such as claude-code or codex
    agent: String,
    /// The folder to start it in; the current folder by default
    #[arg(long)]
    folder: Option<PathBuf>,
    /// The first task, typed in as the agent's first prompt
    #[arg(long)]
    task: Option<String>,
    /// Attach this terminal to the agent once it has started
    #[arg(long)]
    attach: bool,
    /// Extra arguments for the agent, after `--`. Arguments that turn its
    /// permission checks off are refused.
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct RunsArgs {
    /// Only the runs whose agent is still running
    #[arg(long)]
    live: bool,
}

#[derive(Args, Debug)]
pub struct StopArgs {
    /// The run to stop, such as run_01J…
    run: String,
    /// Stop the agent at once, without giving it 2 seconds to exit
    #[arg(long)]
    force: bool,
}

#[derive(Args, Debug)]
pub struct HostArgs {
    #[command(subcommand)]
    command: HostCommand,
}

#[derive(Subcommand, Debug)]
enum HostCommand {
    /// Print the terminal host's state and its terminals as JSON, for
    /// diagnosis
    Status,
}

/// The path of the person token the app writes for the CLI.
pub fn person_token_path() -> PathBuf {
    crate::server::data_dir().join("person-token")
}

/// The client, acting as the person, or why it cannot.
pub fn as_person(client: Client) -> Result<Client> {
    let path = person_token_path();
    let token = std::fs::read_to_string(&path).map_err(|_| {
        anyhow::anyhow!(
            "this command acts as you, and needs the setting /runtime/cli_control on. \
             Pinrail then writes your person token to {}, which this command reads. \
             Every program you run can read that file too, agents included.",
            path.display()
        )
    })?;
    Ok(client.with_person_token(token.trim().to_string()))
}

/// The size of the terminal on standard output, if it is one.
#[allow(unsafe_code)]
pub fn terminal_size() -> Option<(u16, u16)> {
    #[cfg(unix)]
    {
        // SAFETY: a winsize is plain integers, for which zero is valid, and
        // TIOCGWINSZ writes one into the struct it is given.
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        // SAFETY: as above; the descriptor is only read.
        let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) } == 0;
        (ok && size.ws_col > 0 && size.ws_row > 0).then_some((size.ws_col, size.ws_row))
    }
    #[cfg(not(unix))]
    None
}

pub fn start(client: &Client, args: StartArgs, output: Output) -> Result<u8> {
    let folder = match args.folder {
        Some(folder) => crate::full_path(&folder)?,
        None => std::env::current_dir().context("reading the current folder")?,
    };
    let (cols, rows) = terminal_size().unwrap_or((120, 40));
    let mut body = json!({
        "host": "local",
        "agent": args.agent,
        "folder": folder.to_string_lossy(),
        "args": args.args,
        "cols": cols,
        "rows": rows,
    });
    if let Some(task) = args.task {
        body["task"] = json!(task);
    }
    let started = client.post("/api/v1/runs", Some(&body))?;
    if args.attach {
        let run = started["run"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        return crate::attach_to(client, run);
    }
    output.data(&started, started_md);
    Ok(0)
}

pub fn runs(client: &Client, args: RunsArgs, output: Output) -> Result<u8> {
    let query = if args.live {
        vec![("state", "live".to_string())]
    } else {
        vec![]
    };
    let runs = client.get("/api/v1/runs", &query)?;
    output.data(&runs, runs_md);
    Ok(0)
}

pub fn stop(client: &Client, args: StopArgs, output: Output) -> Result<u8> {
    client.post(
        &format!("/api/v1/runs/{}/stop", segment(&args.run)),
        Some(&json!({"force": args.force})),
    )?;
    let stopping = json!({"run": args.run, "stopping": true});
    output.data(&stopping, |v| {
        format!(
            "Stopping {}. The run ends when its agent has exited.\n",
            v["run"].as_str().unwrap_or_default()
        )
    });
    Ok(0)
}

pub fn host(client: &Client, args: HostArgs, pretty: bool) -> Result<u8> {
    match args.command {
        HostCommand::Status => {
            let host = client.get("/api/v1/hosts/local", &[])?;
            // JSON whatever the output asked for, since it is for diagnosis
            let _ = pretty;
            crate::out::print_json(&host, true);
        }
    }
    Ok(0)
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

/// One run, on a line: its id, state, agent, folder and session.
fn run_line(run: &Value) -> String {
    let session = match &run["session"] {
        Value::Null => String::new(),
        session => format!(" · session {}", text(&session["id"])),
    };
    let ended = match &run["end_reason"] {
        Value::Null => String::new(),
        reason => format!(" ({})", text(reason)),
    };
    format!(
        "- {} · {}{} · {} · {}{}\n",
        text(&run["id"]),
        text(&run["state"]),
        ended,
        text(&run["agent"]),
        text(&run["folder"]),
        session
    )
}

fn started_md(started: &Value) -> String {
    let run = &started["run"];
    let mut md = format!(
        "Started {} in {}: {}\n",
        text(&run["agent"]),
        text(&run["folder"]),
        text(&run["id"])
    );
    if let Value::Object(session) = &started["session"] {
        md.push_str(&format!("Session: {}\n", text(&session["id"])));
    }
    md.push_str(&format!(
        "Attach to it with: pinrail attach {}\n",
        text(&run["id"])
    ));
    md
}

fn runs_md(runs: &Value) -> String {
    let rows = runs["runs"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return "No runs.\n".to_string();
    }
    rows.iter().map(run_line).collect()
}
