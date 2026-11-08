//! Questions an agent asks with `pinrail ask`: a review of the kind
//! `question`, with no plugin, checked against the core's own schemas and
//! answered like any review.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

mod common;
use common::{App, app, call};

fn questions() -> Value {
    json!({
        "kind": "question",
        "title": "Deploy to staging or production? and 2 more",
        "origin": {"repo": "acme", "ref": "main"},
        "requested_by": "claude-code",
        "session": "s-1",
        "payload": {"questions": [
            {"id": "q1", "question": "Deploy to staging or production?", "answer": "choice",
             "choices": ["staging", "production"], "recommended": "staging",
             "context": "The release notes PR is still open."},
            {"id": "q2", "question": "Keep the old endpoint for a release?", "answer": "yes-no", "recommended": "yes"},
            {"id": "q3", "question": "What should the new endpoint be called?", "answer": "text"}
        ]}
    })
}

async fn ask(app: &App, body: Value) -> Value {
    let (status, review) = call(app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    review
}

/// The violations' paths, in order.
fn paths(body: &Value) -> Vec<String> {
    body["violations"]
        .as_array()
        .unwrap_or_else(|| panic!("no violations in {body}"))
        .iter()
        .map(|v| v["path"].as_str().unwrap().to_string())
        .collect()
}

async fn refused(app: &App, body: Value) -> Vec<String> {
    let (status, response) = call(app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    paths(&response)
}

async fn markdown(app: &App, id: &str) -> String {
    let request = Request::get(format!("/api/v1/reviews/{id}?format=markdown"))
        .header("host", "127.0.0.1:4747")
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn questions_are_asked_and_answered_like_a_review() {
    let app = app();
    let review = ask(&app, questions()).await;
    assert_eq!(review["kind"], "question");
    assert_eq!(review["plugin"], "");
    assert_eq!(review["plugin_bundle"], Value::Null);
    assert_eq!(review["status"], "pending");
    assert_eq!(review["summary"]["counts"][0]["label"], "questions");
    assert_eq!(review["summary"]["counts"][0]["count"], 3);
    let id = review["id"].as_str().unwrap();

    let answers = json!({"answers": [
        {"id": "q1", "choice": "staging", "note": "production after the release notes"},
        {"id": "q2", "yes": true},
        {"id": "q3", "unanswered": true}
    ]});
    let (status, decided) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/decision"),
        Some(json!({"data": answers})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    assert_eq!(decided["status"], "decided");
    assert_eq!(decided["decision"]["data"], answers);
    let counts = &decided["decision"]["summary"]["counts"];
    assert_eq!(
        counts[0],
        json!({"label": "answered", "count": 2, "tone": "success"})
    );
    assert_eq!(
        counts[1],
        json!({"label": "unanswered", "count": 1, "tone": "warning"})
    );

    let text = markdown(&app, id).await;
    for line in [
        "1. Deploy to staging or production?",
        "staging",
        "Note: production after the release notes",
        "2. Keep the old endpoint for a release?",
        "yes",
        "3. What should the new endpoint be called?",
        "Not answered.",
    ] {
        assert!(
            text.lines().any(|l| l.trim() == line),
            "no {line:?} in\n{text}"
        );
    }
}

#[tokio::test]
async fn every_kind_of_answer_is_taken() {
    let app = app();
    let mut body = questions();
    body["payload"]["questions"] = json!([
        {"id": "a", "question": "Which?", "answer": "choice", "choices": ["x, with a comma", "y"]},
        {"id": "b", "question": "Which ones?", "answer": "choices", "choices": ["x", "y", "z"]},
        {"id": "c", "question": "Sure?", "answer": "yes-no"},
        {"id": "d", "question": "Name?", "answer": "text"}
    ]);
    let id = ask(&app, body).await["id"].as_str().unwrap().to_string();
    let path = format!("/api/v1/reviews/{id}/decision");
    let answers = json!({"answers": [
        {"id": "a", "choice": "x, with a comma"},
        {"id": "b", "choices": ["x", "z"]},
        {"id": "c", "yes": false, "note": "not yet"},
        {"id": "d", "text": "v2"}
    ]});
    let (status, checked) = call(
        &app,
        "POST",
        &format!("{path}?dry_run=true"),
        Some(json!({"data": answers})),
    )
    .await;
    assert!(status.is_success(), "{checked}");
    // an answer in the person's own words, for a question with choices
    let own = json!({"answers": [
        {"id": "a", "text": "neither"},
        {"id": "b", "text": "all of them"},
        {"id": "c", "yes": true},
        {"id": "d", "text": "v2"}
    ]});
    let (status, decided) = call(&app, "POST", &path, Some(json!({"data": own}))).await;
    assert_eq!(status, StatusCode::OK, "{decided}");
}

#[tokio::test]
async fn questions_are_checked_before_they_are_asked() {
    let app = app();
    let with = |list: Value| {
        let mut body = questions();
        body["payload"]["questions"] = list;
        body
    };

    assert_eq!(refused(&app, with(json!([]))).await, ["/payload/questions"]);
    let eleven: Vec<Value> = (1..=11)
        .map(|n| json!({"id": format!("q{n}"), "question": "Why?", "answer": "text"}))
        .collect();
    assert_eq!(
        refused(&app, with(json!(eleven))).await,
        ["/payload/questions"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([
                {"id": "q1", "question": "Why?", "answer": "text"},
                {"id": "q1", "question": "How?", "answer": "text"}
            ]))
        )
        .await,
        ["/payload/questions/1/id"]
    );
    assert_eq!(
        refused(
            &app,
            with(
                json!([{"id": "q1", "question": "Which?", "answer": "choice", "choices": ["only"]}])
            )
        )
        .await,
        ["/payload/questions/0/choices"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([{"id": "q1", "question": "Which?", "answer": "choice", "choices": ["a", "a"]}]))
        )
        .await,
        ["/payload/questions/0/choices"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([{"id": "q1", "question": "Which?", "answer": "choice"}]))
        )
        .await,
        ["/payload/questions/0/choices"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([{"id": "q1", "question": "Sure?", "answer": "yes-no", "choices": ["a", "b"]}]))
        )
        .await,
        ["/payload/questions/0/choices"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([{"id": "q1", "question": "Which?", "answer": "choice", "choices": ["a", "b"], "recommended": "c"}]))
        )
        .await,
        ["/payload/questions/0/recommended"]
    );
    assert_eq!(
        refused(&app, with(json!([{"id": "q1", "question": "Sure?", "answer": "yes-no", "recommended": "maybe"}]))).await,
        ["/payload/questions/0/recommended"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([{"id": "q1", "question": "Name?", "answer": "text", "recommended": "v2"}]))
        )
        .await,
        ["/payload/questions/0/recommended"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([{"id": "q1", "question": "x".repeat(301), "answer": "text"}]))
        )
        .await,
        ["/payload/questions/0/question"]
    );
    assert_eq!(
        refused(
            &app,
            with(json!([{"id": "q1", "question": "Why?", "answer": "essay"}]))
        )
        .await,
        ["/payload/questions/0/answer"]
    );
}

#[tokio::test]
async fn a_question_and_a_plugin_review_do_not_mix() {
    let app = app();
    let mut with_plugin = questions();
    with_plugin["plugin"] = json!("list");
    assert_eq!(refused(&app, with_plugin).await, ["/plugin"]);

    let mut other = questions();
    other["kind"] = json!("message");
    assert_eq!(refused(&app, other).await, ["/kind"]);

    // a review is a plugin's, as before
    let mut no_plugin = questions();
    no_plugin.as_object_mut().unwrap().remove("kind");
    assert_eq!(refused(&app, no_plugin).await, ["/plugin"]);

    // questions are never revised: they are withdrawn and asked again
    let id = ask(&app, questions()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut revised = questions();
    revised["revises"] = json!(id);
    assert_eq!(refused(&app, revised).await, ["/revises"]);
}

#[tokio::test]
async fn answers_are_checked_against_their_questions() {
    let app = app();
    let id = ask(&app, questions()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/v1/reviews/{id}/decision");
    let refused = |answers: Value| {
        let app = &app;
        let path = path.clone();
        async move {
            let (status, response) = call(
                app,
                "POST",
                &path,
                Some(json!({"data": {"answers": answers}})),
            )
            .await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
            paths(&response)
        }
    };
    let q2 = json!({"id": "q2", "yes": true});
    let q3 = json!({"id": "q3", "text": "v2"});

    // every question has an answer, and only those
    assert_eq!(refused(json!([q2, q3])).await, ["/answers"]);
    assert_eq!(
        refused(json!([{"id": "q1", "choice": "staging"}, q2, q3, {"id": "q9", "text": "?"}]))
            .await,
        ["/answers/3/id"]
    );
    assert_eq!(
        refused(json!([{"id": "q1", "choice": "staging"}, {"id": "q1", "choice": "staging"}, q3]))
            .await,
        ["/answers/1/id"]
    );
    // a choice is one of the question's
    assert_eq!(
        refused(json!([{"id": "q1", "choice": "qa"}, q2, q3])).await,
        ["/answers/0/choice"]
    );
    // the answer fits the question
    assert_eq!(
        refused(json!([{"id": "q1", "yes": true}, q2, q3])).await,
        ["/answers/0"]
    );
    assert_eq!(
        refused(json!([{"id": "q1", "choice": "staging"}, {"id": "q2", "choice": "yes"}, q3]))
            .await,
        ["/answers/1"]
    );
    assert_eq!(
        refused(json!([{"id": "q1", "choice": "staging"}, q2, {"id": "q3", "choice": "v2"}])).await,
        ["/answers/2"]
    );
    // one answer each: not a choice and words at once
    assert_eq!(
        refused(json!([{"id": "q1", "choice": "staging", "text": "or both"}, q2, q3])).await,
        ["/answers/0"]
    );
}

#[tokio::test]
async fn asking_again_while_waiting_gives_the_same_questions() {
    let app = app();
    let first = ask(&app, questions()).await;
    let (status, again) = call(&app, "POST", "/api/v1/reviews", Some(questions())).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["id"], first["id"]);
}

#[tokio::test]
async fn reviews_are_listed_by_kind() {
    let app = app();
    let question = ask(&app, questions()).await["id"].clone();
    let (status, review) = call(
        &app,
        "POST",
        "/api/v1/reviews",
        Some(json!({"plugin": "list", "title": "t", "payload": {"groups": [{"title": "g", "items": [{"id": 1, "title": "x"}]}]}})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let ids = |listing: &Value| -> Vec<Value> {
        listing["reviews"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].clone())
            .collect()
    };
    let (_, questions) = call(&app, "GET", "/api/v1/reviews?kind=question", None).await;
    assert_eq!(ids(&questions), [question]);
    let (_, reviews) = call(&app, "GET", "/api/v1/reviews?kind=review", None).await;
    assert_eq!(ids(&reviews), [review["id"].clone()]);
    assert_eq!(reviews["reviews"][0]["kind"], "review");
}

#[tokio::test]
async fn a_question_is_refused_while_sessions_are_off() {
    let app = common::app_as_new();
    let question = json!({
        "kind": "question",
        "title": "Ship it?",
        "requested_by": "claude-code",
        "payload": { "questions": [{ "id": "q1", "question": "Ship it?", "answer": "yes-no" }] },
    });
    let (code, body) = call(&app, "POST", "/api/v1/reviews", Some(question.clone())).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "off");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("Settings › Labs"),
        "{body}"
    );
    let (code, _) = call(&app, "POST", "/api/v1/reviews/validate", Some(question)).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    // a review still goes through
    let (code, body) = call(
        &app,
        "POST",
        "/api/v1/reviews",
        Some(json!({"plugin": "list", "title": "t", "payload": {"groups": [{"title": "g", "items": [{"id": 1, "title": "a"}]}]}})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn questions_carry_the_reply_the_agent_suggests() {
    let app = app();
    let mut body = questions();
    body["payload"]["suggested_reply"] = json!("Staging, keep it, and call it /v2/themes.");
    let asked = ask(&app, body).await;
    let (_, read) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", asked["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(
        read["payload"]["suggested_reply"],
        "Staging, keep it, and call it /v2/themes."
    );
    // one line, of 500 characters at most
    let mut body = questions();
    body["payload"]["suggested_reply"] = json!("Two\nlines");
    assert_eq!(refused(&app, body).await, ["/payload/suggested_reply"]);
    let mut body = questions();
    body["payload"]["suggested_reply"] = json!("x".repeat(501));
    assert_eq!(refused(&app, body).await, ["/payload/suggested_reply"]);
}
