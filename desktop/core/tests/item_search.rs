//! Searching work items: ranked by how well their title, body and comments
//! match, closed items included, and several phrasings merged.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

const REPO: &str = "acme/api";

/// Adds an item as the person, and answers its key.
async fn add(app: &App, repo: &str, title: &str, body: &str) -> String {
    let (code, item) = call(
        app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": repo, "title": title, "body": body})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{item}");
    item["key"].as_str().unwrap().to_string()
}

async fn comment(app: &App, key: &str, body: &str) {
    let (code, c) = call(
        app,
        "POST",
        &format!("/api/v1/items/{key}/comments"),
        Some(json!({"body": body})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{c}");
}

/// The search's answer to `query`, a query string.
async fn search(app: &App, query: &str) -> Vec<Value> {
    let (code, found) = call(app, "GET", &format!("/api/v1/items/search?{query}"), None).await;
    assert_eq!(code, StatusCode::OK, "{found}");
    found["items"].as_array().unwrap().clone()
}

fn keys(found: &[Value]) -> Vec<&str> {
    found.iter().map(|f| f["key"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn items_are_ranked_by_how_well_their_words_match() {
    let app = app();
    let header = add(&app, REPO, "The session header flickers", "").await;
    let page = add(&app, REPO, "Slow session page", "Opening it takes seconds.").await;
    let other = add(&app, REPO, "Rename the settings file", "").await;
    comment(&app, &other, "The header of the file says nothing either.").await;
    let (code, _) = call(
        &app,
        "POST",
        &format!("/api/v1/items/{header}/done"),
        Some(json!({})),
    )
    .await;
    assert_eq!(code, StatusCode::OK);

    // a word in another form, in a closed item; any word may match, and the
    // item that matches more of them, in its title, comes first
    let found = search(&app, "q=flickering+session+header").await;
    assert_eq!(keys(&found), [&header, &page, &other]);
    assert_eq!(found[0]["status"], "done");
    assert_eq!(found[0]["matched"], "title");
    assert_eq!(found[0]["snippet"], "The [session] [header] [flickers]");
    assert!(found[0].get("body").is_none());
    assert_eq!(found[2]["matched"], "comment");
    assert!(found[0]["score"].as_f64().unwrap() > found[1]["score"].as_f64().unwrap());

    // the body counts, a number or key names its item, and the status narrows
    assert_eq!(keys(&search(&app, "q=seconds").await), [&page]);
    assert_eq!(keys(&search(&app, &format!("q={other}")).await)[0], other);
    assert_eq!(keys(&search(&app, "q=3").await), [&other]);
    assert_eq!(keys(&search(&app, "q=header&status=open").await), [&other]);
    // of two equal matches, the item not closed comes first
    assert_eq!(keys(&search(&app, "q=session&limit=1").await), [&page]);
}

#[tokio::test]
async fn several_phrasings_put_first_what_they_all_find() {
    let app = app();
    let render = add(&app, REPO, "Draw the terminal faster", "").await;
    let cpu = add(&app, REPO, "Lower the CPU use of the terminal", "").await;
    let unrelated = add(&app, REPO, "Lower the font size", "").await;

    let one = search(&app, "q=lower+cpu").await;
    assert_eq!(keys(&one), [&cpu, &unrelated]);
    let both = search(&app, "q=lower+cpu&q=terminal+drawing").await;
    assert_eq!(keys(&both), [&cpu, &render, &unrelated]);
}

#[tokio::test]
async fn a_search_keeps_to_its_repository_and_forgets_deleted_items() {
    let app = app();
    let mine = add(&app, REPO, "Wombat importer", "").await;
    let theirs = add(&app, "acme/web", "Wombat exporter", "").await;
    assert_eq!(
        keys(&search(&app, &format!("q=wombat&repo={REPO}")).await),
        [&mine]
    );
    assert_eq!(keys(&search(&app, "q=wombat").await).len(), 2);

    let (code, _) = call(&app, "DELETE", &format!("/api/v1/items/{theirs}"), None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(keys(&search(&app, "q=wombat").await), [&mine]);
}

#[tokio::test]
async fn a_search_needs_words_and_a_limit_in_range() {
    let app = app();
    for query in [
        "",
        "q=",
        "q=%20%20",
        "q=x&limit=0",
        "q=x&limit=101",
        "q=x&status=gone",
    ] {
        let (code, _) = call(&app, "GET", &format!("/api/v1/items/search?{query}"), None).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
    }
}
