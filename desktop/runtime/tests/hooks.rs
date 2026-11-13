//! Each agent's hook input, from the fixtures, read into a report.

use std::collections::HashMap;

use pinrail_runtime::adapter;
use pinrail_runtime::hooks::{HookEvent, HookReport, StartSource, TurnEnd, WaitKind};
use serde_json::Value;

fn input(agent: &str, name: &str) -> Value {
    let path = format!(
        "{}/tests/fixtures/hooks/{agent}/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The report of the hook `event` on the fixture `name`, in an environment
/// with only `vars`.
fn parse_in(agent: &str, event: &str, name: &str, vars: &[(&str, &str)]) -> Option<HookReport> {
    let env: HashMap<&str, &str> = vars.iter().copied().collect();
    adapter(agent)
        .unwrap()
        .parse_hook(event, &input(agent, name), &|k| {
            env.get(k).map(|v| v.to_string())
        })
}

/// The report of the hook `event` on the fixture `name`, as the agent runs
/// it: Claude Code with its session's id in the environment.
fn parse(agent: &str, event: &str, name: &str) -> Option<HookReport> {
    let session = input(agent, name)["session_id"]
        .as_str()
        .map(str::to_string);
    match (agent, session) {
        ("claude-code", Some(id)) => {
            parse_in(agent, event, name, &[("CLAUDE_CODE_SESSION_ID", &id)])
        }
        _ => parse_in(agent, event, name, &[]),
    }
}

fn event(agent: &str, event: &str, name: &str) -> Option<HookEvent> {
    parse(agent, event, name).map(|r| r.event)
}

const CLAUDE_SESSION: &str = "22751b4b-336a-4c33-baef-fbe1d334abab";

#[test]
fn claude_code_reports_each_event_of_its_session() {
    let c = "claude-code";
    let start = parse(c, "session-start", "session-start").unwrap();
    assert_eq!(start.session, CLAUDE_SESSION);
    assert_eq!(
        start.event,
        HookEvent::Started {
            source: StartSource::Startup
        }
    );
    assert_eq!(start.detail.cwd.as_deref(), Some("/Users/you/code/app"));
    assert!(start.detail.transcript_path.unwrap().ends_with(".jsonl"));

    let prompt = parse(c, "prompt", "prompt").unwrap();
    assert_eq!(prompt.event, HookEvent::Prompt);
    assert!(
        prompt
            .detail
            .prompt
            .unwrap()
            .starts_with("Run the shell command")
    );

    assert_eq!(event(c, "tool-done", "tool-done"), Some(HookEvent::Working));

    let permission = parse(c, "permission", "permission").unwrap();
    assert_eq!(
        permission.event,
        HookEvent::NeedsInput {
            kind: WaitKind::Permission
        }
    );
    assert_eq!(permission.detail.tool.as_deref(), Some("Bash"));
    assert_eq!(
        permission.detail.summary.as_deref(),
        Some("touch denied.txt")
    );
    // a refused call clears the wait, and the turn goes on
    assert_eq!(
        event(c, "permission-denied", "permission-denied"),
        Some(HookEvent::Working)
    );

    assert_eq!(
        event(c, "notification", "notification-permission"),
        Some(HookEvent::NeedsInput {
            kind: WaitKind::Permission
        })
    );
    assert_eq!(
        event(c, "notification", "notification-elicitation"),
        Some(HookEvent::NeedsInput {
            kind: WaitKind::Question
        })
    );
    // a notification that asks nothing reports nothing
    assert_eq!(event(c, "notification", "notification-idle"), None);

    let stop = parse(c, "stop", "stop").unwrap();
    assert_eq!(stop.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert!(stop.detail.reply.unwrap().starts_with("Done."));

    let failed = parse(c, "stop-failure", "stop-failure").unwrap();
    assert_eq!(
        failed.event,
        HookEvent::TurnEnded {
            end: TurnEnd::Failed
        }
    );
    assert_eq!(failed.detail.error.as_deref(), Some("rate_limit"));

    let end = parse(c, "session-end", "session-end").unwrap();
    assert_eq!(end.event, HookEvent::Ended);
    assert_eq!(end.detail.reason.as_deref(), Some("other"));
}

#[test]
fn a_claude_code_subagent_neither_speaks_for_the_session_nor_starts_a_turn() {
    let c = "claude-code";
    assert_eq!(event(c, "tool-done", "subagent-tool-done"), None);
    // its result comes back as a prompt without an agent_id, inside the turn
    assert_eq!(
        event(c, "prompt", "handback-prompt"),
        Some(HookEvent::Working)
    );
}

#[test]
fn a_background_task_s_notification_is_work_in_the_turn_not_a_prompt() {
    // Claude Code hands itself a finished background command as a prompt
    assert_eq!(
        event("claude-code", "prompt", "task-notification-prompt"),
        Some(HookEvent::Working)
    );
}

#[test]
fn only_claude_code_s_own_hooks_report() {
    let c = "claude-code";
    // Cursor and Grok run Claude Code's hooks without its session's id
    assert_eq!(parse_in(c, "stop", "cursor-fired", &[]), None);
    assert_eq!(parse_in(c, "stop", "grok-fired", &[]), None);
    // or inherit the id of the Claude Code session they were started in
    assert_eq!(
        parse_in(
            c,
            "stop",
            "cursor-fired",
            &[("CLAUDE_CODE_SESSION_ID", "another-session")]
        ),
        None
    );
    // and Claude Code's own is reported
    assert!(event(c, "stop", "cursor-fired").is_some());
}

#[test]
fn an_unknown_source_or_event_reports_nothing() {
    let c = "claude-code";
    assert_eq!(
        event(c, "session-start", "session-start-unknown-source"),
        None
    );
    assert_eq!(event(c, "compact", "stop"), None);
    assert_eq!(event("codex", "compact", "stop"), None);
    // agents that are data only have no adapter, and report nothing
    assert!(adapter("goose").is_none());
    assert!(adapter("nobody").is_none());
}

#[test]
fn codex_reports_each_event_of_its_session() {
    let x = "codex";
    let start = parse(x, "session-start", "session-start").unwrap();
    assert_eq!(start.session, "019a7f3e-5b2c-7d10-9e4f-2a6b8c0d1e3f");
    assert_eq!(
        start.event,
        HookEvent::Started {
            source: StartSource::Startup
        }
    );
    let prompt = parse(x, "prompt", "prompt").unwrap();
    assert_eq!(prompt.event, HookEvent::Prompt);
    assert_eq!(
        prompt.detail.prompt.as_deref(),
        Some("Add a health check endpoint")
    );
    assert_eq!(event(x, "tool", "pre-tool"), Some(HookEvent::Working));
    assert_eq!(event(x, "tool", "post-tool"), Some(HookEvent::Working));
    let permission = parse(x, "permission", "permission").unwrap();
    assert_eq!(
        permission.event,
        HookEvent::NeedsInput {
            kind: WaitKind::Permission
        }
    );
    assert_eq!(
        permission.detail.summary.as_deref(),
        Some("git push origin main")
    );
    assert_eq!(
        event(x, "interrupt", "interrupt"),
        Some(HookEvent::TurnEnded {
            end: TurnEnd::Interrupted
        })
    );
    let stop = parse(x, "stop", "stop").unwrap();
    assert_eq!(
        stop.detail.reply.as_deref(),
        Some("Added /health, with a test.")
    );
    assert_eq!(
        event(x, "session-end", "session-end"),
        Some(HookEvent::Ended)
    );
}

#[test]
fn a_codex_subagent_does_not_speak_for_the_session() {
    assert_eq!(event("codex", "prompt", "subagent-prompt"), None);
}

#[test]
fn an_adapter_is_found_by_any_name_of_its_agent() {
    assert_eq!(adapter("claude").unwrap().spec().id, "claude-code");
    assert_eq!(adapter("codex").unwrap().spec().id, "codex");
}

#[test]
fn a_report_is_sent_as_the_core_reads_it() {
    let c = "claude-code";
    let body = |agent: &str, event: &str, name: &str| parse(agent, event, name).unwrap().to_body();
    let start = body(c, "session-start", "session-start");
    assert_eq!(
        (start["event"].as_str(), start["source"].as_str()),
        (Some("started"), Some("startup"))
    );
    let permission = body(c, "permission", "permission");
    assert_eq!(permission["event"], "needs_input");
    assert_eq!(permission["wait"], "permission");
    assert_eq!(permission["detail"]["tool"], "Bash");
    assert_eq!(permission["detail"]["summary"], "touch denied.txt");
    // a failed turn's error is its reply; the transcript and the agent's
    // turn id place the turn in the agent's own record
    let failed = body(c, "stop-failure", "stop-failure");
    assert_eq!(failed["event"], "turn_ended");
    assert_eq!(failed["end"], "failed");
    assert_eq!(failed["detail"]["reply"], "rate_limit");
    assert!(
        failed["transcript_path"]
            .as_str()
            .unwrap()
            .ends_with(".jsonl")
    );
    assert_eq!(
        failed["detail"]["turn_id"],
        "7bf504f0-029d-4e13-a137-e4e2ad6a3a77"
    );
    let codex = parse("codex", "stop", "stop").unwrap().to_body();
    assert_eq!(
        codex["detail"]["turn_id"],
        "019a7f3e-6a00-7c00-8000-000000000001"
    );
    // the prompt is cut to what the core keeps
    let mut input = input(c, "prompt");
    input["prompt"] = serde_json::json!("x".repeat(5_000));
    let report = adapter(c)
        .unwrap()
        .parse_hook("prompt", &input, &|k| {
            (k == "CLAUDE_CODE_SESSION_ID").then(|| CLAUDE_SESSION.to_string())
        })
        .unwrap();
    let cut = report.to_body();
    assert_eq!(cut["detail"]["prompt"].as_str().unwrap().len(), 4_000);
    assert_eq!(cut["detail"]["prompt_cut"], true);
    // a reply that fits is not marked
    assert!(body(c, "stop", "stop")["detail"].get("reply_cut").is_none());
}
