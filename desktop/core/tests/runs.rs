//! The record of the agents Pinrail starts: a run is written before its
//! spawn, starts and ends once, moves between sessions, and its token
//! counts until 5 minutes after it ended.

use chrono::{Duration, Utc};
use pinrail_core::db::{Db, NewRun, RunFilters, RunRow};
use serde_json::Map;

fn new_run<'a>(id: &'a str, session: Option<&'a str>, digest: &'a str) -> NewRun<'a> {
    NewRun {
        id,
        host: "local",
        agent: "claude-code",
        kind: "start",
        folder: "/work/app",
        argv: &[],
        created_by: "app",
        token_digest: digest,
        session,
    }
}

fn session(row: &RunRow) -> Option<(&str, &str)> {
    row.session
        .as_ref()
        .map(|(agent, id)| (agent.as_str(), id.as_str()))
}

#[test]
fn a_run_is_starting_until_spawned_and_ends_once() {
    let db = Db::in_memory().unwrap();
    let now = Utc::now();
    let run = db
        .insert_run(&new_run("run_1", Some("s-1"), "d1"), now)
        .unwrap();
    assert_eq!(run.state, "starting");
    assert_eq!(run.terminal, None);
    assert_eq!(session(&run), Some(("claude-code", "s-1")));

    let (run, (_, event)) = db
        .run_spawned("run_1", "t_1", Some((42, "1700000000.5".into())))
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "running");
    assert_eq!(run.process, Some((42, "1700000000.5".into())));
    assert_eq!(event["run"]["terminal"], "local:t_1");
    // a second report of the spawn changes nothing
    assert!(db.run_spawned("run_1", "t_2", None).unwrap().is_none());

    let (run, (_, event)) = db
        .end_run("run_1", "exited", Some(3), None, now)
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "ended");
    assert_eq!(run.end_reason.as_deref(), Some("exited"));
    assert_eq!(event["exit_code"], 3);
    assert!(
        db.end_run("run_1", "host_lost", None, None, now)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.run("run_1").unwrap().unwrap().end_reason.as_deref(),
        Some("exited")
    );
}

#[test]
fn a_run_moves_to_a_new_session_and_keeps_its_history() {
    let db = Db::in_memory().unwrap();
    let now = Utc::now();
    // a Codex run learns its session at the first turn
    db.insert_run(&new_run("run_1", None, "d1"), now).unwrap();
    let bound = db
        .bind_run("run_1", "s-1", &Map::new(), now)
        .unwrap()
        .unwrap();
    let event = bound.event.1;
    assert_eq!(bound.previous, None);
    assert_eq!(event["session"]["id"], "s-1");
    // the same session again is no change
    assert!(
        db.bind_run("run_1", "s-1", &Map::new(), now)
            .unwrap()
            .is_none()
    );
    // /clear moves it to a new session
    let later = now + Duration::seconds(1);
    let bound = db
        .bind_run("run_1", "s-2", &Map::new(), later)
        .unwrap()
        .unwrap();
    let event = bound.event.1;
    assert_eq!(bound.previous, Some(("claude-code".into(), "s-1".into())));
    assert_eq!(event["previous"]["id"], "s-1");
    let run = db.run("run_1").unwrap().unwrap();
    assert_eq!(session(&run), Some(("claude-code", "s-2")));
    let history = db.run_sessions("run_1").unwrap();
    assert_eq!(
        history
            .iter()
            .map(|s| (s.id.as_str(), s.unbound_at.is_some()))
            .collect::<Vec<_>>(),
        [("s-1", true), ("s-2", false)]
    );

    // the runs of a session include those that left it
    let filters = RunFilters {
        session: Some(("claude-code".into(), "s-1".into())),
        ..RunFilters::default()
    };
    assert_eq!(db.runs(&filters).unwrap().len(), 1);
    // an ended run binds no more
    db.end_run("run_1", "stopped", None, None, later).unwrap();
    assert!(
        db.bind_run("run_1", "s-3", &Map::new(), later)
            .unwrap()
            .is_none()
    );
}

#[test]
fn runs_are_listed_by_state_latest_first() {
    let db = Db::in_memory().unwrap();
    let now = Utc::now();
    db.insert_run(&new_run("run_1", None, "d1"), now).unwrap();
    db.insert_run(&new_run("run_2", None, "d2"), now + Duration::seconds(1))
        .unwrap();
    db.end_run("run_1", "exited", Some(0), None, now).unwrap();
    let ids = |live| {
        db.runs(&RunFilters {
            live,
            ..RunFilters::default()
        })
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect::<Vec<_>>()
    };
    assert_eq!(ids(None), ["run_2", "run_1"]);
    assert_eq!(ids(Some(true)), ["run_2"]);
    assert_eq!(ids(Some(false)), ["run_1"]);
}

#[test]
fn a_token_counts_until_five_minutes_after_its_run_ended() {
    let db = Db::in_memory().unwrap();
    let now = Utc::now();
    db.insert_run(&new_run("run_1", None, "d1"), now).unwrap();
    assert_eq!(db.run_for_token("d1", now).unwrap().unwrap().id, "run_1");
    assert!(db.run_for_token("other", now).unwrap().is_none());

    db.end_run("run_1", "exited", Some(0), None, now).unwrap();
    let soon = now + Duration::minutes(4);
    let late = now + Duration::minutes(6);
    assert!(db.run_for_token("d1", soon).unwrap().is_some());
    assert!(db.run_for_token("d1", late).unwrap().is_none());

    assert_eq!(db.forget_run_tokens(soon).unwrap(), 0);
    assert_eq!(db.forget_run_tokens(late).unwrap(), 1);
}
