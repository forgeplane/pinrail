//! The origin a review gets from the git checkout it is sent from, so the
//! inbox files it under its project even when the agent says nothing about
//! it: `repo` from the remote, `ref` from the branch. What the request or
//! `--origin` gives wins; only what is missing is filled.

use std::process::{Command, Stdio};

use serde_json::{Map, Value, json};

/// Fills `repo` and `ref` in the body's origin from the checkout the command
/// runs in, when they are missing, and says so on stderr. Outside a
/// checkout, or without git, the body is left as it is.
pub fn fill_from_git(body: &mut Value) {
    let mut origin = match body.get("origin") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    if !missing(&origin, "repo") && !missing(&origin, "ref") {
        return;
    }
    let Some(found) = from_git() else {
        return;
    };
    let mut said = Vec::new();
    for (key, value) in [("repo", found.repo), ("ref", found.reference)] {
        if let Some(value) = value.filter(|_| missing(&origin, key)) {
            said.push(format!("{key}={value}"));
            origin.insert(key.to_string(), json!(value));
        }
    }
    if !said.is_empty() {
        eprintln!("pinrail: origin from git: {}", said.join(", "));
        body["origin"] = Value::Object(origin);
    }
}

fn missing(origin: &Map<String, Value>, key: &str) -> bool {
    origin
        .get(key)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
}

struct Found {
    repo: Option<String>,
    reference: Option<String>,
}

fn from_git() -> Option<Found> {
    let top = git(&["rev-parse", "--show-toplevel"])?;
    let remote = git(&["remote", "get-url", "origin"]).or_else(|| {
        let first = git(&["remote"])?.lines().next()?.to_string();
        git(&["remote", "get-url", &first])
    });
    let repo = remote
        .as_deref()
        .and_then(project_of)
        .or_else(|| top.rsplit('/').next().map(str::to_string))
        .filter(|r| !r.is_empty());
    // the branch, even before its first commit; none on a detached HEAD
    let reference = git(&["symbolic-ref", "--short", "HEAD"]);
    Some(Found { repo, reference })
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// The project a remote names: its path on the host, without `.git`.
/// `git@github.com:acme/api.git` and `https://github.com/acme/api` are
/// `acme/api`; a subgroup stays in (`group/sub/api`); a local path gives its
/// last folder.
fn project_of(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let path = if let Some((_, rest)) = url.split_once("://") {
        // scheme://[user@]host[:port]/path
        rest.split_once('/').map(|(_, path)| path)?
    } else if url.starts_with('/') || url.starts_with('.') || url.starts_with('~') {
        url.rsplit('/').next()?
    } else if let Some((_, path)) = url.split_once(':') {
        // [user@]host:path
        path
    } else {
        url
    };
    let path = path.trim_matches('/');
    (!path.is_empty()).then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::project_of;

    #[test]
    fn a_remote_names_its_project_by_its_path() {
        for (url, project) in [
            ("git@github.com:acme/api.git", "acme/api"),
            ("https://github.com/acme/api.git", "acme/api"),
            ("https://github.com/acme/api", "acme/api"),
            ("https://github.com/acme/api/", "acme/api"),
            (
                "ssh://git@gitlab.example.com:2222/group/sub/api.git",
                "group/sub/api",
            ),
            ("https://token@gitlab.com/group/api.git", "group/api"),
            ("/srv/git/api.git", "api"),
            ("../api", "api"),
        ] {
            assert_eq!(project_of(url).as_deref(), Some(project), "{url}");
        }
        assert_eq!(project_of("https://github.com"), None);
    }
}
