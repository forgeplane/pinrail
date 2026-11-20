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
    let conversation = input(agent, name)["conversationId"]
        .as_str()
        .map(str::to_string);
    let grok = input(agent, name)["sessionId"].as_str().map(str::to_string);
    if let ("grok", Some(id)) = (agent, grok) {
        return parse_in(agent, event, name, &[("GROK_SESSION_ID", &id)]);
    }
    match (agent, session, conversation) {
        ("claude-code", Some(id), _) => {
            parse_in(agent, event, name, &[("CLAUDE_CODE_SESSION_ID", &id)])
        }
        ("antigravity", _, Some(id)) => {
            parse_in(agent, event, name, &[("ANTIGRAVITY_CONVERSATION_ID", &id)])
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

const AGY_CONVERSATION: &str = "13da3c35-ffb7-4e19-9cd9-b25ce4f1c890";

#[test]
fn antigravity_reports_each_event_of_its_session() {
    let started = parse("antigravity", "session-start", "session-start").unwrap();
    assert_eq!(started.session, AGY_CONVERSATION);
    assert_eq!(
        started.event,
        HookEvent::Started {
            source: StartSource::Startup
        }
    );
    // the first model call of a turn carries its prompt
    let prompt = parse("antigravity", "invocation", "first-invocation").unwrap();
    assert_eq!(prompt.event, HookEvent::Prompt);
    assert_eq!(
        prompt.detail.prompt.as_deref(),
        Some("Say hello in one word.\nThen say bye.")
    );
    assert!(
        prompt
            .detail
            .transcript_path
            .as_deref()
            .is_some_and(|p| p.ends_with("/logs/transcript_full.jsonl"))
    );
    assert_eq!(prompt.detail.cwd.as_deref(), Some("/Users/you/code/app"));
    // the next ones, and each tool call, are work in the turn
    assert_eq!(
        event("antigravity", "invocation", "next-invocation"),
        Some(HookEvent::Working)
    );
    assert_eq!(
        event("antigravity", "tool-done", "tool-done"),
        Some(HookEvent::Working)
    );
    assert_eq!(
        event("antigravity", "stop", "stop-canceled"),
        Some(HookEvent::TurnEnded {
            end: TurnEnd::Interrupted
        })
    );
    let failed = parse("antigravity", "stop", "stop-error").unwrap();
    assert_eq!(
        failed.event,
        HookEvent::TurnEnded {
            end: TurnEnd::Failed
        }
    );
    assert_eq!(failed.detail.error.as_deref(), Some("model unavailable"));
    assert_eq!(event("antigravity", "unknown", "stop"), None);
}

#[test]
fn antigravity_s_reply_is_the_last_response_of_its_transcript() {
    let mut stop = input("antigravity", "stop");
    stop["transcriptPath"] = Value::from(format!(
        "{}/tests/fixtures/hooks/antigravity/transcript_full.jsonl",
        env!("CARGO_MANIFEST_DIR")
    ));
    let parse = |input: &Value| {
        adapter("antigravity")
            .unwrap()
            .parse_hook("stop", input, &|k| {
                (k == "ANTIGRAVITY_CONVERSATION_ID").then(|| AGY_CONVERSATION.to_string())
            })
    };
    let done = parse(&stop).unwrap();
    assert_eq!(done.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert_eq!(done.detail.reply.as_deref(), Some("Hello.\n\nBye."));
    // a transcript it cannot read ends the turn all the same, with no reply
    stop["transcriptPath"] = Value::from("/nowhere/transcript_full.jsonl");
    let done = parse(&stop).unwrap();
    assert_eq!(done.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert_eq!(done.detail.reply, None);
}

#[test]
fn only_the_conversation_s_own_hooks_report() {
    assert!(parse_in("antigravity", "stop", "stop-canceled", &[]).is_none());
    assert!(
        parse_in(
            "antigravity",
            "stop",
            "stop-canceled",
            &[("ANTIGRAVITY_CONVERSATION_ID", "another")]
        )
        .is_none()
    );
}

const GROK_SESSION: &str = "019a8f2e-4c1d-7b3a-9e55-2f6d8c1a0b7e";

#[test]
fn grok_reports_each_event_of_its_session() {
    let started = parse("grok", "session-start", "session-start").unwrap();
    assert_eq!(started.session, GROK_SESSION);
    assert_eq!(
        started.event,
        HookEvent::Started {
            source: StartSource::Startup
        }
    );
    assert_eq!(started.detail.cwd.as_deref(), Some("/Users/you/code/app"));
    // a resumed session starts as loaded
    let mut load = input("grok", "session-start");
    load["source"] = Value::from("load");
    assert_eq!(
        grok_hook("session-start", &load).map(|r| r.event),
        Some(HookEvent::Started {
            source: StartSource::Resume
        })
    );

    let prompt = parse("grok", "prompt", "prompt").unwrap();
    assert_eq!(prompt.event, HookEvent::Prompt);
    assert_eq!(
        prompt.detail.prompt.as_deref(),
        Some("Run the shell command `ls` and tell me how many files there are.")
    );
    // the turn's own id, which a late cancel of it is matched by
    assert_eq!(
        prompt.detail.turn_id.as_deref(),
        Some("10334815-a6af-4b09-8ace-cd9a431e0d19")
    );

    // a tool call is work in the turn, and a permission it asks for waits
    assert_eq!(
        event("grok", "tool", "pre-tool-use"),
        Some(HookEvent::Working)
    );
    assert_eq!(
        event("grok", "notification", "notification-permission"),
        Some(HookEvent::NeedsInput {
            kind: WaitKind::Permission
        })
    );
    // the question tool waits for an answer, and says the question
    let asked = parse("grok", "tool", "pre-tool-use-question").unwrap();
    assert_eq!(
        asked.event,
        HookEvent::NeedsInput {
            kind: WaitKind::Question
        }
    );
    assert_eq!(
        asked.detail.summary.as_deref(),
        Some("Do you prefer tea or coffee?")
    );
    assert_eq!(
        event("grok", "notification", "notification-question"),
        Some(HookEvent::NeedsInput {
            kind: WaitKind::Question
        })
    );
    assert_eq!(
        event("grok", "permission-denied", "permission-denied"),
        Some(HookEvent::Working)
    );

    let done = parse("grok", "stop", "stop").unwrap();
    assert_eq!(done.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert_eq!(done.detail.reply.as_deref(), Some("There are 3 files."));
    for cancel in ["stop-cancelled-interrupt", "stop-cancelled-rejected"] {
        assert_eq!(
            event("grok", "stop-cancelled", cancel),
            Some(HookEvent::TurnEnded {
                end: TurnEnd::Interrupted
            }),
            "{cancel}"
        );
    }
    let mut failure = input("grok", "stop");
    failure["error"] = Value::from("rate_limit");
    let failed = grok_hook("stop-failure", &failure).unwrap();
    assert_eq!(
        failed.event,
        HookEvent::TurnEnded {
            end: TurnEnd::Failed
        }
    );
    assert_eq!(failed.detail.error.as_deref(), Some("rate_limit"));

    let ended = parse("grok", "session-end", "session-end").unwrap();
    assert_eq!(ended.event, HookEvent::Ended);
    assert_eq!(ended.detail.reason.as_deref(), Some("shutdown"));
}

/// The report of Grok's hook `event` on `input`, run by the session it
/// names.
fn grok_hook(event: &str, input: &Value) -> Option<HookReport> {
    adapter("grok").unwrap().parse_hook(event, input, &|k| {
        (k == "GROK_SESSION_ID").then(|| GROK_SESSION.to_string())
    })
}

#[test]
fn grok_reports_nothing_for_another_session_a_subagent_or_its_shutdown() {
    // the Stop Grok fires as it shuts down ends no turn
    assert_eq!(event("grok", "stop", "stop-shutdown"), None);
    // a hook of another session, or of none, as Claude Code's hooks that
    // Grok also runs
    let prompt = input("grok", "prompt");
    let other = |k: &str| (k == "GROK_SESSION_ID").then(|| "another".to_string());
    assert!(
        adapter("grok")
            .unwrap()
            .parse_hook("prompt", &prompt, &other)
            .is_none()
    );
    assert!(
        adapter("grok")
            .unwrap()
            .parse_hook("prompt", &prompt, &|_| None)
            .is_none()
    );
    // a subagent's hook
    let mut subagent = prompt.clone();
    subagent["subagentType"] = Value::from("explore");
    assert!(grok_hook("prompt", &subagent).is_none());
    // the reminder that it waits for the next prompt says nothing new
    let mut idle = input("grok", "notification-permission");
    idle["notificationType"] = Value::from("idle_prompt");
    assert!(grok_hook("notification", &idle).is_none());
}

const COPILOT_SESSION: &str = "4f2a9c1e-5b7d-4e8a-9c3b-1d2e3f4a5b6c";

/// The report of Copilot's hook `event` on `input`.
fn copilot_hook(event: &str, input: &Value) -> Option<HookReport> {
    adapter("copilot")
        .unwrap()
        .parse_hook(event, input, &|_| None)
}

#[test]
fn copilot_reports_each_event_of_its_session() {
    let started = parse("copilot", "session-start", "session-start").unwrap();
    assert_eq!(started.session, COPILOT_SESSION);
    assert_eq!(
        started.event,
        HookEvent::Started {
            source: StartSource::Startup
        }
    );
    assert_eq!(started.detail.cwd.as_deref(), Some("/Users/you/code/app"));
    assert_eq!(
        event("copilot", "session-start", "session-start-resume"),
        Some(HookEvent::Started {
            source: StartSource::Resume
        })
    );
    let prompt = parse("copilot", "prompt", "prompt").unwrap();
    assert_eq!(prompt.event, HookEvent::Prompt);
    assert_eq!(
        prompt.detail.prompt.as_deref(),
        Some("Reply with the word PONG only")
    );
    // a tool call is work in the turn, and a permission it asks for waits
    assert_eq!(
        event("copilot", "tool", "pre-tool-use"),
        Some(HookEvent::Working)
    );
    assert_eq!(
        event("copilot", "tool-done", "post-tool-use"),
        Some(HookEvent::Working)
    );
    let permission = parse("copilot", "notification", "notification-permission").unwrap();
    assert_eq!(
        permission.event,
        HookEvent::NeedsInput {
            kind: WaitKind::Permission
        }
    );
    assert_eq!(
        permission.detail.summary.as_deref(),
        Some("Run command: echo hello > a.txt")
    );
    // the question tool waits for an answer, and says the question
    let asked = parse("copilot", "tool", "pre-tool-use-ask-user").unwrap();
    assert_eq!(
        asked.event,
        HookEvent::NeedsInput {
            kind: WaitKind::Question
        }
    );
    assert_eq!(
        asked.detail.summary.as_deref(),
        Some("Do you prefer red or blue?")
    );
    assert_eq!(
        event("copilot", "notification", "notification-question"),
        Some(HookEvent::NeedsInput {
            kind: WaitKind::Question
        })
    );
    // a background command that finished says nothing of the session
    assert_eq!(
        event("copilot", "notification", "notification-shell-completed"),
        None
    );
    let ended = parse("copilot", "session-end", "session-end").unwrap();
    assert_eq!(ended.event, HookEvent::Ended);
    assert_eq!(ended.detail.reason.as_deref(), Some("user_exit"));
    // an error ends the turn as failed
    let mut error = input("copilot", "prompt");
    error["error"] = serde_json::json!({"message": "rate limited", "recoverable": false});
    let failed = copilot_hook("error", &error).unwrap();
    assert_eq!(
        failed.event,
        HookEvent::TurnEnded {
            end: TurnEnd::Failed
        }
    );
    assert_eq!(failed.detail.error.as_deref(), Some("rate limited"));
}

#[test]
fn copilot_s_reply_is_the_final_answer_in_its_transcript() {
    let mut stop = input("copilot", "agent-stop");
    stop["transcriptPath"] = Value::from(format!(
        "{}/tests/fixtures/hooks/copilot/events.jsonl",
        env!("CARGO_MANIFEST_DIR")
    ));
    let done = copilot_hook("stop", &stop).unwrap();
    assert_eq!(done.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert_eq!(done.detail.reply.as_deref(), Some("There are 3 files."));
    // the turn's reply not written yet: no reply, never the one before
    stop["transcriptPath"] = Value::from(format!(
        "{}/tests/fixtures/hooks/copilot/events-pending.jsonl",
        env!("CARGO_MANIFEST_DIR")
    ));
    let pending = copilot_hook("stop", &stop).unwrap();
    assert_eq!(pending.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert_eq!(pending.detail.reply, None);
    // a transcript it cannot read ends the turn all the same
    stop["transcriptPath"] = Value::from("/nowhere/events.jsonl");
    let done = copilot_hook("stop", &stop).unwrap();
    assert_eq!(done.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert_eq!(done.detail.reply, None);
}

#[test]
fn copilot_reports_nothing_for_a_subagent_or_its_own_notices() {
    let mut subagent = input("copilot", "pre-tool-use");
    subagent["agentId"] = Value::from("agent-1");
    assert!(copilot_hook("tool", &subagent).is_none());
    // a finished background command starts a turn Copilot gives itself
    let mut notice = input("copilot", "prompt");
    notice["prompt"] = Value::from("<system_notification>Shell 1 completed</system_notification>");
    assert_eq!(
        copilot_hook("prompt", &notice).map(|r| r.event),
        Some(HookEvent::Working)
    );
}

const PI_SESSION: &str = "01a1f907-71f2-712b-8625-7fbab4f01dd8";

#[test]
fn pi_reports_each_event_of_its_session_through_pinrail_s_extension() {
    let started = parse("pi", "session-start", "session-start").unwrap();
    assert_eq!(started.session, PI_SESSION);
    assert_eq!(
        started.event,
        HookEvent::Started {
            source: StartSource::Startup
        }
    );
    assert!(started.detail.transcript_path.is_some());
    // /new starts another session in the same process
    let new = parse("pi", "session-start", "session-start-new").unwrap();
    assert_ne!(new.session, PI_SESSION);
    assert_eq!(
        new.event,
        HookEvent::Started {
            source: StartSource::Startup
        }
    );
    let prompt = parse("pi", "prompt", "prompt").unwrap();
    assert_eq!(prompt.event, HookEvent::Prompt);
    assert_eq!(
        prompt.detail.prompt.as_deref(),
        Some("Say hello in one word.\nThen say bye.")
    );
    // a message sent while it works joins the running turn
    assert_eq!(
        event("pi", "prompt", "prompt-steer"),
        Some(HookEvent::Working)
    );
    assert_eq!(event("pi", "tool", "tool"), Some(HookEvent::Working));
    // an extension's dialog: a confirmation asks a permission, the others a
    // question, each with its title
    let confirm = parse("pi", "waiting", "waiting-confirm").unwrap();
    assert_eq!(
        confirm.event,
        HookEvent::NeedsInput {
            kind: WaitKind::Permission
        }
    );
    assert_eq!(confirm.detail.summary.as_deref(), Some("Run rm -rf build?"));
    assert_eq!(
        event("pi", "waiting", "waiting-select"),
        Some(HookEvent::NeedsInput {
            kind: WaitKind::Question
        })
    );
    assert_eq!(
        event("pi", "answered", "answered"),
        Some(HookEvent::Working)
    );

    let done = parse("pi", "stop", "stop").unwrap();
    assert_eq!(done.event, HookEvent::TurnEnded { end: TurnEnd::Done });
    assert_eq!(done.detail.reply.as_deref(), Some("Hello.\n\nBye."));
    assert_eq!(
        event("pi", "stop", "stop-aborted"),
        Some(HookEvent::TurnEnded {
            end: TurnEnd::Interrupted
        })
    );
    let failed = parse("pi", "stop", "stop-error").unwrap();
    assert_eq!(
        failed.event,
        HookEvent::TurnEnded {
            end: TurnEnd::Failed
        }
    );
    assert!(failed.detail.error.unwrap().starts_with("401"));

    let ended = parse("pi", "session-end", "session-end").unwrap();
    assert_eq!(ended.event, HookEvent::Ended);
    assert_eq!(ended.detail.reason.as_deref(), Some("quit"));
    // the session /new leaves ends with the move to the next one
    assert_eq!(event("pi", "session-end", "session-end-new"), None);
}
