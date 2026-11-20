//! Which process is an agent's: each adapter recognises its own, by default
//! by the names of the agent's binaries.

use pinrail_runtime::process::{Process, ancestor, arguments, process};
use pinrail_runtime::{adapter, runs_as};

fn named(name: &str) -> Process {
    Process {
        pid: 42,
        parent: 1,
        name: name.to_string(),
        started: "1".to_string(),
    }
}

#[test]
fn an_agent_s_process_is_one_named_after_its_binary() {
    let claude = adapter("claude-code").unwrap();
    assert!(claude.is_own_process(&named("claude")));
    assert!(!claude.is_own_process(&named("claude-1.0")));
    assert!(!claude.is_own_process(&named("node")));
    // an agent with no adapter of its own, by its registry entry
    assert!(runs_as("goose", &named("goose")));
    assert!(!runs_as("goose", &named("claude")));
    assert!(runs_as("claude-code", &named("claude")));
}

#[test]
fn grok_s_process_is_named_after_the_versioned_binary_it_links_to() {
    let grok = adapter("grok").unwrap();
    for name in ["grok", "grok-1.0.89-macos-aarch64", "grok-1.0.89-lin"] {
        assert!(grok.is_own_process(&named(name)), "{name}");
    }
    for name in ["grok-helper", "groker", "node"] {
        assert!(!grok.is_own_process(&named(name)), "{name}");
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn the_walk_finds_the_nearest_process_a_test_accepts() {
    let me = process(std::process::id()).unwrap();
    let found = ancestor(me.pid, &|p| p.name == me.name).unwrap();
    assert_eq!(found.pid, me.pid);
    // the parent, past this process
    let parent = ancestor(me.pid, &|p| p.pid == me.parent).unwrap();
    assert_eq!(parent.pid, me.parent);
    assert_eq!(ancestor(me.pid, &|_| false), None);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_process_s_arguments_are_read_as_it_was_started() {
    let args = arguments(std::process::id()).unwrap();
    let own: Vec<String> = std::env::args().collect();
    assert_eq!(args, own);
}

#[test]
fn pi_s_process_is_named_pi_or_runs_pi_under_node() {
    let pi = adapter("pi").unwrap();
    assert!(pi.is_own_process(&named("pi")));
    // a node process whose arguments cannot be read is not Pi's
    let mut gone = named("node");
    gone.pid = u32::MAX - 1;
    assert!(!pi.is_own_process(&gone));
    assert!(!pi.is_own_process(&named("claude")));
}
