//! Installing a plugin from a folder or a zip on disk: inspect it, place
//! the bundle in the store, record where it came from. A link points the
//! registry at the folder instead and serves it live. Nothing is
//! downloaded and nothing runs: a plugin that needs building is built
//! before it is installed.
//!
//! Every source ends in a bundle, stored once by its hash, which becomes
//! the installation's bundle, whatever version it replaces: an older one
//! too, which the inspection and the answer say. A bundle stays while a
//! review still renders with it.
//!
//! A source is one string: a folder, or a zip on disk. It is parsed
//! before anything is touched, so a bad one fails at once.
//!
//! A plugin is installed under its manifest's name, and replaces whatever
//! was installed under it: a folder, a zip, a link or the app's own copy.

use std::path::{Path, PathBuf};

use chrono::Utc;
use pinrail_format::bundle::{Listing, Taken};
use pinrail_format::manifest::line_of;
use serde_json::{Map, Value};

use crate::db::{Db, InstallRecord};
use crate::error::Error;
use crate::plugins::{Plugin, Registry};

#[derive(Debug, Default, Clone)]
pub struct Options {
    /// serve the folder live instead of copying it
    pub link: bool,
}

/// Where a plugin comes from, as the source string says.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Folder(PathBuf),
    /// a zip of a plugin, on disk
    Archive(PathBuf),
}

impl Source {
    /// Parses the path a person typed: a folder, such as `./review`,
    /// `~/code/review` or `/abs/review`, or a file ending in `.zip`. An
    /// address is refused: a plugin is downloaded first, then installed.
    pub fn parse(text: &str) -> Result<Source, Error> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::invalid("/source", "is required"));
        }
        if text.contains("://") || text.starts_with("git@") {
            return Err(Error::invalid(
                "/source",
                "Pinrail installs a plugin from a folder or a zip on disk; download it, then install it from there",
            ));
        }
        let expanded = match text.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|| PathBuf::from(text)),
            None => PathBuf::from(text),
        };
        if text.to_ascii_lowercase().ends_with(".zip") && !expanded.is_dir() {
            return Ok(Source::Archive(expanded));
        }
        Ok(Source::Folder(expanded))
    }
}

/// Installs the plugin the source string names. Returns its record and the
/// version it replaced, if any; the registry has been reloaded with it.
pub fn install(
    db: &Db,
    registry: &Registry,
    source: &str,
    options: Options,
) -> Result<(InstallRecord, Option<String>), Error> {
    let prepared = prepare(registry, source, &options)?;
    let scratch = prepared.scratch.clone();
    let result = install_dir(db, registry, &prepared.dir, options, prepared.origin);
    if let Some(scratch) = scratch {
        let _ = std::fs::remove_dir_all(scratch);
    }
    result
}

/// Removes an installed plugin: its installation. The bundles reviews
/// were submitted to stay with them, so they still render; the rest go
/// with the sweep. The plugins the app ships cannot be removed.
pub fn remove(db: &Db, registry: &Registry, name: &str) -> Result<Value, Error> {
    let _changing = registry.changing();
    let record = db
        .install(name)?
        .ok_or_else(|| Error::NotFound(format!("plugin {name}")))?;
    if record.kind == "app" {
        return Err(Error::invalid(
            "/name",
            format!("{name} ships with Pinrail and cannot be removed"),
        ));
    }
    let version = registry
        .get(name)
        .map(|p| p.version.clone())
        .unwrap_or_default();
    db.remove_install(name)?;
    registry.reload()?;
    Ok(serde_json::json!({
        "removed": name,
        "link": record.linked(),
        "version": version,
    }))
}

/// What installing `source` would do, without doing it: the plugin the
/// manifest describes, where it comes from, and what is installed under
/// that name already. A zip is
/// unpacked the way an install does it, and dropped afterwards.
pub fn inspect(
    db: &Db,
    registry: &Registry,
    source: &str,
    options: Options,
) -> Result<Value, Error> {
    let prepared = prepare(registry, source, &options)?;
    let summary = summarize(db, registry, &prepared, &options);
    if let Some(scratch) = &prepared.scratch {
        let _ = std::fs::remove_dir_all(scratch);
    }
    summary
}

fn summarize(
    db: &Db,
    registry: &Registry,
    prepared: &Prepared,
    options: &Options,
) -> Result<Value, Error> {
    let dir = &prepared.dir;
    if !dir.is_dir() {
        return Err(Error::invalid(
            "/source",
            format!("{} is not a directory", dir.display()),
        ));
    }
    let manifest = read_manifest(dir)?;
    // what the registry would say of the folder as it is
    let plugin = Plugin::load(dir);
    if let Some(why) = &plugin.error {
        return Err(Error::invalid("/source", not_a_plugin(why)));
    }
    let version = manifest
        .get("version")
        .and_then(crate::plugins::version_of)
        .map(|(version, _)| version)
        .filter(|version| version != "0.0.0" && line_of(version).is_some())
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
    // what is installed under the name, and whether this source holds
    // the very files that were installed
    let installed = match db.install(&name)? {
        None => None,
        Some(r) => {
            let current = registry.get(&name);
            let installed_version = current
                .as_ref()
                .map(|p| p.version.clone())
                .unwrap_or_default();
            let unchanged = !r.linked()
                && installed_version == version
                && r.bundle.is_some()
                && bundle_hash_of(dir).ok() == r.bundle;
            Some(serde_json::json!({
                "version": installed_version,
                "source_kind": r.kind,
                "source": r.source,
                "link": r.linked(),
                "unchanged": unchanged,
                // the sites it may open stay with a plugin from disk; the
                // app's own copy is another plugin
                "links_kept": r.kind != "app",
            }))
        }
    };
    // a version older than the installed one
    let older = registry
        .get(&name)
        .filter(|p| p.install.as_ref().is_some_and(|i| !i.link))
        .is_some_and(|current| semver(&version) < semver(&current.version));
    Ok(serde_json::json!({
        "name": name,
        "version": version,
        "title": manifest.get("title").and_then(Value::as_str).unwrap_or(&name),
        "source_kind": prepared.origin.kind,
        "source": prepared.origin.source,
        "link": options.link,
        // the icon's markup, as the app shows an installed plugin's
        "icon": super::manifest::icon_markup(&prepared.dir, super::manifest::ICON).ok(),
        // the files it takes beside a payload, for the dialog to say before the yes
        "attachments": manifest.get("attachments"),
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

fn prepare(registry: &Registry, source: &str, options: &Options) -> Result<Prepared, Error> {
    match Source::parse(source)? {
        Source::Folder(folder) => {
            let dir = std::path::absolute(&folder)?;
            Ok(Prepared {
                scratch: None,
                origin: Origin {
                    kind: "folder",
                    source: dir.display().to_string(),
                },
                dir,
            })
        }
        Source::Archive(archive) => {
            if options.link {
                return Err(Error::invalid(
                    "/source",
                    "a link needs a folder; unpack the zip and link that folder",
                ));
            }
            let path = std::path::absolute(&archive)?;
            let unpacked = unpack_archive(registry, &path)?;
            Ok(Prepared {
                scratch: Some(unpacked.scratch),
                dir: unpacked.root,
                origin: Origin {
                    kind: "archive",
                    source: path.display().to_string(),
                },
            })
        }
    }
}

/// Where a record says it came from: `folder` or `archive`, and its full
/// path.
struct Origin {
    kind: &'static str,
    source: String,
}

/// The most a zip, downloaded or on disk, may weigh.
const ASSET_LIMIT: u64 = 200 * 1024 * 1024;

/// A zip unpacked into a scratch folder of its own, and the folder in it
/// that holds the manifest.
struct Unpacked {
    scratch: PathBuf,
    root: PathBuf,
}

/// Reads a zip on disk, no larger than [`ASSET_LIMIT`], and unpacks it.
fn unpack_archive(registry: &Registry, path: &Path) -> Result<Unpacked, Error> {
    let shown = path.display();
    let size = std::fs::metadata(path)
        .map_err(|e| Error::invalid("/source", format!("cannot read {shown}: {e}")))?
        .len();
    if size > ASSET_LIMIT {
        return Err(Error::invalid(
            "/source",
            format!("{shown} is {size} bytes, more than the {ASSET_LIMIT} allowed"),
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| Error::invalid("/source", format!("cannot read {shown}: {e}")))?;
    unpack(registry, &shown.to_string(), &bytes)
}

/// Unpacks a zip, named `name` in messages, into a scratch folder. The
/// manifest sits at the archive's root, or in the single folder at its
/// root, as `zip -r` of a plugin's folder makes it.
fn unpack(registry: &Registry, name: &str, bytes: &[u8]) -> Result<Unpacked, Error> {
    let scratch = registry.work_dir().join(format!(
        "archive-{}",
        crate::id::next().trim_start_matches("r_")
    ));
    let tree = scratch.join("tree");
    std::fs::create_dir_all(&tree)?;
    let unpacked = unzip(bytes, &tree).and_then(|_| {
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
                format!("{name} has no manifest.json at its root"),
            )),
        }
    });
    match unpacked {
        Ok(root) => Ok(Unpacked { scratch, root }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&scratch);
            Err(match e {
                Error::Invalid(_) => e,
                other => Error::invalid("/source", format!("{name}: {other}")),
            })
        }
    }
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

/// Tidies `<data>/plugins` at start: `work` is scratch and no install
/// survives a restart, so what a stop left there goes, and so do the build
/// logs that earlier versions kept in `logs`.
pub fn tidy(plugins_dir: &Path) -> std::io::Result<()> {
    if plugins_dir.join("logs").is_dir() {
        std::fs::remove_dir_all(plugins_dir.join("logs"))?;
    }
    let Ok(entries) = std::fs::read_dir(plugins_dir.join("work")) else {
        return Ok(());
    };
    for leftover in entries.flatten() {
        let path = leftover.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// The hash of the bundle a source folder holds.
fn bundle_hash_of(dir: &Path) -> Result<String, String> {
    Listing::of_folder(dir, Taken::FromSource).map(|listing| listing.hash())
}

/// Installs the plugin in `dir`, a folder or an unpacked zip.
fn install_dir(
    db: &Db,
    registry: &Registry,
    dir: &Path,
    options: Options,
    origin: Origin,
) -> Result<(InstallRecord, Option<String>), Error> {
    let dir = std::path::absolute(dir)?;
    if !dir.is_dir() {
        return Err(Error::invalid(
            "/source",
            format!("{} is not a directory", dir.display()),
        ));
    }
    let manifest = read_manifest(&dir)?;
    match manifest.get("name").and_then(Value::as_str) {
        Some(name) if super::manifest::valid_name(name) => {}
        Some(name) => {
            return Err(Error::invalid(
                "/source",
                format!(
                    "not a plugin: name {name:?} must start with a lowercase letter, followed by letters, digits, _ or -"
                ),
            ));
        }
        None => return Err(Error::invalid("/source", "not a plugin: name is required")),
    }
    let plugin = Plugin::load(&dir);
    if let Some(why) = &plugin.error {
        return Err(Error::invalid("/source", not_a_plugin(why)));
    }

    // a link serves the folder as it is
    if options.link {
        let record = record_for(&plugin, &origin, true, None);
        let _changing = registry.changing();
        let before = installed_version(registry, &record.name);
        return Ok((commit(db, registry, record)?, before));
    }

    // what the store takes: the files of the layout, and only those; a
    // link, which could point anywhere on the machine, is refused
    let bundle = registry
        .bundles()
        .store(&dir)
        .map_err(|error| match error {
            Error::Invalid(violations) => Error::invalid(
                "/source",
                format!(
                    "the plugin cannot be installed: {}",
                    violations
                        .first()
                        .map(|v| v.message.clone())
                        .unwrap_or_default()
                ),
            ),
            other => other,
        })?;
    let _changing = registry.changing();
    let record = record_for(&plugin, &origin, false, Some(bundle.hash.clone()));
    let before = installed_version(registry, &record.name);
    Ok((commit(db, registry, record)?, before))
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

/// Why a folder is not a plugin. A folder without its view is most often
/// a plugin's sources that have not been built.
fn not_a_plugin(why: &str) -> String {
    if why == format!("{} not found", pinrail_format::manifest::VIEW) {
        format!("not a plugin: {why}; build the plugin first, so that its view is in the folder")
    } else {
        format!("not a plugin: {why}")
    }
}

/// The installation a source makes, with `bundle` the one new reviews use;
/// none for a link.
fn record_for(
    plugin: &Plugin,
    origin: &Origin,
    link: bool,
    bundle: Option<String>,
) -> InstallRecord {
    let now = crate::reviews::iso(Utc::now());
    InstallRecord {
        name: plugin.name.clone(),
        kind: origin.kind.into(),
        source: origin.source.clone(),
        link,
        bundle,
        installed_at: now.clone(),
        updated_at: now,
    }
}

/// The version installed under a name, before an install replaces it.
fn installed_version(registry: &Registry, name: &str) -> Option<String> {
    registry.get(name).map(|p| p.version.clone())
}

/// Records the installation and reloads the registry.
fn commit(db: &Db, registry: &Registry, record: InstallRecord) -> Result<InstallRecord, Error> {
    db.record_install(&record)?;
    registry.reload()?;
    Ok(record)
}

pub use pinrail_format::semver;

#[cfg(test)]
mod source_tests {
    use super::Source;

    #[test]
    fn a_source_is_a_folder_or_a_zip_on_disk() {
        for text in [
            "./review",
            "../review",
            "/abs/review",
            "~/code/review",
            "plugins/review",
            "review",
            "github.com/acme/plugins",
        ] {
            assert!(
                matches!(Source::parse(text).unwrap(), Source::Folder(_)),
                "{text}"
            );
        }
        for text in ["./review-1.2.0.zip", "/abs/Review.ZIP", "review.zip"] {
            assert!(
                matches!(Source::parse(text).unwrap(), Source::Archive(_)),
                "{text}"
            );
        }
        for text in [
            "https://github.com/acme/plugins",
            "https://github.com/acme/plugins/releases/download/v1/review.zip",
            "git@github.com:acme/plugins.git",
            "file:///home/me/review",
        ] {
            let error = Source::parse(text).unwrap_err().to_string();
            assert!(
                error.contains("a folder or a zip on disk"),
                "{text}: {error}"
            );
        }
        assert!(Source::parse("  ").is_err());
    }
}

#[cfg(test)]
mod bundle_tests {
    use super::{Path, Unpacking, unzip, unzip_within};

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
}
