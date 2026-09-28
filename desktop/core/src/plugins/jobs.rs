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
    /// The end of the log: at most `LOG_KEPT` bytes of it.
    pub log: String,
    /// How many bytes of the log came before `log`: a reader that follows
    /// the log keeps its place by the total, which only grows.
    pub log_offset: usize,
    pub error: Option<String>,
    /// what kind of error ended it, as the API names kinds: `invalid` is
    /// the source's to fix, `unavailable` and `internal` are not
    pub error_kind: Option<String>,
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
            "log_offset": self.log_offset,
            "error": self.error,
            "error_kind": self.error_kind,
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
                log_offset: 0,
                error: None,
                error_kind: None,
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
                    // the start goes, up to the first whole line that fits;
                    // a newline is one byte, so the cut is a character boundary
                    let from = job.log.len() - LOG_KEPT;
                    let cut = match job.log.as_bytes()[from..].iter().position(|&b| b == b'\n') {
                        Some(i) => from + i + 1,
                        None => job.log.len(),
                    };
                    job.log.drain(..cut);
                    job.log_offset += cut;
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
                job.error_kind = error.to_json()["error"].as_str().map(str::to_string);
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

    #[test]
    fn a_capped_log_of_multi_byte_output_keeps_whole_lines() {
        let jobs = Jobs::default();
        let id = jobs.start("build");
        // what npm and vite print, box drawing and arrows: three bytes a
        // character, in lines of varying length, so the cut lands inside
        // characters as well as between them
        for i in 0..3000 {
            jobs.note(
                &id,
                Progress::Log(format!("{}→ {i}", "─".repeat(90 + i % 7))),
            );
        }
        let log = jobs.get(&id).unwrap().log;
        assert!(log.len() <= LOG_KEPT, "{} bytes", log.len());
        assert!(log.starts_with('─'), "the log starts mid-line");
        assert!(log.ends_with("→ 2999\n"));
    }
}
