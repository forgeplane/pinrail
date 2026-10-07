//! Where the window's buttons are on macOS, where the app draws its own
//! title bar under them. Their size and spacing differ between macOS
//! versions, so the shell places its own controls from what is measured
//! here rather than from fixed numbers.

use serde::Serialize;

/// The window's buttons, in points from the window's top left corner.
#[derive(Debug, PartialEq, Serialize)]
pub struct WindowButtons {
    /// where the last of them, the zoom button, ends
    pub end: f64,
    /// the height of their middle
    pub middle: f64,
}

/// The buttons, from the zoom button's rectangle in window coordinates,
/// which start at the bottom left, and the window's height.
#[cfg(any(target_os = "macos", test))]
fn from_zoom_button(x: f64, y: f64, width: f64, height: f64, window_height: f64) -> WindowButtons {
    WindowButtons {
        end: x + width,
        middle: window_height - (y + height / 2.0),
    }
}

/// Measures the buttons of a window. Call it on the main thread.
#[cfg(target_os = "macos")]
// AppKit is Objective-C, reached only through objc2's unsafe calls
#[allow(unsafe_code)]
pub fn measure(window: &tauri::WebviewWindow) -> Option<WindowButtons> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSRect;

    let ns_window = window.ns_window().ok()? as *mut AnyObject;
    // SAFETY: Tauri hands out the NSWindow it owns, which lives as long as
    // the window, and AppKit is called from the main thread.
    unsafe {
        let ns_window = ns_window.as_ref()?;
        // NSWindowZoomButton
        let zoom: *mut AnyObject = msg_send![ns_window, standardWindowButton: 2usize];
        let zoom = zoom.as_ref()?;
        let bounds: NSRect = msg_send![zoom, bounds];
        let to_window: *mut AnyObject = std::ptr::null_mut();
        let rect: NSRect = msg_send![zoom, convertRect: bounds, toView: to_window];
        let frame: NSRect = msg_send![ns_window, frame];
        Some(from_zoom_button(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
            frame.size.height,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_are_measured_from_the_window_top_left() {
        // macOS 15, as Tauri places the buttons 12 pt in and 18 pt down:
        // each 14 by 16 pt, 20 pt apart, the zoom button the third
        assert_eq!(
            from_zoom_button(52.0, 692.0, 14.0, 16.0, 720.0),
            WindowButtons {
                end: 66.0,
                middle: 20.0
            }
        );
    }
}
