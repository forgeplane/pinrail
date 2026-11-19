//! The origin a review gets from the git checkout it is sent from, so the
//! inbox files it under its project even when the agent says nothing about
//! it: `repo` from the remote, `ref` from the branch. What the request or
//! `--origin` gives wins; only what is missing is filled.

use std::collections::BTreeMap;

use pinrail_runtime::checkout::Checkout as Found;
use serde_json::{Map, Value, json};

/// The origin keys the app keeps, as the core's `ORIGIN_KEYS` lists them;
/// it drops any other.
const KEYS: [&str; 6] = ["repo", "ref", "workflow", "run_id", "url", "folder"];

/// Takes out of the body's origin the keys the app would drop without a
/// word, and says so on stderr, so a misspelled key is noticed.
pub fn drop_unknown(body: &mut Value) {
    let Some(origin) = body.get_mut("origin").and_then(Value::as_object_mut) else {
        return;
    };
    let unknown: Vec<String> = origin
        .keys()
        .filter(|k| !KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    for key in unknown {
        origin.remove(&key);
        eprintln!(
            "pinrail: warning: origin key {key} dropped: the app keeps {}",
            KEYS.join(", ")
        );
    }
}

/// Fills `repo` and `ref` in the body's origin from the checkout the command
/// runs in, when they are missing, and says so on stderr. Outside a
/// checkout, or without git, the body is left as it is.
pub fn fill_from_git(body: &mut Value) {
    let said = fill_body(body);
    if !said.is_empty() {
        crate::out::note(format_args!("origin from git: {}", said.join(", ")));
    }
}

/// `fill_from_git`, saying nothing, for a command that prints nothing.
pub fn fill_quietly(body: &mut Value) {
    fill_body(body);
}

/// Fills the body's origin: the folder the command works in, then what
/// the checkout gives. Answers what it took from git, as key=value.
fn fill_body(body: &mut Value) -> Vec<String> {
    let mut origin = match body.get("origin") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let found = from_git();
    if missing(&origin, "folder")
        && let Some(folder) = found
            .as_ref()
            .map(|f| f.top.clone())
            .or_else(current_folder)
    {
        origin.insert("folder".to_string(), json!(folder));
    }
    let said = match found {
        Some(found) if missing(&origin, "repo") || missing(&origin, "ref") => {
            fill(&mut origin, found)
        }
        _ => Vec::new(),
    };
    body["origin"] = Value::Object(origin);
    said
}

/// The folder the command runs in, when it can be read.
fn current_folder() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|dir| dir.display().to_string())
}

/// Puts in `origin` what the checkout gives that it lacks, and answers
/// what it put, as key=value.
fn fill(origin: &mut Map<String, Value>, found: Found) -> Vec<String> {
    // a branch belongs to its repository: the checkout's is taken only
    // for the checkout's own project
    let given = origin
        .get("repo")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty());
    let reference = match given {
        Some(repo) if found.repo.as_deref() != Some(repo) => None,
        _ => found.reference,
    };
    let mut said = Vec::new();
    for (key, value) in [("repo", found.repo), ("ref", reference)] {
        if let Some(value) = value.filter(|_| missing(origin, key)) {
            said.push(format!("{key}={value}"));
            origin.insert(key.to_string(), json!(value));
        }
    }
    said
}

fn missing(origin: &Map<String, Value>, key: &str) -> bool {
    origin
        .get(key)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
}

/// The project of the checkout the command runs in, as `fill_from_git`
/// would name it.
pub fn repo() -> Option<String> {
    from_git()?.repo
}

/// The checkout the command runs in.
fn from_git() -> Option<Found> {
    pinrail_runtime::checkout::of(&std::env::current_dir().ok()?)
}

/// `--origin repo=acme/api,url=https://…`: comma-separated pairs. A url
/// may hold commas of its own: inside one, only a comma followed by a
/// known key starts the next pair.
pub fn parse(s: &str) -> Result<BTreeMap<String, String>, String> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for part in s.split(',') {
        let key = part.split_once('=').map(|(k, _)| k.trim());
        let in_url = pairs.last().is_some_and(|(k, _)| k == "url");
        match key {
            Some(k) if !in_url || KEYS.contains(&k) => {
                let (k, v) = part.split_once('=').unwrap_or_default();
                pairs.push((k.trim().to_string(), v.trim().to_string()));
            }
            _ if in_url => {
                let (_, url) = pairs.last_mut().unwrap();
                url.push(',');
                url.push_str(part.trim());
            }
            _ if part.trim().is_empty() => {}
            _ => return Err(format!("expected key=value, got {part:?}")),
        }
    }
    let map: BTreeMap<String, String> = pairs.into_iter().collect();
    if map.is_empty() {
        return Err("origin needs at least one key=value".into());
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_branch_is_taken_only_for_the_checkouts_own_project() {
        let checkout = || super::Found {
            top: "/work/api".into(),
            repo: Some("acme/api".into()),
            reference: Some("feature".into()),
        };
        // nothing given: the checkout's project and branch
        let mut origin = serde_json::Map::new();
        super::fill(&mut origin, checkout());
        assert_eq!(origin["repo"], "acme/api");
        assert_eq!(origin["ref"], "feature");
        // this very project: its branch too
        let mut origin = serde_json::json!({ "repo": "acme/api" })
            .as_object()
            .unwrap()
            .clone();
        super::fill(&mut origin, checkout());
        assert_eq!(origin["ref"], "feature");
        // another project: the checkout's branch is not its branch
        let mut origin = serde_json::json!({ "repo": "acme/infra" })
            .as_object()
            .unwrap()
            .clone();
        super::fill(&mut origin, checkout());
        assert_eq!(origin["repo"], "acme/infra");
        assert!(origin.get("ref").is_none(), "{origin:?}");
    }

    #[test]
    fn a_link_with_commas_stays_one_value() {
        // CI links carry commas: only a comma that starts a known key splits
        let origin =
            super::parse("repo=acme/api,url=https://ci.example/run?ids=1,2,workflow=deploy")
                .unwrap();
        assert_eq!(origin["url"], "https://ci.example/run?ids=1,2");
        assert_eq!(origin["repo"], "acme/api");
        assert_eq!(origin["workflow"], "deploy");
    }
}
