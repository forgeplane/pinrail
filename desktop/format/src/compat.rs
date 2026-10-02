//! Whether a new release's schema keeps every document the old one took:
//! the rule an update within a line is held to, so the reviews stored on
//! the line still validate.
//!
//! The rule is fixed and compares the two schemas, never data. Allowed:
//! a new optional property, a new `enum` value, and a change to what only
//! describes (`title`, `description`, `examples`, `default`, `$comment`,
//! `deprecated`, `readOnly`, `writeOnly`). Everything else that changes
//! breaks: a removed property, a newly required one, any `type` change, a
//! removed `enum` value, and any change to another keyword.

use std::path::Path;

use serde_json::{Map, Value};

use crate::Violation;
use crate::manifest::{DECISION_SCHEMA, PAYLOAD_SCHEMA};
use crate::schema::safe_join;

/// Keywords that describe a schema and never decide what it accepts.
const ANNOTATIONS: &[&str] = &[
    "title",
    "description",
    "examples",
    "default",
    "$comment",
    "deprecated",
    "readOnly",
    "writeOnly",
    "$schema",
    "$id",
];

/// Keywords that hold schemas compared where they are used: `$defs` is
/// reached through `$ref`, and these by their own rules.
const COMPARED: &[&str] = &[
    "type",
    "properties",
    "required",
    "enum",
    "items",
    "prefixItems",
    "additionalProperties",
    "allOf",
    "anyOf",
    "oneOf",
    "$ref",
    "$defs",
    "definitions",
];

/// A schema document and how to read the files its `$ref`s name.
pub struct Document<'a> {
    pub root: &'a Value,
    /// The document a `$ref` to another file names, by that reference.
    pub load: &'a dyn Fn(&str) -> Option<Value>,
}

/// What in the release at `new` breaks the payloads and decisions the
/// release at `old` accepted: each of the two schemas compared, a break's
/// path naming the file and the place in it, such as
/// `schemas/payload.schema.json#/properties/tickets`.
pub fn plugin_breaks(old: &Path, new: &Path) -> Vec<Violation> {
    let mut out = Vec::new();
    for file in [PAYLOAD_SCHEMA, DECISION_SCHEMA] {
        let read = |dir: &Path, name: &str| -> Option<Value> {
            let path = safe_join(&dir.join("schemas"), name)?;
            serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
        };
        let name = file.trim_start_matches("schemas/");
        let (Some(before), Some(after)) = (read(old, name), read(new, name)) else {
            out.push(Violation::new(file, "the schema cannot be read"));
            continue;
        };
        let old_files = |r: &str| read(old, r);
        let new_files = |r: &str| read(new, r);
        for found in breaks(
            &Document {
                root: &before,
                load: &old_files,
            },
            &Document {
                root: &after,
                load: &new_files,
            },
        ) {
            out.push(Violation::new(
                format!("{file}#{}", found.path),
                found.message,
            ));
        }
    }
    out
}

/// What in `new` breaks a document `old` accepted, each at its path in the
/// schema. Empty when the new schema keeps everything the old one took.
pub fn breaks(old: &Document, new: &Document) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    compare(old, old.root, new, new.root, "", &mut seen, &mut out);
    out
}

fn compare(
    old_doc: &Document,
    old: &Value,
    new_doc: &Document,
    new: &Value,
    at: &str,
    seen: &mut Vec<(String, String)>,
    out: &mut Vec<Violation>,
) {
    // a reference on either side is compared as what it names, once per
    // pair, so a recursive schema ends
    let (old, old_ref) = resolve(old_doc, old);
    let (new, new_ref) = resolve(new_doc, new);
    if old_ref.is_some() || new_ref.is_some() {
        let pair = (
            old_ref.clone().unwrap_or_default(),
            new_ref.clone().unwrap_or_default(),
        );
        if seen.contains(&pair) {
            return;
        }
        seen.push(pair);
    }
    let (Some(old), Some(new)) = (old, new) else {
        out.push(Violation::new(at, "a $ref names nothing that can be read"));
        return;
    };
    let (old, new) = match (old, new) {
        (Value::Object(o), Value::Object(n)) => (o, n),
        (o, n) if o == n => return,
        _ => {
            out.push(Violation::new(at, "the schema changes"));
            return;
        }
    };

    let (old, new) = (&old, &new);
    if types(old) != types(new) {
        out.push(Violation::new(
            pointer(at, "type"),
            format!(
                "the type changes from {} to {}",
                shown(old.get("type")),
                shown(new.get("type"))
            ),
        ));
    }

    let old_props = old.get("properties").and_then(Value::as_object);
    let new_props = new.get("properties").and_then(Value::as_object);
    let empty = Map::new();
    for (name, schema) in old_props.unwrap_or(&empty) {
        let path = pointer(&pointer(at, "properties"), name);
        match new_props.and_then(|p| p.get(name)) {
            None => out.push(Violation::new(path, format!("removes the property {name}"))),
            Some(next) => compare(old_doc, schema, new_doc, next, &path, seen, out),
        }
    }

    let old_required = strings(old.get("required"));
    let new_required = strings(new.get("required"));
    for name in &new_required {
        if !old_required.contains(name) {
            out.push(Violation::new(
                pointer(at, "required"),
                format!("makes {name} required"),
            ));
        }
    }
    for name in &old_required {
        if !new_required.contains(name) {
            out.push(Violation::new(
                pointer(at, "required"),
                format!("no longer requires {name}"),
            ));
        }
    }

    match (old.get("enum"), new.get("enum")) {
        (None, None) => {}
        (Some(Value::Array(before)), Some(Value::Array(after))) => {
            for value in before {
                if !after.contains(value) {
                    out.push(Violation::new(
                        pointer(at, "enum"),
                        format!("removes the value {value}"),
                    ));
                }
            }
        }
        _ => out.push(Violation::new(pointer(at, "enum"), "the enum changes")),
    }

    for key in ["items", "additionalProperties"] {
        match (old.get(key), new.get(key)) {
            (None, None) => {}
            (Some(o), Some(n)) => compare(old_doc, o, new_doc, n, &pointer(at, key), seen, out),
            _ => out.push(Violation::new(pointer(at, key), format!("{key} changes"))),
        }
    }

    for key in ["prefixItems", "allOf", "anyOf", "oneOf"] {
        match (old.get(key), new.get(key)) {
            (None, None) => {}
            (Some(Value::Array(o)), Some(Value::Array(n))) if o.len() == n.len() => {
                for (i, (o, n)) in o.iter().zip(n).enumerate() {
                    let path = pointer(&pointer(at, key), &i.to_string());
                    compare(old_doc, o, new_doc, n, &path, seen, out);
                }
            }
            _ => out.push(Violation::new(pointer(at, key), format!("{key} changes"))),
        }
    }

    // every other keyword decides what is accepted: it stays as it was
    let mut other: Vec<&String> = old
        .keys()
        .chain(new.keys())
        .filter(|k| !ANNOTATIONS.contains(&k.as_str()) && !COMPARED.contains(&k.as_str()))
        .collect();
    other.sort();
    other.dedup();
    for key in other {
        if old.get(key) != new.get(key) {
            out.push(Violation::new(pointer(at, key), format!("{key} changes")));
        }
    }
}

/// The schema a node stands for: itself, or what its `$ref` names, with
/// the reference for telling a cycle.
fn resolve(doc: &Document, node: &Value) -> (Option<Value>, Option<String>) {
    let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
        return (Some(node.clone()), None);
    };
    let (file, fragment) = reference.split_once('#').unwrap_or((reference, ""));
    let target = if file.is_empty() {
        Some(doc.root.clone())
    } else {
        (doc.load)(file)
    };
    let found = target.and_then(|t| {
        if fragment.is_empty() {
            Some(t)
        } else {
            t.pointer(fragment).cloned()
        }
    });
    (found, Some(reference.to_string()))
}

/// The types a schema allows, sorted; none when it does not say.
fn types(schema: &Map<String, Value>) -> Vec<String> {
    let mut out = strings(schema.get("type"));
    if let Some(Value::String(one)) = schema.get("type") {
        out = vec![one.clone()];
    }
    out.sort();
    out
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn shown(value: Option<&Value>) -> String {
    value.map_or_else(|| "any".to_string(), Value::to_string)
}

/// `at` with one more part, escaped as a JSON pointer escapes it.
fn pointer(at: &str, part: &str) -> String {
    format!("{at}/{}", part.replace('~', "~0").replace('/', "~1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(old: Value, new: Value) -> Vec<String> {
        let none = |_: &str| None;
        breaks(
            &Document {
                root: &old,
                load: &none,
            },
            &Document {
                root: &new,
                load: &none,
            },
        )
        .into_iter()
        .map(|v| format!("{}: {}", v.path, v.message))
        .collect()
    }

    fn ticket() -> Value {
        json!({
            "type": "object",
            "required": ["tickets"],
            "properties": {
                "tickets": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["id"],
                        "properties": {
                            "id": {"type": "integer"},
                            "state": {"enum": ["open", "closed"]},
                            "note": {"type": "string", "maxLength": 200}
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn the_same_schema_breaks_nothing() {
        assert!(check(ticket(), ticket()).is_empty());
    }

    #[test]
    fn an_optional_property_an_enum_value_and_a_description_are_allowed() {
        let mut new = ticket();
        new["properties"]["tickets"]["items"]["properties"]["owner"] = json!({"type": "string"});
        new["properties"]["tickets"]["items"]["properties"]["state"]["enum"] =
            json!(["open", "closed", "merged"]);
        new["properties"]["tickets"]["description"] = json!("What the agent triaged.");
        new["title"] = json!("Tickets");
        assert_eq!(check(ticket(), new), Vec::<String>::new());
    }

    #[test]
    fn removing_a_property_breaks() {
        let mut new = ticket();
        new["properties"]["tickets"]["items"]["properties"]
            .as_object_mut()
            .unwrap()
            .remove("note");
        assert_eq!(
            check(ticket(), new),
            vec!["/properties/tickets/items/properties/note: removes the property note"]
        );
    }

    #[test]
    fn a_new_required_property_or_a_newly_required_one_breaks() {
        let mut new = ticket();
        new["properties"]["tickets"]["items"]["properties"]["owner"] = json!({"type": "string"});
        new["properties"]["tickets"]["items"]["required"] = json!(["id", "owner", "state"]);
        assert_eq!(
            check(ticket(), new),
            vec![
                "/properties/tickets/items/required: makes owner required",
                "/properties/tickets/items/required: makes state required",
            ]
        );
    }

    #[test]
    fn any_type_change_breaks_even_a_wider_one() {
        let mut new = ticket();
        new["properties"]["tickets"]["items"]["properties"]["id"]["type"] =
            json!(["integer", "string"]);
        assert_eq!(
            check(ticket(), new),
            vec![
                "/properties/tickets/items/properties/id/type: the type changes from \"integer\" to [\"integer\",\"string\"]"
            ]
        );
        // the same type written as a list of one is the same type
        let mut same = ticket();
        same["type"] = json!(["object"]);
        assert!(check(ticket(), same).is_empty());
    }

    #[test]
    fn removing_an_enum_value_breaks() {
        let mut new = ticket();
        new["properties"]["tickets"]["items"]["properties"]["state"]["enum"] = json!(["open"]);
        assert_eq!(
            check(ticket(), new),
            vec!["/properties/tickets/items/properties/state/enum: removes the value \"closed\""]
        );
    }

    #[test]
    fn any_other_constraint_that_changes_breaks() {
        for (key, value) in [
            ("maxLength", json!(100)),
            ("maxLength", json!(400)),
            ("pattern", json!("^[a-z]+$")),
        ] {
            let mut new = ticket();
            new["properties"]["tickets"]["items"]["properties"]["note"][key] = value;
            assert_eq!(
                check(ticket(), new),
                vec![format!(
                    "/properties/tickets/items/properties/note/{key}: {key} changes"
                )]
            );
        }
        let mut new = ticket();
        new["additionalProperties"] = json!(false);
        assert_eq!(
            check(ticket(), new),
            vec!["/additionalProperties: additionalProperties changes"]
        );
        // no longer requiring a property is a change too
        let mut new = ticket();
        new["required"] = json!([]);
        assert_eq!(
            check(ticket(), new),
            vec!["/required: no longer requires tickets"]
        );
    }

    #[test]
    fn references_are_compared_as_what_they_name() {
        let inline = ticket();
        // the same item moved into $defs, and one in another file
        let mut moved = ticket();
        let item = moved["properties"]["tickets"]["items"].clone();
        moved["$defs"] = json!({ "ticket": item });
        moved["properties"]["tickets"]["items"] = json!({"$ref": "#/$defs/ticket"});
        assert!(check(inline.clone(), moved.clone()).is_empty());

        let mut narrowed = moved.clone();
        narrowed["$defs"]["ticket"]["properties"]["state"]["enum"] = json!(["open"]);
        assert_eq!(
            check(moved.clone(), narrowed),
            vec!["/properties/tickets/items/properties/state/enum: removes the value \"closed\""]
        );

        let item = inline["properties"]["tickets"]["items"].clone();
        let load = move |file: &str| (file == "ticket.schema.json").then(|| item.clone());
        let mut elsewhere = ticket();
        elsewhere["properties"]["tickets"]["items"] = json!({"$ref": "ticket.schema.json"});
        let none = |_: &str| None;
        assert!(
            breaks(
                &Document {
                    root: &inline,
                    load: &none
                },
                &Document {
                    root: &elsewhere,
                    load: &load
                },
            )
            .is_empty()
        );
        // a reference to nothing breaks, since nothing can be said of it
        let mut missing = ticket();
        missing["properties"]["tickets"]["items"] = json!({"$ref": "#/$defs/nope"});
        assert_eq!(
            check(inline, missing),
            vec!["/properties/tickets/items: a $ref names nothing that can be read"]
        );
    }

    #[test]
    fn a_recursive_schema_is_compared_once_per_reference() {
        let tree = json!({
            "$defs": {"node": {"type": "object", "properties": {
                "name": {"type": "string"},
                "children": {"type": "array", "items": {"$ref": "#/$defs/node"}}
            }}},
            "$ref": "#/$defs/node"
        });
        assert!(check(tree.clone(), tree.clone()).is_empty());
        let mut changed = tree.clone();
        changed["$defs"]["node"]["properties"]["name"]["type"] = json!("integer");
        assert_eq!(
            check(tree, changed),
            vec!["/properties/name/type: the type changes from \"string\" to \"integer\""]
        );
    }

    /// Two releases' folders: both schemas compared, and a schema the
    /// payload's refers to by file read from beside it.
    #[test]
    fn two_releases_are_compared_by_both_their_schemas() {
        let write = |dir: &Path, payload: &Value, decision: &Value| {
            std::fs::create_dir_all(dir.join("schemas")).unwrap();
            std::fs::write(dir.join(PAYLOAD_SCHEMA), payload.to_string()).unwrap();
            std::fs::write(dir.join(DECISION_SCHEMA), decision.to_string()).unwrap();
        };
        let root = tempfile::tempdir().unwrap();
        let (old, new) = (root.path().join("old"), root.path().join("new"));
        let decision = json!({"type": "object", "properties": {"ok": {"type": "boolean"}}});
        write(&old, &ticket(), &decision);
        let mut payload = ticket();
        payload["properties"]["tickets"]["items"] = json!({"$ref": "ticket.schema.json"});
        let item = ticket()["properties"]["tickets"]["items"].clone();
        write(&new, &payload, &decision);
        std::fs::write(new.join("schemas/ticket.schema.json"), item.to_string()).unwrap();
        assert_eq!(plugin_breaks(&old, &new), Vec::<Violation>::new());

        let narrower = json!({"type": "object", "required": ["ok"], "properties": {"ok": {"type": "boolean"}}});
        write(&new, &payload, &narrower);
        assert_eq!(
            plugin_breaks(&old, &new),
            vec![Violation::new(
                "schemas/decision.schema.json#/required",
                "makes ok required"
            )]
        );
    }

    #[test]
    fn combinators_are_compared_branch_by_branch() {
        let old = json!({"oneOf": [{"type": "string"}, {"type": "integer"}]});
        assert!(check(old.clone(), old.clone()).is_empty());
        let fewer = json!({"oneOf": [{"type": "string"}]});
        assert_eq!(check(old.clone(), fewer), vec!["/oneOf: oneOf changes"]);
        let other = json!({"oneOf": [{"type": "string"}, {"type": "number"}]});
        assert_eq!(
            check(old, other),
            vec!["/oneOf/1/type: the type changes from \"integer\" to \"number\""]
        );
    }
}
