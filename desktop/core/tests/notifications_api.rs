//! `/api/v1/notifications`: the lists and their pages, the counts, and the
//! person's changes to the read and done state.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

const REPO: &str = "forgeplane/pinrail";

async fn agent(app: &App, method: &str, path: &str, body: Value) -> Value {
    let mut body = body;
    body["by"] = json!("claude-code");
    body["session"] = json!("s-1");
    let (code, answer) = common::call_as(app, method, path, Some(body), &[]).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
    answer
}

/// Three items an agent added, each with its notification.
async fn three(app: &App) {
    for title in ["Themes", "Docs", "Search"] {
        agent(
            app,
            "POST",
            "/api/v1/items",
            json!({"repo": REPO, "title": title}),
        )
        .await;
    }
}

async fn list(app: &App, query: &str) -> Value {
    let (code, answer) = call(app, "GET", &format!("/api/v1/notifications{query}"), None).await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    answer
}

fn keys(list: &Value) -> Vec<&str> {
    list["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["subject"]["item"]["key"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn the_list_pages_newest_first_with_its_total_and_counts() {
    let app = app();
    three(&app).await;
    let first = list(&app, "?limit=2").await;
    assert_eq!(keys(&first), ["PIN-3", "PIN-2"]);
    assert_eq!(first["total"], 3);
    assert_eq!(first["counts"], json!({"waiting": 0, "unread": 3}));
    let newest = common::db(&app)
        .events_after(0, 1000)
        .unwrap()
        .last()
        .unwrap()
        .id;
    assert_eq!(first["up_to"], newest);
    assert_eq!(keys(&list(&app, "?limit=2&offset=2").await), ["PIN-1"]);

    let row = &first["notifications"][0];
    assert_eq!(row["reason"], "item_added");
    assert_eq!(row["subject"]["item"]["title"], "Search");
    assert_eq!(row["subject"]["item"]["status"], "open");
    assert_eq!(row["repo"], REPO);
    assert_eq!(row["actor"]["kind"], "agent");
    assert_eq!(
        (&row["unread"], &row["read"], &row["done"]),
        (&json!(1), &json!(false), &json!(false))
    );

    let (_, counts) = call(&app, "GET", "/api/v1/notifications/counts", None).await;
    assert_eq!(counts, json!({"waiting": 0, "unread": 3}));
    let (code, _) = call(&app, "GET", "/api/v1/notifications?tab=later", None).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_person_marks_a_notification_read_unread_done_and_back() {
    let app = app();
    three(&app).await;
    let id = list(&app, "?limit=1").await["notifications"][0]["id"].clone();
    let path = |what: &str| format!("/api/v1/notifications/{id}/{what}");
    let mut live = app.state.events().subscribe();

    assert_eq!(
        call(&app, "PUT", &path("read"), None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(keys(&list(&app, "?tab=unread").await), ["PIN-2", "PIN-1"]);
    // every window hears of it, with the notification and the counts
    let notice = live.try_recv().unwrap();
    assert_eq!(notice.kind, "notification_changed");
    assert_eq!(notice.notification.unwrap()["read"], true);
    assert_eq!(notice.counts.unwrap(), json!({"waiting": 0, "unread": 2}));

    call(&app, "DELETE", &path("read"), None).await;
    assert_eq!(list(&app, "?tab=unread").await["total"], 3);

    call(&app, "PUT", &path("done"), None).await;
    assert_eq!(keys(&list(&app, "").await), ["PIN-2", "PIN-1"]);
    call(&app, "DELETE", &path("done"), None).await;
    assert_eq!(keys(&list(&app, "").await), ["PIN-3", "PIN-2", "PIN-1"]);

    let (code, _) = call(&app, "PUT", "/api/v1/notifications/999/read", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_the_person_changes_a_notification() {
    let app = app();
    three(&app).await;
    let id = list(&app, "?limit=1").await["notifications"][0]["id"].clone();
    for (method, path) in [
        ("PUT", format!("/api/v1/notifications/{id}/read")),
        ("DELETE", format!("/api/v1/notifications/{id}/read")),
        ("PUT", format!("/api/v1/notifications/{id}/done")),
        ("DELETE", format!("/api/v1/notifications/{id}/done")),
        ("POST", "/api/v1/notifications/read".to_string()),
    ] {
        let (code, _) =
            common::call_as(&app, method, &path, Some(json!({"up_to": 1000})), &[]).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    assert_eq!(list(&app, "").await["counts"]["unread"], 3);
}

#[tokio::test]
async fn reading_all_leaves_an_event_after_the_list_unread() {
    let app = app();
    three(&app).await;
    let shown = list(&app, "?tab=unread").await;
    // an agent comments after the list was read
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Started."}),
    )
    .await;
    let (code, answer) = call(
        &app,
        "POST",
        "/api/v1/notifications/read",
        Some(json!({"tab": "unread", "up_to": shown["up_to"]})),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    assert_eq!(answer["read"], 2);
    let unread = list(&app, "?tab=unread").await;
    assert_eq!(keys(&unread), ["PIN-1"]);
    assert_eq!(unread["notifications"][0]["unread"], 2);
}

#[tokio::test]
async fn a_waiting_notification_is_not_marked_done() {
    let app = app();
    let review = agent(
        &app,
        "POST",
        "/api/v1/reviews",
        json!({"kind": "question", "title": "Deploy?",
               "origin": {"repo": REPO, "ref": "main"},
               "requested_by": "claude-code", "session": "s-1",
               "payload": {"questions": [{"id": "q1", "question": "Deploy?", "answer": "yes-no"}]}}),
    )
    .await;
    let subject =
        pinrail_core::db::EventSubject::Review(review["id"].as_str().unwrap().to_string());
    let id = common::db(&app)
        .report_notification(&pinrail_core::db::Notify {
            subject: &subject,
            reason: "question_asked",
            waiting: true,
            event: 1,
            actor: Some("claude-code"),
            summary: None,
            at: chrono::Utc::now(),
        })
        .unwrap()
        .unwrap();
    let (code, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/notifications/{id}/done"),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT);
}

/// What a list query keeps, each notification as its subject's kind, its
/// reason and its repository, sorted.
async fn kept(app: &App, query: &str) -> Vec<String> {
    let mut kept: Vec<String> = list(app, query).await["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            format!(
                "{} {} {}",
                n["subject"]["kind"].as_str().unwrap(),
                n["reason"].as_str().unwrap(),
                n["repo"].as_str().unwrap_or("-")
            )
        })
        .collect();
    kept.sort();
    kept
}

/// Work items and reviews in three repositories, two of them in the
/// project Web, and forgeplane/pinrail in the project Pinrail.
async fn mixed(app: &App) -> (String, String) {
    agent(
        app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Load themes"}),
    )
    .await;
    agent(
        app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "The loader is ready."}),
    )
    .await;
    agent(
        app,
        "POST",
        "/api/v1/items",
        json!({"repo": "acme/web", "title": "Dark theme"}),
    )
    .await;
    let review = |repo: &str, plugin: &str, payload: Value| {
        json!({"plugin": plugin, "title": format!("Review of {repo}"),
               "origin": {"repo": repo}, "requested_by": "claude-code", "payload": payload})
    };
    agent(
        app,
        "POST",
        "/api/v1/reviews",
        review(
            REPO,
            "list",
            json!({"groups": [{"title": "a", "items": [{"id": 1, "title": "one"}]}]}),
        ),
    )
    .await;
    agent(
        app,
        "POST",
        "/api/v1/reviews",
        review(
            "acme/api",
            "feedback",
            json!({"groups": [{"id": "g", "title": "G", "questions": [{"id": "q", "type": "text", "prompt": "Why?"}]}]}),
        ),
    )
    .await;
    let project = |name: &str, repos: Value| {
        let body = json!({"name": name, "repos": repos});
        async move {
            let (code, project) = call(app, "POST", "/api/v1/projects", Some(body)).await;
            assert_eq!(code, StatusCode::CREATED, "{project}");
            project["id"].as_str().unwrap().to_string()
        }
    };
    let web = project("Web", json!(["acme/web", "acme/api"])).await;
    let pinrail = project("Pinrail", json!([REPO])).await;
    (web, pinrail)
}

#[tokio::test]
async fn each_tab_and_filter_keeps_its_notifications() {
    let app = app();
    let (web, pinrail) = mixed(&app).await;
    assert_eq!(list(&app, "").await["total"], 4);
    assert_eq!(
        kept(&app, "?tab=waiting").await,
        [
            "review review_requested acme/api",
            "review review_requested forgeplane/pinrail"
        ]
    );
    // two kinds, and a subject kind
    assert_eq!(
        kept(&app, "?kind=item_commented,item_added").await,
        [
            "item item_added acme/web",
            "item item_commented forgeplane/pinrail"
        ]
    );
    assert_eq!(kept(&app, "?kind=review").await.len(), 2);
    // a project and a repository of another project
    assert_eq!(
        kept(&app, &format!("?scope=project:{web}")).await,
        [
            "item item_added acme/web",
            "review review_requested acme/api"
        ]
    );
    assert_eq!(
        kept(&app, &format!("?scope=project:{web},{REPO}"))
            .await
            .len(),
        4
    );
    assert_eq!(
        kept(&app, &format!("?scope=project:{pinrail}")).await.len(),
        2
    );
    // two plugins, and an empty filter
    assert_eq!(kept(&app, "?plugin=list,feedback").await.len(), 2);
    assert_eq!(
        kept(&app, "?plugin=feedback").await,
        ["review review_requested acme/api"]
    );
    assert_eq!(kept(&app, "?kind=&scope=&plugin=").await.len(), 4);
    // filters combine
    assert_eq!(
        kept(&app, &format!("?kind=review&scope=project:{pinrail}")).await,
        ["review review_requested forgeplane/pinrail"]
    );
    // the search: a title, a key, a summary
    assert_eq!(kept(&app, "?q=dark").await, ["item item_added acme/web"]);
    assert_eq!(
        kept(&app, "?q=PIN-1").await,
        ["item item_commented forgeplane/pinrail"]
    );
    assert_eq!(kept(&app, "?q=loader").await.len(), 1);
    assert!(kept(&app, "?q=100%25").await.is_empty());

    let (code, answer) = call(&app, "GET", "/api/v1/notifications?kind=later", None).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(answer.to_string().contains("item_commented"), "{answer}");
}

#[tokio::test]
async fn done_notifications_are_listed_on_request() {
    let app = app();
    three(&app).await;
    let id = list(&app, "?limit=1").await["notifications"][0]["id"].clone();
    call(
        &app,
        "PUT",
        &format!("/api/v1/notifications/{id}/done"),
        None,
    )
    .await;
    assert_eq!(list(&app, "").await["total"], 2);
    let with_done = list(&app, "?done=true").await;
    assert_eq!(with_done["total"], 3);
    assert_eq!(with_done["notifications"][0]["done"], true);
}

#[tokio::test]
async fn reading_all_of_two_kinds_leaves_the_other_kinds_unread() {
    let app = app();
    mixed(&app).await;
    let shown = list(&app, "?kind=item_added,item_commented").await;
    let (code, answer) = call(
        &app,
        "POST",
        "/api/v1/notifications/read",
        Some(json!({"kind": "item_added,item_commented", "up_to": shown["up_to"]})),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    assert_eq!(answer["read"], 2);
    assert_eq!(
        kept(&app, "?tab=unread").await,
        [
            "review review_requested acme/api",
            "review review_requested forgeplane/pinrail"
        ]
    );
}

#[tokio::test]
async fn a_notification_lists_the_events_of_its_subject() {
    let app = app();
    three(&app).await;
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Started."}),
    )
    .await;
    let row = list(&app, "?kind=item_commented").await["notifications"][0].clone();
    let path = format!("/api/v1/notifications/{}/events", row["id"]);
    let (code, answer) = call(&app, "GET", &path, None).await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    let changes: Vec<&str> = answer["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["item"]["change"].as_str().unwrap())
        .collect();
    assert_eq!(changes, ["created", "commented"]);
    // after the first, the second alone
    let first = answer["events"][0]["event_id"].as_i64().unwrap();
    let (_, later) = call(&app, "GET", &format!("{path}?after={first}"), None).await;
    assert_eq!(later["events"].as_array().unwrap().len(), 1);
    let (code, _) = call(&app, "GET", "/api/v1/notifications/999/events", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

/// What a list's first notification says is new.
async fn new_of(app: &App, query: &str) -> Value {
    list(app, query).await["notifications"][0]["new"].clone()
}

/// A request by codex, in its session s-2.
async fn codex(app: &App, method: &str, path: &str, body: Value) {
    let mut body = body;
    body["by"] = json!("codex");
    body["session"] = json!("s-2");
    let (code, answer) = common::call_as(app, method, path, Some(body), &[]).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
}

#[tokio::test]
async fn a_breakdown_is_new_with_its_sub_items() {
    let app = app();
    let items: Vec<Value> =
        std::iter::once(json!({"ref": "p", "title": "First series of shorts"}))
            .chain((1..=10).map(
                |i| json!({"ref": format!("s{i}"), "parent": "p", "title": format!("Short {i}")}),
            ))
            .collect();
    agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "items": items}),
    )
    .await;
    let new = new_of(&app, "?q=First").await;
    assert_eq!(new["breakdown"], true);
    assert_eq!(new["sub_items"], 10);
    assert_eq!(
        new["sub_item"],
        json!({"key": "PIN-11", "title": "Short 10"})
    );
    assert_eq!(new["latest"], "sub_item");
}

#[tokio::test]
async fn sub_items_added_later_are_counted_since_the_last_read() {
    let app = app();
    three(&app).await;
    let id = list(&app, "?q=Themes").await["notifications"][0]["id"].clone();
    call(
        &app,
        "PUT",
        &format!("/api/v1/notifications/{id}/read"),
        None,
    )
    .await;
    for title in ["Read the files", "Apply the colours"] {
        agent(
            &app,
            "POST",
            "/api/v1/items",
            json!({"repo": REPO, "title": title, "parent": "PIN-1"}),
        )
        .await;
    }
    let new = new_of(&app, "?q=Themes").await;
    assert_eq!(
        (&new["sub_items"], &new["breakdown"]),
        (&json!(2), &json!(false))
    );
    assert_eq!(new["sub_item"]["title"], "Apply the colours");
    // a sub-item's change after it was added is its own row, which names
    // its parent
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-5/comments",
        json!({"body": "Started."}),
    )
    .await;
    let sub = new_of(&app, "?kind=item_commented").await;
    assert_eq!(
        list(&app, "?kind=item_commented").await["notifications"][0]["subject"]["item"]["key"],
        "PIN-5"
    );
    assert_eq!(sub["parent"], "PIN-1");
}

#[tokio::test]
async fn comments_are_counted_with_their_authors_and_a_change_after_them_is_latest() {
    let app = app();
    three(&app).await;
    let comment = |text: &str| json!({"body": text});
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        comment("Claimed it."),
    )
    .await;
    codex(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        comment("I can help."),
    )
    .await;
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        comment("The spec is ready."),
    )
    .await;
    let new = new_of(&app, "?q=Themes").await;
    assert_eq!(new["comments"], 3);
    assert_eq!(new["authors"], json!(["claude-code", "codex"]));
    assert_eq!(new["comment"], "The spec is ready.");
    assert_eq!(new["latest"], "comment");

    agent(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        json!({"claim": "claim"}),
    )
    .await;
    let new = new_of(&app, "?q=Themes").await;
    assert_eq!(
        (&new["comments"], &new["changes"], &new["latest"]),
        (&json!(3), &json!(1), &json!("change"))
    );
}

#[tokio::test]
async fn a_finished_agent_is_new_with_its_last_reply() {
    let app = app();
    let at = |ms: i64| (chrono::Utc::now() + chrono::Duration::milliseconds(ms)).to_rfc3339();
    let report = |event: &str, ms: i64, extra: Value| {
        let mut body = json!({"event": event, "at": at(ms)});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        body
    };
    // a session the sidebar lists, as a status makes it
    let (code, answer) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"kind": "step", "text": "Working"})),
    )
    .await;
    assert!(code.is_success(), "{answer}");
    let path = "/api/v1/sessions/claude-code/s-1/reports";
    for body in [
        report("prompt", 0, json!({})),
        report(
            "turn_ended",
            10,
            json!({"end": "done", "detail": {"reply": "Did the first"}}),
        ),
        report("prompt", 20, json!({})),
        report(
            "turn_ended",
            30,
            json!({"end": "done", "detail": {"reply": "## Done\n\nPicked Rachel for the sidekick.\nMore."}}),
        ),
    ] {
        let (code, answer) = call(&app, "POST", path, Some(body)).await;
        assert!(code.is_success(), "{answer}");
    }
    let new = new_of(&app, "?kind=agent_finished").await;
    assert_eq!(new["turns"], 2);
    assert_eq!(new["reply"], "Done");
}

#[tokio::test]
async fn a_muted_notification_takes_no_later_event_until_it_is_unmuted() {
    let app = app();
    three(&app).await;
    let row = list(&app, "?q=Themes").await["notifications"][0].clone();
    let mute = format!("/api/v1/notifications/{}/mute", row["id"]);
    assert_eq!(
        call(&app, "PUT", &mute, None).await.0,
        StatusCode::NO_CONTENT
    );
    // muted is done, and out of the list
    assert!(keys(&list(&app, "").await).iter().all(|k| *k != "PIN-1"));
    let (_, of) = call(&app, "GET", "/api/v1/notifications/of?item=PIN-1", None).await;
    assert_eq!((&of["muted"], &of["done"]), (&json!(true), &json!(true)));

    // a later comment brings nothing back
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Started."}),
    )
    .await;
    assert!(keys(&list(&app, "").await).iter().all(|k| *k != "PIN-1"));

    // unmuted, the next event brings it back
    assert_eq!(
        call(&app, "DELETE", &mute, None).await.0,
        StatusCode::NO_CONTENT
    );
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Halfway."}),
    )
    .await;
    assert_eq!(keys(&list(&app, "?q=Themes").await), ["PIN-1"]);
    // an item with no notification has none to unmute
    let (code, _) = call(&app, "GET", "/api/v1/notifications/of?item=PIN-99", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_waiting_notification_is_not_muted() {
    let app = app();
    let review = agent(
        &app,
        "POST",
        "/api/v1/reviews",
        json!({"kind": "question", "title": "Deploy?",
               "origin": {"repo": REPO, "ref": "main"},
               "requested_by": "claude-code", "session": "s-1",
               "payload": {"questions": [{"id": "q1", "question": "Deploy?", "answer": "yes-no"}]}}),
    )
    .await;
    let row = list(&app, "?tab=waiting").await["notifications"][0].clone();
    assert_eq!(row["subject"]["review"]["id"], review["id"]);
    let (code, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/notifications/{}/mute", row["id"]),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT);
}

#[tokio::test]
async fn an_item_is_muted_from_its_page_before_it_has_a_notification() {
    let app = app();
    // the person's own item has no notification yet
    call(
        &app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": REPO, "title": "Themes"})),
    )
    .await;
    let (code, _) = call(&app, "GET", "/api/v1/notifications/of?item=PIN-1", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let mute = "/api/v1/notifications/of/mute?item=PIN-1";
    assert_eq!(
        call(&app, "PUT", mute, None).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, of) = call(&app, "GET", "/api/v1/notifications/of?item=PIN-1", None).await;
    assert_eq!(of["muted"], true);
    // an agent's comment adds nothing
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Started."}),
    )
    .await;
    assert_eq!(list(&app, "").await["total"], 0);

    assert_eq!(
        call(&app, "DELETE", mute, None).await.0,
        StatusCode::NO_CONTENT
    );
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Halfway."}),
    )
    .await;
    assert_eq!(keys(&list(&app, "").await), ["PIN-1"]);
}
