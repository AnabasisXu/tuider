//! URL plugin cdylib — install as `libtuider_url.so`.

use std::os::raw::{c_char, c_int, c_void};
use std::path::PathBuf;

use readable_readability::Readability;
use tuider_plugin_api::{
    args_vec, cstring_or_null, free_cstring, write_err, TUIDER_PLUGIN_ABI,
};
use url::Url;

struct UrlState {
    title: String,
    markdown: String,
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_abi_version() -> u32 {
    TUIDER_PLUGIN_ABI
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_id() -> *const c_char {
    c"url".as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_name() -> *const c_char {
    c"URL fetch".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_handles(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = unsafe { args_vec(argc, argv) };
    for (i, a) in args.iter().enumerate() {
        if a == "-u" || a == "--url" {
            return 1;
        }
        if a.starts_with("http://") || a.starts_with("https://") {
            return 1;
        }
        if i + 1 < args.len() && (a == "-u" || a == "--url") {
            return 1;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_open(
    argc: c_int,
    argv: *const *const c_char,
    err: *mut c_char,
    err_len: usize,
) -> *mut c_void {
    let args = unsafe { args_vec(argc, argv) };
    let mut url = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-u" || args[i] == "--url" {
            if i + 1 < args.len() {
                url = Some(args[i + 1].clone());
                break;
            }
        } else if args[i].starts_with("http://") || args[i].starts_with("https://") {
            url = Some(args[i].clone());
            break;
        }
        i += 1;
    }
    let Some(url) = url else {
        write_err(err, err_len, "url plugin: need -u URL or bare http(s) URL");
        return std::ptr::null_mut();
    };
    match fetch_url_markdown(&url) {
        Ok(md) => {
            let title = first_heading(&md).unwrap_or_else(|| url.clone());
            let boxed = Box::new(UrlState {
                title,
                markdown: md,
            });
            Box::into_raw(boxed) as *mut c_void
        }
        Err(e) => {
            write_err(err, err_len, &e);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_close(src: *mut c_void) {
    if !src.is_null() {
        drop(unsafe { Box::from_raw(src as *mut UrlState) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_title(src: *mut c_void) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut UrlState) };
    cstring_or_null(&s.title)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_count(src: *mut c_void) -> usize {
    if src.is_null() {
        0
    } else {
        1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_at(src: *mut c_void, index: usize) -> *mut c_char {
    if src.is_null() || index != 0 {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut UrlState) };
    cstring_or_null(&s.title)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_load_body(
    src: *mut c_void,
    index: usize,
    _width: usize,
) -> *mut c_char {
    if src.is_null() || index != 0 {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut UrlState) };
    cstring_or_null(&s.markdown)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}

// ── fetch (self-contained) ────────────────────────────────────────────────

fn fetch_url_markdown(url: &str) -> Result<String, String> {
    let rel = page_rel(url);
    if let Some(c) = cache_get(&rel) {
        return Ok(c);
    }
    let html = http_get(url)?;
    let md = html_to_markdown(url, &html);
    let _ = cache_put(&rel, &md);
    Ok(md)
}

fn http_get(url: &str) -> Result<String, String> {
    let parsed = validate_fetch_url(url)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("tuider-url/0.1")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(parsed.as_str())
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    resp.text().map_err(|e| e.to_string())
}

fn validate_fetch_url(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("only http/https".into());
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".local") {
        return Err("blocked local host".into());
    }
    Ok(parsed)
}

fn html_to_markdown(page_url: &str, html: &str) -> String {
    let mut r = Readability::new();
    if let Ok(u) = Url::parse(page_url) {
        r.base_url(Some(u));
    }
    let (node, meta) = r.parse(html);
    let title = meta
        .article_title
        .or(meta.page_title)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| page_url.to_string());
    let mut fragment = Vec::new();
    let body = if node.serialize(&mut fragment).is_ok() {
        strip_tags(&String::from_utf8_lossy(&fragment))
    } else {
        node.text_contents()
    };
    format!("# {title}\n\n> source: {page_url}\n\n{body}\n")
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn first_heading(md: &str) -> Option<String> {
    md.lines()
        .find_map(|l| l.strip_prefix("# ").map(|s| s.trim().to_string()))
}

fn cache_root() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("tuider");
        }
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".cache/tuider")
}

fn page_rel(url: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in url.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("pages/{h:016x}.md")
}

fn cache_get(rel: &str) -> Option<String> {
    std::fs::read_to_string(cache_root().join(rel)).ok()
}

fn cache_put(rel: &str, text: &str) -> std::io::Result<()> {
    let path = cache_root().join(rel);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, text)
}
