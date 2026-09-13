//! The desktop app starts the review server on loopback and shows the shell.
//! Closing the window hides it; the app lives in the menu bar until "Quit".

mod headless;
mod native;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, RunEvent, State, WindowEvent};
use wicket_core::Config;
use wicket_core::api::{self, AppState};

use native::Native;

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
        .plugin(tauri_plugin_notification::init())
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
            native::build_tray(app.handle())?;
            native::watch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![server_url, take_pending_route])
        .build(tauri::generate_context!())
        .expect("wicket could not start its window");

    app.run(|app, event| {
        // The Dock icon brings the hidden window back.
        if let RunEvent::Reopen { .. } = event {
            native::open(app, "");
        }
    });
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
