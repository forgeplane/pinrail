//! A review's summaries, as its plugin declares them: what a review asks,
//! counted from its payload, and what was decided, counted from the
//! decision. The manifest's `summary` names the arrays to count, the field
//! to group them by, and the label and tone of each value; nothing runs to
//! produce a summary, and a plugin that declares none has none.

use serde_json::{Map, Value, json};

/// The most counts a summary shows.
pub const MAX_COUNTS: usize = 6;
/// The longest label a count may have.
pub const MAX_LABEL: usize = 24;

const TONES: &[&str] = &["danger", "warning", "info", "success", "neutral"];

/// The manifest's `summary`: a rule for the request and one for the
/// outcome, each optional.
#[derive(Debug, Clone, Default)]
pub struct Declaration {
    pub request: Option<Rules>,
    pub outcome: Option<Rules>,
}

#[derive(Debug, Clone)]
pub struct Rules {
    counts: Vec<Count>,
    verdict: Option<Verdict>,
}

#[derive(Debug, Clone)]
struct Count {
    items: Vec<Step>,
    group: Group,
}

#[derive(Debug, Clone)]
enum Group {
    /// The array's length, under one label.
    Whole(Label),
    /// One count per value of a field, in the declared order, and the
    /// values not listed as `other` unless that is turned off. An element
    /// without the field is not counted.
    By {
        field: String,
        values: Vec<(String, Label)>,
        other: bool,
    },
}

#[derive(Debug, Clone)]
struct Verdict {
    at: Vec<Step>,
    values: Vec<(String, Label)>,
}

#[derive(Debug, Clone)]
struct Label {
    label: String,
    /// The label for any count but one, when it differs: `draft`, `drafts`.
    plural: Option<String>,
    tone: &'static str,
}

/// A step of a pointer: a key or index, or `*` for every element.
#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Each,
}

impl Declaration {
    /// Reads the manifest's `summary`, or says what is wrong with it and
    /// where, as `summary/request/counts/0: …`.
    pub fn load(raw: &Value) -> Result<Declaration, String> {
        let raw = raw.as_object().ok_or("summary: must be an object")?;
        let mut declaration = Declaration::default();
        for (key, value) in raw {
            let outcome = match key.as_str() {
                "request" => false,
                "outcome" => true,
                other => return Err(format!("summary/{other}: is not request or outcome")),
            };
            let rules =
                Rules::load(value, outcome).map_err(|e| within(&format!("summary/{key}"), e))?;
            if outcome {
                declaration.outcome = Some(rules);
            } else {
                declaration.request = Some(rules);
            }
        }
        Ok(declaration)
    }
}

impl Rules {
    fn load(raw: &Value, outcome: bool) -> Result<Rules, String> {
        let raw = raw.as_object().ok_or("must be an object")?;
        known_keys(raw, &["counts", "verdict"])?;
        if raw.contains_key("verdict") && !outcome {
            return Err("verdict is for the outcome".into());
        }
        let counts = match raw.get("counts") {
            None => Vec::new(),
            Some(Value::Array(list)) => list
                .iter()
                .enumerate()
                .map(|(i, rule)| Count::load(rule).map_err(|e| within(&format!("counts/{i}"), e)))
                .collect::<Result<_, _>>()?,
            Some(_) => return Err("counts must be a list".into()),
        };
        let verdict = raw
            .get("verdict")
            .map(|rule| Verdict::load(rule).map_err(|e| within("verdict", e)))
            .transpose()?;
        let most: usize = counts.iter().map(Count::most).sum();
        if most > MAX_COUNTS {
            return Err(format!(
                "counts declare up to {most} entries; a summary shows at most {MAX_COUNTS}"
            ));
        }
        Ok(Rules { counts, verdict })
    }

    /// The summary of `data`, the payload or the decision: its counts in
    /// the declared order, those of zero left out, and the verdict. `None`
    /// when there is nothing to show.
    pub fn derive(&self, data: &Value) -> Option<Value> {
        let mut counts = Vec::new();
        for rule in &self.counts {
            let items = select(data, &rule.items);
            match &rule.group {
                Group::Whole(label) => {
                    push(&mut counts, label, items.iter().map(|a| a.len()).sum());
                }
                Group::By {
                    field,
                    values,
                    other,
                } => {
                    // each element's value of the field, when it is text
                    let found: Vec<Option<&str>> = items
                        .iter()
                        .flat_map(|a| a.iter())
                        .map(|element| element.get(field).and_then(Value::as_str))
                        .collect();
                    for (value, label) in values {
                        let n = found.iter().filter(|v| **v == Some(value.as_str())).count();
                        push(&mut counts, label, n);
                    }
                    if *other {
                        let n = found
                            .iter()
                            .flatten()
                            .filter(|v| !values.iter().any(|(value, _)| value == *v))
                            .count();
                        push(&mut counts, &Label::other(), n);
                    }
                }
            }
        }
        let verdict = self.verdict.as_ref().and_then(|rule| {
            let found = lookup(data, &rule.at)?.as_str()?;
            let (_, label) = rule.values.iter().find(|(value, _)| value == found)?;
            Some(json!({ "label": label.label, "tone": label.tone }))
        });
        if counts.is_empty() && verdict.is_none() {
            return None;
        }
        let mut summary = json!({ "counts": counts });
        if let Some(verdict) = verdict {
            summary["verdict"] = verdict;
        }
        Some(summary)
    }
}

impl Count {
    fn load(raw: &Value) -> Result<Count, String> {
        let raw = raw.as_object().ok_or("must be an object")?;
        known_keys(
            raw,
            &["items", "by", "values", "other", "label", "plural", "tone"],
        )?;
        let items = pointer(raw.get("items").ok_or("items is required")?)?;
        let group = match raw.get("by") {
            None => {
                for key in ["values", "other"] {
                    if raw.contains_key(key) {
                        return Err(format!("{key} needs by"));
                    }
                }
                Group::Whole(Label::load(raw, None)?.ok_or("label is required without by")?)
            }
            Some(Value::String(field)) if !field.is_empty() => {
                for key in ["label", "plural", "tone"] {
                    if raw.contains_key(key) {
                        return Err(format!("{key} is set per value with by"));
                    }
                }
                let values = values(raw.get("values").ok_or("values is required with by")?)?;
                let other = match raw.get("other") {
                    None => true,
                    Some(Value::Bool(other)) => *other,
                    Some(_) => return Err("other must be true or false".into()),
                };
                Group::By {
                    field: field.clone(),
                    values,
                    other,
                }
            }
            Some(_) => return Err("by must name a field".into()),
        };
        Ok(Count { items, group })
    }

    /// How many entries the rule can add to a summary.
    fn most(&self) -> usize {
        match &self.group {
            Group::Whole(_) => 1,
            Group::By { values, other, .. } => values.len() + usize::from(*other),
        }
    }
}

impl Verdict {
    fn load(raw: &Value) -> Result<Verdict, String> {
        let raw = raw.as_object().ok_or("must be an object")?;
        known_keys(raw, &["at", "values"])?;
        let at = pointer(raw.get("at").ok_or("at is required")?)?;
        if at.iter().any(|step| matches!(step, Step::Each)) {
            return Err("at names one field; * is for counts".into());
        }
        let values = values(raw.get("values").ok_or("values is required")?)?;
        Ok(Verdict { at, values })
    }
}

impl Label {
    /// The label and tone in `raw`; `fallback` is the label when only the
    /// tone is given. `None` when neither is.
    fn load(raw: &Map<String, Value>, fallback: Option<&str>) -> Result<Option<Label>, String> {
        let text = |key: &str| match raw.get(key) {
            None => Ok(None),
            Some(Value::String(text)) if !text.trim().is_empty() => {
                if text.chars().count() > MAX_LABEL {
                    Err(format!(
                        "{key} {text:?} is longer than {MAX_LABEL} characters"
                    ))
                } else {
                    Ok(Some(text.clone()))
                }
            }
            Some(_) => Err(format!("{key} must be text")),
        };
        let plural = text("plural")?;
        let Some(label) = text("label")?.or_else(|| fallback.map(str::to_string)) else {
            return match plural {
                Some(_) => Err("plural needs a label".into()),
                None => Ok(None),
            };
        };
        let tone = match raw.get("tone") {
            None => "neutral",
            Some(Value::String(tone)) => TONES
                .iter()
                .find(|known| *known == tone)
                .ok_or_else(|| format!("tone {tone:?} is not one of {}", TONES.join(", ")))?,
            Some(_) => return Err("tone must be text".into()),
        };
        Ok(Some(Label {
            label,
            plural,
            tone,
        }))
    }

    fn other() -> Label {
        Label {
            label: "other".into(),
            plural: None,
            tone: "neutral",
        }
    }
}

/// `values`: each value of the field, with its label and tone, in order.
fn values(raw: &Value) -> Result<Vec<(String, Label)>, String> {
    let raw = raw.as_object().ok_or("values must be an object")?;
    if raw.is_empty() {
        return Err("values lists no value".into());
    }
    raw.iter()
        .map(|(value, spec)| {
            let spec = spec
                .as_object()
                .ok_or_else(|| format!("values/{value} must be an object"))?;
            known_keys(spec, &["label", "plural", "tone"])
                .map_err(|e| within(&format!("values/{value}"), e))?;
            let label = Label::load(spec, Some(value))
                .map_err(|e| within(&format!("values/{value}"), e))?
                .expect("a value is its own label");
            Ok((value.clone(), label))
        })
        .collect()
}

/// A JSON Pointer (RFC 6901), with `*` for every element of an array.
fn pointer(raw: &Value) -> Result<Vec<Step>, String> {
    let text = raw.as_str().ok_or("a pointer must be text")?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let rest = text
        .strip_prefix('/')
        .ok_or_else(|| format!("{text:?} is not a JSON Pointer; it starts with /"))?;
    Ok(rest
        .split('/')
        .map(|step| match step {
            "*" => Step::Each,
            key => Step::Key(key.replace("~1", "/").replace("~0", "~")),
        })
        .collect())
}

/// The arrays `steps` reaches in `data`: one, or one per element a `*`
/// passes through. What is missing or not an array is left out.
fn select<'a>(data: &'a Value, steps: &[Step]) -> Vec<&'a Vec<Value>> {
    let mut found = Vec::new();
    walk(data, steps, &mut found);
    found
}

fn walk<'a>(data: &'a Value, steps: &[Step], found: &mut Vec<&'a Vec<Value>>) {
    match steps.split_first() {
        None => {
            if let Value::Array(items) = data {
                found.push(items);
            }
        }
        Some((Step::Each, rest)) => {
            if let Value::Array(items) = data {
                for item in items {
                    walk(item, rest, found);
                }
            }
        }
        Some((Step::Key(key), rest)) => {
            if let Some(next) = child(data, key) {
                walk(next, rest, found);
            }
        }
    }
}

/// The value one pointer without `*` reaches.
fn lookup<'a>(data: &'a Value, steps: &[Step]) -> Option<&'a Value> {
    steps.iter().try_fold(data, |at, step| match step {
        Step::Key(key) => child(at, key),
        Step::Each => None,
    })
}

fn child<'a>(data: &'a Value, key: &str) -> Option<&'a Value> {
    match data {
        Value::Object(map) => map.get(key),
        Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
        _ => None,
    }
}

/// `error` placed under `path`: a message that starts with a path of its
/// own (`counts/0: …`) is joined to it, any other follows it.
fn within(path: &str, error: String) -> String {
    match error.split_once(": ") {
        Some((inner, _)) if !inner.contains(' ') => format!("{path}/{error}"),
        _ => format!("{path}: {error}"),
    }
}

fn push(counts: &mut Vec<Value>, label: &Label, count: usize) {
    if count > 0 {
        let text = match &label.plural {
            Some(plural) if count != 1 => plural,
            _ => &label.label,
        };
        counts.push(json!({ "label": text, "count": count, "tone": label.tone }));
    }
}

fn known_keys(raw: &Map<String, Value>, known: &[&str]) -> Result<(), String> {
    match raw.keys().find(|key| !known.contains(&key.as_str())) {
        Some(key) => Err(format!("{key} is not a key here")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration(raw: Value) -> Declaration {
        Declaration::load(&raw).unwrap()
    }

    #[test]
    fn counts_group_by_a_field_in_the_declared_order() {
        let rules = declaration(json!({ "request": { "counts": [{
            "items": "/proposals",
            "by": "severity",
            "values": { "blocker": { "tone": "danger" }, "major": { "tone": "warning" }, "nit": {} }
        }]}}))
        .request
        .unwrap();
        // a proposal without a severity has none to count, not "other"
        let payload = json!({ "proposals": [
            { "severity": "nit" }, { "severity": "major" }, { "severity": "major" }, { "severity": "odd" }, {}
        ]});
        assert_eq!(
            rules.derive(&payload),
            Some(json!({ "counts": [
                { "label": "major", "count": 2, "tone": "warning" },
                { "label": "nit", "count": 1, "tone": "neutral" },
                { "label": "other", "count": 1, "tone": "neutral" }
            ]}))
        );
    }

    #[test]
    fn a_star_counts_across_nested_arrays_and_other_can_be_left_out() {
        let rules = declaration(json!({ "request": { "counts": [{
            "items": "/groups/*/items",
            "by": "severity",
            "values": { "high": { "label": "urgent", "tone": "danger" } },
            "other": false
        }]}}))
        .request
        .unwrap();
        let payload = json!({ "groups": [
            { "items": [{ "severity": "high" }, { "severity": "low" }] },
            { "items": [{ "severity": "high" }] },
            { "title": "no items here" }
        ]});
        assert_eq!(
            rules.derive(&payload),
            Some(json!({ "counts": [{ "label": "urgent", "count": 2, "tone": "danger" }] }))
        );
    }

    #[test]
    fn an_outcome_counts_whole_arrays_and_reads_a_verdict() {
        let rules = declaration(json!({ "outcome": {
            "counts": [{ "items": "/undecided", "label": "undecided" }],
            "verdict": { "at": "/verdict", "values": {
                "approve": { "label": "approved", "tone": "success" },
                "revise": { "label": "changes requested", "tone": "warning" }
            }}
        }}))
        .outcome
        .unwrap();
        assert_eq!(
            rules.derive(&json!({ "verdict": "revise", "undecided": [3, 4] })),
            Some(json!({
                "counts": [{ "label": "undecided", "count": 2, "tone": "neutral" }],
                "verdict": { "label": "changes requested", "tone": "warning" }
            }))
        );
        // a verdict the plugin did not declare is not shown
        assert_eq!(
            rules.derive(&json!({ "verdict": "maybe", "undecided": [] })),
            None
        );
    }

    #[test]
    fn a_noun_takes_its_plural_for_any_count_but_one() {
        let rules = declaration(json!({ "request": { "counts": [
            { "items": "/drafts", "label": "draft", "plural": "drafts" },
            { "items": "/notes", "label": "note", "plural": "notes" }
        ]}}))
        .request
        .unwrap();
        assert_eq!(
            rules.derive(&json!({ "drafts": [1, 2, 3], "notes": [1] })),
            Some(json!({ "counts": [
                { "label": "drafts", "count": 3, "tone": "neutral" },
                { "label": "note", "count": 1, "tone": "neutral" }
            ]}))
        );
    }

    #[test]
    fn nothing_to_count_is_no_summary() {
        let rules = declaration(
            json!({ "request": { "counts": [{ "items": "/drafts", "label": "drafts" }] }}),
        )
        .request
        .unwrap();
        assert_eq!(rules.derive(&json!({ "drafts": [] })), None);
        assert_eq!(rules.derive(&json!({ "other": 1 })), None);
        assert_eq!(rules.derive(&json!("not an object")), None);
    }

    /// Every `plugins/*/fixtures/*.decided.json` sums up, by its plugin's
    /// declaration, to the `.decided.summary.json` beside it: the request
    /// from the payload and the outcome from the decision.
    /// `UPDATE_FIXTURES=1 cargo test` rewrites the expected files.
    #[test]
    fn decided_fixtures_sum_up_to_their_expected_summaries() {
        let plugins = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let update = std::env::var("UPDATE_FIXTURES").is_ok();
        let mut seen = 0;
        for dir in std::fs::read_dir(&plugins)
            .unwrap()
            .flatten()
            .map(|e| e.path())
        {
            if !dir.join("manifest.json").is_file() {
                continue;
            }
            let plugin = super::super::Plugin::load(&dir);
            assert_eq!(plugin.summary_error, None, "{}", dir.display());
            let Ok(fixtures) = std::fs::read_dir(dir.join("fixtures")) else {
                continue;
            };
            for path in fixtures.flatten().map(|e| e.path()) {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                let Some(stem) = name.strip_suffix(".decided.json") else {
                    continue;
                };
                let fixture: Value =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                let derive = |rules: &Option<Rules>, data: &Value| {
                    rules
                        .as_ref()
                        .and_then(|r| r.derive(data))
                        .unwrap_or(Value::Null)
                };
                let summed = json!({
                    "request": derive(&plugin.summary.request, &fixture["payload"]),
                    "outcome": derive(&plugin.summary.outcome, &fixture["decision"]["data"]),
                });
                let expected_path = path.with_file_name(format!("{stem}.decided.summary.json"));
                if update {
                    let text = serde_json::to_string_pretty(&summed).unwrap() + "\n";
                    std::fs::write(&expected_path, text).unwrap();
                }
                let expected: Value = serde_json::from_str(
                    &std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
                        panic!(
                            "{} is missing; run with UPDATE_FIXTURES=1 to write it",
                            expected_path.display()
                        )
                    }),
                )
                .unwrap();
                assert_eq!(summed, expected, "{}", path.display());
                seen += 1;
            }
        }
        assert!(seen >= 7, "only {seen} decided fixtures found");
    }

    #[test]
    fn a_wrong_declaration_says_where() {
        let wrong = |raw: Value| Declaration::load(&raw).unwrap_err();
        assert_eq!(
            wrong(json!({ "reqest": { "counts": [] } })),
            "summary/reqest: is not request or outcome"
        );
        assert_eq!(
            wrong(json!({ "request": { "counts": [{ "items": "proposals", "label": "x" }] }})),
            "summary/request/counts/0: \"proposals\" is not a JSON Pointer; it starts with /"
        );
        assert_eq!(
            wrong(
                json!({ "request": { "counts": [{ "items": "/a", "label": "x", "tone": "red" }] }})
            ),
            "summary/request/counts/0: tone \"red\" is not one of danger, warning, info, success, neutral"
        );
        assert_eq!(
            wrong(json!({ "request": { "verdict": { "at": "/v", "values": { "a": {} } } }})),
            "summary/request: verdict is for the outcome"
        );
        assert_eq!(
            wrong(json!({ "outcome": { "counts": [{
                "items": "/d", "by": "action",
                "values": { "a": {}, "b": {}, "c": {}, "d": {}, "e": {}, "f": {} }
            }]}})),
            "summary/outcome: counts declare up to 7 entries; a summary shows at most 6"
        );
        assert_eq!(
            wrong(
                json!({ "outcome": { "counts": [{ "items": "/d", "by": "action", "label": "x", "values": { "a": {} } }] }})
            ),
            "summary/outcome/counts/0: label is set per value with by"
        );
    }
}
