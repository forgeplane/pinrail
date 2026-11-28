//! The files copied to the clipboard, as Explorer copies them, and the
//! files of the last drop on the window: what clipboard_mac.rs reads on
//! macOS.
//!
//! A drop of files from Explorer gives the page `File`s without their
//! paths, so the page hands them to the app with WebView2's
//! `chrome.webview.postMessageWithAdditionalObjects`, under [`DROP`], and
//! the app reads each one's path here.

// the clipboard and WebView2 are reached only through unsafe calls; each
// says why it is sound
#![allow(unsafe_code)]

use std::sync::{Condvar, Mutex};
use std::time::Duration;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs,
    ICoreWebView2WebMessageReceivedEventArgs2,
};
use webview2_com::WebMessageReceivedEventHandler;
use windows::core::{Interface, PWSTR};

/// The message a drop of files is posted with, followed by `:` and the
/// drop's number.
pub const DROP: &str = "pinrail:dropped-files";

/// The paths of the last drop the page posted, with its number, until
/// `dragged_files` takes them. A drop's number names it, so that one whose
/// paths came late, or were never taken, is not taken for the next.
static DROPPED: Mutex<Option<(u64, Vec<String>)>> = Mutex::new(None);
static ARRIVED: Condvar = Condvar::new();

/// The clipboard, open for this thread until this is dropped.
struct OpenClipboard;

impl OpenClipboard {
    /// Opens it, trying a few times, as another program may have it open
    /// for a moment.
    fn open() -> Option<Self> {
        use windows_sys::Win32::System::DataExchange::OpenClipboard as open;
        for _ in 0..5 {
            // SAFETY: opens the clipboard for this thread; closed on drop.
            if unsafe { open(std::ptr::null_mut()) } != 0 {
                return Some(Self);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

impl Drop for OpenClipboard {
    fn drop(&mut self) {
        // SAFETY: this thread opened the clipboard.
        unsafe { windows_sys::Win32::System::DataExchange::CloseClipboard() };
    }
}

/// The paths of the files on the clipboard, in their order: none when it
/// holds no files, or another program keeps it open. It can wait a moment
/// for the clipboard, so it runs off the main thread.
pub fn copied_files() -> Vec<String> {
    use windows_sys::Win32::System::DataExchange::{GetClipboardData, IsClipboardFormatAvailable};
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};
    // CF_HDROP: the files Explorer copies
    const FILES: u32 = 15;
    // SAFETY: asks without opening the clipboard.
    if unsafe { IsClipboardFormatAvailable(FILES) } == 0 {
        return Vec::new();
    }
    let Some(_open) = OpenClipboard::open() else {
        return Vec::new();
    };
    // SAFETY: the clipboard is open while `_open` lives; the handle it
    // gives is read only meanwhile, into buffers of the sizes asked for.
    unsafe {
        let drop = GetClipboardData(FILES) as HDROP;
        if drop.is_null() {
            return Vec::new();
        }
        let count = DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0);
        let mut paths = Vec::new();
        for i in 0..count {
            let len = DragQueryFileW(drop, i, std::ptr::null_mut(), 0) as usize;
            if len == 0 {
                continue;
            }
            let mut buffer = vec![0u16; len.saturating_add(1)];
            let written = DragQueryFileW(drop, i, buffer.as_mut_ptr(), buffer.len() as u32);
            if written > 0 {
                paths.push(String::from_utf16_lossy(&buffer[..written as usize]));
            }
        }
        paths
    }
}

/// Waits a moment for the paths of the drop numbered `id`, and takes them:
/// none when they did not come. Another drop's, older, are let go of.
pub fn dragged_files(id: u64) -> Vec<String> {
    take_drop(id, Duration::from_secs(1))
}

fn take_drop(id: u64, within: Duration) -> Vec<String> {
    let Ok(dropped) = DROPPED.lock() else {
        return Vec::new();
    };
    let Ok((mut dropped, _)) = ARRIVED.wait_timeout_while(
        dropped,
        within,
        |d| !matches!(d, Some((got, _)) if *got >= id),
    ) else {
        return Vec::new();
    };
    match dropped.take() {
        Some((got, paths)) if got == id => paths,
        // a later drop's: left for it
        Some(later) => {
            *dropped = Some(later);
            Vec::new()
        }
        None => Vec::new(),
    }
}

/// Keeps the paths of the drop numbered `id`.
fn keep_drop(id: u64, paths: Vec<String>) {
    if let Ok(mut dropped) = DROPPED.lock() {
        *dropped = Some((id, paths));
        ARRIVED.notify_all();
    }
}

/// Reads the paths of the files the page posts with a drop, in the
/// window's webview.
pub fn listen_in(webview: &tauri::webview::PlatformWebview) -> windows::core::Result<()> {
    // SAFETY: the controller is the live webview's, on its own thread.
    let core = unsafe { webview.controller().CoreWebView2()? };
    listen(&core)
}

fn listen(webview: &ICoreWebView2) -> windows::core::Result<()> {
    let handler = WebMessageReceivedEventHandler::create(Box::new(|_, args| {
        if let Some(args) = args {
            received(&args);
        }
        Ok(())
    }));
    let mut token = 0;
    // SAFETY: the handler is a COM object WebView2 keeps for as long as
    // the webview lives.
    unsafe { webview.add_WebMessageReceived(&handler, &mut token) }
}

fn received(args: &ICoreWebView2WebMessageReceivedEventArgs) {
    let mut message = PWSTR::null();
    // SAFETY: WebView2 allocates the string, which take_pwstr frees.
    if unsafe { args.TryGetWebMessageAsString(&mut message) }.is_err() {
        return;
    }
    let message = webview2_com::take_pwstr(message);
    let Some(id) = message
        .strip_prefix(DROP)
        .and_then(|rest| rest.strip_prefix(':'))
        .and_then(|id| id.parse().ok())
    else {
        return;
    };
    keep_drop(id, paths_of(args).unwrap_or_default());
}

/// The paths of the `File`s a message carries.
fn paths_of(args: &ICoreWebView2WebMessageReceivedEventArgs) -> windows::core::Result<Vec<String>> {
    let args: ICoreWebView2WebMessageReceivedEventArgs2 = args.cast()?;
    // SAFETY: COM calls on objects WebView2 gave this handler, which keeps
    // them alive while it runs; each path string is freed by take_pwstr.
    unsafe {
        let objects = args.AdditionalObjects()?;
        let mut count = 0;
        objects.Count(&mut count)?;
        let mut paths = Vec::new();
        for i in 0..count {
            let Ok(file) = objects.GetValueAtIndex(i)?.cast::<ICoreWebView2File>() else {
                continue;
            };
            let mut path = PWSTR::null();
            file.Path(&mut path)?;
            paths.push(webview2_com::take_pwstr(path));
        }
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_s_paths_are_its_own_never_an_earlier_one_s() {
        // the paths of drop 1 came, but nothing took them
        keep_drop(1, vec!["old".into()]);
        assert_eq!(
            take_drop(2, Duration::from_millis(50)),
            Vec::<String>::new()
        );
        keep_drop(2, vec!["new".into()]);
        assert_eq!(
            take_drop(2, Duration::from_millis(50)),
            vec!["new".to_string()]
        );
        // drop 4's came before drop 3 asked: 3 gets nothing, 4 still its own
        keep_drop(4, vec!["four".into()]);
        assert_eq!(
            take_drop(3, Duration::from_millis(50)),
            Vec::<String>::new()
        );
        assert_eq!(
            take_drop(4, Duration::from_millis(50)),
            vec!["four".to_string()]
        );
    }
}
