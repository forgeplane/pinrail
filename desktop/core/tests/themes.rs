//! `/api/v1/themes` against a fresh data directory: what is listed, how a
//! file is read, a new theme, and a change to the folder.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

fn write(app: &App, file: &str, text: &str) {
    std::fs::write(app.dir.path().join("themes").join(file), text).unwrap();
}

async fn theme(app: &App, id: &str) -> Value {
    let (status, body) = call(app, "GET", "/api/v1/themes", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["themes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn the_built_in_themes_are_listed_with_no_variables() {
    let app = app();
    let dark = theme(&app, "dark").await;
    assert_eq!(dark["name"], "Dark");
    assert_eq!(dark["scheme"], "dark");
    assert_eq!(dark["shell"], json!({}));
    assert_eq!(dark["plugin"], json!({}));
    assert_eq!(theme(&app, "light").await["scheme"], "light");
}

#[tokio::test]
async fn a_file_sets_what_it_names_and_inherits_the_rest() {
    let app = app();
    write(
        &app,
        "nord.json",
        r##"{"name": "Nord", "scheme": "dark", "colors": {"accent": "#88c0d0", "sidebar": "#2e3440", "diff-add-bg": "#00ff0020"}}"##,
    );
    let nord = theme(&app, "nord").await;
    assert_eq!(nord["source"], "user");
    assert_eq!(nord["error"], Value::Null);
    assert_eq!(
        nord["shell"],
        json!({"--accent": "#88c0d0", "--sidebar": "#2e3440"})
    );
    assert_eq!(
        nord["plugin"],
        json!({
            "--pinrail-accent": "#88c0d0",
            "--pinrail-info": "#88c0d0",
            "--pinrail-add-bg": "#00ff0020"
        })
    );
}

#[tokio::test]
async fn a_theme_needs_no_colours() {
    let app = app();
    write(
        &app,
        "plain.json",
        r#"{"name": "Plain", "scheme": "light"}"#,
    );
    let plain = theme(&app, "plain").await;
    assert_eq!(plain["error"], Value::Null);
    assert_eq!(plain["shell"], json!({}));
}

#[tokio::test]
async fn an_unknown_key_is_a_warning_and_a_bad_file_is_an_error() {
    let app = app();
    write(
        &app,
        "newer.json",
        r##"{"name": "Newer", "scheme": "dark", "colors": {"bg": "#000", "glow": "#fff"}}"##,
    );
    let newer = theme(&app, "newer").await;
    assert_eq!(newer["error"], Value::Null);
    assert_eq!(newer["shell"], json!({"--bg": "#000"}));
    assert_eq!(newer["warnings"], json!(["Unknown colour key `glow`."]));

    for (id, text, says) in [
        ("broken", "{", "not valid JSON"),
        ("nameless", r#"{"scheme": "dark"}"#, "`name`"),
        ("sepia", r#"{"name": "S", "scheme": "sepia"}"#, "`scheme`"),
        (
            "tracker",
            r#"{"name": "T", "scheme": "dark", "colors": {"bg": "url(http://x)"}}"#,
            "`bg` is not a valid colour",
        ),
    ] {
        write(&app, &format!("{id}.json"), text);
        let bad = theme(&app, id).await;
        assert!(bad["error"].as_str().unwrap().contains(says), "{bad}");
        assert_eq!(bad["scheme"], Value::Null);
        assert_eq!(bad["shell"], json!({}));
    }
}

#[tokio::test]
async fn a_large_file_is_refused() {
    let app = app();
    let padding = "x".repeat(70 * 1024);
    write(
        &app,
        "big.json",
        &format!(r#"{{"name": "Big", "scheme": "dark", "note": "{padding}"}}"#),
    );
    let big = theme(&app, "big").await;
    assert!(big["error"].as_str().unwrap().contains("64 KiB"), "{big}");
}

#[tokio::test]
async fn the_built_in_ids_are_reserved() {
    let app = app();
    write(&app, "dark.json", r#"{"name": "Mine", "scheme": "dark"}"#);
    let (_, body) = call(&app, "GET", "/api/v1/themes", None).await;
    let dark: Vec<&Value> = body["themes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["id"] == "dark")
        .collect();
    assert_eq!(dark.len(), 2, "the built-in theme and the refused file");
    assert_eq!(dark[0]["source"], "bundled");
    assert!(dark[1]["error"].as_str().unwrap().contains("reserved"));
}

#[tokio::test]
async fn a_new_theme_is_a_complete_copy() {
    let app = app();
    write(
        &app,
        "nord.json",
        r##"{"name": "Nord", "scheme": "dark", "colors": {"accent": "#88c0d0"}}"##,
    );
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/themes",
        Some(json!({"id": "my-nord", "from": "nord"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let path = body["path"].as_str().unwrap();
    assert!(path.ends_with("my-nord.json"), "{path}");

    let copy: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(copy["name"], "My nord");
    assert_eq!(copy["scheme"], "dark");
    assert_eq!(copy["colors"]["accent"], "#88c0d0");
    assert_eq!(copy["colors"]["bg"], "#18191b", "from the built-in theme");
    assert_eq!(copy["colors"].as_object().unwrap().len(), 38);
    assert_eq!(theme(&app, "my-nord").await["error"], Value::Null);
}

#[tokio::test]
async fn a_new_theme_keeps_what_is_there() {
    let app = app();
    let post = |id: &'static str, from: &'static str| {
        let body = json!({"id": id, "from": from});
        let app = &app;
        async move { call(app, "POST", "/api/v1/themes", Some(body)).await }
    };
    assert_eq!(post("mine", "dark").await.0, StatusCode::CREATED);
    assert_eq!(post("mine", "light").await.0, StatusCode::CONFLICT);
    assert_eq!(
        post("dark", "light").await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post("../up", "dark").await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(post("other", "nothing").await.0, StatusCode::NOT_FOUND);
    assert_eq!(theme(&app, "mine").await["scheme"], "dark");
}

#[tokio::test]
async fn a_change_to_the_folder_is_announced() {
    let app = app();
    let mut rx = app.state.events().subscribe();
    assert!(!app.state.themes().reload().unwrap());

    write(&app, "nord.json", r#"{"name": "Nord", "scheme": "dark"}"#);
    assert!(app.state.themes().reload().unwrap());
    assert_eq!(rx.try_recv().unwrap().kind, "themes_changed");
    assert!(!app.state.themes().reload().unwrap());

    std::fs::remove_file(app.dir.path().join("themes/nord.json")).unwrap();
    assert!(app.state.themes().reload().unwrap());
}

#[tokio::test]
async fn each_scheme_names_a_theme_that_need_not_exist() {
    let app = app();
    let (_, before) = call(&app, "GET", "/api/v1/settings", None).await;
    assert_eq!(before["appearance"]["dark_theme"], "dark");
    assert_eq!(before["appearance"]["light_theme"], "light");

    let (status, after) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"appearance": {"dark_theme": "nord"}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["appearance"]["dark_theme"], "nord");

    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"appearance": {"light_theme": "Not An Id"}})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
