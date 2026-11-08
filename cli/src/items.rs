//! `pinrail items`: work items, as an agent records, takes and closes them.
//! Each subcommand prints Markdown for an agent, or the app's JSON with
//! --json.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::api::{ApiError, Client, segment};
use crate::{Output, agent, attachments, origin};

#[derive(Args, Debug)]
pub struct ItemsArgs {
    #[command(subcommand)]
    command: ItemsCommand,
}

#[derive(Subcommand, Debug)]
enum ItemsCommand {
    /// Add a work item, or a whole breakdown of them with --file
    ///
    /// An item belongs to the repository of the git checkout, or to the one
    /// --repo names. It prints the item's key, such as PIN-42, which every
    /// other command takes, as does the number alone inside the checkout.
    ///
    ///     pinrail items add "Load themes in the core" \
    ///       --body "Read the theme files when the app starts." \
    ///       --parent PIN-40 --depends-on PIN-41
    ///
    /// A breakdown is a JSON file of items, each with a ref of its own that
    /// the others name it by, as a parent or in depends_on, before it has a
    /// key. Pinrail adds them all or none:
    ///
    ///     {"items": [
    ///       {"ref": "themes", "title": "Custom themes"},
    ///       {"ref": "format", "parent": "themes", "title": "Theme file format"},
    ///       {"ref": "load", "parent": "themes", "title": "Load themes",
    ///        "depends_on": ["format", "PIN-38"]}
    ///     ]}
    #[command(verbatim_doc_comment)]
    Add(AddArgs),
    /// List the work items that are open, in progress or on hold
    List(ListArgs),
    /// Print the next work item that is ready, and claim it with --claim
    ///
    /// Ready means open, not on hold nor under an item on hold, waiting for
    /// no item it depends on, and with no open sub-item. The next is a sub-item of an item the session holds,
    /// if any, then the item that unblocks the most others, then the
    /// oldest. --claim takes it for this session: it is in progress, and
    /// the reviews and questions the session sends belong to it.
    #[command(verbatim_doc_comment)]
    Next(NextArgs),
    /// Print a work item: its description, what it depends on and blocks,
    /// its sub-items, reviews, comments and sessions
    Show(ShowArgs),
    /// Change a work item: its title, description, status, claim, parent,
    /// dependencies, link, repository or files
    Edit(EditArgs),
    /// Comment on a work item
    Comment(CommentArgs),
    /// Mark a work item done, with a comment that says what was done
    ///
    /// An agent can mark an item done only when no review or question of it
    /// waits on the person and its sub-items are done or cancelled; the
    /// command says what is still open otherwise, and exits with 2. It
    /// prints the items that are ready now.
    #[command(verbatim_doc_comment)]
    Done(DoneArgs),
}

/// Where the command works, and who runs it.
#[derive(Args, Debug)]
struct Common {
    /// The repository, outside its git checkout [default: the checkout's]
    #[arg(long)]
    repo: Option<String>,
    /// Who is working [default: the agent the command runs under]
    #[arg(long, env = "PINRAIL_REQUESTED_BY")]
    requested_by: Option<String>,
}

#[derive(Args, Debug)]
struct AddArgs {
    /// The item's title, one line
    #[arg(required_unless_present = "file", conflicts_with = "file")]
    title: Option<String>,
    /// The description, in Markdown: the goal, the context and what done means
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    /// Read the description from a file, or from stdin with -
    #[arg(long, value_name = "FILE")]
    body_file: Option<PathBuf>,
    /// The item it is part of
    #[arg(long, value_name = "ITEM")]
    parent: Option<String>,
    /// Items that must be done first; several, or separated by commas
    #[arg(long, value_name = "ITEM", value_delimiter = ',')]
    depends_on: Vec<String>,
    /// A link, such as the issue elsewhere
    #[arg(long, value_name = "URL")]
    link: Option<String>,
    /// A file the description shows as attachment:NAME
    #[arg(long = "attach", value_name = "PATH[=NAME]", value_parser = attachments::parse_flag)]
    attachments: Vec<(String, PathBuf)>,
    /// A breakdown: a JSON file of items, or - for stdin
    #[arg(long, value_name = "FILE")]
    file: Option<PathBuf>,
    #[command(flatten)]
    common: Common,
}

#[derive(Args, Debug)]
struct ListArgs {
    /// Only the items that are ready
    #[arg(long)]
    ready: bool,
    /// Only the items that wait for an item they depend on
    #[arg(long)]
    blocked: bool,
    /// Only these: open, in_progress, on_hold, done, cancelled, ready or
    /// blocked, separated by commas
    #[arg(long)]
    status: Option<String>,
    /// Only the sub-items of this item
    #[arg(long, value_name = "ITEM")]
    parent: Option<String>,
    /// Words that must all be in the title or the description
    #[arg(long, value_name = "WORDS")]
    search: Option<String>,
    #[command(flatten)]
    common: Common,
}

#[derive(Args, Debug)]
struct NextArgs {
    /// Claim it for this session
    #[arg(long)]
    claim: bool,
    #[command(flatten)]
    common: Common,
}

#[derive(Args, Debug)]
struct ShowArgs {
    /// The item: its key, such as PIN-42, or its number in the checkout
    item: String,
    #[command(flatten)]
    common: Common,
}

#[derive(Args, Debug)]
struct EditArgs {
    /// The item: its key, such as PIN-42, or its number in the checkout
    item: String,
    #[arg(long)]
    title: Option<String>,
    /// The description, in Markdown, which replaces the one it has
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    /// Read the description from a file, or from stdin with -
    #[arg(long, value_name = "FILE")]
    body_file: Option<PathBuf>,
    /// open, in_progress, done or cancelled; an agent cancels with
    /// --comment. Only the person puts an item on hold (on_hold) or takes
    /// it off
    #[arg(long)]
    status: Option<String>,
    /// Claim it for this session
    #[arg(long, conflicts_with = "release")]
    claim: bool,
    /// With --claim, take it from the session that holds it
    #[arg(long, requires = "claim")]
    take: bool,
    /// Give up this session's claim: the item is open again
    #[arg(long)]
    release: bool,
    /// Put it under this item
    #[arg(long, value_name = "ITEM", conflicts_with = "no_parent")]
    parent: Option<String>,
    /// Take it out from under its parent
    #[arg(long)]
    no_parent: bool,
    /// Items it must wait for; several, or separated by commas
    #[arg(long, value_name = "ITEM", value_delimiter = ',')]
    depends_on: Vec<String>,
    /// Items it no longer waits for
    #[arg(long, value_name = "ITEM", value_delimiter = ',')]
    no_depends_on: Vec<String>,
    #[arg(long, value_name = "URL", conflicts_with = "no_link")]
    link: Option<String>,
    /// Remove its link
    #[arg(long)]
    no_link: bool,
    /// Move it to another repository, where it takes the next number; its
    /// old key still finds it
    #[arg(long, value_name = "REPO")]
    move_to: Option<String>,
    /// A file the description shows as attachment:NAME
    #[arg(long = "attach", value_name = "PATH[=NAME]", value_parser = attachments::parse_flag)]
    attachments: Vec<(String, PathBuf)>,
    /// Remove a file from the description, by name
    #[arg(long, value_name = "NAME")]
    detach: Vec<String>,
    /// A comment that goes with the change
    #[arg(long)]
    comment: Option<String>,
    #[command(flatten)]
    common: Common,
}

#[derive(Args, Debug)]
struct CommentArgs {
    /// The item: its key, such as PIN-42, or its number in the checkout
    item: String,
    /// The comment, in Markdown
    #[arg(required_unless_present = "file", conflicts_with = "file")]
    text: Option<String>,
    /// Read the comment from a file, or from stdin with -
    #[arg(long, value_name = "FILE")]
    file: Option<PathBuf>,
    /// A file the comment shows as attachment:NAME
    #[arg(long = "attach", value_name = "PATH[=NAME]", value_parser = attachments::parse_flag)]
    attachments: Vec<(String, PathBuf)>,
    #[command(flatten)]
    common: Common,
}

#[derive(Args, Debug)]
struct DoneArgs {
    /// The item: its key, such as PIN-42, or its number in the checkout
    item: String,
    /// What was done and where, for whoever continues from it
    #[arg(long)]
    comment: Option<String>,
    #[command(flatten)]
    common: Common,
}

/// The context of a command: the repository it works in, and who runs it.
struct Scope {
    repo: Option<String>,
    by: String,
    session: Option<String>,
}

impl Scope {
    fn of(common: &Common) -> Self {
        Scope {
            repo: common.repo.clone().or_else(origin::repo),
            by: common
                .requested_by
                .clone()
                .or_else(agent::detect)
                .unwrap_or_else(|| "pinrail-cli".to_string()),
            session: agent::session(),
        }
    }

    /// The repository, which adding an item and taking the next one need.
    fn repo(&self) -> Result<&str> {
        self.repo.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "this is not a git checkout: name the repository with --repo, such as --repo forgeplane/pinrail"
            )
        })
    }

    /// The body with who asks, and in which session.
    fn sign(&self, mut body: Value) -> Value {
        body["by"] = json!(self.by);
        if let Some(session) = &self.session {
            body["session"] = json!(session);
        }
        body
    }

    /// The item's path, with the repository for a number alone.
    fn path(&self, item: &str, rest: &str) -> String {
        let query = match &self.repo {
            Some(repo) => format!("?repo={}", segment(repo)),
            None => String::new(),
        };
        format!("/api/v1/items/{}{rest}{query}", segment(item.trim()))
    }
}

pub fn run(client: &Client, args: ItemsArgs, output: Output) -> Result<u8> {
    match args.command {
        ItemsCommand::Add(args) => add(client, args, output),
        ItemsCommand::List(args) => list(client, args, output),
        ItemsCommand::Next(args) => next(client, args, output),
        ItemsCommand::Show(args) => {
            let cx = Scope::of(&args.common);
            let item = client.get(&cx.path(&args.item, ""), &[])?;
            let markdown = show(client, &item);
            output.data(&item, |_| markdown);
            Ok(0)
        }
        ItemsCommand::Edit(args) => edit(client, args, output),
        ItemsCommand::Comment(args) => comment(client, args, output),
        ItemsCommand::Done(args) => done(client, args, output),
    }
}

/// Text from a file, or from stdin for -.
fn read_text(path: &Path) -> Result<String> {
    if path == Path::new("-") {
        return std::io::read_to_string(std::io::stdin()).context("reading stdin");
    }
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

/// The files given with --attach, uploaded unless the app has them, as the
/// `attachments` a request declares them by.
fn send_files(client: &Client, flags: &[(String, PathBuf)]) -> Result<Option<Value>> {
    let files = attachments::read(&attachments::collect(None, Path::new(""), flags)?)?;
    if files.is_empty() {
        return Ok(None);
    }
    attachments::upload(client, &files)?;
    Ok(Some(attachments::declare(&files)))
}

fn add(client: &Client, args: AddArgs, output: Output) -> Result<u8> {
    let cx = Scope::of(&args.common);
    let repo = cx.repo()?.to_string();
    if let Some(file) = &args.file {
        let text = read_text(file)?;
        let plan: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} is not JSON", file.display()))?;
        let mut body =
            json!({"repo": repo, "items": plan.get("items").cloned().unwrap_or(plan.clone())});
        // --parent, or the file's own, puts every top item under one
        let parent = args.parent.clone().or_else(|| {
            plan.get("parent")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        if let Some(parent) = parent {
            body["parent"] = json!(parent);
        }
        let added = client.post("/api/v1/items", Some(&cx.sign(body)))?;
        output.data(&added, |v| breakdown_md(v, &repo));
        return Ok(0);
    }
    let description = match (&args.body, &args.body_file) {
        (Some(text), _) => text.clone(),
        (None, Some(file)) => read_text(file)?,
        (None, None) => String::new(),
    };
    let mut body = json!({
        "repo": repo,
        "title": args.title.unwrap_or_default(),
        "body": description,
        "depends_on": args.depends_on,
    });
    if let Some(parent) = &args.parent {
        body["parent"] = json!(parent);
    }
    if let Some(link) = &args.link {
        body["link"] = json!(link);
    }
    if let Some(files) = send_files(client, &args.attachments)? {
        body["attachments"] = files;
    }
    let item = client.post("/api/v1/items", Some(&cx.sign(body)))?;
    output.data(&item, |v| {
        format!(
            "Added {} {}.\n",
            v["key"].as_str().unwrap_or_default(),
            v["title"].as_str().unwrap_or_default()
        )
    });
    Ok(0)
}

fn list(client: &Client, args: ListArgs, output: Output) -> Result<u8> {
    let cx = Scope::of(&args.common);
    let mut states: Vec<String> = args
        .status
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if args.ready {
        states.push("ready".into());
    }
    if args.blocked {
        states.push("blocked".into());
    }
    let mut query: Vec<(&str, String)> = Vec::new();
    if !states.is_empty() {
        query.push(("status", states.join(",")));
    }
    if let Some(repo) = &cx.repo {
        query.push(("repo", repo.clone()));
    }
    if let Some(parent) = &args.parent {
        query.push(("parent", parent.clone()));
    }
    if let Some(words) = &args.search {
        query.push(("q", words.clone()));
    }
    let listed = client.get("/api/v1/items", &query)?;
    let everywhere = cx.repo.is_none();
    output.data(&listed, |v| list_md(&v["items"], everywhere));
    Ok(0)
}

fn next(client: &Client, args: NextArgs, output: Output) -> Result<u8> {
    let cx = Scope::of(&args.common);
    let repo = cx.repo()?.to_string();
    let answer = client.post(
        "/api/v1/items/next",
        Some(&cx.sign(json!({"repo": repo, "claim": args.claim}))),
    )?;
    let item = &answer["item"];
    let markdown = if item.is_null() {
        format!("No work item is ready in {repo}.\n")
    } else {
        let mut text = show(client, item);
        if args.claim {
            text.push_str(&format!(
                "\nYou hold {} now: the reviews and questions you send belong to it until you mark it done or release it.\n",
                item["key"].as_str().unwrap_or_default()
            ));
        }
        text
    };
    output.data(&answer, |_| markdown);
    Ok(0)
}

fn edit(client: &Client, args: EditArgs, output: Output) -> Result<u8> {
    let cx = Scope::of(&args.common);
    let mut body = json!({});
    if let Some(title) = &args.title {
        body["title"] = json!(title);
    }
    match (&args.body, &args.body_file) {
        (Some(text), _) => body["body"] = json!(text),
        (None, Some(file)) => body["body"] = json!(read_text(file)?),
        (None, None) => {}
    }
    if let Some(status) = &args.status {
        body["status"] = json!(status);
    }
    if args.claim {
        body["claim"] = json!(if args.take { "take" } else { "claim" });
    }
    if args.release {
        body["claim"] = json!("release");
    }
    if let Some(parent) = &args.parent {
        body["parent"] = json!(parent);
    }
    if args.no_parent {
        body["parent"] = Value::Null;
    }
    if !args.depends_on.is_empty() {
        body["depends_on"] = json!(args.depends_on);
    }
    if !args.no_depends_on.is_empty() {
        body["no_depends_on"] = json!(args.no_depends_on);
    }
    if let Some(link) = &args.link {
        body["link"] = json!(link);
    }
    if args.no_link {
        body["link"] = Value::Null;
    }
    if let Some(to) = &args.move_to {
        body["repo"] = json!(to);
    }
    if !args.detach.is_empty() {
        body["detach"] = json!(args.detach);
    }
    if let Some(comment) = &args.comment {
        body["comment"] = json!(comment);
    }
    if let Some(files) = send_files(client, &args.attachments)? {
        body["attachments"] = files;
    }
    let Value::Object(fields) = &body else {
        unreachable!()
    };
    anyhow::ensure!(
        !fields.is_empty(),
        "say what to change, such as --title, --status or --depends-on; pinrail items edit --help lists them all"
    );
    let edited = client.patch(&cx.path(&args.item, ""), &cx.sign(body))?;
    output.data(&edited, |v| edited_md("Changed", v));
    Ok(0)
}

fn comment(client: &Client, args: CommentArgs, output: Output) -> Result<u8> {
    let cx = Scope::of(&args.common);
    let text = match (&args.text, &args.file) {
        (_, Some(file)) => read_text(file)?,
        (Some(text), None) if text == "-" => read_text(Path::new("-"))?,
        (Some(text), None) => text.clone(),
        (None, None) => String::new(),
    };
    let mut body = json!({"body": text});
    if let Some(files) = send_files(client, &args.attachments)? {
        body["attachments"] = files;
    }
    let comment = client.post(&cx.path(&args.item, "/comments"), Some(&cx.sign(body)))?;
    let key = args.item.clone();
    output.data(&comment, |_| format!("Commented on {key}.\n"));
    Ok(0)
}

fn done(client: &Client, args: DoneArgs, output: Output) -> Result<u8> {
    let cx = Scope::of(&args.common);
    let mut body = json!({});
    if let Some(comment) = &args.comment {
        body["comment"] = json!(comment);
    }
    let answer = client.post(&cx.path(&args.item, "/done"), Some(&cx.sign(body)));
    let edited = match answer {
        Ok(edited) => edited,
        Err(error) => {
            // what to do about a review that still waits
            if let Some(api) = error.downcast_ref::<ApiError>()
                && api.status == 409
                && api.body["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("still waits on the person"))
            {
                return Err(ApiError {
                    status: api.status,
                    body: api.body.clone(),
                    hint: Some(
                        "Wait for each one with pinrail wait <id>, then mark the item done.".into(),
                    ),
                }
                .into());
            }
            return Err(error);
        }
    };
    output.data(&edited, |v| edited_md("Done:", v));
    Ok(0)
}

// --- Markdown

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

/// A status as words: `in progress`.
fn status_words(v: &Value) -> String {
    s(v).replace('_', " ")
}

/// A day, from a time the app gives: `2026-11-05`.
fn day(v: &Value) -> &str {
    s(v).get(..10).unwrap_or_default()
}

fn who(author: &str) -> &str {
    if author == "person" {
        "the person"
    } else {
        author
    }
}

fn list_md(items: &Value, everywhere: bool) -> String {
    let items = items.as_array().map(Vec::as_slice).unwrap_or_default();
    if items.is_empty() {
        return "No work items match.\n".to_string();
    }
    let mut out = String::new();
    for item in items {
        let mut parts = vec![status_words(&item["status"])];
        if let Some(agent) = item["claim"]["agent"].as_str() {
            parts[0] = format!("{} by {agent}", parts[0]);
        }
        if item["ready"] == true {
            parts.push("ready".into());
        }
        let blocked: Vec<&str> = item["blocked_by"]
            .as_array()
            .map(|b| b.iter().map(s).collect())
            .unwrap_or_default();
        if !blocked.is_empty() {
            parts.push(format!("blocked by {}", blocked.join(", ")));
        }
        let total = item["sub_items"]["total"].as_u64().unwrap_or(0);
        if total > 0 {
            parts.push(format!(
                "{} of {total} sub-items closed",
                item["sub_items"]["closed"].as_u64().unwrap_or(0)
            ));
        }
        if let Some(parent) = item["parent"].as_str() {
            parts.push(format!("part of {parent}"));
        }
        if everywhere {
            parts.push(s(&item["repo"]).to_string());
        }
        out.push_str(&format!(
            "- {} {} · {}\n",
            s(&item["key"]),
            s(&item["title"]),
            parts.join(" · ")
        ));
    }
    out
}

fn breakdown_md(added: &Value, repo: &str) -> String {
    let items = added["items"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut out = format!(
        "Added {} work item{} in {repo}:\n",
        items.len(),
        if items.len() == 1 { "" } else { "s" }
    );
    for entry in items {
        let item = &entry["item"];
        let mut line = format!(
            "- {}: {} {}",
            s(&entry["ref"]),
            s(&item["key"]),
            s(&item["title"])
        );
        let blocked: Vec<&str> = item["blocked_by"]
            .as_array()
            .map(|b| b.iter().map(s).collect())
            .unwrap_or_default();
        if !blocked.is_empty() {
            line.push_str(&format!(" · depends on {}", blocked.join(", ")));
        }
        if let Some(parent) = item["parent"].as_str() {
            line.push_str(&format!(" · part of {parent}"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn edited_md(verb: &str, edited: &Value) -> String {
    let item = &edited["item"];
    let mut out = format!("{verb} {} {}.\n", s(&item["key"]), s(&item["title"]));
    for gone in edited["cancelled_with"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        out.push_str(&format!(
            "Cancelled with it: {} {}\n",
            s(&gone["key"]),
            s(&gone["title"])
        ));
    }
    for ready in edited["now_ready"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        out.push_str(&format!(
            "Now ready: {} {}\n",
            s(&ready["key"]),
            s(&ready["title"])
        ));
    }
    if let Some(parent) = edited["parent_left_open"].as_object() {
        out.push_str(&format!(
            "{} {} has no open sub-item left but stays open: mark it done when its work is complete.\n",
            s(&parent["key"]),
            s(&parent["title"])
        ));
    }
    out
}

/// The comment an item was closed with, and who closed it: the comment
/// made with the change of status to done, if any.
fn closing(item: &Value) -> Option<(String, String)> {
    let timeline = item["timeline"].as_array()?;
    let at = timeline
        .iter()
        .rposition(|e| e["kind"] == "status" && e["attrs"]["to"] == "done")?;
    let by = s(&timeline[at]["actor"]).to_string();
    let comment = timeline[at..]
        .iter()
        .find(|e| e["type"] == "comment")
        .map(|c| s(&c["body"]).to_string())
        .unwrap_or_default();
    Some((by, comment))
}

/// An item as an agent reads it before working on it.
pub fn show(client: &Client, item: &Value) -> String {
    let mut out = format!("{} {}\n", s(&item["key"]), s(&item["title"]));
    let mut line = vec![status_words(&item["status"])];
    if let Some(agent) = item["claim"]["agent"].as_str() {
        line[0] = match item["claim"]["session"].as_str() {
            Some(session) => format!("{} · claimed by {agent} (session {session})", line[0]),
            None => format!("{} · claimed by {agent}", line[0]),
        };
    }
    if item["ready"] == true {
        line.push("ready".into());
    }
    line.push(s(&item["repo"]).to_string());
    out.push_str(&line.join(" · "));
    out.push('\n');
    if let Some(parent) = item["parent"].as_str() {
        let title = item["ancestors"]
            .as_array()
            .and_then(|a| a.last())
            .map(|p| s(&p["title"]).to_string())
            .unwrap_or_default();
        let progress = client
            .get(&format!("/api/v1/items/{}", segment(parent)), &[])
            .ok()
            .map(|p| {
                format!(
                    " ({} of {} done)",
                    p["sub_items"]["closed"].as_u64().unwrap_or(0),
                    p["sub_items"]["total"].as_u64().unwrap_or(0)
                )
            })
            .unwrap_or_default();
        out.push_str(&format!("part of {parent} {title}{progress}\n"));
    }
    if let Some(link) = item["link"].as_str() {
        out.push_str(&format!("link: {link}\n"));
    }
    let body = s(&item["body"]).trim();
    if !body.is_empty() {
        out.push_str(&format!("\n## Description\n{body}\n"));
    }
    let files = item["files"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if !files.is_empty() {
        out.push_str("\n## Files\n");
        for f in files {
            out.push_str(&format!("- {} ({} bytes)\n", s(&f["name"]), f["size"]));
        }
    }
    let depends = item["depends_on"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if !depends.is_empty() {
        out.push_str("\n## Depends on\n");
        for d in depends {
            let mut entry = format!(
                "- {} {}, {}",
                s(&d["key"]),
                s(&d["title"]),
                status_words(&d["status"])
            );
            if d["status"] == "done"
                && let Ok(full) =
                    client.get(&format!("/api/v1/items/{}", segment(s(&d["id"]))), &[])
                && let Some((by, comment)) = closing(&full)
            {
                entry = format!(
                    "- {} {}, done by {}",
                    s(&d["key"]),
                    s(&d["title"]),
                    who(&by)
                );
                if !comment.is_empty() {
                    entry.push_str(&format!(": \"{comment}\""));
                }
            }
            if d["status"] == "cancelled" {
                entry.push_str(": the work it was to do will not happen");
            }
            out.push_str(&entry);
            out.push('\n');
        }
    }
    let blocks = item["blocks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if !blocks.is_empty() {
        out.push_str("\n## Blocks\n");
        for b in blocks {
            out.push_str(&format!(
                "- {} {}, {}\n",
                s(&b["key"]),
                s(&b["title"]),
                status_words(&b["status"])
            ));
        }
    }
    let subs = item["sub_items_list"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if !subs.is_empty() {
        out.push_str("\n## Sub-items\n");
        for c in subs {
            out.push_str(&format!(
                "- {} {}, {}\n",
                s(&c["key"]),
                s(&c["title"]),
                status_words(&c["status"])
            ));
        }
    }
    let reviews = item["reviews"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    // how each review ended, from the timeline, which keeps it after a sweep
    let ended: Vec<&Value> = item["timeline"]
        .as_array()
        .map(|t| t.iter().filter(|e| e["kind"] == "review_ended").collect())
        .unwrap_or_default();
    if !reviews.is_empty() || !ended.is_empty() {
        out.push_str("\n## Reviews\n");
        for r in reviews {
            let kind = if r["kind"] == "question" {
                "question".to_string()
            } else {
                s(&r["plugin"]).to_string()
            };
            out.push_str(&format!(
                "- {} \"{}\" ({kind}), {}\n",
                s(&r["id"]),
                s(&r["title"]),
                s(&r["status"])
            ));
        }
        for e in ended {
            let id = s(&e["attrs"]["review"]);
            if reviews.iter().any(|r| r["id"] == id) {
                continue;
            }
            out.push_str(&format!(
                "- {id} \"{}\", {} on {}\n",
                s(&e["attrs"]["title"]),
                s(&e["attrs"]["outcome"]),
                day(&e["at"])
            ));
        }
    }
    let comments: Vec<&Value> = item["timeline"]
        .as_array()
        .map(|t| t.iter().filter(|e| e["type"] == "comment").collect())
        .unwrap_or_default();
    if !comments.is_empty() {
        out.push_str("\n## Comments\n");
        for c in comments {
            out.push_str(&format!(
                "- {}, {}: {}\n",
                who(s(&c["author"])),
                day(&c["created_at"]),
                s(&c["body"]).trim()
            ));
        }
    }
    let sessions = item["sessions"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if !sessions.is_empty() {
        out.push_str("\n## Sessions\n");
        for x in sessions {
            let mut line = format!("- {} {}", s(&x["agent"]), s(&x["session"]));
            if let Some(status) = x["status"].as_str() {
                line.push_str(&format!(": \"{status}\""));
            }
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}
