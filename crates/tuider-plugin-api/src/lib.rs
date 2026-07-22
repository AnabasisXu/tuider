//! Tuider plugin ABI (version 1).
//!
//! # Dynamic loading model
//! Plugins are **cdylib** files in a plugins directory. The host binary does
//! **not** link plugin crates. Only files present on disk can be used.
//!
//! # Contract
//! - Plugin returns **UTF-8 text** (markdown/plain) for bodies.
//! - Host owns ratatui rendering (`md` / plain lines).
//! - All heap strings from plugin must be freed with [`tuider_string_free`].

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

/// Bump when breaking C ABI.
pub const TUIDER_PLUGIN_ABI: u32 = 1;

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

// ── Helpers for plugin authors ────────────────────────────────────────────

/// Allocate a C string for ABI return values.
pub fn cstring_or_null(s: &str) -> *mut c_char {
    CString::new(s.replace('\0', "")).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
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

/// Minimal Rust trait used **inside the host only** (after FFI adaptation).
pub trait ContentSource: Send {
    fn title(&self) -> &str;
    fn entries(&self) -> &[String];
    /// Returns markdown/plain body text (host will render).
    fn load_text(&mut self, index: usize, width: usize) -> Result<String, String>;
}

pub struct LoadResult {
    pub text: String,
    pub status: String,
}
