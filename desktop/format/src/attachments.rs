//! What a plugin takes beside a payload, from its manifest's `attachments`,
//! the media type of a file by its name, and how the payload schema's file
//! fields agree with both.

use serde_json::Value;

/// The most files one review may carry.
pub const MAX_COUNT: usize = 32;
/// The most one attached file may be, 100 MB: a model, a recording, a
/// document with its images.
pub const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;

/// The SDK's schema of a reference to an attached file,
/// `{ "$attachment": "<name>" }`, which a payload schema copies for each of
/// its file fields.
pub const ATTACHMENT_SCHEMA: &str =
    include_str!(concat!(env!("OUT_DIR"), "/attachment.schema.json"));

/// Where in a payload schema a file field is defined, as JSON Pointers:
/// every object with an `$attachment` property.
fn file_fields(schema: &Value, at: String, out: &mut Vec<(String, Value)>) {
    match schema {
        Value::Object(map) => {
            if map
                .get("properties")
                .and_then(|p| p.get("$attachment"))
                .is_some()
            {
                out.push((at.clone(), schema.clone()));
            }
            for (key, value) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                file_fields(value, format!("{at}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                file_fields(value, format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// What decides which names a definition accepts: all but its title and
/// description.
fn rules(definition: &Value) -> Value {
    let mut rules = definition.clone();
    if let Some(map) = rules.as_object_mut() {
        for key in ["$schema", "$id", "title", "description"] {
            map.remove(key);
        }
    }
    rules
}

/// How the payload schema's file fields agree with the manifest, which
/// `declared` says takes files, and with the SDK's attachment schema. Each
/// disagreement is a sentence for `pinrail plugins check` and Settings; the
/// plugin works either way.
pub fn file_field_warnings(declared: bool, payload_schema: &Value) -> Vec<String> {
    let mut fields = Vec::new();
    file_fields(payload_schema, String::new(), &mut fields);
    let mut warnings = Vec::new();
    if declared && fields.is_empty() {
        warnings.push(
            "the manifest declares attachments, but no field of the payload schema names a file: \
             describe one with a copy of the SDK's attachment.schema.json"
                .to_string(),
        );
    }
    if !declared && !fields.is_empty() {
        let at: Vec<&str> = fields.iter().map(|(at, _)| at.as_str()).collect();
        warnings.push(format!(
            "the payload schema names a file at {}, but the manifest declares no attachments, \
             so the app refuses every file",
            at.join(", ")
        ));
    }
    let sdk =
        rules(&serde_json::from_str(ATTACHMENT_SCHEMA).expect("attachment.schema.json is JSON"));
    for (at, field) in &fields {
        if rules(field) != sdk {
            warnings.push(format!(
                "{at} takes other file names than the app: make it a copy of the SDK's attachment.schema.json"
            ));
        }
    }
    warnings
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
                Some(n) if n > 0 && n <= MAX_ATTACHMENT_BYTES => Some(n),
                _ => {
                    return Err(format!(
                        "attachments.max_size: must be a number of bytes from 1 to {}",
                        MAX_ATTACHMENT_BYTES
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

#[cfg(test)]
mod tests {
    use super::*;

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
            serde_json::json!({ "accept": [".glb"], "max_size": MAX_ATTACHMENT_BYTES + 1 }),
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
}
