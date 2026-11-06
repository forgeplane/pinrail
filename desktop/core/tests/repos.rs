//! Repositories: a row for each name a review, a session or a project gives,
//! which the reviews and sessions of that repository refer to, and which
//! holds the repository's project.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

fn review(repo: Option<&str>, session: &str, title: &str) -> Value {
    let origin = match repo {
        Some(repo) => json!({"repo": repo, "ref": "main"}),
        None => json!({}),
    };
    json!({
        "plugin": "list",
        "title": title,
        "origin": origin,
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

async fn status(app: &App, agent: &str, session: &str, repo: Option<&str>, text: &str) {
    let mut body = json!({"text": text});
    if let Some(repo) = repo {
        body["origin"] = json!({"repo": repo});
    }
    let path = format!("/api/v1/sessions/{agent}/{session}/status");
    let (code, answer) = call(app, "POST", &path, Some(body)).await;
    assert_eq!(code, StatusCode::OK, "{answer}");
}

/// The database, opened beside the app, to read what the API does not show.
fn sql(app: &App) -> rusqlite::Connection {
    rusqlite::Connection::open(app.state.config().db_path()).unwrap()
}

fn strings(app: &App, query: &str) -> Vec<String> {
    let conn = sql(app);
    let mut stmt = conn.prepare(query).unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Each review's title and its repository's name, by title.
fn review_repos(app: &App) -> Vec<String> {
    strings(
        app,
        "SELECT r.title || ' ' || IFNULL(g.name, '-') FROM reviews r \
         LEFT JOIN repos g ON g.id = r.repo_id ORDER BY r.title",
    )
}

fn titles(listed: &Value) -> Vec<String> {
    let mut titles: Vec<String> = listed["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap().to_string())
        .collect();
    titles.sort();
    titles
}

#[tokio::test]
async fn a_review_adds_its_repository_the_first_time_it_is_named() {
    let app = app();
    send(&app, review(Some("acme/api"), "s-1", "First")).await;
    send(&app, review(Some("acme/api"), "s-1", "Second")).await;
    send(&app, review(Some("acme/web"), "s-2", "Web")).await;
    send(&app, review(None, "s-3", "Nowhere")).await;
    send(&app, review(Some(""), "s-4", "Empty")).await;

    assert_eq!(
        strings(&app, "SELECT name FROM repos ORDER BY name"),
        ["acme/api", "acme/web"]
    );
    assert_eq!(
        review_repos(&app),
        [
            "Empty -",
            "First acme/api",
            "Nowhere -",
            "Second acme/api",
            "Web acme/web"
        ]
    );
}

#[tokio::test]
async fn a_session_follows_the_repository_it_last_reported() {
    let app = app();
    status(&app, "codex", "s-1", Some("acme/api"), "Starting").await;
    status(&app, "codex", "s-1", None, "Still here").await;
    let repo_of = |app: &App| {
        strings(
            app,
            "SELECT IFNULL(g.name, '-') FROM sessions s LEFT JOIN repos g ON g.id = s.repo_id \
             WHERE s.id = 's-1'",
        )
    };
    // a status naming no repository leaves the session where it was
    assert_eq!(repo_of(&app), ["acme/api"]);

    status(&app, "codex", "s-1", Some("acme/web"), "Moved").await;
    assert_eq!(repo_of(&app), ["acme/web"]);
    let (_, read) = call(&app, "GET", "/api/v1/sessions/codex/s-1", None).await;
    assert_eq!(read["session"]["repo"], "acme/web");
    // the repository it left keeps its row
    assert_eq!(
        strings(&app, "SELECT name FROM repos ORDER BY name"),
        ["acme/api", "acme/web"]
    );
}

#[tokio::test]
async fn reviews_naming_a_new_repository_at_once_share_one_row() {
    let app = app();
    let mut sending = Vec::new();
    for i in 0..10 {
        let router = app.router.clone();
        sending.push(tokio::spawn(async move {
            use axum::body::Body;
            use axum::http::Request;
            use tower::ServiceExt;
            let request = Request::post("/api/v1/reviews")
                .header("content-type", "application/json")
                .body(Body::from(
                    review(Some("acme/new"), &format!("s-{i}"), &format!("R{i}")).to_string(),
                ))
                .unwrap();
            router.oneshot(request).await.unwrap().status()
        }));
    }
    for sent in sending {
        assert_eq!(sent.await.unwrap(), StatusCode::CREATED);
    }
    assert_eq!(strings(&app, "SELECT name FROM repos"), ["acme/new"]);
    assert_eq!(
        strings(
            &app,
            "SELECT COUNT(*) || '' FROM reviews WHERE repo_id = (SELECT id FROM repos)"
        ),
        ["10"]
    );
}

#[tokio::test]
async fn the_filters_read_the_repository_of_each_review_and_session() {
    let app = app();
    send(&app, review(Some("acme/api"), "s-api", "Api")).await;
    send(&app, review(Some("acme/web"), "s-web", "Web")).await;
    send(&app, review(None, "s-none", "Nowhere")).await;
    let (_, project) = call(
        &app,
        "POST",
        "/api/v1/projects",
        Some(json!({"name": "Acme", "repos": ["acme/api"]})),
    )
    .await;
    let id = project["id"].as_str().unwrap();

    let reviews = |query: String| {
        let app = &app;
        async move {
            let (code, listed) = call(app, "GET", &format!("/api/v1/reviews?{query}"), None).await;
            assert_eq!(code, StatusCode::OK, "{listed}");
            titles(&listed)
        }
    };
    assert_eq!(reviews("repo=acme%2Fweb".into()).await, ["Web"]);
    assert_eq!(reviews(format!("project={id}")).await, ["Api"]);
    assert_eq!(reviews("repo=-".into()).await, ["Nowhere"]);
    assert!(reviews("repo=acme%2Funknown".into()).await.is_empty());

    let (_, listed) = call(&app, "GET", "/api/v1/reviews?include=facets", None).await;
    assert_eq!(listed["facets"]["repos"], json!(["acme/api", "acme/web"]));
    assert_eq!(listed["facets"]["unassigned"], true);

    let sessions = |query: String| {
        let app = &app;
        async move {
            let (code, listed) = call(app, "GET", &format!("/api/v1/sessions?{query}"), None).await;
            assert_eq!(code, StatusCode::OK, "{listed}");
            let mut ids: Vec<String> = listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["id"].as_str().unwrap().to_string())
                .collect();
            ids.sort();
            ids
        }
    };
    assert_eq!(sessions("repo=acme%2Fweb".into()).await, ["s-web"]);
    assert_eq!(sessions(format!("project={id}")).await, ["s-api"]);
    assert_eq!(sessions("repo=-".into()).await, ["s-none"]);
}

#[tokio::test]
async fn a_repository_out_of_every_project_and_with_nothing_in_it_is_not_listed() {
    let app = app();
    let (_, project) = call(
        &app,
        "POST",
        "/api/v1/projects",
        Some(json!({"name": "Acme", "repos": ["acme/planned"]})),
    )
    .await;
    // a repository nothing has sent from is listed in its project
    let (_, listing) = call(&app, "GET", "/api/v1/projects", None).await;
    assert_eq!(listing["projects"][0]["repos"][0]["repo"], "acme/planned");

    // out of the project, it has nothing to show
    let (code, _) = call(
        &app,
        "PUT",
        "/api/v1/repos/acme%2Fplanned/project",
        Some(json!({"project_id": null})),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    let (_, listing) = call(&app, "GET", "/api/v1/projects", None).await;
    assert!(!listing.to_string().contains("acme/planned"), "{listing}");

    // deleting a project leaves its repositories, in no project
    send(&app, review(Some("acme/api"), "s-1", "Api")).await;
    let id = project["id"].as_str().unwrap();
    call(
        &app,
        "PUT",
        "/api/v1/repos/acme%2Fapi/project",
        Some(json!({"project_id": id})),
    )
    .await;
    let (code, _) = call(&app, "DELETE", &format!("/api/v1/projects/{id}"), None).await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    let (_, listing) = call(&app, "GET", "/api/v1/projects", None).await;
    assert_eq!(listing["loose"][0]["repo"], "acme/api");
    assert_eq!(
        strings(
            &app,
            "SELECT name || ' ' || IFNULL(project_id, '-') FROM repos WHERE name = 'acme/api'"
        ),
        ["acme/api -"]
    );
}

#[tokio::test]
async fn deleting_a_repository_deletes_its_rounds_files_events_and_sessions_only() {
    let app = app();
    let first = send(&app, review(Some("acme/api"), "s-api", "Round 1")).await;
    let mut second = review(Some("acme/api"), "s-api", "Round 2");
    second["revises"] = json!(first);
    let second = send(&app, second).await;
    status(&app, "claude-code", "s-api", Some("acme/api"), "Working").await;
    let kept = send(&app, review(Some("acme/web"), "s-web", "Web")).await;

    let (code, deleted) = call(&app, "DELETE", "/api/v1/repos/acme%2Fapi", None).await;
    assert_eq!(code, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["reviews"], 2);
    assert_eq!(deleted["sessions"], 1);

    for table in ["reviews", "review_payloads", "events", "outcomes"] {
        let column = if table == "reviews" {
            "id"
        } else {
            "review_id"
        };
        let left = strings(
            &app,
            &format!("SELECT {column} FROM {table} WHERE {column} IN ('{first}', '{second}')"),
        );
        assert!(left.is_empty(), "{table}: {left:?}");
    }
    assert!(
        strings(
            &app,
            "SELECT id FROM sessions WHERE id = 's-api' \
             UNION ALL SELECT session FROM session_statuses WHERE session = 's-api'"
        )
        .is_empty()
    );
    assert_eq!(strings(&app, "SELECT name FROM repos"), ["acme/web"]);
    let (code, _) = call(&app, "GET", &format!("/api/v1/reviews/{kept}"), None).await;
    assert_eq!(code, StatusCode::OK);
    let (code, _) = call(&app, "GET", "/api/v1/sessions/claude-code/s-web", None).await;
    assert_eq!(code, StatusCode::OK);

    // a review sent later from the same repository starts it again
    send(&app, review(Some("acme/api"), "s-again", "Again")).await;
    assert_eq!(
        strings(&app, "SELECT name FROM repos ORDER BY name"),
        ["acme/api", "acme/web"]
    );
    assert_eq!(review_repos(&app), ["Again acme/api", "Web acme/web"]);
}
