//! Questions an agent asks with `pinrail ask`: a review of the kind
//! `question`, with no plugin. The app shows them with a form of its own,
//! and they are checked here, against the core's own schemas, and then
//! against each other: ids that are unique, choices that the answers and
//! recommendations name.

use std::collections::HashSet;
use std::sync::LazyLock;

use pinrail_format::schema::Schema;
use serde_json::{Value, json};

use crate::error::Violation;

/// The most questions one review asks.
pub const MAX_QUESTIONS: usize = 10;

/// The answer types, as a question's `answer` names them.
const TEXT: &str = "text";
const CHOICE: &str = "choice";
const CHOICES: &str = "choices";
const YES_NO: &str = "yes-no";

/// The keys an answer gives its answer in, one of which it has.
const ANSWERS: [&str; 5] = ["text", "choice", "choices", "yes", "unanswered"];

static PAYLOAD: LazyLock<Schema> = LazyLock::new(|| {
    Schema::standalone(&json!({
        "type": "object",
        "required": ["questions"],
        "additionalProperties": false,
        "properties": {
            "questions": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_QUESTIONS,
                "items": {
                    "type": "object",
                    "required": ["id", "question", "answer"],
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "minLength": 1, "maxLength": 64 },
                        "question": { "type": "string", "minLength": 1, "maxLength": 300 },
                        "answer": { "enum": [TEXT, CHOICE, CHOICES, YES_NO] },
                        "choices": {
                            "type": "array",
                            "maxItems": 20,
                            "items": { "type": "string", "minLength": 1, "maxLength": 200 }
                        },
                        "recommended": { "type": "string" },
                        "context": { "type": "string", "maxLength": 20000 }
                    }
                }
            }
        }
    }))
    .expect("the questions' payload schema compiles")
});

static DECISION: LazyLock<Schema> = LazyLock::new(|| {
    Schema::standalone(&json!({
        "type": "object",
        "required": ["answers"],
        "additionalProperties": false,
        "properties": {
            "answers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["id"],
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string" },
                        "text": { "type": "string", "minLength": 1, "maxLength": 20000 },
                        "choice": { "type": "string" },
                        "choices": { "type": "array", "items": { "type": "string" } },
                        "yes": { "type": "boolean" },
                        "unanswered": { "const": true },
                        "note": { "type": "string", "maxLength": 4000 }
                    }
                }
            }
        }
    }))
    .expect("the questions' decision schema compiles")
});

/// The answers as an agent reads them back, in the order they were asked,
/// with the questions left unanswered said, so silence never reads as a no.
pub const TEMPLATE: &str = r#"## Answers
{% for question in review.payload.questions %}
{%- set answer = data.answers | selectattr("id", "eq", question.id) | first %}
{{ loop.index }}. {{ question.question }}
{% if answer is undefined or answer.unanswered -%}
Not answered.
{% elif answer.choice is defined -%}
{{ answer.choice }}
{% elif answer.choices is defined -%}
{% for choice in answer.choices %}{{ choice }}
{% endfor -%}
{% elif answer.yes is defined -%}
{{ "yes" if answer.yes else "no" }}
{% else -%}
{% if question.answer != "text" %}Answered in their own words:
{% endif %}{{ answer.text }}
{% endif -%}
{% if answer.note %}Note: {{ answer.note }}
{% endif -%}
{% endfor %}"#;

/// Every violation of the payload, with paths inside it.
pub fn check_payload(payload: &Value) -> Vec<Violation> {
    let violations = PAYLOAD.validate(payload);
    if !violations.is_empty() {
        return violations;
    }
    let mut violations = Vec::new();
    let mut ids = HashSet::new();
    for (i, question) in questions(payload).iter().enumerate() {
        let at = |key: &str| format!("/questions/{i}/{key}");
        if !ids.insert(question["id"].as_str()) {
            violations.push(Violation::new(at("id"), "is the id of an earlier question"));
        }
        let answer = question["answer"].as_str().unwrap_or_default();
        let choices = choices(question);
        match (answer, &choices) {
            (CHOICE | CHOICES, None) => {
                violations.push(Violation::new(at("choices"), "is required for this answer"));
            }
            (CHOICE | CHOICES, Some(list)) if list.len() < 2 => {
                violations.push(Violation::new(
                    at("choices"),
                    "must offer at least 2 choices",
                ));
            }
            (CHOICE | CHOICES, Some(list))
                if list.iter().collect::<HashSet<_>>().len() < list.len() =>
            {
                violations.push(Violation::new(at("choices"), "must not repeat a choice"));
            }
            (TEXT | YES_NO, Some(_)) => {
                violations.push(Violation::new(
                    at("choices"),
                    format!("is not taken by a {answer} question"),
                ));
            }
            _ => {}
        }
        if let Some(recommended) = question["recommended"].as_str() {
            let fits = match answer {
                CHOICE | CHOICES => choices.is_some_and(|list| list.contains(&recommended)),
                YES_NO => recommended == "yes" || recommended == "no",
                _ => false,
            };
            if !fits {
                let message = match answer {
                    YES_NO => "must be yes or no".to_string(),
                    TEXT => "is not taken by a text question".to_string(),
                    _ => "must be one of the choices".to_string(),
                };
                violations.push(Violation::new(at("recommended"), message));
            }
        }
    }
    violations
}

/// Every violation of the answers to `payload`'s questions, with paths
/// inside the decision.
pub fn check_decision(payload: &Value, data: &Value) -> Vec<Violation> {
    let violations = DECISION.validate(data);
    if !violations.is_empty() {
        return violations;
    }
    let questions = questions(payload);
    let mut violations = Vec::new();
    let mut answered = HashSet::new();
    for (i, answer) in data["answers"].as_array().into_iter().flatten().enumerate() {
        let id = answer["id"].as_str().unwrap_or_default();
        let Some(question) = questions.iter().find(|q| q["id"] == id) else {
            violations.push(Violation::new(
                format!("/answers/{i}/id"),
                format!("names no question: {id}"),
            ));
            continue;
        };
        if !answered.insert(id) {
            violations.push(Violation::new(
                format!("/answers/{i}/id"),
                format!("{id} is answered twice"),
            ));
            continue;
        }
        let given: Vec<&str> = ANSWERS
            .into_iter()
            .filter(|k| answer.get(*k).is_some())
            .collect();
        let kind = question["answer"].as_str().unwrap_or_default();
        let [key] = given[..] else {
            violations.push(Violation::new(
                format!("/answers/{i}"),
                "must have exactly one of text, choice, choices, yes and unanswered",
            ));
            continue;
        };
        // words in place of a choice are always taken
        let fits = matches!(
            (kind, key),
            (_, "text" | "unanswered")
                | (CHOICE, "choice")
                | (CHOICES, "choices")
                | (YES_NO, "yes")
        );
        if !fits {
            violations.push(Violation::new(
                format!("/answers/{i}"),
                format!("a {kind} question is not answered with {key}"),
            ));
            continue;
        }
        let offered = choices(question).unwrap_or_default();
        let picked: Vec<&str> = match key {
            "choice" => answer["choice"].as_str().into_iter().collect(),
            "choices" => answer["choices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect(),
            _ => Vec::new(),
        };
        if let Some(stray) = picked.iter().find(|c| !offered.contains(c)) {
            violations.push(Violation::new(
                format!("/answers/{i}/{key}"),
                format!("is not one of the choices: {stray}"),
            ));
        }
    }
    let missing: Vec<&str> = questions
        .iter()
        .filter_map(|q| q["id"].as_str())
        .filter(|id| !answered.contains(id))
        .collect();
    if !missing.is_empty() && violations.is_empty() {
        violations.push(Violation::new(
            "/answers",
            format!(
                "has no answer for {}; leave a question with unanswered: true",
                missing.join(", ")
            ),
        ));
    }
    violations
}

/// What the questions ask, as a review's summary: how many.
pub fn request_summary(payload: &Value) -> Option<Value> {
    let n = questions(payload).len();
    let label = if n == 1 { "question" } else { "questions" };
    (n > 0).then(|| json!({ "counts": [{ "label": label, "count": n, "tone": "neutral" }] }))
}

/// What was answered, as a decision's summary: how many were answered and
/// how many were not.
pub fn outcome_summary(data: &Value) -> Option<Value> {
    let answers = data["answers"].as_array()?;
    let unanswered = answers.iter().filter(|a| a["unanswered"] == true).count();
    let counts: Vec<Value> = [
        ("answered", answers.len() - unanswered, "success"),
        ("unanswered", unanswered, "warning"),
    ]
    .into_iter()
    .filter(|(_, n, _)| *n > 0)
    .map(|(label, count, tone)| json!({ "label": label, "count": count, "tone": tone }))
    .collect();
    (!counts.is_empty()).then(|| json!({ "counts": counts }))
}

fn questions(payload: &Value) -> &[Value] {
    payload["questions"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn choices(question: &Value) -> Option<Vec<&str>> {
    question["choices"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
}
