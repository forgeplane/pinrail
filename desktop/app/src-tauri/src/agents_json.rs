//! The window's copy of the agent registry: each agent's id, label and
//! aliases, in `ui/src/agents.generated.json`, which the window reads to
//! name an agent and pick its icon. The registry in `pinrail-runtime` is
//! the source; this test fails when the copy differs from it, and writes
//! the copy with `PINRAIL_WRITE_AGENTS=1`.

use std::path::PathBuf;

use serde_json::json;

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/src/agents.generated.json")
}

fn expected() -> String {
    let agents: Vec<_> = pinrail_runtime::all()
        .iter()
        .map(|a| json!({ "id": a.id, "label": a.label, "aliases": a.aliases }))
        .collect();
    serde_json::to_string_pretty(&agents).unwrap() + "\n"
}

#[test]
fn the_window_names_the_agents_the_registry_knows() {
    if std::env::var_os("PINRAIL_WRITE_AGENTS").is_some() {
        std::fs::write(path(), expected()).unwrap();
    }
    let written = std::fs::read_to_string(path()).unwrap_or_default();
    assert!(
        written == expected(),
        "{} differs from the agent registry; write it with \
         PINRAIL_WRITE_AGENTS=1 cargo test -p pinrail-desktop agents_json",
        path().display()
    );
}
