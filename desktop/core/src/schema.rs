//! JSON Schema 2020-12 validation for a plugin's payload and decision.
//!
//! A plugin's schemas are built with `$id` set to
//! `pinrail-plugin://<name>/<version>/<key>`, so a relative `$ref` such as
//! `payload.schema.json` resolves to a URI under that prefix, and the
//! retriever maps such URIs back to files in the plugin directory and nowhere
//! else. Violations keep a fixed wording and order, recorded in the API tests'
//! fixtures, because plugins render them.

use std::path::{Component, Path, PathBuf};

use jsonschema::error::{TypeKind, ValidationErrorKind};
use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;

use crate::error::Violation;

const SCHEME: &str = "pinrail-plugin://";

pub struct Schema {
    validator: Validator,
}

impl std::fmt::Debug for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Schema")
    }
}

/// The `$id` prefix for a plugin, ending in a slash.
pub fn prefix(name: &str, version: u32) -> String {
    format!("{SCHEME}{name}/{version}/")
}

impl Schema {
    /// Compiles `schema` for the plugin at `dir`. `key` names the manifest
    /// field, e.g. `payload_schema`, and becomes the schema's `$id` suffix.
    pub fn compile(
        dir: &Path,
        name: &str,
        version: u32,
        key: &str,
        schema: &Value,
    ) -> Result<Self, String> {
        let Value::Object(map) = schema else {
            return Err(format!("{key} must be a JSON Schema object"));
        };
        let prefix = prefix(name, version);
        let mut root = map.clone();
        root.entry("$schema").or_insert_with(|| {
            Value::String("https://json-schema.org/draft/2020-12/schema".into())
        });
        root.insert("$id".into(), Value::String(format!("{prefix}{key}")));

        let retriever = Retriever {
            prefix,
            dir: dir.to_path_buf(),
        };
        jsonschema::options()
            .with_draft(Draft::Draft202012)
            .with_retriever(retriever)
            .build(&Value::Object(root))
            .map(|validator| Schema { validator })
            .map_err(|error| format!("{key}: {error}"))
    }

    /// Compiles a schema that stands alone: no `$ref` to anything outside
    /// it, such as the manifest schema the core carries.
    pub fn standalone(schema: &Value) -> Result<Self, String> {
        jsonschema::options()
            .with_draft(Draft::Draft202012)
            .build(schema)
            .map(|validator| Schema { validator })
            .map_err(|error| error.to_string())
    }

    /// Every violation, sorted by path, with leaf messages only.
    pub fn validate(&self, instance: &Value) -> Vec<Violation> {
        let mut violations: Vec<Violation> = Vec::new();
        for error in self.validator.iter_errors(instance) {
            let path = error.instance_path().to_string();
            match error.kind() {
                ValidationErrorKind::AdditionalProperties { unexpected } => {
                    for property in unexpected {
                        violations.push(Violation::new(
                            path.clone(),
                            format!("additional properties are not allowed but found property '{property}'"),
                        ));
                        violations.push(Violation::new(
                            format!("{path}/{property}"),
                            "unexpected property",
                        ));
                    }
                }
                ValidationErrorKind::Required { property } => {
                    let name = property
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| property.to_string());
                    violations.push(Violation::new(
                        path,
                        format!("property '{name}' is required"),
                    ));
                }
                ValidationErrorKind::Type { kind } => {
                    let expected = match kind {
                        TypeKind::Single(t) => t.to_string(),
                        TypeKind::Multiple(types) => types
                            .iter()
                            .map(|t| t.to_string())
                            .collect::<Vec<_>>()
                            .join(" or "),
                    };
                    violations.push(Violation::new(
                        path,
                        format!("value is not of type {expected}"),
                    ));
                }
                ValidationErrorKind::Enum { options } => {
                    let listed = match options {
                        Value::Array(items) => join_or(items.iter().map(|v| v.to_string())),
                        other => other.to_string(),
                    };
                    violations.push(Violation::new(
                        path,
                        format!("value must be one of the enum values: {listed}"),
                    ));
                }
                _ => violations.push(Violation::new(path, error.to_string())),
            }
        }
        violations.sort();
        violations.dedup();
        violations
    }
}

fn join_or(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.collect();
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} or {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

struct Retriever {
    prefix: String,
    dir: PathBuf,
}

impl Retrieve for Retriever {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let text = uri.as_str();
        let Some(relative) = text.strip_prefix(&self.prefix) else {
            return Err(format!(
                "cannot resolve {text}: only files in the plugin directory can be referenced"
            )
            .into());
        };
        let relative = relative.split('#').next().unwrap_or("");
        let path = safe_join(&self.dir, relative).ok_or_else(|| {
            format!("cannot resolve {text}: the path leaves the plugin directory")
        })?;
        let body = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot load {}: {e}", path.display()))?;
        serde_json::from_str(&body)
            .map_err(|e| format!("{} is not valid JSON: {e}", path.display()).into())
    }
}

/// `dir/relative` when `relative` stays inside `dir`.
pub fn safe_join(dir: &Path, relative: &str) -> Option<PathBuf> {
    let mut out = dir.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/list")
    }

    fn payload_schema() -> Schema {
        let schema = json!({ "$ref": "payload.schema.json" });
        Schema::compile(&list_dir(), "list", 1, "payload_schema", &schema).unwrap()
    }

    #[test]
    fn valid_payload_has_no_violations() {
        let s = payload_schema();
        assert!(s.validate(&json!({ "groups": [] })).is_empty());
    }

    #[test]
    fn violations_keep_their_wording_and_order() {
        let s = payload_schema();
        assert_eq!(
            s.validate(&json!({ "intro": 1 })),
            vec![
                Violation::new("", "property 'groups' is required"),
                Violation::new("/intro", "value is not of type string"),
            ]
        );
        assert_eq!(
            s.validate(&json!({ "groups": [], "extra": 1, "intro": "x" })),
            vec![
                Violation::new(
                    "",
                    "additional properties are not allowed but found property 'extra'"
                ),
                Violation::new("/extra", "unexpected property"),
            ]
        );
        assert_eq!(
            s.validate(&json!({ "groups": [{ "title": "g", "items": [{ "id": "x", "title": "t", "bogus": true }] }] })),
            vec![
                Violation::new("/groups/0/items/0", "additional properties are not allowed but found property 'bogus'"),
                Violation::new("/groups/0/items/0/bogus", "unexpected property"),
                Violation::new("/groups/0/items/0/id", "value is not of type integer"),
            ]
        );
    }

    #[test]
    fn enum_violations_list_the_options() {
        let schema = json!({ "$ref": "decision.schema.json" });
        let s = Schema::compile(&list_dir(), "list", 1, "decision_schema", &schema).unwrap();
        assert_eq!(
            s.validate(&json!({ "decisions": [{ "id": 1, "action": "maybe" }] })),
            vec![
                Violation::new("", "property 'undecided' is required"),
                Violation::new(
                    "/decisions/0/action",
                    "value must be one of the enum values: \"accept\" or \"reject\""
                ),
            ]
        );
    }

    #[test]
    fn refs_cannot_leave_the_plugin_directory() {
        let schema = json!({ "$ref": "../../../Cargo.toml" });
        let error = Schema::compile(&list_dir(), "list", 1, "payload_schema", &schema).unwrap_err();
        assert!(error.starts_with("payload_schema:"), "{error}");
        assert!(safe_join(Path::new("/p"), "../x").is_none());
        assert_eq!(
            safe_join(Path::new("/p"), "./a/b.json"),
            Some(PathBuf::from("/p/a/b.json"))
        );
    }
}
