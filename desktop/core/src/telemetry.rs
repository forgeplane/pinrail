//! Anonymous usage data: the events the app records, and the queue they
//! wait in until they are sent.
//!
//! Every event is a variant of [`Event`], whose fields are numbers,
//! booleans, names fixed in the code (`&'static str`) and the names of
//! plugins and agents ([`Name`]), so no title, path or other content can
//! be recorded by accident. A plugin's or an agent's name is sent as it
//! is when it looks like a name, short and without spaces, and as
//! `other` when it does not.
//!
//! Nothing is recorded until the person has seen the notice that says so,
//! while *Send usage data* is off, or while `DO_NOT_TRACK` or
//! `PINRAIL_TELEMETRY` is set. Turning usage data off deletes the install
//! id and the events that wait.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Value, json};

use crate::plugins::Catalog;
use crate::settings::SettingsReader;

mod crash;
mod send;
pub use crash::{Kind as CrashKind, Report as CrashReport, catch_panics, crashes_url, scrub};
pub use send::{Batch, Context, ENDPOINT, Sent, send};

/// The settings that turn usage data and crash reports on and off, and the
/// time the notice was shown.
pub const USAGE: &str = "/telemetry/usage";
pub const CRASHES: &str = "/telemetry/crashes";
pub const NOTICE_SHOWN: &str = "/telemetry/notice_shown";

/// The environment variables that turn telemetry off when set to any value.
const OFF_VARIABLES: [&str; 2] = ["DO_NOT_TRACK", "PINRAIL_TELEMETRY"];

/// The file in the data directory that holds the install id.
const ID_FILE: &str = "telemetry-id";

/// The most events that wait to be sent; the oldest go first.
const MAX_WAITING: usize = 500;
/// The sent events kept for *Show what is sent*.
const KEEP_SENT: usize = 100;

/// What a plugin's or an agent's name is sent as when it does not look
/// like one.
pub const OTHER: &str = "other";

/// The longest name of a plugin or an agent that is sent.
const MAX_NAME: usize = 40;

/// The name of a plugin or an agent, as an event sends it: the name
/// itself when it is at most 40 letters, digits, `-`, `_` and `.`, and
/// `other` otherwise. It is made only with [`Name::of`], so an event can
/// carry nothing else in its place.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Name(String);

impl Name {
    pub fn of(name: &str) -> Name {
        let looks_like_a_name = (1..=MAX_NAME).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        Name(if looks_like_a_name { name } else { OTHER }.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An event of the catalogue, with its fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "name", content = "fields", rename_all = "snake_case")]
pub enum Event {
    /// The app started.
    AppStarted { first_run: bool },
    /// A review was decided.
    ReviewDecided {
        plugin: Name,
        outcome: &'static str,
        rounds: u32,
        seconds_waiting: u64,
    },
    /// A set of questions was answered.
    QuestionsAnswered {
        questions: u32,
        unanswered: u32,
        suggested_reply_used: bool,
    },
    /// A review was discarded.
    ReviewDiscarded { plugin: Name },
    /// A session reported its first status.
    SessionStarted { agent: Name },
    /// A work item was added, claimed, done, held or queued.
    ItemChanged {
        change: &'static str,
        by_agent: bool,
    },
    /// A plugin was installed or removed.
    PluginInstalled {
        plugin: Name,
        official: bool,
        removed: bool,
    },
    /// A feature was used; recorded once a day for each feature.
    FeatureUsed { feature: &'static str },
    /// The CLI called the core; recorded once a day for each command.
    CliUsed { command: &'static str, agent: Name },
}

impl Event {
    /// What makes the event count once a day, for the events that do.
    fn daily_key(&self) -> Option<String> {
        match self {
            Event::FeatureUsed { feature } => Some(format!("feature:{feature}")),
            Event::CliUsed { command, agent } => Some(format!("cli:{command}:{agent}")),
            _ => None,
        }
    }
}

/// The features `feature_used` counts that the app's window reports, with
/// `POST /api/v1/telemetry/features`.
pub const WINDOW_FEATURES: &[&str] = &[
    "command_palette",
    "find_in_thread",
    "list_filter",
    "item_picker",
    "quick_add",
    "page_inbox",
    "page_sessions",
    "page_items",
    "page_projects",
    "page_history",
];

/// The features `feature_used` counts that the core sees happen itself.
pub const ROUND_STARRED: &str = "round_starred";
pub const SESSION_MESSAGE: &str = "session_message";
pub const CUSTOM_THEME: &str = "custom_theme";
pub const NOTIFICATIONS_PAUSED: &str = "notifications_paused";

/// The window's feature `name` names, when it is one.
pub fn window_feature(name: &str) -> Option<&'static str> {
    WINDOW_FEATURES.iter().copied().find(|f| *f == name)
}

/// The commands of the `pinrail` CLI that `cli_used` names; any other is
/// `other`.
const CLI_COMMANDS: &[&str] = &[
    "ask",
    "status",
    "submit",
    "items",
    "wait",
    "show",
    "rounds",
    "events",
    "list",
    "decide",
    "withdraw",
    "discard",
    "plugins",
    "attachments",
    "export",
    "serve",
    "docs",
    "open",
    "telemetry",
];

/// The CLI's command `name` names, or `other`.
pub fn cli_command(name: &str) -> &'static str {
    CLI_COMMANDS
        .iter()
        .copied()
        .find(|c| *c == name)
        .unwrap_or(OTHER)
}

/// What a decision amounts to, from the summary its plugin derives:
/// `changes` when any count is a warning or a danger, such as rejected
/// items or changes requested; `approved` otherwise; `unknown` when the
/// plugin derives no summary.
pub fn outcome_of(summary: Option<&Value>) -> &'static str {
    let Some(counts) = summary.and_then(|s| s["counts"].as_array()) else {
        return "unknown";
    };
    let against = counts
        .iter()
        .any(|c| matches!(c["tone"].as_str(), Some("warning" | "danger")));
    if against { "changes" } else { "approved" }
}

/// Whether `name` is one of the official plugins the app carries, for the
/// `official` field of `plugin_installed`.
pub fn is_official(name: &str) -> bool {
    static OFFICIAL: OnceLock<Vec<String>> = OnceLock::new();
    OFFICIAL
        .get_or_init(|| {
            Catalog::builtin()
                .entries()
                .iter()
                .map(|e| e.name.clone())
                .collect()
        })
        .iter()
        .any(|official| official == name)
}

/// An event as it waits and is sent: when it happened, with its fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recorded {
    pub at: DateTime<Utc>,
    #[serde(flatten)]
    pub event: Event,
}

/// Records events while telemetry is on, and holds them until they are
/// sent.
#[derive(Debug)]
pub struct Telemetry {
    data_dir: PathBuf,
    settings: SettingsReader,
    /// the environment variable that turns telemetry off, when one is set
    off_by: Option<&'static str>,
    /// this launch's id, which groups its events
    session: String,
    /// whether crash reports are on, kept where a panic can read it
    /// without a lock, since it may have struck while one was held
    crashes: std::sync::atomic::AtomicBool,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    waiting: VecDeque<Recorded>,
    /// a batch whose send failed, tried once more before anything else
    retry: Option<Vec<Recorded>>,
    /// when a batch was last sent, or telemetry opened
    last_sent: Option<std::time::Instant>,
    sent: VecDeque<Recorded>,
    /// the day the once-a-day events were last counted, and which were
    day: Option<NaiveDate>,
    counted: HashSet<String>,
}

impl Telemetry {
    /// Telemetry for the data directory, reading the off switches from the
    /// process's environment.
    pub(crate) fn open(data_dir: &Path, settings: SettingsReader) -> Self {
        Self::open_with(data_dir, settings, |key| std::env::var_os(key).is_some())
    }

    /// [`Telemetry::open`] with `is_set` telling which environment
    /// variables are set.
    pub(crate) fn open_with(
        data_dir: &Path,
        settings: SettingsReader,
        is_set: impl Fn(&str) -> bool,
    ) -> Self {
        let telemetry = Telemetry {
            data_dir: data_dir.to_path_buf(),
            settings,
            off_by: OFF_VARIABLES.into_iter().find(|key| is_set(key)),
            session: random_id(),
            crashes: std::sync::atomic::AtomicBool::new(false),
            state: Mutex::new(State {
                last_sent: Some(std::time::Instant::now()),
                ..State::default()
            }),
        };
        // an id left from a run that had usage data on, or a switch turned
        // off in settings.json while the app was closed
        telemetry.sync();
        telemetry
    }

    /// Whether usage data is recorded now: the notice was shown, the
    /// setting is on and no environment variable turns it off.
    pub fn usage_on(&self) -> bool {
        self.allowed(USAGE)
    }

    /// Whether crash reports are kept and sent now, by the same rules.
    pub fn crashes_on(&self) -> bool {
        self.allowed(CRASHES)
    }

    fn allowed(&self, switch: &str) -> bool {
        self.off_by.is_none()
            && !self.settings.value(NOTICE_SHOWN).is_null()
            && self.settings.value(switch) == Value::Bool(true)
    }

    /// Records `event` while usage data is on, and drops it otherwise. An
    /// event that counts once a day is dropped when it was already
    /// recorded today.
    pub fn record(&self, event: Event) {
        self.record_at(event, Utc::now());
    }

    fn record_at(&self, event: Event, at: DateTime<Utc>) {
        if !self.usage_on() {
            return;
        }
        if self.install_id().is_none() {
            return;
        }
        let mut state = self.lock();
        if let Some(key) = event.daily_key() {
            let today = at.date_naive();
            if state.day != Some(today) {
                state.day = Some(today);
                state.counted.clear();
            }
            if !state.counted.insert(key) {
                return;
            }
        }
        state.waiting.push_back(Recorded { at, event });
        while state.waiting.len() > MAX_WAITING {
            state.waiting.pop_front();
        }
    }

    /// The batch to send at `now`, if one is due: the batch whose last send
    /// failed, or the waiting events once there are [`send::BATCH`] of
    /// them or [`send::EVERY`] has passed since the last send. `None`
    /// while usage data is off or nothing waits.
    pub fn due(&self, now: std::time::Instant) -> Option<Batch> {
        let install = self.install_id()?;
        let mut state = self.lock();
        let (events, retry) = match state.retry.take() {
            Some(events) => (events, true),
            None => {
                let since = state.last_sent.map_or(send::EVERY, |last| now - last);
                let full = state.waiting.len() >= send::BATCH;
                if state.waiting.is_empty() || !(full || since >= send::EVERY) {
                    return None;
                }
                let n = send::BATCH.min(state.waiting.len());
                (state.waiting.drain(..n).collect(), false)
            }
        };
        Some(Batch {
            install,
            session: self.session.clone(),
            context: Context::of_this_system(),
            events,
            retry,
        })
    }

    /// Records how sending `batch` went at `now`: sent events are kept for
    /// *Show what is sent*; a batch the endpoint refused is dropped, since
    /// it would be refused again; a batch that failed is tried once more at
    /// the next send, then dropped.
    pub fn settle(&self, batch: Batch, sent: Sent, now: std::time::Instant) {
        match sent {
            Sent::Stored => {
                self.lock().last_sent = Some(now);
                self.sent(batch.events);
            }
            Sent::Refused => self.lock().last_sent = Some(now),
            Sent::Failed if !batch.retry && self.usage_on() => {
                self.lock().retry = Some(batch.events);
            }
            Sent::Failed => self.lock().last_sent = Some(now),
        }
    }

    /// Takes up to `max` of the waiting events, oldest first, to send.
    pub fn take(&self, max: usize) -> Vec<Recorded> {
        let mut state = self.lock();
        let n = max.min(state.waiting.len());
        state.waiting.drain(..n).collect()
    }

    /// Keeps `events`, just sent, for *Show what is sent*.
    pub fn sent(&self, events: Vec<Recorded>) {
        let mut state = self.lock();
        state.sent.extend(events);
        while state.sent.len() > KEEP_SENT {
            state.sent.pop_front();
        }
    }

    /// Where telemetry stands, with the events that wait and the last ones
    /// sent, for Settings and `pinrail telemetry status`.
    pub fn status(&self) -> Value {
        self.sync();
        let state = self.lock();
        json!({
            "usage": self.usage_on(),
            "crashes": self.crashes_on(),
            "off_by": self.off_by,
            "notice_shown": self.settings.value(NOTICE_SHOWN),
            "install": self.read_id(),
            "session": self.session,
            "waiting": state.retry.iter().flatten().chain(&state.waiting).collect::<Vec<_>>(),
            "sent": state.sent,
            "crash": self.waiting_crash(),
        })
    }

    /// This launch's session id.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// The install id, made when usage data is on and there is none yet;
    /// `None` while it is off.
    pub fn install_id(&self) -> Option<String> {
        if !self.usage_on() {
            return None;
        }
        if let Some(id) = self.read_id() {
            return Some(id);
        }
        let id = random_id();
        std::fs::write(self.id_path(), &id).ok()?;
        Some(id)
    }

    /// Brings the stored state in line with the switches: while usage data
    /// is off, the install id is deleted and nothing waits. Called after
    /// every change of the settings.
    pub(crate) fn sync(&self) {
        let crashes = self.crashes_on();
        self.crashes
            .store(crashes, std::sync::atomic::Ordering::Relaxed);
        if !crashes {
            crash::forget(&self.data_dir);
        }
        if self.usage_on() {
            return;
        }
        let _ = std::fs::remove_file(self.id_path());
        let mut state = self.lock();
        state.waiting.clear();
        state.retry = None;
        state.counted.clear();
    }

    /// Whether crash reports are on, as last brought in line with the
    /// settings, read without a lock.
    fn crashes_allowed(&self) -> bool {
        self.crashes.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn read_id(&self) -> Option<String> {
        let id = std::fs::read_to_string(self.id_path()).ok()?;
        let id = id.trim();
        (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| id.to_string())
    }

    fn id_path(&self) -> PathBuf {
        self.data_dir.join(ID_FILE)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// 16 random bytes as 32 hex digits.
fn random_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the operating system's random numbers");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Pinrail};
    use chrono::TimeZone;

    fn app() -> (tempfile::TempDir, Pinrail) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::new(dir.path(), 0);
        config.telemetry_env = false;
        let app = Pinrail::open(config).unwrap();
        (dir, app)
    }

    fn show_notice(app: &Pinrail) {
        app.settings()
            .change(&json!({"telemetry": {"notice_shown": "2026-11-10T09:00:00Z"}}))
            .unwrap();
        // the first launch, counted with the notice
        let started = app.telemetry().take(MAX_WAITING);
        assert_eq!(
            started.iter().map(|r| &r.event).collect::<Vec<_>>(),
            [&Event::AppStarted { first_run: true }]
        );
    }

    fn waiting(t: &Telemetry) -> Vec<Value> {
        t.status()["waiting"].as_array().unwrap().clone()
    }

    fn discarded() -> Event {
        Event::ReviewDiscarded {
            plugin: Name::of("list"),
        }
    }

    #[test]
    fn nothing_is_recorded_before_the_notice_was_shown() {
        let (dir, app) = app();
        let t = app.telemetry();
        t.record(discarded());
        assert!(waiting(t).is_empty());
        assert_eq!(t.status()["usage"], false);
        assert!(!dir.path().join(ID_FILE).exists());

        show_notice(&app);
        t.record(discarded());
        assert_eq!(waiting(t).len(), 1);
        let status = t.status();
        assert_eq!(status["usage"], true);
        assert_eq!(status["crashes"], true);
        assert_eq!(status["off_by"], Value::Null);
        let id = status["install"].as_str().unwrap().to_string();
        assert_eq!(id.len(), 32);
        assert_ne!(id, status["session"]);
        // the same id while it stays on
        t.record(discarded());
        assert_eq!(t.status()["install"], id);
    }

    #[test]
    fn each_event_serializes_to_its_listed_fields_only() {
        let at = Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap();
        let cases = [
            (
                Event::AppStarted { first_run: true },
                json!({"name": "app_started", "fields": {"first_run": true}}),
            ),
            (
                Event::ReviewDecided {
                    plugin: Name::of("list"),
                    outcome: "approved",
                    rounds: 2,
                    seconds_waiting: 340,
                },
                json!({"name": "review_decided", "fields": {"plugin": "list", "outcome": "approved", "rounds": 2, "seconds_waiting": 340}}),
            ),
            (
                Event::QuestionsAnswered {
                    questions: 3,
                    unanswered: 1,
                    suggested_reply_used: false,
                },
                json!({"name": "questions_answered", "fields": {"questions": 3, "unanswered": 1, "suggested_reply_used": false}}),
            ),
            (
                discarded(),
                json!({"name": "review_discarded", "fields": {"plugin": "list"}}),
            ),
            (
                Event::SessionStarted {
                    agent: Name::of("codex"),
                },
                json!({"name": "session_started", "fields": {"agent": "codex"}}),
            ),
            (
                Event::ItemChanged {
                    change: "claimed",
                    by_agent: true,
                },
                json!({"name": "item_changed", "fields": {"change": "claimed", "by_agent": true}}),
            ),
            (
                Event::PluginInstalled {
                    plugin: Name::of("acme-review"),
                    official: false,
                    removed: true,
                },
                json!({"name": "plugin_installed", "fields": {"plugin": "acme-review", "official": false, "removed": true}}),
            ),
            (
                Event::FeatureUsed { feature: "search" },
                json!({"name": "feature_used", "fields": {"feature": "search"}}),
            ),
            (
                Event::CliUsed {
                    command: "ask",
                    agent: Name::of("claude-code"),
                },
                json!({"name": "cli_used", "fields": {"command": "ask", "agent": "claude-code"}}),
            ),
        ];
        for (event, mut expected) in cases {
            expected["at"] = json!("2026-11-10T09:00:00Z");
            assert_eq!(
                serde_json::to_value(Recorded { at, event }).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn a_name_is_sent_as_it_is_only_when_it_looks_like_one() {
        for name in ["list", "claude-code", "my_bot.v2", &"a".repeat(MAX_NAME)] {
            assert_eq!(Name::of(name).as_str(), name);
        }
        for name in [
            "",
            "my bot",
            "/Users/me/plugins/review",
            "ünïcode",
            "a\nb",
            &"a".repeat(MAX_NAME + 1),
        ] {
            assert_eq!(Name::of(name).as_str(), OTHER, "{name:?}");
        }
    }

    #[test]
    fn official_plugins_are_the_ones_the_app_carries() {
        assert!(is_official("list"));
        assert!(is_official("feedback"));
        assert!(!is_official("acme-review"));
    }

    #[test]
    fn an_environment_variable_stops_recording_whatever_the_settings_say() {
        for variable in OFF_VARIABLES {
            let (dir, app) = app();
            show_notice(&app);
            let t =
                Telemetry::open_with(dir.path(), app.settings().reader(), |key| key == variable);
            t.record(discarded());
            let status = t.status();
            assert!(waiting(&t).is_empty(), "{variable}");
            assert_eq!(status["usage"], false);
            assert_eq!(status["crashes"], false);
            assert_eq!(status["off_by"], variable);
            assert_eq!(status["install"], Value::Null);
        }
    }

    #[test]
    fn turning_usage_data_off_deletes_the_install_id_and_what_waits() {
        let (dir, app) = app();
        show_notice(&app);
        let t = app.telemetry();
        t.record(discarded());
        let first = t.status()["install"].as_str().unwrap().to_string();

        app.settings()
            .change(&json!({"telemetry": {"usage": false}}))
            .unwrap();
        assert!(!dir.path().join(ID_FILE).exists());
        assert!(waiting(t).is_empty());
        t.record(discarded());
        assert!(waiting(t).is_empty());
        // crash reports have their own switch
        assert_eq!(t.status()["crashes"], true);

        // on again: a new id
        app.settings()
            .change(&json!({"telemetry": {"usage": true}}))
            .unwrap();
        t.record(discarded());
        let second = t.status()["install"].as_str().unwrap().to_string();
        assert_ne!(first, second);
    }

    #[test]
    fn an_id_left_by_a_run_that_had_it_on_is_deleted_at_start() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ID_FILE), "0".repeat(32)).unwrap();
        let _app = Pinrail::open(Config::new(dir.path(), 0)).unwrap();
        assert!(!dir.path().join(ID_FILE).exists());
    }

    #[test]
    fn the_queue_drops_the_oldest_past_its_limit() {
        let (_dir, app) = app();
        show_notice(&app);
        let t = app.telemetry();
        for rounds in 0..(MAX_WAITING as u32 + 20) {
            t.record(Event::ReviewDecided {
                plugin: Name::of("list"),
                outcome: "approved",
                rounds,
                seconds_waiting: 1,
            });
        }
        let waiting = waiting(t);
        assert_eq!(waiting.len(), MAX_WAITING);
        assert_eq!(waiting[0]["fields"]["rounds"], 20);

        // taken to be sent, then kept, the last hundred, for Show what is sent
        let batch = t.take(MAX_WAITING);
        assert_eq!(batch.len(), MAX_WAITING);
        t.sent(batch);
        let status = t.status();
        assert!(status["waiting"].as_array().unwrap().is_empty());
        let sent = status["sent"].as_array().unwrap();
        assert_eq!(sent.len(), KEEP_SENT);
        assert_eq!(sent[KEEP_SENT - 1]["fields"]["rounds"], MAX_WAITING + 19);
    }

    /// `n` events recorded, at once.
    fn record(t: &Telemetry, n: u32) {
        for rounds in 0..n {
            t.record(Event::ReviewDecided {
                plugin: Name::of("list"),
                outcome: "approved",
                rounds,
                seconds_waiting: 1,
            });
        }
    }

    #[test]
    fn a_batch_is_due_at_fifty_events_or_after_five_minutes() {
        use std::time::{Duration, Instant};
        let (_dir, app) = app();
        show_notice(&app);
        let t = app.telemetry();
        let start = Instant::now();
        assert_eq!(t.due(start + send::EVERY), None);

        record(t, 3);
        assert_eq!(t.due(start), None);
        let batch = t.due(start + send::EVERY).unwrap();
        assert_eq!(batch.events.len(), 3);
        assert_eq!(batch.install, t.install_id().unwrap());
        assert_eq!(batch.session, t.session());
        assert!(!batch.retry);
        t.settle(batch, Sent::Stored, start + send::EVERY);
        assert_eq!(t.status()["sent"].as_array().unwrap().len(), 3);

        // a full batch goes at once, the rest after five minutes more
        let later = start + send::EVERY + Duration::from_secs(1);
        record(t, 60);
        let batch = t.due(later).unwrap();
        assert_eq!(batch.events.len(), send::BATCH);
        t.settle(batch, Sent::Stored, later);
        assert_eq!(t.due(later), None);
        assert_eq!(t.due(later + send::EVERY).unwrap().events.len(), 10);
    }

    #[test]
    fn a_failed_batch_is_tried_once_more_and_a_refused_one_is_dropped() {
        use std::time::Instant;
        let (_dir, app) = app();
        show_notice(&app);
        let t = app.telemetry();
        let now = Instant::now() + send::EVERY;
        record(t, 2);

        let first = t.due(now).unwrap();
        t.settle(first.clone(), Sent::Failed, now);
        // it still waits, for Show what is sent
        assert_eq!(t.status()["waiting"].as_array().unwrap().len(), 2);
        let again = t.due(now).unwrap();
        assert!(again.retry);
        assert_eq!(again.events, first.events);
        t.settle(again, Sent::Failed, now);
        assert_eq!(t.due(now + send::EVERY), None);
        assert!(t.status()["waiting"].as_array().unwrap().is_empty());

        record(t, 1);
        let refused = t.due(now + send::EVERY).unwrap();
        t.settle(refused, Sent::Refused, now + send::EVERY);
        assert_eq!(t.due(now + send::EVERY * 3), None);
        assert!(t.status()["sent"].as_array().unwrap().is_empty());
    }

    #[test]
    fn nothing_is_due_while_usage_data_is_off() {
        use std::time::Instant;
        let (_dir, app) = app();
        show_notice(&app);
        let t = app.telemetry();
        record(t, 60);
        let failed = t.due(Instant::now()).unwrap();
        t.settle(failed, Sent::Failed, Instant::now());
        app.settings()
            .change(&json!({"telemetry": {"usage": false}}))
            .unwrap();
        assert_eq!(t.due(Instant::now() + send::EVERY), None);
        assert!(t.status()["waiting"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_daily_event_counts_once_a_day() {
        let (_dir, app) = app();
        show_notice(&app);
        let t = app.telemetry();
        let day = |d| Utc.with_ymd_and_hms(2026, 11, d, 9, 0, 0).unwrap();
        let search = Event::FeatureUsed { feature: "search" };
        t.record_at(search.clone(), day(10));
        t.record_at(search.clone(), day(10));
        t.record_at(Event::FeatureUsed { feature: "palette" }, day(10));
        t.record_at(discarded(), day(10));
        t.record_at(discarded(), day(10));
        t.record_at(search, day(11));
        assert_eq!(waiting(t).len(), 5);
    }
}
