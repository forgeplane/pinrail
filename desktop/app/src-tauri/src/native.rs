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
    /// Wakes the task that rebuilds the tray; see `refresh_tray`.
    tray: Arc<tokio::sync::Notify>,
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
            tray: Arc::default(),
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
    /// from and to, local time; to may be earlier, across midnight
    quiet_hours: Option<(chrono::NaiveTime, chrono::NaiveTime)>,
}

/// Whether a review of `plugin` arriving at `now` is announced. One that is
/// not is still counted in the tray.
fn announces(settings: &NotificationSettings, plugin: &str, now: DateTime<chrono::Local>) -> bool {
    let quiet = settings.quiet_hours.is_some_and(|(from, to)| {
        let t = now.time();
        // a range across midnight, 22:00 to 07:30, is quiet on both sides of it
        if from <= to {
            from <= t && t < to
        } else {
            t >= from || t < to
        }
    });
    settings.enabled
        && settings.paused_until.is_none()
        && !quiet
        && !settings.muted_plugins.iter().any(|m| m == plugin)
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
        quiet_hours: {
            let clock = |key: &str| {
                n["quiet_hours"][key]
                    .as_str()
                    .and_then(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M").ok())
            };
            clock("from").zip(clock("to"))
        },
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
                refresh_tray(&handle);
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

/// The oldest pending review, or the inbox when nothing is waiting. It is
/// looked up off the main thread, where the menu and the shortcut call it.
pub fn open_next(app: &AppHandle) {
    let Some(state) = app.try_state::<Native>().map(|native| native.state.clone()) else {
        return open(app, "/");
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let filters = Filters {
            statuses: vec![Status::Pending],
            limit: 1,
            oldest_first: true,
            ..Filters::default()
        };
        match state
            .reviews()
            .list(&filters)
            .ok()
            .and_then(|r| r.into_iter().next())
        {
            Some(review) => open_review(&app, &review.id),
            None => open(&app, "/"),
        }
    });
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
        // the core's id alphabet only, since the shell puts the id into API paths
        (Some("reviews"), Some(id))
            if !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') =>
        {
            Some(format!("/reviews/{id}"))
        }
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

/// What the tray shows: how many reviews are pending, and the oldest of
/// them, oldest first, for its rows.
#[derive(Debug, Default)]
struct TrayContents {
    count: usize,
    oldest: Vec<Review>,
}

fn tray_contents(state: &Pinrail) -> TrayContents {
    let filters = Filters {
        statuses: vec![Status::Pending],
        limit: TRAY_ROWS,
        oldest_first: true,
        ..Filters::default()
    };
    match state.reviews().listing(&filters, false) {
        Ok(listing) => TrayContents {
            count: listing.total,
            oldest: listing.reviews,
        },
        Err(error) => {
            eprintln!("pinrail: the menu bar icon could not read the pending reviews: {error}");
            TrayContents::default()
        }
    }
}

/// The menu bar icon, and the same at 45% opacity while notifications are
/// paused or off.
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray.png");
const TRAY_PAUSED_ICON: &[u8] = include_bytes!("../icons/tray-paused.png");

pub fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let icon = tauri::image::Image::from_bytes(TRAY_ICON)?;
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Pinrail")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    watch_tray(app);
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
                // saving the setting writes a file and records an event,
                // which is not the main thread's to wait for
                let state = native.state.clone();
                tauri::async_runtime::spawn_blocking(move || pause_notifications(&state, until));
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

/// Asks for the tray to be rebuilt: its title, tooltip and menu, and the
/// Dock badge. Any thread may ask. Requests that arrive together make one
/// rebuild, done by the task `build_tray` starts.
pub fn refresh_tray(app: &AppHandle) {
    if let Some(native) = app.try_state::<Native>() {
        native.tray.notify_one();
    }
}

/// Rebuilds the tray each time `refresh_tray` asks, for the app's life. It
/// waits a moment, so a burst of events makes one rebuild, reads the pending
/// reviews and the settings on a blocking thread, where waiting for the
/// database holds nothing up, and hands only the finished data to the main
/// thread, where menus live on macOS.
fn watch_tray(app: &AppHandle) {
    let Some(native) = app.try_state::<Native>() else {
        return;
    };
    let wake = native.tray.clone();
    let state = native.state.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            wake.notified().await;
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let state = state.clone();
            let read = tauri::async_runtime::spawn_blocking(move || {
                (tray_contents(&state), notification_settings(&state))
            });
            let Ok((contents, notifications)) = read.await else {
                continue;
            };
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || show_tray(&handle, &contents, &notifications));
        }
    });
}

/// Puts what `watch_tray` read into the tray and the Dock badge. Menus are
/// main-thread objects on macOS; call this there.
fn show_tray(app: &AppHandle, contents: &TrayContents, notifications: &NotificationSettings) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let count = contents.count;
    // dimmed while nothing will notify: paused, or off in Settings
    let quiet = if !notifications.enabled {
        Some(" · notifications off".to_string())
    } else {
        notifications.paused_until.map(|until| {
            let local = until.with_timezone(&chrono::Local);
            format!(" · notifications paused until {}", local.format("%H:%M"))
        })
    };
    let icon = if quiet.is_some() {
        TRAY_PAUSED_ICON
    } else {
        TRAY_ICON
    };
    if let Ok(image) = tauri::image::Image::from_bytes(icon) {
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_icon_as_template(true);
    }
    // an empty title, not None: None leaves the old title in place on macOS
    let _ = tray.set_title(Some(if count == 0 {
        String::new()
    } else {
        count.to_string()
    }));
    let _ = tray.set_tooltip(Some(format!(
        "{}{}",
        match count {
            0 => "Pinrail: nothing pending".to_string(),
            1 => "Pinrail: 1 review pending".to_string(),
            n => format!("Pinrail: {n} reviews pending"),
        },
        quiet.unwrap_or_default()
    )));
    if let Ok(menu) = menu(app, contents, notifications) {
        let _ = tray.set_menu(Some(menu));
    }
    // The Dock icon carries the count too, for a menu bar that hides the tray.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_badge_count((count > 0).then_some(count as i64));
    }
}

fn menu(
    app: &AppHandle,
    contents: &TrayContents,
    notifications: &NotificationSettings,
) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    if contents.oldest.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "none",
            "Nothing pending",
            false,
            None::<&str>,
        )?)?;
    } else {
        let now = Utc::now();
        for review in &contents.oldest {
            menu.append(&MenuItem::with_id(
                app,
                format!("review:{}", review.id),
                tray_label(review, now),
                true,
                None::<&str>,
            )?)?;
        }
        if contents.count > contents.oldest.len() {
            menu.append(&MenuItem::with_id(
                app,
                "more",
                format!(
                    "{} more in the inbox",
                    contents.count - contents.oldest.len()
                ),
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

/// A review's request summary as a line, "1 blocker, 2 major", when it has
/// one.
fn summary_counts(summary: Option<&serde_json::Value>) -> Option<String> {
    let counts = summary?.get("counts")?.as_array()?;
    let parts: Vec<String> = counts
        .iter()
        .filter_map(|count| {
            let label = count.get("label")?.as_str()?;
            let n = count.get("count")?.as_u64()?;
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
                        refresh_tray(&app);
                    }
                    if notice.kind == events::CREATED {
                        notify(&app, &notice);
                    }
                    if ends_notification(&notice.kind)
                        && let Some(id) = &notice.review_id
                    {
                        close_notification(&app, id);
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
                Err(RecvError::Lagged(_)) => refresh_tray(&app),
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
    let Some(review) = &notice.review else {
        return;
    };
    let title = review["title"].as_str().unwrap_or("A review is waiting");
    let plugin = review["plugin"].as_str().unwrap_or("");
    if !announces(&settings, plugin, chrono::Local::now()) {
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
    #[cfg(target_os = "linux")]
    {
        crate::notify_linux::notify(app, title, &body, notice.review_id.clone(), settings.sound);
        return;
    }
    #[allow(unreachable_code)]
    {
        let mut builder = app.notification().builder().title(title).body(body);
        if settings.sound {
            builder = builder.sound("default");
        }
        if let Err(error) = builder.show() {
            eprintln!("pinrail: notification not shown: {error}");
        }
    }
}

/// The notices after which a review's notification has done its work: the
/// person opened the review, or it stopped waiting.
fn ends_notification(kind: &str) -> bool {
    matches!(
        kind,
        events::VIEWED | events::DECIDED | events::WITHDRAWN | events::DISCARDED | events::EXPIRED
    )
}

/// Takes the review's notification off the screen and out of the
/// system's list of notifications.
#[allow(unused_variables)]
fn close_notification(app: &AppHandle, review_id: &str) {
    #[cfg(target_os = "macos")]
    if crate::notify_mac::available() {
        let id = review_id.to_string();
        let _ = app.run_on_main_thread(move || crate::notify_mac::close(&id));
    }
    #[cfg(target_os = "linux")]
    crate::notify_linux::close(review_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_ends_when_its_review_is_opened_or_stops_waiting() {
        for kind in ["viewed", "decided", "withdrawn", "discarded", "expired"] {
            assert!(ends_notification(kind), "{kind}");
        }
        for kind in ["created", "plugin_changed", "settings_changed"] {
            assert!(!ends_notification(kind), "{kind}");
        }
    }

    #[test]
    fn the_tray_counts_every_pending_review_and_lists_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let state = Pinrail::open(pinrail_core::Config::new(dir.path(), 0)).unwrap();
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(state.plugins().install_offered("list"))
            .unwrap();
        for n in 0..502 {
            let body = serde_json::json!({
                "plugin": "list", "title": format!("review {n}"), "payload": {"groups": []}
            });
            state.reviews().submit(&body, None).unwrap();
        }
        let tray = tray_contents(&state);
        assert_eq!(tray.count, 502);
        let titles: Vec<&str> = tray.oldest.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            (0..TRAY_ROWS)
                .map(|n| format!("review {n}"))
                .collect::<Vec<_>>()
        );
    }

    fn settings(quiet: Option<(&str, &str)>) -> NotificationSettings {
        NotificationSettings {
            enabled: true,
            paused_until: None,
            sound: true,
            muted_plugins: vec!["logo".into()],
            quiet_hours: quiet.map(|(from, to)| {
                (
                    chrono::NaiveTime::parse_from_str(from, "%H:%M").unwrap(),
                    chrono::NaiveTime::parse_from_str(to, "%H:%M").unwrap(),
                )
            }),
        }
    }

    fn at(time: &str) -> DateTime<chrono::Local> {
        use chrono::TimeZone;
        let t =
            chrono::NaiveDateTime::parse_from_str(&format!("2026-09-27 {time}"), "%Y-%m-%d %H:%M")
                .unwrap();
        chrono::Local.from_local_datetime(&t).single().unwrap()
    }

    #[test]
    fn a_review_is_announced_unless_off_paused_or_muted() {
        let on = settings(None);
        assert!(announces(&on, "list", at("12:00")));
        assert!(!announces(&on, "logo", at("12:00")), "muted");
        assert!(!announces(
            &NotificationSettings {
                enabled: false,
                ..settings(None)
            },
            "list",
            at("12:00")
        ));
        assert!(!announces(
            &NotificationSettings {
                paused_until: Some(Utc::now() + chrono::Duration::hours(1)),
                ..settings(None)
            },
            "list",
            at("12:00")
        ));
    }

    #[test]
    fn quiet_hours_hold_back_announcements_across_midnight_too() {
        let night = settings(Some(("22:00", "07:30")));
        assert!(!announces(&night, "list", at("23:15")));
        assert!(!announces(&night, "list", at("03:00")));
        assert!(
            announces(&night, "list", at("07:30")),
            "the end is not quiet"
        );
        assert!(announces(&night, "list", at("12:00")));
        let lunch = settings(Some(("12:00", "13:00")));
        assert!(!announces(&lunch, "list", at("12:30")));
        assert!(announces(&lunch, "list", at("18:00")));
    }

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
        // an id is the core's alphabet only: it goes into API paths
        for bad in [
            "pinrail://reviews/..",
            "pinrail://reviews/%2e%2e",
            "pinrail://reviews/r_1%2Fviewed",
            "pinrail://reviews/a.b",
        ] {
            assert_eq!(route_for_url(bad), None, "{bad}");
        }
        assert_eq!(route_for_url("pinrail://settings"), None);
        assert_eq!(route_for_url("not a url"), None);
    }

    #[test]
    fn summary_counts_read_as_a_line() {
        let summary = serde_json::json!({ "counts": [
            { "label": "blocker", "count": 1, "tone": "danger" },
            { "label": "major", "count": 2, "tone": "warning" }
        ]});
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
