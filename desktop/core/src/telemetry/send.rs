//! Sending the waiting events to Pinrail's endpoint: in batches of at most
//! 50, every 5 minutes or as soon as 50 wait, with a short timeout. A send
//! never blocks the app, and quitting never waits for one.

use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;

use super::Recorded;

/// Where the app sends its usage data.
pub const ENDPOINT: &str = "https://api.pinrail.dev/v1/events";
/// The most events in one batch, as the endpoint takes them.
pub const BATCH: usize = 50;
/// How long the waiting events wait, unless there are a batch of them.
pub const EVERY: Duration = Duration::from_secs(5 * 60);
/// The longest a send may take.
const TIMEOUT: Duration = Duration::from_secs(5);

/// A batch of events as the endpoint takes it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Batch {
    pub install: String,
    pub session: String,
    pub context: Context,
    pub events: Vec<Recorded>,
    /// whether the batch is being sent again after a failed send
    #[serde(skip)]
    pub retry: bool,
}

/// What every batch says about the app and the system it runs on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Context {
    pub version: String,
    /// `macos`, `linux` or `windows`
    pub os: String,
    pub os_version: String,
    /// `aarch64` or `x86_64`
    pub arch: String,
    /// `stable`, `prerelease`, or `dev` for a build that is not a release
    pub channel: String,
    /// the CI service the CLI runs in, if any
    pub ci: Option<String>,
}

impl Context {
    /// The context of this app on this system.
    pub fn of_this_system() -> Context {
        let version = crate::VERSION;
        Context {
            version: version.to_string(),
            os: std::env::consts::OS.to_string(),
            os_version: os_version().to_string(),
            arch: std::env::consts::ARCH.to_string(),
            channel: if cfg!(debug_assertions) {
                "dev"
            } else if version.contains('-') {
                "prerelease"
            } else {
                "stable"
            }
            .to_string(),
            ci: None,
        }
    }
}

/// The system's version, such as `15.4` on macOS or `24.04` on Ubuntu, or
/// `unknown`; read once.
fn os_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let found = if cfg!(target_os = "macos") {
            std::process::Command::new("sw_vers")
                .arg("-productVersion")
                .output()
                .ok()
                .and_then(|out| String::from_utf8(out.stdout).ok())
        } else {
            std::fs::read_to_string("/etc/os-release")
                .ok()
                .and_then(|text| {
                    text.lines()
                        .find_map(|line| line.strip_prefix("VERSION_ID="))
                        .map(|v| v.trim_matches('"').to_string())
                })
        };
        found
            .map(|v| v.trim().to_string())
            .filter(|v| {
                (1..=40).contains(&v.len())
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
            })
            .unwrap_or_else(|| "unknown".to_string())
    })
}

/// How a send went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// the endpoint stored the batch
    Stored,
    /// the endpoint refused it, as it would refuse it again
    Refused,
    /// it did not arrive, or the endpoint could not take it now
    Failed,
}

/// Sends `batch` to `url`, waiting at most 5 seconds. With `debug`, it
/// prints the batch to stderr instead, as `PINRAIL_TELEMETRY_DEBUG` asks.
/// Blocks: call it off the async runtime.
pub fn send(url: &str, debug: bool, batch: &Batch) -> Sent {
    post(url, debug, batch)
}

/// Posts `body` as JSON to `url`, or prints it with `debug`.
pub(super) fn post(url: &str, debug: bool, body: &impl Serialize) -> Sent {
    let body = match serde_json::to_string(body) {
        Ok(body) => body,
        Err(_) => return Sent::Refused,
    };
    if debug {
        eprintln!("pinrail: telemetry, not sent to {url}: {body}");
        return Sent::Stored;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .post(url)
        .header("content-type", "application/json")
        .header("x-pinrail-client", format!("pinrail/{}", crate::VERSION))
        .send(body.as_str());
    match response.map(|r| r.status().as_u16()) {
        Ok(200..=299) => Sent::Stored,
        Ok(400 | 413) => Sent::Refused,
        _ => Sent::Failed,
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::telemetry::{Event, Name};
    use axum::Router;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use std::sync::{Arc, Mutex};

    fn batch() -> Batch {
        Batch {
            install: "0".repeat(32),
            session: "1".repeat(32),
            context: Context::of_this_system(),
            events: vec![Recorded {
                at: "2026-11-10T09:00:00Z".parse().unwrap(),
                event: Event::ReviewDiscarded {
                    plugin: Name::of("list"),
                },
            }],
            retry: false,
        }
    }

    /// What a test endpoint was sent: each request's headers and body.
    pub(in crate::telemetry) type Got = Arc<Mutex<Vec<(HeaderMap, String)>>>;

    /// A server on a free port that answers `status` and keeps what it was
    /// sent; its URL.
    fn endpoint(status: StatusCode, got: Got) -> String {
        endpoint_at("/v1/events", status, got)
    }

    /// [`endpoint`] at `path`.
    pub(in crate::telemetry) fn endpoint_at(
        path: &'static str,
        status: StatusCode,
        got: Got,
    ) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}{path}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async move {
                    let app = Router::new().route(
                        path,
                        post(move |headers: HeaderMap, body: String| async move {
                            got.lock().unwrap().push((headers, body));
                            status
                        }),
                    );
                    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                    axum::serve(listener, app).await.unwrap();
                })
        });
        url
    }

    #[test]
    fn a_batch_is_sent_in_the_shape_the_endpoint_checks() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let url = endpoint(StatusCode::NO_CONTENT, got.clone());
        assert_eq!(send(&url, false, &batch()), Sent::Stored);

        let got = got.lock().unwrap();
        let (headers, body) = &got[0];
        assert_eq!(
            headers["x-pinrail-client"],
            format!("pinrail/{}", crate::VERSION).as_str()
        );
        assert_eq!(headers["content-type"], "application/json");
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        let context = &body["context"];
        assert_eq!(
            body,
            serde_json::json!({
                "install": "0".repeat(32),
                "session": "1".repeat(32),
                "context": {
                    "version": crate::VERSION,
                    "os": std::env::consts::OS,
                    "os_version": context["os_version"],
                    "arch": std::env::consts::ARCH,
                    "channel": "dev",
                    "ci": null
                },
                "events": [{"name": "review_discarded", "at": "2026-11-10T09:00:00Z", "fields": {"plugin": "list"}}]
            })
        );
        // as the endpoint's rules take them
        let os_version = context["os_version"].as_str().unwrap();
        assert!((1..=40).contains(&os_version.len()), "{os_version}");
        for system in ["os", "arch", "channel"] {
            let value = context[system].as_str().unwrap();
            assert!(
                value
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{system}: {value}"
            );
        }
    }

    #[test]
    fn a_refusal_is_told_apart_from_a_failure() {
        for (status, sent) in [
            (StatusCode::BAD_REQUEST, Sent::Refused),
            (StatusCode::PAYLOAD_TOO_LARGE, Sent::Refused),
            (StatusCode::TOO_MANY_REQUESTS, Sent::Failed),
            (StatusCode::SERVICE_UNAVAILABLE, Sent::Failed),
        ] {
            let url = endpoint(status, Arc::new(Mutex::new(Vec::new())));
            assert_eq!(send(&url, false, &batch()), sent, "{status}");
        }
        // nothing listening
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("http://127.0.0.1:{port}/v1/events");
        assert_eq!(send(&url, false, &batch()), Sent::Failed);
    }

    /// The deployed endpoint stores a batch the app makes. It sends one
    /// test event to the real endpoint, so it runs only when asked:
    /// `cargo test -p pinrail-core -- --ignored deployed`.
    #[test]
    #[ignore]
    fn the_deployed_endpoint_stores_a_batch() {
        let mut batch = batch();
        batch.events[0].at = chrono::Utc::now();
        assert_eq!(send(ENDPOINT, false, &batch), Sent::Stored);
    }

    #[test]
    fn debug_prints_the_batch_and_sends_nothing() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let url = endpoint(StatusCode::NO_CONTENT, got.clone());
        assert_eq!(send(&url, true, &batch()), Sent::Stored);
        assert!(got.lock().unwrap().is_empty());
    }
}
