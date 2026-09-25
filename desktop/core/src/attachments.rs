//! Attachments: files an agent sends beside a review, such as a model, a PDF
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

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::error::{Error, Violation};

/// The most files one review may carry.
pub const MAX_COUNT: usize = 32;
/// The most one review's files may add up to: 512 MiB.
pub const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// How a payload names a file the review carries: an object with this one
/// key, `{ "$attachment": "pivot.glb" }`, which no text can be mistaken for.
pub const REFERENCE: &str = "$attachment";

/// A file a review carries: the name its payload knows it by, and the blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttachment {
    pub name: String,
    pub sha256: String,
    pub size: u64,
    pub media_type: String,
}

/// What a plugin takes, from its manifest's `attachments`: kinds as file
/// extensions (`.glb`) or media types (`model/gltf-binary`, `image/*`), and
/// limits no looser than the core's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRules {
    pub accept: Vec<String>,
    pub max_size: Option<u64>,
    pub max_count: Option<usize>,
}

impl AttachmentRules {
    /// The manifest's block, or why it cannot be used.
    pub fn parse(value: &Value) -> Result<Self, String> {
        let Some(block) = value.as_object() else {
            return Err("attachments: must be an object".into());
        };
        let accept: Vec<String> = match block.get("accept") {
            Some(Value::Array(kinds)) if !kinds.is_empty() => kinds
                .iter()
                .map(|k| match k.as_str() {
                    Some(k) if is_kind(k) => Ok(k.to_ascii_lowercase()),
                    _ => Err(format!("attachments.accept: {k} is neither an extension like .glb nor a media type like image/png")),
                })
                .collect::<Result<_, _>>()?,
            _ => return Err("attachments.accept: must list at least one extension or media type".into()),
        };
        let max_size = match block.get("max_size") {
            None => None,
            Some(v) => match v.as_u64() {
                Some(n) if n > 0 && n <= crate::config::MAX_ATTACHMENT_BYTES => Some(n),
                _ => {
                    return Err(format!(
                        "attachments.max_size: must be a number of bytes from 1 to {}",
                        crate::config::MAX_ATTACHMENT_BYTES
                    ));
                }
            },
        };
        let max_count = match block.get("max_count") {
            None => None,
            Some(v) => match v.as_u64() {
                Some(n) if n > 0 && n as usize <= MAX_COUNT => Some(n as usize),
                _ => {
                    return Err(format!(
                        "attachments.max_count: must be from 1 to {MAX_COUNT}"
                    ));
                }
            },
        };
        Ok(AttachmentRules {
            accept,
            max_size,
            max_count,
        })
    }

    /// Whether a file of this name and media type is a kind the plugin takes.
    pub fn accepts(&self, name: &str, media_type: &str) -> bool {
        let name = name.to_ascii_lowercase();
        let media_type = media_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        self.accept.iter().any(|kind| {
            if kind.starts_with('.') {
                name.ends_with(kind.as_str())
            } else if let Some(family) = kind.strip_suffix("/*") {
                media_type.split('/').next() == Some(family)
            } else {
                media_type == *kind
            }
        })
    }
}

/// `.ext`, `type/subtype` or `type/*`.
fn is_kind(kind: &str) -> bool {
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
    };
    match kind.strip_prefix('.') {
        Some(ext) => token(ext),
        None => {
            matches!(kind.split_once('/'), Some((t, sub)) if token(t) && (sub == "*" || token(sub)))
        }
    }
}

/// A file's name within its review: 1 to 120 characters, no path
/// separators or control characters, not starting with a dot.
pub fn is_name(name: &str) -> bool {
    (1..=120).contains(&name.chars().count())
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
}

/// Every `{ "$attachment": ... }` in the payload, with where it is and what
/// it holds (a name, if it is a string).
pub fn references(payload: &Value) -> Vec<(String, Value)> {
    fn walk(value: &Value, at: &str, found: &mut Vec<(String, Value)>) {
        match value {
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    walk(item, &format!("{at}/{i}"), found);
                }
            }
            Value::Object(map) => {
                if let Some(name) = map.get(REFERENCE) {
                    found.push((at.to_string(), name.clone()));
                    return;
                }
                for (key, item) in map {
                    walk(
                        item,
                        &format!("{at}/{}", key.replace('~', "~0").replace('/', "~1")),
                        found,
                    );
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(payload, "/payload", &mut found);
    found
}

/// Whether the files are only being described (a dry run), or must be
/// stored already (a submission).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Described,
    Stored,
}

#[derive(Debug, Clone)]
pub struct Attachments {
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

/// What a file is, by its extension: the kinds plugins are likely to
/// take, as the CLI names them. Anything else is plain bytes.
pub fn media_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "obj" => "model/obj",
        "stl" => "model/stl",
        "usdz" => "model/vnd.usdz+zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "json" => "application/json",
        "csv" => "text/csv",
        "txt" | "md" => "text/plain",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// True for 64 lowercase hex digits.
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Attachments {
    /// Opens the store under `dir`, emptying `tmp/` of uploads a stop cut
    /// short.
    pub fn open(dir: &Path, db: Arc<Db>, max_bytes: u64) -> Result<Self, Error> {
        let _ = fs::remove_dir_all(dir.join("tmp"));
        fs::create_dir_all(dir.join("tmp"))?;
        fs::create_dir_all(dir.join("sha256"))?;
        Ok(Attachments {
            dir: dir.to_path_buf(),
            db,
            max_bytes,
        })
    }

    /// How many files are stored, and their bytes: what Settings › History
    /// says the reviews' files take.
    pub fn totals(&self) -> Result<(u64, u64), Error> {
        Ok(self.db.blob_totals()?)
    }

    /// The most one attachment may be, in bytes.
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Reads and checks a submission's `attachments` against the plugin's
    /// rules, the core's caps and the payload's references. With
    /// [`Presence::Stored`], every blob must be here with its size.
    pub fn check(
        &self,
        declared: Option<&Value>,
        rules: Option<&AttachmentRules>,
        payload: &Value,
        presence: Presence,
    ) -> Result<Vec<ReviewAttachment>, Vec<Violation>> {
        let mut violations = Vec::new();
        let empty = Map::new();
        let map = match declared {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(map)) => map,
            Some(_) => {
                return Err(vec![Violation::new(
                    "/attachments",
                    "must be an object of name to {sha256, size, media_type}",
                )]);
            }
        };
        if !map.is_empty() && rules.is_none() {
            return Err(vec![Violation::new(
                "/attachments",
                "this plugin takes no attachments",
            )]);
        }
        let max_size = rules
            .and_then(|r| r.max_size)
            .map_or(self.max_bytes, |m| m.min(self.max_bytes));
        let max_count = rules.and_then(|r| r.max_count).unwrap_or(MAX_COUNT);
        if map.len() > max_count {
            violations.push(Violation::new(
                "/attachments",
                format!("at most {max_count} attachments, not {}", map.len()),
            ));
        }
        let mut attachments = Vec::new();
        let mut total = 0u64;
        for (name, entry) in map {
            let at = format!(
                "/attachments/{}",
                name.replace('~', "~0").replace('/', "~1")
            );
            if !is_name(name) {
                violations.push(Violation::new(
                    &at,
                    "a name is 1 to 120 characters, with no / or \\, not starting with a dot",
                ));
                continue;
            }
            let sha256 = entry
                .get("sha256")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !is_sha256(sha256) {
                violations.push(Violation::new(
                    format!("{at}/sha256"),
                    "must be 64 lowercase hex digits",
                ));
                continue;
            }
            let Some(size) = entry.get("size").and_then(Value::as_u64) else {
                violations.push(Violation::new(
                    format!("{at}/size"),
                    "must be the number of bytes",
                ));
                continue;
            };
            let media_type = match entry.get("media_type") {
                None | Some(Value::Null) => "application/octet-stream".to_string(),
                Some(Value::String(t)) if t.contains('/') => t.clone(),
                Some(_) => {
                    violations.push(Violation::new(
                        format!("{at}/media_type"),
                        "must be a media type like model/gltf-binary",
                    ));
                    continue;
                }
            };
            if let Some(rules) = rules
                && !rules.accepts(name, &media_type)
            {
                violations.push(Violation::new(
                    &at,
                    format!(
                        "this plugin takes {}, not {media_type}",
                        rules.accept.join(", ")
                    ),
                ));
            }
            if size > max_size {
                violations.push(Violation::new(
                    format!("{at}/size"),
                    format!("an attachment may be {} bytes at most", max_size),
                ));
            }
            total += size;
            if presence == Presence::Stored {
                match self.stored(sha256) {
                    Ok(Some(blob)) if blob.size == size => {}
                    Ok(Some(blob)) => violations.push(Violation::new(
                        format!("{at}/size"),
                        format!("the stored blob is {} bytes, not {size}", blob.size),
                    )),
                    Ok(None) => violations.push(Violation::new(
                        format!("{at}/sha256"),
                        "not uploaded: PUT /api/v1/attachments/{sha256} first",
                    )),
                    Err(e) => {
                        violations.push(Violation::new(format!("{at}/sha256"), e.to_string()))
                    }
                }
            }
            attachments.push(ReviewAttachment {
                name: name.clone(),
                sha256: sha256.to_string(),
                size,
                media_type,
            });
        }
        if total > MAX_TOTAL_BYTES {
            violations.push(Violation::new(
                "/attachments",
                format!(
                    "a review's attachments may add up to {MAX_TOTAL_BYTES} bytes, not {total}"
                ),
            ));
        }
        // only a plugin that takes files has references to check: to any
        // other, `$attachment` is just data
        if rules.is_some() {
            for (at, name) in references(payload) {
                match name.as_str() {
                    Some(name) if map.contains_key(name) => {}
                    Some(name) => violations.push(Violation::new(
                        at,
                        format!("no attachment \"{name}\" on this review"),
                    )),
                    None => violations.push(Violation::new(
                        format!("{at}/$attachment"),
                        "must be the name of an attachment on this review",
                    )),
                }
            }
        }
        if violations.is_empty() {
            Ok(attachments)
        } else {
            Err(violations)
        }
    }

    /// Deletes blobs no review names that were stored before `before`: what
    /// a swept review left, and uploads nothing was submitted with. The row
    /// goes before the file, so a stop between them leaves a file for the
    /// next upload of it to replace. Returns how many went.
    pub fn sweep(&self, before: chrono::DateTime<chrono::Utc>) -> Result<usize, Error> {
        let orphans = self.db.orphan_blobs(before)?;
        for sha256 in &orphans {
            self.db.delete_blob(sha256)?;
            let path = self.path(sha256);
            if let Ok(meta) = fs::metadata(&path) {
                let mut permissions = meta.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = fs::set_permissions(&path, permissions);
            }
            let _ = fs::remove_file(&path);
        }
        Ok(orphans.len())
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

    fn store(max: u64) -> (tempfile::TempDir, Attachments) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::in_memory().unwrap());
        let attachments = Attachments::open(&dir.path().join("attachments"), db, max).unwrap();
        (dir, attachments)
    }

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn tmp_is_empty(attachments: &Attachments) -> bool {
        fs::read_dir(attachments.dir.join("tmp"))
            .unwrap()
            .next()
            .is_none()
    }

    #[test]
    fn an_upload_is_stored_read_only_under_its_hash() {
        let (_dir, attachments) = store(1024);
        let hash = sha(b"hello model");
        let mut upload = attachments.begin(&hash).unwrap();
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
        let path = attachments.path(&hash);
        assert!(path.ends_with(format!("sha256/{}/{hash}", &hash[..2])));
        assert_eq!(fs::read(&path).unwrap(), b"hello model");
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
        assert_eq!(attachments.stored(&hash).unwrap(), Some(blob));
        assert!(tmp_is_empty(&attachments));
        // the same content again is the same file
        let mut again = attachments.begin(&hash).unwrap();
        again.write(b"hello model").unwrap();
        again.finish().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello model");
    }

    #[test]
    fn bytes_that_hash_to_another_name_are_not_kept() {
        let (_dir, attachments) = store(1024);
        let claimed = sha(b"what was promised");
        let mut upload = attachments.begin(&claimed).unwrap();
        upload.write(b"something else").unwrap();
        match upload.finish() {
            Err(UploadError::Mismatch { sent, actual }) => {
                assert_eq!(sent, claimed);
                assert_eq!(actual, sha(b"something else"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(attachments.stored(&claimed).unwrap(), None);
        assert!(!attachments.path(&claimed).exists());
        assert!(tmp_is_empty(&attachments));
    }

    #[test]
    fn an_upload_past_the_cap_stops_and_leaves_nothing() {
        let (_dir, attachments) = store(8);
        let hash = sha(b"far too long");
        let mut upload = attachments.begin(&hash).unwrap();
        upload.write(b"far too ").unwrap();
        assert!(matches!(
            upload.write(b"long"),
            Err(UploadError::TooLarge { limit: 8 })
        ));
        drop(upload);
        assert!(tmp_is_empty(&attachments));
        assert_eq!(attachments.stored(&hash).unwrap(), None);
    }

    #[test]
    fn a_name_that_is_not_a_hash_is_refused() {
        let (_dir, attachments) = store(8);
        for bad in [
            "",
            "abc",
            &"A".repeat(64),
            &format!("{}/x", "a".repeat(62)),
            "../../../etc/passwd",
        ] {
            assert!(attachments.begin(bad).is_err(), "{bad}");
            assert_eq!(attachments.stored(bad).unwrap(), None);
        }
    }

    #[test]
    fn opening_empties_uploads_a_stop_cut_short() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("attachments");
        fs::create_dir_all(root.join("tmp")).unwrap();
        fs::write(root.join("tmp").join("r_half"), b"half an upload").unwrap();
        let attachments = Attachments::open(&root, Arc::new(Db::in_memory().unwrap()), 8).unwrap();
        assert!(tmp_is_empty(&attachments));
    }

    #[test]
    fn rules_read_kinds_and_limits_no_looser_than_the_core() {
        let rules = AttachmentRules::parse(&serde_json::json!({ "accept": [".GLB", "image/*", "model/gltf-binary"], "max_size": 1024, "max_count": 3 })).unwrap();
        assert_eq!(rules.accept, [".glb", "image/*", "model/gltf-binary"]);
        assert_eq!((rules.max_size, rules.max_count), (Some(1024), Some(3)));
        for bad in [
            serde_json::json!([".glb"]),
            serde_json::json!({ "accept": [] }),
            serde_json::json!({ "accept": ["glb"] }),
            serde_json::json!({ "accept": ["image/"] }),
            serde_json::json!({ "accept": [".glb"], "max_size": 0 }),
            serde_json::json!({ "accept": [".glb"], "max_size": crate::config::MAX_ATTACHMENT_BYTES + 1 }),
            serde_json::json!({ "accept": [".glb"], "max_count": MAX_COUNT + 1 }),
        ] {
            assert!(AttachmentRules::parse(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_file_is_taken_by_its_extension_or_its_media_type() {
        let rules = AttachmentRules::parse(
            &serde_json::json!({ "accept": [".glb", "image/*", "application/pdf"] }),
        )
        .unwrap();
        assert!(rules.accepts("Pivot.GLB", "application/octet-stream"));
        assert!(rules.accepts("desk", "image/jpeg"));
        assert!(rules.accepts("brief", "application/pdf; charset=binary"));
        assert!(!rules.accepts("pivot.glb.html", "text/html"));
        assert!(!rules.accepts("clip.mp4", "video/mp4"));
    }

    #[test]
    fn names_are_plain_file_names() {
        for ok in [
            "pivot.glb",
            "Desk photo (2).jpg",
            "ünïcode.png",
            &"a".repeat(120),
        ] {
            assert!(is_name(ok), "{ok}");
        }
        for bad in [
            "",
            ".hidden",
            "..",
            "a/b.glb",
            "a\\b.glb",
            "tab\there",
            &"a".repeat(121),
        ] {
            assert!(!is_name(bad), "{bad}");
        }
    }

    #[test]
    fn references_are_found_wherever_the_payload_has_them() {
        let payload = serde_json::json!({
            "models": [{ "file": { "$attachment": "pivot.glb" } }, { "object": { "uuid": "x" } }],
            "a/b": { "x~y": { "$attachment": "desk.jpg" } },
            "note": "attachment:linux-x64 is text, not a reference",
            "odd": { "$attachment": 7 },
        });
        let mut found = references(&payload);
        found.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            found,
            [
                (
                    "/payload/a~1b/x~0y".to_string(),
                    serde_json::json!("desk.jpg")
                ),
                (
                    "/payload/models/0/file".to_string(),
                    serde_json::json!("pivot.glb")
                ),
                ("/payload/odd".to_string(), serde_json::json!(7)),
            ]
        );
    }
}
