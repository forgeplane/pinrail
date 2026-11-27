//! Notifications on Windows, as toasts: a review's toast opens the review
//! when clicked, carries the review's id as its tag, and is taken out of
//! the Action Center once the review is opened or stops waiting, as
//! notify_mac.rs and notify_linux.rs do. The system's switch for the app's
//! notifications is read as macOS's permission is.
//!
//! A toast names the app it is from by an AppUserModelID, which Windows
//! knows from the Start menu shortcut the installer makes. A development
//! build has none, so it shows its toasts as PowerShell's, as the
//! notification plugin does.

use tauri::AppHandle;
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    NotificationSetting, ToastNotification, ToastNotificationManager,
};
use windows::core::{HSTRING, IInspectable};

/// The group every review's toast is in, which with its tag names it.
const GROUP: &str = "reviews";
/// PowerShell's AppUserModelID, which Windows always knows.
const POWERSHELL: &str =
    "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";

/// The AppUserModelID toasts are shown under: the app's identifier when it
/// runs installed, PowerShell's when it runs from a build folder.
fn app_id(app: &AppHandle) -> String {
    let installed = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .is_some_and(|dir| !(dir.ends_with("target/debug") || dir.ends_with("target/release")));
    if installed {
        app.config().identifier.clone()
    } else {
        POWERSHELL.to_string()
    }
}

/// A review's id as a toast's tag, which Windows keeps to 64 characters.
fn tag_of(review_id: &str) -> String {
    review_id.chars().take(64).collect()
}

/// Text for the toast's XML.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The toast's XML: its title and body, silent when asked.
fn toast_xml(title: &str, body: &str, sound: bool) -> String {
    let audio = if sound {
        ""
    } else {
        r#"<audio silent="true"/>"#
    };
    format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{audio}</toast>"#,
        escape(title),
        escape(body)
    )
}

/// Shows a toast; a click opens the review when one is named, and a later
/// toast for the same review takes its place.
pub fn notify(app: &AppHandle, title: &str, body: &str, review_id: Option<String>, sound: bool) {
    if let Err(error) = show(app, title, body, review_id, sound) {
        eprintln!("pinrail: notification not shown: {error}");
    }
}

fn show(
    app: &AppHandle,
    title: &str,
    body: &str,
    review_id: Option<String>,
    sound: bool,
) -> windows::core::Result<()> {
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(toast_xml(title, body, sound)))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    if let Some(id) = review_id {
        toast.SetTag(&HSTRING::from(tag_of(&id)))?;
        toast.SetGroup(&HSTRING::from(GROUP))?;
        let app = app.clone();
        // Windows calls it on a thread of its own
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, _| {
                let opener = app.clone();
                let id = id.clone();
                let _ = app.run_on_main_thread(move || crate::native::open_review(&opener, &id));
                Ok(())
            },
        ))?;
    }
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id(app)))?.Show(&toast)
}

/// Takes the review's toast off the screen and out of the Action Center.
pub fn close(app: &AppHandle, review_id: &str) {
    let removed = ToastNotificationManager::History().and_then(|history| {
        history.RemoveGroupedTagWithId(
            &HSTRING::from(tag_of(review_id)),
            &HSTRING::from(GROUP),
            &HSTRING::from(app_id(app)),
        )
    });
    // there is none when it was dismissed already
    let _ = removed;
}

/// Whether Windows lets the app's toasts show: off when the person turned
/// off the app's notifications, or all of them, or a policy does.
pub fn allowed(app: &AppHandle) -> Option<bool> {
    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id(app))).ok()?;
    Some(notifier.Setting().ok()? == NotificationSetting::Enabled)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_toast_is_its_title_and_body_escaped() {
        let xml = super::toast_xml("A & B", "<review>", false);
        assert!(
            xml.contains("<text>A &amp; B</text><text>&lt;review&gt;</text>"),
            "{xml}"
        );
        assert!(xml.contains(r#"<audio silent="true"/>"#));
        assert!(!super::toast_xml("t", "b", true).contains("audio"));
    }
}
