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
fn antigravity_starts_with_its_task_and_learns_its_conversation_later() {
    let agy = adapter("antigravity").unwrap();
    // a bare argument is ignored: the task goes with --prompt-interactive,
    // as one argument, so a task that starts with - is never a flag
    let extra = args(&["--model", "gemini-4-neon"]);
    let plan = agy.start(&request(Some("-fix the login"), &extra)).unwrap();
    assert_eq!(
        plan.argv,
        args(&[
            "agy",
            "--model",
            "gemini-4-neon",
            "--prompt-interactive=-fix the login"
        ])
    );
    assert_eq!(plan.session, None);
    let plan = agy.start(&request(None, &[])).unwrap();
    assert_eq!(plan.argv, args(&["agy"]));
}

#[test]
fn grok_starts_with_the_session_id_pinrail_chose() {
    let extra = args(&["--model", "grok-4.7"]);
    let plan = adapter("grok")
        .unwrap()
        .start(&request(Some("help"), &extra))
        .unwrap();
    // a task named as one of Grok's commands is still the prompt
    assert_eq!(
        plan.argv,
        args(&["grok", "-s", ID, "--model", "grok-4.7", "--", "help"])
    );
    assert_eq!(plan.session.as_deref(), Some(ID));
}

#[test]
fn copilot_starts_with_the_session_id_pinrail_chose() {
    let extra = args(&["--model", "gpt-5"]);
    let plan = adapter("copilot")
        .unwrap()
        .start(&request(Some("-fix the login"), &extra))
        .unwrap();
    // the task as one argument, never read as a flag
    assert_eq!(
        plan.argv,
        args(&[
            "copilot",
            "--session-id",
            ID,
            "--model",
            "gpt-5",
            "--interactive=-fix the login"
        ])
    );
    assert_eq!(plan.session.as_deref(), Some(ID));
    let bare = adapter("copilot")
        .unwrap()
        .start(&request(None, &[]))
        .unwrap();
    assert_eq!(bare.argv, args(&["copilot", "--session-id", ID]));
}

#[test]
fn pi_starts_with_the_session_id_pinrail_chose() {
    let extra = args(&["--model", "haiku"]);
    let plan = adapter("pi")
        .unwrap()
        .start(&request(Some("-fix the login"), &extra))
        .unwrap();
    assert_eq!(
        plan.argv,
        args(&[
            "pi",
            "--session-id",
            ID,
            "--model",
            "haiku",
            "--",
            "-fix the login"
        ])
    );
    assert_eq!(plan.session.as_deref(), Some(ID));
    // Pi reads an argument that starts with @ as a file, even after --
    let at = adapter("pi")
        .unwrap()
        .start(&request(Some("@src/app.ts fix it"), &[]))
        .unwrap();
    assert_eq!(
        at.argv.last().map(String::as_str),
        Some(" @src/app.ts fix it")
    );
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
        (
            adapter("antigravity").unwrap(),
            &["--dangerously-skip-permissions"],
            "--dangerously-skip-permissions",
        ),
        (
            adapter("grok").unwrap(),
            &["--always-approve"],
            "--always-approve",
        ),
        (
            adapter("copilot").unwrap(),
            &["--allow-all-tools"],
            "--allow-all-tools",
        ),
        (adapter("copilot").unwrap(), &["--allow-all"], "--allow-all"),
        (adapter("pi").unwrap(), &["--approve"], "--approve"),
        (adapter("pi").unwrap(), &["-a"], "-a"),
        (
            adapter("copilot").unwrap(),
            &["--allow-all-paths"],
            "--allow-all-paths",
        ),
        (
            adapter("copilot").unwrap(),
            &["--allow-all-urls"],
            "--allow-all-urls",
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
fn each_agent_resumes_a_session_by_its_id() {
    assert_eq!(
        adapter("claude-code").unwrap().resume(ID),
        Some(args(&["claude", "--resume", ID]))
    );
    assert_eq!(
        adapter("codex").unwrap().resume(ID),
        Some(args(&["codex", "--no-daemon", "resume", ID]))
    );
    assert_eq!(
        adapter("antigravity").unwrap().resume(ID),
        Some(args(&["agy", &format!("--conversation={ID}")]))
    );
    assert_eq!(
        adapter("grok").unwrap().resume(ID),
        Some(args(&["grok", "-r", ID]))
    );
    assert_eq!(
        adapter("copilot").unwrap().resume(ID),
        Some(args(&["copilot", &format!("--resume={ID}")]))
    );
    // --session-id would create a session that is not there
    assert_eq!(
        adapter("pi").unwrap().resume(ID),
        Some(args(&["pi", "--session", ID]))
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
        assert_eq!(adapter("antigravity").unwrap().resume(id), None, "{id:?}");
        assert_eq!(adapter("grok").unwrap().resume(id), None, "{id:?}");
        assert_eq!(adapter("copilot").unwrap().resume(id), None, "{id:?}");
        assert_eq!(adapter("pi").unwrap().resume(id), None, "{id:?}");
    }
}

#[test]
fn only_claude_code_resumes_a_session_still_running_elsewhere() {
    assert!(adapter("claude-code").unwrap().resumes_while_running());
    assert!(!adapter("codex").unwrap().resumes_while_running());
}

#[test]
fn each_agent_typed_into_takes_a_message_as_a_bracketed_paste_and_enter() {
    for id in ["claude-code", "codex", "grok", "copilot", "pi"] {
        let plan = adapter(id).unwrap().submit().unwrap();
        assert!(plan.bracketed, "{id}");
        assert_eq!((plan.settle_ms, plan.max_ms), (150, 1_000), "{id}");
        assert_eq!(plan.submit, "\r", "{id}");
        assert_eq!(plan.confirms, Some("prompt"), "{id}");
    }
}

#[test]
fn antigravity_takes_a_message_as_a_bracketed_paste_and_enter() {
    let plan = adapter("antigravity").unwrap().submit().unwrap();
    assert!(plan.bracketed);
    assert_eq!(plan.submit, "\r");
    // the turn's first model call carries the prompt that confirms it
    assert_eq!(plan.confirms, Some("invocation"));
}
