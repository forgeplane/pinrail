//! Pinrail's hooks in an agent's own settings: what to add so that each
//! hook runs `pinrail hook`, how to take them out again, and whether the
//! file holds them as this version would write them. Everything else in the
//! file is left as it is. These are edits of the parsed file; reading and
//! writing it is the caller's.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

/// How a command Pinrail installs starts, which is also how Pinrail tells
/// its own entries from the person's.
pub const COMMAND_PREFIX: &str = "\"${PINRAIL_CLI:-pinrail}\" hook";

/// The longest a hook may run, in seconds: `pinrail hook` gives up after
/// one.
pub const TIMEOUT_SECONDS: u64 = 2;

/// Where an agent keeps its hooks, and the entries Pinrail adds there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookPlan {
    pub file: PathBuf,
    pub entries: Vec<HookEntry>,
}

/// One hook: the agent's event, its matcher if it takes one, and the
/// command it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEntry {
    pub event: &'static str,
    pub matcher: Option<&'static str>,
    pub command: String,
}

impl HookEntry {
    pub fn new(
        agent: &str,
        event: &'static str,
        matcher: Option<&'static str>,
        hook: &str,
    ) -> Self {
        HookEntry {
            event,
            matcher,
            command: format!("{COMMAND_PREFIX} {agent} {hook}"),
        }
    }
}

/// What an agent's settings hold of Pinrail's hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookState {
    /// None of Pinrail's hooks.
    Absent,
    /// Pinrail's hooks, as this version writes them.
    Current,
    /// Pinrail's hooks as another version wrote them: some are missing or
    /// no longer used.
    Outdated,
    /// One of Pinrail's hooks was changed by hand.
    Modified,
    /// The file is not a settings object Pinrail can edit: nothing is
    /// written to it.
    Unreadable,
}

impl HookState {
    pub fn as_str(self) -> &'static str {
        match self {
            HookState::Absent => "absent",
            HookState::Current => "current",
            HookState::Outdated => "outdated",
            HookState::Modified => "modified",
            HookState::Unreadable => "unreadable",
        }
    }
}

/// Whether a handler is one of Pinrail's.
fn is_pinrail(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.starts_with(COMMAND_PREFIX))
}

/// A handler as Pinrail writes it.
fn handler(command: &str) -> Value {
    json!({"type": "command", "command": command, "timeout": TIMEOUT_SECONDS})
}

/// Pinrail's handlers in `settings`, each with its event and matcher.
fn installed(settings: &Value) -> Vec<(String, Option<String>, Value)> {
    let mut found = Vec::new();
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return found;
    };
    for (event, groups) in hooks {
        for group in groups.as_array().into_iter().flatten() {
            let matcher = group
                .get("matcher")
                .and_then(Value::as_str)
                .map(str::to_string);
            for h in group
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if is_pinrail(h) {
                    found.push((event.clone(), matcher.clone(), h.clone()));
                }
            }
        }
    }
    found
}

/// The state of Pinrail's hooks in `settings` (a file's parsed contents, or
/// `None` when the file does not parse) against `plan`.
pub fn state(settings: Option<&Value>, plan: &HookPlan) -> HookState {
    let Some(settings) = settings else {
        return HookState::Unreadable;
    };
    // hooks Pinrail could not edit without guessing: not an object of
    // events, each a list
    let hooks_ok = match settings.get("hooks") {
        None => true,
        Some(Value::Object(events)) => events.values().all(Value::is_array),
        Some(_) => false,
    };
    if !settings.is_object() || !hooks_ok {
        return HookState::Unreadable;
    }
    let found = installed(settings);
    if found.is_empty() {
        return HookState::Absent;
    }
    // a handler that is not exactly as Pinrail writes one was edited
    if found.iter().any(|(_, _, h)| {
        h != &handler(h.get("command").and_then(Value::as_str).unwrap_or_default())
    }) {
        return HookState::Modified;
    }
    let mut have: Vec<(String, Option<String>, String)> = found
        .into_iter()
        .map(|(e, m, h)| (e, m, h["command"].as_str().unwrap_or_default().to_string()))
        .collect();
    let mut want: Vec<(String, Option<String>, String)> = plan
        .entries
        .iter()
        .map(|e| {
            (
                e.event.to_string(),
                e.matcher.map(str::to_string),
                e.command.clone(),
            )
        })
        .collect();
    have.sort();
    want.sort();
    if have == want {
        HookState::Current
    } else {
        HookState::Outdated
    }
}

/// `settings` without Pinrail's hooks: groups and events left empty go,
/// and every other entry stays as it was.
pub fn remove(settings: &mut Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    for groups in hooks.values_mut() {
        let Some(list) = groups.as_array_mut() else {
            continue;
        };
        for group in list.iter_mut() {
            if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                handlers.retain(|h| !is_pinrail(h));
            }
        }
        list.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|h| !h.is_empty())
        });
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    if hooks.is_empty() {
        settings.as_object_mut().map(|s| s.remove("hooks"));
    }
}

/// `settings` with Pinrail's hooks as `plan` lists them, in place of any it
/// held before. Each goes in a group of its own, so the person's groups are
/// never changed.
pub fn install(settings: &mut Value, plan: &HookPlan) {
    remove(settings);
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let root = settings.as_object_mut().expect("an object");
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("hooks is an object, as state() checks first");
    for entry in &plan.entries {
        let mut group = Map::new();
        if let Some(matcher) = entry.matcher {
            group.insert("matcher".into(), json!(matcher));
        }
        group.insert("hooks".into(), json!([handler(&entry.command)]));
        hooks
            .entry(entry.event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("an event's hooks are a list")
            .push(Value::Object(group));
    }
}

/// The state of Pinrail's hooks in the file `plan` names. A file that is
/// not there holds none.
pub fn file_state(plan: &HookPlan) -> HookState {
    match read(&plan.file) {
        Ok(settings) => state(Some(&settings), plan),
        Err(_) => HookState::Unreadable,
    }
}

/// Installs Pinrail's hooks in the file `plan` names, or removes them, and
/// returns the state the file is left in. A file Pinrail cannot edit
/// safely is left alone. The first time Pinrail changes a file, it keeps
/// the original beside it as `<file>.pinrail-backup`.
pub fn apply(plan: &HookPlan, installed: bool) -> Result<HookState, String> {
    let shown = plan.file.display();
    let mut settings = read(&plan.file)?;
    if state(Some(&settings), plan) == HookState::Unreadable {
        return Err(format!(
            "{shown} is not a settings file Pinrail can edit, so it was left as it is"
        ));
    }
    let before = settings.clone();
    if installed {
        install(&mut settings, plan);
    } else {
        remove(&mut settings);
    }
    if settings != before {
        write(&plan.file, &settings)?;
    }
    Ok(state(Some(&settings), plan))
}

/// A settings file, parsed; `{}` when it is not there.
fn read(file: &std::path::Path) -> Result<Value, String> {
    match std::fs::read_to_string(file) {
        Ok(text) if text.trim().is_empty() => Ok(Value::Object(Map::new())),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} does not parse: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(e) => Err(format!("reading {}: {e}", file.display())),
    }
}

/// Writes `settings` to `file` through a temporary file beside it, which
/// replaces the file whole, after keeping a backup of the original once.
fn write(file: &std::path::Path, settings: &Value) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("writing {}: {e}", file.display());
    let mut backup = file.as_os_str().to_owned();
    backup.push(".pinrail-backup");
    let backup = PathBuf::from(backup);
    if file.exists() && !backup.exists() {
        std::fs::copy(file, &backup).map_err(fail)?;
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder).map_err(fail)?;
    }
    let mut partial = file.as_os_str().to_owned();
    partial.push(".pinrail-partial");
    let partial = PathBuf::from(partial);
    let mut text = serde_json::to_string_pretty(settings).expect("JSON values serialise");
    text.push('\n');
    std::fs::write(&partial, text).map_err(fail)?;
    // keep the file's permissions, which may hide it from other users
    if let Ok(meta) = std::fs::metadata(file) {
        std::fs::set_permissions(&partial, meta.permissions()).map_err(fail)?;
    }
    std::fs::rename(&partial, file).map_err(fail)
}
