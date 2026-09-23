//! Artifacts: files an agent sends beside a review, such as a model, a PDF
//! or a recording, stored once by their SHA-256.
//!
//! An upload is written to `tmp/` while it is hashed and counted, and
//! refused as soon as it passes the size cap. Once whole, and only if its
//! hash is the one it was sent under, it moves to `sha256/<ab>/<hash>`,
//! read-only, and is recorded. The same content uploaded again is the same
//! file. What reviews carry which blob is recorded with the review.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::error::Error;

#[derive(Debug, Clone)]
pub struct Artifacts {
    dir: PathBuf,
    db: Arc<Db>,
    max_bytes: u64,
}

/// Why an upload stopped.
#[derive(Debug)]
pub enum UploadError {
    /// More bytes than the cap; nothing is kept.
    TooLarge {
        limit: u64,
    },
    /// The bytes do not hash to the name they were sent under.
    Mismatch {
        sent: String,
        actual: String,
    },
    Failed(Error),
}

impl From<std::io::Error> for UploadError {
    fn from(error: std::io::Error) -> Self {
        UploadError::Failed(error.into())
    }
}

/// An upload in flight: bytes go in chunk by chunk, and `finish` stores
/// them. Dropped unfinished, it leaves nothing behind.
pub struct Upload {
    sha256: String,
    tmp: PathBuf,
    file: Option<File>,
    hasher: Sha256,
    size: u64,
    max_bytes: u64,
    stored: Option<PathBuf>,
    db: Arc<Db>,
}

/// A stored blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub sha256: String,
    pub size: u64,
}

/// True for 64 lowercase hex digits.
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Artifacts {
    /// Opens the store under `dir`, emptying `tmp/` of uploads a stop cut
    /// short.
    pub fn open(dir: &Path, db: Arc<Db>, max_bytes: u64) -> Result<Self, Error> {
        let _ = fs::remove_dir_all(dir.join("tmp"));
        fs::create_dir_all(dir.join("tmp"))?;
        fs::create_dir_all(dir.join("sha256"))?;
        Ok(Artifacts {
            dir: dir.to_path_buf(),
            db,
            max_bytes,
        })
    }

    /// The most one artifact may be, in bytes.
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Where a blob's bytes are.
    pub fn path(&self, sha256: &str) -> PathBuf {
        self.dir.join("sha256").join(&sha256[..2]).join(sha256)
    }

    /// The blob, if it is recorded and its file is there.
    pub fn stored(&self, sha256: &str) -> Result<Option<Blob>, Error> {
        if !is_sha256(sha256) {
            return Ok(None);
        }
        let Some(size) = self.db.blob_size(sha256)? else {
            return Ok(None);
        };
        Ok(self.path(sha256).is_file().then(|| Blob {
            sha256: sha256.to_string(),
            size,
        }))
    }

    /// Starts an upload of content that should hash to `sha256`.
    pub fn begin(&self, sha256: &str) -> Result<Upload, Error> {
        if !is_sha256(sha256) {
            return Err(Error::invalid("/sha256", "must be 64 lowercase hex digits"));
        }
        let tmp = self.dir.join("tmp").join(crate::id::next());
        Ok(Upload {
            sha256: sha256.to_string(),
            file: Some(File::create(&tmp)?),
            tmp,
            hasher: Sha256::new(),
            size: 0,
            max_bytes: self.max_bytes,
            stored: Some(self.path(sha256)),
            db: self.db.clone(),
        })
    }
}

impl Upload {
    /// Adds a chunk; past the cap, the upload is over.
    pub fn write(&mut self, chunk: &[u8]) -> Result<(), UploadError> {
        self.size += chunk.len() as u64;
        if self.size > self.max_bytes {
            return Err(UploadError::TooLarge {
                limit: self.max_bytes,
            });
        }
        self.hasher.update(chunk);
        self.file
            .as_mut()
            .expect("written after finish")
            .write_all(chunk)?;
        Ok(())
    }

    /// Checks the hash, moves the file into place and records it.
    pub fn finish(mut self) -> Result<Blob, UploadError> {
        let mut file = self.file.take().expect("finished twice");
        file.flush()?;
        drop(file);
        let actual = format!("{:x}", std::mem::take(&mut self.hasher).finalize());
        if actual != self.sha256 {
            return Err(UploadError::Mismatch {
                sent: self.sha256.clone(),
                actual,
            });
        }
        let target = self.stored.take().expect("finished twice");
        fs::create_dir_all(target.parent().unwrap())?;
        let mut permissions = fs::metadata(&self.tmp)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&self.tmp, permissions)?;
        fs::rename(&self.tmp, &target)?;
        // the file first, then the row: a stop between them leaves a file
        // no row names, which the next upload of it replaces
        self.db
            .insert_blob(&self.sha256, self.size)
            .map_err(|e| UploadError::Failed(e.into()))?;
        Ok(Blob {
            sha256: self.sha256.clone(),
            size: self.size,
        })
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(max: u64) -> (tempfile::TempDir, Artifacts) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let artifacts = Artifacts::open(&dir.path().join("artifacts"), db, max).unwrap();
        (dir, artifacts)
    }

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn tmp_is_empty(artifacts: &Artifacts) -> bool {
        fs::read_dir(artifacts.dir.join("tmp"))
            .unwrap()
            .next()
            .is_none()
    }

    #[test]
    fn an_upload_is_stored_read_only_under_its_hash() {
        let (_dir, artifacts) = store(1024);
        let hash = sha(b"hello model");
        let mut upload = artifacts.begin(&hash).unwrap();
        upload.write(b"hello ").unwrap();
        upload.write(b"model").unwrap();
        let blob = upload.finish().unwrap();
        assert_eq!(
            blob,
            Blob {
                sha256: hash.clone(),
                size: 11
            }
        );
        let path = artifacts.path(&hash);
        assert!(path.ends_with(format!("sha256/{}/{hash}", &hash[..2])));
        assert_eq!(fs::read(&path).unwrap(), b"hello model");
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
        assert_eq!(artifacts.stored(&hash).unwrap(), Some(blob));
        assert!(tmp_is_empty(&artifacts));
        // the same content again is the same file
        let mut again = artifacts.begin(&hash).unwrap();
        again.write(b"hello model").unwrap();
        again.finish().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello model");
    }

    #[test]
    fn bytes_that_hash_to_another_name_are_not_kept() {
        let (_dir, artifacts) = store(1024);
        let claimed = sha(b"what was promised");
        let mut upload = artifacts.begin(&claimed).unwrap();
        upload.write(b"something else").unwrap();
        match upload.finish() {
            Err(UploadError::Mismatch { sent, actual }) => {
                assert_eq!(sent, claimed);
                assert_eq!(actual, sha(b"something else"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(artifacts.stored(&claimed).unwrap(), None);
        assert!(!artifacts.path(&claimed).exists());
        assert!(tmp_is_empty(&artifacts));
    }

    #[test]
    fn an_upload_past_the_cap_stops_and_leaves_nothing() {
        let (_dir, artifacts) = store(8);
        let hash = sha(b"far too long");
        let mut upload = artifacts.begin(&hash).unwrap();
        upload.write(b"far too ").unwrap();
        assert!(matches!(
            upload.write(b"long"),
            Err(UploadError::TooLarge { limit: 8 })
        ));
        drop(upload);
        assert!(tmp_is_empty(&artifacts));
        assert_eq!(artifacts.stored(&hash).unwrap(), None);
    }

    #[test]
    fn a_name_that_is_not_a_hash_is_refused() {
        let (_dir, artifacts) = store(8);
        for bad in [
            "",
            "abc",
            &"A".repeat(64),
            &format!("{}/x", "a".repeat(62)),
            "../../../etc/passwd",
        ] {
            assert!(artifacts.begin(bad).is_err(), "{bad}");
            assert_eq!(artifacts.stored(bad).unwrap(), None);
        }
    }

    #[test]
    fn opening_empties_uploads_a_stop_cut_short() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("artifacts");
        fs::create_dir_all(root.join("tmp")).unwrap();
        fs::write(root.join("tmp").join("r_half"), b"half an upload").unwrap();
        let artifacts = Artifacts::open(&root, Arc::new(Db::in_memory().unwrap()), 8).unwrap();
        assert!(tmp_is_empty(&artifacts));
    }
}
