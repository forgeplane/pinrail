//! The desktop app starts the review server on loopback and shows the shell.

mod headless;

use std::sync::Arc;

use tauri::State;
use wicket_core::Config;
use wicket_core::api::{self, AppState};

/// Where the shell finds the server the app started.
struct ServerUrl(String);

#[tauri::command]
fn server_url(url: State<'_, ServerUrl>) -> String {
    url.0.clone()
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
    let state: Arc<AppState> = match AppState::open(config) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("wicket: cannot open the data directory: {error}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .manage(ServerUrl(url))
        .setup(move |app| {
            let state = state.clone();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = api::serve(state, std::future::pending()).await {
                    eprintln!("wicket: the server could not start: {error}");
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![server_url])
        .run(tauri::generate_context!())
        .expect("wicket could not start its window");
}
