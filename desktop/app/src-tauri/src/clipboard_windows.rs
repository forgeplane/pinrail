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

/// The message a drop of files is posted with.
pub const DROP: &str = "pinrail:dropped-files";

/// The paths of the last drop, until `dragged_files` takes them.
static DROPPED: Mutex<Option<Vec<String>>> = Mutex::new(None);
static ARRIVED: Condvar = Condvar::new();

/// The paths of the files on the clipboard, in their order: none when it
/// holds no files, or another program keeps it open.
pub fn copied_files() -> Vec<String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};
    // CF_HDROP: the files Explorer copies
    const FILES: u32 = 15;
    // SAFETY: the clipboard is opened for this thread and closed below;
    // the handle it gives is read only while it is open.
    unsafe {
        if IsClipboardFormatAvailable(FILES) == 0 {
            return Vec::new();
        }
        // another program may have it open for a moment
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !opened {
            return Vec::new();
        }
        let drop = GetClipboardData(FILES) as HDROP;
        let mut paths = Vec::new();
        if !drop.is_null() {
            let count = DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0);
            for i in 0..count {
                let len = DragQueryFileW(drop, i, std::ptr::null_mut(), 0);
                let mut buffer = vec![0u16; len as usize + 1];
                let written = DragQueryFileW(drop, i, buffer.as_mut_ptr(), buffer.len() as u32);
                paths.push(String::from_utf16_lossy(&buffer[..written as usize]));
            }
        }
        CloseClipboard();
        paths
    }
}

/// Waits a moment for the paths of the drop the page posted, and takes
/// them: none when no drop came.
pub fn dragged_files() -> Vec<String> {
    let Ok(dropped) = DROPPED.lock() else {
        return Vec::new();
    };
    let Ok((mut dropped, _)) =
        ARRIVED.wait_timeout_while(dropped, Duration::from_secs(1), |d| d.is_none())
    else {
        return Vec::new();
    };
    dropped.take().unwrap_or_default()
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
    if webview2_com::take_pwstr(message) != DROP {
        return;
    }
    let paths = paths_of(args).unwrap_or_default();
    if let Ok(mut dropped) = DROPPED.lock() {
        *dropped = Some(paths);
        ARRIVED.notify_all();
    }
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
