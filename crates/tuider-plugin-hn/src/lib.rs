//! HN plugin cdylib — libtuider_hn.so
//!
//! Sidebar = top stories. Body = meta + full article as markdown (host mdterm).
//! Article fetch is lazy on `load_body` (not at open).
// ponytail: no comment tree yet — add when asked

use std::os::raw::{c_char, c_int, c_void};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use readable_readability::Readability;
use serde::{Deserialize, Serialize};
use tuider_plugin_api::{
    args_vec, cstring_or_null, free_cstring, write_err, TUIDER_PLUGIN_ABI,
};
use url::Url;

const HN_BASE: &str = "https://hacker-news.firebaseio.com/v0";
// TTL mirrors mdx-tui defaults (roughly)
const TTL_TOP: Duration = Duration::from_secs(15 * 60);
const TTL_ITEM: Duration = Duration::from_secs(6 * 60 * 60);
const TTL_PAGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Deserialize, Serialize)]
struct HnItem {
    id: u64,
    #[serde(rename = "type")]
    item_type: Option<String>,
    by: Option<String>,
    title: Option<String>,
    url: Option<String>,
    text: Option<String>,
    score: Option<i64>,
    descendants: Option<u32>,
    kids: Option<Vec<u64>>,
}

struct Story {
    id: u64,
    title: String,
    url: Option<String>,
    /// Self-post HTML from HN API (`text` field).
    text: Option<String>,
    score: i64,
    by: String,
    comments: u32,
    /// Cached markdown body (meta + article).
    body: Option<String>,
}

struct HnState {
    entries: Vec<String>,
    stories: Vec<Story>,
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_abi_version() -> u32 {
    TUIDER_PLUGIN_ABI
}
#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_id() -> *const c_char {
    c"hn".as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_name() -> *const c_char {
    c"Hacker News".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_handles(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = unsafe { args_vec(argc, argv) };
    if args.iter().any(|a| a == "-hn" || a == "--hn") {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_open(
    argc: c_int,
    argv: *const *const c_char,
    err: *mut c_char,
    err_len: usize,
) -> *mut c_void {
    let args = unsafe { args_vec(argc, argv) };
    let mut limit = 30usize;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            if let Ok(n) = args[i + 1].parse() {
                limit = n;
            }
        }
        i += 1;
    }
    match fetch_top(limit) {
        Ok(state) => Box::into_raw(Box::new(state)) as *mut c_void,
        Err(e) => {
            write_err(err, err_len, &e);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_close(src: *mut c_void) {
    if !src.is_null() {
        drop(unsafe { Box::from_raw(src as *mut HnState) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_title(src: *mut c_void) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    cstring_or_null("hn")
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_count(src: *mut c_void) -> usize {
    if src.is_null() {
        0
    } else {
        unsafe { &*(src as *mut HnState) }.entries.len()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_at(src: *mut c_void, index: usize) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut HnState) };
    s.entries
        .get(index)
        .map(|e| cstring_or_null(e))
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_load_body(
    src: *mut c_void,
    index: usize,
    _w: usize,
) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &mut *(src as *mut HnState) };
    let Some(story) = s.stories.get_mut(index) else {
        return cstring_or_null("out of range");
    };
    if story.body.is_none() {
        story.body = Some(load_story_markdown(story));
    }
    cstring_or_null(story.body.as_deref().unwrap_or(""))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}
fn fetch_top(limit: usize) -> Result<HnState, String> {
    // ponytail: firebase occasional connect blip — few tries
    let mut last = String::from("HN: fetch failed");
    for attempt in 0..5 {
        match fetch_top_once(limit) {
            Ok(s) => return Ok(s),
            Err(e) => {
                last = e;
                if attempt + 1 < 5 {
                    std::thread::sleep(std::time::Duration::from_millis(
                        300 * (1u64 << attempt.min(3)),
                    ));
                }
            }
        }
    }
    Err(format!("HN open failed after retries: {last}"))
}

fn fetch_top_once(limit: usize) -> Result<HnState, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("tuider-hn/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let ids: Vec<u64> = match cache_get_fresh("hn/topstories.json", TTL_TOP) {
        Some(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string())?,
        None => {
            let raw = client
                .get(format!("{HN_BASE}/topstories.json"))
                .send()
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?
                .text()
                .map_err(|e| e.to_string())?;
            let ids: Vec<u64> = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let _ = cache_put("hn/topstories.json", &raw);
            ids
        }
    };

    let mut entries = Vec::new();
    let mut stories = Vec::new();
    for id in ids.into_iter().take(limit.max(1)) {
        let item = match load_item_cached(&client, id) {
            Some(i) => i,
            None => continue,
        };
        let title = item.title.clone().unwrap_or_else(|| format!("#{id}"));
        if title.trim().is_empty() {
            continue;
        }
        entries.push(title.clone());
        stories.push(Story {
            id,
            title,
            url: item.url,
            text: item.text,
            score: item.score.unwrap_or(0),
            by: item.by.unwrap_or_else(|| "-".into()),
            comments: item.descendants.unwrap_or(0),
            body: None,
        });
    }
    if entries.is_empty() {
        return Err("HN: no stories".into());
    }
    Ok(HnState { entries, stories })
}

fn load_item_cached(client: &reqwest::blocking::Client, id: u64) -> Option<HnItem> {
    let rel = format!("hn/item/{id}.json");
    if let Some(raw) = cache_get_fresh(&rel, TTL_ITEM) {
        if let Ok(item) = serde_json::from_str::<HnItem>(&raw) {
            return Some(item);
        }
    }
    let raw = client
        .get(format!("{HN_BASE}/item/{id}.json"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .ok()?;
    let item: HnItem = serde_json::from_str(&raw).ok()?;
    let _ = cache_put(&rel, &raw);
    Some(item)
}

fn load_story_markdown(story: &Story) -> String {
    let hn_link = format!("https://news.ycombinator.com/item?id={}", story.id);
    let link = story.url.as_deref().unwrap_or(&hn_link);
    let mut md = format!(
        "# {}\n\n- **score:** {}\n- **by:** {}\n- **comments:** {}\n- **url:** {}\n- **hn:** {}\n\n---\n\n",
        story.title, story.score, story.by, story.comments, link, hn_link
    );

    if let Some(url) = story.url.as_deref() {
        match fetch_article_markdown(url) {
            Ok(article) => md.push_str(&article),
            Err(e) => {
                md.push_str(&format!("_article fetch failed: {e}_\n\n"));
                let fb = story_fallback(story);
                if !fb.starts_with("_no external") {
                    md.push_str(&fb);
                }
            }
        }
    } else {
        md.push_str(&story_fallback(story));
    }
    md
}

fn story_fallback(story: &Story) -> String {
    if let Some(text) = story.text.as_deref() {
        let plain = decode_basic_entities(&strip_tags(text));
        format!("{plain}\n")
    } else {
        "_no external article (self post without text)_\n".into()
    }
}

fn fetch_article_markdown(url: &str) -> Result<String, String> {
    let rel = page_rel(url);
    if let Some(c) = cache_get_fresh(&rel, TTL_PAGE) {
        return Ok(c);
    }
    let html = http_get(url)?;
    let md = html_to_markdown(url, &html);
    // don't cache empty/failed extracts forever — still write; TTL handles refresh
    let _ = cache_put(&rel, &md);
    Ok(md)
}

// ── disk cache (~/.cache/tuider) ───────────────────────────────────────────

fn cache_root() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("tuider");
        }
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".cache/tuider")
}

fn page_rel(url: &str) -> String {
    // FNV-1a 64 — same idea as url plugin
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in url.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("pages/{h:016x}.md")
}

fn cache_get_fresh(rel: &str, max_age: Duration) -> Option<String> {
    let path = cache_root().join(rel);
    let meta = std::fs::metadata(&path).ok()?;
    let modified = meta.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;
    if age > max_age {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn cache_put(rel: &str, text: &str) -> std::io::Result<()> {
    let path = cache_root().join(rel);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, text)
}

fn http_get(url: &str) -> Result<String, String> {
    let parsed = validate_fetch_url(url)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        )
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;
    client
        .get(parsed.as_str())
        .header("accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
        .header("accept-language", "en-US,en;q=0.8")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())
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

/// readability extract → markdown for host mdterm (`looks_like_md` / `render_md_width`).
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
    // text_contents skips script/style; serialize+strip_tags leaks JS
    let body = decode_basic_entities(&node.text_contents());
    let body = collapse_blank_lines(body.trim());
    if body.is_empty() {
        format!("## {title}\n\n> source: {page_url}\n\n_empty extract (paywall/JS page?)_\n")
    } else {
        format!("## {title}\n\n> source: {page_url}\n\n{body}\n")
    }
}

fn strip_tags(s: &str) -> String {
    // drop whole script/style blocks first
    let s = drop_blocks(s, &["script", "style", "noscript"]);
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let name = tag
                    .trim()
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                // block-ish → newline so mdterm wraps readable paragraphs
                if matches!(
                    name.as_str(),
                    "p" | "div" | "br" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "blockquote"
                ) {
                    out.push('\n');
                } else if !out.ends_with(' ') && !out.ends_with('\n') {
                    // inline close: keep text glued
                }
            }
            _ if in_tag => tag.push(c),
            _ => out.push(c),
        }
    }
    out
}

fn drop_blocks(html: &str, tags: &[&str]) -> String {
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        let Some(rel) = lower[i..].find('<') else {
            out.push_str(&html[i..]);
            break;
        };
        let start = i + rel;
        out.push_str(&html[i..start]);
        let after = &lower[start + 1..];
        let mut dropped = false;
        for tag in tags {
            let open = *tag;
            if after.starts_with(open)
                && after
                    .as_bytes()
                    .get(open.len())
                    .is_some_and(|b| !b.is_ascii_alphanumeric())
            {
                let close = format!("</{open}>");
                if let Some(end_rel) = lower[start..].find(&close) {
                    i = start + end_rel + close.len();
                    dropped = true;
                    break;
                }
            }
        }
        if dropped {
            continue;
        }
        // keep this '<' and advance one
        out.push('<');
        i = start + 1;
        let _ = bytes; // silence
    }
    out
}

fn decode_basic_entities(s: &str) -> String {
    let mut out = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    // numeric &#NNN; / &#xHH; — common in HN text fields
    while let Some(start) = out.find("&#") {
        let rest = &out[start + 2..];
        let Some(end) = rest.find(';') else {
            break;
        };
        let num = &rest[..end];
        let ch = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else {
            num.parse::<u32>().ok().and_then(char::from_u32)
        };
        let Some(ch) = ch else {
            let keep = start + 2 + end + 1;
            out = format!("{}{}", &out[..start], &out[keep..]);
            continue;
        };
        out = format!("{}{}{}", &out[..start], ch, &out[start + 2 + end + 1..]);
    }
    out
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0u8;
    for line in s.lines() {
        if line.trim().is_empty() {
            blank = blank.saturating_add(1);
            if blank <= 2 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_markdown_emits_md_heading() {
        let html = r#"
        <html><head><title>Page Title</title></head>
        <body>
          <article>
            <h1>Hello Article</h1>
            <p>First paragraph with <b>bold</b> text.</p>
            <p>Second paragraph.</p>
          </article>
        </body></html>
        "#;
        let md = html_to_markdown("https://example.com/a", html);
        assert!(md.contains("## "), "got: {md}");
        assert!(md.contains("source: https://example.com/a"));
        assert!(md.contains("First paragraph"));
        assert!(md.contains("Second paragraph"));
    }

    #[test]
    fn strip_and_entities() {
        assert_eq!(
            decode_basic_entities(&strip_tags("a &amp; <b>b</b>")),
            "a & b"
        );
        assert_eq!(decode_basic_entities("https:&#x2F;&#x2F;x"), "https://x");
    }

    #[test]
    fn story_markdown_self_post_no_url() {
        let story = Story {
            id: 1,
            title: "Ask HN: test".into(),
            url: None,
            text: Some("Hello &amp; <i>world</i>".into()),
            score: 10,
            by: "alice".into(),
            comments: 2,
            body: None,
        };
        let md = load_story_markdown(&story);
        assert!(md.starts_with("# Ask HN: test"));
        assert!(md.contains("Hello & world"));
        assert!(md.contains("**score:** 10"));
    }

    #[test]
    fn validate_blocks_localhost() {
        assert!(validate_fetch_url("http://localhost/x").is_err());
        assert!(validate_fetch_url("https://example.com/x").is_ok());
    }

    #[test]
    fn disk_cache_roundtrip_and_ttl() {
        // isolate under temp HOME
        let dir = std::env::temp_dir().join(format!("tuider-hn-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: test-only env for cache_root
        unsafe {
            std::env::set_var("XDG_CACHE_HOME", &dir);
        }
        let rel = "hn/test.json";
        cache_put(rel, "{\"ok\":1}").unwrap();
        let got = cache_get_fresh(rel, Duration::from_secs(60)).unwrap();
        assert_eq!(got, "{\"ok\":1}");
        // expired
        assert!(cache_get_fresh(rel, Duration::from_secs(0)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn page_rel_stable() {
        assert_eq!(
            page_rel("https://example.com/a"),
            page_rel("https://example.com/a")
        );
        assert_ne!(
            page_rel("https://example.com/a"),
            page_rel("https://example.com/b")
        );
    }
}
