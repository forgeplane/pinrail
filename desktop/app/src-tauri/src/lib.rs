//! The desktop app starts the review server on loopback and shows the shell.
//! Closing the window hides it; the app lives in the menu bar until "Quit".

mod agent_skills;
mod cli_install;
mod feedback;
mod headless;
mod native;
#[cfg(target_os = "macos")]
mod notify_mac;
mod startup;
mod updater;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pinrail_core::Config;
use pinrail_core::{Pinrail, api};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_deep_link::DeepLinkExt;

use native::Native;
use updater::Updates;

/// The shell listens for this; the payload names the command.
const COMMAND_EVENT: &str = "pinrail:command";

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

/// Asks macOS to let the app notify, then reports as `notification_status`;
/// the welcome screen's and Settings' *Turn on notifications*.
#[cfg(target_os = "macos")]
#[tauri::command]
async fn request_notifications(app: AppHandle) -> Option<notify_mac::Status> {
    if !notify_mac::available() {
        return None;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    app.run_on_main_thread(move || {
        notify_mac::request(move |_| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
        })
    })
    .ok()?;
    rx.await.ok()?;
    notification_status(app).await
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn request_notifications() -> Option<()> {
    None
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

/// Where the bundled CLI is, whether `~/.local/bin/pinrail` links to it, and
/// what a new terminal would run. Asks the login shell, so it runs off the
/// main thread.
/// Sends the feedback dialog's form, as the bytes of a multipart body with
/// its content type in the `x-content-type` header, to the feedback service.
#[tauri::command]
async fn send_feedback(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(body) = request.body() else {
        return Err("the feedback must be sent as bytes".into());
    };
    let content_type = request
        .headers()
        .get("x-content-type")
        .and_then(|value| value.to_str().ok())
        .ok_or("the feedback's content type is missing")?
        .to_string();
    let body = body.clone();
    tauri::async_runtime::spawn_blocking(move || {
        feedback::send(&feedback::endpoint(), &content_type, &body)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}

/// The agents found on this computer, and whether each has the `pinrail`
/// skill.
#[tauri::command]
async fn agents_status(app: AppHandle) -> Result<Vec<agent_skills::AgentStatus>, String> {
    let version = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || Ok(agent_skills::status(&home()?, &version)))
        .await
        .map_err(|e| e.to_string())?
}

/// The skill as this version writes it, for an agent the app does not know.
#[tauri::command]
fn agent_skill(app: AppHandle) -> String {
    agent_skills::skill_text(&app.package_info().version.to_string())
}

/// Writes or updates the `pinrail` skill for an agent, then reports as
/// `agents_status`.
#[tauri::command]
async fn connect_agent(
    app: AppHandle,
    id: String,
) -> Result<Vec<agent_skills::AgentStatus>, String> {
    let version = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let home = home()?;
        agent_skills::connect(&home, &id, &version)?;
        Ok(agent_skills::status(&home, &version))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Removes the app's `pinrail` skill from an agent, then reports as
/// `agents_status`.
#[tauri::command]
async fn disconnect_agent(
    app: AppHandle,
    id: String,
) -> Result<Vec<agent_skills::AgentStatus>, String> {
    let version = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let home = home()?;
        agent_skills::disconnect(&home, &id)?;
        Ok(agent_skills::status(&home, &version))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn cli_status() -> Result<cli_install::Status, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let bundled = cli_install::bundled();
        let mode = bundled.as_deref().map_or(cli_install::Mode::Link, |b| {
            cli_install::mode(b, cli_install::in_appimage())
        });
        let shell = cli_install::ask_login_shell();
        Ok(cli_install::status(
            mode,
            bundled.as_deref(),
            &cli_install::link_path(&home),
            shell.as_ref(),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Saves a file a review carries where the person chooses: the save dialog
/// from here, and the stored bytes copied straight to the path, so the
/// shell needs no file access of its own. None when the dialog is
/// cancelled; the path written otherwise.
#[tauri::command]
async fn save_attachment(
    app: AppHandle,
    native: State<'_, Native>,
    review: String,
    name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let state = native.state.clone();
    let carried = state
        .reviews()
        .get(&review)
        .map_err(|e| e.to_string())?
        .attachments
        .into_iter()
        .find(|a| a.name == name)
        .ok_or_else(|| format!("review {review} carries no attachment \"{name}\""))?;
    let from = state.attachments().path(&carried.sha256);
    tauri::async_runtime::spawn_blocking(move || {
        let Some(to) = app
            .dialog()
            .file()
            .set_file_name(&name)
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let to = to.into_path().map_err(|e| e.to_string())?;
        save_copy(&from, &to).map_err(|e| format!("saving {}: {e}", to.display()))?;
        Ok(Some(to.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Copies a stored attachment to where the person chose. The store keeps
/// its files read-only; the copy is the person's own, a new file made as
/// any they save is, so their umask sets who else may read it.
fn save_copy(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut stored = std::fs::File::open(from)?;
    let mut saved = std::fs::File::create(to)?;
    std::io::copy(&mut stored, &mut saved)?;
    Ok(())
}

/// Links or copies the bundled CLI to `~/.local/bin/pinrail`, as the way the
/// app was installed calls for, then reports as `cli_status`.
#[tauri::command]
async fn install_cli() -> Result<cli_install::Status, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let bundled = cli_install::bundled()
            .ok_or("this build of Pinrail carries no CLI; the packaged app does")?;
        let mode = cli_install::mode(&bundled, cli_install::in_appimage());
        let link = cli_install::link_path(&home);
        cli_install::install(mode, &bundled, &link)?;
        let shell = cli_install::ask_login_shell();
        Ok(cli_install::status(
            mode,
            Some(&bundled),
            &link,
            shell.as_ref(),
        ))
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

/// Where updating stands.
#[tauri::command]
fn update_status(updates: State<'_, Updates>) -> updater::Status {
    updates.status()
}

/// Looks for a new version now and downloads it; *Check for updates…*.
#[tauri::command]
async fn check_for_updates(app: AppHandle) -> updater::Status {
    updater::check(&app).await
}

/// Installs the downloaded version and starts it. Async, so the install
/// runs off the main thread, which would otherwise freeze the window.
#[tauri::command]
async fn restart_to_update(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || updater::restart(&app))
        .await
        .map_err(|error| error.to_string())?
}

// async so their file I/O runs off the main thread
#[tauri::command]
async fn autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
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
            eprintln!("pinrail: {message}");
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(ServerUrl(url))
        .manage(Updates::new())
        .setup(move |app| {
            let mut config = config;
            if config.sdk_dir.is_none() {
                config.sdk_dir = std::env::current_exe().ok().and_then(|exe| sdk_dir(&exe));
            }
            // What the app cannot do without, the port first: a server
            // already running on it keeps its data directory to itself.
            // Either failing is said in a dialog before the app quits.
            let listener = match api::bind(&config) {
                Ok(listener) => listener,
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                    let running = pinrail_core::server_info::read(&config);
                    cannot_start(app, startup::port_in_use(config.port, running.as_ref()));
                    return Ok(());
                }
                Err(error) => {
                    cannot_start(app, format!("Pinrail cannot start its server: {error}."));
                    return Ok(());
                }
            };
            let data_dir = config.data_dir.clone();
            let state: Arc<Pinrail> = match Pinrail::open(config) {
                Ok(state) => Arc::new(state),
                Err(pinrail_core::Error::InUse(_)) => {
                    let holder = pinrail_core::app::locked_by(&data_dir);
                    cannot_start(app, startup::data_dir_in_use(&data_dir, holder));
                    return Ok(());
                }
                Err(error) => {
                    cannot_start(app, startup::data_dir(&data_dir, &error.to_string()));
                    return Ok(());
                }
            };
            let handle = app.handle().clone();
            let server = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = api::serve(server, listener, std::future::pending()).await {
                    eprintln!("pinrail: the server stopped: {error}");
                    handle.exit(1);
                }
            });
            app.manage(Native::new(state.clone()));
            #[cfg(target_os = "macos")]
            if notify_mac::available() {
                notify_mac::setup(app.handle());
            }
            // the menu and the tray are conveniences: without them the app
            // still does its work, so a failure is logged, not fatal
            if let Err(error) = app_menu(app.handle()).and_then(|menu| app.set_menu(menu)) {
                eprintln!("pinrail: the menu could not be set up: {error}");
            }
            if let Err(error) = native::build_tray(app.handle()) {
                eprintln!("pinrail: the menu bar icon could not be set up: {error}");
            }
            native::apply_menu_bar_icon(app.handle(), &state);
            native::apply_shortcut(app.handle(), &state);
            native::watch_pause_end(app.handle(), state.clone());
            native::watch(app.handle().clone());
            let settings = state.clone();
            updater::start(app.handle(), move || {
                settings
                    .settings()
                    .value("/updates/check")
                    .as_bool()
                    .unwrap_or(true)
            });

            // an update replaces the app, not a copy of the CLI that Install
            // the CLI made from it (the AppImage's way): bring it up to date
            if let Some(bundled) = cli_install::bundled()
                && cli_install::mode(&bundled, cli_install::in_appimage())
                    == cli_install::Mode::Copy
            {
                std::thread::spawn(move || {
                    let Some(home) = std::env::var_os("HOME") else {
                        return;
                    };
                    let link = cli_install::link_path(&PathBuf::from(home));
                    match cli_install::refresh(&bundled, &link) {
                        Ok(true) => eprintln!("pinrail: updated the CLI at {}", link.display()),
                        Ok(false) => {}
                        Err(error) => eprintln!("pinrail: the CLI was not updated: {error}"),
                    }
                });
            }

            // pinrail:// links; a packaged app registers the scheme through
            // its bundle or package, a development build or an AppImage
            // here. Links not working is no reason not to start.
            #[cfg(any(windows, target_os = "linux"))]
            if (cfg!(debug_assertions) || std::env::var_os("APPIMAGE").is_some())
                && let Err(error) = app.deep_link().register_all()
            {
                eprintln!("pinrail: pinrail:// links could not be registered: {error}");
            }
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
                    | "toggle-theme"
                    | "maximize-view"
                    | "back"
                    | "forward"
                    | "feedback"
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
                    .is_some_and(|n| n.state.settings().value("/close_window") == "quit");
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
            send_feedback,
            take_pending_route,
            shortcut_state,
            notification_status,
            request_notifications,
            open_notification_settings,
            autostart_enabled,
            set_autostart,
            cli_status,
            install_cli,
            agents_status,
            agent_skill,
            connect_agent,
            disconnect_agent,
            open_notices,
            save_attachment,
            update_status,
            check_for_updates,
            restart_to_update
        ])
        .build(tauri::generate_context!())
        .expect("pinrail could not start its window");

    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            // the server goes with the app: server.json must not advertise
            // it once it has. Shutting the server down gracefully would wait
            // on every open long poll, so the file is removed directly.
            if let Some(native) = app.try_state::<Native>() {
                pinrail_core::server_info::remove(native.state.config());
            }
            // A downloaded update is installed on the way out, so quitting
            // updates as well as restarting does.
            if let Err(error) = updater::install(app) {
                eprintln!("pinrail: the update could not be installed: {error}");
            }
        }
        // The Dock icon brings the hidden window back.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            native::open(app, "");
        }
    });
}

/// The standard menus plus a Navigate menu, whose accelerators reach the
/// shell even while a plugin view has the keyboard, and Send Feedback in the
/// Help menu.
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
            &MenuItem::with_id(
                app,
                "toggle-theme",
                "Toggle Theme",
                true,
                Some("CmdOrCtrl+Shift+L"),
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
    if let Some(help) = menu
        .get(tauri::menu::HELP_SUBMENU_ID)
        .and_then(|item| item.as_submenu().cloned())
    {
        help.prepend(&MenuItem::with_id(
            app,
            "feedback",
            "Send Feedback…",
            true,
            None::<&str>,
        )?)?;
    }
    Ok(menu)
}

/// Hides the window, tells the person why the app cannot start, and quits
/// once they have read it. Opened from the Dock or a launcher, stderr is
/// never seen.
fn cannot_start(app: &tauri::App, message: String) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    eprintln!("pinrail: {message}");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let handle = app.handle().clone();
    app.dialog()
        .message(message)
        .title("Pinrail cannot start")
        .kind(MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
}

/// Opens the first URL that leads somewhere in the shell; with none, just
/// brings the window forward.
fn open_urls<'a>(app: &AppHandle, urls: impl Iterator<Item = &'a str>) {
    let route = urls.filter_map(native::route_for_url).next();
    native::open(app, route.as_deref().unwrap_or(""));
}

/// The SDK bundled with the app, found from the executable where Tauri
/// puts resources, or, in a development build, the one the UI build
/// produced next to the sources. The windowed app and `--headless` both
/// use it. Without one, plugin views cannot load, and the log says so.
pub(crate) fn sdk_dir(exe: &Path) -> Option<PathBuf> {
    // tauri.conf.json's productName: the Linux packages' resource folder
    const PRODUCT: &str = "Pinrail";
    let dir = exe.parent()?;
    let mut resources = Vec::new();
    if cfg!(target_os = "macos") {
        resources.push(dir.join("../Resources"));
    } else {
        resources.push(dir.join("../lib").join(PRODUCT));
        if let Some(appdir) = std::env::var_os("APPDIR") {
            resources.push(PathBuf::from(appdir).join("usr/lib").join(PRODUCT));
        }
        resources.push(PathBuf::from("/usr/lib").join(PRODUCT));
    }
    // a release looks only in its own bundle, so a packaging error shows
    let sources =
        cfg!(debug_assertions).then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sdk/v1"));
    let found = resources
        .into_iter()
        .map(|r| r.join("sdk/v1"))
        .chain(sources)
        .find(|dir| dir.join("pinrail-plugin.js").is_file());
    if found.is_none() {
        eprintln!(
            "pinrail: the plugin SDK is not in the app's bundle, so plugin views cannot load"
        );
    }
    found
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_saved_attachment_is_a_new_file_of_the_persons() {
        let dir = tempfile::tempdir().unwrap();
        let (stored, saved) = (dir.path().join("blob"), dir.path().join("pivot.glb"));
        std::fs::write(&stored, b"glTF").unwrap();
        // as the store keeps its files
        std::fs::set_permissions(&stored, std::fs::Permissions::from_mode(0o444)).unwrap();

        super::save_copy(&stored, &saved).unwrap();
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(std::fs::read(&saved).unwrap(), b"glTF");
        assert_ne!(
            mode(&saved) & 0o200,
            0,
            "the person can write it: {:o}",
            mode(&saved)
        );
        // not the store's read-only mode: the one any file they make gets,
        // which their umask decides
        let fresh = dir.path().join("fresh");
        std::fs::File::create(&fresh).unwrap();
        assert_eq!(mode(&saved), mode(&fresh));
    }
}
