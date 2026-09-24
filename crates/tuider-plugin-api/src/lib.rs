//! Tuider plugin ABI (version 1).
//!
//! # Dynamic loading model
//! Plugins are **cdylib** files in a plugins directory. The host binary does
//! **not** link plugin crates. Only files present on disk can be used.
//!
//! # Contract
//! - Plugin returns **UTF-8 text** for bodies (see body formats below).
//! - Host owns ratatui rendering (`md` / plain lines / HTML_V1 CSS).
//! - All heap strings from plugin must be freed with [`tuider_string_free`].
//!
//! # Body text formats
//! - Default: UTF-8 markdown or plain text (host renders via md/plain).
//! - HTML envelope (optional): body starts with [`BODY_HTML_V1_PREFIX`], then
//!   `css`, then `"\n\u{1e}\n"`, then `html`. Host runs CSS subset → terminal lines.
//!   This is a **body payload** convention; it does not bump [`TUIDER_PLUGIN_ABI`].

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

/// Bump when breaking C ABI.
pub const TUIDER_PLUGIN_ABI: u32 = 1;

/// Prefix for HTML+CSS body payloads (value stable; do not change without migration).
pub const BODY_HTML_V1_PREFIX: &str = "TUIDER_HTML_V1\n";

// ── C ABI (exported by every plugin .so) ──────────────────────────────────

/// Required: returns [`TUIDER_PLUGIN_ABI`].
pub type FnAbiVersion = unsafe extern "C" fn() -> u32;
/// Required: static C string plugin id ("url", "hn", …).
pub type FnId = unsafe extern "C" fn() -> *const c_char;
/// Required: static C string display name.
pub type FnName = unsafe extern "C" fn() -> *const c_char;
/// Optional: 1 if this plugin claims the given CLI args (argc/argv UTF-8).
pub type FnHandles = unsafe extern "C" fn(argc: c_int, argv: *const *const c_char) -> c_int;
/// Required: open source from CLI. Returns opaque handle or null; on error write msg into err (NUL-terminated).
pub type FnOpen = unsafe extern "C" fn(
    argc: c_int,
    argv: *const *const c_char,
    err: *mut c_char,
    err_len: usize,
) -> *mut c_void;
/// Required: free source handle.
pub type FnClose = unsafe extern "C" fn(src: *mut c_void);
/// Required: heap CString title (caller frees with tuider_string_free).
pub type FnTitle = unsafe extern "C" fn(src: *mut c_void) -> *mut c_char;
/// Required: number of sidebar entries.
pub type FnEntryCount = unsafe extern "C" fn(src: *mut c_void) -> usize;
/// Required: heap CString for entry i (caller frees).
pub type FnEntryAt = unsafe extern "C" fn(src: *mut c_void, index: usize) -> *mut c_char;
/// Required: heap CString body text for entry i (markdown/plain; caller frees).
pub type FnLoadBody =
    unsafe extern "C" fn(src: *mut c_void, index: usize, width: usize) -> *mut c_char;
/// Required: free string returned by title/entry/load.
pub type FnStringFree = unsafe extern "C" fn(s: *mut c_char);
/// Optional dict: lookup word → heap text (JSON lines or multi-entry text).
pub type FnDictLookup = unsafe extern "C" fn(src: *mut c_void, word: *const c_char) -> *mut c_char;
/// Optional dict: prefix search → newline-separated headwords.
pub type FnDictSearch =
    unsafe extern "C" fn(src: *mut c_void, prefix: *const c_char, limit: usize) -> *mut c_char;
/// Optional dict: reverse lookup in definitions → newline-separated headwords.
pub type FnDictReverse =
    unsafe extern "C" fn(src: *mut c_void, query: *const c_char, limit: usize) -> *mut c_char;
/// Optional dict: list loaded dictionary titles → newline-separated.
pub type FnDictList = unsafe extern "C" fn(src: *mut c_void) -> *mut c_char;
/// Optional dict: select active dictionary by index; 1 = ok.
pub type FnDictSelect = unsafe extern "C" fn(src: *mut c_void, index: usize) -> c_int;
/// Optional dict: fulltext search headwords+definitions → newline-separated headwords.
pub type FnDictFulltextSearch =
    unsafe extern "C" fn(src: *mut c_void, query: *const c_char, limit: usize) -> *mut c_char;

// ── Helpers for plugin authors ────────────────────────────────────────────

/// Allocate a C string for ABI return values.
pub fn cstring_or_null(s: &str) -> *mut c_char {
    CString::new(s.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// # Safety
/// `p` must be from [`cstring_or_null`] / `CString::into_raw` or null.
pub unsafe fn free_cstring(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}

/// Read argv[i] as UTF-8 lossy.
///
/// # Safety
/// `argv` must be valid for `argc` pointers.
pub unsafe fn arg_at(argc: c_int, argv: *const *const c_char, i: usize) -> Option<String> {
    if i >= argc as usize || argv.is_null() {
        return None;
    }
    let p = unsafe { *argv.add(i) };
    if p.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

/// Collect all args.
///
/// # Safety
/// Same as [`arg_at`].
pub unsafe fn args_vec(argc: c_int, argv: *const *const c_char) -> Vec<String> {
    (0..argc as usize)
        .filter_map(|i| unsafe { arg_at(argc, argv, i) })
        .collect()
}

pub fn write_err(err: *mut c_char, err_len: usize, msg: &str) {
    if err.is_null() || err_len == 0 {
        return;
    }
    let bytes = msg.as_bytes();
    let n = bytes.len().min(err_len - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), err as *mut u8, n);
        *err.add(n) = 0;
    }
}

// ── Host-side opaque source (implemented in host with libloading) ─────────

/// Text source after FFI adaptation (host loader only).
pub trait PluginTextSource: Send {
    fn title(&self) -> &str;
    fn entries(&self) -> &[String];
    /// Markdown / plain / HTML_V1 envelope.
    fn load_text(&mut self, index: usize, width: usize) -> Result<String, String>;
}
