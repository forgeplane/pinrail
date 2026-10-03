//! The registered plugins: every installation, each with the plugin its new
//! reviews use, and the lines that render older reviews, loaded from their
//! bundles when a review asks for them.
//!
//! A plugin is known by its full name, `<publisher>/<name>`. An agent may
//! name it by its name alone when no other installed plugin has that name.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use chrono::Utc;
use include_dir::{Dir, include_dir};
use serde_json::Value;

use super::bundles::{Bundles, Files};
use super::install::BUNDLED_PUBLISHER;
use super::manifest::{Install, Plugin};
use crate::db::{Db, InstallRecord};
use crate::error::Error;

/// The plugins that ship with the app. They live in `plugins/` with the
/// others; build.rs copies their bundles here for the binary to carry.
static BUILTIN: Dir = include_dir!("$OUT_DIR/builtin");

/// The files of each plugin the app carries, by its folder: paths relative
/// to the plugin, and their bytes.
pub(crate) fn bundled() -> Vec<(String, Files)> {
    fn files(dir: &Dir, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for file in dir.files() {
            let path = file.path().strip_prefix(root).unwrap_or(file.path());
            out.push((
                path.to_string_lossy().replace('\\', "/"),
                file.contents().to_vec(),
            ));
        }
        for child in dir.dirs() {
            files(child, root, out);
        }
    }
    BUILTIN
        .dirs()
        .map(|plugin| {
            let mut out = Vec::new();
            files(plugin, plugin.path(), &mut out);
            (plugin.path().to_string_lossy().into_owned(), out)
        })
        .collect()
}

/// Stores the plugins the app carries as bundles and installs each as
/// `forgeplane/<name>`. A bundled release becomes what new reviews use when
/// it is newer than the installed one, so a downgrade of the app leaves a
/// newer release in place; reviews keep the bundle they were submitted to.
pub(crate) fn store_bundled(db: &Db, bundles: &Bundles) -> Result<(), Error> {
    store_releases(db, bundles, bundled())
}

/// `store_bundled` for the given plugins, as each release of the app
/// carries its own.
pub(crate) fn store_releases(
    db: &Db,
    bundles: &Bundles,
    plugins: Vec<(String, Files)>,
) -> Result<(), Error> {
    let now = crate::reviews::iso(Utc::now());
    for (_, files) in plugins {
        let bundle = bundles.store_files(&files)?;
        let plugin = format!("{BUNDLED_PUBLISHER}/{}", bundle.name);
        let installed = db.install(&plugin)?;
        let newer = match installed.as_ref().and_then(|i| i.bundle.as_ref()) {
            None => true,
            Some(current) => db.bundle(current)?.is_none_or(|current| {
                pinrail_format::semver(&bundle.version) > pinrail_format::semver(&current.version)
            }),
        };
        if !newer {
            continue;
        }
        let source = format!("Pinrail {}", env!("CARGO_PKG_VERSION"));
        let record = match installed {
            // a link in its place, for working on it: left alone
            Some(install) if install.linked() => continue,
            // installed from elsewhere, such as its repository: that
            // installation stands, with the newer release
            Some(install) if install.kind != "bundled" => InstallRecord {
                bundle: Some(bundle.hash.clone()),
                updated_at: now.clone(),
                ..install
            },
            _ => InstallRecord {
                plugin: plugin.clone(),
                publisher: BUNDLED_PUBLISHER.into(),
                name: bundle.name.clone(),
                kind: "bundled".into(),
                resolved: source.clone(),
                source,
                build_log: None,
                bundle: Some(bundle.hash.clone()),
                replaced: None,
                installed_at: now.clone(),
                updated_at: now.clone(),
            },
        };
        db.record_install(&record)?;
    }
    Ok(())
}

#[derive(Debug, Default)]
struct RegistryState {
    installs: Vec<InstallRecord>,
    /// Each installation's plugin for new reviews, by full name.
    plugins: BTreeMap<String, Arc<Plugin>>,
    /// Each linked folder as it was when it was read, to tell a change.
    folders: BTreeMap<String, Vec<(String, u64, u128)>>,
}

/// The installed plugins, and the bundles reviews render with.
#[derive(Debug)]
pub struct Registry {
    db: Arc<Db>,
    bundles: Bundles,
    /// Where zips are unpacked, builds run, and their logs go: `work/` and `logs/`.
    plugins_dir: PathBuf,
    /// How long a plugin's build may run.
    build_timeout: std::time::Duration,
    state: RwLock<RegistryState>,
    /// The plugins of bundles a review asked for, by the bundle's hash: a
    /// bundle never changes.
    by_bundle: Mutex<HashMap<String, Arc<Plugin>>>,
    /// Held while installations and lines change, so two installs or
    /// removals of a plugin cannot interleave.
    changes: Mutex<()>,
}

impl Registry {
    /// Loads every installation the database records.
    pub fn open(db: Arc<Db>, bundles: Bundles, plugins_dir: PathBuf) -> Result<Registry, Error> {
        let registry = Registry {
            db,
            bundles,
            plugins_dir,
            build_timeout: crate::config::BUILD_TIMEOUT,
            state: RwLock::default(),
            by_bundle: Mutex::default(),
            changes: Mutex::default(),
        };
        registry.reload()?;
        Ok(registry)
    }

    /// Stops builds after `timeout` rather than the default.
    pub fn with_build_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.build_timeout = timeout;
        self
    }

    pub fn build_timeout(&self) -> std::time::Duration {
        self.build_timeout
    }

    pub fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    /// Where a zip is unpacked and a source built: scratch, emptied at start.
    pub fn work_dir(&self) -> PathBuf {
        self.plugins_dir.join("work")
    }

    /// Where each build's output is kept.
    pub fn logs_dir(&self) -> PathBuf {
        self.plugins_dir.join("logs")
    }

    /// Taken while installations and lines change: an install recording a
    /// plugin, a removal, or the history sweep letting a line go.
    pub(crate) fn changing(&self) -> std::sync::MutexGuard<'_, ()> {
        self.changes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, RegistryState> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn installs(&self) -> Vec<InstallRecord> {
        self.read().installs.clone()
    }

    pub fn all(&self) -> Vec<Arc<Plugin>> {
        self.read().plugins.values().cloned().collect()
    }

    /// The full name an agent's name stands for: the name itself when it is
    /// one, or the one installed plugin with that name. Two with the name
    /// are refused with both full names to choose from.
    pub fn resolve(&self, name: &str) -> Result<String, Error> {
        let state = self.read();
        if name.contains('/') {
            return match state.installs.iter().any(|i| i.plugin == name) {
                true => Ok(name.to_string()),
                false => Err(Error::invalid("/plugin", format!("unknown plugin {name}"))),
            };
        }
        let matches: Vec<&str> = state
            .installs
            .iter()
            .filter(|i| i.name == name)
            .map(|i| i.plugin.as_str())
            .collect();
        match matches.as_slice() {
            [one] => Ok(one.to_string()),
            [] => Err(Error::invalid("/plugin", format!("unknown plugin {name}"))),
            several => Err(Error::invalid(
                "/plugin",
                format!(
                    "{name} names {}; give the full name of one",
                    several.join(" and ")
                ),
            )),
        }
    }

    /// The installed plugin new reviews use, usable or not, by full name or
    /// by a name only it has.
    pub fn get(&self, name: &str) -> Option<Arc<Plugin>> {
        let plugin = self.resolve(name).ok()?;
        self.read().plugins.get(&plugin).cloned()
    }

    /// The usable plugin new reviews use, or an `invalid` error pointing at
    /// `/plugin`.
    pub fn fetch(&self, name: &str) -> Result<Arc<Plugin>, Error> {
        let plugin = self.resolve(name)?;
        match self.read().plugins.get(&plugin) {
            Some(p) if p.usable() => Ok(p.clone()),
            Some(p) => Err(Error::invalid(
                "/plugin",
                format!(
                    "plugin {plugin} is not usable: {}",
                    p.error.clone().unwrap_or_default()
                ),
            )),
            None => Err(Error::invalid("/plugin", format!("unknown plugin {name}"))),
        }
    }

    /// The plugin a review renders with: the linked folder while its plugin
    /// is linked, so the developer sees their changes; else the bundle the
    /// review was submitted to.
    pub fn fetch_review(&self, plugin: &str, bundle: Option<&str>) -> Result<Arc<Plugin>, Error> {
        if let Some(p) = self.read().plugins.get(plugin)
            && p.install.as_ref().is_some_and(|i| i.linked)
            && p.usable()
        {
            return Ok(p.clone());
        }
        let bundle = bundle.ok_or_else(|| {
            Error::invalid("/plugin", format!("plugin {plugin} is not installed"))
        })?;
        self.fetch_bundle(bundle)
    }

    /// The plugin a stored bundle holds; bundles never change, so each is
    /// read once.
    pub fn fetch_bundle(&self, bundle: &str) -> Result<Arc<Plugin>, Error> {
        let missing = || Error::invalid("/plugin", format!("bundle {bundle} is not stored"));
        let mut by_bundle = self.by_bundle.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = by_bundle.get(bundle) {
            return Ok(p.clone());
        }
        if self.bundles.get(bundle)?.is_none() {
            return Err(missing());
        }
        let loaded = Plugin::load(&self.bundles.path(bundle));
        if !loaded.usable() {
            return Err(missing());
        }
        let loaded = Arc::new(loaded);
        by_bundle.insert(bundle.to_string(), loaded.clone());
        Ok(loaded)
    }

    /// Reads every installation again from the database, and the plugins
    /// they name.
    pub fn reload(&self) -> Result<usize, Error> {
        let installs = self.db.installs()?;
        let mut plugins = BTreeMap::new();
        let mut folders = BTreeMap::new();
        for install in &installs {
            if install.linked() {
                folders.insert(
                    install.plugin.clone(),
                    folder_state(Path::new(&install.resolved)),
                );
            }
            plugins.insert(install.plugin.clone(), Arc::new(self.load(install)?));
        }
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = plugins.len();
        *state = RegistryState {
            installs,
            plugins,
            folders,
        };
        Ok(count)
    }

    /// Whether a linked folder changed since it was read: a file the app
    /// reads from it was added, removed or written, or the folder went or
    /// came back.
    pub fn links_changed(&self) -> bool {
        let state = self.read();
        state
            .installs
            .iter()
            .filter(|i| i.linked())
            .any(|i| state.folders.get(&i.plugin) != Some(&folder_state(Path::new(&i.resolved))))
    }

    /// The plugin an installation's new reviews use: its linked folder, or
    /// its current bundle, checked against that bundle's listing. A broken
    /// one is listed with its error.
    fn load(&self, install: &InstallRecord) -> Result<Plugin, Error> {
        let mut plugin = match (install.linked(), &install.bundle) {
            (true, _) => Plugin::load(Path::new(&install.resolved)),
            (false, Some(bundle)) => Plugin::load(&self.bundles.path(bundle)),
            (false, None) => {
                let mut broken = Plugin::load(&self.bundles.path("none"));
                broken.error = Some("no bundle is installed".into());
                broken
            }
        };
        // listed under the installation's name, whatever the folder holds
        if plugin.error.is_none() && plugin.name != install.name {
            plugin.error = Some(format!(
                "the manifest names {}, the installation {}",
                plugin.name, install.name
            ));
        }
        plugin.name = install.name.clone();
        let modified = !install.linked()
            && install
                .bundle
                .as_ref()
                .is_some_and(|b| self.bundles.verify(b).is_err());
        plugin.install = Some(Install {
            plugin: install.plugin.clone(),
            publisher: install.publisher.clone(),
            kind: install.kind.clone(),
            source: install.source.clone(),
            linked: install.linked(),
            bundle: install.bundle.clone(),
            replaced: install
                .replaced
                .as_deref()
                .and_then(|r| serde_json::from_str::<Value>(r).ok())
                .map(|r| serde_json::json!({ "kind": r["kind"], "source": r["source"] })),
            modified,
            installed_at: install.installed_at.clone(),
            updated_at: install.updated_at.clone(),
        });
        Ok(plugin)
    }
}

/// What the app reads of a linked folder, as each file's path, size and
/// modification time: the layout's files, without the rest of `view/`,
/// which is served as it is on each request. Empty for a folder that is
/// not there.
fn folder_state(dir: &Path) -> Vec<(String, u64, u128)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64, u128)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let wanted = pinrail_format::bundle::holds(&relative)
                || (pinrail_format::bundle::holds(&format!("{relative}/x")) && path.is_dir());
            if !wanted || (relative.starts_with("view/") && relative != super::manifest::VIEW) {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                walk(root, &path, out);
            } else {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                out.push((relative, meta.len(), modified));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(dir: &Path, db: Arc<Db>) -> Registry {
        let bundles = Bundles::open(&dir.join("bundles"), db.clone()).unwrap();
        store_bundled(&db, &bundles).unwrap();
        Registry::open(db, bundles, dir.to_path_buf()).unwrap()
    }

    fn installs(db: &Db) -> Vec<(String, String, bool)> {
        db.installs()
            .unwrap()
            .into_iter()
            .map(|i| (i.plugin, i.kind, i.bundle.is_some()))
            .collect()
    }

    #[test]
    fn the_bundled_plugins_are_stored_and_installed_under_forgeplane() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone());
        assert_eq!(
            installs(&db),
            vec![
                ("forgeplane/feedback".into(), "bundled".into(), true),
                ("forgeplane/list".into(), "bundled".into(), true),
            ]
        );
        let list = r.fetch("list").unwrap();
        assert_eq!(list.full_name(), "forgeplane/list");
        assert_eq!(list.version, "1.0.0");
        assert_eq!(
            r.fetch("forgeplane/list").unwrap().full_name(),
            "forgeplane/list"
        );
        let bundle = list.install.as_ref().unwrap().bundle.clone().unwrap();
        assert_eq!(list.path, r.bundles().path(&bundle));
        assert_eq!(r.fetch_bundle(&bundle).unwrap().version, "1.0.0");
        assert!(r.fetch_bundle(&"0".repeat(64)).is_err());

        // a second start finds them stored: nothing changes
        let before = db.installs().unwrap();
        store_bundled(&db, r.bundles()).unwrap();
        assert_eq!(db.installs().unwrap(), before);
    }

    #[test]
    fn a_bundled_plugin_ships_its_bundle_and_nothing_else() {
        use pinrail_format::bundle::{Listing, Taken};
        for (folder, files) in bundled() {
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../plugins")
                .join(&folder);
            let bundle = Listing::of_folder(&source, Taken::FromSource).unwrap();
            let shipped = Listing::from_files(
                files.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
                Taken::AsBundle,
            );
            assert_eq!(shipped, Ok(bundle), "{folder}");
        }
    }

    /// An older bundled release does not replace a newer installed one.
    #[test]
    fn a_bundled_release_is_installed_only_when_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone());
        let shipped = r
            .fetch("list")
            .unwrap()
            .install
            .as_ref()
            .unwrap()
            .bundle
            .clone()
            .unwrap();

        // a newer release, as an update from elsewhere left it
        let newer = tmp.path().join("newer");
        copy_dir(&r.bundles().path(&shipped), &newer);
        let manifest = newer.join("manifest.json");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, text.replace("\"1.0.0\"", "\"1.9.0\"")).unwrap();
        let stored = r.bundles().store(&newer).unwrap();
        let install = db.install("forgeplane/list").unwrap().unwrap();
        db.record_install(&InstallRecord {
            bundle: Some(stored.hash.clone()),
            ..install
        })
        .unwrap();

        store_bundled(&db, r.bundles()).unwrap();
        r.reload().unwrap();
        let list = r.fetch("list").unwrap();
        assert_eq!(list.version, "1.9.0");
        assert_eq!(list.install.as_ref().unwrap().bundle, Some(stored.hash));
    }

    fn link(db: &Db, publisher: &str, name: &str, folder: &Path) {
        db.record_install(&InstallRecord {
            plugin: format!("{publisher}/{name}"),
            publisher: publisher.into(),
            name: name.into(),
            kind: "link".into(),
            source: folder.display().to_string(),
            resolved: folder.display().to_string(),
            build_log: None,
            bundle: None,
            replaced: None,
            installed_at: "2026-10-01T10:00:00Z".into(),
            updated_at: "2026-10-01T10:00:00Z".into(),
        })
        .unwrap();
    }

    #[test]
    fn short_names_resolve_to_the_one_plugin_with_that_name() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone());
        assert_eq!(r.resolve("list").unwrap(), "forgeplane/list");
        assert_eq!(r.resolve("forgeplane/list").unwrap(), "forgeplane/list");
        let unknown = r.resolve("nope").unwrap_err().to_string();
        assert!(unknown.contains("unknown plugin nope"), "{unknown}");
        assert!(r.resolve("acme/list").is_err());

        // a second plugin named list, linked from a folder
        let folder = tmp.path().join("list");
        let shipped = r.fetch("list").unwrap().path.clone();
        copy_dir(&shipped, &folder);
        link(&db, "local", "list", &folder);
        r.reload().unwrap();
        let error = r.resolve("list").unwrap_err().to_string();
        assert!(
            error.contains("forgeplane/list") && error.contains("local/list"),
            "{error}"
        );
        assert!(r.fetch("list").is_err());
        let linked = r.fetch("local/list").unwrap();
        assert_eq!(linked.full_name(), "local/list");
        assert!(linked.install.as_ref().unwrap().linked);
        assert_eq!(linked.path, folder);
        // its reviews render with the folder while it is linked
        assert_eq!(r.fetch_review("local/list", None).unwrap().path, folder);
        assert_eq!(r.resolve("feedback").unwrap(), "forgeplane/feedback");
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_error() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone());
        let folder = tmp.path().join("broken");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("manifest.json"),
            r#"{"name": "broken", "version": "1.0.0"}"#,
        )
        .unwrap();
        link(&db, "local", "broken", &folder);
        r.reload().unwrap();
        let broken = r.get("broken").unwrap();
        assert!(!broken.usable());
        assert!(
            broken.error.as_deref().unwrap().contains("not found"),
            "{:?}",
            broken.error
        );
        assert!(r.fetch("broken").is_err());
        // the rest still loads
        assert!(r.fetch("list").is_ok());
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::write(&target, std::fs::read(entry.path()).unwrap()).unwrap();
            }
        }
    }
}
