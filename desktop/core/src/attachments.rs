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
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::error::{Error, Violation};

pub use pinrail_format::attachments::{AttachmentRules, MAX_COUNT, media_type};

/// The most one review's files may add up to: 512 MB.
pub const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// The most files one note may carry.
pub const NOTE_MAX_COUNT: usize = 10;
/// The most one of a note's files may be: 100 MB.
pub const NOTE_MAX_BYTES: u64 = 100 * 1024 * 1024;
/// The most a note's files may add up to: 200 MB.
pub const NOTE_MAX_TOTAL_BYTES: u64 = 200 * 1024 * 1024;
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

/// A file's name within its review: 1 to 120 characters, no path
/// separators or control characters, not starting with a dot.
pub fn is_name(name: &str) -> bool {
    (1..=120).contains(&name.chars().count())
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
}

/// What a set of files may be: how many, how big each and in all, and
/// whose files they are, for the messages.
struct Limits {
    max_size: u64,
    max_count: usize,
    max_total: u64,
    whose: &'static str,
}

/// A submission's `attachments`: an object of name to file, or nothing.
fn declared_map<'a>(
    declared: Option<&'a Value>,
    empty: &'a Map<String, Value>,
) -> Result<&'a Map<String, Value>, Error> {
    match declared {
        None | Some(Value::Null) => Ok(empty),
        Some(Value::Object(map)) => Ok(map),
        Some(_) => Err(Error::Invalid(vec![Violation::new(
            "/attachments",
            "must be an object of name to {sha256, size, media_type}",
        )])),
    }
}

/// The names a note's Markdown refers to as `](attachment:NAME)`, in an
/// image or a link. One in a code span or a code block is text, as the
/// Markdown shows it.
pub fn note_references(markdown: &str) -> Vec<String> {
    let mut names = Vec::new();
    let prose = outside_code(markdown);
    let mut rest = prose.as_str();
    while let Some(at) = rest.find("](attachment:") {
        rest = &rest[at + "](attachment:".len()..];
        let end = rest.find([')', ' ', '\n']).unwrap_or(rest.len());
        let name = &rest[..end];
        if !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        rest = &rest[end..];
    }
    names
}

/// The Markdown with its code blocks, fenced with ``` or ~~~, and its code
/// spans blanked out, so that what they show is not read as Markdown.
fn outside_code(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut fence: Option<String> = None;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = |c: char| {
            let run: String = trimmed.chars().take_while(|&x| x == c).collect();
            (run.len() >= 3).then_some(run)
        };
        match &fence {
            Some(open) => {
                if trimmed.trim_end().starts_with(open.as_str())
                    && trimmed
                        .trim_end()
                        .chars()
                        .all(|c| c == open.chars().next().unwrap())
                {
                    fence = None;
                }
                out.push('\n');
            }
            None => {
                if let Some(open) = marker('`').or_else(|| marker('~')) {
                    fence = Some(open);
                    out.push('\n');
                } else {
                    out.push_str(&without_code_spans(line));
                }
            }
        }
    }
    out
}

/// A line with its code spans, between runs of backticks of the same
/// length, blanked out.
fn without_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('`') {
        out.push_str(&rest[..at]);
        let run = rest[at..].chars().take_while(|&c| c == '`').count();
        let after = &rest[at + run..];
        let closing = "`".repeat(run);
        match after.find(&closing) {
            Some(end) => rest = &after[end + run..],
            None => {
                out.push_str(&rest[at..at + run]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
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
    /// Held while a blob's row and file change together: an upload
    /// finishing, a blob confirmed as stored, and the sweep. None of them
    /// can then land between another's two steps.
    files: Arc<Mutex<()>>,
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
    files: Arc<Mutex<()>>,
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

impl Attachments {
    /// Opens the store under `dir`, emptying `tmp/` of uploads a stop cut
    /// short.
    pub fn open(dir: &Path, db: Arc<Db>, max_bytes: u64) -> Result<Self, Error> {
        let _ = fs::remove_dir_all(dir.join("tmp"));
        fs::create_dir_all(dir.join("tmp"))?;
        fs::create_dir_all(dir.join("sha256"))?;
        let attachments = Attachments {
            dir: dir.to_path_buf(),
            db,
            max_bytes,
            files: Arc::default(),
        };
        attachments.remove_unrecorded()?;
        Ok(attachments)
    }

    /// Removes stored files that no row names: an upload is stored before it
    /// is recorded, so a stop between the two, or a removal the sweep could
    /// not finish, leaves one behind that nothing would find again. A file
    /// younger than an hour is kept, as the sweep keeps an upload that long.
    fn remove_unrecorded(&self) -> Result<(), Error> {
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let Ok(prefixes) = fs::read_dir(self.dir.join("sha256")) else {
            return Ok(());
        };
        for prefix in prefixes.flatten() {
            let Ok(files) = fs::read_dir(prefix.path()) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                let old = file
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|at| at < hour_ago);
                if old && self.db.blob_size(&name)?.is_none() {
                    let path = file.path();
                    writable(&path);
                    let _ = fs::remove_file(&path);
                }
            }
        }
        Ok(())
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
    ) -> Result<Vec<ReviewAttachment>, Error> {
        let mut violations = Vec::new();
        let empty = Map::new();
        let map = declared_map(declared, &empty)?;
        if !map.is_empty() && rules.is_none() {
            return Err(Error::Invalid(vec![Violation::new(
                "/attachments",
                "this plugin takes no attachments",
            )]));
        }
        let limits = Limits {
            max_size: rules
                .and_then(|r| r.max_size)
                .map_or(self.max_bytes, |m| m.min(self.max_bytes)),
            max_count: rules.and_then(|r| r.max_count).unwrap_or(MAX_COUNT),
            max_total: MAX_TOTAL_BYTES,
            whose: "a review's attachments",
        };
        let attachments = self.entries(map, &limits, rules, presence, &mut violations)?;
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
            Err(Error::Invalid(violations))
        }
    }

    /// Reads and checks a note's `attachments` against a note's caps and the
    /// files its Markdown refers to as `attachment:NAME`. Every blob must be
    /// here with its size. `field` is where the Markdown is and what it is,
    /// such as `("/text", "the note")`, for the violations.
    pub fn check_note(
        &self,
        declared: Option<&Value>,
        markdown: &str,
        field: (&str, &str),
    ) -> Result<Vec<ReviewAttachment>, Error> {
        let mut violations = Vec::new();
        let empty = Map::new();
        let map = declared_map(declared, &empty)?;
        let limits = Limits {
            max_size: NOTE_MAX_BYTES.min(self.max_bytes),
            max_count: NOTE_MAX_COUNT,
            max_total: NOTE_MAX_TOTAL_BYTES,
            whose: "a note's files",
        };
        let attachments = self.entries(map, &limits, None, Presence::Stored, &mut violations)?;
        for name in note_references(markdown) {
            if !map.contains_key(&name) {
                violations.push(Violation::new(
                    field.0,
                    format!(
                        "refers to attachment:{name}, which {} does not carry",
                        field.1
                    ),
                ));
            }
        }
        if violations.is_empty() {
            Ok(attachments)
        } else {
            Err(Error::Invalid(violations))
        }
    }

    /// Checks each declared file against the limits and, with `rules`, the
    /// plugin's accepted types, and with [`Presence::Stored`] that it is
    /// here with its size. Adds what is wrong to `violations`.
    fn entries(
        &self,
        map: &Map<String, Value>,
        limits: &Limits,
        rules: Option<&AttachmentRules>,
        presence: Presence,
        violations: &mut Vec<Violation>,
    ) -> Result<Vec<ReviewAttachment>, Error> {
        let max_count = limits.max_count;
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
            if size > limits.max_size {
                violations.push(Violation::new(
                    format!("{at}/size"),
                    format!("an attachment may be {} bytes at most", limits.max_size),
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
                    // the store failing is not the submission's fault
                    Err(e) => return Err(e),
                }
            }
            attachments.push(ReviewAttachment {
                name: name.clone(),
                sha256: sha256.to_string(),
                size,
                media_type,
            });
        }
        if total > limits.max_total {
            violations.push(Violation::new(
                "/attachments",
                format!(
                    "{} may add up to {} bytes, not {total}",
                    limits.whose, limits.max_total
                ),
            ));
        }
        Ok(attachments)
    }

    /// Deletes blobs no review names that were stored before `before`: what
    /// a swept review left, and uploads nothing was submitted with. The rows
    /// go before the files, so a stop between them leaves files for the
    /// next upload of each to replace. Returns how many went.
    pub fn sweep(&self, before: chrono::DateTime<chrono::Utc>) -> Result<usize, Error> {
        let _files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let orphans = self.db.delete_orphan_blobs(before)?;
        for sha256 in &orphans {
            let path = self.path(sha256);
            writable(&path);
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

    /// The blob, if it is stored, made new again so the sweep leaves it for
    /// another hour: what an agent is told before it submits, by an upload
    /// of content already there or by asking.
    pub fn confirm(&self, sha256: &str) -> Result<Option<Blob>, Error> {
        let _files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let Some(blob) = self.stored(sha256)? else {
            return Ok(None);
        };
        Ok(self.db.touch_blob(sha256)?.then_some(blob))
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
            files: self.files.clone(),
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
        let files = self.files.clone();
        let _files = files.lock().unwrap_or_else(|e| e.into_inner());
        fs::create_dir_all(target.parent().unwrap())?;
        let mut permissions = fs::metadata(&self.tmp)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&self.tmp, permissions)?;
        // a copy already stored is replaced by this one, the same bytes:
        // Windows renames over a file only when it is not read-only
        writable(&target);
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

/// Makes a stored file writable, which Windows needs before it deletes or
/// replaces it. A file that is not there is left as it is.
fn writable(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut permissions = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
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
    fn a_note_refers_to_files_outside_code_only() {
        let markdown = "![a](attachment:a.png) and `[b](attachment:b.log)` and \
                        ``a `[c](attachment:c.log)` span`` and an odd ` tick [d](attachment:d.log)\n\
                        ```\n[e](attachment:e.log)\n```\n\
                        [f](attachment:f.log)\n\
                        ~~~~\n~~~\n[g](attachment:g.log)\n~~~~\n\
                        [h](attachment:h.log)";
        assert_eq!(
            note_references(markdown),
            ["a.png", "d.log", "f.log", "h.log"]
        );
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

    /// The definitions of a reference to a file in a payload schema: every
    /// object in it with an `$attachment` property.
    fn file_fields(schema: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        match schema {
            serde_json::Value::Object(map) => {
                if map
                    .get("properties")
                    .and_then(|p| p.get(REFERENCE))
                    .is_some()
                {
                    out.push(schema.clone());
                }
                map.values().for_each(|v| file_fields(v, out));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| file_fields(v, out)),
            _ => {}
        }
    }

    /// The parts of a definition that decide what it accepts, without its
    /// title and description.
    fn rules(definition: &serde_json::Value) -> serde_json::Value {
        let mut rules = definition.clone();
        if let Some(map) = rules.as_object_mut() {
            for key in ["$schema", "$id", "title", "description"] {
                map.remove(key);
            }
        }
        rules
    }

    /// A plugin describes a file field with a copy of the SDK's attachment
    /// schema, so every copy has to accept what the SDK's does, and the
    /// SDK's has to agree with the names the app takes.
    #[test]
    fn the_plugins_copies_of_the_attachment_schema_match_the_sdks() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let read = |path: &std::path::Path| -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
        };
        let sdk = read(&repo.join("sdk/schemas/attachment.schema.json"));

        let limits = &sdk["properties"][REFERENCE];
        let (min, max) = (
            limits["minLength"].as_u64().unwrap() as usize,
            limits["maxLength"].as_u64().unwrap() as usize,
        );
        assert!(is_name(&"a".repeat(min)) && !is_name(&"a".repeat(min.saturating_sub(1))));
        assert!(is_name(&"a".repeat(max)) && !is_name(&"a".repeat(max + 1)));

        let mut checked = 0;
        for entry in std::fs::read_dir(repo.join("plugins")).unwrap() {
            let schema = entry.unwrap().path().join("schemas/payload.schema.json");
            if !schema.is_file() {
                continue;
            }
            let mut fields = Vec::new();
            file_fields(&read(&schema), &mut fields);
            for field in fields {
                assert_eq!(rules(&field), rules(&sdk), "{}", schema.display());
                checked += 1;
            }
        }
        assert!(checked > 0, "the official plugins take files");
    }
}
