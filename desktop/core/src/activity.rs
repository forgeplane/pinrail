//! What the core is doing: the requests and background jobs in progress,
//! how long each waited for the database's write connection, and what
//! held the connection meanwhile. The log of slow requests and of a
//! stalled runtime is written from these.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A request or a job takes this long or more before it is logged.
pub const SLOW: Duration = Duration::from_millis(500);

tokio::task_local! {
    static CURRENT: Current;
}

/// The request or job a task or a thread runs.
struct Current {
    name: Arc<str>,
    db_wait: Cell<Duration>,
    /// what held the write connection during the longest wait
    held_by: RefCell<Option<(Duration, Arc<str>)>>,
}

impl Current {
    fn new(name: Arc<str>) -> Self {
        Current {
            name,
            db_wait: Cell::new(Duration::ZERO),
            held_by: RefCell::new(None),
        }
    }

    fn report(&self, took: Duration) -> Report {
        Report {
            took,
            db_wait: self.db_wait.get(),
            held_by: self.held_by.borrow().as_ref().map(|(_, name)| name.clone()),
        }
    }
}

/// How a request or a job went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub took: Duration,
    /// the time spent waiting for the database's write connection
    pub db_wait: Duration,
    /// what held the write connection during the longest of those waits
    pub held_by: Option<Arc<str>>,
}

impl Report {
    /// Its time, and its wait for the write connection when it waited.
    pub fn describe(&self) -> String {
        let mut text = format!("took {} ms", self.took.as_millis());
        if !self.db_wait.is_zero() {
            text += &format!(
                ", waited {} ms for the database's write connection",
                self.db_wait.as_millis()
            );
            if let Some(holder) = &self.held_by {
                text += &format!(", held by {holder}");
            }
        }
        text
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
static RUNNING: Mutex<BTreeMap<u64, (Arc<str>, Instant)>> = Mutex::new(BTreeMap::new());

/// A request or job listed as in progress until it is dropped.
struct Running(u64);

impl Running {
    fn add(name: Arc<str>) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        lock_running().insert(id, (name, Instant::now()));
        Running(id)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        lock_running().remove(&self.0);
    }
}

fn lock_running() -> std::sync::MutexGuard<'static, BTreeMap<u64, (Arc<str>, Instant)>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `work` as the request or job `name`, and reports how it went.
pub async fn run<F: Future>(name: impl Into<Arc<str>>, work: F) -> (F::Output, Report) {
    let name = name.into();
    let _running = Running::add(name.clone());
    let started = Instant::now();
    CURRENT
        .scope(Current::new(name), async move {
            let output = work.await;
            let report = CURRENT.with(|current| current.report(started.elapsed()));
            (output, report)
        })
        .await
}

/// Runs `work` on this thread as the job `name`, and reports how it went.
pub fn run_sync<R>(name: impl Into<Arc<str>>, work: impl FnOnce() -> R) -> (R, Report) {
    let name = name.into();
    let _running = Running::add(name.clone());
    let started = Instant::now();
    CURRENT.sync_scope(Current::new(name), || {
        let output = work();
        let report = CURRENT.with(|current| current.report(started.elapsed()));
        (output, report)
    })
}

/// The request or job running here, if any.
pub(crate) fn current() -> Option<Arc<str>> {
    CURRENT.try_with(|current| current.name.clone()).ok()
}

/// Counts a wait for the write connection, which `holder` held, against
/// the request or job running here.
pub(crate) fn waited_for_write(wait: Duration, holder: Option<Arc<str>>) {
    let _ = CURRENT.try_with(|current| {
        current.db_wait.set(current.db_wait.get() + wait);
        let mut held_by = current.held_by.borrow_mut();
        if let Some(holder) = holder
            && held_by.as_ref().is_none_or(|(longest, _)| wait > *longest)
        {
            *held_by = Some((wait, holder));
        }
    });
}

/// The requests and jobs in progress, each with how long it has run,
/// oldest first.
pub fn running() -> Vec<(Arc<str>, Duration)> {
    let mut running: Vec<_> = lock_running()
        .values()
        .map(|(name, since)| (name.clone(), since.elapsed()))
        .collect();
    running.sort_by_key(|(_, took)| std::cmp::Reverse(*took));
    running
}

/// Watches the runtime `handle` names on a thread of its own: every 250 ms
/// it starts a task there, and when the task has not run within `SLOW`,
/// it logs how long the runtime stalled and what was in progress. It ends
/// when the runtime shuts down.
pub fn watch_runtime(handle: tokio::runtime::Handle) {
    let spawned = std::thread::Builder::new()
        .name("pinrail-watchdog".to_string())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(250));
                if !check_runtime(&handle) {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("the runtime's watchdog did not start: {error}");
    }
}

/// Starts a task on the runtime and waits for it to run. Logs a stall,
/// and answers false once the runtime has shut down.
fn check_runtime(handle: &tokio::runtime::Handle) -> bool {
    use std::sync::mpsc::{RecvTimeoutError, channel};
    let (tx, rx) = channel();
    let started = Instant::now();
    handle.spawn(async move {
        let _ = tx.send(());
    });
    match rx.recv_timeout(SLOW) {
        Ok(()) => true,
        Err(RecvTimeoutError::Disconnected) => false,
        Err(RecvTimeoutError::Timeout) => {
            let running = running();
            let resumed = rx.recv().is_ok();
            let stalled = started.elapsed().as_millis();
            let running = if running.is_empty() {
                "nothing that the core tracks".to_string()
            } else {
                running
                    .iter()
                    .map(|(name, took)| format!("{name} ({} ms)", took.as_millis()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            tracing::warn!("the runtime ran no new task for {stalled} ms; in progress: {running}");
            resumed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_run_reports_its_wait_for_the_write_connection_and_the_longest_holder() {
        let (_, report) = run("GET /a", async {
            waited_for_write(Duration::from_millis(10), Some("POST /b".into()));
            waited_for_write(Duration::from_millis(30), Some("sweep".into()));
            waited_for_write(Duration::from_millis(20), Some("POST /c".into()));
        })
        .await;
        assert_eq!(report.db_wait, Duration::from_millis(60));
        assert_eq!(report.held_by.as_deref(), Some("sweep"));
    }

    #[tokio::test]
    async fn a_run_is_listed_as_in_progress_until_it_ends() {
        let (_, _) = run("GET /listed", async {
            assert!(running().iter().any(|(name, _)| &**name == "GET /listed"));
            assert_eq!(current().as_deref(), Some("GET /listed"));
        })
        .await;
        assert!(!running().iter().any(|(name, _)| &**name == "GET /listed"));
        assert_eq!(current(), None);
    }

    #[test]
    fn a_report_names_the_wait_and_the_holder() {
        let report = Report {
            took: Duration::from_millis(1200),
            db_wait: Duration::from_millis(1100),
            held_by: Some("sweep: presence".into()),
        };
        assert_eq!(
            report.describe(),
            "took 1200 ms, waited 1100 ms for the database's write connection, \
             held by sweep: presence"
        );
        let quick = Report {
            took: Duration::from_millis(700),
            db_wait: Duration::ZERO,
            held_by: None,
        };
        assert_eq!(quick.describe(), "took 700 ms");
    }

    #[test]
    fn a_stalled_runtime_is_logged_with_what_was_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.log");
        crate::log::init(&path);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        // the only worker is busy with a job that never yields
        runtime.spawn(async {
            run_sync("sweep: presence", || {
                std::thread::sleep(Duration::from_millis(900))
            });
        });
        std::thread::sleep(Duration::from_millis(50));
        assert!(check_runtime(runtime.handle()));
        let log = std::fs::read_to_string(&path).unwrap();
        assert!(log.contains("the runtime ran no new task for"), "{log}");
        assert!(log.contains("in progress: sweep: presence ("), "{log}");
    }
}
