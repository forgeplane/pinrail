//! What the app does beyond the window: the menu-bar tray with the pending
//! reviews, a notification when one arrives, and the routes the shell is
//! sent to from the tray, the shortcut, a deep link or a second launch.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use pinrail_core::Pinrail;
use pinrail_core::events::{self, Notice};
use pinrail_core::reviews::{Filters, Review, Status};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast::error::RecvError;

/// The shell listens for this and navigates to the payload.
pub const OPEN_EVENT: &str = "pinrail:open";
/// The global shortcut as registered, or why it is not; the shell shows it.
pub const SHORTCUT_EVENT: &str = "pinrail:shortcut";
/// What the shortcut is when the setting is unreadable.
const DEFAULT_SHORTCUT: &str = "alt+shift+w";
const TRAY_ID: &str = "main";
const TRAY_ROWS: usize = 8;
const TRAY_TITLE_CHARS: usize = 48;

pub struct Native {
    pub state: Arc<Pinrail>,
    /// A route the shell has not picked up yet: it may still be loading.
    pending_route: Mutex<Option<String>>,
    /// The global shortcut as last registered.
    shortcut: Mutex<ShortcutState>,
}

impl Native {
    pub fn new(state: Arc<Pinrail>) -> Self {
        Native {
            state,
            pending_route: Mutex::new(None),
            shortcut: Mutex::new(ShortcutState {
                shortcut: DEFAULT_SHORTCUT.to_string(),
                error: None,
            }),
        }
    }

    pub fn take_pending_route(&self) -> Option<String> {
        self.pending_route.lock().unwrap().take()
    }

    pub fn shortcut_state(&self) -> ShortcutState {
        self.shortcut.lock().unwrap().clone()
    }
}

/// The keys the app listens for everywhere, and the error when the system
/// would not give them.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ShortcutState {
    pub shortcut: String,
    pub error: Option<String>,
}

fn shortcut_keys(state: &Pinrail) -> String {
    state
        .settings()
        .value("/shortcut/global")
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_SHORTCUT)
        .to_string()
}

/// Registers the global shortcut from the settings in place of the last
/// one, records how that went and tells the shell. Main thread.
pub fn apply_shortcut(app: &AppHandle, state: &Pinrail) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState as Pressed};
    let keys = shortcut_keys(state);
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let result = shortcuts.on_shortcut(keys.as_str(), |app, _shortcut, event| {
        if event.state() == Pressed::Pressed {
            open_from_shortcut(app);
        }
    });
    let error = result.err().map(|error| error.to_string());
    if let Some(error) = &error {
        eprintln!("pinrail: the shortcut {keys} is not registered: {error}");
    }
    let registered = ShortcutState {
        shortcut: keys,
        error,
    };
    if let Some(native) = app.try_state::<Native>() {
        *native.shortcut.lock().unwrap() = registered.clone();
    }
    let _ = app.emit(SHORTCUT_EVENT, registered);
    refresh_tray(app);
}

/// What the shortcut opens: the oldest pending review, or the inbox when
/// the setting says so or nothing is pending.
pub fn open_from_shortcut(app: &AppHandle) {
    let inbox = app
        .try_state::<Native>()
        .is_some_and(|native| native.state.settings().value("/shortcut/global_opens") == "inbox");
    if inbox {
        open(app, "/");
    } else {
        open_next(app);
    }
}

/// How notifications stand, from the settings: off, paused until a moment
/// still to come, or on; and whether they sound.
struct NotificationSettings {
    enabled: bool,
    paused_until: Option<DateTime<Utc>>,
    sound: bool,
    muted_plugins: Vec<String>,
}

fn notification_settings(state: &Pinrail) -> NotificationSettings {
    let s = state.settings().get();
    let n = &s["notifications"];
    let paused_until = n["paused_until"]
        .as_str()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc))
        .filter(|t| *t > Utc::now());
    NotificationSettings {
        enabled: n["enabled"].as_bool().unwrap_or(true),
        paused_until,
        sound: n["sound"].as_bool().unwrap_or(true),
        muted_plugins: n["muted_plugins"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Pauses notifications until a moment, or resumes them with `None`. The
/// setting is what the dialog shows too; the tray follows through the
/// change event, like any other way of setting it.
fn pause_notifications(state: &Pinrail, until: Option<DateTime<Utc>>) {
    let value = until.map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    if let Err(error) = state
        .settings()
        .change(&serde_json::json!({ "notifications": { "paused_until": value } }))
    {
        eprintln!("pinrail: pause not recorded: {error}");
    }
}

/// Refreshes the tray when a pause runs out, so "Resume" gives way to
/// "Pause" without anyone touching a setting. One task for the app's life,
/// checking the wall clock: a timer stops while a Mac sleeps, and a task
/// per change of pause would pile up.
pub fn watch_pause_end(app: &AppHandle, state: Arc<Pinrail>) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // a pause the tray may still be showing
        let mut shown = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let until = notification_settings(&state).paused_until;
            if until.is_some_and(|until| until > Utc::now()) {
                shown = true;
            } else if std::mem::take(&mut shown) {
                let again = handle.clone();
                let _ = handle.run_on_main_thread(move || refresh_tray(&again));
            }
        }
    });
}

/// The start of tomorrow, local time.
fn tomorrow() -> DateTime<Utc> {
    use chrono::TimeZone;
    let local = chrono::Local::now().date_naive() + chrono::Days::new(1);
    chrono::Local
        .from_local_datetime(&local.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(12))
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

/// Where `pinrail://reviews/<id>` and the HTTP URL the CLI prints lead.
pub fn route_for_url(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let path = if scheme == "pinrail" {
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

/// The tray icon shown or hidden, as settings say; the Dock icon stays
/// either way. Applied at start and whenever the setting changes.
pub fn apply_menu_bar_icon(app: &AppHandle, state: &Pinrail) {
    let shown = state
        .settings()
        .value("/menu_bar_icon")
        .as_bool()
        .unwrap_or(true);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(shown);
    }
}

/// Pending reviews, newest first, as the API lists them.
fn pending(state: &Pinrail) -> Vec<Review> {
    let filters = Filters {
        statuses: vec![Status::Pending],
        limit: 500,
        ..Filters::default()
    };
    state.reviews().list(&filters).unwrap_or_default()
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Pinrail")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    refresh_tray(app);
    Ok(tray)
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "inbox" => open(app, "/"),
        "next" => open_from_shortcut(app),
        "pause:15" | "pause:60" | "pause:tomorrow" | "resume" => {
            if let Some(native) = app.try_state::<Native>() {
                let until = match id {
                    "pause:15" => Some(Utc::now() + chrono::Duration::minutes(15)),
                    "pause:60" => Some(Utc::now() + chrono::Duration::hours(1)),
                    "pause:tomorrow" => Some(tomorrow()),
                    _ => None,
                };
                pause_notifications(&native.state, until);
            }
        }
        "quit" => app.exit(0),
        // off the main thread, as the window's Restart to update is
        "update-restart" => {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = crate::updater::restart(&app) {
                    eprintln!("pinrail: the update could not be installed: {error}");
                }
            });
        }
        "update-download" => {
            use tauri_plugin_opener::OpenerExt;
            if let crate::updater::Status::Available { url, .. } =
                app.state::<crate::updater::Updates>().status()
            {
                let _ = app.opener().open_url(url, None::<&str>);
            }
        }
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
        0 => "Pinrail: nothing pending".to_string(),
        1 => "Pinrail: 1 review pending".to_string(),
        n => format!("Pinrail: {n} reviews pending"),
    }));
    if let Ok(menu) = menu(app, &pending, &notification_settings(&native.state)) {
        let _ = tray.set_menu(Some(menu));
    }
    // The Dock icon carries the count too, for a menu bar that hides the tray.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_badge_count((count > 0).then_some(count as i64));
    }
}

fn menu(
    app: &AppHandle,
    pending: &[Review],
    notifications: &NotificationSettings,
) -> tauri::Result<Menu<Wry>> {
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
    // the shortcut's row carries its keys; the parser and the system may
    // yet refuse them, and the row then goes without
    let native = app.try_state::<Native>();
    let opens_inbox = native
        .as_ref()
        .is_some_and(|n| n.state.settings().value("/shortcut/global_opens") == "inbox");
    let keys = native.as_ref().map(|n| n.shortcut_state().shortcut);
    let with_keys = |id: &str, label: &str| {
        MenuItem::with_id(app, id, label, true, keys.as_deref())
            .or_else(|_| MenuItem::with_id(app, id, label, true, None::<&str>))
    };
    if opens_inbox {
        menu.append(&with_keys("inbox", "Open inbox")?)?;
    } else {
        menu.append(&MenuItem::with_id(
            app,
            "inbox",
            "Open inbox",
            true,
            None::<&str>,
        )?)?;
        menu.append(&with_keys("next", "Open oldest review")?)?;
    }
    // the pause, as a submenu: how long, or resume with the time it ends
    if !notifications.enabled {
        menu.append(&MenuItem::with_id(
            app,
            "notifications-off",
            "Notifications are off in Settings",
            false,
            None::<&str>,
        )?)?;
    } else if let Some(until) = notifications.paused_until {
        let local = until.with_timezone(&chrono::Local);
        menu.append(&MenuItem::with_id(
            app,
            "resume",
            format!(
                "Resume notifications (paused until {})",
                local.format("%H:%M")
            ),
            true,
            None::<&str>,
        )?)?;
    } else {
        menu.append(&Submenu::with_items(
            app,
            "Pause notifications",
            true,
            &[
                &MenuItem::with_id(app, "pause:15", "For 15 minutes", true, None::<&str>)?,
                &MenuItem::with_id(app, "pause:60", "For 1 hour", true, None::<&str>)?,
                &MenuItem::with_id(app, "pause:tomorrow", "Until tomorrow", true, None::<&str>)?,
            ],
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    // a new version: the restart that installs it, or where to get it
    let update = app
        .try_state::<crate::updater::Updates>()
        .map(|u| u.status());
    match update {
        Some(crate::updater::Status::Ready { version, .. }) => {
            menu.append(&MenuItem::with_id(
                app,
                "update-restart",
                format!("Restart to update to {version}"),
                true,
                None::<&str>,
            )?)?;
        }
        Some(crate::updater::Status::Available { version, .. }) => {
            menu.append(&MenuItem::with_id(
                app,
                "update-download",
                format!("Download Pinrail {version}…"),
                true,
                None::<&str>,
            )?)?;
        }
        _ => {}
    }
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit Pinrail",
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
    let mut rx = native.state.events().subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(notice) => {
                    if matches!(
                        notice.kind.as_str(),
                        events::CREATED
                            | events::DECIDED
                            | events::WITHDRAWN
                            | events::DISCARDED
                            | events::EXPIRED
                    ) {
                        let handle = app.clone();
                        let _ = app.run_on_main_thread(move || refresh_tray(&handle));
                    }
                    if notice.kind == events::CREATED {
                        notify(&app, &notice);
                    }
                    if notice.kind == events::SETTINGS_CHANGED
                        && let Some(keys) = &notice.keys
                        && let Some(native) = app.try_state::<Native>()
                    {
                        let menu_bar = keys.iter().any(|p| p == "/menu_bar_icon");
                        let notifications = keys.iter().any(|p| p.starts_with("/notifications/"));
                        let shortcut = keys.iter().any(|p| p == "/shortcut/global");
                        let opens = keys.iter().any(|p| p == "/shortcut/global_opens");
                        if menu_bar || notifications || shortcut || opens {
                            let handle = app.clone();
                            let state = native.state.clone();
                            let _ = app.run_on_main_thread(move || {
                                if menu_bar {
                                    apply_menu_bar_icon(&handle, &state);
                                }
                                if shortcut {
                                    apply_shortcut(&handle, &state);
                                }
                                refresh_tray(&handle);
                            });
                        }
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
    let settings = notification_settings(&native.state);
    if !settings.enabled || settings.paused_until.is_some() {
        return;
    }
    let Some(review) = &notice.review else {
        return;
    };
    let title = review["title"].as_str().unwrap_or("A review is waiting");
    let plugin = review["plugin"].as_str().unwrap_or("");
    // muted: still counted in the tray, never announced
    if settings.muted_plugins.iter().any(|m| m == plugin) {
        return;
    }
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
        let sound = settings.sound;
        let _ = app.run_on_main_thread(move || {
            crate::notify_mac::notify(&title, &body, id.as_deref(), sound)
        });
        return;
    }
    let mut builder = app.notification().builder().title(title).body(body);
    if settings.sound {
        builder = builder.sound("default");
    }
    if let Err(error) = builder.show() {
        eprintln!("pinrail: notification not shown: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_for_urls() {
        assert_eq!(
            route_for_url("pinrail://reviews/r_01"),
            Some("/reviews/r_01".to_string())
        );
        assert_eq!(
            route_for_url("http://127.0.0.1:4747/reviews/r_01?x=1"),
            Some("/reviews/r_01".to_string())
        );
        assert_eq!(route_for_url("pinrail://"), Some("/".to_string()));
        assert_eq!(route_for_url("pinrail://inbox"), Some("/".to_string()));
        assert_eq!(
            route_for_url("pinrail://history"),
            Some("/history".to_string())
        );
        assert_eq!(route_for_url("pinrail://reviews/"), None);
        assert_eq!(route_for_url("pinrail://settings"), None);
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
