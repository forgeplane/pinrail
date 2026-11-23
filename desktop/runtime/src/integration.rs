//! What Report status installs for an agent so that its events run
//! `pinrail hook`: hooks in its settings or a hooks file, or an extension
//! file of Pinrail's own. How to add it, how to take it out again, and
//! whether the file holds it as this version would write it. Everything
//! else in a file is left as it is.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

/// How a command Pinrail installs starts, which is also how Pinrail tells
/// its own entries from the person's.
pub const COMMAND_PREFIX: &str = "\"${PINRAIL_CLI:-pinrail}\" hook";

/// The longest a hook may run, in seconds: `pinrail hook` gives up after
/// one.
pub const TIMEOUT_SECONDS: u64 = 2;

/// What Report status installs for an agent: its kind, the file it goes
/// in, how the file lays it out, and the hooks Pinrail adds there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Integration {
    pub kind: Kind,
    pub file: PathBuf,
    pub layout: Layout,
    pub entries: Vec<HookEntry>,
}

/// What Report status installs, as the person reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Hooks in the agent's settings or hooks file.
    Hooks,
    /// An extension file the agent loads, such as Pi's.
    Extension,
    /// A plugin the agent loads from a list in its settings, such as
    /// OpenCode's.
    Plugin,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Hooks => "hooks",
            Kind::Extension => "extension",
            Kind::Plugin => "plugin",
        }
    }
}

/// How a file holds its hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// A settings file whose `hooks` maps each event to groups of
    /// `{matcher, hooks}`, as Claude Code's does. Pinrail's handlers go in
    /// groups of their own beside the person's.
    Settings,
    /// A file of named hooks, each mapping events to its handlers, as
    /// Antigravity's does. Pinrail owns the hook of this name: an event
    /// with a matcher takes groups of `{matcher, hooks}`, any other a list
    /// of handlers.
    Named(&'static str),
    /// A hooks file with a `version` and a `hooks` object whose events each
    /// list their handlers directly, without matcher groups, as Copilot's
    /// and Cursor's do. Pinrail's handlers go beside the person's.
    Flat,
    /// A file of Pinrail's own written whole, such as Pi's extension:
    /// `text` is its content, and a file whose first line starts with
    /// `marker` is Pinrail's, of this version or another. Any other file at
    /// that path is the person's, and is never written or removed.
    File {
        text: &'static str,
        marker: &'static str,
    },
    /// A file of Pinrail's own, at `path` beside the settings file, which
    /// the agent loads because `entry` is in the list `key` of that file,
    /// as OpenCode loads its plugins from `cli.json`. The entry goes beside
    /// the person's, and the file is told from theirs as `File`'s is.
    Listed {
        key: &'static str,
        entry: &'static str,
        path: &'static str,
        text: &'static str,
        marker: &'static str,
    },
}

/// One hook: the agent's event, its matcher if it takes one, the command
/// it runs, and whether the agent runs it without waiting for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEntry {
    pub event: &'static str,
    pub matcher: Option<&'static str>,
    pub command: String,
    pub background: bool,
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
            background: false,
        }
    }

    /// The same hook, written with `"async": true`, which Codex reads as
    /// running it without waiting for it.
    pub fn in_background(self) -> Self {
        HookEntry {
            background: true,
            ..self
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
fn handler(command: &str, background: bool) -> Value {
    let mut handler = json!({"type": "command", "command": command, "timeout": TIMEOUT_SECONDS});
    if background {
        handler["async"] = json!(true);
    }
    handler
}

/// The handler Pinrail writes for `entry`.
fn entry_handler(entry: &HookEntry) -> Value {
    handler(&entry.command, entry.background)
}

/// Whether `h` is a handler exactly as some version of Pinrail writes one,
/// run in the background or not.
fn as_written(h: &Value) -> bool {
    let command = h.get("command").and_then(Value::as_str).unwrap_or_default();
    is_pinrail(h) && (h == &handler(command, false) || h == &handler(command, true))
}

/// Whether a handler is written to run in the background.
fn runs_in_background(h: &Value) -> bool {
    h.get("async").and_then(Value::as_bool) == Some(true)
}

/// Pinrail's handlers in `settings`, each with its event and matcher.
fn installed(settings: &Value) -> Vec<(String, Option<String>, Value)> {
    let mut found = Vec::new();
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return found;
    };
    for (event, groups) in hooks {
        for group in groups.as_array().into_iter().flatten() {
            // a flat file lists the handlers themselves
            let Some(handlers) = group.get("hooks").and_then(Value::as_array) else {
                if is_pinrail(group) {
                    found.push((event.clone(), None, group.clone()));
                }
                continue;
            };
            let matcher = group
                .get("matcher")
                .and_then(Value::as_str)
                .map(str::to_string);
            for h in handlers {
                if is_pinrail(h) {
                    found.push((event.clone(), matcher.clone(), h.clone()));
                }
            }
        }
    }
    found
}

/// The named hook Pinrail writes for `plan`.
fn named_hook(plan: &Integration) -> Value {
    let mut events = Map::new();
    for entry in &plan.entries {
        let item = match entry.matcher {
            Some(matcher) => json!({"matcher": matcher, "hooks": [entry_handler(entry)]}),
            None => entry_handler(entry),
        };
        events
            .entry(entry.event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("an event's hooks are a list")
            .push(item);
    }
    Value::Object(events)
}

/// The state of the named hook `name` in `file` against `plan`.
fn named_state(file: &Value, name: &str, plan: &Integration) -> HookState {
    let Some(hooks) = file.as_object() else {
        return HookState::Unreadable;
    };
    let Some(hook) = hooks.get(name) else {
        return HookState::Absent;
    };
    if !hook.is_object() {
        return HookState::Unreadable;
    }
    if hook == &named_hook(plan) {
        return HookState::Current;
    }
    // every handler as Pinrail writes one, for other events: another
    // version's; anything else was edited
    let handlers = hook.as_object().into_iter().flat_map(|events| {
        events
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .flat_map(|item| match item.get("hooks").and_then(Value::as_array) {
                Some(group) => group.iter().collect::<Vec<_>>(),
                None => vec![item],
            })
    });
    if handlers.clone().all(as_written) && handlers.count() > 0 {
        HookState::Outdated
    } else {
        HookState::Modified
    }
}

/// The state of Pinrail's hooks in `settings` (a file's parsed contents, or
/// `None` when the file does not parse) against `plan`.
pub fn state(settings: Option<&Value>, plan: &Integration) -> HookState {
    let Some(settings) = settings else {
        return HookState::Unreadable;
    };
    if let Layout::Named(name) = plan.layout {
        return named_state(settings, name, plan);
    }
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
    if found.iter().any(|(_, _, h)| !as_written(h)) {
        return HookState::Modified;
    }
    let mut have: Vec<(String, Option<String>, String, bool)> = found
        .into_iter()
        .map(|(e, m, h)| {
            let command = h["command"].as_str().unwrap_or_default().to_string();
            (e, m, command, runs_in_background(&h))
        })
        .collect();
    let mut want: Vec<(String, Option<String>, String, bool)> = plan
        .entries
        .iter()
        .map(|e| {
            (
                e.event.to_string(),
                e.matcher.map(str::to_string),
                e.command.clone(),
                e.background,
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
pub fn remove(settings: &mut Value, plan: &Integration) {
    if let Layout::Named(name) = plan.layout {
        settings.as_object_mut().map(|s| s.remove(name));
        return;
    }
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
        // a group left empty goes, and in a flat file Pinrail's handlers
        list.retain(|g| match g.get("hooks").and_then(Value::as_array) {
            Some(handlers) => !handlers.is_empty(),
            None => !is_pinrail(g),
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
pub fn install(settings: &mut Value, plan: &Integration) {
    remove(settings, plan);
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let root = settings.as_object_mut().expect("an object");
    if let Layout::Named(name) = plan.layout {
        root.insert(name.to_string(), named_hook(plan));
        return;
    }
    let flat = matches!(plan.layout, Layout::Flat);
    if flat {
        root.entry("version").or_insert(json!(1));
    }
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("hooks is an object, as state() checks first");
    for entry in &plan.entries {
        if flat {
            hooks
                .entry(entry.event)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("an event's hooks are a list")
                .push(entry_handler(entry));
            continue;
        }
        let mut group = Map::new();
        if let Some(matcher) = entry.matcher {
            group.insert("matcher".into(), json!(matcher));
        }
        group.insert("hooks".into(), json!([entry_handler(entry)]));
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
pub fn file_state(plan: &Integration) -> HookState {
    if let Layout::File { text, marker } = plan.layout {
        return own_file_state(&plan.file, text, marker);
    }
    if let Layout::Listed { .. } = plan.layout {
        return listed_state(plan);
    }
    match read(&plan.file) {
        Ok(settings) => state(Some(&settings), plan),
        Err(_) => HookState::Unreadable,
    }
}

/// Installs Pinrail's hooks in the file `plan` names, or removes them, and
/// returns the state the file is left in. A file Pinrail cannot edit
/// safely is left alone. The first time Pinrail changes a file, it keeps
/// the original beside it as `<file>.pinrail-backup`.
pub fn apply(plan: &Integration, installed: bool) -> Result<HookState, String> {
    let shown = plan.file.display();
    if let Layout::File { text, marker } = plan.layout {
        return apply_own_file(&plan.file, text, marker, installed);
    }
    if let Layout::Listed { .. } = plan.layout {
        return apply_listed(plan, installed);
    }
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
        remove(&mut settings, plan);
    }
    if settings != before {
        write(&plan.file, &settings)?;
    }
    Ok(state(Some(&settings), plan))
}

/// A file of Pinrail's own written whole, which goes with what Pinrail
/// installs for an agent, such as Codex's rule for the `pinrail` command:
/// `text` at `path`. A file there whose first line starts with `marker` is
/// Pinrail's, of this version or another; any other file is the person's,
/// and is never written or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnFile {
    pub path: PathBuf,
    pub text: &'static str,
    pub marker: &'static str,
}

impl OwnFile {
    /// Whether the file is there as this version writes it.
    pub fn state(&self) -> HookState {
        own_file_state(&self.path, self.text, self.marker)
    }

    /// Writes the file, or removes it, unless a file of the person's is at
    /// its path, and returns the state it is left in.
    pub fn apply(&self, installed: bool) -> Result<HookState, String> {
        apply_own_file(&self.path, self.text, self.marker, installed)
    }
}

/// The state of a file Pinrail writes whole.
fn own_file_state(file: &std::path::Path, text: &str, marker: &str) -> HookState {
    match std::fs::read_to_string(file) {
        Ok(found) if found == text => HookState::Current,
        Ok(found) if found.lines().next().is_some_and(|l| l.starts_with(marker)) => {
            HookState::Outdated
        }
        Ok(_) => HookState::Unreadable,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HookState::Absent,
        Err(_) => HookState::Unreadable,
    }
}

/// Writes a file Pinrail owns whole, or removes it, unless a file of the
/// person's is at its path.
fn apply_own_file(
    file: &std::path::Path,
    text: &str,
    marker: &str,
    installed: bool,
) -> Result<HookState, String> {
    let shown = file.display();
    if own_file_state(file, text, marker) == HookState::Unreadable {
        return Err(format!(
            "{shown} is not a file Pinrail wrote, so it was left as it is"
        ));
    }
    let fail = |e: std::io::Error| format!("writing {shown}: {e}");
    if installed {
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder).map_err(fail)?;
        }
        let mut partial = file.as_os_str().to_owned();
        partial.push(".pinrail-partial");
        let partial = PathBuf::from(partial);
        std::fs::write(&partial, text).map_err(fail)?;
        std::fs::rename(&partial, file).map_err(fail)?;
    } else {
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(fail(e)),
            _ => {}
        }
    }
    Ok(own_file_state(file, text, marker))
}

/// The parts of a `Listed` integration: the list's key, the entry, and the
/// file's path, text and marker.
fn listed(
    plan: &Integration,
) -> (
    &'static str,
    &'static str,
    PathBuf,
    &'static str,
    &'static str,
) {
    let Layout::Listed {
        key,
        entry,
        path,
        text,
        marker,
    } = plan.layout
    else {
        unreachable!("a listed integration");
    };
    let folder = plan.file.parent().unwrap_or(std::path::Path::new(""));
    (key, entry, folder.join(path), text, marker)
}

/// Whether the settings file's list holds the entry, or `None` when the
/// file or its list is not one Pinrail can edit.
fn listed_entry(settings: &Value, key: &str, entry: &str) -> Option<bool> {
    let object = settings.as_object()?;
    match object.get(key) {
        None => Some(false),
        Some(Value::Array(list)) => Some(list.iter().any(|e| e.as_str() == Some(entry))),
        Some(_) => None,
    }
}

/// The state of a `Listed` integration: current with both the entry and
/// the file as this version writes them, absent with neither, and outdated
/// with one of them, or the file of another version.
fn listed_state(plan: &Integration) -> HookState {
    let (key, entry, file, text, marker) = listed(plan);
    let Ok(settings) = read(&plan.file) else {
        return HookState::Unreadable;
    };
    let Some(listed) = listed_entry(&settings, key, entry) else {
        return HookState::Unreadable;
    };
    match (listed, own_file_state(&file, text, marker)) {
        (_, HookState::Unreadable) => HookState::Unreadable,
        (true, HookState::Current) => HookState::Current,
        (false, HookState::Absent) => HookState::Absent,
        _ => HookState::Outdated,
    }
}

/// Writes a `Listed` integration's file and adds its entry, or takes both
/// out, leaving the person's entries and files as they are.
fn apply_listed(plan: &Integration, installed: bool) -> Result<HookState, String> {
    let (key, entry, file, text, marker) = listed(plan);
    if listed_state(plan) == HookState::Unreadable {
        return Err(format!(
            "{} is not a settings file Pinrail can edit, or {} is not Pinrail's, so they were left as they are",
            plan.file.display(),
            file.display()
        ));
    }
    let mut settings = read(&plan.file)?;
    let before = settings.clone();
    let object = settings
        .as_object_mut()
        .expect("an object, as listed_state checks");
    let list = object
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("a list, as listed_state checks");
    list.retain(|e| e.as_str() != Some(entry));
    if installed {
        apply_own_file(&file, text, marker, true)?;
        list.push(Value::from(entry));
    }
    if settings != before {
        write(&plan.file, &settings)?;
    }
    if !installed {
        apply_own_file(&file, text, marker, false)?;
        // the plugin's folder, left empty
        if let Some(folder) = file.parent() {
            let _ = std::fs::remove_dir(folder);
        }
    }
    Ok(listed_state(plan))
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
