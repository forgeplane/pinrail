//! The desktop app starts the review server on loopback and shows the shell.
//! Closing the window hides it; the app lives in the menu bar until "Quit".

mod cli_install;
mod headless;
mod native;
#[cfg(target_os = "macos")]
mod notify_mac;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_deep_link::DeepLinkExt;
use wicket_core::Config;
use wicket_core::api::{self, AppState};

use native::Native;

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

/// The global shortcut as registered, with the error when it is not.
#[tauri::command]
fn shortcut_state(native: State<'_, Native>) -> native::ShortcutState {
    native.shortcut_state()
}

/// What macOS will do with a notification; `None` outside an app bundle,
/// where the plugin's notification is all there is.
#[cfg(target_os = "macos")]
#[tauri::command]
async fn notification_status(app: AppHandle) -> Option<notify_mac::Status> {
    if !notify_mac::available() {
        return None;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    app.run_on_main_thread(move || {
        notify_mac::status(move |status| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(status);
            }
        })
    })
    .ok()?;
    rx.await.ok()
}

/// Elsewhere the system has no say the app can read: the plugin's
/// notification is all there is, and the shell shows no status row.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn notification_status() -> Option<()> {
    None
}

/// System Settings, at the app's notification page.
#[cfg(target_os = "macos")]
#[tauri::command]
fn open_notification_settings(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(notify_mac::settings_url(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn open_notification_settings() -> Result<(), String> {
    Err("this system has no notification settings page for the app".into())
}

/// Where the bundled CLI is, whether `~/.local/bin/wicket` links to it, and
/// what a new terminal would run. Asks the login shell, so it runs off the
/// main thread.
#[tauri::command]
async fn cli_status() -> Result<cli_install::Status, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let shell = cli_install::ask_login_shell();
        Ok(cli_install::status(
            cli_install::bundled().as_deref(),
            &cli_install::link_path(&home),
            shell.as_ref(),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Links `~/.local/bin/wicket` to the bundled CLI, then reports as `cli_status`.
#[tauri::command]
async fn install_cli() -> Result<cli_install::Status, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let bundled = cli_install::bundled()
            .ok_or("this build of Wicket carries no CLI; the packaged app does")?;
        let link = cli_install::link_path(&home);
        cli_install::install(&bundled, &link)?;
        let shell = cli_install::ask_login_shell();
        Ok(cli_install::status(Some(&bundled), &link, shell.as_ref()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens the third-party notices a release bundle carries, in the system's
/// text viewer. A development build has none, and says so.
#[tauri::command]
fn open_notices(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let file = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("THIRD_PARTY_NOTICES.txt");
    if !file.is_file() {
        return Err("the packaged app carries the notices; this development build does not".into());
    }
    app.opener()
        .open_path(file.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
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
        // the window's size and position come back on the next launch;
        // not its visibility, since closing hides it and it must reopen shown
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
            app.manage(Native::new(state.clone()));
            #[cfg(target_os = "macos")]
            if notify_mac::available() {
                notify_mac::setup(app.handle());
            }
            app.set_menu(app_menu(app.handle())?)?;
            native::build_tray(app.handle())?;
            native::apply_menu_bar_icon(app.handle(), &state);
            native::apply_shortcut(app.handle(), &state);
            native::refresh_tray_at_pause_end(app.handle(), &state);
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
                // the close button hides the window, unless settings say quit
                let quit = window
                    .app_handle()
                    .try_state::<Native>()
                    .is_some_and(|n| n.state.settings.value("/close_window") == "quit");
                if quit {
                    window.app_handle().exit(0);
                } else {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            server_url,
            take_pending_route,
            shortcut_state,
            notification_status,
            open_notification_settings,
            autostart_enabled,
            set_autostart,
            cli_status,
            install_cli,
            open_notices
        ])
        .build(tauri::generate_context!())
        .expect("wicket could not start its window");

    app.run(|app, event| {
        // The Dock icon brings the hidden window back.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            native::open(app, "");
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
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
