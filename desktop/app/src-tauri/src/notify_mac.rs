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
    UNAlertStyle, UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotification, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationResponse, UNNotificationSetting, UNNotificationSettings, UNNotificationSound,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use tauri::AppHandle;

/// What macOS will do with the app's notifications, for the settings row.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Status {
    /// `authorized`, `denied`, `not_determined` or `provisional`
    pub authorization: &'static str,
    /// `none`, `banner` or `alert`
    pub alert_style: &'static str,
    pub alerts: bool,
    pub sound: bool,
    pub badge: bool,
    /// whether a notification would appear on screen at all
    pub shows: bool,
}

fn status_of(s: &UNNotificationSettings) -> Status {
    let authorization = match s.authorizationStatus() {
        UNAuthorizationStatus::Denied => "denied",
        UNAuthorizationStatus::NotDetermined => "not_determined",
        UNAuthorizationStatus::Provisional => "provisional",
        _ => "authorized",
    };
    let alert_style = match s.alertStyle() {
        UNAlertStyle::None => "none",
        UNAlertStyle::Alert => "alert",
        _ => "banner",
    };
    let alerts = s.alertSetting() != UNNotificationSetting::Disabled;
    Status {
        authorization,
        alert_style,
        alerts,
        sound: s.soundSetting() != UNNotificationSetting::Disabled,
        badge: s.badgeSetting() != UNNotificationSetting::Disabled,
        shows: authorization != "denied"
            && authorization != "not_determined"
            && alert_style != "none"
            && alerts,
    }
}

/// Asks the notification center how the app stands. Call on the main
/// thread; the answer comes on the center's own queue.
pub fn status(done: impl Fn(Status) + 'static) {
    let report = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
        done(status_of(unsafe { settings.as_ref() }));
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .getNotificationSettingsWithCompletionHandler(&report);
}

static APP: OnceLock<AppHandle> = OnceLock::new();

/// The request identifier carries the review, so a click knows where to go.
const REVIEW_PREFIX: &str = "review:";

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "PinrailNotificationDelegate"]
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

/// True inside an app bundle, where the framework works. A bundle identifier
/// alone is not enough: the development binary carries one in an embedded
/// Info.plist, and the framework throws for it all the same.
pub fn available() -> bool {
    let bundle = NSBundle::mainBundle();
    bundle.bundleIdentifier().is_some() && bundle.bundlePath().to_string().ends_with(".app")
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
            eprintln!("pinrail: notifications are off: {why}");
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert
            | UNAuthorizationOptions::Sound
            | UNAuthorizationOptions::Badge,
        &done,
    );

    // a word on stderr when macOS will not show a banner, and nothing when
    // it will
    status(|s| {
        if !s.sound {
            eprintln!("pinrail: notifications will not sound: the sound is off in System Settings");
        }
        let why = match (s.authorization, s.alert_style, s.alerts) {
            ("denied", _, _) => "not allowed in System Settings",
            ("not_determined", _, _) => "not yet allowed",
            (_, "none", _) => "the alert style is None in System Settings",
            (_, _, false) => "alerts are off in System Settings",
            _ => return,
        };
        eprintln!("pinrail: notifications will not show: {why}");
    });
}

/// The system's notification settings, at the app's own page.
pub fn settings_url() -> String {
    let id = NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|s| s.to_string())
        .unwrap_or_default();
    format!("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id={id}")
}

/// Posts a notification; a click opens the review when one is named.
pub fn notify(title: &str, body: &str, review_id: Option<&str>, sound: bool) {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    if sound {
        content.setSound(Some(&UNNotificationSound::defaultSound()));
    }
    let identifier = match review_id {
        Some(id) => format!("{REVIEW_PREFIX}{id}"),
        None => format!(
            "pinrail:{}",
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
                "pinrail: notification not shown: {}",
                error.localizedDescription()
            );
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, Some(&done));
}
