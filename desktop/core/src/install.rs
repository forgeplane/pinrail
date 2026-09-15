//! Installing a plugin from a folder: inspect it, build it when its
//! manifest says so, place the bundle in the store, record where it came
//! from. A link points the registry at the folder instead and serves it
//! live.
//!
//! The store keeps one entry per plugin and major version, the latest
//! installed in that line: an equal or higher version replaces it, an
//! older one is refused unless forced. An old line stays while a review
//! still renders from it.
//!
//! An install is a job: it reports its step and its build log as it goes,
//! so a dialog or a terminal can follow a build that takes a minute.
//!
//! A source is one string: a folder, or a git URL with `#path=` and
//! `#ref=` in its fragment, or a GitHub release URL (fetched in a later
//! step). It is parsed before anything is touched, so a bad one fails at
//! once and offline.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use chrono::Utc;
use serde_json::{Map, Value};

use crate::db::{Db, InstalledRecord};
use crate::error::Error;
use crate::plugins::{Plugin, Registry, hash_dir};

pub struct Options {
    /// serve the folder live instead of copying it
    pub link: bool,
    /// replace a newer version already installed
    pub force: bool,
    /// a branch, tag or commit, given beside a git source
    pub reference: Option<String>,
    /// the plugin's folder inside the repository, given beside a git source
    pub path: Option<String>,
}

/// Where a plugin comes from, as the source string says.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Folder(PathBuf),
    Git {
        url: String,
        /// the plugin's folder inside the repository, none for the root
        path: Option<String>,
        /// a branch, tag or commit; none for the default branch
        reference: Option<String>,
    },
    Release {
        owner: String,
        repo: String,
        /// none for the latest
        tag: Option<String>,
    },
}

impl Source {
    /// Parses the string a person typed, with the ref and the folder given
    /// beside it when they were (flags on the CLI, fields in the dialog).
    ///
    /// A folder is a path: `./review`, `~/code/review`, `/abs/review`, or a
    /// bare name that is not a host. A git source is a browser URL —
    /// `https://github.com/acme/plugins/tree/v3/review` — or the short
    /// form `github.com/acme/plugins/review@v3`, read as "this folder, at
    /// this version"; an SSH address takes its ref and folder beside it. A
    /// GitHub `/releases/tag/<tag>` or `/releases` URL is a release.
    pub fn parse(text: &str, reference: Option<&str>, path: Option<&str>) -> Result<Source, Error> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::invalid("/source", "is required"));
        }
        let reference = reference
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_string);
        let path = match path.map(str::trim).filter(|p| !p.is_empty()) {
            Some(p) => Some(Self::folder_in_repo(p)?),
            None => None,
        };

        let scheme = text.split_once("://").map(|(scheme, _)| scheme);
        let ssh = text.starts_with("git@");
        let starts_like_path =
            text.starts_with('.') || text.starts_with('/') || text.starts_with('~');
        let first = text.split('/').next().unwrap_or_default();
        let looks_like_host = !starts_like_path
            && scheme.is_none()
            && !ssh
            && first.contains('.')
            && !Path::new(text).is_dir();

        if scheme.is_none() && !ssh && !looks_like_host {
            if reference.is_some() || path.is_some() {
                return Err(Error::invalid("/source", "a folder takes no ref or path"));
            }
            let expanded = match text.strip_prefix("~/") {
                Some(rest) => std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(rest))
                    .unwrap_or_else(|| PathBuf::from(text)),
                None => PathBuf::from(text),
            };
            return Ok(Source::Folder(expanded));
        }

        if ssh || matches!(scheme, Some("ssh" | "git" | "file")) {
            return Ok(Source::Git {
                url: text.to_string(),
                path,
                reference,
            });
        }

        // https://host/owner/repo[/tree/ref/path] and host/owner/repo[/path][@ref]
        let (host, rest) = match scheme {
            Some(_) => {
                let without = text.split_once("://").map(|(_, r)| r).unwrap_or(text);
                without.split_once('/').unwrap_or((without, ""))
            }
            None => text.split_once('/').unwrap_or((text, "")),
        };
        let mut segments: Vec<&str> = rest
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if segments.len() < 2 {
            return Err(Error::invalid(
                "/source",
                "a repository needs an owner and a name: host/owner/repo",
            ));
        }
        let owner = segments.remove(0).to_string();
        let mut repo = segments.remove(0).to_string();

        if host == "github.com"
            && let Some(release) = Self::release(&owner, &repo, &segments)
        {
            if reference.is_some() || path.is_some() {
                return Err(Error::invalid("/source", "a release takes no ref or path"));
            }
            return Ok(release);
        }

        let (mut url_ref, mut url_path) = (None, None);
        if scheme.is_some() {
            // the browser's URL: /tree/<ref>/<path…>, GitLab's /-/tree/…
            let tree = segments.iter().position(|s| *s == "tree");
            if let Some(i) = tree {
                let before: Vec<&str> = segments[..i]
                    .iter()
                    .copied()
                    .filter(|s| *s != "-")
                    .collect();
                if !before.is_empty() {
                    return Err(Error::invalid(
                        "/source",
                        "the URL is not a repository or a folder in one",
                    ));
                }
                url_ref = segments.get(i + 1).map(|r| r.to_string());
                if url_ref.is_none() {
                    return Err(Error::invalid(
                        "/source",
                        "tree/ needs a branch or tag after it",
                    ));
                }
                if segments.len() > i + 2 {
                    url_path = Some(Self::folder_in_repo(&segments[i + 2..].join("/"))?);
                }
            } else if !segments.is_empty() {
                return Err(Error::invalid(
                    "/source",
                    "the URL is not a repository or a folder in one; a folder is …/tree/<branch>/<folder>",
                ));
            }
            if let Some(stripped) = repo.strip_suffix(".git") {
                repo = stripped.to_string();
            }
        } else {
            // the short form: folders as a path, the ref as a pin after @
            let mut tail = segments.join("/");
            if let Some((left, r)) = repo.split_once('@') {
                url_ref = Some(r.to_string());
                repo = left.to_string();
            } else if let Some((left, r)) = tail.rsplit_once('@') {
                url_ref = Some(r.to_string());
                tail = left.to_string();
            }
            if !tail.is_empty() {
                url_path = Some(Self::folder_in_repo(&tail)?);
            }
            if url_ref.as_deref().is_some_and(str::is_empty) {
                return Err(Error::invalid(
                    "/source",
                    "@ needs a branch, tag or commit after it",
                ));
            }
        }
        if reference.is_some() && url_ref.is_some() && reference != url_ref {
            return Err(Error::invalid("/source", "the ref is given twice"));
        }
        if path.is_some() && url_path.is_some() && path != url_path {
            return Err(Error::invalid("/source", "the folder is given twice"));
        }
        Ok(Source::Git {
            url: format!("https://{host}/{owner}/{repo}"),
            path: path.or(url_path),
            reference: reference.or(url_ref),
        })
    }

    fn folder_in_repo(p: &str) -> Result<String, Error> {
        let clean = p.trim_matches('/');
        if clean.is_empty() || clean.split('/').any(|seg| seg == ".." || seg == ".") {
            return Err(Error::invalid(
                "/source",
                "the folder must be inside the repository",
            ));
        }
        Ok(clean.to_string())
    }

    fn release(owner: &str, repo: &str, segments: &[&str]) -> Option<Source> {
        match segments {
            ["releases"] => Some(Source::Release {
                owner: owner.to_string(),
                repo: repo.to_string(),
                tag: None,
            }),
            ["releases", "tag", tag] if !tag.is_empty() => Some(Source::Release {
                owner: owner.to_string(),
                repo: repo.to_string(),
                tag: Some(tag.to_string()),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod source_tests {
    use super::Source;

    fn git(url: &str, path: Option<&str>, reference: Option<&str>) -> Source {
        Source::Git {
            url: url.into(),
            path: path.map(str::to_string),
            reference: reference.map(str::to_string),
        }
    }

    #[test]
    fn the_browser_url_and_the_short_form_say_the_same_thing() {
        let cases = [
            (
                "https://github.com/acme/plugins",
                git("https://github.com/acme/plugins", None, None),
            ),
            (
                "https://github.com/acme/plugins.git",
                git("https://github.com/acme/plugins", None, None),
            ),
            (
                "https://github.com/acme/plugins/tree/main/review",
                git(
                    "https://github.com/acme/plugins",
                    Some("review"),
                    Some("main"),
                ),
            ),
            (
                "https://github.com/acme/plugins/tree/v3/tools/review/",
                git(
                    "https://github.com/acme/plugins",
                    Some("tools/review"),
                    Some("v3"),
                ),
            ),
            (
                "https://gitlab.com/acme/plugins/-/tree/v3/review",
                git(
                    "https://gitlab.com/acme/plugins",
                    Some("review"),
                    Some("v3"),
                ),
            ),
            (
                "github.com/acme/plugins",
                git("https://github.com/acme/plugins", None, None),
            ),
            (
                "github.com/acme/plugins@v3",
                git("https://github.com/acme/plugins", None, Some("v3")),
            ),
            (
                "github.com/acme/plugins/review",
                git("https://github.com/acme/plugins", Some("review"), None),
            ),
            (
                "github.com/acme/plugins/review@v3",
                git(
                    "https://github.com/acme/plugins",
                    Some("review"),
                    Some("v3"),
                ),
            ),
            (
                "acme.internal/team/plugins/tools/review@abc1234",
                git(
                    "https://acme.internal/team/plugins",
                    Some("tools/review"),
                    Some("abc1234"),
                ),
            ),
            (
                "git@github.com:acme/plugins.git",
                git("git@github.com:acme/plugins.git", None, None),
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(Source::parse(text, None, None).unwrap(), expected, "{text}");
        }
        assert_eq!(
            Source::parse(
                "git@github.com:acme/plugins.git",
                Some("v3"),
                Some("review")
            )
            .unwrap(),
            git(
                "git@github.com:acme/plugins.git",
                Some("review"),
                Some("v3")
            )
        );
        assert_eq!(
            Source::parse(
                "https://github.com/acme/plugins/releases/tag/v1.2.0",
                None,
                None
            )
            .unwrap(),
            Source::Release {
                owner: "acme".into(),
                repo: "plugins".into(),
                tag: Some("v1.2.0".into())
            }
        );
        assert_eq!(
            Source::parse("https://github.com/acme/plugins/releases", None, None).unwrap(),
            Source::Release {
                owner: "acme".into(),
                repo: "plugins".into(),
                tag: None
            }
        );
    }

    #[test]
    fn folders_are_paths_and_bad_sources_fail_offline() {
        for text in [
            "./review",
            "../review",
            "/abs/review",
            "~/code/review",
            "plugins/review",
            "review",
        ] {
            assert!(
                matches!(Source::parse(text, None, None).unwrap(), Source::Folder(_)),
                "{text}"
            );
        }
        for (text, why) in [
            ("https://github.com/acme", "owner and a name"),
            (
                "https://github.com/acme/plugins/blob/main/x.js",
                "not a repository or a folder",
            ),
            ("https://github.com/acme/plugins/tree", "needs a branch"),
            ("github.com/acme/plugins/../x", "inside the repository"),
            ("github.com/acme/plugins@", "@ needs"),
            (
                "https://github.com/acme/plugins/releases#x",
                "not a repository",
            ),
        ] {
            let error = Source::parse(text, None, None).unwrap_err().to_string();
            assert!(error.contains(why), "{text}: {error}");
        }
        assert!(
            Source::parse("./review", Some("v3"), None)
                .unwrap_err()
                .to_string()
                .contains("takes no ref")
        );
        assert!(
            Source::parse("github.com/a/b@v3", Some("v4"), None)
                .unwrap_err()
                .to_string()
                .contains("given twice")
        );
    }
}

/// What a fetch of a git source produced: the checkout and its commit.
struct Fetched {
    root: PathBuf,
    commit: String,
}

/// Installs the plugin the source string names, telling `progress` as it
/// goes. Returns its record; the registry has been reloaded with it.
pub fn install(
    db: &Db,
    registry: &Registry,
    source: &str,
    options: Options,
    progress: &dyn Fn(Progress),
) -> Result<InstalledRecord, Error> {
    match Source::parse(
        source,
        options.reference.as_deref(),
        options.path.as_deref(),
    )? {
        Source::Folder(folder) => install_path(db, registry, &folder, options, progress),
        Source::Git {
            url,
            path,
            reference,
        } => {
            if options.link {
                return Err(Error::invalid(
                    "/source",
                    "a link needs a folder; clone the repository and link that",
                ));
            }
            progress(Progress::Step("fetching"));
            let fetched = fetch_git(registry, &url, reference.as_deref(), progress)?;
            let dir = match &path {
                Some(p) => fetched.root.join(p),
                None => fetched.root.clone(),
            };
            let origin = Origin {
                kind: "git",
                source: source.trim().to_string(),
                resolved: serde_json::json!({ "url": url, "path": path, "ref": reference })
                    .to_string(),
                commit: Some(fetched.commit.clone()),
            };
            let result = install_dir(db, registry, &dir, options, progress, origin);
            let _ = std::fs::remove_dir_all(&fetched.root);
            result
        }
        Source::Release { .. } => Err(Error::invalid(
            "/source",
            "installing from a GitHub release is not here yet; install the repository, or the folder",
        )),
    }
}

/// Where a record says it came from.
struct Origin {
    kind: &'static str,
    source: String,
    resolved: String,
    commit: Option<String>,
}

/// A shallow clone into the fetch directory, at the ref when given: a
/// branch or a tag by name, a commit by fetching it alone.
fn fetch_git(
    registry: &Registry,
    url: &str,
    reference: Option<&str>,
    progress: &dyn Fn(Progress),
) -> Result<Fetched, Error> {
    let root = fetch_dir(registry).join(format!(
        "git-{}",
        crate::id::next().trim_start_matches("r_")
    ));
    std::fs::create_dir_all(&root)?;
    let looks_like_commit =
        reference.is_some_and(|r| r.len() >= 7 && r.chars().all(|c| c.is_ascii_hexdigit()));
    let steps: Vec<Vec<String>> = if looks_like_commit {
        let commit = reference.unwrap();
        vec![
            vec!["init".into(), "-q".into()],
            vec!["remote".into(), "add".into(), "origin".into(), url.into()],
            vec![
                "fetch".into(),
                "-q".into(),
                "--depth".into(),
                "1".into(),
                "origin".into(),
                commit.into(),
            ],
            vec!["checkout".into(), "-q".into(), "FETCH_HEAD".into()],
        ]
    } else {
        let mut clone = vec!["clone".into(), "-q".into(), "--depth".into(), "1".into()];
        if let Some(r) = reference {
            clone.push("--branch".into());
            clone.push(r.into());
        }
        clone.push(url.into());
        clone.push(".".into());
        vec![clone]
    };
    for args in steps {
        progress(Progress::Log(format!("$ git {}", args.join(" "))));
        let output = Command::new("git")
            .args(&args)
            .current_dir(&root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| Error::invalid("/source", format!("git could not run: {e}")))?;
        for line in String::from_utf8_lossy(&output.stderr)
            .lines()
            .chain(String::from_utf8_lossy(&output.stdout).lines())
        {
            if !line.trim().is_empty() {
                progress(Progress::Log(line.to_string()));
            }
        }
        if !output.status.success() {
            let _ = std::fs::remove_dir_all(&root);
            let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(Error::invalid(
                "/source",
                format!(
                    "git {} failed: {}",
                    args.first().cloned().unwrap_or_default(),
                    said
                ),
            ));
        }
    }
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .ok_or_else(|| Error::invalid("/source", "the clone has no commit"))?;
    Ok(Fetched { root, commit })
}

fn fetch_dir(registry: &Registry) -> PathBuf {
    registry
        .store_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| registry.store_dir().to_path_buf())
        .join("fetch")
}

/// What is new for an installed plugin, asked of its source: `up_to_date`,
/// `available` with the newer commit, `pinned` for a tag or a commit that
/// never moves, or `unknown` when the source cannot be asked.
pub fn check_updates(registry: &Registry, record: &InstalledRecord) -> serde_json::Value {
    if record.linked {
        return serde_json::json!({ "state": "linked" });
    }
    match record.kind.as_str() {
        "git" => {
            let resolved: Value = serde_json::from_str(&record.resolved).unwrap_or(Value::Null);
            let url = resolved["url"].as_str().unwrap_or_default().to_string();
            let reference = resolved["ref"].as_str().map(str::to_string);
            if reference.as_deref().is_some_and(|r| r.len() >= 7 && r.chars().all(|c| c.is_ascii_hexdigit())) {
                return serde_json::json!({ "state": "pinned", "ref": reference });
            }
            let output = Command::new("git")
                .args(["ls-remote", &url, reference.as_deref().unwrap_or("HEAD")])
                .env("GIT_TERMINAL_PROMPT", "0")
                .output();
            let Ok(output) = output else {
                return serde_json::json!({ "state": "unknown", "message": "git could not run" });
            };
            if !output.status.success() {
                return serde_json::json!({ "state": "unknown", "message": String::from_utf8_lossy(&output.stderr).trim() });
            }
            let listing = String::from_utf8_lossy(&output.stdout);
            let mut tag = None;
            let mut head = None;
            for line in listing.lines() {
                let mut parts = line.split_whitespace();
                let (Some(sha), Some(name)) = (parts.next(), parts.next()) else { continue };
                if name.starts_with("refs/tags/") {
                    tag = Some(sha.to_string());
                } else if head.is_none() {
                    head = Some(sha.to_string());
                }
            }
            if tag.is_some() {
                return serde_json::json!({ "state": "pinned", "ref": reference });
            }
            match head {
                Some(sha) if Some(sha.as_str()) == record.commit.as_deref() => serde_json::json!({ "state": "up_to_date", "commit": sha }),
                Some(sha) => serde_json::json!({ "state": "available", "commit": sha, "installed": record.commit }),
                None => serde_json::json!({ "state": "unknown", "message": format!("{} has no such ref", reference.unwrap_or_else(|| "HEAD".into())) }),
            }
        }
        "path" => {
            let source = Path::new(&record.resolved);
            match (hash_dir(source), &record.hash) {
                (Ok(now), Some(then)) if &now == then => serde_json::json!({ "state": "up_to_date" }),
                (Ok(_), Some(_)) => serde_json::json!({ "state": "available", "message": "the folder changed since it was installed" }),
                _ => serde_json::json!({ "state": "unknown", "message": "the folder cannot be read" }),
            }
        }
        _ => serde_json::json!({ "state": "unknown" }),
    }
    .tap(|_| { let _ = registry; })
}

trait Tap: Sized {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}
impl Tap for Value {}

/// What an install says as it goes: the step it is at, and lines of the
/// build's output.
pub enum Progress {
    Step(&'static str),
    Log(String),
}

/// One install, followed by id: its step, its log, and how it ended.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub source: String,
    /// `fetching`, `inspecting`, `building`, `placing`, `done`, `failed`
    pub status: String,
    pub log: String,
    pub error: Option<String>,
    /// the plugin's row, once done
    pub plugin: Option<Value>,
}

impl Job {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "source": self.source,
            "status": self.status,
            "log": self.log,
            "error": self.error,
            "plugin": self.plugin,
        })
    }
}

/// The jobs the app has run, by id, for the dialog and the CLI to follow.
#[derive(Debug, Default)]
pub struct Jobs(Mutex<HashMap<String, Job>>);

impl Jobs {
    pub fn start(&self, source: &str) -> String {
        let id = crate::id::next().replace("r_", "j_");
        self.0.lock().unwrap().insert(
            id.clone(),
            Job {
                id: id.clone(),
                source: source.to_string(),
                status: "fetching".into(),
                log: String::new(),
                error: None,
                plugin: None,
            },
        );
        id
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.0.lock().unwrap().get(id).cloned()
    }

    pub fn note(&self, id: &str, progress: Progress) {
        let mut jobs = self.0.lock().unwrap();
        let Some(job) = jobs.get_mut(id) else { return };
        match progress {
            Progress::Step(step) => job.status = step.to_string(),
            Progress::Log(line) => {
                job.log.push_str(&line);
                job.log.push('\n');
            }
        }
    }

    pub fn finish(&self, id: &str, outcome: Result<Value, Error>) {
        let mut jobs = self.0.lock().unwrap();
        let Some(job) = jobs.get_mut(id) else { return };
        match outcome {
            Ok(plugin) => {
                job.status = "done".into();
                job.plugin = Some(plugin);
            }
            Err(error) => {
                job.status = "failed".into();
                job.error = Some(error.to_string());
            }
        }
    }
}

/// Installs the plugin at `source`, telling `progress` as it goes. Returns
/// its record; the registry has been reloaded with it.
pub fn install_path(
    db: &Db,
    registry: &Registry,
    source: &Path,
    options: Options,
    progress: &dyn Fn(Progress),
) -> Result<InstalledRecord, Error> {
    let dir = std::path::absolute(source)?;
    let origin = Origin {
        kind: "path",
        source: source.display().to_string(),
        resolved: dir.display().to_string(),
        commit: None,
    };
    install_dir(db, registry, &dir, options, progress, origin)
}

/// Installs the plugin in `dir`, wherever it was fetched from.
fn install_dir(
    db: &Db,
    registry: &Registry,
    dir: &Path,
    options: Options,
    progress: &dyn Fn(Progress),
    origin: Origin,
) -> Result<InstalledRecord, Error> {
    progress(Progress::Step("inspecting"));
    let dir = std::path::absolute(dir)?;
    if !dir.is_dir() {
        return Err(Error::invalid(
            "/source",
            format!("{} is not a directory", dir.display()),
        ));
    }
    let manifest = read_manifest(&dir)?;
    let build = build_command(&manifest)?;

    // a link serves the folder as it is; what the folder has to be, the
    // registry says when it loads it
    if options.link {
        let plugin = Plugin::load(&dir);
        if let Some(why) = &plugin.error {
            return Err(Error::invalid("/source", format!("not a plugin: {why}")));
        }
        let record = record_for(
            &plugin,
            &origin,
            None,
            None,
            true,
            dir.display().to_string(),
        );
        return commit(db, registry, record);
    }

    // build in a scratch copy, so the source is never written to
    let (staged, log_path) = match &build {
        Some(command) => {
            progress(Progress::Step("building"));
            let scratch = scratch_dir(registry, &manifest);
            copy_tree(&dir, &scratch, &[".git", "node_modules"])?;
            let log_path = run_build(registry, &manifest, &scratch, command, progress)?;
            (scratch, Some(log_path))
        }
        None => (dir.clone(), None),
    };
    let plugin = Plugin::load(&staged);
    if let Some(why) = &plugin.error {
        let _ = build.as_ref().map(|_| std::fs::remove_dir_all(&staged));
        return Err(Error::invalid(
            "/source",
            match build {
                Some(_) => format!("after the build, not a plugin: {why}"),
                None => format!(
                    "not a plugin: {why}; a source that needs building declares its build in the manifest"
                ),
            },
        ));
    }
    if let Some(refusal) = older_than_installed(db, &plugin, options.force)? {
        let _ = build.as_ref().map(|_| std::fs::remove_dir_all(&staged));
        return Err(refusal);
    }

    progress(Progress::Step("placing"));
    let entry = place(registry, &plugin, &staged)?;
    if build.is_some() {
        let _ = std::fs::remove_dir_all(&staged);
    }
    let hash = hash_dir(&entry)?;
    let record = record_for(
        &plugin,
        &origin,
        Some(hash),
        log_path.map(|p| p.display().to_string()),
        false,
        entry.display().to_string(),
    );
    commit(db, registry, record)
}

fn read_manifest(dir: &Path) -> Result<Map<String, Value>, Error> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| {
        Error::invalid(
            "/source",
            format!("not a plugin: cannot read manifest.json ({e})"),
        )
    })?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(Error::invalid(
            "/source",
            "not a plugin: manifest.json is not a JSON object",
        )),
    }
}

/// The manifest's `build.command`, when it declares one.
fn build_command(manifest: &Map<String, Value>) -> Result<Option<String>, Error> {
    match manifest.get("build") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(build)) => match build.get("command").and_then(Value::as_str) {
            Some(command) if !command.trim().is_empty() => Ok(Some(command.trim().to_string())),
            _ => Err(Error::invalid(
                "/source",
                "the manifest's build needs a command",
            )),
        },
        Some(_) => Err(Error::invalid(
            "/source",
            "the manifest's build must be an object with a command",
        )),
    }
}

fn scratch_dir(registry: &Registry, manifest: &Map<String, Value>) -> PathBuf {
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("plugin");
    registry
        .store_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| registry.store_dir().to_path_buf())
        .join("fetch")
        .join(format!(
            "{name}-{}",
            crate::id::next().trim_start_matches("r_")
        ))
}

/// Runs the build command through the shell in the scratch copy, its
/// output going to the log and to `progress` line by line. A non-zero exit
/// stops the install with the tail of the log.
fn run_build(
    registry: &Registry,
    manifest: &Map<String, Value>,
    scratch: &Path,
    command: &str,
    progress: &dyn Fn(Progress),
) -> Result<PathBuf, Error> {
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("plugin");
    let logs = registry
        .store_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| registry.store_dir().to_path_buf())
        .join("logs");
    std::fs::create_dir_all(&logs)?;
    let log_path = logs.join(format!("{name}-{}.log", Utc::now().format("%Y%m%dT%H%M%S")));
    let mut log = std::fs::File::create(&log_path)?;
    use std::io::Write;
    writeln!(log, "$ {command}")?;
    progress(Progress::Log(format!("$ {command}")));

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(format!("{command} 2>&1"))
        .current_dir(scratch)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::invalid("/source", format!("the build could not start: {e}")))?;
    let mut tail: Vec<String> = Vec::new();
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines() {
            let line = line.unwrap_or_default();
            writeln!(log, "{line}")?;
            if tail.len() == 20 {
                tail.remove(0);
            }
            tail.push(line.clone());
            progress(Progress::Log(line));
        }
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(Error::invalid(
            "/source",
            format!(
                "the build failed ({status}); the log is at {}\n{}",
                log_path.display(),
                tail.join("\n")
            ),
        ));
    }
    Ok(log_path)
}

/// Whether the same line already holds a newer version.
fn older_than_installed(db: &Db, plugin: &Plugin, force: bool) -> Result<Option<Error>, Error> {
    let records = db.installed_plugins()?;
    let current = records.iter().find(|r| r.name == plugin.name);
    if let Some(current) = current
        && !current.linked
        && current.major == plugin.version as i64
        && semver(&plugin.release) < semver(&current.version)
        && !force
    {
        return Ok(Some(Error::invalid(
            "/source",
            format!(
                "{} {} is older than the installed {}; pass force to replace it",
                plugin.name, plugin.release, current.version
            ),
        )));
    }
    Ok(None)
}

fn record_for(
    plugin: &Plugin,
    origin: &Origin,
    hash: Option<String>,
    build_log: Option<String>,
    linked: bool,
    path: String,
) -> InstalledRecord {
    InstalledRecord {
        name: plugin.name.clone(),
        version: plugin.release.clone(),
        major: plugin.version as i64,
        kind: origin.kind.into(),
        source: origin.source.clone(),
        resolved: origin.resolved.clone(),
        commit: origin.commit.clone(),
        asset_hash: None,
        hash,
        build_log,
        installed_at: crate::review::iso(Utc::now()),
        linked,
        path,
    }
}

/// Writes the record, drops a previous line no review renders from, and
/// reloads the registry.
fn commit(db: &Db, registry: &Registry, record: InstalledRecord) -> Result<InstalledRecord, Error> {
    let previous = db
        .installed_plugins()?
        .into_iter()
        .find(|r| r.name == record.name);
    if let Some(previous) = previous
        && !previous.linked
        && (previous.major != record.major || record.linked)
        && !db.reviews_use(&previous.name, previous.major as u32)?
    {
        let _ = std::fs::remove_dir_all(registry.store_entry(&previous.name, previous.major));
    }
    db.upsert_installed(&record)?;
    let records = db.installed_plugins()?;
    registry
        .reload_with(records)
        .map_err(|message| Error::invalid("/source", message))?;
    Ok(record)
}

/// What never enters the store: sources and the tooling that builds them.
/// The bundle is what is left.
const NOT_IN_THE_BUNDLE: &[&str] = &[
    "node_modules",
    "src",
    "tests",
    "test",
    "fixtures",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lockb",
    "tsconfig.json",
];

fn in_the_bundle(name: &str) -> bool {
    !name.starts_with('.')
        && !NOT_IN_THE_BUNDLE.contains(&name)
        && !name.starts_with("tsconfig.")
        && !name.starts_with("vite.config.")
        && !name.starts_with("vitest.config.")
        && !name.starts_with("playwright.config.")
}

/// Copies the plugin's bundle into the store entry for its line, whole or
/// not at all: the copy lands beside the entry and takes its place with
/// one rename.
fn place(registry: &Registry, plugin: &Plugin, dir: &Path) -> Result<PathBuf, Error> {
    let entry = registry.store_entry(&plugin.name, plugin.version as i64);
    let staging = entry.with_extension("staging");
    let _ = std::fs::remove_dir_all(&staging);
    if let Some(parent) = entry.parent() {
        std::fs::create_dir_all(parent)?;
    }
    copy_bundle(dir, &staging)?;
    if entry.exists() {
        let old = entry.with_extension("old");
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&entry, &old)?;
        std::fs::rename(&staging, &entry)?;
        let _ = std::fs::remove_dir_all(&old);
    } else {
        std::fs::rename(&staging, &entry)?;
    }
    Ok(entry)
}

fn copy_bundle(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if !in_the_bundle(&name.to_string_lossy()) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_bundle(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A copy of a source tree for building in, without the names given.
fn copy_tree(from: &Path, to: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if skip.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target, skip)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// "1.2.3" as something that orders; anything else sorts first.
pub fn semver(text: &str) -> (u64, u64, u64) {
    let mut parts = text.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
