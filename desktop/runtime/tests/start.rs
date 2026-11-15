//! How Pinrail starts each agent, and the arguments it refuses because
//! they turn the agent's permission checks off.

use pinrail_runtime::{StartRequest, adapter, all, denied_arg};

const ID: &str = "6f1c2a4e-0b7d-4c8e-9f3a-2d5b6c7e8f90";

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn request<'a>(task: Option<&'a str>, extra_args: &'a [String]) -> StartRequest<'a> {
    StartRequest {
        task,
        extra_args,
        session_id: ID,
    }
}

#[test]
fn claude_code_starts_with_the_session_id_pinrail_chose() {
    let extra = args(&["--model", "opus"]);
    let plan = adapter("claude-code")
        .unwrap()
        .start(&request(Some("-fix the login"), &extra))
        .unwrap();
    assert_eq!(
        plan.argv,
        args(&[
            "claude",
            "--session-id",
            ID,
            "--model",
            "opus",
            "--",
            "-fix the login"
        ])
    );
    assert_eq!(plan.session.as_deref(), Some(ID));
}

#[test]
fn codex_starts_without_its_daemon_and_learns_its_session_later() {
    let plan = adapter("codex")
        .unwrap()
        .start(&request(None, &[]))
        .unwrap();
    assert_eq!(plan.argv, args(&["codex", "--no-daemon"]));
    assert_eq!(plan.session, None);
}

#[test]
fn a_blank_task_is_left_out() {
    let plan = adapter("codex")
        .unwrap()
        .start(&request(Some("  "), &[]))
        .unwrap();
    assert_eq!(plan.argv, args(&["codex", "--no-daemon"]));
}

#[test]
fn flags_that_turn_permission_checks_off_are_refused() {
    let claude = adapter("claude-code").unwrap();
    let codex = adapter("codex").unwrap();
    for (agent, given, refused) in [
        (
            claude,
            &["--dangerously-skip-permissions"][..],
            "--dangerously-skip-permissions",
        ),
        (claude, &["--model", "opus", "--yolo"], "--yolo"),
        (
            claude,
            &["--permission-mode", "bypassPermissions"],
            "--permission-mode",
        ),
        (
            claude,
            &["--permission-mode=bypassPermissions"],
            "--permission-mode=bypassPermissions",
        ),
        (
            codex,
            &["--dangerously-bypass-approvals-and-sandbox"],
            "--dangerously-bypass-approvals-and-sandbox",
        ),
        (
            codex,
            &["--dangerously-bypass-hook-trust=true"],
            "--dangerously-bypass-hook-trust=true",
        ),
    ] {
        assert_eq!(denied_arg(agent, &args(given)), Some(refused), "{given:?}");
    }
    assert_eq!(
        denied_arg(
            claude,
            &args(&["--permission-mode", "plan", "--model", "opus"])
        ),
        None
    );
}

#[test]
fn no_start_plan_carries_a_denied_flag() {
    for spec in all() {
        let Some(agent) = adapter(spec.id) else {
            continue;
        };
        if let Some(plan) = agent.start(&request(Some("task"), &[])) {
            assert_eq!(denied_arg(agent, &plan.argv), None, "{}", spec.id);
        }
    }
}

#[test]
fn claude_code_and_codex_resume_a_session_by_its_id() {
    assert_eq!(
        adapter("claude-code").unwrap().resume(ID),
        Some(args(&["claude", "--resume", ID]))
    );
    assert_eq!(
        adapter("codex").unwrap().resume(ID),
        Some(args(&["codex", "--no-daemon", "resume", ID]))
    );
}

#[test]
fn a_session_id_that_could_be_read_as_a_flag_is_not_resumed() {
    for id in [
        "--dangerously-skip-permissions",
        "-x",
        "",
        "a b",
        "id\n--yolo",
    ] {
        assert_eq!(adapter("claude-code").unwrap().resume(id), None, "{id:?}");
        assert_eq!(adapter("codex").unwrap().resume(id), None, "{id:?}");
    }
}
