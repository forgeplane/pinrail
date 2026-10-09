//! Notifications on Linux, through the desktop's notification server. A
//! review's notification has a default action, which the server invokes
//! when the person clicks it, and the review then opens. The notification
//! is closed once the review is opened or stops waiting: GNOME keeps a
//! notification in its list until it is closed, and Ubuntu Dock counts
//! that list on the app's icon.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use notify_rust::{Notification, NotificationHandle};
use tauri::AppHandle;

/// The notification on screen for each review, by the review's id.
static SHOWN: Mutex<BTreeMap<String, Arc<NotificationHandle>>> = Mutex::new(BTreeMap::new());

fn shown() -> MutexGuard<'static, BTreeMap<String, Arc<NotificationHandle>>> {
    SHOWN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Shows a notification; a click opens the review when one is named. A
/// task waits for the click and ends when the notification closes.
pub fn notify(app: &AppHandle, title: &str, body: &str, review_id: Option<String>, sound: bool) {
    let mut notification = Notification::new();
    notification.summary(title).body(body).auto_icon();
    if sound {
        notification.sound_name("default");
    }
    if review_id.is_some() {
        notification.action("default", "Open");
    }
    let handle = match notification.show() {
        Ok(handle) => Arc::new(handle),
        Err(error) => {
            eprintln!("pinrail: notification not shown: {error}");
            return;
        }
    };
    let Some(id) = review_id else {
        return;
    };
    shown().insert(id.clone(), handle.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut clicked = false;
        handle
            .wait_for_action_async(|response| clicked = response.is_default_action())
            .await;
        // forget it, unless a later notification for the review took its place
        {
            let mut shown = shown();
            if shown.get(&id).is_some_and(|h| Arc::ptr_eq(h, &handle)) {
                shown.remove(&id);
            }
        }
        if clicked {
            // most servers close a clicked notification, but not every one
            handle.close_async().await;
            let opener = app.clone();
            let _ = app.run_on_main_thread(move || crate::native::open_review(&opener, &id));
        }
    });
}

/// Closes the review's notification, if one is on screen.
pub fn close(review_id: &str) {
    let Some(handle) = shown().remove(review_id) else {
        return;
    };
    tauri::async_runtime::spawn(async move { handle.close_async().await });
}
