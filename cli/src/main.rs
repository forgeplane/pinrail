//! `pinrail`: the CLI agents call. It talks HTTP to the running server and
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
//! | 5 | the person discarded the review: stop the work it was gating |

mod api;
mod attachments;
mod describe;
#[cfg(feature = "docs")]
mod docs;
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
/// The person said no, and stop; the reason, if any, is in the envelope.
pub const EXIT_DISCARDED: u8 = 5;

#[derive(Parser)]
#[command(name = "pinrail", version, about, long_about = None)]
struct Cli {
    /// Server URL; default: PINRAIL_URL, then the running server's server.json, then http://127.0.0.1:4747
    #[arg(long, global = true, env = "PINRAIL_URL")]
    url: Option<String>,

    /// Pretty-print JSON output
    #[arg(long, global = true)]
    pretty: bool,

    /// How a review is printed: json (the default, for scripts) or
    /// markdown (for a session reading the decision); PINRAIL_FORMAT sets it
    #[arg(long, global = true, env = "PINRAIL_FORMAT", value_enum, default_value_t = Format::Json)]
    format: Format,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Format {
    Json,
    #[value(alias = "md")]
    Markdown,
}

/// How reviews are printed: JSON as the API gives them, or the markdown
/// the server renders. Everything that is not a review stays JSON.
#[derive(Clone, Copy)]
struct Output {
    pretty: bool,
    markdown: bool,
}

impl Output {
    fn review(&self, client: &Client, review: &serde_json::Value) -> Result<()> {
        if self.markdown
            && let Some(id) = review["id"].as_str()
        {
            println!("{}", client.review_markdown(id)?.trim_end());
        } else {
            out::print_json(review, self.pretty);
        }
        Ok(())
    }
}

#[derive(Subcommand)]
enum Command {
    /// Submit a review; with --wait, block until it is decided and print the decision
    ///
    /// The review is built from the flags: the plugin, --title, and the
    /// payload from --data. Or give the whole request as one JSON file with
    /// --request, the body the API takes:
    ///
    ///     {
    ///       "plugin": "list",
    ///       "title": "Sentry triage",
    ///       "origin": {"repo": "acme"},
    ///       "payload": {"groups": []}
    ///     }
    ///
    /// Its keys are plugin, title, payload, origin, summary, revises,
    /// expires_at and requested_by. Flags given as well override the file's
    /// keys, and --data replaces its payload, so a new round is the same
    /// file with --revises and the earlier round's id. The plugin argument
    /// can be left out when the file names one.
    ///
    /// Files go beside the payload with --attach, for a plugin that
    /// takes them (pinrail plugins describe says which). The payload names
    /// each one as {"$attachment": "<name>"}; the name is the file's own, or
    /// the one after =:
    ///
    ///     pinrail submit model --data models.json \
    ///       --attach out/pivot.glb --attach out/v2.glb=column.glb
    ///
    /// In a --request file they are "attachments": {"pivot.glb":
    /// "out/pivot.glb"}, paths relative to the file, or {"path": …,
    /// "media_type": …} as a plugin's fixture has them, so a fixture is
    /// sent as it is. The submission is checked before anything is
    /// uploaded, and a file the app already has is not sent again.
    #[command(alias = "create", verbatim_doc_comment)]
    Submit(SubmitArgs),
    /// Block until a review leaves pending; print it
    Wait(WaitArgs),
    /// Print a review: envelope, payload and decision
    Show {
        /// The review's id
        id: String,
    },
    /// Every round of a review, oldest first, without payloads
    Rounds {
        /// The review's id
        id: String,
    },
    /// A review's event log
    Events {
        /// The review's id
        id: String,
    },
    /// List reviews, newest first, without payloads
    List(ListArgs),
    /// Record a decision from a script (the app is the usual way)
    Decide(DecideArgs),
    /// Withdraw a pending review; its waiter exits 3
    Withdraw {
        /// The review's id
        id: String,
        /// Why the requester gave up
        #[arg(long)]
        reason: Option<String>,
    },
    /// Discard a pending review as the person would in the app; its waiter
    /// exits 5 and is told to stop
    Discard {
        /// The review's id
        id: String,
        /// Why, for the agent
        #[arg(long)]
        reason: Option<String>,
        /// Who discards it; the server's user when omitted
        #[arg(long)]
        by: Option<String>,
    },
    /// Registered plugins
    #[command(alias = "types")]
    Plugins(PluginsArgs),
    /// The files a review carries: list them, or save one
    #[command(subcommand)]
    Attachments(AttachmentsCommand),
    /// Write every review as JSON files under a directory
    Export {
        /// Where to write them; created when missing
        dir: PathBuf,
    },
    /// Start the server if it is not running; print its URL
    Serve,
    /// Open a review in the app
    Open {
        /// The review's id
        id: String,
    },
}

#[derive(Args)]
struct SubmitArgs {
    /// The plugin that defines this sort of review, e.g. code_review
    #[arg(required_unless_present = "request")]
    plugin: Option<String>,
    /// What the review is about, as the inbox shows it
    #[arg(long, required_unless_present = "request")]
    title: Option<String>,
    /// The whole request as JSON: a file path, or - for stdin; flags
    /// override its keys
    #[arg(long, value_name = "FILE|-")]
    request: Option<String>,
    /// Where the review comes from: repo=acme,workflow=review,run_id=…,ref=42,url=…
    #[arg(long, alias = "source", value_parser = parse_origin)]
    origin: Option<BTreeMap<String, String>>,
    /// Payload JSON: a file path, or - for stdin
    #[arg(long, value_name = "FILE|-")]
    data: Option<String>,
    /// A file to send beside the payload, which names it {"$attachment":
    /// "<name>"}; the name is the file's own unless given after =.
    /// Repeat for more
    #[arg(long = "attach", value_name = "PATH[=NAME]", value_parser = attachments::parse_flag)]
    attachments: Vec<(String, PathBuf)>,
    /// Inbox summary JSON, e.g. '{"counts":[["major",2]],"subtitle":"3 new"}'
    #[arg(long, value_parser = parse_json)]
    summary: Option<Value>,
    /// The review this one is a new round of
    #[arg(long, alias = "supersedes")]
    revises: Option<String>,
    /// ISO 8601 timestamp after which the review expires
    #[arg(long)]
    expires_at: Option<String>,
    /// Who is asking, shown on the review [default: pinrail-cli]
    #[arg(long, env = "PINRAIL_REQUESTED_BY")]
    requested_by: Option<String>,
    /// Block until decided (see wait)
    #[arg(long)]
    wait: bool,
    /// Run every check a submission gets and create no review: exit 0 when
    /// it would be accepted, 2 with the violations
    #[arg(long, conflicts_with = "wait")]
    dry_run: bool,
    #[command(flatten)]
    wait_opts: WaitOpts,
    /// Do not start the server when it is not running
    #[arg(long)]
    no_start: bool,
}

#[derive(Args)]
struct WaitArgs {
    /// The review's id
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
    /// pending, decided, withdrawn, discarded, expired; comma-separated for several
    #[arg(long)]
    status: Option<String>,
    /// The project (origin repo); "-" for reviews that name none
    #[arg(long)]
    repo: Option<String>,
    /// The workflow that asked (origin workflow)
    #[arg(long)]
    workflow: Option<String>,
    /// The branch, pull request or other ref (origin ref)
    #[arg(long = "ref")]
    reference: Option<String>,
    /// The run that asked (origin run_id)
    #[arg(long)]
    run_id: Option<String>,
    /// Only reviews of this plugin
    #[arg(long, alias = "type")]
    plugin: Option<String>,
    /// Words to look for, all of them, in titles, payloads, plugins, requesters, origins and who decided
    #[arg(long)]
    q: Option<String>,
    /// Only reviews older than this id
    #[arg(long)]
    cursor: Option<String>,
    /// How many reviews to ask for; with --all, how many per request
    #[arg(long)]
    limit: Option<u32>,
    /// Every matching review, following the cursor to the last page
    #[arg(long)]
    all: bool,
    /// Include rounds that a later round revises
    #[arg(long, alias = "superseded")]
    include_revised: bool,
}

#[derive(Args)]
struct DecideArgs {
    /// The review's id
    id: String,
    /// Decision JSON: a file path, or - for stdin
    #[arg(long, value_name = "FILE|-")]
    data: String,
    /// Free-text note to the requesting agent
    #[arg(long)]
    note: Option<String>,
}

#[derive(Subcommand)]
enum AttachmentsCommand {
    /// The files a review carries: name, size, media type and hash
    List {
        /// The review's id
        id: String,
    },
    /// Save a file a review carries
    Get {
        /// The review's id
        id: String,
        /// The file's name on the review
        name: String,
        /// Where to write it: a path, or - for stdout [default: the name,
        /// in the current directory]
        #[arg(short, long, value_name = "PATH|-")]
        output: Option<String>,
        /// Replace a file that is already there
        #[arg(long)]
        force: bool,
    },
}

#[derive(Args)]
struct PluginsArgs {
    #[command(subcommand)]
    command: Option<PluginsCommand>,
}

#[derive(Subcommand)]
enum PluginsCommand {
    /// Install one plugin into the app's store: a folder, a repository
    /// (github.com/acme/plugins/review@v3, or the folder's URL in the
    /// browser), or a GitHub release
    Install {
        /// the plugin's folder, repository or release
        source: String,
        /// serve a folder live instead of copying it, for development
        #[arg(long)]
        link: bool,
        /// replace a newer version that is already installed
        #[arg(long)]
        force: bool,
        /// a branch, tag or commit, for a git source that does not say
        #[arg(long = "ref")]
        reference: Option<String>,
        /// the plugin's folder inside the repository, likewise
        #[arg(long)]
        path: Option<String>,
    },
    /// Install a plugin again from where it came, whatever is new there;
    /// every installed plugin when no name is given
    Update {
        /// the plugin's name, as `pinrail plugins` lists it
        name: Option<String>,
    },
    /// Remove an installed plugin; store entries a review still renders
    /// from are kept
    Remove {
        /// the plugin's name
        name: String,
    },
    /// What an agent needs to ask with each usable plugin, or the one
    /// named: what it is for and when to use it, its payload and decision
    /// schemas, an example payload, and the exit codes; markdown with
    /// --format markdown
    Describe {
        /// the plugin's name; every usable plugin when omitted
        name: Option<String>,
    },
    /// Reload the installed plugins from disk
    Reload,
    /// The versions of a plugin that reviews can still render with
    Versions {
        /// the plugin's name
        name: String,
    },
}

fn main() -> ExitCode {
    // the docs' CLI reference, from this definition; only in a docs build
    #[cfg(feature = "docs")]
    if std::env::args().nth(1).as_deref() == Some("--markdown-help") {
        print!(
            "{}",
            docs::render(&<Cli as clap::CommandFactory>::command())
        );
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            if let Some(api) = err.downcast_ref::<ApiError>() {
                out::error_json(&api.body);
                ExitCode::from(EXIT_REFUSED)
            } else {
                eprintln!("pinrail: {err:#}");
                ExitCode::from(EXIT_ERROR)
            }
        }
    }
}

fn run(cli: Cli) -> Result<u8> {
    let pretty = cli.pretty;
    let output = Output {
        pretty,
        markdown: cli.format == Format::Markdown,
    };

    if let Command::Serve = cli.command {
        let info = server::ensure_running(cli.url.as_deref())?;
        out::print_json(&info, pretty);
        return Ok(0);
    }

    let auto_start = match &cli.command {
        Command::Submit(args) => !args.no_start,
        Command::Plugins(args) => matches!(args.command, Some(PluginsCommand::Describe { .. })),
        _ => false,
    };
    let base = server::resolve_url(cli.url.as_deref(), auto_start)?;
    let client = Client::new(&base);

    match cli.command {
        Command::Submit(args) => submit(&client, args, output),
        Command::Wait(args) => wait(&client, &args.id, &args.opts, output),
        Command::Show { id } => {
            output.review(&client, &client.get_review(&id)?)?;
            Ok(0)
        }
        Command::Attachments(AttachmentsCommand::List { id }) => {
            let review = client.get_review(&id)?;
            out::print_json(&review["attachments"], pretty);
            Ok(0)
        }
        Command::Attachments(AttachmentsCommand::Get {
            id,
            name,
            output: to,
            force,
        }) => {
            let to = to.unwrap_or_else(|| name.clone());
            let size = if to == "-" {
                client.download_attachment(&id, &name, &mut std::io::stdout().lock())?
            } else {
                let path = PathBuf::from(&to);
                anyhow::ensure!(
                    force || !path.exists(),
                    "{to} is already there; --force replaces it"
                );
                // written beside, then moved: a failed download leaves nothing half there
                let partial = path.with_file_name(format!(
                    ".{}.part",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ));
                let mut file = std::fs::File::create(&partial)
                    .with_context(|| format!("writing {}", partial.display()))?;
                let result = client.download_attachment(&id, &name, &mut file);
                drop(file);
                match result {
                    Ok(size) => {
                        std::fs::rename(&partial, &path)
                            .with_context(|| format!("writing {to}"))?;
                        size
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&partial);
                        return Err(e);
                    }
                }
            };
            if to != "-" {
                eprintln!(
                    "pinrail: saved {name} to {to} ({})",
                    attachments::human(size)
                );
            }
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
            // the reviews alone, as before the API wrapped them with its paging
            let mut listing = client.list(&query)?;
            if !args.all {
                out::print_json(&listing["reviews"], pretty);
                return Ok(0);
            }
            let mut reviews = Vec::new();
            loop {
                if let Some(page) = listing["reviews"].as_array_mut() {
                    reviews.append(page);
                }
                match listing["next_cursor"].as_str() {
                    Some(next) if listing["has_more"] == true => {
                        let next = next.to_string();
                        query.retain(|(k, _)| *k != "cursor");
                        query.push(("cursor", next));
                        listing = client.list(&query)?;
                    }
                    _ => break,
                }
            }
            out::print_json(&Value::Array(reviews), pretty);
            Ok(0)
        }
        Command::Decide(args) => {
            let data = read_json_arg(&args.data)?;
            let review = client.decide(&args.id, data, args.note)?;
            output.review(&client, &review)?;
            Ok(0)
        }
        Command::Withdraw { id, reason } => {
            out::print_json(&client.withdraw(&id, reason)?, pretty);
            Ok(0)
        }
        Command::Discard { id, reason, by } => {
            out::print_json(&client.discard(&id, reason, by)?, pretty);
            Ok(0)
        }
        Command::Plugins(PluginsArgs {
            command: Some(PluginsCommand::Describe { name }),
        }) => {
            let described = client.plugins_describe(name.as_deref())?;
            if output.markdown {
                print!("{}", describe::markdown(&described));
            } else {
                out::print_json(&describe::document(described), pretty);
            }
            Ok(0)
        }
        Command::Plugins(args) => {
            let value = match args.command {
                None => client.plugins()?,
                Some(PluginsCommand::Install {
                    source,
                    link,
                    force,
                    reference,
                    path,
                }) => {
                    // a folder that exists is sent as an absolute path
                    let source = match std::path::absolute(&source) {
                        Ok(p) if p.is_dir() => p.to_string_lossy().into_owned(),
                        _ => source,
                    };
                    client.plugins_install(
                        &source,
                        link,
                        force,
                        reference.as_deref(),
                        path.as_deref(),
                    )?
                }
                Some(PluginsCommand::Update { name }) => {
                    let names: Vec<String> = match name {
                        Some(name) => vec![name],
                        None => client.plugins()?["plugins"]
                            .as_array()
                            .map(|rows| {
                                rows.iter()
                                    .filter(|p| {
                                        p["install"].is_object() && p["install"]["linked"] != true
                                    })
                                    .filter_map(|p| p["name"].as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    };
                    let mut answers = Vec::new();
                    for name in names {
                        eprintln!("pinrail: {name}");
                        answers.push(client.plugins_update(&name)?);
                    }
                    match answers.len() {
                        1 => answers.remove(0),
                        _ => serde_json::Value::Array(answers),
                    }
                }
                Some(PluginsCommand::Remove { name }) => client.plugins_remove(&name)?,
                Some(PluginsCommand::Reload) => client.plugins_reload()?,
                Some(PluginsCommand::Versions { name }) => client.plugin_versions(&name)?,
                Some(PluginsCommand::Describe { .. }) => unreachable!(),
            };
            out::print_json(&value, pretty);
            Ok(0)
        }
        Command::Export { dir } => {
            let count = out::export(&client, &dir)?;
            eprintln!("pinrail: {count} reviews written to {}", dir.display());
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

fn submit(client: &Client, args: SubmitArgs, output: Output) -> Result<u8> {
    if args.request.as_deref() == Some("-") && args.data.as_deref() == Some("-") {
        anyhow::bail!("--request and --data cannot both read stdin");
    }
    let mut body = match &args.request {
        Some(spec) => match read_json_arg(spec)? {
            Value::Object(map) => Value::Object(map),
            _ => anyhow::bail!("{spec} must hold a JSON object, the request"),
        },
        None => json!({}),
    };
    if let Some(plugin) = &args.plugin {
        body["plugin"] = json!(plugin);
    }
    if let Some(title) = &args.title {
        body["title"] = json!(title);
    }
    if let Some(spec) = &args.data {
        body["payload"] = read_json_arg(spec)?;
    } else if body.get("payload").is_none() {
        body["payload"] = json!({});
    }
    match &args.requested_by {
        Some(by) => body["requested_by"] = json!(by),
        None if body.get("requested_by").is_none() => body["requested_by"] = json!("pinrail-cli"),
        None => {}
    }
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

    // the files: the request's map of name to path, then the flags
    let request_dir = match args.request.as_deref() {
        Some(spec) if spec != "-" => std::path::Path::new(spec)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default(),
        _ => PathBuf::new(),
    };
    let listed = body.as_object_mut().and_then(|m| m.remove("attachments"));
    let files = attachments::read(&attachments::collect(
        listed.as_ref(),
        &request_dir,
        &args.attachments,
    )?)?;
    if !files.is_empty() {
        body["attachments"] = attachments::declare(&files);
        // checked before anything is uploaded, so a submission that would be
        // refused does not move a byte
        if !args.dry_run {
            client.validate(&body)?;
            attachments::upload(client, &files)?;
        }
    }

    if args.dry_run {
        let answer = client.validate(&body)?;
        eprintln!(
            "pinrail: valid; {} {} would render it",
            answer["plugin"].as_str().unwrap_or_default(),
            answer["plugin_release"].as_str().unwrap_or_default()
        );
        out::print_json(&answer, output.pretty);
        return Ok(0);
    }
    let review = client.submit(&body)?;
    let id = review["id"]
        .as_str()
        .context("server returned a review without an id")?
        .to_string();
    eprintln!("review {id}: {}/reviews/{id}", client.base());

    if args.wait {
        wait(client, &id, &args.wait_opts, output)
    } else {
        output.review(client, &review)?;
        Ok(0)
    }
}

/// Long-polls until the review settles. Each poll asks the server for at
/// most `Client::POLL_SECS`; a 204 or a dropped connection (the server
/// restarting) just loops, so a wait survives the app coming and going.
fn wait(client: &Client, id: &str, opts: &WaitOpts, output: Output) -> Result<u8> {
    let deadline = (opts.timeout > 0).then(|| Instant::now() + Duration::from_secs(opts.timeout));
    let mut last_error = String::new();

    loop {
        let remaining = match deadline {
            Some(d) => {
                let left = d.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    eprintln!(
                        "pinrail: timed out after {}s, review {id} is still pending",
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
                        eprintln!("pinrail: decision written to {}", path.display());
                    }
                    if let Some(counts) = out::editorial_counts(&review["decision"]["data"]) {
                        eprintln!("pinrail: {counts}");
                    }
                }
                output.review(client, &review)?;
                return Ok(match status {
                    "decided" => 0,
                    "discarded" => {
                        // the person's "no, and stop": say so, with their reason
                        let by = review["discarded_by"].as_str().unwrap_or("the reviewer");
                        match review["discarded_reason"].as_str() {
                            Some(reason) => eprintln!(
                                "pinrail: review {id} was discarded by {by}: {reason}. Stop the work it was gating."
                            ),
                            None => eprintln!(
                                "pinrail: review {id} was discarded by {by}. Stop the work it was gating."
                            ),
                        }
                        EXIT_DISCARDED
                    }
                    _ => {
                        eprintln!("pinrail: review {id} was {status}, not decided");
                        EXIT_CLOSED
                    }
                });
            }
            Ok(None) => continue,
            Err(err) if err.downcast_ref::<ApiError>().is_some() => return Err(err),
            Err(err) => {
                let message = format!("{err:#}");
                if message != last_error {
                    eprintln!("pinrail: {message}; retrying until the server is back");
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
