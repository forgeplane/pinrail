//! A panic, once the server catches them, is kept as a scrubbed crash
//! report to send at the next start. In a test binary of its own, since
//! the hook is the whole process's.

use std::sync::Arc;

use pinrail_core::telemetry::{CrashKind, catch_panics};
use pinrail_core::{Config, Pinrail};
use serde_json::json;

#[test]
fn a_panic_is_kept_as_a_scrubbed_report() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.telemetry_env = false;
    let app = Arc::new(Pinrail::open(config).unwrap());
    app.settings()
        .change(&json!({"telemetry": {"notice_shown": "2026-11-10T09:00:00Z"}}))
        .unwrap();
    catch_panics(app.telemetry().clone());

    let caught = std::panic::catch_unwind(|| {
        panic!("could not read \"Acme roadmap.md\" in /Users/ana/acme/docs");
    });
    assert!(caught.is_err());

    let report = app.telemetry().waiting_crash().unwrap();
    assert_eq!(report.kind, CrashKind::Core);
    assert!(
        report
            .message
            .starts_with("could not read \"…\" in docs at crashes.rs:"),
        "{}",
        report.message
    );
    assert!(!report.stack.contains("/Users/"), "{}", report.stack);
}
