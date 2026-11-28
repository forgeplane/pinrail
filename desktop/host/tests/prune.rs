//! A host that starts forgets the terminals earlier hosts ended more than a
//! day ago, with their final screens, as it forgets its own: the host
//! exits when idle, so a record it never held would otherwise stay.

mod common;

use std::fs;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use common::{folders, serve};

fn ended(id: &str, hours_ago: i64) -> Value {
    let at = chrono::Utc::now() - chrono::Duration::hours(hours_ago);
    json!({
        "id": id,
        "argv0": "sh",
        "cwd": "/",
        "cols": 80,
        "rows": 24,
        "labels": {},
        "started_at": (at - chrono::Duration::minutes(5)).to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        "ended_at": at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        "exit": {"code": 0},
    })
}

fn ids(path: &std::path::Path) -> Vec<String> {
    let records: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut ids: Vec<String> = records
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

#[test]
fn a_starting_host_forgets_terminals_earlier_hosts_ended_a_day_ago() {
    let folders = folders();
    let host_dir = folders.data.join("host");
    let screens = host_dir.join("screens");
    fs::create_dir_all(&screens).unwrap();
    let records = host_dir.join("terminals.json");
    fs::write(
        &records,
        serde_json::to_string(&[ended("t_old", 30), ended("t_recent", 1)]).unwrap(),
    )
    .unwrap();
    for id in ["t_old", "t_recent", "t_gone"] {
        fs::write(screens.join(format!("{id}.snap")), b"screen").unwrap();
    }

    let _host = serve(&folders, &[]);

    // the writer saves the records within a second of a change
    let deadline = Instant::now() + Duration::from_secs(5);
    while ids(&records) != ["t_recent"] {
        assert!(Instant::now() < deadline, "records: {:?}", ids(&records));
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut left: Vec<String> = fs::read_dir(&screens)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["t_recent.snap"]);
}
