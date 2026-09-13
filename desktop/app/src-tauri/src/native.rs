//! What the app does beyond the window: the menu-bar tray with the pending
//! reviews, a notification when one arrives, and the routes the shell is
//! sent to from the tray, the shortcut, a deep link or a second launch.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast::error::RecvError;
use wicket_core::api::AppState;
use wicket_core::db::Filters;
use wicket_core::events::{self, Notice};
use wicket_core::review::{Review, Status};

/// The shell listens for this and navigates to the payload.
pub const OPEN_EVENT: &str = "wicket:open";
const TRAY_ID: &str = "main";
const TRAY_ROWS: usize = 8;
const TRAY_TITLE_CHARS: usize = 48;

pub struct Native {
    pub state: Arc<AppState>,
    pub paused: AtomicBool,
    /// A route the shell has not picked up yet: it may still be loading.
    pending_route: Mutex<Option<String>>,
}

impl Native {
    pub fn new(state: Arc<AppState>) -> Self {
        Native {
            state,
            paused: AtomicBool::new(false),
            pending_route: Mutex::new(None),
        }
    }

    pub fn take_pending_route(&self) -> Option<String> {
        self.pending_route.lock().unwrap().take()
    }
}

/// Shows the window and sends the shell to a route.
pub fn open(app: &AppHandle, route: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(native) = app.try_state::<Native>() {
        *native.pending_route.lock().unwrap() = Some(route.to_string());
    }
    let _ = app.emit(OPEN_EVENT, route.to_string());
}

pub fn open_review(app: &AppHandle, id: &str) {
    open(app, &format!("/reviews/{id}"));
}

/// The oldest pending review, or the inbox when nothing is waiting.
pub fn open_next(app: &AppHandle) {
    let oldest = app
        .try_state::<Native>()
        .and_then(|native| pending(&native.state).pop());
    match oldest {
        Some(review) => open_review(app, &review.id),
        None => open(app, "/"),
    }
}

/// Where `wicket://reviews/<id>` and the HTTP URL the CLI prints lead.
pub fn route_for_url(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let path = if scheme == "wicket" {
        rest
    } else {
        rest.split_once('/').map(|(_, path)| path).unwrap_or("")
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut parts = path.trim_matches('/').split('/');
    match (parts.next(), parts.next()) {
        (Some("reviews"), Some(id)) if !id.is_empty() => Some(format!("/reviews/{id}")),
        (Some("") | Some("inbox") | None, _) => Some("/".to_string()),
        (Some("history"), _) => Some("/history".to_string()),
        (Some("plugins"), _) => Some("/plugins".to_string()),
        _ => None,
    }
}

/// Pending reviews, newest first, as the API lists them.
fn pending(state: &AppState) -> Vec<Review> {
    let filters = Filters {
        statuses: vec![Status::Pending],
        limit: 500,
        ..Filters::default()
    };
    state.reviews.list(&filters).unwrap_or_default()
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Wicket")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    refresh_tray(app);
    Ok(tray)
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "inbox" => open(app, "/"),
        "next" => open_next(app),
        "pause" => {
            if let Some(native) = app.try_state::<Native>() {
                native.paused.fetch_xor(true, Ordering::Relaxed);
            }
            refresh_tray(app);
        }
        "quit" => app.exit(0),
        other => {
            if let Some(id) = other.strip_prefix("review:") {
                open_review(app, id);
            }
        }
    }
}

/// Rebuilds the tray's title, tooltip and menu, and the Dock badge, from the
/// pending reviews.
/// Menus are main-thread objects on macOS; call this there.
pub fn refresh_tray(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let Some(native) = app.try_state::<Native>() else {
        return;
    };
    let pending = pending(&native.state);
    let count = pending.len();
    // an empty title, not None: None leaves the old title in place on macOS
    let _ = tray.set_title(Some(if count == 0 {
        String::new()
    } else {
        count.to_string()
    }));
    let _ = tray.set_tooltip(Some(match count {
        0 => "Wicket: nothing pending".to_string(),
        1 => "Wicket: 1 review pending".to_string(),
        n => format!("Wicket: {n} reviews pending"),
    }));
    if let Ok(menu) = menu(app, &pending, native.paused.load(Ordering::Relaxed)) {
        let _ = tray.set_menu(Some(menu));
    }
    // The Dock icon carries the count too, for a menu bar that hides the tray.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_badge_count((count > 0).then_some(count as i64));
    }
}

fn menu(app: &AppHandle, pending: &[Review], paused: bool) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    if pending.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "none",
            "Nothing pending",
            false,
            None::<&str>,
        )?)?;
    } else {
        let now = Utc::now();
        for review in pending.iter().rev().take(TRAY_ROWS) {
            menu.append(&MenuItem::with_id(
                app,
                format!("review:{}", review.id),
                tray_label(review, now),
                true,
                None::<&str>,
            )?)?;
        }
        if pending.len() > TRAY_ROWS {
            menu.append(&MenuItem::with_id(
                app,
                "more",
                format!("{} more in the inbox", pending.len() - TRAY_ROWS),
                false,
                None::<&str>,
            )?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "inbox",
        "Open inbox",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "next",
        "Open oldest review",
        true,
        Some("Alt+Shift+W"),
    )?)?;
    menu.append(&CheckMenuItem::with_id(
        app,
        "pause",
        "Pause notifications",
        true,
        paused,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit Wicket",
        true,
        Some("CmdOrCtrl+Q"),
    )?)?;
    Ok(menu)
}

/// "Dedup tickets on save · review · 1 blocker, 2 major · 3m"
fn tray_label(review: &Review, now: DateTime<Utc>) -> String {
    let mut parts = vec![
        truncate(&review.title, TRAY_TITLE_CHARS),
        review.plugin.clone(),
    ];
    if let Some(counts) = summary_counts(review.summary.as_ref()) {
        parts.push(counts);
    }
    parts.push(age(review.created_at, now));
    parts.join("  ·  ")
}

/// The summary's counts as "1 blocker, 2 major", when the review has them.
fn summary_counts(summary: Option<&serde_json::Value>) -> Option<String> {
    let counts = summary?.get("counts")?.as_array()?;
    let parts: Vec<String> = counts
        .iter()
        .filter_map(|pair| {
            let label = pair.get(0)?.as_str()?;
            let n = pair.get(1)?.as_u64()?;
            Some(format!("{n} {label}"))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

fn age(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = (now - at).num_seconds().max(0);
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 48 * 3600 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86400),
    }
}

/// Follows the server's events: the tray tracks the pending reviews and a
/// new one is announced, unless notifications are paused.
pub fn watch(app: AppHandle) {
    let Some(native) = app.try_state::<Native>() else {
        return;
    };
    let mut rx = native.state.reviews.bus().subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(notice) => {
                    if matches!(
                        notice.kind.as_str(),
                        events::CREATED | events::DECIDED | events::WITHDRAWN | events::EXPIRED
                    ) {
                        let handle = app.clone();
                        let _ = app.run_on_main_thread(move || refresh_tray(&handle));
                    }
                    if notice.kind == events::CREATED {
                        notify(&app, &notice);
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || refresh_tray(&handle));
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

fn notify(app: &AppHandle, notice: &Notice) {
    let Some(native) = app.try_state::<Native>() else {
        return;
    };
    if native.paused.load(Ordering::Relaxed) {
        return;
    }
    let Some(review) = &notice.review else {
        return;
    };
    let title = review["title"].as_str().unwrap_or("A review is waiting");
    let plugin = review["plugin"].as_str().unwrap_or("");
    let mut lines = Vec::new();
    if let Some(counts) = summary_counts(review.get("summary")) {
        lines.push(counts);
    }
    lines.push(match review["requested_by"].as_str() {
        Some(by) => format!("{plugin} · requested by {by}"),
        None => plugin.to_string(),
    });
    let body = lines.join("\n");
    #[cfg(target_os = "macos")]
    if crate::notify_mac::available() {
        let title = title.to_string();
        let id = notice.review_id.clone();
        let _ =
            app.run_on_main_thread(move || crate::notify_mac::notify(&title, &body, id.as_deref()));
        return;
    }
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        eprintln!("wicket: notification not shown: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_for_urls() {
        assert_eq!(
            route_for_url("wicket://reviews/r_01"),
            Some("/reviews/r_01".to_string())
        );
        assert_eq!(
            route_for_url("http://127.0.0.1:4747/reviews/r_01?x=1"),
            Some("/reviews/r_01".to_string())
        );
        assert_eq!(route_for_url("wicket://"), Some("/".to_string()));
        assert_eq!(route_for_url("wicket://inbox"), Some("/".to_string()));
        assert_eq!(
            route_for_url("wicket://history"),
            Some("/history".to_string())
        );
        assert_eq!(route_for_url("wicket://reviews/"), None);
        assert_eq!(route_for_url("wicket://settings"), None);
        assert_eq!(route_for_url("not a url"), None);
    }

    #[test]
    fn summary_counts_read_the_pairs() {
        let summary = serde_json::json!({ "counts": [["blocker", 1], ["major", 2]] });
        assert_eq!(
            summary_counts(Some(&summary)),
            Some("1 blocker, 2 major".to_string())
        );
        assert_eq!(summary_counts(Some(&serde_json::json!({}))), None);
        assert_eq!(summary_counts(None), None);
    }

    #[test]
    fn labels_are_short() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a title that is far too long", 12), "a title tha…");
    }
}
