//! Notifications on macOS through the UserNotifications framework: the app
//! asks for permission once, a banner shows even while the app is in front,
//! and a click opens the review. Only an app bundle can use the framework;
//! the bare development binary keeps the plugin's notification.

use std::sync::OnceLock;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{AnyThread, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use tauri::AppHandle;

static APP: OnceLock<AppHandle> = OnceLock::new();

/// The request identifier carries the review, so a click knows where to go.
const REVIEW_PREFIX: &str = "review:";

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "WicketNotificationDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            handler.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let identifier = response.notification().request().identifier().to_string();
            if let Some(app) = APP.get() {
                let app = app.clone();
                let _ = app.clone().run_on_main_thread(move || {
                    match identifier.strip_prefix(REVIEW_PREFIX) {
                        Some(id) => crate::native::open_review(&app, id),
                        None => crate::native::open(&app, ""),
                    }
                });
            }
            handler.call(());
        }
    }
);

/// True inside an app bundle, where the framework works.
pub fn available() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

/// Asks for permission and takes the delegate. Call once, on the main thread.
pub fn setup(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let delegate: Retained<Delegate> =
        unsafe { msg_send![super(Delegate::alloc().set_ivars(())), init] };
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // the center keeps a weak reference; the delegate lives as long as the app
    std::mem::forget(delegate);
    let done = RcBlock::new(|granted: Bool, error: *mut NSError| {
        if !granted.as_bool() {
            let why = unsafe { error.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "declined".to_string());
            eprintln!("wicket: notifications are off: {why}");
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert
            | UNAuthorizationOptions::Sound
            | UNAuthorizationOptions::Badge,
        &done,
    );
}

/// Posts a notification; a click opens the review when one is named.
pub fn notify(title: &str, body: &str, review_id: Option<&str>) {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    content.setSound(Some(&UNNotificationSound::defaultSound()));
    let identifier = match review_id {
        Some(id) => format!("{REVIEW_PREFIX}{id}"),
        None => format!(
            "wicket:{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ),
    };
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&identifier),
        &content,
        None,
    );
    let done = RcBlock::new(|error: *mut NSError| {
        if let Some(error) = unsafe { error.as_ref() } {
            eprintln!(
                "wicket: notification not shown: {}",
                error.localizedDescription()
            );
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, Some(&done));
}
