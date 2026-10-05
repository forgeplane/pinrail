//! The registered plugins: every installation, each with the plugin its new
//! reviews use, and the bundles that render older reviews, loaded when a
//! review asks for them.
//!
//! A plugin is known by its manifest's name, and one installation holds
//! each name.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use chrono::Utc;

use super::bundles::Bundles;
use super::catalog::{self, Catalog, Entry};
use super::manifest::{Install, Plugin};
use crate::db::{Db, InstallRecord};
use crate::error::Error;

#[derive(Debug, Default)]
struct RegistryState {
    installs: Vec<InstallRecord>,
    /// Each installation's plugin for new reviews, by name.
    plugins: BTreeMap<String, Arc<Plugin>>,
}

/// What a linked folder was when it was last captured, why it could not
/// be while it cannot, and the change the check of linked folders noticed
/// since, which the next use captures.
#[derive(Debug, Default)]
struct Captured {
    /// The folder this describes: a plugin linked again from another folder
    /// starts afresh.
    folder: String,
    seen: Vec<(String, u64, u128)>,
    problem: Option<String>,
    noticed: Option<Vec<(String, u64, u128)>>,
}

/// The installed plugins, and the bundles reviews render with.
#[derive(Debug)]
pub struct Registry {
    db: Arc<Db>,
    bundles: Bundles,
    /// Where zips are unpacked: `work/`.
    plugins_dir: PathBuf,
    state: RwLock<RegistryState>,
    /// The plugins of bundles a review asked for, by the bundle's hash: a
    /// bundle never changes.
    by_bundle: Mutex<HashMap<String, Arc<Plugin>>>,
    /// Held while installations change, so two installs or removals of a
    /// plugin cannot interleave.
    changes: Mutex<()>,
    /// Each linked folder as last captured, by name.
    captured: Mutex<BTreeMap<String, Captured>>,
    /// What can be installed by id, and the versions installs from it can
    /// be updated to: the app's own catalog, and later the registry's.
    catalogs: Vec<Catalog>,
}

impl Registry {
    /// Loads every installation the database records. Nothing is
    /// installed from the catalogs: the person chooses what to install.
    pub fn open(
        db: Arc<Db>,
        bundles: Bundles,
        plugins_dir: PathBuf,
        catalogs: Vec<Catalog>,
    ) -> Result<Registry, Error> {
        let registry = Registry {
            db,
            bundles,
            plugins_dir,
            state: RwLock::default(),
            by_bundle: Mutex::default(),
            changes: Mutex::default(),
            captured: Mutex::default(),
            catalogs,
        };
        registry.reload()?;
        Ok(registry)
    }

    /// The entry `id` names, `forgeplane/<name>` or a bare name, at the
    /// highest version any catalog offers.
    pub fn offered(&self, id: &str) -> Option<&Entry> {
        catalog::best(&self.catalogs, id)
    }

    /// Every plugin the catalogs offer, each at its highest version.
    pub fn catalog(&self) -> Vec<&Entry> {
        catalog::listing(&self.catalogs)
    }

    /// The version `name` can be updated to: the highest any catalog
    /// offers, when it was installed from one and that version is higher.
    /// A plugin from a folder, a zip or a link is not updated from a
    /// catalog.
    pub fn update_for(&self, name: &str) -> Option<&Entry> {
        let plugin = self.get(name)?;
        let install = plugin.install.as_ref()?;
        if install.source_kind != "index" {
            return None;
        }
        self.offered(&install.source).filter(|entry| {
            entry.needs().is_none()
                && pinrail_format::semver(&entry.version) > pinrail_format::semver(&plugin.version)
        })
    }

    pub fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    /// Where a zip is unpacked: scratch, emptied at start.
    pub fn work_dir(&self) -> PathBuf {
        self.plugins_dir.join("work")
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

    pub fn all(&self) -> Vec<Arc<Plugin>> {
        self.read().plugins.values().cloned().collect()
    }

    /// The installed plugin new reviews use, usable or not.
    pub fn get(&self, name: &str) -> Option<Arc<Plugin>> {
        self.read().plugins.get(name).cloned()
    }

    /// The usable plugin new reviews use, or an `invalid` error pointing at
    /// `/plugin`.
    pub fn fetch(&self, name: &str) -> Result<Arc<Plugin>, Error> {
        self.capture(name)?;
        match self.read().plugins.get(name) {
            Some(p) if p.usable() => Ok(p.clone()),
            Some(p) => Err(Error::invalid(
                "/plugin",
                format!(
                    "plugin {name} is not usable: {}",
                    p.error.clone().unwrap_or_default()
                ),
            )),
            None => Err(self.missing(name)),
        }
    }

    /// Why no plugin named `name` can be used: unknown, or an official
    /// plugin that is not installed, with how to install it.
    pub fn missing(&self, name: &str) -> Error {
        match self.offered(name) {
            Some(entry) if !name.contains('/') => Error::invalid(
                "/plugin",
                format!(
                    "plugin {name} is not installed; install it with `pinrail plugins install {}`, or in Settings › Plugins",
                    entry.name
                ),
            ),
            _ => Error::invalid("/plugin", format!("unknown plugin {name}")),
        }
    }

    /// The plugin a review renders with: the bundle it records.
    pub fn fetch_review(&self, plugin: &str, bundle: Option<&str>) -> Result<Arc<Plugin>, Error> {
        let bundle = bundle.ok_or_else(|| {
            Error::invalid("/plugin", format!("a review of {plugin} records no bundle"))
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
        // what was captured of a folder no installation links any more
        self.captured
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|name, c| {
                installs
                    .iter()
                    .any(|i| &i.name == name && i.linked() && i.source == c.folder)
            });
        let mut plugins = BTreeMap::new();
        for install in &installs {
            plugins.insert(install.name.clone(), Arc::new(self.load(install)?));
        }
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = plugins.len();
        *state = RegistryState { installs, plugins };
        Ok(count)
    }

    /// Captures a linked plugin's folder when it changed since it was last
    /// captured: stored as a bundle, which the installation then uses. A
    /// folder that is not a plugin leaves the installation on its last good
    /// bundle, and the plugin is listed with the folder's problem until it
    /// is repaired. Whether anything changed; nothing for a plugin that is
    /// not linked. An unchanged folder is told by its files' sizes and
    /// modification times, without reading them.
    pub fn capture(&self, name: &str) -> Result<bool, Error> {
        let Some(install) = self
            .read()
            .installs
            .iter()
            .find(|i| i.name == name && i.linked())
            .cloned()
        else {
            return Ok(false);
        };
        let source = install.source.clone();
        let folder = PathBuf::from(&source);
        let seen = folder_state(&folder);
        let before = {
            let captured = self.captured.lock().unwrap_or_else(|e| e.into_inner());
            match captured.get(name) {
                Some(c) if c.seen == seen => return Ok(false),
                Some(c) => c.problem.clone(),
                None => None,
            }
        };
        let _changing = self.changing();
        let (problem, moved) = match self.store_folder(&folder, name) {
            Ok(hash) if hash != install.bundle => {
                self.db.record_install(&InstallRecord {
                    bundle: hash,
                    updated_at: crate::reviews::iso(Utc::now()),
                    ..install
                })?;
                (None, true)
            }
            Ok(_) => (None, false),
            Err(why) => (Some(why), false),
        };
        let changed = moved || problem != before;
        let noticed_before = self
            .captured
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                name.to_string(),
                Captured {
                    folder: source,
                    seen,
                    problem,
                    noticed: None,
                },
            )
            .is_some_and(|c| c.noticed.is_some());
        let changed = changed || noticed_before;
        if changed {
            self.reload()?;
        }
        Ok(changed)
    }

    /// Notices which linked folders changed since they were captured, and
    /// marks them, storing nothing: the next use of the plugin captures it.
    /// Whether a mark came or went. A folder seen for the first time since
    /// the app started is captured, so its installation matches it.
    pub fn notice_links(&self) -> Result<bool, Error> {
        let links: Vec<(String, PathBuf)> = self
            .read()
            .installs
            .iter()
            .filter(|i| i.linked())
            .map(|i| (i.name.clone(), PathBuf::from(&i.source)))
            .collect();
        let mut changed = false;
        let mut first = Vec::new();
        {
            let mut captured = self.captured.lock().unwrap_or_else(|e| e.into_inner());
            for (name, folder) in &links {
                let now = folder_state(folder);
                match captured.get_mut(name) {
                    None => first.push(name.clone()),
                    Some(c) if c.seen == now => changed |= c.noticed.take().is_some(),
                    Some(c) if c.noticed.as_ref() != Some(&now) => {
                        c.noticed = Some(now);
                        changed = true;
                    }
                    Some(_) => {}
                }
            }
        }
        for name in first {
            changed |= self.capture(&name)?;
        }
        if changed {
            self.reload()?;
        }
        Ok(changed)
    }

    /// A linked folder stored as a bundle: its hash, or why the folder is
    /// not the plugin installed under `name`.
    fn store_folder(&self, folder: &Path, name: &str) -> Result<String, String> {
        let plugin = Plugin::load(folder);
        if let Some(why) = plugin.error {
            return Err(why);
        }
        if plugin.name != name {
            return Err(format!(
                "the manifest names {}, the installation {name}",
                plugin.name
            ));
        }
        self.bundles
            .store(folder)
            .map(|bundle| bundle.hash)
            .map_err(|error| match error {
                Error::Invalid(violations) => violations
                    .first()
                    .map(|v| v.message.clone())
                    .unwrap_or_default(),
                other => other.to_string(),
            })
    }

    /// The plugin an installation's new reviews use: its bundle, checked
    /// against that bundle's listing. A broken one is listed with its error,
    /// and a linked one with its folder's problem while it has one.
    fn load(&self, install: &InstallRecord) -> Result<Plugin, Error> {
        let mut plugin = Plugin::load(&self.bundles.path(&install.bundle));
        if let Some(problem) = self
            .captured
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&install.name)
            .and_then(|c| c.problem.clone())
        {
            plugin.error = Some(problem);
        }
        // listed under the installation's name, whatever the folder holds
        if plugin.error.is_none() && plugin.name != install.name {
            plugin.error = Some(format!(
                "the manifest names {}, the installation {}",
                plugin.name, install.name
            ));
        }
        plugin.name = install.name.clone();
        let modified = self.bundles.verify(&install.bundle).is_err();
        let folder_changed = install.linked()
            && self
                .captured
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&install.name)
                .is_some_and(|c| c.noticed.is_some());
        plugin.install = Some(Install {
            source_kind: install.kind.clone(),
            source: install.source.clone(),
            link: install.linked(),
            bundle: Some(install.bundle.clone()),
            modified,
            folder_changed,
            installed_at: install.installed_at.clone(),
            updated_at: install.updated_at.clone(),
        });
        Ok(plugin)
    }
}

/// What a linked folder holds of the layout, as each file's path, size and
/// modification time, to tell a change without reading the files. Empty
/// for a folder that is not there.
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
            if !wanted {
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
    use crate::plugins::carried;

    fn open(dir: &Path, db: Arc<Db>, catalogs: Vec<Catalog>) -> Registry {
        let bundles = Bundles::open(&dir.join("bundles"), db.clone()).unwrap();
        Registry::open(db, bundles, dir.to_path_buf(), catalogs).unwrap()
    }

    /// The list plugin's files in a folder, as `name` at `version`.
    fn list_folder(at: &Path, name: &str, version: &str) -> PathBuf {
        let (_, files) = carried().into_iter().find(|(f, _)| f == "list").unwrap();
        for (path, bytes) in files {
            let target = at.join(&path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            let bytes = if path == "manifest.json" {
                String::from_utf8(bytes)
                    .unwrap()
                    .replace("\"list\"", &format!("\"{name}\""))
                    .replace("\"1.0.0\"", &format!("\"{version}\""))
                    .into_bytes()
            } else {
                bytes
            };
            std::fs::write(target, bytes).unwrap();
        }
        at.to_path_buf()
    }

    /// The catalog with list at `version`.
    fn catalog_with_list(version: &str) -> Catalog {
        let tmp = tempfile::tempdir().unwrap();
        let folder = list_folder(tmp.path(), "list", version);
        let listing = pinrail_format::bundle::Listing::of_folder(
            &folder,
            pinrail_format::bundle::Taken::FromSource,
        )
        .unwrap();
        let files = listing
            .files
            .iter()
            .map(|f| (f.path.clone(), std::fs::read(folder.join(&f.path)).unwrap()))
            .collect();
        Catalog::of(vec![("list".into(), files)])
    }

    /// Links a folder that holds a plugin, as an install stores it.
    fn link(r: &Registry, db: &Db, name: &str, folder: &Path) {
        let bundle = r.bundles().store(folder).unwrap();
        db.record_install(&InstallRecord {
            name: name.into(),
            kind: "folder".into(),
            source: folder.display().to_string(),
            link: true,
            bundle: bundle.hash,
            installed_at: "2026-10-01T10:00:00Z".into(),
            updated_at: "2026-10-01T10:00:00Z".into(),
        })
        .unwrap();
    }

    #[test]
    fn an_official_plugin_is_installed_from_the_catalog_by_its_id() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone(), vec![Catalog::builtin()]);
        assert!(db.installs().unwrap().is_empty());
        assert!(r.fetch("list").is_err());

        crate::plugins::install::install_offered(&db, &r, "forgeplane/list").unwrap();
        let list = r.fetch("list").unwrap();
        assert_eq!(list.version, "1.0.0");
        let install = list.install.as_ref().unwrap();
        assert_eq!(
            (install.source_kind.as_str(), install.source.as_str()),
            ("index", "forgeplane/list")
        );
        let bundle = install.bundle.clone().unwrap();
        assert_eq!(bundle, r.offered("list").unwrap().hash);
        assert_eq!(list.path, r.bundles().path(&bundle));
        assert_eq!(r.fetch_bundle(&bundle).unwrap().version, "1.0.0");
        assert!(r.fetch_bundle(&"0".repeat(64)).is_err());
        // a name with a publisher is no name
        assert!(r.fetch("forgeplane/list").is_err());
        // another publisher's, or one no catalog offers, cannot be installed
        for id in ["acme/list", "nothing"] {
            assert!(
                crate::plugins::install::install_offered(&db, &r, id).is_err(),
                "{id}"
            );
        }
    }

    /// A plugin from disk under an official plugin's name is the person's
    /// own: no catalog updates it.
    #[test]
    fn a_plugin_from_disk_is_not_updated_from_a_catalog() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone(), vec![catalog_with_list("2.0.0")]);
        let folder = list_folder(&tmp.path().join("mine"), "list", "1.0.0");
        link(&r, &db, "list", &folder);
        r.reload().unwrap();
        assert!(r.fetch("list").unwrap().install.as_ref().unwrap().link);
        assert!(r.update_for("list").is_none());
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_error() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone(), vec![Catalog::builtin()]);
        crate::plugins::install::install_offered(&db, &r, "list").unwrap();
        let folder = list_folder(&tmp.path().join("broken"), "broken", "1.0.0");
        link(&r, &db, "broken", &folder);
        r.reload().unwrap();
        // its view goes from the folder
        std::fs::remove_file(folder.join("view/index.html")).unwrap();
        assert!(r.capture("broken").unwrap());
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
}
