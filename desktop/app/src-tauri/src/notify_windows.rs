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

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::AppHandle;
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    NotificationSetting, ToastDismissedEventArgs, ToastNotification, ToastNotificationManager,
};
use windows::core::{HSTRING, IInspectable};

/// The toasts shown, newest last, each with a key of its own: a toast that
/// is let go of takes its click handler with it, so each is kept until it
/// is clicked, dismissed or closed, up to a bound.
static SHOWN: Mutex<VecDeque<(u64, ToastNotification)>> = Mutex::new(VecDeque::new());
static NEXT: AtomicU64 = AtomicU64::new(1);
const KEPT: usize = 64;

fn shown() -> std::sync::MutexGuard<'static, VecDeque<(u64, ToastNotification)>> {
    SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

fn forget(key: u64) {
    shown().retain(|(k, _)| *k != key);
}

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
        .and_then(|exe| exe.parent().map(installed))
        .unwrap_or(false);
    if installed {
        app.config().identifier.clone()
    } else {
        POWERSHELL.to_string()
    }
}

/// Whether the app in `dir` runs installed, not from a build folder.
fn installed(dir: &std::path::Path) -> bool {
    // target\debug, or target\<triple>\debug for a build for a target
    let names: Vec<_> = dir
        .components()
        .rev()
        .take(3)
        .map(|c| c.as_os_str())
        .collect();
    let profile = names
        .first()
        .is_some_and(|n| *n == "debug" || *n == "release");
    let in_target = names.iter().skip(1).any(|n| *n == "target");
    !(profile && in_target)
}

/// A review's id as a toast's tag, which Windows keeps to 64 characters.
fn tag_of(review_id: &str) -> String {
    review_id.chars().take(64).collect()
}

/// Text for the toast's XML, without the control characters XML cannot
/// hold, such as an agent's colour codes.
fn escape(text: &str) -> String {
    let text: String = text
        .chars()
        .filter(|&c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect();
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
    let key = NEXT.fetch_add(1, Ordering::Relaxed);
    if let Some(id) = &review_id {
        toast.SetTag(&HSTRING::from(tag_of(id)))?;
        toast.SetGroup(&HSTRING::from(GROUP))?;
    }
    // a click opens the review, or the window for a toast of no review;
    // Windows calls it on a thread of its own
    let clicked = app.clone();
    toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
        move |_, _| {
            forget(key);
            let opener = clicked.clone();
            let id = review_id.clone();
            let _ = clicked.run_on_main_thread(move || match id {
                Some(id) => crate::native::open_review(&opener, &id),
                None => crate::native::show(&opener),
            });
            Ok(())
        },
    ))?;
    toast.Dismissed(&TypedEventHandler::<
        ToastNotification,
        ToastDismissedEventArgs,
    >::new(move |_, _| {
        forget(key);
        Ok(())
    }))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id(app)))?
        .Show(&toast)?;
    let mut shown = shown();
    shown.push_back((key, toast));
    while shown.len() > KEPT {
        shown.pop_front();
    }
    Ok(())
}

/// Takes the review's toast off the screen and out of the Action Center.
pub fn close(app: &AppHandle, review_id: &str) {
    let tag = HSTRING::from(tag_of(review_id));
    shown().retain(|(_, toast)| toast.Tag().ok().as_ref() != Some(&tag));
    let removed = ToastNotificationManager::History().and_then(|history| {
        history.RemoveGroupedTagWithId(&tag, &HSTRING::from(GROUP), &HSTRING::from(app_id(app)))
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
    fn a_toast_with_control_characters_in_its_text_still_loads() {
        let xml = super::toast_xml("\u{1b}[31mfailed\u{1b}[0m", "a\0b\u{7}c", true);
        let document = super::XmlDocument::new().unwrap();
        document
            .LoadXml(&super::HSTRING::from(xml.as_str()))
            .unwrap_or_else(|e| panic!("{e}: {xml:?}"));
    }

    #[test]
    fn a_build_for_another_target_is_no_installed_app() {
        use std::path::Path;
        assert!(!super::installed(Path::new(
            r"C:\dev\pinrail\desktop\target\debug"
        )));
        assert!(!super::installed(Path::new(
            r"C:\dev\pinrail\desktop\target\x86_64-pc-windows-msvc\release"
        )));
        assert!(super::installed(Path::new(
            r"C:\Users\ann\AppData\Local\Pinrail"
        )));
    }

    #[test]
    fn a_long_review_id_makes_a_tag_of_64_characters() {
        assert_eq!(super::tag_of(&"x".repeat(100)).chars().count(), 64);
        assert_eq!(super::tag_of("rev_1"), "rev_1");
    }

    /// From a thread nothing initialised COM on, as the runtime's are: the
    /// windows crate starts it itself.
    #[test]
    fn a_toast_is_made_on_a_plain_thread() {
        let made = std::thread::spawn(|| {
            let document = super::XmlDocument::new()?;
            document.LoadXml(&super::HSTRING::from(super::toast_xml("t", "b", true)))?;
            super::ToastNotification::CreateToastNotification(&document).map(|_| ())
        })
        .join()
        .unwrap();
        assert!(made.is_ok(), "{made:?}");
    }

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
