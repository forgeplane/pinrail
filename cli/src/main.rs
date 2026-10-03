//! `pinrail`: the CLI agents call. It talks HTTP to the running server and
//! does nothing itself. Markdown on stdout by default and JSON with --json,
//! diagnostics on stderr, and meaningful exit codes, which [`EXITS`] lists
//! for the help and the docs alike.

mod agent;
mod api;
mod attachments;
mod briefs;
mod describe;
#[cfg(feature = "docs")]
mod docs;
mod md;
mod origin;
mod out;
mod scaffold;
mod server;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Args, FromArgMatches, Parser, Subcommand};
use serde_json::{Value, json};

use api::{ApiError, Client, InstallRequest};

pub const EXIT_ERROR: u8 = 1;
pub const EXIT_REFUSED: u8 = 2;
pub const EXIT_CLOSED: u8 = 3;
pub const EXIT_TIMEOUT: u8 = 4;
/// The person said no, and stop; the reason, if any, is in the envelope.
pub const EXIT_DISCARDED: u8 = 5;

/// Every way a command ends, in the order a reader looks them up: the one
/// list the help and the docs' CLI reference print.
pub const EXITS: &[(u8, &str)] = &[
    (
        0,
        "Done. For `wait` and `submit --wait`, the review was decided.",
    ),
    (
        EXIT_ERROR,
        "Error: bad arguments, the app could not be reached, a file could not be read or written, or the app failed on its side. `wait` and `submit --wait` keep waiting through an error inside the app until it answers again.",
    ),
    (
        EXIT_REFUSED,
        "The app refused the request, for example a payload the plugin's schema rejects, or a folder that `plugins check` would not install. Why is on stderr.",
    ),
    (
        EXIT_CLOSED,
        "The review was withdrawn by the agent, or expired, before anyone decided.",
    ),
    (
        EXIT_TIMEOUT,
        "`--timeout` ran out. The review is still pending.",
    ),
    (
        EXIT_DISCARDED,
        "The person discarded the review: stop the work it was gating, and do not ask again.",
    ),
];

/// The limit on waiting that PINRAIL_TIMEOUT sets, in seconds, when set.
fn timeout_from_env() -> Option<u64> {
    std::env::var("PINRAIL_TIMEOUT").ok()?.trim().parse().ok()
}

/// The exit codes as the help lists them: plain text, without the
/// markdown code marks the docs keep.
fn exit_codes() -> String {
    let mut text = String::from("Exit codes:");
    for (code, meaning) in EXITS {
        text.push_str(&format!("\n  {code}  {}", meaning.replace('`', "")));
    }
    text
}

#[derive(Parser)]
#[command(
    name = "pinrail",
    version,
    about,
    long_about = None
)]
struct Cli {
    /// The server's URL [default: PINRAIL_URL, then the address in the running server's server.json, then http://127.0.0.1:4747]
    #[arg(long, global = true, env = "PINRAIL_URL", hide = true)]
    url: Option<String>,

    /// Indent the JSON output. Requires --json
    #[arg(long, global = true, requires = "json", hide = true)]
    pretty: bool,

    /// Print JSON instead of Markdown, for a script or tool that processes
    /// the result. PINRAIL_JSON=1 sets this for a whole session
    #[arg(long, global = true, env = "PINRAIL_JSON", value_parser = clap::builder::BoolishValueParser::new(), hide = true)]
    json: bool,

    /// Also report each step on stderr, such as the origin read from git,
    /// the server started, the files uploaded and where files were written
    #[arg(short, long, global = true, env = "PINRAIL_VERBOSE", value_parser = clap::builder::BoolishValueParser::new(), hide = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

/// How results are printed: markdown by default, for an agent reading
/// them; JSON with --json, as the API gives them.
#[derive(Clone, Copy)]
struct Output {
    pretty: bool,
    markdown: bool,
}

impl Output {
    /// JSON with --json; otherwise the markdown `render` makes of it.
    fn data(&self, value: &serde_json::Value, render: impl FnOnce(&serde_json::Value) -> String) {
        if self.markdown {
            print!("{}", out::terminal_safe(&render(value)));
        } else {
            out::print_json(value, self.pretty);
        }
    }

    fn review(&self, client: &Client, review: &serde_json::Value) -> Result<()> {
        if self.markdown
            && let Some(id) = review["id"].as_str()
        {
            match client.review_markdown(id) {
                Ok(markdown) => println!("{}", out::terminal_safe(markdown.trim_end())),
                // the review is already here: the exit code must still say
                // how it ended, so print what it holds
                Err(err) => {
                    eprintln!(
                        "pinrail: the review's markdown is unavailable ({err:#}); printing it as it came"
                    );
                    println!(
                        "Review {id}: {}",
                        review["status"].as_str().unwrap_or("unknown")
                    );
                    if !review["decision"]["data"].is_null() {
                        println!(
                            "\nDecision:\n\n```json\n{}\n```",
                            serde_json::to_string_pretty(&review["decision"]["data"])?
                        );
                    }
                }
            }
            // a review still waiting: how to wait on it
            if review["status"] == "pending" {
                println!("\nWait for the decision: pinrail wait {id}");
            }
        } else {
            out::print_json(review, self.pretty);
        }
        Ok(())
    }
}

#[derive(Subcommand)]
enum Command {
    /// Submit a review, and with --wait, wait for the decision and print it
    ///
    /// The options build the review: the plugin, --title, and the payload
    /// from --data. Alternatively, give the whole request as one JSON file
    /// with --request, in the form the API takes:
    ///
    ///     {
    ///       "plugin": "list",
    ///       "title": "Sentry triage",
    ///       "origin": {"repo": "acme"},
    ///       "payload": {"groups": []}
    ///     }
    ///
    /// The keys are plugin, title, payload, origin, revises, expires_at and
    /// requested_by. Options override the file's keys, and
    /// --data replaces its payload, so you can send a new round with the
    /// same file, --revises and the id of the earlier round. You can leave
    /// out the plugin argument when the file names one.
    ///
    /// To send files with the payload, use --attach, for a plugin that
    /// accepts files (pinrail plugins shows which plugins do). The payload
    /// refers to each file as {"$attachment": "<name>"}. The name is the
    /// file's own name, or the name given after =:
    ///
    ///     pinrail submit model --data models.json \
    ///       --attach out/pivot.glb --attach out/v2.glb=column.glb
    ///
    /// In a --request file, list the files under "attachments", such as
    /// {"pivot.glb": "out/pivot.glb"}, with paths relative to the file. An
    /// entry can also be {"path": …, "media_type": …}, as in a plugin's
    /// fixtures, so you can send a fixture unchanged. Pinrail checks the
    /// submission before it uploads anything, and it does not upload a file
    /// that the app already has.
    #[command(verbatim_doc_comment)]
    #[command(after_help = exit_codes())]
    Submit(SubmitArgs),
    /// Wait until a review ends, then print its decision as Markdown, or the
    /// whole review as JSON with --json
    #[command(after_help = exit_codes())]
    Wait(WaitArgs),
    /// Print a review as Markdown, or the whole review as JSON with --json,
    /// including its payload and decision
    Show {
        /// The review's id
        id: String,
    },
    /// List every round of a review, oldest first, without payloads
    Rounds {
        /// The review's id
        id: String,
    },
    /// Print a review's event log
    Events {
        /// The review's id
        id: String,
    },
    /// List reviews, newest first, without payloads
    ///
    /// By default, the command lists only pending reviews and, in a git
    /// checkout, only those of its project. The project is named the way
    /// submit names it: the remote's owner/name, or the folder's name when
    /// there is no remote. Outside a git checkout, the command lists the
    /// pending reviews of every project. Use --status and --repo to choose
    /// other reviews, and --all to list every review.
    List(ListArgs),
    /// Record a decision on behalf of the person, from a test or a tool
    ///
    /// An agent never decides a review that it submitted. Only the person
    /// does.
    Decide(DecideArgs),
    /// Withdraw a pending review. A command that is waiting on it exits with 3
    Withdraw {
        /// The review's id
        id: String,
        /// Why the review is no longer needed
        #[arg(long)]
        reason: Option<String>,
    },
    /// Discard a pending review on behalf of the person
    ///
    /// This does what discarding does in the app. A command that is waiting
    /// on the review exits with 5 and tells the agent to stop. An agent never
    /// discards a review that it submitted.
    Discard {
        /// The review's id
        id: String,
        /// The reason, which the agent receives
        #[arg(long)]
        reason: Option<String>,
        /// Who discards the review [default: the server's user]
        #[arg(long)]
        by: Option<String>,
    },
    /// List the installed plugins and when to use each one
    ///
    /// The listing includes each plugin's install record and settings.
    /// `pinrail plugins describe <name>` shows one plugin in full.
    Plugins(PluginsArgs),
    /// List the files a review carries, or save one of them
    #[command(subcommand)]
    Attachments(AttachmentsCommand),
    /// Write every review as a JSON file in a directory
    Export {
        /// The directory to write to. It is created if it does not exist
        dir: PathBuf,
    },
    /// Start the server if it is not running, and print its URL
    Serve,
    /// Print the briefs on using Pinrail as an agent
    ///
    /// Without a path, the command prints the main brief and a menu of the
    /// other briefs. With a path, such as `pinrail docs plugins`, it prints
    /// that brief.
    Docs {
        /// The brief to print, as the menu names it [default: the main brief]
        path: Option<String>,
        /// Print the path and subject of every brief, as a tree
        #[arg(long, conflicts_with = "path")]
        tree: bool,
    },
    /// Open a review in the app, or its preview in a browser with --browser
    ///
    /// The command opens the link pinrail://reviews/<id> with the system's
    /// default handler, which passes it to the Pinrail app, and the app
    /// shows the review. The command prints the address it opened. An id
    /// that the app does not have is refused with exit code 2, and nothing
    /// opens.
    ///
    /// --browser opens <server>/preview/reviews/<id> instead. The preview
    /// shows the review in the plugin's view, served by the running app to
    /// this computer only. Use it to try a view while you build a plugin.
    /// Its hand-over button checks the decision against the plugin's schema
    /// and shows it, but decides nothing. Only the person decides, in the
    /// app.
    Open {
        /// The review's id
        id: String,
        /// Open the preview in the default browser, to try a view while building a plugin
        #[arg(long)]
        browser: bool,
    },
}

#[derive(Args)]
struct SubmitArgs {
    /// The plugin that defines this kind of review, such as review
    #[arg(required_unless_present = "request")]
    plugin: Option<String>,
    /// The title the inbox shows for the review
    #[arg(long, required_unless_present_any = ["request", "sample"])]
    title: Option<String>,
    /// The whole request as JSON, given inline, as a file path, or as - for
    /// stdin. Other options override its keys
    #[arg(long, value_name = "JSON|FILE|-")]
    request: Option<String>,
    /// Where the review comes from, as comma-separated key=value pairs. The
    /// keys are repo (the project, as owner/name), ref (a branch or pull
    /// request), workflow and run_id (what asked for the review), and url (a
    /// link back, which may contain commas). In a git checkout, repo defaults
    /// to the remote, and ref defaults to the branch when the repo is the
    /// checkout's own. For example: repo=acme/api,ref=42,url=…
    #[arg(long, value_parser = origin::parse)]
    origin: Option<BTreeMap<String, String>>,
    /// The payload as JSON, given inline, as a file path, or as - for stdin
    #[arg(long, value_name = "JSON|FILE|-")]
    data: Option<String>,
    /// A file to send with the payload. The payload refers to it as
    /// {"$attachment": "<name>"}, where the name is the file's own name
    /// unless another name follows =. Repeat the option for more files
    #[arg(long = "attach", value_name = "PATH[=NAME]", value_parser = attachments::parse_flag)]
    attachments: Vec<(String, PathBuf)>,
    /// The id of the review that this one is a new round of
    #[arg(long)]
    revises: Option<String>,
    /// The time after which the review expires, as an ISO 8601 timestamp
    #[arg(long)]
    expires_at: Option<String>,
    /// Who is asking, shown on the review as its requester, with the
    /// agent's icon when the app knows the agent [default: the coding agent
    /// the command runs under, detected from the variables it sets
    /// (claude-code, codex, cursor, gemini-cli, opencode, kimi), or else
    /// pinrail-cli]
    #[arg(long, env = "PINRAIL_REQUESTED_BY")]
    requested_by: Option<String>,
    /// Send one of the plugin's sample reviews, which show what the plugin
    /// looks like, instead of a payload: the one named, or else its first.
    /// --title and --origin still apply
    #[arg(long, value_name = "NAME", num_args = 0..=1, default_missing_value = "", conflicts_with_all = ["request", "data", "attachments", "revises", "expires_at", "dry_run"])]
    sample: Option<String>,
    /// Wait until the review ends, whether or not it is decided (see wait)
    #[arg(long)]
    wait: bool,
    /// Run every check on the submission without creating a review. The
    /// command exits with 0 if the review would be accepted, and with 2 and
    /// the violations if not
    #[arg(long, conflicts_with = "wait")]
    dry_run: bool,
    /// With --wait, stop waiting after this many seconds and exit with 4. 0
    /// waits indefinitely. PINRAIL_TIMEOUT sets this for every wait
    #[arg(long, requires = "wait")]
    timeout: Option<u64>,
    /// With --wait, also write decision.data to this file as JSON once the
    /// review is decided. pinrail show <id> shows the decision at any time
    #[arg(long, value_name = "FILE", requires = "wait")]
    decision_out: Option<PathBuf>,
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
    /// Stop waiting after this many seconds and exit with 4. 0 waits
    /// indefinitely. PINRAIL_TIMEOUT sets this for every wait
    #[arg(long)]
    timeout: Option<u64>,
    /// Also write decision.data to this file as JSON once the review is
    /// decided. pinrail show <id> shows the decision at any time
    #[arg(long, value_name = "FILE")]
    decision_out: Option<PathBuf>,
}

#[derive(Args)]
struct ListArgs {
    /// The status to list: pending, decided, withdrawn, discarded or expired.
    /// Separate several with commas [default: pending, unless --all]
    #[arg(long)]
    status: Option<String>,
    /// The project, as the origin's repo. Use - for reviews that name no
    /// project [default: the git checkout's project, unless --all]
    #[arg(long)]
    repo: Option<String>,
    /// The workflow that asked for the review, as the origin's workflow
    #[arg(long)]
    workflow: Option<String>,
    /// The branch, pull request or other ref, as the origin's ref
    #[arg(long = "ref")]
    reference: Option<String>,
    /// The run that asked for the review, as the origin's run_id
    #[arg(long)]
    run_id: Option<String>,
    /// List only reviews of this plugin
    #[arg(long)]
    plugin: Option<String>,
    /// Words that must all appear in the title, payload, plugin, requester,
    /// origin or the name of the person who decided
    #[arg(long = "search", value_name = "WORDS")]
    q: Option<String>,
    /// List only reviews older than this id
    #[arg(long)]
    cursor: Option<String>,
    /// The most reviews to list. Use --cursor to list the next page
    #[arg(long)]
    limit: Option<u32>,
    /// List every review, of any status and project unless --status or
    /// --repo limits them, and every page unless --limit caps the number
    #[arg(long)]
    all: bool,
    /// Include rounds that a later round revises
    #[arg(long)]
    include_revised: bool,
}

#[derive(Args)]
struct DecideArgs {
    /// The review's id
    id: String,
    /// The decision as JSON, given inline, as a file path, or as - for stdin
    #[arg(long, value_name = "JSON|FILE|-")]
    data: String,
    /// A note to the agent that requested the review
    #[arg(long)]
    note: Option<String>,
}

#[derive(Subcommand)]
enum AttachmentsCommand {
    /// List the files a review carries, with their names, sizes, media types
    /// and hashes
    List {
        /// The review's id
        id: String,
    },
    /// Save a file that a review carries
    Get {
        /// The review's id
        id: String,
        /// The file's name on the review
        name: String,
        /// The path to write the file to, or - for stdout [default: the
        /// file's name, in the current directory]
        #[arg(short, long, value_name = "PATH|-")]
        output: Option<String>,
        /// Replace an existing file
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
    /// Install a plugin from a folder or a zip
    ///
    /// Installing again, from the same place or another, replaces the
    /// installed plugin: this is how a plugin is upgraded.
    Install {
        /// The plugin's folder, or a zip of it
        source: String,
        /// Serve the folder directly instead of copying it, while you develop
        /// the plugin
        #[arg(long)]
        link: bool,
        /// With --link: take the place of this installed plugin, by its full
        /// name such as forgeplane/review, so its reviews render with the
        /// folder; removing the link puts the plugin back
        #[arg(long, value_name = "PLUGIN", requires = "link")]
        replace: Option<String>,
    },
    /// Remove an installed plugin. Lines that existing reviews still render
    /// with are kept
    Remove {
        /// The plugin's name
        name: String,
    },
    /// Show what an agent needs to submit a review with a plugin
    ///
    /// The description includes the payload schema, an example payload and
    /// the files the plugin accepts. With --json, it also includes the
    /// decision schema. `pinrail plugins` lists the plugins and says when to
    /// use each one.
    Describe {
        /// The plugin's name
        name: String,
        /// Print only the payload's JSON schema, for a tool that checks or
        /// builds payloads
        #[arg(long, group = "part")]
        payload_schema: bool,
        /// Print only the example payload, as a starting point for your own
        #[arg(long, group = "part")]
        example: bool,
        /// Print only the decision's JSON schema, which describes
        /// decision.data, for processing the decision with --json. Without
        /// --json, the decision is printed as Markdown
        #[arg(long, group = "part")]
        decision_schema: bool,
    },
    /// Read the installed plugins from disk again
    ///
    /// Run this after you change the manifest or schemas of a linked plugin.
    /// Changes to a linked plugin's view take effect without a reload.
    Reload,
    /// Create a new plugin that needs no build step
    ///
    /// The new folder contains a manifest, schemas, a sample review, a view,
    /// the SDK's types and an AGENTS.md file. --link installs the plugin as
    /// a link right away.
    New {
        /// The plugin's name, which starts with a lowercase letter followed by
        /// letters, digits, _ or -
        name: String,
        /// The folder to write the plugin to [default: ./<name>]
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Install the plugin as a link after writing it, so the app serves the
        /// folder directly
        #[arg(long)]
        link: bool,
    },
    /// Check a plugin folder without installing it
    ///
    /// The command reports why the app would refuse the folder, and each
    /// feature the app would drop. It exits with 0 if the app would accept
    /// the plugin, and with 2 if not.
    Check {
        /// The plugin's folder
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The release before this one, as a folder: also report what this
        /// release's schemas no longer accept of what that one took, which
        /// semantic versioning allows only in a version that announces it
        #[arg(long, value_name = "DIR")]
        since: Option<PathBuf>,
    },
}

/// What a submit or a wait exits with: what an agent acts on.
/// The CLI as clap parses it. The flags every command takes are hidden
/// where they are defined, so no command's help repeats them, as git's
/// don't; the root's help lists them once, from their definitions.
fn command() -> clap::Command {
    let root = <Cli as clap::CommandFactory>::command();
    let mut listed = String::from("Every command takes:\n");
    for arg in root.get_arguments().filter(|a| a.is_global_set()) {
        let long = arg.get_long().map(|l| format!("--{l}")).unwrap_or_default();
        let name = match arg.get_short() {
            Some(short) => format!("-{short}, {long}"),
            None => format!("    {long}"),
        };
        let value = if arg.get_action().takes_values() {
            format!(" <{}>", arg.get_id().as_str().to_uppercase())
        } else {
            String::new()
        };
        let mut help = arg.get_help().map(ToString::to_string).unwrap_or_default();
        if let Some(env) = arg.get_env() {
            help.push_str(&format!(" [env: {}]", env.to_string_lossy()));
        }
        listed.push_str(&format!("  {name}{value}\n          {help}\n"));
    }
    // every command's help names them too, since an agent reads only the
    // help of the command it runs
    let globals: Vec<String> = root
        .get_arguments()
        .filter(|a| a.is_global_set())
        .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
        .collect();
    let footer = format!(
        "Every command also takes {} (see pinrail --help).",
        globals.join(", ")
    );
    with_footer(root, &footer).after_help(format!(
        "{listed}\n{}\n\nHow to use Pinrail as an agent: pinrail docs",
        exit_codes()
    ))
}

/// Adds `footer` to the help of every command under `command`, after any
/// help of its own.
fn with_footer(mut command: clap::Command, footer: &str) -> clap::Command {
    let names: Vec<String> = command
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in names {
        command = command.mut_subcommand(name, |sub| {
            let text = match sub.get_after_help() {
                Some(own) => format!("{own}\n\n{footer}"),
                None => footer.to_string(),
            };
            with_footer(sub, footer).after_help(text)
        });
    }
    command
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
    let parsed = command()
        .try_get_matches()
        .and_then(|matches| Cli::from_arg_matches(&matches));
    let cli = match parsed {
        Ok(cli) => cli,
        // help and version are answers; anything else is bad arguments,
        // exit 1 as the exit codes say, not clap's 2, which means refused
        Err(err) => {
            let _ = err.print();
            return ExitCode::from(if err.use_stderr() { EXIT_ERROR } else { 0 });
        }
    };
    let json = cli.json;
    out::set_verbose(cli.verbose);
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            if let Some(api) = err.downcast_ref::<ApiError>() {
                // a refusal is the request's to fix; a server error is not
                let code = if api.status >= 500 {
                    EXIT_ERROR
                } else {
                    EXIT_REFUSED
                };
                if json {
                    out::error_json(&api.body, api.hint.as_deref(), code);
                } else {
                    eprint!("{}", out::terminal_safe(&md::refusal(&api.body)));
                    if let Some(hint) = &api.hint {
                        eprintln!("{hint}");
                    }
                }
                ExitCode::from(code)
            } else {
                if json {
                    let message = format!("{err:#}");
                    out::error_json(
                        &json!({ "error": "cli", "message": message }),
                        None,
                        EXIT_ERROR,
                    );
                } else {
                    eprintln!("pinrail: {}", out::terminal_safe(&format!("{err:#}")));
                }
                ExitCode::from(EXIT_ERROR)
            }
        }
    }
}

fn run(cli: Cli) -> Result<u8> {
    let pretty = cli.pretty;
    let output = Output {
        pretty,
        markdown: !cli.json,
    };

    if let Command::Serve = cli.command {
        let info = server::ensure_running(cli.url.as_deref())?;
        output.data(&info, md::server);
        return Ok(0);
    }

    // a new plugin is written here; only --link needs the app
    if let Command::Plugins(PluginsArgs {
        command: Some(PluginsCommand::New { name, dir, link }),
    }) = &cli.command
    {
        let dir = dir.clone().unwrap_or_else(|| PathBuf::from(name));
        scaffold::write(name, &dir)?;
        // the folder as it was named, for the prompt a person pastes
        let given = dir.display().to_string();
        let dir = dir.canonicalize()?;
        if *link {
            let base = server::resolve_url(cli.url.as_deref(), true)?;
            Client::new(&base).plugins_install(&InstallRequest {
                source: &dir.to_string_lossy(),
                link: true,
                ..InstallRequest::default()
            })?;
        }
        // what comes next, for whoever ran it, most often an agent
        let mut next = vec![
            format!(
                "Read AGENTS.md in {given}, then make the plugin what is needed: the decision schema first, around the action you will take on the answer, then the payload schema, example, sample and view, kept in step."
            ),
            format!("pinrail plugins check {given}"),
        ];
        if !*link {
            next.push(format!(
                "pinrail plugins install {given} --link, so the app serves it live."
            ));
        }
        next.push(format!(
            "pinrail submit {name} --sample, a real review in the person's inbox, then pinrail open <id> --browser, the preview: the view in a browser, whose hand-over checks the decision. pinrail withdraw <id> when done."
        ));
        let written = json!({
            "name": name,
            "dir": dir.to_string_lossy(),
            "linked": link,
            "next": next,
            "docs": "pinrail docs plugins/building",
        });
        output.data(&written, md::scaffolded);
        return Ok(0);
    }

    // the briefs are in the command itself: no server needed
    if let Command::Docs { path, tree } = &cli.command {
        if *tree {
            print!("{}", briefs::tree());
            return Ok(0);
        }
        let Some(brief) = briefs::find(path.as_deref()) else {
            anyhow::bail!("{}", briefs::not_found(path.as_deref().unwrap_or_default()));
        };
        if cli.json {
            out::print_json(&briefs::to_json(&brief), pretty);
        } else {
            print!("{}", briefs::render(&brief));
        }
        return Ok(0);
    }

    let auto_start = match &cli.command {
        Command::Submit(args) => !args.no_start,
        Command::Plugins(args) => matches!(
            args.command,
            None | Some(PluginsCommand::Describe { .. } | PluginsCommand::Check { .. })
        ),
        _ => false,
    };
    let base = server::resolve_url(cli.url.as_deref(), auto_start)?;
    let client = Client::new(&base);

    match cli.command {
        Command::Submit(args) => submit(&client, args, output),
        Command::Wait(args) => wait(&client, &args.id, &args.opts, false, output),
        Command::Show { id } => {
            output.review(&client, &client.get_review(&id)?)?;
            Ok(0)
        }
        Command::Attachments(AttachmentsCommand::List { id }) => {
            let review = client.get_review(&id)?;
            output.data(&review["attachments"], md::attachments);
            Ok(0)
        }
        Command::Attachments(AttachmentsCommand::Get {
            id,
            name,
            output: to,
            force,
        }) => {
            let to = to.unwrap_or_else(|| name.clone());
            // a folder takes the file under its own name
            let to = if to != "-" && Path::new(&to).is_dir() {
                let own = Path::new(&name)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| name.clone());
                Path::new(&to).join(own).display().to_string()
            } else {
                to
            };
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
                        // without --force, a file that appeared meanwhile
                        // is kept: a link fails where one is there already
                        let placed = if force {
                            std::fs::rename(&partial, &path)
                        } else {
                            std::fs::hard_link(&partial, &path)
                                .and_then(|()| std::fs::remove_file(&partial))
                        };
                        if let Err(error) = placed {
                            let _ = std::fs::remove_file(&partial);
                            return Err(anyhow::Error::new(error).context(format!("writing {to}")));
                        }
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
            output.data(&client.rounds(&id)?, md::rounds);
            Ok(0)
        }
        Command::Events { id } => {
            output.data(&client.events(&id)?, md::events);
            Ok(0)
        }
        Command::List(args) => {
            // what an agent means by "the reviews": the pending ones of the
            // project it works in; --all for every one
            let status = args
                .status
                .clone()
                .or_else(|| (!args.all).then(|| "pending".into()));
            let repo = args
                .repo
                .clone()
                .or_else(|| (!args.all).then(origin::repo).flatten());
            let scope = md::Scope {
                status: status.clone(),
                repo: repo.clone(),
                narrowed: !args.all
                    && (args.status.is_none() || (args.repo.is_none() && repo.is_some())),
            };
            let mut query: Vec<(&str, String)> = Vec::new();
            for (k, v) in [
                ("status", status),
                ("repo", repo),
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
            // only the reviews array; md::listing shows the paging
            let mut listing = client.list(&query)?;
            if !args.all || args.limit.is_some() {
                output.data(&listing["reviews"], |rows| {
                    md::listing(rows, &listing, &scope)
                });
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
            let all = Value::Array(reviews);
            output.data(&all, |rows| {
                md::listing(
                    rows,
                    &json!({ "total": rows.as_array().map_or(0, Vec::len) }),
                    &scope,
                )
            });
            Ok(0)
        }
        Command::Decide(args) => {
            let data = read_json_arg(&args.data)?;
            let review = client.decide(&args.id, data, args.note)?;
            output.review(&client, &review)?;
            Ok(0)
        }
        Command::Withdraw { id, reason } => {
            output.review(&client, &client.withdraw(&id, reason)?)?;
            Ok(0)
        }
        Command::Discard { id, reason, by } => {
            output.review(&client, &client.discard(&id, reason, by)?)?;
            Ok(0)
        }
        Command::Plugins(PluginsArgs {
            command: Some(PluginsCommand::Check { dir, since }),
        }) => {
            let dir = dir
                .canonicalize()
                .with_context(|| format!("{} is not a folder here", dir.display()))?;
            // the format's own check, the one the app loads plugins with
            let mut verdict = pinrail_format::Plugin::check(&dir);
            if let Some(since) = since {
                let since = since
                    .canonicalize()
                    .with_context(|| format!("{} is not a folder here", since.display()))?;
                verdict["since"] = pinrail_format::compat::since(&since, &dir);
            }
            // a break that the version does not announce
            let breaks_line = verdict["since"]["claims_compatible"] == true
                && verdict["since"]["breaks"]
                    .as_array()
                    .is_some_and(|b| !b.is_empty());
            if output.markdown {
                print!(
                    "{}",
                    out::terminal_safe(&describe::verdict(&verdict, &dir.display().to_string()))
                );
            } else {
                out::print_json(&verdict, pretty);
                for w in verdict["warnings"].as_array().into_iter().flatten() {
                    eprintln!(
                        "pinrail: {} dropped: {}",
                        out::terminal_safe(w["key"].as_str().unwrap_or_default()),
                        out::terminal_safe(w["message"].as_str().unwrap_or_default())
                    );
                }
                for p in verdict["problems"].as_array().into_iter().flatten() {
                    eprintln!(
                        "pinrail: refused: {}",
                        out::terminal_safe(p["message"].as_str().unwrap_or_default())
                    );
                }
                if breaks_line {
                    for b in verdict["since"]["breaks"].as_array().into_iter().flatten() {
                        eprintln!(
                            "pinrail: breaks {}: {}: {}",
                            out::terminal_safe(
                                verdict["since"]["previous"].as_str().unwrap_or_default()
                            ),
                            out::terminal_safe(b["path"].as_str().unwrap_or_default()),
                            out::terminal_safe(b["message"].as_str().unwrap_or_default())
                        );
                    }
                }
            }
            Ok(if verdict["usable"] == true && !breaks_line {
                0
            } else {
                EXIT_REFUSED
            })
        }
        Command::Plugins(PluginsArgs {
            command:
                Some(PluginsCommand::Describe {
                    name,
                    payload_schema,
                    example,
                    decision_schema,
                }),
        }) => {
            let described = client
                .plugins_describe(&name)
                .map_err(|err| unusable(&client, &name, err))?;
            // the plugin itself, not a list of one
            let shown = described["plugins"][0].clone();
            // one part alone: JSON whichever way it is printed
            let part = [
                (payload_schema, "payload_schema"),
                (example, "example"),
                (decision_schema, "decision_schema"),
            ]
            .into_iter()
            .find_map(|(asked, key)| asked.then_some(key));
            if let Some(key) = part {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&shown[key]).unwrap_or_default()
                );
                return Ok(0);
            }
            output.data(&shown, describe::markdown);
            Ok(0)
        }
        Command::Plugins(args) => {
            let value = match args.command {
                None if output.markdown => {
                    print!(
                        "{}",
                        out::terminal_safe(&describe::listing(&client.plugins()?))
                    );
                    return Ok(0);
                }
                None => client.plugins()?,
                Some(PluginsCommand::Install {
                    source,
                    link,
                    replace,
                }) => {
                    // a folder or a zip that exists is sent as its full
                    // path, `..` resolved, the way the app records and shows it
                    let source = match std::fs::canonicalize(&source) {
                        Ok(p) if p.is_dir() || p.is_file() => p.to_string_lossy().into_owned(),
                        _ => source,
                    };
                    client.plugins_install(&InstallRequest {
                        source: &source,
                        link,
                        replace: replace.as_deref(),
                    })?
                }
                Some(PluginsCommand::Remove { name }) => client.plugins_remove(&name)?,
                Some(PluginsCommand::Reload) => client.plugins_reload()?,
                Some(
                    PluginsCommand::Describe { .. }
                    | PluginsCommand::Check { .. }
                    | PluginsCommand::New { .. },
                ) => {
                    unreachable!()
                }
            };
            output.data(&value, md::plugins_result);
            Ok(0)
        }
        Command::Export { dir } => {
            let count = out::export(&client, &dir)?;
            eprintln!("pinrail: {count} reviews written to {}", dir.display());
            Ok(0)
        }
        Command::Open { id, browser } => {
            // a review the app does not have is refused here, not opened
            client.get_review(&id)?;
            let url = if browser {
                format!("{base}/preview/reviews/{id}")
            } else {
                format!("pinrail://reviews/{id}")
            };
            server::open_browser(&url)?;
            println!("{url}");
            Ok(0)
        }
        Command::Serve | Command::Docs { .. } => unreachable!(),
    }
}

fn submit(client: &Client, args: SubmitArgs, output: Output) -> Result<u8> {
    if args.request.as_deref() == Some("-") && args.data.as_deref() == Some("-") {
        anyhow::bail!("--request and --data cannot both read stdin");
    }
    if let Some(which) = &args.sample {
        let plugin = args
            .plugin
            .as_deref()
            .context("--sample needs the plugin")?;
        let mut body = json!({});
        if !which.is_empty() {
            body["sample"] = json!(which);
        }
        if let Some(title) = &args.title {
            body["title"] = json!(title);
        }
        if let Some(origin) = &args.origin {
            body["origin"] = json!(origin);
        }
        origin::drop_unknown(&mut body);
        origin::fill_from_git(&mut body);
        body["requested_by"] = json!(requester(args.requested_by.as_deref()));
        let review = client.sample(plugin, &body)?;
        return submitted(client, review, &args, output);
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
    if args.requested_by.is_some() || body.get("requested_by").is_none() {
        body["requested_by"] = json!(requester(args.requested_by.as_deref()));
    }
    if let Some(origin) = &args.origin {
        body["origin"] = json!(origin);
    }
    origin::drop_unknown(&mut body);
    origin::fill_from_git(&mut body);
    if let Some(id) = &args.revises {
        body["revises"] = json!(id);
    }
    if let Some(at) = &args.expires_at {
        body["expires_at"] = json!(at);
    }

    // the files: the request's map of name to path, then the flags
    let request_dir = match args.request.as_deref() {
        Some(spec) if spec != "-" && !is_inline_json(spec) => std::path::Path::new(spec)
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
            client.validate(&body).map_err(|e| schema_hint(e, &body))?;
            attachments::upload(client, &files)?;
        }
    }

    if args.dry_run {
        let answer = client.validate(&body).map_err(|e| schema_hint(e, &body))?;
        output.data(&answer, md::valid);
        return Ok(0);
    }
    let review = client.submit(&body).map_err(|e| schema_hint(e, &body))?;
    submitted(client, review, &args, output)
}

/// A plugin describe could not find: when it is installed but broken, says
/// so and why, and how to see what to fix, rather than that it is not there.
fn unusable(client: &Client, name: &str, err: anyhow::Error) -> anyhow::Error {
    if !err
        .downcast_ref::<ApiError>()
        .is_some_and(|a| a.status == 404)
    {
        return err;
    }
    let listed = client.plugins().ok();
    let Some(plugin) = listed
        .as_ref()
        .and_then(|l| l["plugins"].as_array())
        .and_then(|rows| {
            rows.iter()
                .find(|p| (p["name"] == name || p["plugin"] == name) && p["error"].is_string())
        })
    else {
        return not_installed(err);
    };
    ApiError {
        status: 409,
        body: json!({
            "error": "unusable",
            "message": format!("plugin {name} is installed but can't be used: {}", plugin["error"].as_str().unwrap_or_default()),
            "violations": [],
        }),
        hint: plugin["path"]
            .as_str()
            .map(|path| format!("What to fix: pinrail plugins check {path}")),
    }
    .into()
}

/// A plugin that is not installed: the next step is the list of those that are.
fn not_installed(err: anyhow::Error) -> anyhow::Error {
    match err.downcast::<ApiError>() {
        Ok(mut api) => {
            api.hint = Some(INSTALLED_HINT.into());
            api.into()
        }
        Err(err) => err,
    }
}

/// A payload the plugin refused, with where to read the shape it takes.
fn schema_hint(err: anyhow::Error, body: &Value) -> anyhow::Error {
    let Some(plugin) = body["plugin"].as_str() else {
        return err;
    };
    let mut api = match err.downcast::<ApiError>() {
        Ok(api) => api,
        Err(err) => return err,
    };
    let violations = api.body["violations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let at = |prefix: &str| {
        violations
            .iter()
            .any(|v| v["path"].as_str().is_some_and(|p| p.starts_with(prefix)))
    };
    if at("/plugin") {
        api.hint = Some(INSTALLED_HINT.into());
    } else if at("/payload") {
        api.hint = Some(format!(
            "The payload it takes: pinrail plugins describe {plugin}"
        ));
    }
    api.into()
}

/// Who a review is from: who the flag or PINRAIL_REQUESTED_BY names, else
/// the coding agent the CLI runs under, else the CLI itself.
fn requester(given: Option<&str>) -> String {
    if let Some(by) = given {
        return by.to_string();
    }
    match agent::detect() {
        Some(agent) => {
            out::note(format_args!(
                "requested by {agent}, the agent this runs under"
            ));
            agent
        }
        None => "pinrail-cli".to_string(),
    }
}

/// Says where the new review is, then waits on it or prints it.
fn submitted(client: &Client, review: Value, args: &SubmitArgs, output: Output) -> Result<u8> {
    let id = review["id"]
        .as_str()
        .context("server returned a review without an id")?
        .to_string();
    // said at once, so whoever runs it has the id even while --wait blocks
    if args.wait {
        eprintln!("pinrail: review {id} submitted; waiting for a decision");
    } else {
        eprintln!("pinrail: review {id} submitted");
    }

    if args.wait {
        let opts = WaitOpts {
            timeout: args.timeout,
            decision_out: args.decision_out.clone(),
        };
        // the server just took the review: keep waiting through a restart
        wait(client, &id, &opts, true, output)
    } else {
        output.review(client, &review)?;
        Ok(0)
    }
}

/// Blocks until the review ends, by long polls of at most
/// `Client::POLL_SECS` each. A 204 or a dropped connection, such as the
/// server restarting, just loops, so a wait survives the app coming and
/// going. A server that goes away is waited for once it has answered
/// (`answered`); one that never did is not running.
fn wait(
    client: &Client,
    id: &str,
    opts: &WaitOpts,
    mut answered: bool,
    output: Output,
) -> Result<u8> {
    // --timeout, else PINRAIL_TIMEOUT, else no limit
    let timeout = opts.timeout.or_else(timeout_from_env).unwrap_or(0);
    let deadline = (timeout > 0).then(|| Instant::now() + Duration::from_secs(timeout));
    let mut last_error = String::new();

    loop {
        let remaining = match deadline {
            Some(d) => {
                let left = d.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    eprintln!(
                        "pinrail: timed out after {timeout}s, review {id} is still pending. \
                         To keep waiting, run: pinrail wait {id} --timeout {timeout}"
                    );
                    return Ok(EXIT_TIMEOUT);
                }
                left.as_secs().clamp(1, Client::POLL_SECS)
            }
            None => Client::POLL_SECS,
        };

        let polled = client.wait(id, remaining);
        // a server that answers with an error is running, and may recover
        let server_error = polled
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<ApiError>())
            .map(|e| e.status >= 500);
        answered |= polled.is_ok() || server_error.is_some();
        match polled {
            Ok(Some(review)) => {
                let status = review["status"].as_str().unwrap_or("");
                if status == "pending" {
                    continue;
                }
                if status == "decided"
                    && let Some(path) = &opts.decision_out
                {
                    out::write_decision(path, &review["decision"]["data"])?;
                    out::note(format_args!("decision written to {}", path.display()));
                }
                output.review(client, &review)?;
                return Ok(match status {
                    "decided" => 0,
                    "discarded" => {
                        // the person's "no, and stop": say so, with their reason
                        let by = out::terminal_safe(
                            review["discarded_by"].as_str().unwrap_or("the reviewer"),
                        );
                        match review["discarded_reason"].as_str().map(out::terminal_safe) {
                            Some(reason) => eprintln!(
                                "pinrail: review {id} was discarded by {by}: {reason}. Stop the work it was gating."
                            ),
                            None => eprintln!(
                                "pinrail: review {id} was discarded by {by}. Stop the work it was gating."
                            ),
                        }
                        EXIT_DISCARDED
                    }
                    "withdrawn" | "expired" => {
                        eprintln!("pinrail: review {id} was {status}, not decided");
                        EXIT_CLOSED
                    }
                    // a status a newer app knows and this CLI does not: an
                    // exit code for it would be a guess
                    other => anyhow::bail!(
                        "review {id} ended as {other:?}, which this pinrail does not know; update the pinrail command"
                    ),
                });
            }
            Ok(None) => continue,
            Err(err) if server_error == Some(false) => return Err(err),
            Err(err) if !answered => return Err(err.context(client.unreachable())),
            Err(err) => {
                let message = format!("{err:#}");
                if message != last_error {
                    eprintln!(
                        "pinrail: {}; retrying until the server is back",
                        out::terminal_safe(&message)
                    );
                    last_error = message;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

/// JSON given on the command line itself: what starts as an object or an
/// array and names no file that exists.
fn is_inline_json(spec: &str) -> bool {
    let start = spec.trim_start();
    (start.starts_with('{') || start.starts_with('[')) && !std::path::Path::new(spec).exists()
}

/// Where a refusal about a plugin points: the list of those installed.
const INSTALLED_HINT: &str = "Installed plugins: pinrail plugins";

/// The JSON a flag gives: inline, from a file, or from stdin with `-`.
fn read_json_arg(spec: &str) -> Result<Value> {
    if is_inline_json(spec) {
        return serde_json::from_str(spec).context("the JSON given inline is not valid JSON");
    }
    let text = if spec == "-" {
        std::io::read_to_string(std::io::stdin()).context("reading stdin")?
    } else {
        std::fs::read_to_string(spec).with_context(|| format!("reading {spec}"))?
    };
    serde_json::from_str(&text).with_context(|| format!("{spec} is not valid JSON"))
}
