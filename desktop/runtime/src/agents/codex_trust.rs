//! Trusting Pinrail's hooks in Codex. Codex runs a hook of the person's
//! only when `config.toml` holds the hash of its handler, and shows a
//! review screen at its start otherwise. Pinrail asks Codex's own app
//! server for the hooks and their hashes, and writes the hash of each of
//! its own handlers, exactly as it installed them, so that no other hook
//! is trusted on the person's behalf.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::integration::{HookEntry, Integration, TIMEOUT_SECONDS};

/// How long the app server has for the whole exchange, its start through
/// the login shell included.
const WITHIN: Duration = Duration::from_secs(20);

/// Trusts Pinrail's handlers in `plan`'s file that Codex does not trust
/// yet, through the app server that `command` starts, and returns how many
/// it trusted.
pub(crate) fn trust(plan: &Integration, mut command: Command) -> Result<usize, String> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Codex's app server did not start: {e}"))?;
    let result = exchange(&mut child, plan);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn exchange(child: &mut Child, plan: &Integration) -> Result<usize, String> {
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (lines, received) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let mut server = Server {
        stdin,
        received,
        until: Instant::now() + WITHIN,
        last_id: 0,
    };
    server.call(
        "initialize",
        json!({"clientInfo": {"name": "pinrail", "version": env!("CARGO_PKG_VERSION")}}),
    )?;
    server.notify("initialized")?;
    let folder = plan.file.parent().unwrap_or(&plan.file);
    let listed = server.call("hooks/list", json!({"cwds": [folder]}))?;
    let hashes = untrusted(&listed, plan);
    if hashes.is_empty() {
        return Ok(0);
    }
    let count = hashes.len();
    server.call(
        "config/batchWrite",
        json!({
            "edits": [{"keyPath": "hooks.state", "mergeStrategy": "upsert", "value": hashes}],
            "reloadUserConfig": true
        }),
    )?;
    Ok(count)
}

/// Pinrail's handlers in `plan`'s file that Codex does not trust, in the
/// result of `hooks/list`, as the entries of `hooks.state` that trust them:
/// each key with `{"trusted_hash": <Codex's hash>}`. A handler counts only
/// when it is exactly as Pinrail installed it.
pub(crate) fn untrusted(listed: &Value, plan: &Integration) -> Map<String, Value> {
    let file = plan.file.to_string_lossy();
    listed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| entry["hooks"].as_array().into_iter().flatten())
        .filter(|hook| {
            hook["sourcePath"]
                .as_str()
                .is_some_and(|path| same_path(path, &file))
        })
        .filter(|hook| matches!(hook["trustStatus"].as_str(), Some("untrusted" | "modified")))
        .filter(|hook| plan.entries.iter().any(|entry| as_installed(hook, entry)))
        .filter_map(|hook| {
            let key = hook["key"].as_str()?.to_string();
            let hash = hook["currentHash"].as_str()?;
            Some((key, json!({"trusted_hash": hash})))
        })
        .collect()
}

/// Whether two spellings of a path name one file: as they are, or on
/// Windows with either separator, in any case, and without the `\\?\` of
/// a long path.
fn same_path(a: &str, b: &str) -> bool {
    if !cfg!(windows) {
        return a == b;
    }
    let plain = |p: &str| {
        let p = p.replace('/', "\\");
        p.strip_prefix(r"\\?\")
            .map(str::to_string)
            .unwrap_or(p)
            .to_lowercase()
    };
    plain(a) == plain(b)
}

/// Whether Codex's description of a hook is `entry` as Pinrail writes it.
fn as_installed(hook: &Value, entry: &HookEntry) -> bool {
    hook["handlerType"] == "command"
        && hook["command"].as_str() == Some(&entry.command)
        && hook["eventName"].as_str() == Some(&event_name(entry.event))
        && hook["async"].as_bool().unwrap_or(false) == entry.background
        && hook["timeoutSec"].as_u64() == Some(TIMEOUT_SECONDS)
        && hook["matcher"].as_str() == entry.matcher
}

/// The app server's name of a hooks file event: `userPromptSubmit` for
/// `UserPromptSubmit`.
fn event_name(event: &str) -> String {
    let mut chars = event.chars();
    chars
        .next()
        .map(|first| first.to_ascii_lowercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

/// The app server, over its standard input and output, one JSON message
/// per line.
struct Server {
    stdin: ChildStdin,
    received: Receiver<String>,
    until: Instant,
    last_id: u64,
}

impl Server {
    fn send(&mut self, message: Value) -> Result<(), String> {
        writeln!(self.stdin, "{message}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| format!("Codex's app server stopped: {e}"))
    }

    fn notify(&mut self, method: &str) -> Result<(), String> {
        self.send(json!({"jsonrpc": "2.0", "method": method}))
    }

    /// The result of `method`. Lines that are not the answer, such as
    /// notifications or what the login shell prints, are skipped.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.last_id += 1;
        let id = self.last_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let left = self.until.saturating_duration_since(Instant::now());
            let line = match self.received.recv_timeout(left) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "Codex's app server did not answer {method} in time"
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("Codex's app server stopped".to_string());
                }
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let detail = error["message"].as_str().unwrap_or("an error");
                return Err(format!("Codex's app server refused {method}: {detail}"));
            }
            return Ok(message["result"].clone());
        }
    }
}

#[cfg(test)]
/// An app server in a shell script: after a line of the login shell's
/// own output, it answers `initialize`, answers `hooks/list` with the
/// fixture, and keeps the `config/batchWrite` request in `$OUT`.
pub(crate) const FAKE_SERVER: &str = r#"
echo "Welcome to the login shell"
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
*'"method":"initialize"'*) echo "{\"id\":$id,\"result\":{}}" ;;
*'"method":"hooks/list"'*) tr -d '\n' < "$LIST" | sed "s/\"id\": *[0-9]*/\"id\":$id/"; echo ;;
*'"method":"config/batchWrite"'*)
  printf '%s\n' "$line" > "$OUT"
  echo "{\"id\":$id,\"result\":{\"status\":\"ok\"}}" ;;
  esac
done
"#;

/// A command that runs the fake app server, answering `hooks/list` from
/// the fixture and keeping the `config/batchWrite` request in `out`.
#[cfg(test)]
pub(crate) fn fake_app_server(out: &std::path::Path) -> Command {
    let mut command = Command::new("sh");
    command
        .args(["-c", FAKE_SERVER])
        .env(
            "LIST",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/codex-app-server/hooks-list.json"
            ),
        )
        .env("OUT", out);
    command
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::agent::Agent;
    use crate::agents::Codex;

    fn plan() -> Integration {
        Codex
            .integration(Path::new("/Users/you"), &|_| None)
            .unwrap()
    }

    /// The hooks file the fixture names, as Codex writes its path on Unix.
    const FIXTURE_FILE: &str = "/Users/you/.codex/hooks.json";

    /// The fixture, naming the plan's hooks file as this system writes its
    /// path, and Pinrail's commands as this system's are.
    fn fixture() -> String {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/codex-app-server/hooks-list.json"
        );
        let file = plan().file.display().to_string().replace('\\', "\\\\");
        // as JSON writes them, quotes and all
        let unix = serde_json::to_string(r#""${PINRAIL_CLI:-pinrail}" hook"#).unwrap();
        let ours = serde_json::to_string(crate::integration::COMMAND_PREFIX).unwrap();
        std::fs::read_to_string(path)
            .unwrap()
            .replace(FIXTURE_FILE, &file)
            .replace(unix.trim_matches('"'), ours.trim_matches('"'))
    }

    fn listed() -> Value {
        serde_json::from_str::<Value>(&fixture()).unwrap()["result"].clone()
    }

    /// The key Codex gives one of the plan's handlers.
    fn key(handler: &str) -> String {
        format!("{}:{handler}", plan().file.display())
    }

    #[test]
    fn only_pinrail_s_handlers_as_installed_are_trusted() {
        let hashes = untrusted(&listed(), &plan());
        // seven of Pinrail's, and not the person's own SessionStart hook
        assert_eq!(hashes.len(), 7);
        assert!(!hashes.contains_key(&key("session_start:0:0")));
        assert_eq!(
            hashes[&key("session_start:1:0")],
            json!({"trusted_hash": "sha256:372fa435cd287901b7cd307bdab1750ab43bcd7330e9178603bb6e72516aa179"})
        );
    }

    /// Codex can spell the file otherwise on Windows: with / for \, in
    /// other capitals, or with the \\?\ of a long path.
    #[cfg(windows)]
    #[test]
    fn the_hooks_file_is_known_however_windows_spells_it() {
        let mut listed = listed();
        let respelt = format!(
            r"\\?\{}",
            plan()
                .file
                .display()
                .to_string()
                .to_uppercase()
                .replace('\\', "/")
        );
        for hook in listed["data"][0]["hooks"].as_array_mut().unwrap() {
            hook["sourcePath"] = json!(respelt);
        }
        assert_eq!(untrusted(&listed, &plan()).len(), 7);
    }

    #[test]
    fn a_trusted_or_edited_handler_is_left_alone() {
        let mut listed = listed();
        for hook in listed["data"][0]["hooks"].as_array_mut().unwrap() {
            match hook["eventName"].as_str().unwrap() {
                "stop" => hook["trustStatus"] = json!("trusted"),
                "interrupt" => hook["timeoutSec"] = json!(30),
                "sessionEnd" => hook["async"] = json!(true),
                _ => {}
            }
        }
        let hashes = untrusted(&listed, &plan());
        assert_eq!(hashes.len(), 4);
        assert!(hashes.keys().all(|k| !k.contains(":stop:")
            && !k.contains(":interrupt:")
            && !k.contains(":session_end:")));
    }

    #[test]
    fn trust_writes_codex_s_hashes_through_its_app_server() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("write.json");
        let list = dir.path().join("hooks-list.json");
        std::fs::write(&list, fixture()).unwrap();
        let mut server = fake_app_server(&out);
        server.env("LIST", &list);
        assert_eq!(trust(&plan(), server), Ok(7));
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let edit = &written["params"]["edits"][0];
        assert_eq!(edit["keyPath"], "hooks.state");
        assert_eq!(edit["mergeStrategy"], "upsert");
        assert_eq!(edit["value"], Value::Object(untrusted(&listed(), &plan())));
    }

    #[test]
    fn an_app_server_that_cannot_start_is_an_error() {
        let command = Command::new("/nonexistent/codex");
        let error = trust(&plan(), command).unwrap_err();
        assert!(
            error.starts_with("Codex's app server did not start"),
            "{error}"
        );
    }
}
