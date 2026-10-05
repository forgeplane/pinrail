//! A plugin's samples: request files in `samples/`, each with a title, a
//! payload and the files it refers to, which anyone can send to see the
//! plugin without writing a payload. Checked when the plugin loads; their
//! files, beside them in `samples/`, are read when one is sent.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::schema::{Schema, safe_join};

/// Where a plugin's samples are.
pub const SAMPLES: &str = "samples";

#[derive(Debug, Clone)]
pub struct Sample {
    /// The file's name without `.json`: what `--sample <name>` takes.
    pub name: String,
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

/// Every sample in `dir`'s `samples/` folder, in order of name, and why
/// each one that does not load was dropped.
pub fn load_all(dir: &Path, schema: &Schema) -> (Vec<Sample>, Vec<String>) {
    let mut names: Vec<String> = std::fs::read_dir(dir.join(SAMPLES))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    let (mut samples, mut errors) = (Vec::new(), Vec::new());
    for name in names {
        match load(dir, &format!("{SAMPLES}/{name}"), schema) {
            Ok(sample) => samples.push(sample),
            Err(message) => errors.push(message),
        }
    }
    (samples, errors)
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
    let name = Path::new(file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file)
        .to_string();
    Ok(Sample {
        name,
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

    use crate::Plugin;

    fn plugins_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins")
    }

    #[test]
    fn a_sample_names_the_files_it_sends() {
        // every plugin's sample loads: tests/check.rs goes through them all
        let sampler = Plugin::load(&plugins_dir().join("sampler"));
        let sample = sampler.sample(None).unwrap();
        assert_eq!(sample.name, "sampler");
        assert_eq!(sample.files.len(), 2);
        assert!(
            sample
                .files
                .iter()
                .all(|f| f.media_type == "text/plain" && f.path.is_file())
        );
    }

    /// The list plugin with these files in its `samples/` folder.
    fn plugin_with(samples: &[(&str, String)]) -> (tempfile::TempDir, Plugin) {
        let dir = tempfile::tempdir().unwrap();
        let list = plugins_dir().join("list");
        for f in [
            "manifest.json",
            "schemas/payload.schema.json",
            "schemas/decision.schema.json",
            "view/index.html",
        ] {
            let to = dir.path().join(f);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(list.join(f), to).unwrap();
        }
        std::fs::create_dir_all(dir.path().join("samples")).unwrap();
        for (name, body) in samples {
            std::fs::write(dir.path().join("samples").join(name), body).unwrap();
        }
        let plugin = Plugin::load(dir.path());
        (dir, plugin)
    }

    fn payload() -> serde_json::Value {
        json!({ "groups": [{ "title": "g", "items": [{ "id": 1, "title": "i" }] }] })
    }

    #[test]
    fn a_sample_that_does_not_load_costs_that_sample_not_the_plugin() {
        let one = |sample: serde_json::Value, files: &[(&str, &str)]| {
            let mut all = vec![("only.json", sample.to_string())];
            all.extend(files.iter().map(|(n, b)| (*n, b.to_string())));
            plugin_with(&all)
        };
        let (_d, untitled) = one(json!({ "payload": payload() }), &[]);
        assert!(untitled.usable() && untitled.samples.is_empty());
        assert_eq!(untitled.sample_errors, ["samples/only.json: needs a title"]);

        let (_d, invalid) = one(json!({ "title": "t", "payload": { "groups": "no" } }), &[]);
        assert!(invalid.sample_errors[0].contains("does not pass payload_schema"));

        let (_d, missing) = one(
            json!({ "title": "t", "payload": payload(), "attachments": { "a.png": "a.png" } }),
            &[],
        );
        assert_eq!(
            missing.sample_errors,
            ["samples/only.json: attachments.a.png: a.png not found"]
        );

        let (_d, outside) = one(
            json!({ "title": "t", "payload": payload(), "attachments": { "a.png": "../../a.png" } }),
            &[],
        );
        assert!(outside.sample_errors[0].contains("must stay inside"));

        // a sample's files are beside it in samples/
        let (_d, good) = one(
            json!({ "title": "t", "payload": payload(), "attachments": { "a.png": { "path": "a.png" } } }),
            &[("a.png", "png")],
        );
        assert!(good.sample_errors.is_empty(), "{:?}", good.sample_errors);
        assert_eq!(good.sample(None).unwrap().files[0].media_type, "image/png");
    }

    #[test]
    fn several_samples_load_in_order_of_name_and_the_first_is_the_example() {
        let sample = |title: &str, n: i64| {
            let mut p = payload();
            p["groups"][0]["items"][0]["id"] = json!(n);
            json!({ "title": title, "payload": p }).to_string()
        };
        let (_d, plugin) = plugin_with(&[
            ("b-second.json", sample("Second", 2)),
            ("a-first.json", sample("First", 1)),
            ("c-broken.json", "{".into()),
            ("notes.txt", "not a sample".into()),
        ]);
        assert_eq!(plugin.sample_names(), ["a-first", "b-second"]);
        assert_eq!(plugin.sample(None).unwrap().title, "First");
        assert_eq!(plugin.sample(Some("b-second")).unwrap().title, "Second");
        assert!(plugin.sample(Some("nope")).is_none());
        assert_eq!(plugin.example().unwrap()["groups"][0]["items"][0]["id"], 1);
        assert_eq!(plugin.sample_errors.len(), 1);
        assert!(plugin.sample_errors[0].starts_with("samples/c-broken.json: not JSON"));

        // without samples there is no example
        let (_d, none) = plugin_with(&[]);
        assert!(none.example().is_none() && none.samples.is_empty());
    }
}
