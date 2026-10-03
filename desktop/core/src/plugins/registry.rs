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
use include_dir::{Dir, include_dir};

use super::bundles::{Bundles, Files};
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

/// Stores the plugins the app carries as bundles and installs each under
/// its name, with `app` as its source, when nothing is installed under
/// that name; one installed from disk, or linked, is left alone. An app's
/// copy is replaced when this release of the app carries a newer version,
/// so a downgrade of the app leaves a newer one in place; reviews keep the
/// bundle they were submitted to.
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
        let installed = db.install(&bundle.name)?;
        let record = match installed {
            None => InstallRecord {
                name: bundle.name.clone(),
                kind: "app".into(),
                source: String::new(),
                link: false,
                bundle: bundle.hash.clone(),
                installed_at: now.clone(),
                updated_at: now.clone(),
            },
            Some(install) if install.kind == "app" => {
                let newer = db.bundle(&install.bundle)?.is_none_or(|current| {
                    pinrail_format::semver(&bundle.version)
                        > pinrail_format::semver(&current.version)
                });
                if !newer {
                    continue;
                }
                InstallRecord {
                    bundle: bundle.hash.clone(),
                    updated_at: now.clone(),
                    ..install
                }
            }
            // installed from disk, or linked, in the app's copy's place
            Some(_) => continue,
        };
        db.record_install(&record)?;
    }
    Ok(())
}

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
}

impl Registry {
    /// Loads every installation the database records.
    pub fn open(db: Arc<Db>, bundles: Bundles, plugins_dir: PathBuf) -> Result<Registry, Error> {
        let registry = Registry {
            db,
            bundles,
            plugins_dir,
            state: RwLock::default(),
            by_bundle: Mutex::default(),
            changes: Mutex::default(),
            captured: Mutex::default(),
        };
        registry.reload()?;
        Ok(registry)
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
            None => Err(Error::invalid("/plugin", format!("unknown plugin {name}"))),
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
        let folder = PathBuf::from(&install.source);
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

    fn open(dir: &Path, db: Arc<Db>) -> Registry {
        let bundles = Bundles::open(&dir.join("bundles"), db.clone()).unwrap();
        store_bundled(&db, &bundles).unwrap();
        Registry::open(db, bundles, dir.to_path_buf()).unwrap()
    }

    fn installs(db: &Db) -> Vec<(String, String, bool)> {
        db.installs()
            .unwrap()
            .into_iter()
            .map(|i| (i.name, i.kind, i.link))
            .collect()
    }

    #[test]
    fn the_bundled_plugins_are_stored_and_installed_from_the_app() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone());
        assert_eq!(
            installs(&db),
            vec![
                ("feedback".into(), "app".into(), false),
                ("list".into(), "app".into(), false),
            ]
        );
        let list = r.fetch("list").unwrap();
        assert_eq!(list.name, "list");
        assert_eq!(list.version, "1.0.0");
        let bundle = list.install.as_ref().unwrap().bundle.clone().unwrap();
        assert_eq!(list.path, r.bundles().path(&bundle));
        assert_eq!(r.fetch_bundle(&bundle).unwrap().version, "1.0.0");
        assert!(r.fetch_bundle(&"0".repeat(64)).is_err());
        // a name with a publisher is no name
        assert!(r.fetch("forgeplane/list").is_err());

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

    /// An older release the app carries does not replace a newer one that
    /// came with an earlier app.
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

        let newer = tmp.path().join("newer");
        copy_dir(&r.bundles().path(&shipped), &newer);
        let manifest = newer.join("manifest.json");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, text.replace("\"1.0.0\"", "\"1.9.0\"")).unwrap();
        let stored = r.bundles().store(&newer).unwrap();
        let install = db.install("list").unwrap().unwrap();
        db.record_install(&InstallRecord {
            bundle: stored.hash.clone(),
            ..install
        })
        .unwrap();

        store_bundled(&db, r.bundles()).unwrap();
        r.reload().unwrap();
        let list = r.fetch("list").unwrap();
        assert_eq!(list.version, "1.9.0");
        assert_eq!(list.install.as_ref().unwrap().bundle, Some(stored.hash));
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

    /// A plugin from disk under the name of one the app carries takes its
    /// place, and the app leaves it there at the next start.
    #[test]
    fn a_plugin_from_disk_takes_the_name_of_the_apps_own() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone());
        let folder = tmp.path().join("list");
        copy_dir(&r.fetch("list").unwrap().path, &folder);
        link(&r, &db, "list", &folder);
        r.reload().unwrap();

        store_bundled(&db, r.bundles()).unwrap();
        r.reload().unwrap();
        let linked = r.fetch("list").unwrap();
        assert!(linked.install.as_ref().unwrap().link);
        assert_eq!(installs(&db)[1], ("list".into(), "folder".into(), true));
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_error() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let r = open(tmp.path(), db.clone());
        let folder = tmp.path().join("broken");
        copy_dir(&r.fetch("list").unwrap().path, &folder);
        let manifest = folder.join("manifest.json");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, text.replace("\"list\"", "\"broken\"")).unwrap();
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
