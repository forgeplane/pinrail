//! libghostty-vt, through its C API.

// The library is called through its C API, which Rust can only reach with
// `unsafe`. Each call states why it is sound.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char};

unsafe extern "C" {
    fn ghostty_version() -> *const c_char;
}

/// The version of the linked libghostty-vt, such as "1.0.0".
pub fn version() -> &'static str {
    // SAFETY: the library returns a pointer to a static, NUL-terminated
    // string that lives as long as the program.
    let version = unsafe { CStr::from_ptr(ghostty_version()) };
    version.to_str().unwrap_or("")
}
