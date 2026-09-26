//! Files sent beside a review: where they are on disk, what they are, and
//! getting them to the app. A file is hashed first, so the submission can
//! be checked before anything big moves, and uploaded only when the app
//! does not have it yet: a new round re-sends only what changed.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::api::Client;

/// A file to send: the name the payload knows it by, and what it is.
pub struct Local {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub media_type: String,
}

/// Where a file to send is, and what it is when the request says so.
#[derive(Debug, PartialEq)]
pub struct Source {
    pub path: PathBuf,
    pub media_type: Option<String>,
}

/// `PATH` or `PATH=NAME`, as `--attach` takes it; the name defaults to
/// the file's own.
pub fn parse_flag(spec: &str) -> Result<(String, PathBuf), String> {
    let (path, name) = match spec.rsplit_once('=') {
        Some((path, name)) if !path.is_empty() && !name.is_empty() => {
            (PathBuf::from(path), name.to_string())
        }
        _ => {
            let path = PathBuf::from(spec);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| format!("{spec}: not a file path"))?;
            (path, name)
        }
    };
    Ok((name, path))
}

/// The files to send, by name: the request file's `attachments` (paths
/// relative to the file) and then the flags, a flag replacing a file entry
/// of the same name. Two flags with one name is a mistake.
///
/// An entry is a path, or `{"path": …, "media_type": …}` as a plugin's
/// fixture has it, so a fixture can be sent as it is.
pub fn collect(
    from_request: Option<&Value>,
    request_dir: &Path,
    flags: &[(String, PathBuf)],
) -> Result<BTreeMap<String, Source>> {
    let mut files = BTreeMap::new();
    match from_request {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (name, entry) in map {
                let (path, media_type) = match entry {
                    Value::String(path) => (path.as_str(), None),
                    Value::Object(o) => (
                        o.get("path").and_then(Value::as_str).with_context(|| {
                            format!("the request's attachments: {name} needs a path")
                        })?,
                        o.get("media_type")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    ),
                    _ => anyhow::bail!(
                        "the request's attachments: {name} must be a file path or {{\"path\": …}}"
                    ),
                };
                let path = request_dir.join(path);
                files.insert(name.clone(), Source { path, media_type });
            }
        }
        Some(_) => anyhow::bail!("the request's attachments must map a name to a file path"),
    }
    let mut flagged = std::collections::BTreeSet::new();
    for (name, path) in flags {
        anyhow::ensure!(
            flagged.insert(name.clone()),
            "two --attach flags name {name}; give one of them another name with PATH=NAME"
        );
        let source = Source {
            path: path.clone(),
            media_type: None,
        };
        files.insert(name.clone(), source);
    }
    Ok(files)
}

/// Hashes each file and says what it is.
pub fn read(files: &BTreeMap<String, Source>) -> Result<Vec<Local>> {
    files
        .iter()
        .map(
            |(
                name,
                Source {
                    path,
                    media_type: given,
                },
            )| {
                let mut file =
                    File::open(path).with_context(|| format!("reading {}", path.display()))?;
                let mut hasher = Sha256::new();
                let size = std::io::copy(&mut file, &mut hasher)
                    .with_context(|| format!("reading {}", path.display()))?;
                Ok(Local {
                    name: name.clone(),
                    path: path.clone(),
                    sha256: format!("{:x}", hasher.finalize()),
                    size,
                    media_type: given
                        .clone()
                        .unwrap_or_else(|| media_type(name).to_string()),
                })
            },
        )
        .collect()
}

/// The request's `attachments`, as the API takes it.
pub fn declare(files: &[Local]) -> Value {
    let map: Map<String, Value> = files
        .iter()
        .map(|f| {
            (
                f.name.clone(),
                json!({ "sha256": f.sha256, "size": f.size, "media_type": f.media_type }),
            )
        })
        .collect();
    Value::Object(map)
}

/// Uploads what the app does not have yet, saying so on a terminal.
pub fn upload(client: &Client, files: &[Local]) -> Result<()> {
    let terminal = std::io::stderr().is_terminal();
    for file in files {
        if client.attachment_stored(&file.sha256)? {
            continue;
        }
        let reader =
            File::open(&file.path).with_context(|| format!("reading {}", file.path.display()))?;
        let progress = Progress {
            inner: reader,
            name: file.name.clone(),
            size: file.size,
            sent: 0,
            shown: 0,
            terminal,
        };
        crate::out::note(format_args!("uploading {} ({})", file.name, human(file.size)));
        client
            .upload_attachment(&file.sha256, file.size, progress)
            .with_context(|| format!("uploading {}", file.name))?;
        if terminal {
            eprint!("\r\x1b[2K");
        }
    }
    Ok(())
}

/// A reader that shows how far an upload has come, on a terminal only.
struct Progress {
    inner: File,
    name: String,
    size: u64,
    sent: u64,
    shown: u64,
    terminal: bool,
}

impl Read for Progress {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.sent += n as u64;
        if self.terminal && self.size > 0 {
            let percent = self.sent * 100 / self.size;
            if percent != self.shown {
                self.shown = percent;
                eprint!(
                    "\r\x1b[2K  {} {percent}% of {}",
                    self.name,
                    human(self.size)
                );
            }
        }
        Ok(n)
    }
}

pub fn human(bytes: u64) -> String {
    match bytes {
        b if b >= 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
        b if b >= 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{b} bytes"),
    }
}

/// What a file is, by its extension: the kinds plugins are likely to
/// take. Anything else is plain bytes.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flag_is_a_path_with_an_optional_name() {
        assert_eq!(
            parse_flag("out/pivot.glb").unwrap(),
            ("pivot.glb".into(), PathBuf::from("out/pivot.glb"))
        );
        assert_eq!(
            parse_flag("out/a.glb=Pivot lamp.glb").unwrap(),
            ("Pivot lamp.glb".into(), PathBuf::from("out/a.glb"))
        );
        assert!(parse_flag("out/").is_ok_and(|(name, _)| name == "out"));
    }

    #[test]
    fn flags_add_to_the_request_file_and_replace_by_name() {
        let request = json!({ "pivot.glb": "models/pivot.glb", "column.glb": "models/column.glb" });
        let flags = [
            ("pivot.glb".to_string(), PathBuf::from("/tmp/new-pivot.glb")),
            ("desk.jpg".to_string(), PathBuf::from("desk.jpg")),
        ];
        let files = collect(Some(&request), Path::new("/work"), &flags).unwrap();
        assert_eq!(files["pivot.glb"].path, PathBuf::from("/tmp/new-pivot.glb"));
        assert_eq!(
            files["column.glb"].path,
            PathBuf::from("/work/models/column.glb")
        );
        assert_eq!(files["desk.jpg"].path, PathBuf::from("desk.jpg"));
        let twice = [
            ("a.glb".to_string(), PathBuf::from("x.glb")),
            ("a.glb".to_string(), PathBuf::from("y.glb")),
        ];
        assert!(collect(None, Path::new("."), &twice).is_err());
    }

    #[test]
    fn a_fixture_entry_is_a_path_with_an_optional_kind() {
        let request = json!({
            "pivot.glb": { "path": "halden/pivot.glb" },
            "take.opus": { "path": "takes/take.opus", "media_type": "audio/ogg" },
        });
        let files = collect(Some(&request), Path::new("/work/fixtures"), &[]).unwrap();
        assert_eq!(
            files["pivot.glb"],
            Source {
                path: PathBuf::from("/work/fixtures/halden/pivot.glb"),
                media_type: None
            }
        );
        assert_eq!(files["take.opus"].media_type.as_deref(), Some("audio/ogg"));
        let pathless = json!({ "pivot.glb": { "media_type": "model/gltf-binary" } });
        assert!(collect(Some(&pathless), Path::new("."), &[]).is_err());
        assert!(collect(Some(&json!({ "pivot.glb": 3 })), Path::new("."), &[]).is_err());
    }

    #[test]
    fn a_given_kind_wins_over_the_extension() {
        let path = std::env::temp_dir().join(format!("pinrail-kind-{}.bin", std::process::id()));
        std::fs::write(&path, b"take").unwrap();
        let given = Source {
            path: path.clone(),
            media_type: Some("audio/ogg".into()),
        };
        let guessed = Source {
            path: path.clone(),
            media_type: None,
        };
        let files = BTreeMap::from([("a.bin".to_string(), given), ("b.bin".to_string(), guessed)]);
        let read = read(&files);
        std::fs::remove_file(&path).unwrap();
        let read = read.unwrap();
        assert_eq!(read[0].media_type, "audio/ogg");
        assert_eq!(read[1].media_type, "application/octet-stream");
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(media_type("Pivot.GLB"), "model/gltf-binary");
        assert_eq!(media_type("desk.jpeg"), "image/jpeg");
        assert_eq!(media_type("take.m4a"), "audio/mp4");
        assert_eq!(media_type("take.opus"), "audio/ogg");
        assert_eq!(media_type("README"), "application/octet-stream");
    }
}
