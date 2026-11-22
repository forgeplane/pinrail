//! Quitting while agents run in Pinrail. The setting `/runtime/on_quit`
//! decides: `ask` opens a dialog in the window, `keep` quits and leaves
//! the agents running, and `stop` stops them first. Restarting to install
//! an update never asks, because the agents survive an update.
//!
//! Every way to quit that Pinrail controls requests an exit, which comes
//! here: Quit in the app menu and in the tray menu, and closing the window
//! when it quits. macOS also quits the app without asking it, from the
//! Dock or at logout; then only `stop` is applied, on the way out.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use tauri::menu::{Menu, MenuItem, MenuItemKind};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::native::Native;

/// The event that asks the window to open the quit dialog, with the
/// running agents: each one's agent, session title and folder.
const ASK_EVENT: &str = "pinrail:quit-asked";

/// What to do with an exit request.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Quit at once.
    Quit,
    /// Stop every agent, then quit.
    StopAndQuit,
    /// Stay open and ask, for this many running agents.
    Ask(usize),
}

/// The decision for an exit request, from the setting and the running
/// agents. `confirmed` is set once the person has answered the dialog.
pub fn decide(on_quit: &str, live: usize, confirmed: bool, restart: bool) -> Decision {
    if confirmed || restart || live == 0 {
        return Decision::Quit;
    }
    match on_quit {
        "keep" => Decision::Quit,
        "stop" => Decision::StopAndQuit,
        _ => Decision::Ask(live),
    }
}

/// Whether the person has answered the quit dialog.
#[derive(Default)]
pub struct Quitting {
    confirmed: AtomicBool,
}

/// The setting, and the running agents as the window lists them.
fn live(app: &AppHandle) -> Option<(String, Vec<serde_json::Value>)> {
    let native = app.try_state::<Native>()?;
    let on_quit = native.state.settings().value("/runtime/on_quit");
    let on_quit = on_quit.as_str().unwrap_or("ask").to_string();
    let live = native.state.runtime().live().unwrap_or_default();
    let running = live
        .into_iter()
        .map(|(run, title)| json!({"agent": run.agent, "title": title, "folder": run.folder}))
        .collect();
    Some((on_quit, running))
}

/// Stops every agent, waiting at most a few seconds for the host to take
/// the requests.
fn stop_all(app: &AppHandle) {
    let Some(native) = app.try_state::<Native>() else {
        return;
    };
    let state = native.state.clone();
    let stopped = tauri::async_runtime::block_on(async move {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            state.runtime().stop_all(),
        )
        .await
    });
    match stopped {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("pinrail: the agents could not all be stopped: {error}"),
        Err(_) => eprintln!("pinrail: stopping the agents took too long"),
    }
}

/// Handles an exit request: lets it through, stops the agents first, or
/// keeps the app open and asks in the window.
pub fn on_exit_requested(app: &AppHandle, code: Option<i32>, api: &tauri::ExitRequestApi) {
    let confirmed = app
        .try_state::<Quitting>()
        .is_some_and(|q| q.confirmed.load(Ordering::SeqCst));
    let Some((on_quit, live)) = live(app) else {
        return;
    };
    match decide(
        &on_quit,
        live.len(),
        confirmed,
        code == Some(tauri::RESTART_EXIT_CODE),
    ) {
        Decision::Quit => {}
        Decision::StopAndQuit => stop_all(app),
        Decision::Ask(_) => {
            api.prevent_exit();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = app.emit(ASK_EVENT, json!({ "running": live }));
        }
    }
}

/// On the way out, when macOS quit the app without an exit request:
/// applies `stop`, which must not be skipped.
pub fn on_exit(app: &AppHandle) {
    let confirmed = app
        .try_state::<Quitting>()
        .is_some_and(|q| q.confirmed.load(Ordering::SeqCst));
    if let Some((on_quit, live)) = live(app)
        && decide(&on_quit, live.len(), confirmed, false) == Decision::StopAndQuit
    {
        stop_all(app);
    }
}

/// The quit dialog's answer: quit, after stopping the agents with `stop`.
/// With `remember`, the choice becomes `/runtime/on_quit`, so that the
/// next quit does not ask.
#[tauri::command]
pub async fn quit(app: AppHandle, stop: bool, remember: bool) -> Result<(), String> {
    let native = app.state::<Native>();
    if remember {
        let choice = if stop { "stop" } else { "keep" };
        native
            .state
            .settings()
            .change(&json!({"runtime": {"on_quit": choice}}))
            .map_err(|error| error.to_string())?;
    }
    if stop {
        native
            .state
            .runtime()
            .stop_all()
            .await
            .map_err(|error| error.to_string())?;
    }
    app.state::<Quitting>()
        .confirmed
        .store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

/// Replaces the menu's standard Quit item, which on macOS ends the app
/// without an exit request, with one that requests it.
pub fn replace_quit_item<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<()> {
    for item in menu.items()? {
        let MenuItemKind::Submenu(submenu) = item else {
            continue;
        };
        for (at, entry) in submenu.items()?.iter().enumerate() {
            let MenuItemKind::Predefined(predefined) = entry else {
                continue;
            };
            let text = predefined.text()?;
            if !text.starts_with("Quit") {
                continue;
            }
            submenu.remove(entry)?;
            submenu.insert(
                &MenuItem::with_id(app, "quit", text, true, Some("CmdOrCtrl+Q"))?,
                at,
            )?;
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_waits_for_the_answer_only_while_agents_run() {
        assert_eq!(decide("ask", 3, false, false), Decision::Ask(3));
        assert_eq!(decide("ask", 0, false, false), Decision::Quit);
        assert_eq!(decide("ask", 3, true, false), Decision::Quit);
    }

    #[test]
    fn keep_quits_and_stop_stops_the_agents_first() {
        assert_eq!(decide("keep", 2, false, false), Decision::Quit);
        assert_eq!(decide("stop", 2, false, false), Decision::StopAndQuit);
        assert_eq!(decide("stop", 0, false, false), Decision::Quit);
    }

    #[test]
    fn restarting_to_update_never_asks_or_stops() {
        assert_eq!(decide("ask", 2, false, true), Decision::Quit);
        assert_eq!(decide("stop", 2, false, true), Decision::Quit);
    }
}
