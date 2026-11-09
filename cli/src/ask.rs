//! `pinrail ask`: questions for the person, each with the options that
//! follow it on the command line, sent as one review of the kind
//! `question` and answered in the app's own form.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use clap::{ArgAction, ArgMatches, Args};
use serde_json::{Map, Value, json};

use crate::origin;

#[derive(Args)]
pub struct AskArgs {
    /// The first question, as one line, which can use inline Markdown such
    /// as `code` for a command or a path
    #[arg(value_name = "QUESTION", required_unless_present = "data")]
    question: Option<String>,
    /// Another question. The options after it, up to the next --ask, apply
    /// to it; those before it apply to the question before. Up to 10
    /// questions in all
    #[arg(long = "ask", value_name = "QUESTION", action = ArgAction::Append)]
    more: Vec<String>,
    /// An answer the person can pick. Give it once for each choice, at least
    /// twice. A choice can contain spaces and commas. Without it, the person
    /// types the answer
    #[arg(long = "choice", value_name = "TEXT", action = ArgAction::Append)]
    choices: Vec<String>,
    /// With --choice: the person can pick more than one
    // appended, not counted, so clap keeps where each one stands
    #[arg(long, num_args = 0, default_missing_value = "true", action = ArgAction::Append)]
    multiple: Vec<bool>,
    /// A question answered with yes or no
    #[arg(long, num_args = 0, default_missing_value = "true", action = ArgAction::Append)]
    yes_no: Vec<bool>,
    /// The choice you would make, shown as recommended: one of the choices,
    /// or yes or no
    #[arg(long, value_name = "CHOICE", action = ArgAction::Append)]
    recommend: Vec<String>,
    /// More detail, shown below the question, as Markdown: given inline,
    /// as - for stdin, or as @FILE
    #[arg(long, value_name = "MARKDOWN|-|@FILE", action = ArgAction::Append)]
    context: Vec<String>,
    /// The reply you expect, one line: the app shows it in the person's
    /// empty reply box, and Tab accepts it. Such as "Yes, commit it and go
    /// on to PIN-55"
    #[arg(long, value_name = "TEXT")]
    pub suggest_reply: Option<String>,
    /// The title the inbox shows [default: the question, and with several,
    /// the first and how many more]
    #[arg(long)]
    pub title: Option<String>,
    /// The questions as JSON, given inline, as a file path, or as - for
    /// stdin, in place of the options above: {"questions": [{"id",
    /// "question", "answer", "choices", "recommended", "context"}]}, where
    /// answer is text, choice, choices or yes-no
    #[arg(long, value_name = "JSON|FILE|-", conflicts_with_all = ["question", "more", "choices", "multiple", "yes_no", "recommend", "context"])]
    data: Option<String>,
    /// Where the questions come from, as for submit
    #[arg(long, value_parser = origin::parse)]
    pub origin: Option<BTreeMap<String, String>>,
    /// Who is asking, as for submit
    #[arg(long, env = "PINRAIL_REQUESTED_BY")]
    pub requested_by: Option<String>,
    /// Stop waiting after this many seconds and exit with 4. 0 waits
    /// indefinitely. PINRAIL_TIMEOUT sets this for every wait
    #[arg(long)]
    pub timeout: Option<u64>,
    /// The work item this is for, by its key or its number in the
    /// checkout. Repeat it for several [default: every item the session
    /// holds]
    #[arg(long, value_name = "ITEM")]
    pub item: Vec<String>,
    /// Do not start the server when it is not running
    #[arg(long)]
    pub no_start: bool,
    /// Where each option stands on the command line, from `place`
    #[arg(skip)]
    places: Places,
}

/// The command-line index of each occurrence of the options that belong to
/// a question, so each can be given to the question it follows.
#[derive(Default)]
struct Places {
    question: Vec<usize>,
    more: Vec<usize>,
    choices: Vec<usize>,
    multiple: Vec<usize>,
    yes_no: Vec<usize>,
    recommend: Vec<usize>,
    context: Vec<usize>,
}

impl AskArgs {
    /// Records where each option stands, which clap's derive does not keep.
    pub fn place(&mut self, matches: &ArgMatches) {
        let at = |id: &str| -> Vec<usize> {
            matches
                .indices_of(id)
                .map(|i| i.collect())
                .unwrap_or_default()
        };
        self.places = Places {
            question: at("question"),
            more: at("more"),
            choices: at("choices"),
            multiple: at("multiple"),
            yes_no: at("yes_no"),
            recommend: at("recommend"),
            context: at("context"),
        };
    }

    /// The questions to send, from --data or from the options.
    pub fn questions(&self) -> Result<Vec<Value>> {
        let mut questions = match &self.data {
            Some(spec) => from_data(spec)?,
            None => self.built()?,
        };
        for (i, question) in questions.iter_mut().enumerate() {
            if !question.is_object() {
                bail!(
                    "question {} is not an object: give each question as \
                     {{\"question\": \"...\", ...}}",
                    i + 1
                );
            }
            if question.get("id").is_none() {
                question["id"] = json!(format!("q{}", i + 1));
            }
        }
        Ok(questions)
    }

    /// The questions the options build, each with those that follow it.
    fn built(&self) -> Result<Vec<Value>> {
        // each question with where it starts, in order
        let mut starts: Vec<(usize, &str)> = self
            .places
            .question
            .iter()
            .copied()
            .zip(self.question.iter().map(String::as_str))
            .chain(
                self.places
                    .more
                    .iter()
                    .copied()
                    .zip(self.more.iter().map(String::as_str)),
            )
            .collect();
        starts.sort_by_key(|(at, _)| *at);
        let owner = |at: usize| {
            starts
                .iter()
                .rposition(|(start, _)| *start < at)
                .unwrap_or(0)
        };
        let mut drafts: Vec<Draft> = starts.iter().map(|(_, text)| Draft::new(text)).collect();
        for (at, choice) in self.places.choices.iter().zip(&self.choices) {
            drafts[owner(*at)].choices.push(choice.clone());
        }
        for at in &self.places.multiple {
            drafts[owner(*at)].flag("--multiple", |d| &mut d.multiple)?;
        }
        for at in &self.places.yes_no {
            drafts[owner(*at)].flag("--yes-no", |d| &mut d.yes_no)?;
        }
        for (at, choice) in self.places.recommend.iter().zip(&self.recommend) {
            drafts[owner(*at)].once("--recommend", choice, |d| &mut d.recommended)?;
        }
        let mut stdin = false;
        for (at, spec) in self.places.context.iter().zip(&self.context) {
            if spec == "-" {
                if stdin {
                    bail!("only one --context can read stdin");
                }
                stdin = true;
            }
            drafts[owner(*at)].once("--context", &read_context(spec)?, |d| &mut d.context)?;
        }
        Ok(drafts.into_iter().map(Draft::into_question).collect())
    }
}

/// A question as its options build it.
struct Draft {
    question: String,
    choices: Vec<String>,
    multiple: bool,
    yes_no: bool,
    recommended: Option<String>,
    context: Option<String>,
}

impl Draft {
    fn new(question: &str) -> Self {
        Draft {
            question: question.to_string(),
            choices: Vec::new(),
            multiple: false,
            yes_no: false,
            recommended: None,
            context: None,
        }
    }

    fn flag(&mut self, name: &str, field: impl Fn(&mut Self) -> &mut bool) -> Result<()> {
        let question = self.question.clone();
        let set = field(self);
        if *set {
            bail!("{name} is given twice for the question {question:?}");
        }
        *set = true;
        Ok(())
    }

    fn once(
        &mut self,
        name: &str,
        value: &str,
        field: impl Fn(&mut Self) -> &mut Option<String>,
    ) -> Result<()> {
        let question = self.question.clone();
        let slot = field(self);
        if slot.is_some() {
            bail!("{name} is given twice for the question {question:?}");
        }
        *slot = Some(value.to_string());
        Ok(())
    }

    /// The question as the app takes it. Contradictory options, such as
    /// --yes-no with choices, are sent as given, for the app to refuse
    /// with its reason.
    fn into_question(self) -> Value {
        let answer = match (self.yes_no, self.choices.is_empty(), self.multiple) {
            (true, _, _) => "yes-no",
            (false, true, false) => "text",
            (false, _, false) => "choice",
            (false, _, true) => "choices",
        };
        let mut question = Map::new();
        question.insert("question".into(), json!(self.question));
        question.insert("answer".into(), json!(answer));
        if !self.choices.is_empty() {
            question.insert("choices".into(), json!(self.choices));
        }
        if let Some(recommended) = self.recommended {
            question.insert("recommended".into(), json!(recommended));
        }
        if let Some(context) = self.context {
            question.insert("context".into(), json!(context));
        }
        Value::Object(question)
    }
}

/// The questions --data gives: an object with `questions`, or the array.
fn from_data(spec: &str) -> Result<Vec<Value>> {
    match crate::read_json_arg(spec)? {
        Value::Array(list) => Ok(list),
        Value::Object(mut map) => match map.remove("questions") {
            Some(Value::Array(list)) => Ok(list),
            _ => bail!("--data must hold {{\"questions\": [...]}} or the array of questions"),
        },
        _ => bail!("--data must hold {{\"questions\": [...]}} or the array of questions"),
    }
}

/// --context's text: given inline, from stdin with `-`, or from a file
/// with `@FILE`.
fn read_context(spec: &str) -> Result<String> {
    if spec == "-" {
        return std::io::read_to_string(std::io::stdin()).context("reading stdin");
    }
    match spec.strip_prefix('@') {
        Some(path) => std::fs::read_to_string(path).with_context(|| format!("reading {path}")),
        None => Ok(spec.to_string()),
    }
}

/// The title the inbox shows for `questions`: the first, and how many more.
pub fn title(questions: &[Value]) -> String {
    let first = questions
        .first()
        .and_then(|q| q["question"].as_str())
        .unwrap_or("Questions");
    match questions.len() {
        0 | 1 => first.to_string(),
        n => format!("{first} and {} more", n - 1),
    }
}

/// The answers as an agent reads them: with one question, only its answer;
/// with several, each under its question, in the order they were asked;
/// then the person's comment on them all, if they wrote one.
pub fn answers(review: &Value) -> String {
    let mut text = each_answer(review);
    if let Some(comment) = review["agent_note"]
        .as_str()
        .filter(|c| !c.trim().is_empty())
    {
        text.push_str(&format!("\nComment: {comment}\n"));
    }
    text
}

fn each_answer(review: &Value) -> String {
    let questions = review["payload"]["questions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let answers = review["decision"]["data"]["answers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let answer_to = |question: &Value| {
        let found = answers.iter().find(|a| a["id"] == question["id"]);
        answer_lines(question, found)
    };
    match &questions[..] {
        [one] => answer_to(one),
        _ => questions
            .iter()
            .enumerate()
            .map(|(i, q)| {
                format!(
                    "{}. {}\n{}",
                    i + 1,
                    q["question"].as_str().unwrap_or_default(),
                    answer_to(q).replace("\n\nNote:", "\nNote:")
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// One answer: what was answered, then the person's note.
fn answer_lines(question: &Value, answer: Option<&Value>) -> String {
    let Some(answer) = answer.filter(|a| a["unanswered"] != true) else {
        return "Not answered.\n".to_string();
    };
    let mut text = if let Some(choice) = answer["choice"].as_str() {
        format!("{choice}\n")
    } else if let Some(list) = answer["choices"].as_array() {
        list.iter()
            .filter_map(Value::as_str)
            .map(|c| format!("{c}\n"))
            .collect()
    } else if let Some(yes) = answer["yes"].as_bool() {
        format!("{}\n", if yes { "yes" } else { "no" })
    } else {
        let words = answer["text"].as_str().unwrap_or_default();
        match question["answer"].as_str() {
            Some("text") => format!("{words}\n"),
            _ => format!("Answered in their own words:\n{words}\n"),
        }
    };
    if let Some(note) = answer["note"].as_str() {
        text.push_str(&format!("\nNote: {note}\n"));
    }
    text
}
