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

/// How many finished jobs are kept for the dialog and the CLI to read back.
const FINISHED_KEPT: usize = 50;
/// The most of a job's log kept in memory; the whole log is on disk.
const LOG_KEPT: usize = 256 * 1024;

/// The jobs the app has run, by id, for the dialog and the CLI to follow.
#[derive(Debug, Default)]
pub(super) struct Jobs(Mutex<HashMap<String, Job>>);

impl Jobs {
    pub fn start(&self, source: &str) -> String {
        let id = crate::id::next().replace("r_", "j_");
        let mut jobs = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // ids sort by time: the oldest finished jobs go first
        let mut finished: Vec<String> = jobs
            .values()
            .filter(|j| j.status == "done" || j.status == "failed")
            .map(|j| j.id.clone())
            .collect();
        if finished.len() > FINISHED_KEPT {
            finished.sort();
            for old in &finished[..finished.len() - FINISHED_KEPT] {
                jobs.remove(old);
            }
        }
        jobs.insert(
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
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    pub fn note(&self, id: &str, progress: Progress) {
        let mut jobs = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(job) = jobs.get_mut(id) else { return };
        match progress {
            Progress::Step(step) => job.status = step.to_string(),
            Progress::Log(line) => {
                job.log.push_str(&line);
                job.log.push('\n');
                if job.log.len() > LOG_KEPT {
                    // the start goes, from the first whole line that fits
                    let from = job.log.len() - LOG_KEPT;
                    let cut = job.log[from..].find('\n').map_or(from, |i| from + i + 1);
                    job.log.drain(..cut);
                }
            }
        }
    }

    pub fn finish(&self, id: &str, outcome: Result<Value, Error>) {
        let mut jobs = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_recent_finished_jobs_are_kept_and_logs_are_capped() {
        let jobs = Jobs::default();
        let first = jobs.start("first");
        jobs.finish(&first, Ok(Value::Null));
        for i in 0..(FINISHED_KEPT + 5) {
            let id = jobs.start(&format!("job {i}"));
            jobs.finish(&id, Ok(Value::Null));
        }
        let running = jobs.start("running");
        assert!(
            jobs.get(&first).is_none(),
            "the oldest finished job was kept"
        );
        assert!(jobs.get(&running).is_some());
        assert!(jobs.0.lock().unwrap().len() <= FINISHED_KEPT + 1);

        let line = "x".repeat(1000);
        for _ in 0..1000 {
            jobs.note(&running, Progress::Log(line.clone()));
        }
        let log = jobs.get(&running).unwrap().log;
        assert!(log.len() <= LOG_KEPT, "{} bytes", log.len());
        assert!(log.ends_with(&format!("{line}\n")));
    }
}
