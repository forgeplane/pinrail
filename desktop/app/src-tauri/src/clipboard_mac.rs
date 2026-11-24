//! The files copied to the clipboard, as Finder copies them: each a file URL
//! on an item of the general pasteboard.

// AppKit is Objective-C, reached only through objc2's unsafe calls
#![allow(unsafe_code)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSArray, NSString, NSURL};

/// The paths of the files on the general pasteboard, in their order.
pub fn copied_files() -> Vec<String> {
    let file_url = NSString::from_str("public.file-url");
    let board: Option<Retained<AnyObject>> =
        unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
    let Some(board) = board else {
        return Vec::new();
    };
    let items: Option<Retained<NSArray<AnyObject>>> =
        unsafe { msg_send![&*board, pasteboardItems] };
    let mut paths = Vec::new();
    for item in items.iter().flat_map(|items| items.iter()) {
        let url: Option<Retained<NSString>> =
            unsafe { msg_send![&*item, stringForType: &*file_url] };
        let path = url
            .and_then(|url| NSURL::URLWithString(&url))
            .and_then(|url| url.path());
        if let Some(path) = path {
            paths.push(path.to_string());
        }
    }
    paths
}
