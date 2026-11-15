//! A window for the terminal benchmark, in debug builds only: the bench
//! page that Vite serves for development (`ui/bench/terminal/app.html`),
//! so that the benchmark measures the renderers in the app's own WebView.
//!
//! `pinrail-desktop --bench <url>` opens it, or points it at a new URL;
//! a second launch hands the flag to the running app. The window comes to
//! the front with focus: covered, macOS would slow its animation frames
//! and its timings would mean nothing. The page asks for `done` when the
//! run ends, which closes the window. The window has no capabilities, so it
//! can call no command of the app.

use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

const FLAG: &str = "--bench";
const LABEL: &str = "bench";
/// The only pages the window opens: the bench's, on the Vite dev server.
const PAGES: &str = "http://localhost:5173/bench/terminal/";

/// Opens the bench window if the arguments ask for it, and says whether
/// they did.
pub fn open_from_args<'a>(app: &AppHandle, args: impl IntoIterator<Item = &'a str>) -> bool {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == FLAG {
            match args.next().map(page) {
                Some(Ok(url)) => open(app, url),
                Some(Err(why)) => eprintln!("bench: {why}"),
                None => eprintln!("bench: {FLAG} needs the URL of the bench page"),
            }
            return true;
        }
    }
    false
}

/// The URL, if it is one of the bench's pages.
fn page(url: &str) -> Result<Url, String> {
    if !url.starts_with(PAGES) {
        return Err(format!("only pages under {PAGES} open, not {url}"));
    }
    url.parse().map_err(|e| format!("{url}: {e}"))
}

fn open(app: &AppHandle, url: Url) {
    if let Some(window) = app.get_webview_window(LABEL) {
        if let Err(e) = window.navigate(url) {
            eprintln!("bench: {e}");
        }
        let _ = window.set_focus();
        return;
    }
    let handle = app.clone();
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url))
        .title("Terminal bench")
        .inner_size(1940.0, 760.0)
        .focused(true)
        .center()
        .on_navigation(move |to| {
            // the page asks for `done` when the run ends
            if to.as_str().starts_with(PAGES) && to.path().ends_with("/done") {
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(window) = handle.get_webview_window(LABEL) {
                        let _ = window.close();
                    }
                });
                return false;
            }
            true
        })
        .build();
    match built {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("bench: {e}"),
    }
}
