//! The desktop app starts the review server on loopback and shows the shell.
//! Closing the window hides it; the app lives in the menu bar until "Quit".

mod headless;
mod native;
#[cfg(target_os = "macos")]
mod notify_mac;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_global_shortcut::ShortcutState;
use wicket_core::Config;
use wicket_core::api::{self, AppState};

use native::Native;

/// Opens the oldest pending review, or the inbox, from anywhere.
const SHORTCUT: &str = "alt+shift+w";

/// The shell listens for this; the payload names the command.
const COMMAND_EVENT: &str = "wicket:command";

/// Where the shell finds the server the app started.
struct ServerUrl(String);

#[tauri::command]
fn server_url(url: State<'_, ServerUrl>) -> String {
    url.0.clone()
}

/// The route the shell should show, sent before it was listening.
#[tauri::command]
fn take_pending_route(native: State<'_, Native>) -> Option<String> {
    native.take_pending_route()
}

#[tauri::command]
fn notifications_paused(native: State<'_, Native>) -> bool {
    native.paused.load(std::sync::atomic::Ordering::Relaxed)
}

#[tauri::command]
fn set_notifications_paused(app: AppHandle, native: State<'_, Native>, paused: bool) {
    native.set_paused(&app, paused);
}

#[tauri::command]
fn autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let launch = app.autolaunch();
    if enabled {
        launch.enable()
    } else {
        launch.disable()
    }
    .map_err(|error| error.to_string())
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    match headless::parse(&args) {
        Ok(Some(options)) => std::process::exit(headless::run(options)),
        Ok(None) => {}
        Err(message) => {
            eprintln!("wicket: {message}");
            std::process::exit(2);
        }
    }

    let config = Config::from_env();
    let url = config.url();

    let app = tauri::Builder::default()
        // A second launch, with or without a URL, lands in the first one.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            open_urls(app, args.iter().map(String::as_str));
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([SHORTCUT])
                .expect("the default shortcut parses")
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        native::open_next(app);
                    }
                })
                .build(),
        )
        .manage(ServerUrl(url))
        .setup(move |app| {
            let mut config = config;
            if config.sdk_dir.is_none() {
                config.sdk_dir = sdk_dir(app);
            }
            let state: Arc<AppState> = AppState::open(config).map_err(|error| {
                eprintln!("wicket: cannot open the data directory: {error}");
                std::io::Error::other(error.to_string())
            })?;
            let handle = app.handle().clone();
            let server = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = api::serve(server, std::future::pending()).await {
                    eprintln!("wicket: the server could not start: {error}");
                    handle.exit(1);
                }
            });
            app.manage(Native::new(state));
            #[cfg(target_os = "macos")]
            if notify_mac::available() {
                notify_mac::setup(app.handle());
            }
            app.set_menu(app_menu(app.handle())?)?;
            native::build_tray(app.handle())?;
            native::watch(app.handle().clone());

            // wicket:// links; a packaged app registers the scheme through
            // its bundle, a development build registers it here.
            #[cfg(any(windows, target_os = "linux"))]
            app.deep_link().register_all()?;
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                open_urls(&handle, event.urls().iter().map(|url| url.as_str()));
            });
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                open_urls(app.handle(), urls.iter().map(|url| url.as_str()));
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if matches!(
                id,
                "search"
                    | "settings"
                    | "go-inbox"
                    | "go-history"
                    | "go-plugins"
                    | "toggle-sidebar"
                    | "maximize-view"
                    | "back"
                    | "forward"
            ) {
                let _ = app.emit(COMMAND_EVENT, id.to_string());
            }
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            server_url,
            take_pending_route,
            notifications_paused,
            set_notifications_paused,
            autostart_enabled,
            set_autostart
        ])
        .build(tauri::generate_context!())
        .expect("wicket could not start its window");

    app.run(|app, event| {
        // The Dock icon brings the hidden window back.
        if let RunEvent::Reopen { .. } = event {
            native::open(app, "");
        }
    });
}

/// The standard menus plus a Navigate menu, whose accelerators reach the
/// shell even while a plugin view has the keyboard.
fn app_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::default(app)?;
    let navigate = Submenu::with_items(
        app,
        "Navigate",
        true,
        &[
            &MenuItem::with_id(app, "search", "Search…", true, Some("CmdOrCtrl+K"))?,
            &MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "go-inbox", "Inbox", true, Some("CmdOrCtrl+I"))?,
            &MenuItem::with_id(
                app,
                "go-history",
                "History",
                true,
                Some("CmdOrCtrl+Shift+H"),
            )?,
            &MenuItem::with_id(
                app,
                "go-plugins",
                "Plugins",
                true,
                Some("CmdOrCtrl+Shift+P"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "maximize-view",
                "Maximize View",
                true,
                Some("CmdOrCtrl+Shift+M"),
            )?,
            &MenuItem::with_id(
                app,
                "toggle-sidebar",
                "Toggle Sidebar",
                true,
                Some("CmdOrCtrl+B"),
            )?,
            &MenuItem::with_id(app, "back", "Back", true, Some("CmdOrCtrl+["))?,
            &MenuItem::with_id(app, "forward", "Forward", true, Some("CmdOrCtrl+]"))?,
        ],
    )?;
    // before the Window menu, where macOS expects app-specific menus
    let at = menu
        .items()?
        .iter()
        .position(|item| item.as_submenu().and_then(|s| s.text().ok()).as_deref() == Some("Window"))
        .unwrap_or(0);
    menu.insert(&navigate, at)?;
    Ok(menu)
}

/// Opens the first URL that leads somewhere in the shell; with none, just
/// brings the window forward.
fn open_urls<'a>(app: &AppHandle, urls: impl Iterator<Item = &'a str>) {
    let route = urls.filter_map(native::route_for_url).next();
    native::open(app, route.as_deref().unwrap_or(""));
}

/// The SDK bundled with the app, or the one the UI build produced next to
/// the sources during development.
fn sdk_dir(app: &tauri::App) -> Option<PathBuf> {
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("sdk").join("v1"))
        .filter(|dir| dir.join("wicket-plugin.js").is_file());
    bundled.or_else(|| {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sdk/v1");
        dev.join("wicket-plugin.js").is_file().then_some(dev)
    })
}
