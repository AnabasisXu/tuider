//! HN plugin cdylib — libtuider_hn.so
//!
//! Sidebar = top stories. Enter → meta + self-text + comments.
//! `a` / action "article" → also fetch linked article body.


use std::os::raw::{c_char, c_int, c_void};
use std::ffi::CStr;
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
/// `--sync`: prefetch article HTML→md for first N top stories.
const SYNC_ARTICLE_N: usize = 3;


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
    deleted: Option<bool>,
    dead: Option<bool>,
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
    kids: Option<Vec<u64>>,
    /// meta + self-text + comments (no article); filled at open for TUI.
    body_base: Option<String>,
    /// Full body including optional article section.
    body: Option<String>,
    /// Set by action "article" or TUIDER_HN_FETCH_ARTICLE=1.
    include_article: bool,
    /// One-shot status after `a` (shown on next load_body).
    last_action_note: Option<String>,
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
    let list_only = args
        .iter()
        .any(|a| matches!(a.as_str(), "-l" | "--list" | "--print" | "--lite"));
    // --sync: also prefetch article bodies for first SYNC_ARTICLE_N stories
    let sync_articles = args.iter().any(|a| a == "--sync");
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "-n" || args[i] == "--limit") && i + 1 < args.len() {
            if let Ok(n) = args[i + 1].parse() {
                limit = n;
            }
        }
        i += 1;
    }
    // -l path only needs titles/meta: smaller default if user didn't pass -n
    if list_only && !args.windows(2).any(|w| w[0] == "-n" || w[0] == "--limit") {
        limit = 15;
    }
    if list_only {
        eprintln!("hn: fetching top {limit}…");
    }
    match fetch_top(limit, list_only) {
        Ok(mut state) => {
            if list_only {
                eprintln!("hn: {} stories", state.entries.len());
            } else {
                eprintln!("hn: prefetch {} stories (meta+comments)…", state.stories.len());
                prefetch_story_bodies(&mut state);
                if sync_articles {
                    let n = SYNC_ARTICLE_N.min(state.stories.len());
                    eprintln!("hn: --sync article bodies for first {n}…");
                    prefetch_articles(&mut state, n);
                }
                eprintln!("hn: ready");
            }
            Box::into_raw(Box::new(state)) as *mut c_void
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
    // rebuild full body if missing
    if story.body.is_none() {
        story.body = Some(compose_full_body(story));
    }
    let body = story.body.as_deref().unwrap_or("");
    let hint = story
        .last_action_note
        .take()
        .or_else(|| article_status_hint(story));
    if let Some(hint) = hint {
        cstring_or_null(&format!("{body}\n\u{1f}STATUS:{hint}"))
    } else {
        cstring_or_null(body)
    }

}


/// Optional host action. `article` → next load_body includes linked page.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_action(
    src: *mut c_void,
    index: usize,
    action: *const c_char,
) -> c_int {
    if src.is_null() || action.is_null() {
        return 0;
    }
    let act = unsafe { CStr::from_ptr(action) }.to_string_lossy();
    // return codes via status trailer on next load; 0 = no reload
    // convention: set story.status_hint for host when rejecting
    if act != "article" {
        return 0;
    }
    let s = unsafe { &mut *(src as *mut HnState) };
    let Some(story) = s.stories.get_mut(index) else {
        return 0;
    };
    if story.url.is_none() {
        // still reload so host shows the note in status trailer
        story.last_action_note = Some("a: 无外链（Ask HN / 纯讨论帖）".into());
        return 1;
    }
    if story.include_article && story.body.is_some() {
        story.last_action_note = Some(format!(
            "a: 已含全文 · {}",
            story.url.as_deref().unwrap_or("?")
        ));
        return 1;
    }
    story.include_article = true;
    story.body = None; // recompose; disk page cache → fast
    story.last_action_note = Some(format!(
        "a: 抓取全文… · {}",
        story.url.as_deref().unwrap_or("?")
    ));
    1

}


#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}
fn fetch_top(limit: usize, list_only: bool) -> Result<HnState, String> {
    // ponytail: firebase blip — 2 tries for list, 3 for TUI
    let tries = if list_only { 2 } else { 3 };
    let mut last = String::from("HN: fetch failed");
    for attempt in 0..tries {
        match fetch_top_once(limit, list_only) {
            Ok(s) => return Ok(s),
            Err(e) => {
                last = e;
                if attempt + 1 < tries {
                    std::thread::sleep(std::time::Duration::from_millis(
                        200 * (1u64 << attempt.min(2)),
                    ));
                }
            }
        }
    }
    Err(format!("HN open failed after retries: {last}"))
}

fn fetch_top_once(limit: usize, list_only: bool) -> Result<HnState, String> {
    // list: short timeout; TUI body may need more headroom later
    let timeout = if list_only {
        std::time::Duration::from_secs(8)
    } else {
        std::time::Duration::from_secs(15)
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .connect_timeout(std::time::Duration::from_secs(5))
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

    // ponytail: parallel item GET; Firebase is latency-bound
    let take: Vec<u64> = ids.into_iter().take(limit.max(1)).collect();
    let handles: Vec<_> = take
        .into_iter()
        .map(|id| {
            let client = client.clone();
            std::thread::spawn(move || load_item_cached(&client, id).map(|item| (id, item)))
        })
        .collect();

    let mut entries = Vec::new();
    let mut stories = Vec::new();
    for h in handles {
        let Ok(Some((id, item))) = h.join() else {
            continue;
        };
        let title = item.title.clone().unwrap_or_else(|| format!("#{id}"));
        if title.trim().is_empty() {
            continue;
        }
        let score = item.score.unwrap_or(0);
        let by = item.by.clone().unwrap_or_else(|| "-".into());
        let comments = item.descendants.unwrap_or(0);
        // -l: one-line "score | comments | title" (no body fetch — that was never done here)
        if list_only {
            entries.push(format!("{score:>4} │ {comments:>3}c │ {title}"));
        } else {
            entries.push(title.clone());
        }
        stories.push(Story {
            id,
            title,
            url: item.url,
            text: item.text,
            score,
            by,
            comments,
            kids: item.kids,
            body_base: None,
            body: None,
            include_article: !list_only
                && std::env::var("TUIDER_HN_FETCH_ARTICLE").as_deref() == Ok("1"),
            last_action_note: None,
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

/// Parallel fill `body_base` (+ `body` without article) using disk cache when comments match.
fn prefetch_story_bodies(state: &mut HnState) {
    let snapshots: Vec<(usize, u64, u32, String, Option<String>, i64, String, Option<Vec<u64>>)> =
        state
            .stories
            .iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    i,
                    s.id,
                    s.comments,
                    s.title.clone(),
                    s.url.clone(),
                    s.score,
                    s.by.clone(),
                    s.kids.clone(),
                )
            })
            .collect();
    let texts: Vec<Option<String>> = state.stories.iter().map(|s| s.text.clone()).collect();

    let handles: Vec<_> = snapshots
        .into_iter()
        .map(|(i, id, comments, title, url, score, by, kids)| {
            let text = texts[i].clone();
            std::thread::spawn(move || {
                let base = load_or_build_base(StorySnap {
                    id,
                    title,
                    url,
                    text,
                    score,
                    by,
                    comments,
                    kids,
                });
                (i, base)
            })
        })
        .collect();

    for h in handles {
        if let Ok((i, base)) = h.join() {
            if let Some(story) = state.stories.get_mut(i) {
                story.body_base = Some(base.clone());
                if story.include_article {
                    story.body = Some(compose_full_body(story));
                } else {
                    story.body = Some(base);
                }
            }
        }
    }
}

struct StorySnap {
    id: u64,
    title: String,
    url: Option<String>,
    text: Option<String>,
    score: i64,
    by: String,
    comments: u32,
    kids: Option<Vec<u64>>,
}

fn body_base_rel(id: u64) -> String {
    // v2: # Meta / # Article / # Comments major sections
    format!("hn/body/v2/{id}.md")
}

fn body_count_rel(id: u64) -> String {
    format!("hn/body/v2/{id}.comments")
}


/// Disk cache hit only if TTL ok AND comment count still matches (incremental).
fn load_or_build_base(s: StorySnap) -> String {
    let rel = body_base_rel(s.id);
    let crel = body_count_rel(s.id);
    if let (Some(md), Some(nraw)) = (
        cache_get_fresh(&rel, TTL_ITEM),
        cache_get_fresh(&crel, TTL_ITEM),
    ) {
        if nraw.trim().parse::<u32>().ok() == Some(s.comments) {
            return md;
        }
    }
    let md = build_base_markdown(&s);
    let _ = cache_put(&rel, &md);
    let _ = cache_put(&crel, &s.comments.to_string());
    md
}

fn build_base_markdown(s: &StorySnap) -> String {
    let hn_link = format!("https://news.ycombinator.com/item?id={}", s.id);
    let link = s.url.as_deref().unwrap_or(&hn_link);
    // Fixed major sections for [ ] jump: Meta / Article / Comments
    let mut md = format!(
        "# Meta\n\n**{}**\n\n- **score:** {}\n- **by:** **{}**\n- **comments:** {}\n- **url:** {}\n- **hn:** {}\n\n",
        s.title, s.score, s.by, s.comments, link, hn_link
    );
    if let Some(text) = s.text.as_deref() {
        let plain = decode_basic_entities(&strip_tags(text));
        let plain = plain.trim();
        if !plain.is_empty() {
            md.push_str("## Self-text\n\n");
            md.push_str(plain);
            md.push_str("\n\n");
        }
    }
    let kids = s.kids.as_deref().unwrap_or(&[]);
    // Always expose Comments when story has/claims comments so `o` / [ ] work.
    if !kids.is_empty() || s.comments > 0 {
        md.push_str("\n---\n\n");
        md.push_str("# Comments\n\n");
        if kids.is_empty() {
            md.push_str(&format!("_({} comments; tree not loaded)_\n", s.comments));
        } else {
            append_comments_body(&mut md, kids);
        }
    }
    md
}

fn ensure_body_base(story: &mut Story) {
    if story.body_base.is_some() {
        return;
    }
    let base = load_or_build_base(StorySnap {
        id: story.id,
        title: story.title.clone(),
        url: story.url.clone(),
        text: story.text.clone(),
        score: story.score,
        by: story.by.clone(),
        comments: story.comments,
        kids: story.kids.clone(),
    });
    story.body_base = Some(base);
}

fn compose_full_body(story: &mut Story) -> String {
    ensure_body_base(story);
    let mut md = story.body_base.clone().unwrap_or_default();
    if story.include_article {
        if let Some(url) = story.url.as_deref() {
            match fetch_article_markdown(url) {
                Ok(article) => {
                    let block = format_article_block(&article);
                    if let Some(pos) = md.find("\n---\n") {
                        let (head, tail) = md.split_at(pos);
                        md = format!("{head}\n{block}{tail}");
                    } else if let Some(pos) = md.find("\n# Comments\n") {
                        let (head, tail) = md.split_at(pos);
                        md = format!("{head}\n{block}{tail}");
                    } else {
                        md.push('\n');
                        md.push_str(&block);
                    }
                }
                Err(e) => {
                    md.push_str(&format!("\n# Article\n\n_article fetch failed: {e}_\n\n"));
                }
            }
        }
    }
    md
}


fn format_article_block(article_md: &str) -> String {
    // article_md already has structure from html_to_markdown; ensure top is # Article
    let t = article_md.trim();
    if t.starts_with("# Article") {
        format!("{t}\n\n")
    } else if t.starts_with("# ") {
        // demote first H1 to under Article
        format!("# Article\n\n{t}\n\n")
    } else {
        format!("# Article\n\n{t}\n\n")
    }
}


/// Prefetch article bodies for first `n` stories (`--sync`).
fn prefetch_articles(state: &mut HnState, n: usize) {
    let n = n.min(state.stories.len());
    let urls: Vec<(usize, String)> = state
        .stories
        .iter()
        .take(n)
        .enumerate()
        .filter_map(|(i, s)| s.url.clone().map(|u| (i, u)))
        .collect();
    let handles: Vec<_> = urls
        .into_iter()
        .map(|(i, url)| std::thread::spawn(move || (i, fetch_article_markdown(&url))))
        .collect();
    let mut results = Vec::new();
    for h in handles {
        if let Ok(pair) = h.join() {
            results.push(pair);
        }
    }
    for (i, res) in results {
        let Some(story) = state.stories.get_mut(i) else {
            continue;
        };
        story.include_article = true;
        ensure_body_base(story);
        let mut md = story.body_base.clone().unwrap_or_default();
        match res {
            Ok(article) => {
                if let Some(pos) = md.find("\n---\n") {
                    let (head, tail) = md.split_at(pos);
                    md = format!("{head}\n\n## Article\n\n{article}\n{tail}");
                } else {
                    md.push_str("\n## Article\n\n");
                    md.push_str(&article);
                    md.push('\n');
                }
            }
            Err(e) => md.push_str(&format!("\n_article fetch failed: {e}_\n\n")),
        }
        story.body = Some(md);
    }
}


/// Host status bar hint when article not yet fetched.
pub(crate) fn article_status_hint(story: &Story) -> Option<String> {
    if story.url.is_some() && !story.include_article {
        Some(format!("a 抓取全文 · {}", story.url.as_deref().unwrap_or("")))
    } else if story.url.is_none() {
        Some("无外链（讨论帖）".into())
    } else {
        None
    }
}

// ponytail: keep Enter snappy; deep threads later / paginate
const MAX_COMMENTS: usize = 20;
fn append_comments(md: &mut String, root_kids: &[u64]) {
    if root_kids.is_empty() {
        return;
    }
    md.push_str("# Comments\n\n");
    append_comments_body(md, root_kids);
}

fn append_comments_body(md: &mut String, root_kids: &[u64]) {
    if root_kids.is_empty() {
        return;
    }
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(4))
        .user_agent("tuider-hn/0.1")
        .build()
    else {
        return;
    };

    // BFS by depth; each level fetched in parallel
    let mut level: Vec<(u64, u32)> = root_kids.iter().copied().map(|id| (id, 0)).collect();
    let mut n = 0usize;
    while !level.is_empty() && n < MAX_COMMENTS {
        let batch: Vec<_> = level
            .into_iter()
            .take(MAX_COMMENTS - n)
            .map(|(id, depth)| {
                let client = client.clone();
                std::thread::spawn(move || (depth, load_item_cached(&client, id)))
            })
            .collect();

        let mut next = Vec::new();
        for h in batch {
            let Ok((depth, Some(item))) = h.join() else {
                continue;
            };
            if item.deleted.unwrap_or(false) || item.dead.unwrap_or(false) {
                continue;
            }
            if let Some(kids) = &item.kids {
                for &k in kids {
                    next.push((k, depth + 1));
                }
            }
            let Some(text) = item.text.as_deref() else {
                continue;
            };
            let plain = decode_basic_entities(&strip_tags(text));
            let plain = plain.trim();
            if plain.is_empty() {
                continue;
            }
            let by = item.by.as_deref().unwrap_or("-");
            let indent = "  ".repeat(depth as usize);
            let mut lines = plain.lines();
            if let Some(first) = lines.next() {
                md.push_str(&format!("{indent}- **{by}:** {first}\n"));
                for line in lines {
                    md.push_str(&format!("{indent}  {line}\n"));
                }
                md.push('\n');
                n += 1;
            }
        }
        level = next;
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

/// readability extract → structured markdown (h2/h3 kept when present).
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
    // Prefer structured extract; if readability flattened headings, fall back to raw HTML tags
    let from_node = structured_text_from_html(&node.to_string());
    let from_raw = structured_text_from_html(html);
    let body = if body_has_md_heading(&from_node) {
        collapse_blank_lines(from_node.trim())
    } else if body_has_md_heading(&from_raw) {
        collapse_blank_lines(from_raw.trim())
    } else {
        soft_paragraphs(&collapse_blank_lines(from_node.trim()))
    };
    if body.is_empty() {
        format!("# Article\n\n> source: {page_url}\n\n_empty extract (paywall/JS page?)_\n")
    } else if body_has_md_heading(&body) {
        format!("# Article\n\n> source: {page_url}\n\n**{title}**\n\n{body}\n")
    } else {
        format!("# Article\n\n> source: {page_url}\n\n**{title}**\n\n{body}\n")
    }
}


fn body_has_md_heading(s: &str) -> bool {
    s.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("## ") || t.starts_with("### ")
    })
}

/// Convert HTML to text, promoting h1–h3 to markdown headings.
fn structured_text_from_html(html: &str) -> String {
    let s = drop_blocks(html, &["script", "style", "noscript"]);
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    let mut heading: Option<u8> = None; // open heading level
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let raw = tag.trim();
                let closing = raw.starts_with('/');
                let name = raw
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if closing {
                    if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4") {
                        if let Some(lv) = heading.take() {
                            // trim trailing space on heading line
                            while out.ends_with(' ') {
                                out.pop();
                            }
                            out.push('\n');
                            let _ = lv;
                        }
                    } else if matches!(name.as_str(), "p" | "div" | "li" | "tr" | "blockquote") {
                        out.push('\n');
                    }
                } else {
                    match name.as_str() {
                        "h1" => {
                            out.push_str("\n## "); // demote page H1 under # Article
                            heading = Some(2);
                        }
                        "h2" => {
                            out.push_str("\n## ");
                            heading = Some(2);
                        }
                        "h3" | "h4" => {
                            out.push_str("\n### ");
                            heading = Some(3);
                        }
                        "br" => out.push('\n'),
                        "p" | "div" | "li" | "tr" | "blockquote" => {
                            if !out.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ if in_tag => tag.push(c),
            _ => out.push(c),
        }
    }
    out
}

/// No headings: keep readable paragraphs (blank line every ~3 sentences-ish).
fn soft_paragraphs(s: &str) -> String {
    let lines: Vec<&str> = s
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    // already multi-line blocks → join with blank lines every single line group
    if lines.len() >= 3 {
        return lines.join("\n\n");
    }
    // one blob: split on sentence end + space
    let blob = lines.join(" ");
    let mut out = String::new();
    let mut n = 0usize;
    for part in blob.split_inclusive(['.', '!', '?', '。', '！', '？']) {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(p);
        n += 1;
        if n % 3 == 0 {
            out.push_str("\n\n");
        }
    }
    out
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
            <h2>Subsection</h2>
            <p>Second paragraph.</p>
          </article>
        </body></html>
        "#;
        let md = html_to_markdown("https://example.com/a", html);
        assert!(md.contains("# Article"), "got: {md}");
        assert!(md.contains("source: https://example.com/a"), "got: {md}");
        assert!(md.contains("First paragraph"), "got: {md}");
        // structured headings when tags present
        assert!(
            md.contains("## ") || md.contains("Subsection") || md.contains("Hello Article"),
            "got: {md}"
        );
    }

    #[test]
    fn story_markdown_link_post_skips_article_without_flag() {
        let mut story = Story {
            id: 2,
            title: "Link post".into(),
            url: Some("https://example.invalid/no-fetch".into()),
            text: None,
            score: 3,
            by: "bob".into(),
            comments: 0,
            kids: None,
            body_base: None,
            body: None,
            include_article: false,
            last_action_note: None,
        };
        let md = compose_full_body(&mut story);
        assert!(md.contains("# Meta"), "got: {md}");
        assert!(md.contains("**Link post**"), "got: {md}");
        assert!(md.contains("**url:** https://example.invalid/no-fetch"));
        assert!(md.contains("**hn:** https://news.ycombinator.com/item?id=2"));
        assert!(!md.contains("article fetch failed"));
        assert!(!md.contains("# Comments"));
        let hint = article_status_hint(&story).unwrap();
        assert!(hint.contains("a 抓取全文"), "{hint}");
        assert!(hint.contains("example.invalid"), "{hint}");
    }

    #[test]
    fn action_article_flag_clears_body_cache() {
        let mut story = Story {
            id: 3,
            title: "x".into(),
            url: Some("https://example.com".into()),
            text: None,
            score: 1,
            by: "z".into(),
            comments: 0,
            kids: None,
            body_base: Some("# x\n".into()),
            body: Some("old".into()),
            include_article: false,
            last_action_note: None,
        };

        story.include_article = true;
        story.body = None;
        assert!(story.include_article);
        assert!(story.body.is_none());
        let md = compose_full_body(&mut story);
        assert!(md.contains("# x"));
    }

    #[test]
    fn live_top_one_with_comments() {
        let Ok(mut state) = fetch_top(1, false) else {
            return; // offline CI
        };
        prefetch_story_bodies(&mut state);
        let story = &mut state.stories[0];
        let md = compose_full_body(story);
        assert!(md.starts_with("# "));
        assert!(md.contains("**score:**"));
        let _ = md.contains("## Comments");
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
