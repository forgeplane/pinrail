//! A plugin's sample: a request file the manifest names, with a title, a
//! payload and the files it refers to, which anyone can send to see the
//! plugin without writing a payload. Checked when the plugin loads; its
//! files are read when it is sent.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::schema::{Schema, safe_join};

#[derive(Debug, Clone)]
pub struct Sample {
    pub title: String,
    pub payload: Value,
    pub files: Vec<SampleFile>,
}

#[derive(Debug, Clone)]
pub struct SampleFile {
    /// the name the payload refers to
    pub name: String,
    pub path: PathBuf,
    pub media_type: String,
}

/// Reads the request file `file` names inside `dir`, and checks it: a
/// title, a payload that passes the schema, and every file there. The
/// message says what is wrong, naming the file.
pub fn load(dir: &Path, file: &str, schema: &Schema) -> Result<Sample, String> {
    let path = safe_join(dir, file)
        .ok_or_else(|| format!("{file}: must stay inside the plugin's folder"))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{file}: cannot read ({e})"))?;
    let request: Value =
        serde_json::from_str(&text).map_err(|e| format!("{file}: not JSON ({e})"))?;
    let title = request
        .get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| format!("{file}: needs a title"))?
        .to_string();
    let payload = request.get("payload").cloned().unwrap_or(Value::Null);
    if !payload.is_object() {
        return Err(format!("{file}: needs a payload, a JSON object"));
    }
    if let Some(v) = schema.validate(&payload).first() {
        let at = if v.path.is_empty() { "/" } else { &v.path };
        return Err(format!(
            "{file}: the payload does not pass payload_schema at {at}: {}",
            v.message
        ));
    }
    let base = path.parent().unwrap_or(dir);
    let mut files = Vec::new();
    match request.get("attachments") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (name, entry) in map {
                let (relative, media_type) = match entry {
                    Value::String(p) => (p.as_str(), None),
                    Value::Object(o) => (
                        o.get("path")
                            .and_then(Value::as_str)
                            .ok_or_else(|| format!("{file}: attachments.{name} needs a path"))?,
                        o.get("media_type").and_then(Value::as_str),
                    ),
                    _ => {
                        return Err(format!(
                            "{file}: attachments.{name} must be a path or {{path, media_type}}"
                        ));
                    }
                };
                let at = safe_join(base, relative).ok_or_else(|| {
                    format!("{file}: attachments.{name} must stay inside the plugin's folder")
                })?;
                if !at.is_file() {
                    return Err(format!("{file}: attachments.{name}: {relative} not found"));
                }
                files.push(SampleFile {
                    name: name.clone(),
                    media_type: media_type
                        .map(str::to_string)
                        .unwrap_or_else(|| crate::attachments::media_type(name).to_string()),
                    path: at,
                });
            }
        }
        Some(_) => return Err(format!("{file}: attachments must map each name to a file")),
    }
    Ok(Sample {
        title,
        payload,
        files,
    })
}

impl Sample {
    /// The submission it makes for `plugin`, with the files declared as
    /// stored under their hashes: what `POST /api/v1/reviews` takes.
    pub fn request(&self, plugin: &str, stored: &[(String, u64)]) -> Value {
        let mut body = json!({
            "plugin": plugin,
            "title": self.title,
            "payload": self.payload,
            "requested_by": "sample",
        });
        if !self.files.is_empty() {
            let declared: Map<String, Value> = self
                .files
                .iter()
                .zip(stored)
                .map(|(f, (sha256, size))| {
                    (
                        f.name.clone(),
                        json!({ "sha256": sha256, "size": size, "media_type": f.media_type }),
                    )
                })
                .collect();
            body["attachments"] = Value::Object(declared);
        }
        body
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::super::Plugin;

    fn plugins_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins")
    }

    #[test]
    fn a_sample_names_the_files_it_sends() {
        // every plugin's sample loads: tests/plugins.rs goes through them all
        let model = Plugin::load(&plugins_dir().join("model"));
        let files: Vec<&str> = model
            .sample
            .as_ref()
            .unwrap()
            .files
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(files.len(), 4);
        assert!(
            model
                .sample
                .unwrap()
                .files
                .iter()
                .all(|f| f.media_type == "model/gltf-binary" && f.path.is_file())
        );
    }

    fn plugin_with(
        sample: serde_json::Value,
        files: &[(&str, &str)],
    ) -> (tempfile::TempDir, Plugin) {
        let dir = tempfile::tempdir().unwrap();
        let list = plugins_dir().join("list");
        for f in [
            "manifest.json",
            "schemas/payload.schema.json",
            "schemas/decision.schema.json",
            "example.json",
            "view/index.html",
        ] {
            let to = dir.path().join(f);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(list.join(f), to).unwrap();
        }
        std::fs::write(dir.path().join("sample.json"), sample.to_string()).unwrap();
        for (name, body) in files {
            std::fs::write(dir.path().join(name), body).unwrap();
        }
        let plugin = Plugin::load(dir.path());
        (dir, plugin)
    }

    #[test]
    fn a_sample_that_does_not_load_costs_the_sample_not_the_plugin() {
        let payload = json!({ "groups": [{ "title": "g", "items": [{ "id": 1, "title": "i" }] }] });
        let (_d, untitled) = plugin_with(json!({ "payload": payload }), &[]);
        assert!(untitled.usable());
        assert_eq!(
            untitled.sample_error.as_deref(),
            Some("sample.json: needs a title")
        );

        let (_d, invalid) =
            plugin_with(json!({ "title": "t", "payload": { "groups": "no" } }), &[]);
        assert!(
            invalid
                .sample_error
                .unwrap()
                .contains("does not pass payload_schema")
        );

        let (_d, missing) = plugin_with(
            json!({ "title": "t", "payload": payload, "attachments": { "a.png": "a.png" } }),
            &[],
        );
        assert_eq!(
            missing.sample_error.as_deref(),
            Some("sample.json: attachments.a.png: a.png not found")
        );

        let (_d, outside) = plugin_with(
            json!({ "title": "t", "payload": payload, "attachments": { "a.png": "../a.png" } }),
            &[],
        );
        assert!(outside.sample_error.unwrap().contains("must stay inside"));

        let (_d, good) = plugin_with(
            json!({ "title": "t", "payload": payload, "attachments": { "a.png": { "path": "a.png" } } }),
            &[("a.png", "png")],
        );
        assert_eq!(good.sample_error, None);
        assert_eq!(good.sample.unwrap().files[0].media_type, "image/png");
    }
}
