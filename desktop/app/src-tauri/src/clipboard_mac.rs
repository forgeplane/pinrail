//! The files copied to the clipboard, as Finder copies them, and the files
//! of the last drag: each a file URL on an item of a pasteboard.

// AppKit is Objective-C, reached only through objc2's unsafe calls
#![allow(unsafe_code)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSArray, NSString, NSURL};

/// The paths of the files on the general pasteboard, in their order.
pub fn copied_files() -> Vec<String> {
    let board: Option<Retained<AnyObject>> =
        unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
    board.map(|board| files_on(&board)).unwrap_or_default()
}

/// The paths of the files of the last drag, which the drag pasteboard
/// keeps after the drop.
pub fn dragged_files() -> Vec<String> {
    // NSPasteboardNameDrag
    let name = NSString::from_str("Apple CFPasteboard drag");
    let board: Option<Retained<AnyObject>> =
        unsafe { msg_send![class!(NSPasteboard), pasteboardWithName: &*name] };
    board.map(|board| files_on(&board)).unwrap_or_default()
}

/// The paths of the file URLs on a pasteboard's items.
fn files_on(board: &AnyObject) -> Vec<String> {
    let file_url = NSString::from_str("public.file-url");
    let items: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![board, pasteboardItems] };
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
