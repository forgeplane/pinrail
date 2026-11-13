//! The protocol's schema: its methods with their parameters, its events
//! with their fields, and its error codes. A test compares it with the
//! schema of the released generation, in `protocol/generation-1.json`, and
//! fails on a breaking change; anything else is added as a feature.

use serde_json::{Map, Value, json};

/// A method: the parameters it requires, and those it also takes.
pub struct Method {
    pub name: &'static str,
    pub required: &'static [&'static str],
    pub optional: &'static [&'static str],
}

pub const METHODS: &[Method] = &[
    Method {
        name: "hello",
        required: &["secret"],
        optional: &["client", "generation", "features"],
    },
    Method {
        name: "host.info",
        required: &[],
        optional: &[],
    },
    Method {
        name: "host.stop",
        required: &[],
        optional: &["stop_terminals", "formats"],
    },
    Method {
        name: "env.probe",
        required: &[],
        optional: &["refresh"],
    },
    Method {
        name: "terminal.spawn",
        required: &["argv", "cwd", "cols", "rows"],
        optional: &["env", "session_env", "labels", "env_base"],
    },
    Method {
        name: "terminal.list",
        required: &[],
        optional: &[],
    },
    Method {
        name: "terminal.submit",
        required: &["terminal", "text"],
        optional: &[
            "bracketed",
            "settle_ms",
            "max_ms",
            "startup_quiet_ms",
            "submit",
            "expect_pid",
            "idle_since",
            "anyway",
        ],
    },
    Method {
        name: "terminal.signal",
        required: &["terminal", "signal"],
        optional: &[],
    },
    Method {
        name: "terminal.stop",
        required: &["terminal"],
        optional: &["grace_ms"],
    },
    Method {
        name: "terminal.text",
        required: &["terminal"],
        optional: &["lines"],
    },
    Method {
        name: "terminal.attach",
        required: &["terminal", "cols", "rows"],
        optional: &["mode", "history", "formats", "resume_from"],
    },
    Method {
        name: "events.subscribe",
        required: &[],
        optional: &["after", "instance"],
    },
];

/// The events and the fields each always carries, besides `seq`.
pub const EVENTS: &[(&str, &[&str])] = &[
    ("spawned", &["terminal", "pid", "labels", "cols", "rows"]),
    ("exited", &["terminal", "reason"]),
    ("title", &["terminal", "text"]),
    ("foreground", &["terminal", "pid", "name", "args", "cwd"]),
    ("bell", &["terminal"]),
    ("notify", &["terminal", "title", "body"]),
    ("program_status", &["terminal", "state"]),
    ("input", &["terminal", "kind"]),
    ("size", &["terminal", "cols", "rows"]),
    ("missed", &[]),
    ("ping", &[]),
];

/// The codes of an error's `data.code`.
pub const ERRORS: &[&str] = &[
    "not_found",
    "folder_missing",
    "limit",
    "used_since_idle",
    "not_foreground",
    "exited",
    "busy",
    "spawn_failed",
    "signal_failed",
    "unsupported",
    "invalid",
];

/// The schema as JSON, in the form of the released files.
pub fn schema() -> Value {
    let methods: Map<String, Value> = METHODS
        .iter()
        .map(|m| {
            (
                m.name.to_string(),
                json!({"required": m.required, "optional": m.optional}),
            )
        })
        .collect();
    let events: Map<String, Value> = EVENTS
        .iter()
        .map(|(name, fields)| (name.to_string(), json!(fields)))
        .collect();
    json!({
        "generation": crate::GENERATION,
        "methods": methods,
        "events": events,
        "errors": ERRORS,
    })
}

/// The ways `current` breaks a client of `released`: a method, parameter,
/// event, field or error code removed, or a parameter newly required.
pub fn breaking_changes(released: &Value, current: &Value) -> Vec<String> {
    let names = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect()
    };
    let mut broken = Vec::new();
    if released["generation"] != current["generation"] {
        broken.push("the generation changed".to_string());
    }
    for (name, method) in released["methods"].as_object().into_iter().flatten() {
        let Some(now) = current["methods"].get(name) else {
            broken.push(format!("method {name} was removed"));
            continue;
        };
        let (required, optional) = (names(&now["required"]), names(&now["optional"]));
        for param in names(&method["required"])
            .into_iter()
            .chain(names(&method["optional"]))
        {
            if !required.contains(&param) && !optional.contains(&param) {
                broken.push(format!("{name} no longer takes {param}"));
            }
        }
        let was_required = names(&method["required"]);
        for param in required {
            if !was_required.contains(&param) {
                broken.push(format!("{name} now requires {param}"));
            }
        }
    }
    for (name, fields) in released["events"].as_object().into_iter().flatten() {
        let Some(now) = current["events"].get(name) else {
            broken.push(format!("event {name} was removed"));
            continue;
        };
        let now = names(now);
        for field in names(fields) {
            if !now.contains(&field) {
                broken.push(format!("event {name} no longer carries {field}"));
            }
        }
    }
    let errors = names(&current["errors"]);
    for code in names(&released["errors"]) {
        if !errors.contains(&code) {
            broken.push(format!("error {code} was removed"));
        }
    }
    broken
}

#[cfg(test)]
mod tests {
    use super::*;

    fn released() -> Value {
        serde_json::from_str(include_str!("../protocol/generation-1.json")).unwrap()
    }

    #[test]
    fn the_protocol_keeps_everything_the_released_generation_has() {
        assert_eq!(
            breaking_changes(&released(), &schema()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn removing_or_requiring_something_is_a_breaking_change() {
        let mut changed = schema();
        changed["methods"]
            .as_object_mut()
            .unwrap()
            .remove("terminal.text");
        changed["methods"]["terminal.stop"]["required"] = json!(["terminal", "grace_ms"]);
        changed["methods"]["terminal.stop"]["optional"] = json!([]);
        changed["events"]["exited"] = json!(["terminal"]);
        assert_eq!(
            breaking_changes(&schema(), &changed),
            [
                "terminal.stop now requires grace_ms",
                "method terminal.text was removed",
                "event exited no longer carries reason",
            ]
        );
        // an added method or optional parameter is not
        let mut added = schema();
        added["methods"]["terminal.stop"]["optional"] = json!(["grace_ms", "reason"]);
        added["methods"]["terminal.rename"] = json!({"required": ["terminal"], "optional": []});
        assert!(breaking_changes(&schema(), &added).is_empty());
    }
}
