//! `wicket`: the CLI agents call. It talks HTTP to the running server and
//! does nothing itself. JSON on stdout, diagnostics on stderr, meaningful
//! exit codes:
//!
//! | code | meaning |
//! |---|---|
//! | 0 | done |
//! | 1 | error: bad arguments, server unreachable, I/O |
//! | 2 | the server refused the request (404, 409, 422); the body is on stderr |
//! | 3 | the review was withdrawn or expired instead of decided |
//! | 4 | `wait` timed out; the review is still pending |

mod api;
mod out;
mod server;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};

use api::{ApiError, Client};

pub const EXIT_ERROR: u8 = 1;
pub const EXIT_REFUSED: u8 = 2;
pub const EXIT_CLOSED: u8 = 3;
pub const EXIT_TIMEOUT: u8 = 4;

#[derive(Parser)]
#[command(name = "wicket", version, about, long_about = None)]
struct Cli {
    /// Server URL; default: WICKET_URL, then the running server's server.json, then http://127.0.0.1:4747
    #[arg(long, global = true, env = "WICKET_URL")]
    url: Option<String>,

    /// Pretty-print JSON output
    #[arg(long, global = true)]
    pretty: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Submit a review; with --wait, block until it is decided and print the decision
    #[command(alias = "create")]
    Submit(SubmitArgs),
    /// Block until a review leaves pending; print it
    Wait(WaitArgs),
    /// Print a review: envelope, payload and decision
    Show { id: String },
    /// Every round of a review, oldest first, without payloads
    Rounds { id: String },
    /// A review's event log
    Events { id: String },
    /// List reviews, newest first, without payloads
    List(ListArgs),
    /// Record a decision from a script (the app is the usual way)
    Decide(DecideArgs),
    /// Withdraw a pending review; its waiter exits 3
    Withdraw {
        id: String,
        /// Why the requester gave up
        #[arg(long)]
        reason: Option<String>,
    },
    /// Registered plugins
    #[command(alias = "types")]
    Plugins(PluginsArgs),
    /// Write every review as JSON files under a directory
    Export { dir: PathBuf },
    /// Start the server if it is not running; print its URL
    Serve,
    /// Open a review in the app
    Open { id: String },
}

#[derive(Args)]
struct SubmitArgs {
    /// The plugin that defines this sort of review, e.g. code_review
    plugin: String,
    #[arg(long)]
    title: String,
    /// Where the review comes from: repo=acme,workflow=review,run_id=…,ref=42,url=…
    #[arg(long, alias = "source", value_parser = parse_origin)]
    origin: Option<BTreeMap<String, String>>,
    /// Payload JSON: a file path, or - for stdin
    #[arg(long, value_name = "FILE|-")]
    data: Option<String>,
    /// Inbox summary JSON, e.g. '{"counts":[["major",2]],"subtitle":"3 new"}'
    #[arg(long, value_parser = parse_json)]
    summary: Option<Value>,
    /// The review this one is a new round of
    #[arg(long, alias = "supersedes")]
    revises: Option<String>,
    /// ISO 8601 timestamp after which the review expires
    #[arg(long)]
    expires_at: Option<String>,
    #[arg(long, env = "WICKET_REQUESTED_BY", default_value = "wicket-cli")]
    requested_by: String,
    /// Block until decided (see wait)
    #[arg(long)]
    wait: bool,
    #[command(flatten)]
    wait_opts: WaitOpts,
    /// Do not start the server when it is not running
    #[arg(long)]
    no_start: bool,
}

#[derive(Args)]
struct WaitArgs {
    id: String,
    #[command(flatten)]
    opts: WaitOpts,
}

#[derive(Args, Clone)]
struct WaitOpts {
    /// Give up after this many seconds (exit 4); 0 waits forever
    #[arg(long, default_value_t = 0)]
    timeout: u64,
    /// Write decision.data to this file once decided
    #[arg(long, value_name = "FILE")]
    decision_out: Option<PathBuf>,
}

#[derive(Args)]
struct ListArgs {
    /// pending, decided, withdrawn, expired; comma-separated for several
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    repo: Option<String>,
    #[arg(long)]
    workflow: Option<String>,
    #[arg(long = "ref")]
    reference: Option<String>,
    #[arg(long)]
    run_id: Option<String>,
    #[arg(long, alias = "type")]
    plugin: Option<String>,
    /// Text to look for in titles and payloads
    #[arg(long)]
    q: Option<String>,
    /// Only reviews older than this id
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long)]
    limit: Option<u32>,
    /// Include rounds that a later round revises
    #[arg(long, alias = "superseded")]
    include_revised: bool,
}

#[derive(Args)]
struct DecideArgs {
    id: String,
    /// Decision JSON: a file path, or - for stdin
    #[arg(long, value_name = "FILE|-")]
    data: String,
    /// Free-text note to the requesting agent
    #[arg(long)]
    note: Option<String>,
}

#[derive(Args)]
struct PluginsArgs {
    #[command(subcommand)]
    command: Option<PluginsCommand>,
}

#[derive(Subcommand)]
enum PluginsCommand {
    /// Register a directory whose subdirectories are plugins
    Add { dir: PathBuf },
    /// Rescan the plugin directories
    Reload,
    /// The versions of a plugin that reviews can still render with
    Versions { name: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            if let Some(api) = err.downcast_ref::<ApiError>() {
                out::error_json(&api.body);
                ExitCode::from(EXIT_REFUSED)
            } else {
                eprintln!("wicket: {err:#}");
                ExitCode::from(EXIT_ERROR)
            }
        }
    }
}

fn run(cli: Cli) -> Result<u8> {
    let pretty = cli.pretty;

    if let Command::Serve = cli.command {
        let info = server::ensure_running(cli.url.as_deref())?;
        out::print_json(&info, pretty);
        return Ok(0);
    }

    let auto_start = matches!(&cli.command, Command::Submit(args) if !args.no_start);
    let base = server::resolve_url(cli.url.as_deref(), auto_start)?;
    let client = Client::new(&base);

    match cli.command {
        Command::Submit(args) => submit(&client, args, pretty),
        Command::Wait(args) => wait(&client, &args.id, &args.opts, pretty),
        Command::Show { id } => {
            out::print_json(&client.get_review(&id)?, pretty);
            Ok(0)
        }
        Command::Rounds { id } => {
            out::print_json(&client.rounds(&id)?, pretty);
            Ok(0)
        }
        Command::Events { id } => {
            out::print_json(&client.events(&id)?, pretty);
            Ok(0)
        }
        Command::List(args) => {
            let mut query: Vec<(&str, String)> = Vec::new();
            for (k, v) in [
                ("status", args.status),
                ("repo", args.repo),
                ("workflow", args.workflow),
                ("ref", args.reference),
                ("run_id", args.run_id),
                ("plugin", args.plugin),
                ("q", args.q),
                ("cursor", args.cursor),
                ("limit", args.limit.map(|n| n.to_string())),
            ] {
                if let Some(v) = v {
                    query.push((k, v));
                }
            }
            if args.include_revised {
                query.push(("include_revised", "true".into()));
            }
            out::print_json(&client.list(&query)?, pretty);
            Ok(0)
        }
        Command::Decide(args) => {
            let data = read_json_arg(&args.data)?;
            let review = client.decide(&args.id, data, args.note)?;
            out::print_json(&review, pretty);
            Ok(0)
        }
        Command::Withdraw { id, reason } => {
            out::print_json(&client.withdraw(&id, reason)?, pretty);
            Ok(0)
        }
        Command::Plugins(args) => {
            let value = match args.command {
                None => client.plugins()?,
                Some(PluginsCommand::Add { dir }) => {
                    let dir = std::path::absolute(&dir)?;
                    client.plugins_add(&dir.to_string_lossy())?
                }
                Some(PluginsCommand::Reload) => client.plugins_reload()?,
                Some(PluginsCommand::Versions { name }) => client.plugin_versions(&name)?,
            };
            out::print_json(&value, pretty);
            Ok(0)
        }
        Command::Export { dir } => {
            let count = out::export(&client, &dir)?;
            eprintln!("wicket: {count} reviews written to {}", dir.display());
            Ok(0)
        }
        Command::Open { id } => {
            let url = format!("{base}/reviews/{id}");
            server::open_browser(&url)?;
            eprintln!("{url}");
            Ok(0)
        }
        Command::Serve => unreachable!(),
    }
}

fn submit(client: &Client, args: SubmitArgs, pretty: bool) -> Result<u8> {
    let payload = match &args.data {
        Some(spec) => read_json_arg(spec)?,
        None => json!({}),
    };
    let mut body = json!({
        "plugin": args.plugin,
        "title": args.title,
        "payload": payload,
        "requested_by": args.requested_by,
    });
    if let Some(origin) = &args.origin {
        body["origin"] = json!(origin);
    }
    if let Some(summary) = &args.summary {
        body["summary"] = summary.clone();
    }
    if let Some(id) = &args.revises {
        body["revises"] = json!(id);
    }
    if let Some(at) = &args.expires_at {
        body["expires_at"] = json!(at);
    }

    let review = client.submit(&body)?;
    let id = review["id"]
        .as_str()
        .context("server returned a review without an id")?
        .to_string();
    eprintln!("review {id}: {}/reviews/{id}", client.base());

    if args.wait {
        wait(client, &id, &args.wait_opts, pretty)
    } else {
        out::print_json(&review, pretty);
        Ok(0)
    }
}

/// Long-polls until the review settles. Each poll asks the server for at
/// most `Client::POLL_SECS`; a 204 or a dropped connection (the server
/// restarting) just loops, so a wait survives the app coming and going.
fn wait(client: &Client, id: &str, opts: &WaitOpts, pretty: bool) -> Result<u8> {
    let deadline = (opts.timeout > 0).then(|| Instant::now() + Duration::from_secs(opts.timeout));
    let mut last_error = String::new();

    loop {
        let remaining = match deadline {
            Some(d) => {
                let left = d.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    eprintln!(
                        "wicket: timed out after {}s, review {id} is still pending",
                        opts.timeout
                    );
                    return Ok(EXIT_TIMEOUT);
                }
                left.as_secs().clamp(1, Client::POLL_SECS)
            }
            None => Client::POLL_SECS,
        };

        match client.wait(id, remaining) {
            Ok(Some(review)) => {
                let status = review["status"].as_str().unwrap_or("");
                if status == "pending" {
                    continue;
                }
                if status == "decided" {
                    if let Some(path) = &opts.decision_out {
                        out::write_decision(path, &review["decision"]["data"])?;
                        eprintln!("wicket: decision written to {}", path.display());
                    }
                    if let Some(counts) = out::editorial_counts(&review["decision"]["data"]) {
                        eprintln!("wicket: {counts}");
                    }
                }
                out::print_json(&review, pretty);
                return Ok(if status == "decided" {
                    0
                } else {
                    eprintln!("wicket: review {id} was {status}, not decided");
                    EXIT_CLOSED
                });
            }
            Ok(None) => continue,
            Err(err) if err.downcast_ref::<ApiError>().is_some() => return Err(err),
            Err(err) => {
                let message = format!("{err:#}");
                if message != last_error {
                    eprintln!("wicket: {message}; retrying until the server is back");
                    last_error = message;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn read_json_arg(spec: &str) -> Result<Value> {
    let text = if spec == "-" {
        std::io::read_to_string(std::io::stdin()).context("reading stdin")?
    } else {
        std::fs::read_to_string(spec).with_context(|| format!("reading {spec}"))?
    };
    serde_json::from_str(&text).with_context(|| format!("{spec} is not valid JSON"))
}

fn parse_json(s: &str) -> Result<Value, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

fn parse_origin(s: &str) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for pair in s.split(',').filter(|p| !p.trim().is_empty()) {
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| format!("expected key=value, got {pair:?}"))?;
        map.insert(k.trim().to_string(), v.trim().to_string());
    }
    if map.is_empty() {
        return Err("origin needs at least one key=value".into());
    }
    Ok(map)
}
