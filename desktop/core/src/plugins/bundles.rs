//! The bundle store: each release of a plugin as files, stored once under
//! `bundles/<hash>/` by the hash of its listing, and never changed.
//!
//! A bundle's folder and its row change together under one lock: storing a
//! bundle and the sweep cannot land between each other's steps. The folder
//! is put in place before the row is written, so a stop between the two
//! leaves a folder no row names, which the next open removes.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pinrail_format::bundle::{Listing, Taken};
use pinrail_format::manifest::{MANIFEST, line_of};
use serde_json::Value;

use crate::db::{BundleRecord, Db};
use crate::error::Error;

/// A bundle's files as paths and bytes.
pub type Files = Vec<(String, Vec<u8>)>;

/// Where a staged copy waits before it takes its hash's name.
const STAGING: &str = ".staging-";

#[derive(Debug, Clone)]
pub struct Bundles {
    dir: PathBuf,
    db: Arc<Db>,
    /// Held while a bundle's folder and row change together.
    files: Arc<Mutex<()>>,
    /// The listings read so far: a bundle never changes, so a listing
    /// stays true until the sweep removes its bundle.
    listings: Arc<Mutex<HashMap<String, Arc<Listing>>>>,
}

impl Bundles {
    /// Opens the store under `dir`, removing what a stop left behind: copies
    /// still staged, and folders no row names. Only one application holds
    /// the data directory, so nothing is being stored while it opens.
    pub fn open(dir: &Path, db: Arc<Db>) -> Result<Self, Error> {
        fs::create_dir_all(dir)?;
        let recorded = db.bundle_hashes()?;
        for entry in fs::read_dir(dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(STAGING) || !recorded.contains(&name) {
                remove(&entry.path())?;
            }
        }
        Ok(Bundles {
            dir: dir.to_path_buf(),
            db,
            files: Arc::default(),
            listings: Arc::default(),
        })
    }

    /// Where a bundle's files are.
    pub fn path(&self, hash: &str) -> PathBuf {
        self.dir.join(hash)
    }

    /// Stores the bundle a plugin's folder holds: the files of the layout,
    /// read-only. The same files stored again, from any folder, are the
    /// same bundle, which is only made new again for the sweep.
    pub fn store(&self, source: &Path) -> Result<BundleRecord, Error> {
        let listing = Listing::of_folder(source, Taken::FromSource)
            .map_err(|why| Error::invalid("/source", why))?;
        let files = listing
            .files
            .iter()
            .map(|f| Ok((f.path.clone(), fs::read(source.join(&f.path))?)))
            .collect::<std::io::Result<Vec<_>>>()?;
        self.store_files(&files)
    }

    /// Stores a bundle given as its files, paths and bytes, such as one the
    /// app carries inside it.
    pub fn store_files(&self, files: &[(String, Vec<u8>)]) -> Result<BundleRecord, Error> {
        let listing = Listing::from_files(
            files.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
            Taken::AsBundle,
        )
        .map_err(|why| Error::invalid("/source", why))?;
        let hash = listing.hash();
        let manifest = files
            .iter()
            .find(|(p, _)| p == MANIFEST)
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .ok_or_else(|| Error::invalid("/source", format!("{MANIFEST} not found")))?;
        let parsed: Value = serde_json::from_str(&manifest)
            .map_err(|e| Error::invalid("/source", format!("{MANIFEST}: {e}")))?;
        let name = parsed["name"]
            .as_str()
            .ok_or_else(|| Error::invalid("/source", format!("{MANIFEST} has no name")))?;
        let version = parsed["version"].as_str().unwrap_or_default();
        if line_of(version).is_none() {
            return Err(Error::invalid(
                "/source",
                format!("{MANIFEST}: {version:?} is not a version such as 1.0.0"),
            ));
        }
        let record = BundleRecord {
            hash: hash.clone(),
            name: name.to_string(),
            version: version.to_string(),
            manifest,
            size: listing.size(),
            stored_at: String::new(),
        };

        let _files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let target = self.path(&hash);
        if target.is_dir() && self.db.touch_bundle(&hash)? {
            return Ok(self.db.bundle(&hash)?.unwrap_or(record));
        }
        let staging = self.dir.join(format!("{STAGING}{}", crate::id::next()));
        let placed = write_read_only(files, &staging).and_then(|()| {
            if target.exists() {
                remove(&target)?;
            }
            fs::rename(&staging, &target)
        });
        if let Err(error) = placed {
            let _ = remove(&staging);
            return Err(error.into());
        }
        self.db.insert_bundle(&record, &listing.text())?;
        Ok(self.db.bundle(&hash)?.unwrap_or(record))
    }

    /// The bundle's row, if it is stored.
    pub fn get(&self, hash: &str) -> Result<Option<BundleRecord>, Error> {
        if !crate::attachments::is_sha256(hash) {
            return Ok(None);
        }
        Ok(self.db.bundle(hash)?)
    }

    /// The listing a stored bundle's hash was taken over.
    pub fn listing(&self, hash: &str) -> Result<Option<Arc<Listing>>, Error> {
        if !crate::attachments::is_sha256(hash) {
            return Ok(None);
        }
        let mut listings = self.listings.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(listing) = listings.get(hash) {
            return Ok(Some(listing.clone()));
        }
        let Some(text) = self.db.bundle_listing(hash)? else {
            return Ok(None);
        };
        let listing = Arc::new(Listing::parse(&text).map_err(Error::Internal)?);
        listings.insert(hash.to_string(), listing.clone());
        Ok(Some(listing))
    }

    /// Whether a stored bundle's folder still holds exactly the files its
    /// listing names, byte for byte; what differs, if not.
    pub fn verify(&self, hash: &str) -> Result<(), Error> {
        let Some(listing) = self.listing(hash)? else {
            return Err(Error::NotFound(format!("bundle {hash}")));
        };
        let found = Listing::of_folder(&self.path(hash), Taken::AsBundle)
            .map_err(|why| Error::Internal(format!("bundle {hash}: {why}")))?;
        if found != *listing {
            return Err(Error::Internal(format!(
                "bundle {hash}: its files no longer match its listing"
            )));
        }
        Ok(())
    }

    /// Removes the bundles that were stored before `before` and that no
    /// installation or review refers to: the rows first, then the folders, so a stop between
    /// them leaves folders the next open removes. Returns how many went.
    pub fn sweep(&self, before: chrono::DateTime<chrono::Utc>) -> Result<usize, Error> {
        let _files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let gone = self.db.delete_unreferenced_bundles(before)?;
        let mut listings = self.listings.lock().unwrap_or_else(|e| e.into_inner());
        for hash in &gone {
            listings.remove(hash);
            let _ = remove(&self.path(hash));
        }
        Ok(gone.len())
    }
}

/// Writes the files into a new folder `to`, each made read-only.
fn write_read_only(files: &[(String, Vec<u8>)], to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for (path, bytes) in files {
        let target = to.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, bytes)?;
        let mut permissions = fs::metadata(&target)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&target, permissions)?;
    }
    Ok(())
}

/// Removes a folder of read-only files, which Windows does not delete as
/// they are.
fn remove(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)?.flatten() {
            remove(&entry.path())?;
        }
        fs::remove_dir(path)
    } else {
        let mut permissions = fs::symlink_metadata(path)?.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Bundles) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let bundles = Bundles::open(&dir.path().join("bundles"), db).unwrap();
        (dir, bundles)
    }

    /// A plugin's source folder, with what a source has beside its bundle.
    fn source(root: &Path, folder: &str, name: &str, version: &str) -> PathBuf {
        let dir = root.join(folder);
        for (path, text) in [
            (
                "manifest.json".to_string(),
                format!(r#"{{"name": "{name}", "version": "{version}"}}"#),
            ),
            ("schemas/payload.schema.json".into(), "{}".into()),
            ("schemas/decision.schema.json".into(), "{}".into()),
            ("view/index.html".into(), "<p>hello</p>".into()),
            ("src/main.ts".into(), "x".into()),
            ("node_modules/a/index.js".into(), "x".into()),
        ] {
            let to = dir.join(path);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::write(to, text).unwrap();
        }
        dir
    }

    /// Makes the bundle the current one of an installation, which is one
    /// thing that keeps a bundle.
    fn refer(bundles: &Bundles, bundle: &BundleRecord) {
        let record = crate::db::InstallRecord {
            plugin: format!("local/{}", bundle.name),
            publisher: "local".into(),
            name: bundle.name.clone(),
            kind: "folder".into(),
            source: "./hello".into(),
            resolved: "/hello".into(),
            commit: None,
            asset_hash: None,
            build_log: None,
            bundle: Some(bundle.hash.clone()),
            previous: None,
            previous_until: None,
            replaced: None,
            installed_at: "2026-10-01T10:00:00Z".into(),
            updated_at: "2026-10-01T10:00:00Z".into(),
        };
        bundles.db.record_install(&record).unwrap();
    }

    fn later() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() + chrono::Duration::minutes(1)
    }

    fn folders(bundles: &Bundles) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&bundles.dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_bundle_is_stored_once_by_its_hash_with_its_record() {
        let (root, bundles) = store();
        let from = source(root.path(), "a", "hello", "1.4.2");
        let stored = bundles.store(&from).unwrap();
        let listing = Listing::of_folder(&from, Taken::FromSource).unwrap();
        assert_eq!(stored.hash, listing.hash());
        assert_eq!(
            (stored.name.as_str(), stored.version.as_str()),
            ("hello", "1.4.2")
        );
        assert_eq!(stored.size, listing.size());
        assert_eq!(*bundles.listing(&stored.hash).unwrap().unwrap(), listing);
        // the bundle's files, and nothing the source has beside them
        let at = bundles.path(&stored.hash);
        assert!(at.join("view/index.html").is_file());
        assert!(!at.join("src").exists() && !at.join("node_modules").exists());
        bundles.verify(&stored.hash).unwrap();

        // the same files again, and the same files from another folder
        let again = bundles.store(&from).unwrap();
        let elsewhere = bundles
            .store(&source(root.path(), "b", "hello", "1.4.2"))
            .unwrap();
        assert_eq!(again.hash, stored.hash);
        assert_eq!(elsewhere.hash, stored.hash);
        assert_eq!(folders(&bundles), vec![stored.hash.clone()]);
        assert_eq!(bundles.db.bundle_hashes().unwrap().len(), 1);

        // another release is another bundle
        let next = bundles
            .store(&source(root.path(), "c", "hello", "0.3.1"))
            .unwrap();
        assert_ne!(next.hash, stored.hash);
        assert_eq!(folders(&bundles).len(), 2);
    }

    #[test]
    fn a_folder_without_a_name_or_a_version_is_not_stored() {
        let (root, bundles) = store();
        for manifest in [
            r#"{"version": "1.0.0"}"#,
            r#"{"name": "hello"}"#,
            r#"{"name": "hello", "version": "1.0"}"#,
            "not json",
        ] {
            let from = source(root.path(), "a", "hello", "1.0.0");
            fs::write(from.join("manifest.json"), manifest).unwrap();
            assert!(bundles.store(&from).is_err(), "{manifest}");
        }
        assert!(folders(&bundles).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_stored_bundle_is_read_only_and_a_changed_one_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let (root, bundles) = store();
        let stored = bundles
            .store(&source(root.path(), "a", "hello", "1.0.0"))
            .unwrap();
        let page = bundles.path(&stored.hash).join("view/index.html");
        let mode = fs::metadata(&page).unwrap().permissions().mode();
        assert_eq!(mode & 0o222, 0, "{mode:o} is writable");
        assert!(fs::write(&page, "changed").is_err());

        // changed by someone who made it writable first
        let mut permissions = fs::metadata(&page).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&page, permissions).unwrap();
        fs::write(&page, "<p>changed</p>").unwrap();
        let error = bundles.verify(&stored.hash).unwrap_err().to_string();
        assert!(error.contains("no longer match"), "{error}");

        // a file added beside the others
        let (root, bundles) = store();
        let stored = bundles
            .store(&source(root.path(), "a", "hello", "1.0.0"))
            .unwrap();
        fs::write(bundles.path(&stored.hash).join("view/extra.js"), "x").unwrap();
        assert!(bundles.verify(&stored.hash).is_err());
    }

    #[test]
    fn the_sweep_removes_what_nothing_refers_to_and_keeps_the_rest() {
        let (root, bundles) = store();
        let kept = bundles
            .store(&source(root.path(), "a", "hello", "1.0.0"))
            .unwrap();
        let unused = bundles
            .store(&source(root.path(), "b", "hello", "2.0.0"))
            .unwrap();
        refer(&bundles, &kept);

        // stored within the grace period: an install may be about to refer
        // to it, so an hour-old cutoff leaves both
        let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
        assert_eq!(bundles.sweep(hour_ago).unwrap(), 0);
        assert_eq!(folders(&bundles).len(), 2);

        assert_eq!(bundles.sweep(later()).unwrap(), 1);
        assert_eq!(folders(&bundles), vec![kept.hash.clone()]);
        assert!(bundles.get(&unused.hash).unwrap().is_none());
        assert!(bundles.listing(&unused.hash).unwrap().is_none());
        bundles.verify(&kept.hash).unwrap();

        // stored again after it went: back, whole
        let back = bundles
            .store(&source(root.path(), "b", "hello", "2.0.0"))
            .unwrap();
        assert_eq!(back.hash, unused.hash);
        bundles.verify(&back.hash).unwrap();
    }

    #[test]
    fn a_bundle_stored_during_sweeps_is_kept_whole() {
        let (root, bundles) = store();
        let sources: Vec<PathBuf> = (0..20)
            .map(|i| {
                source(
                    root.path(),
                    &format!("s{i}"),
                    "hello",
                    &format!("{}.0.0", i + 1),
                )
            })
            .collect();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sweeper = {
            let bundles = bundles.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    bundles.sweep(hour_ago).unwrap();
                }
            })
        };
        let stored: Vec<String> = sources
            .iter()
            .map(|from| bundles.store(from).unwrap().hash)
            .collect();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        sweeper.join().unwrap();
        for hash in &stored {
            bundles.verify(hash).unwrap();
        }
    }

    #[test]
    fn opening_removes_what_a_stop_left_behind() {
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let dir = root.path().join("bundles");
        let bundles = Bundles::open(&dir, db.clone()).unwrap();
        let stored = bundles
            .store(&source(root.path(), "a", "hello", "1.0.0"))
            .unwrap();
        // a copy staged but not placed, and one placed but not recorded
        fs::create_dir_all(dir.join(format!("{STAGING}x/view"))).unwrap();
        fs::create_dir_all(dir.join("0".repeat(64)).join("view")).unwrap();

        let bundles = Bundles::open(&dir, db).unwrap();
        assert_eq!(folders(&bundles), vec![stored.hash.clone()]);
        bundles.verify(&stored.hash).unwrap();
    }
}
