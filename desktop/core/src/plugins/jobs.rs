//! Installation progress and job results, shared by every interface.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

use crate::error::Error;

/// What an install says as it goes: the step it is at, and lines of the
/// build's output.
pub(super) enum Progress {
    Step(&'static str),
    Log(String),
}

/// One install, followed by id: its step, its log, and how it ended.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub source: String,
    /// `fetching`, `inspecting`, `building`, `placing`, `done`, `failed`
    pub status: String,
    pub log: String,
    pub error: Option<String>,
    /// the plugin's row, once done
    pub plugin: Option<Value>,
}

impl Job {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "source": self.source,
            "status": self.status,
            "log": self.log,
            "error": self.error,
            "plugin": self.plugin,
        })
    }
}

/// The jobs the app has run, by id, for the dialog and the CLI to follow.
#[derive(Debug, Default)]
pub(super) struct Jobs(Mutex<HashMap<String, Job>>);

impl Jobs {
    pub fn start(&self, source: &str) -> String {
        let id = crate::id::next().replace("r_", "j_");
        self.0.lock().unwrap().insert(
            id.clone(),
            Job {
                id: id.clone(),
                source: source.to_string(),
                status: "fetching".into(),
                log: String::new(),
                error: None,
                plugin: None,
            },
        );
        id
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.0.lock().unwrap().get(id).cloned()
    }

    pub fn note(&self, id: &str, progress: Progress) {
        let mut jobs = self.0.lock().unwrap();
        let Some(job) = jobs.get_mut(id) else { return };
        match progress {
            Progress::Step(step) => job.status = step.to_string(),
            Progress::Log(line) => {
                job.log.push_str(&line);
                job.log.push('\n');
            }
        }
    }

    pub fn finish(&self, id: &str, outcome: Result<Value, Error>) {
        let mut jobs = self.0.lock().unwrap();
        let Some(job) = jobs.get_mut(id) else { return };
        match outcome {
            Ok(plugin) => {
                job.status = "done".into();
                job.plugin = Some(plugin);
            }
            Err(error) => {
                job.status = "failed".into();
                job.error = Some(error.to_string());
            }
        }
    }
}
