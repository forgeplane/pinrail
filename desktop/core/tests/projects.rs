//! Projects: named sets of repositories the person makes in the app. A
//! review or a session names only its repository, and carries the project
//! that repository is in when it is read.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

fn review(repo: &str, session: &str, title: &str) -> Value {
    json!({
        "plugin": "list",
        "title": title,
        "origin": {"repo": repo, "ref": "main"},
        "requested_by": "claude-code",
        "session": session,
        "payload": {"groups": [{"title": "g", "items": [{"id": 1, "title": "x"}]}]}
    })
}

async fn send(app: &App, body: Value) -> String {
    let (status, sent) = call(app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    sent["id"].as_str().unwrap().to_string()
}

async fn create(app: &App, body: Value) -> Value {
    let (status, project) = call(app, "POST", "/api/v1/projects", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{project}");
    project
}

async fn listing(app: &App) -> Value {
    let (status, listing) = call(app, "GET", "/api/v1/projects", None).await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    listing
}

async fn move_repo(app: &App, repo: &str, project: Value) -> StatusCode {
    let path = format!("/api/v1/repos/{}/project", repo.replace('/', "%2F"));
    call(app, "PUT", &path, Some(json!({ "project_id": project })))
        .await
        .0
}

fn repos(project: &Value) -> Vec<&str> {
    project["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["repo"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn a_project_groups_its_repositories_and_lists_the_others_apart() {
    let app = app();
    send(&app, review("forgeplane/pinrail", "s-1", "Schema")).await;
    send(&app, review("forgeplane/pinrail-infra", "s-2", "Bucket")).await;
    send(&app, review("acme/api", "s-3", "Rate limits")).await;

    let project = create(
        &app,
        json!({"name": "  Pinrail ", "repos": ["forgeplane/pinrail", "forgeplane/pinrail-infra"]}),
    )
    .await;
    assert!(project["id"].as_str().unwrap().starts_with("p_"));
    assert_eq!(project["name"], "Pinrail");
    assert_eq!(
        repos(&project),
        ["forgeplane/pinrail", "forgeplane/pinrail-infra"]
    );
    // what waits in it, from both repositories
    assert_eq!(project["waiting"], 2);
    assert_eq!(project["sessions"], 2);
    assert!(project["last_at"].is_string());

    let listing = listing(&app).await;
    assert_eq!(listing["projects"].as_array().unwrap().len(), 1);
    let loose: Vec<&str> = listing["loose"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["repo"].as_str().unwrap())
        .collect();
    assert_eq!(loose, ["acme/api"]);
    assert_eq!(listing["loose"][0]["waiting"], 1);
}

#[tokio::test]
async fn reviews_and_sessions_name_their_project_and_filter_by_it() {
    let app = app();
    let schema = send(&app, review("forgeplane/pinrail", "s-1", "Schema")).await;
    let bucket = send(&app, review("forgeplane/pinrail-infra", "s-2", "Bucket")).await;
    send(&app, review("acme/api", "s-3", "Rate limits")).await;
    let project = create(
        &app,
        json!({"name": "Pinrail", "repos": ["forgeplane/pinrail"]}),
    )
    .await;
    let id = project["id"].as_str().unwrap();

    // the review sent before the project existed names it now
    let (_, read) = call(&app, "GET", &format!("/api/v1/reviews/{schema}"), None).await;
    assert_eq!(read["project"], json!({"id": id, "name": "Pinrail"}));
    let (_, read) = call(&app, "GET", &format!("/api/v1/reviews/{bucket}"), None).await;
    assert_eq!(read["project"], Value::Null);
    // and one sent after it names it as it is created
    let sent = call(
        &app,
        "POST",
        "/api/v1/reviews",
        Some(review("forgeplane/pinrail", "s-1", "Docs")),
    )
    .await
    .1;
    assert_eq!(sent["project"]["id"], id);

    // a repository added later brings its reviews and sessions with it
    assert_eq!(
        move_repo(&app, "forgeplane/pinrail-infra", json!(id)).await,
        StatusCode::NO_CONTENT
    );
    let (_, listed) = call(&app, "GET", &format!("/api/v1/reviews?project={id}"), None).await;
    let mut titles: Vec<&str> = listed["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    titles.sort();
    assert_eq!(titles, ["Bucket", "Docs", "Schema"]);
    assert_eq!(listed["total"], 3);

    let (_, sessions) = call(&app, "GET", &format!("/api/v1/sessions?project={id}"), None).await;
    let mut ids: Vec<&str> = sessions["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, ["s-1", "s-2"]);
    assert_eq!(sessions["sessions"][0]["project"]["name"], "Pinrail");

    let (_, session) = call(&app, "GET", "/api/v1/sessions/claude-code/s-3", None).await;
    assert_eq!(session["session"]["project"], Value::Null);
}

#[tokio::test]
async fn a_repository_is_in_one_project_and_moves_between_them() {
    let app = app();
    send(&app, review("forgeplane/pinrail", "s-1", "Schema")).await;
    let pinrail = create(
        &app,
        json!({"name": "Pinrail", "repos": ["forgeplane/pinrail"]}),
    )
    .await;
    let other = create(&app, json!({"name": "Other"})).await;
    let other_id = other["id"].as_str().unwrap();

    // moving it into another project takes it out of the first
    assert_eq!(
        move_repo(&app, "forgeplane/pinrail", json!(other_id)).await,
        StatusCode::NO_CONTENT
    );
    let listing = listing(&app).await;
    let by_name = |name: &str| {
        listing["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(repos(&by_name("Other")), ["forgeplane/pinrail"]);
    assert!(repos(&by_name("Pinrail")).is_empty());

    // a new project with a repository of another takes it too
    create(
        &app,
        json!({"name": "Third", "repos": ["forgeplane/pinrail"]}),
    )
    .await;
    let listing = self::listing(&app).await;
    let owners: Vec<&str> = listing["projects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| repos(p).contains(&"forgeplane/pinrail"))
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(owners, ["Third"]);

    // out of every project, it is listed apart; a repository typed in that
    // nothing has sent from yet can be put in one too
    assert_eq!(
        move_repo(&app, "forgeplane/pinrail", Value::Null).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        move_repo(&app, "acme/web", json!(pinrail["id"])).await,
        StatusCode::NO_CONTENT
    );
    let listing = self::listing(&app).await;
    assert_eq!(listing["loose"][0]["repo"], "forgeplane/pinrail");
    assert_eq!(listing["loose"][0]["sessions"], 1);
    assert_eq!(
        repos(
            listing["projects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "Pinrail")
                .unwrap()
        ),
        ["acme/web"]
    );

    assert_eq!(
        move_repo(&app, "acme/web", json!("p_unknown")).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_project_is_renamed_and_deleted_without_touching_its_work() {
    let app = app();
    let review = send(&app, review("forgeplane/pinrail", "s-1", "Schema")).await;
    let project = create(
        &app,
        json!({"name": "Pinrail", "repos": ["forgeplane/pinrail"]}),
    )
    .await;
    let id = project["id"].as_str().unwrap();

    let (status, renamed) = call(
        &app,
        "PATCH",
        &format!("/api/v1/projects/{id}"),
        Some(json!({"name": "Pinrail app"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed["name"], "Pinrail app");
    let (_, read) = call(&app, "GET", &format!("/api/v1/reviews/{review}"), None).await;
    assert_eq!(read["project"]["name"], "Pinrail app");

    let (status, _) = call(&app, "DELETE", &format!("/api/v1/projects/{id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, read) = call(&app, "GET", &format!("/api/v1/reviews/{review}"), None).await;
    assert_eq!(read["project"], Value::Null);
    assert_eq!(read["title"], "Schema");
    let listing = listing(&app).await;
    assert_eq!(listing["projects"], json!([]));
    assert_eq!(listing["loose"][0]["repo"], "forgeplane/pinrail");

    for (method, body) in [("PATCH", Some(json!({"name": "x"}))), ("DELETE", None)] {
        let (status, _) = call(&app, method, &format!("/api/v1/projects/{id}"), body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
}

#[tokio::test]
async fn a_name_must_be_given_short_and_unused() {
    let app = app();
    create(&app, json!({"name": "Pinrail"})).await;
    let long = "x".repeat(81);
    for (body, path) in [
        (json!({"name": "pinrail"}), "/name"),
        (json!({"name": "   "}), "/name"),
        (json!({"name": long}), "/name"),
        (json!({}), "/name"),
        (json!({"name": "Ok", "repos": ["a", " "]}), "/repos/1"),
        (json!({"name": "Ok", "repos": "a"}), "/repos"),
    ] {
        let (status, refused) = call(&app, "POST", "/api/v1/projects", Some(body.clone())).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {refused}"
        );
        assert!(refused.to_string().contains(path), "{body}: {refused}");
    }
    // nothing was created by a refused request
    assert_eq!(listing(&app).await["projects"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn every_change_is_announced() {
    let app = app();
    let kinds = |app: &App| {
        app.state
            .events()
            .after(0, 100)
            .unwrap()
            .into_iter()
            .filter(|n| n.kind == "projects_changed")
            .count()
    };
    let project = create(&app, json!({"name": "Pinrail"})).await;
    let id = project["id"].as_str().unwrap();
    move_repo(&app, "acme/api", json!(id)).await;
    call(
        &app,
        "PATCH",
        &format!("/api/v1/projects/{id}"),
        Some(json!({"name": "P"})),
    )
    .await;
    call(&app, "DELETE", &format!("/api/v1/projects/{id}"), None).await;
    assert_eq!(kinds(&app), 4);
}

#[tokio::test]
async fn a_project_has_an_optional_description_that_can_be_changed_or_removed() {
    let app = app();
    let project = create(
        &app,
        json!({"name": "Pinrail", "description": "  The app, its plugins and its site  "}),
    )
    .await;
    assert_eq!(project["description"], "The app, its plugins and its site");
    let id = project["id"].as_str().unwrap();
    let path = format!("/api/v1/projects/{id}");

    // a new description leaves the name as it is
    let (status, changed) = call(
        &app,
        "PATCH",
        &path,
        Some(json!({"description": "The app"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["name"], "Pinrail");
    assert_eq!(changed["description"], "The app");
    // and a new name the description
    let (_, changed) = call(&app, "PATCH", &path, Some(json!({"name": "Pinrail app"}))).await;
    assert_eq!(changed["description"], "The app");
    // empty or null removes it
    for gone in [json!(""), Value::Null] {
        call(&app, "PATCH", &path, Some(json!({"description": "again"}))).await;
        let (_, changed) = call(&app, "PATCH", &path, Some(json!({ "description": gone }))).await;
        assert_eq!(changed["description"], Value::Null, "{gone}");
    }
    // one without a description has none
    let other = create(&app, json!({"name": "Other"})).await;
    assert_eq!(other["description"], Value::Null);

    let long = "x".repeat(281);
    let (status, refused) = call(&app, "PATCH", &path, Some(json!({ "description": long }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.to_string().contains("/description"), "{refused}");
    let (status, _) = call(&app, "PATCH", &path, Some(json!({}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
