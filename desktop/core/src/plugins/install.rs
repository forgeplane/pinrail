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
//! `#ref=` in its fragment, or a GitHub release URL. It is parsed before
//! anything is touched, so a bad one fails at
//! once and offline.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::Utc;
use serde_json::{Map, Value};

use super::jobs::Progress;
use super::registry::hash_dir;
use crate::db::{Db, InstalledRecord};
use crate::error::Error;
use crate::plugins::{Plugin, Registry};

#[derive(Debug, Default, Clone)]
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
            ["releases"] | ["releases", "latest"] => Some(Source::Release {
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
    let prepared = prepare(registry, source, &options, progress)?;
    let scratch = prepared.scratch.clone();
    let result = install_dir(
        db,
        registry,
        &prepared.dir,
        options,
        progress,
        prepared.origin,
    );
    if let Some(scratch) = scratch {
        let _ = std::fs::remove_dir_all(scratch);
    }
    result
}

/// The source an installed plugin came from, as an install takes it: the
/// path, or the URL with its ref and folder, or the release's page.
pub fn source_of(record: &InstalledRecord) -> (String, Options) {
    let resolved: Value = serde_json::from_str(&record.resolved).unwrap_or(Value::Null);
    match record.kind.as_str() {
        "git" => (
            resolved["url"]
                .as_str()
                .unwrap_or(&record.source)
                .to_string(),
            Options {
                reference: resolved["ref"].as_str().map(str::to_string),
                path: resolved["path"].as_str().map(str::to_string),
                ..Options::default()
            },
        ),
        "release" => (
            match (resolved["owner"].as_str(), resolved["repo"].as_str()) {
                (Some(owner), Some(repo)) => {
                    let page = format!("https://github.com/{owner}/{repo}/releases");
                    match (resolved["pinned"].as_bool(), resolved["tag"].as_str()) {
                        (Some(true), Some(tag)) => format!("{page}/tag/{tag}"),
                        _ => page,
                    }
                }
                _ => record.source.clone(),
            },
            Options::default(),
        ),
        _ => (
            record.resolved.clone(),
            Options {
                link: record.linked,
                ..Options::default()
            },
        ),
    }
}

/// Removes an installed plugin: its record, and its store entries no
/// review renders from. An entry a review still uses stays, and the
/// answer says which. A link loses only its record.
pub fn remove(db: &Db, registry: &Registry, name: &str) -> Result<Value, Error> {
    let record = db
        .installed_plugins()?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or_else(|| Error::NotFound(format!("plugin {name}")))?;
    let mut kept: Vec<i64> = Vec::new();
    let mut removed: Vec<i64> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(registry.store_dir().join(name)) {
        for entry in entries.flatten() {
            let Ok(major) = entry.file_name().to_string_lossy().parse::<i64>() else {
                continue;
            };
            if db.reviews_use(name, major as u32)? {
                kept.push(major);
            } else {
                std::fs::remove_dir_all(entry.path())?;
                removed.push(major);
            }
        }
    }
    if kept.is_empty() {
        let _ = std::fs::remove_dir(registry.store_dir().join(name));
    }
    db.remove_installed(name)?;
    let records = db.installed_plugins()?;
    registry
        .reload_with(records)
        .map_err(|message| Error::invalid("/name", message))?;
    kept.sort_unstable();
    removed.sort_unstable();
    Ok(serde_json::json!({
        "removed": name,
        "linked": record.linked,
        "version": record.version,
        "entries_removed": removed,
        "entries_kept": kept,
    }))
}

/// What installing `source` would do, without doing it: the plugin the
/// manifest describes, where it comes from, whether a build runs and what
/// it executes, and what is installed under that name already. Fetches
/// the source the way an install does and drops it afterwards.
pub fn inspect(
    db: &Db,
    registry: &Registry,
    source: &str,
    options: Options,
    progress: &dyn Fn(Progress),
) -> Result<Value, Error> {
    let prepared = prepare(registry, source, &options, progress)?;
    let summary = summarize(db, &prepared, &options);
    if let Some(scratch) = &prepared.scratch {
        let _ = std::fs::remove_dir_all(scratch);
    }
    summary
}

fn summarize(db: &Db, prepared: &Prepared, options: &Options) -> Result<Value, Error> {
    let dir = &prepared.dir;
    if !dir.is_dir() {
        return Err(Error::invalid(
            "/source",
            format!("{} is not a directory", dir.display()),
        ));
    }
    let manifest = read_manifest(dir)?;
    let build = if prepared.origin.build {
        build_command(&manifest)?
    } else {
        None
    };
    // what the registry would say of the folder as it is; a source that
    // builds first is judged after the build
    let plugin = Plugin::load(dir);
    if let Some(why) = &plugin.error
        && (build.is_none() || options.link)
    {
        return Err(Error::invalid(
            "/source",
            not_a_plugin(why, !options.link && prepared.origin.kind != "release"),
        ));
    }
    let (version, major) = manifest
        .get("version")
        .and_then(crate::plugins::version_of)
        .filter(|(release, _)| release != "0.0.0")
        .ok_or_else(|| {
            Error::invalid(
                "/source",
                "not a plugin: version is required: a semantic version like \"1.2.0\"",
            )
        })?;
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| super::manifest::valid_name(n))
        .ok_or_else(|| Error::invalid("/source", "not a plugin: name is required"))?
        .to_string();
    builtin_name(&name)?;
    // what is installed under the name, and whether this source is the
    // very thing that was installed: the same files from a folder, the
    // same commit, the same asset
    let installed = db
        .installed_plugins()?
        .into_iter()
        .find(|r| r.name == name)
        .map(|r| {
            let unchanged = !r.linked
                && r.version == version
                && match prepared.origin.kind {
                    "path" => {
                        build.is_none()
                            && r.hash.is_some()
                            && super::registry::hash_dir_where(dir, &in_the_bundle).ok() == r.hash
                    }
                    "git" => r.commit.is_some() && r.commit == prepared.origin.commit,
                    _ => r.asset_hash.is_some() && r.asset_hash == prepared.origin.asset_hash,
                };
            serde_json::json!({
                "version": r.version, "major": r.major, "linked": r.linked, "kind": r.kind,
                "path": r.path, "unchanged": unchanged,
            })
        });
    let older = installed.as_ref().is_some_and(|r| {
        !r["linked"].as_bool().unwrap_or(false)
            && r["major"].as_i64() == Some(major)
            && semver(&version) < semver(r["version"].as_str().unwrap_or_default())
    });
    let resolved: Value = serde_json::from_str(&prepared.origin.resolved)
        .unwrap_or_else(|_| Value::String(prepared.origin.resolved.clone()));
    Ok(serde_json::json!({
        "source": prepared.origin.source,
        "link": options.link,
        "name": name,
        "version": version,
        "major": major,
        "title": manifest.get("title").and_then(Value::as_str).unwrap_or(&name),
        // the icon's markup, as the app shows an installed plugin's
        "icon": manifest
            .get("icon")
            .and_then(Value::as_str)
            .and_then(|file| super::manifest::icon_markup(&prepared.dir, file).ok()),
        "entry": manifest.get("entry").and_then(Value::as_str).unwrap_or("index.html"),
        "build": build,
        // the files it takes beside a payload, for the dialog to say before the yes
        "attachments": manifest.get("attachments"),
        "origin": { "kind": prepared.origin.kind, "resolved": resolved, "commit": prepared.origin.commit },
        "installed": installed,
        "older": older,
    }))
}

/// A source fetched and ready: the folder holding the manifest, where it
/// came from, and the scratch to drop when done with it.
struct Prepared {
    scratch: Option<PathBuf>,
    dir: PathBuf,
    origin: Origin,
}

fn prepare(
    registry: &Registry,
    source: &str,
    options: &Options,
    progress: &dyn Fn(Progress),
) -> Result<Prepared, Error> {
    match Source::parse(
        source,
        options.reference.as_deref(),
        options.path.as_deref(),
    )? {
        Source::Folder(folder) => {
            let dir = std::path::absolute(&folder)?;
            Ok(Prepared {
                scratch: None,
                origin: Origin {
                    kind: "path",
                    source: folder.display().to_string(),
                    resolved: dir.display().to_string(),
                    commit: None,
                    asset_hash: None,
                    build: true,
                },
                dir,
            })
        }
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
            Ok(Prepared {
                scratch: Some(fetched.root.clone()),
                dir,
                origin: Origin {
                    kind: "git",
                    source: source.trim().to_string(),
                    resolved: serde_json::json!({ "url": url, "path": path, "ref": reference })
                        .to_string(),
                    commit: Some(fetched.commit.clone()),
                    asset_hash: None,
                    build: true,
                },
            })
        }
        Source::Release { owner, repo, tag } => {
            if options.link {
                return Err(Error::invalid(
                    "/source",
                    "a link needs a folder; a release is a bundle",
                ));
            }
            progress(Progress::Step("fetching"));
            let release = fetch_release(registry, &owner, &repo, tag.as_deref(), progress)?;
            // the tag and the manifest must agree on the version
            let declared = read_manifest(&release.root)
                .ok()
                .and_then(|m| m.get("version").and_then(crate::plugins::version_of))
                .map(|(v, _)| v);
            let tagged = tag_version(&release.tag).map(|(_, version)| version);
            if declared.as_deref() != tagged {
                let _ = std::fs::remove_dir_all(&release.scratch);
                return Err(Error::invalid(
                    "/source",
                    format!(
                        "the release is tagged {} but its manifest says version {}",
                        release.tag,
                        declared.unwrap_or_else(|| "nothing".into())
                    ),
                ));
            }
            Ok(Prepared {
                scratch: Some(release.scratch.clone()),
                dir: release.root.clone(),
                origin: Origin {
                    kind: "release",
                    source: source.trim().to_string(),
                    resolved: serde_json::json!({
                        // a tag of one plugin's series is followed, not pinned
                        "owner": owner, "repo": repo, "tag": release.tag,
                        "pinned": tag.as_deref().is_some_and(|t| tag_version(t).is_none_or(|(series, _)| series.is_empty())),
                        "asset": release.asset_name, "asset_url": release.asset_url,
                        "asset_size": release.asset_size,
                    })
                    .to_string(),
                    commit: None,
                    asset_hash: Some(release.asset_hash.clone()),
                    build: false,
                },
            })
        }
    }
}

/// Where a record says it came from.
struct Origin {
    kind: &'static str,
    source: String,
    resolved: String,
    commit: Option<String>,
    asset_hash: Option<String>,
    /// whether a build the manifest declares runs: never for a release,
    /// whose asset is the bundle already
    build: bool,
}

/// What fetching a release produced: the unpacked bundle and its asset.
struct FetchedRelease {
    scratch: PathBuf,
    root: PathBuf,
    tag: String,
    asset_name: String,
    asset_url: String,
    asset_size: u64,
    asset_hash: String,
}

/// The most an asset may weigh.
const ASSET_LIMIT: u64 = 200 * 1024 * 1024;

/// One request to the releases API, the asset that is the bundle downloaded
/// and hashed, the archive unpacked. The bundle's manifest sits at the
/// archive's root, or in the single folder at its root.
fn fetch_release(
    registry: &Registry,
    owner: &str,
    repo: &str,
    tag: Option<&str>,
    progress: &dyn Fn(Progress),
) -> Result<FetchedRelease, Error> {
    let api = format!(
        "{}/repos/{owner}/{repo}/releases/{}",
        registry.github_api(),
        match tag {
            Some(t) => format!("tags/{t}"),
            None => "latest".into(),
        }
    );
    progress(Progress::Log(format!("GET {api}")));
    let release: Value = github_get(&api)?
        .body_mut()
        .read_json()
        .map_err(|e| Error::invalid("/source", format!("GitHub's answer is not a release: {e}")))?;
    let tag_name = release["tag_name"]
        .as_str()
        .ok_or_else(|| Error::invalid("/source", "GitHub's answer is not a release: no tag_name"))?
        .to_string();
    let assets: Vec<(String, String, u64)> = release["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some((
                        a["name"].as_str()?.to_string(),
                        a["browser_download_url"].as_str()?.to_string(),
                        a["size"].as_u64().unwrap_or(0),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let zips: Vec<&(String, String, u64)> = assets
        .iter()
        .filter(|(n, _, _)| n.ends_with(".zip"))
        .collect();
    let (asset_name, asset_url, size) = zips
        .iter()
        .find(|(n, _, _)| n == "pinrail-plugin.zip")
        .copied()
        .or_else(|| (zips.len() == 1).then(|| zips[0]))
        .cloned()
        .ok_or_else(|| {
            let names = if assets.is_empty() {
                "none".to_string()
            } else {
                assets.iter().map(|(n, _, _)| n.as_str()).collect::<Vec<_>>().join(", ")
            };
            Error::invalid(
                "/source",
                format!(
                    "the release {tag_name} needs one .zip asset that is the bundle, or one named pinrail-plugin.zip; it has: {names}"
                ),
            )
        })?;
    if size > ASSET_LIMIT {
        return Err(Error::invalid(
            "/source",
            format!("{asset_name} is {size} bytes, more than the {ASSET_LIMIT} allowed"),
        ));
    }
    progress(Progress::Log(format!(
        "downloading {asset_name} ({size} bytes)"
    )));
    let bytes = github_get(&asset_url)?
        .body_mut()
        .with_config()
        .limit(ASSET_LIMIT)
        .read_to_vec()
        .map_err(|e| Error::invalid("/source", format!("downloading {asset_name} failed: {e}")))?;
    let asset_hash = {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(&bytes))
    };

    let scratch = fetch_dir(registry).join(format!(
        "release-{}",
        crate::id::next().trim_start_matches("r_")
    ));
    let tree = scratch.join("tree");
    std::fs::create_dir_all(&tree)?;
    std::fs::write(scratch.join(&asset_name), &bytes)?;
    progress(Progress::Log(format!("unpacking {asset_name}")));
    let unpacked = unzip(&bytes, &tree).and_then(|_| {
        // the manifest at the root, or inside the one folder at the root
        if tree.join("manifest.json").is_file() {
            return Ok(tree.clone());
        }
        let entries: Vec<PathBuf> = std::fs::read_dir(&tree)?
            .flatten()
            .map(|e| e.path())
            .collect();
        match entries.as_slice() {
            [only] if only.is_dir() && only.join("manifest.json").is_file() => Ok(only.clone()),
            _ => Err(Error::invalid(
                "/source",
                format!("{asset_name} has no manifest.json at its root"),
            )),
        }
    });
    let root = match unpacked {
        Ok(root) => root,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(match e {
                Error::Invalid(_) => e,
                other => Error::invalid("/source", format!("{asset_name}: {other}")),
            });
        }
    };
    Ok(FetchedRelease {
        scratch,
        root,
        tag: tag_name,
        asset_name,
        asset_url,
        asset_size: size,
        asset_hash,
    })
}

/// The archive's entries under `into`; an entry that would leave the folder
/// fails the whole thing.
fn unzip(bytes: &[u8], into: &Path) -> Result<(), Error> {
    unzip_within(bytes, into, UNPACKING)
}

/// How much a release may unpack to. A small archive can expand to far more
/// than its download, enough to fill the disk that holds the person's data.
#[derive(Debug, Clone, Copy)]
struct Unpacking {
    bytes: u64,
    entries: usize,
}

const UNPACKING: Unpacking = Unpacking {
    bytes: 500 * 1024 * 1024,
    entries: 20_000,
};

fn unzip_within(bytes: &[u8], into: &Path, limits: Unpacking) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| Error::invalid("/source", format!("not a zip archive: {e}")))?;
    if archive.len() > limits.entries {
        return Err(Error::invalid(
            "/source",
            format!(
                "the archive has {} entries, more than the {} allowed",
                archive.len(),
                limits.entries
            ),
        ));
    }
    let mut left = limits.bytes;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| Error::invalid("/source", format!("bad zip entry: {e}")))?;
        let Some(relative) = file.enclosed_name() else {
            return Err(Error::invalid(
                "/source",
                "the archive has an entry that leaves the archive",
            ));
        };
        let target = into.join(relative);
        if file.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)?;
        // counted as written, not as the entry's header says
        let written = std::io::copy(&mut std::io::Read::take(&mut file, left + 1), &mut out)?;
        if written > left {
            return Err(Error::invalid(
                "/source",
                format!(
                    "the archive unpacks to more than the {} MB allowed",
                    limits.bytes / (1024 * 1024)
                ),
            ));
        }
        left -= written;
    }
    Ok(())
}

fn github_get(url: &str) -> Result<ureq::http::Response<ureq::Body>, Error> {
    ureq::get(url)
        .header("accept", "application/vnd.github+json")
        .header("user-agent", "pinrail")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(code) => {
                Error::invalid("/source", format!("GitHub answered {code} for {url}"))
            }
            other => Error::invalid("/source", format!("{url}: {other}")),
        })
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

/// How many build logs a plugin keeps; older ones go when a new one is written.
const LOGS_KEPT: usize = 5;

/// Tidies `<data>/plugins` at start. The folder holds `store`, `fetch`
/// and `logs`; anything else at its top level is a snapshot from before
/// the store existed and goes, since the store or a link has what it
/// held. `fetch` is scratch and no install survives a restart, so it is
/// emptied.
pub fn tidy(plugins_dir: &Path) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        match name.to_str() {
            Some("store") | Some("logs") => {}
            Some("fetch") => {
                for leftover in std::fs::read_dir(entry.path())?.flatten() {
                    let path = leftover.path();
                    if path.is_dir() {
                        std::fs::remove_dir_all(&path)?;
                    } else {
                        std::fs::remove_file(&path)?;
                    }
                }
            }
            _ => {
                let path = entry.path();
                if path.is_dir() {
                    std::fs::remove_dir_all(&path)?;
                } else {
                    std::fs::remove_file(&path)?;
                }
            }
        }
    }
    Ok(())
}

/// Keeps the last `LOGS_KEPT` logs of a plugin, by name.
fn trim_logs(logs: &Path, name: &str) {
    let Ok(entries) = std::fs::read_dir(logs) else {
        return;
    };
    let prefix = format!("{name}-");
    let mut mine: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|f| f.to_str())
                .is_some_and(|f| f.starts_with(&prefix) && f.ends_with(".log"))
        })
        .collect();
    mine.sort();
    while mine.len() > LOGS_KEPT {
        let _ = std::fs::remove_file(mine.remove(0));
    }
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
            if reference
                .as_deref()
                .is_some_and(|r| r.len() >= 7 && r.chars().all(|c| c.is_ascii_hexdigit()))
            {
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
                let (Some(sha), Some(name)) = (parts.next(), parts.next()) else {
                    continue;
                };
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
                Some(sha) if Some(sha.as_str()) == record.commit.as_deref() => {
                    serde_json::json!({ "state": "up_to_date", "commit": sha })
                }
                Some(sha) => {
                    serde_json::json!({ "state": "available", "commit": sha, "installed": record.commit })
                }
                None => {
                    serde_json::json!({ "state": "unknown", "message": format!("{} has no such ref", reference.unwrap_or_else(|| "HEAD".into())) })
                }
            }
        }
        "release" => {
            let resolved: Value = serde_json::from_str(&record.resolved).unwrap_or(Value::Null);
            if resolved["pinned"].as_bool().unwrap_or(false) {
                return serde_json::json!({ "state": "pinned", "tag": resolved["tag"] });
            }
            let owner = resolved["owner"].as_str().unwrap_or_default();
            let repo = resolved["repo"].as_str().unwrap_or_default();
            let read = |url: String| {
                github_get(&url).and_then(|mut r| {
                    r.body_mut()
                        .read_json::<Value>()
                        .map_err(|e| Error::Internal(e.to_string()))
                })
            };
            let series = resolved["tag"]
                .as_str()
                .and_then(tag_version)
                .map(|(series, _)| series.to_string())
                .unwrap_or_default();
            let latest = if series.is_empty() {
                read(format!(
                    "{}/repos/{owner}/{repo}/releases/latest",
                    registry.github_api()
                ))
            } else {
                // the repository's latest release may be another plugin's or
                // the repository's own: the newest of this plugin's series
                read(format!(
                    "{}/repos/{owner}/{repo}/releases?per_page=100",
                    registry.github_api()
                ))
                .map(|all| {
                    all.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| {
                            let tag = r["tag_name"].as_str()?;
                            let (s, version) = tag_version(tag)?;
                            (s == series).then(|| (semver(version), r.clone()))
                        })
                        .max_by_key(|(version, _)| *version)
                        .map(|(_, r)| r)
                        .unwrap_or(Value::Null)
                })
            };
            match latest {
                Ok(v) => {
                    let tag = v["tag_name"].as_str().unwrap_or_default().to_string();
                    let version = tag_version(&tag)
                        .map(|(_, version)| version.to_string())
                        .unwrap_or_default();
                    if semver(&version) > semver(&record.version) {
                        serde_json::json!({ "state": "available", "tag": tag, "version": version, "installed": record.version })
                    } else {
                        serde_json::json!({ "state": "up_to_date", "tag": tag })
                    }
                }
                Err(e) => serde_json::json!({ "state": "unknown", "message": e.to_string() }),
            }
        }
        "path" => {
            // the folder as the bundle would be copied from it, against the
            // bundle that was
            let source = Path::new(&record.resolved);
            match (
                super::registry::hash_dir_where(source, &in_the_bundle),
                &record.hash,
            ) {
                (Ok(now), Some(then)) if &now == then => {
                    serde_json::json!({ "state": "up_to_date" })
                }
                (Ok(_), Some(_)) => {
                    serde_json::json!({ "state": "available", "message": "the folder changed since it was installed" })
                }
                _ => {
                    serde_json::json!({ "state": "unknown", "message": "the folder cannot be read" })
                }
            }
        }
        _ => serde_json::json!({ "state": "unknown" }),
    }
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
    if let Some(name) = manifest.get("name").and_then(Value::as_str) {
        builtin_name(name)?;
    }
    let build = if origin.build {
        build_command(&manifest)?
    } else {
        None
    };

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
                None => not_a_plugin(why, true),
            },
        ));
    }
    if let Some(refusal) = older_than_installed(db, &plugin, options.force)? {
        let _ = build.as_ref().map(|_| std::fs::remove_dir_all(&staged));
        return Err(refusal);
    }
    // a link could point anywhere on the machine, and a copy would carry
    // what it points to into the store
    if let Some(link) = first_link(&staged)? {
        let _ = build.as_ref().map(|_| std::fs::remove_dir_all(&staged));
        return Err(Error::invalid(
            "/source",
            format!(
                "the plugin contains a symbolic link, which Pinrail does not install: {}",
                link.strip_prefix(&staged).unwrap_or(&link).display()
            ),
        ));
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

/// A built-in ships in the binary and is written out at every start, so an
/// installed plugin of the same name would never be the one served.
fn builtin_name(name: &str) -> Result<(), Error> {
    if super::registry::is_builtin(name) {
        return Err(Error::invalid(
            "/source",
            format!("{name} ships with Pinrail and cannot be installed over"),
        ));
    }
    Ok(())
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

/// Why a folder is not a plugin. When its entry is missing and a build could
/// have written it, say how to declare one; any other reason stands alone.
fn not_a_plugin(why: &str, could_build: bool) -> String {
    let entry_missing = why.starts_with("entry ") && why.ends_with(" not found");
    if could_build && entry_missing {
        format!(
            "not a plugin: {why}; a source that needs building declares its build in the manifest"
        )
    } else {
        format!("not a plugin: {why}")
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
    let log_path = logs.join(format!(
        "{name}-{}.log",
        Utc::now().format("%Y%m%dT%H%M%S%.3f")
    ));
    let mut log = std::fs::File::create(&log_path)?;
    trim_logs(&logs, name);
    use std::io::Write;
    writeln!(log, "$ {command}")?;
    progress(Progress::Log(format!("$ {command}")));

    // stderr joins stdout for the whole command, every step of it, and on a
    // line of its own, so nothing the command says can undo it; nothing is
    // there to answer a prompt, so one fails at once rather than waiting
    let mut shell = Command::new("sh");
    shell
        .arg("-c")
        .arg(format!("exec 2>&1\n{command}"))
        .current_dir(scratch)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // its own process group, so a stop reaches everything it started
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut shell, 0);
    let mut child = shell
        .spawn()
        .map_err(|e| Error::invalid("/source", format!("the build could not start: {e}")))?;
    let timeout = registry.build_timeout();
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watchdog = {
        let (finished, stopped, pid) = (finished.clone(), stopped.clone(), child.id());
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            while !finished.load(std::sync::atomic::Ordering::SeqCst) {
                if start.elapsed() >= timeout {
                    stopped.store(true, std::sync::atomic::Ordering::SeqCst);
                    stop_group(pid);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
    };
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
    let status = child.wait();
    finished.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = watchdog.join();
    let status = status?;
    if stopped.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(Error::invalid(
            "/source",
            format!(
                "the build did not finish within {} and was stopped; the log is at {}\n{}",
                minutes(timeout),
                log_path.display(),
                tail.join("\n")
            ),
        ));
    }
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

/// Ends a build and everything it started: the build runs in a process
/// group of its own, led by the shell.
fn stop_group(pid: u32) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .status();
    #[cfg(not(unix))]
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .status();
}

/// A duration as the person reads it: minutes, or seconds when shorter.
fn minutes(duration: std::time::Duration) -> String {
    match duration.as_secs() {
        s if s >= 60 && s % 60 == 0 => format!("{} minutes", s / 60),
        s => format!("{s} seconds"),
    }
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
        asset_hash: origin.asset_hash.clone(),
        hash,
        build_log,
        installed_at: crate::reviews::iso(Utc::now()),
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
    if let Some(previous) = &previous
        && !previous.linked
        && (previous.major != record.major || record.linked)
        && !db.reviews_use(&previous.name, previous.major as u32)?
    {
        let _ = std::fs::remove_dir_all(registry.store_entry(&previous.name, previous.major));
    }
    db.upsert_installed(&record)?;
    if let Err(message) = registry.reload_with(db.installed_plugins()?) {
        // The record is written before the registry takes it, so a plugin
        // the registry refuses must be taken out again: left there, it
        // would come back at the next start and be refused for ever.
        match &previous {
            Some(previous) => db.upsert_installed(previous)?,
            None => {
                db.remove_installed(&record.name)?;
            }
        }
        let _ = registry.reload_with(db.installed_plugins()?);
        return Err(Error::invalid("/source", message));
    }
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

/// Whether a file or folder of this name belongs to a plugin's bundle, at
/// any depth: what an install copies, and what is served for a linked one.
pub(crate) fn in_the_bundle(name: &str) -> bool {
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

/// The first symbolic link among the files a copy of the plugin would hold.
fn first_link(dir: &Path) -> std::io::Result<Option<PathBuf>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !in_the_bundle(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Ok(Some(entry.path()));
        }
        if kind.is_dir()
            && let Some(link) = first_link(&entry.path())?
        {
            return Ok(Some(link));
        }
    }
    Ok(None)
}

fn copy_bundle(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if !in_the_bundle(&name.to_string_lossy()) {
            continue;
        }
        // refused before the copy; never followed here either
        if entry.file_type()?.is_symlink() {
            return Err(std::io::Error::other(format!(
                "symbolic link {}",
                entry.path().display()
            )));
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
/// A release tag's series and version. `1.2.0` and `v1.2.0` are the
/// repository's own series; `plugin-review-v1.2.0` is one plugin's series in
/// a repository that releases several. None when what follows the last `v`
/// is not a semantic version.
fn tag_version(tag: &str) -> Option<(&str, &str)> {
    let tag = tag.trim();
    let is_version = |text: &str| crate::plugins::version_of(&Value::from(text)).is_some();
    if is_version(tag) {
        return Some(("", tag));
    }
    let at = tag.rfind('v')?;
    let version = &tag[at + 1..];
    is_version(version).then(|| (&tag[..at], version))
}

pub fn semver(text: &str) -> (u64, u64, u64) {
    let mut parts = text.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
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
        // the latest release, by the address GitHub gives it
        for url in [
            "https://github.com/acme/plugins/releases",
            "https://github.com/acme/plugins/releases/latest",
        ] {
            assert_eq!(
                Source::parse(url, None, None).unwrap(),
                Source::Release {
                    owner: "acme".into(),
                    repo: "plugins".into(),
                    tag: None
                },
                "{url}"
            );
        }
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

#[cfg(test)]
mod bundle_tests {
    use super::{Path, Unpacking, in_the_bundle, unzip, unzip_within};

    fn zipped(entries: &[&str]) -> Vec<u8> {
        use std::io::Write;
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for name in entries {
            out.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            out.write_all(b"x").unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    #[test]
    fn an_archive_entry_that_leaves_the_folder_is_refused() {
        // a release is someone else's zip: nothing in it may land outside
        // the folder it is unpacked into
        for evil in ["../evil.txt", "view/../../evil.txt", "/abs/evil.txt"] {
            let root = tempfile::tempdir().unwrap();
            let into = root.path().join("fetch");
            std::fs::create_dir_all(&into).unwrap();
            let error = unzip(&zipped(&["manifest.json", evil]), &into).unwrap_err();
            assert!(
                error.to_string().contains("leaves the archive"),
                "{evil}: {error}"
            );
            assert!(!root.path().join("evil.txt").exists(), "{evil} escaped");
            assert!(!Path::new("/abs/evil.txt").exists(), "{evil} escaped");
        }
    }

    /// A small archive can hold a great deal once unpacked: unpacking stops
    /// at a total size and a number of entries, whatever the entries claim.
    #[test]
    fn an_archive_that_unpacks_too_large_or_into_too_many_files_is_refused() {
        use std::io::Write;
        let limits = Unpacking {
            bytes: 1024 * 1024,
            entries: 10,
        };

        // 4 MB of zeros, which deflate squeezes into a few kilobytes
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        out.start_file("manifest.json", deflated).unwrap();
        out.write_all(&vec![0u8; 4 * 1024 * 1024]).unwrap();
        let bomb = out.finish().unwrap().into_inner();
        assert!(bomb.len() < 64 * 1024, "{} bytes", bomb.len());
        let into = tempfile::tempdir().unwrap();
        let error = unzip_within(&bomb, into.path(), limits).unwrap_err();
        assert!(error.to_string().contains("more than"), "{error}");

        let names: Vec<String> = (0..20).map(|i| format!("view/{i}.txt")).collect();
        let many = zipped(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let into = tempfile::tempdir().unwrap();
        let error = unzip_within(&many, into.path(), limits).unwrap_err();
        assert!(error.to_string().contains("entries"), "{error}");
    }

    #[test]
    fn an_archive_unpacks_whole_inside_its_folder() {
        let into = tempfile::tempdir().unwrap();
        unzip(&zipped(&["manifest.json", "view/index.html"]), into.path()).unwrap();
        assert!(into.path().join("manifest.json").is_file());
        assert!(into.path().join("view/index.html").is_file());
    }

    #[test]
    fn the_bundle_is_what_the_view_needs_and_not_the_tooling() {
        for kept in ["manifest.json", "index.html", "view", "schemas", "icon.svg"] {
            assert!(in_the_bundle(kept), "{kept} left out");
        }
        for dropped in [
            ".git",
            ".env",
            "node_modules",
            "src",
            "tests",
            "fixtures",
            "package.json",
            "tsconfig.json",
            "vite.config.ts",
            "vitest.config.ts",
            "playwright.config.ts",
        ] {
            assert!(!in_the_bundle(dropped), "{dropped} kept");
        }
    }
}
