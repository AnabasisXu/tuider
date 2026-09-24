//! URL plugin cdylib — install as `libtuider_url.so`.

use std::os::raw::{c_char, c_int, c_void};
use std::path::PathBuf;

use readable_readability::Readability;
use tuider_plugin_api::{TUIDER_PLUGIN_ABI, args_vec, cstring_or_null, free_cstring, write_err};
use url::Url;

struct UrlState {
    title: String,
    items: Vec<FeedItem>,
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
    if args
        .iter()
        .any(|a| a.starts_with("http://") || a.starts_with("https://"))
    {
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
    let url = args
        .iter()
        .find(|a| a.starts_with("http://") || a.starts_with("https://"))
        .cloned();
    let Some(url) = url else {
        write_err(err, err_len, "url plugin: need bare http(s) URL");
        return std::ptr::null_mut();
    };
    match open_url(&url) {
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
        let s = unsafe { &*(src as *mut UrlState) };
        s.items.len()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_at(src: *mut c_void, index: usize) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut UrlState) };
    s.items
        .get(index)
        .map(|i| cstring_or_null(&i.title))
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_load_body(
    src: *mut c_void,
    index: usize,
    _width: usize,
) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut UrlState) };
    s.items
        .get(index)
        .map(|i| cstring_or_null(&i.body_md))
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}

// ── fetch (self-contained) ────────────────────────────────────────────────

fn open_url(url: &str) -> Result<UrlState, String> {
    if let Some(raw) = cache_get(&feed_rel(url)) {
        if let Ok((title, mut items)) = parse_feed_items(&raw) {
            // still expand: cache is first page only
            items = expand_feed_window(url, items);
            items = filter_feed_age(items);
            if !items.is_empty() {
                return Ok(UrlState { title, items });
            }
        }
    }
    if let Some(md) = cache_get(&page_rel(url)) {
        if md.trim_start().starts_with('#') || !md.trim_start().starts_with('<') {
            let title = first_heading(&md).unwrap_or_else(|| url.to_string());
            return Ok(UrlState {
                title: title.clone(),
                items: vec![FeedItem {
                    title,
                    body_md: md,
                    link: Some(url.to_string()),
                    published: None,
                }],
            });
        }
        return Ok(state_from_body(url, &md, None));
    }
    let (final_url, ct, body) = http_get(url)?;
    let mut state = state_from_body(&final_url, &body, ct.as_deref());
    if state.items.len() > 1 || parse_feed_items(&body).is_ok() {
        // feed: expand archives if newest window is short of 6 months
        state.items = expand_feed_window(&final_url, state.items);
        state.items = filter_feed_age(state.items);
        // cache merged as synthetic multi-xml? store first page only + rebuild on expand
        let _ = cache_put(&feed_rel(&final_url), &body);
        if final_url != url {
            let _ = cache_put(&feed_rel(url), &body);
        }
    } else if let Some(item) = state.items.first() {
        let _ = cache_put(&page_rel(&final_url), &item.body_md);
        if final_url != url {
            let _ = cache_put(&page_rel(url), &item.body_md);
        }
    }
    Ok(state)
}

fn state_from_body(url: &str, body: &str, content_type: Option<&str>) -> UrlState {
    let try_feed = content_type
        .map(|ct| {
            let c = ct.to_ascii_lowercase();
            c.contains("xml") || c.contains("rss") || c.contains("atom") || c.contains("rdf")
        })
        .unwrap_or(false)
        || body.trim_start().starts_with('<');
    if try_feed {
        if let Ok((title, items)) = parse_feed_items(body) {
            return UrlState { title, items };
        }
    }
    let md = html_to_markdown(url, body);
    let title = first_heading(&md).unwrap_or_else(|| url.to_string());
    UrlState {
        title: title.clone(),
        items: vec![FeedItem {
            title,
            body_md: md,
            link: Some(url.to_string()),
            published: None,
        }],
    }
}

fn filter_feed_age(items: Vec<FeedItem>) -> Vec<FeedItem> {
    let cutoff = now_unix().saturating_sub(FEED_MAX_AGE_DAYS * 86400);
    items
        .into_iter()
        .filter(|i| match item_time(i) {
            Some(t) => t >= cutoff,
            // undated: keep (rare real feed items); archive usually has title/URL date
            None => true,
        })
        .collect()
}

fn item_time(item: &FeedItem) -> Option<i64> {
    item.published
        .or_else(|| date_prefix(&item.title))
        .or_else(|| item.link.as_deref().and_then(date_in_url))
}

fn date_prefix(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.len() >= 10 && t.as_bytes().get(4) == Some(&b'-') && t.as_bytes().get(7) == Some(&b'-') {
        parse_iso_date(&t[..10])
    } else {
        None
    }
}

fn date_in_url(url: &str) -> Option<i64> {
    // .../2025/12/2025-12-29-emacs-news/ or .../2025-12-29-...
    for part in url.split('/') {
        if let Some(t) = date_prefix(part) {
            return Some(t);
        }
        // /2025/12/ segments — need year+month+day; skip bare year
    }
    None
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// If feed is short (WP often 7–10 items ≈ 1–2 months for weekly), crawl category pages.
fn expand_feed_window(feed_url: &str, mut items: Vec<FeedItem>) -> Vec<FeedItem> {
    let cutoff = now_unix().saturating_sub(FEED_MAX_AGE_DAYS * 86400);
    let oldest = items.iter().filter_map(item_time).min();
    let need = match oldest {
        Some(t) if t > cutoff && items.len() < 80 => true,
        None if items.len() < 20 => true,
        _ => false,
    };
    if !need {
        return items;
    }
    let Some(base) = category_base_from_feed(feed_url) else {
        return items;
    };
    let mut seen: std::collections::HashSet<String> =
        items.iter().filter_map(|i| i.link.clone()).collect();
    for page in 2..=FEED_ARCHIVE_MAX_PAGES {
        let page_url = format!("{base}page/{page}/");
        let Ok((_fu, _ct, html)) = http_get(&page_url) else {
            break;
        };
        let extra = archive_items_from_html(&html);
        if extra.is_empty() {
            break;
        }
        let mut added = 0;
        let mut hit_old = false;
        for it in extra {
            let key = it.link.clone().unwrap_or_else(|| it.title.clone());
            if !seen.insert(key) {
                continue;
            }
            if let Some(t) = item_time(&it) {
                if t < cutoff {
                    hit_old = true;
                    continue; // drop older than window — do not keep
                }
            }
            items.push(it);
            added += 1;
        }
        if added == 0 || hit_old {
            break;
        }
    }
    // sort newest first
    items.sort_by(|a, b| item_time(b).cmp(&item_time(a)));
    items
}

fn category_base_from_feed(feed_url: &str) -> Option<String> {
    // https://host/.../feed/ or .../feed → category base ending with /
    let u = feed_url.trim_end_matches('/');
    let base = u.strip_suffix("/feed").or_else(|| u.strip_suffix("/rss"))?;
    let mut b = base.to_string();
    if !b.ends_with('/') {
        b.push('/');
    }
    Some(b)
}

fn archive_items_from_html(html: &str) -> Vec<FeedItem> {
    // ponytail: WP-ish h2>a entry titles + nearby time datetime=
    // scan bytes; only slice str at ASCII tag boundaries (never mid-char)
    let bytes = html.as_bytes();
    let mut items = Vec::new();
    let mut i = 0;
    while i + 4 < bytes.len() {
        if bytes[i] != b'<' || !bytes[i + 1].eq_ignore_ascii_case(&b'h') || bytes[i + 2] != b'2' {
            i += 1;
            continue;
        }
        let Some(gt) = html[i..].find('>') else {
            break;
        };
        let after = i + gt + 1;
        if after >= html.len() {
            break;
        }
        let Some(end_h2) = find_ascii_ci(&html[after..], b"</h2>") else {
            break;
        };
        let inner = &html[after..after + end_h2];
        i = after + end_h2 + 5;
        let Some(a_pos) = find_ascii_ci(inner, b"<a ") else {
            continue;
        };
        let Some(gt_rel) = inner[a_pos..].find('>') else {
            continue;
        };
        let a_tag_end = a_pos + gt_rel;
        let a_open = &inner[a_pos..=a_tag_end];
        let Some(href) = attr_value(a_open, "href") else {
            continue;
        };
        if !href.contains("/blog/") && !href.contains("http") {
            continue;
        }
        let text_start = a_tag_end + 1;
        let Some(a_close) = find_ascii_ci(&inner[text_start..], b"</a>") else {
            continue;
        };
        let title = strip_tags(&inner[text_start..text_start + a_close])
            .trim()
            .to_string();
        let title_norm = title.trim_end_matches(':').trim();
        if title.is_empty()
            || title_norm.eq_ignore_ascii_case("categories")
            || title_norm.eq_ignore_ascii_case("category")
        {
            continue;
        }
        let mut end = (i + 800).min(html.len());
        while end > i && !html.is_char_boundary(end) {
            end -= 1;
        }
        let published = extract_datetime_attr(&html[i..end]);
        let body_md =
            format!("# {title}\n\n> link: {href}\n\n(archive stub — open link for full post)\n");
        items.push(FeedItem {
            title: title.chars().take(ENTRY_TITLE_MAX).collect(),
            body_md,
            link: Some(href),
            published,
        });
    }
    items
}

/// Case-insensitive search for ASCII `needle` in `hay`. Match start is char-safe.
fn find_ascii_ci(hay: &str, needle: &[u8]) -> Option<usize> {
    let h = hay.as_bytes();
    if needle.is_empty() || h.len() < needle.len() {
        return None;
    }
    'outer: for i in 0..=h.len() - needle.len() {
        for j in 0..needle.len() {
            if !h[i + j].eq_ignore_ascii_case(&needle[j]) {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

fn extract_datetime_attr(s: &str) -> Option<i64> {
    let low = s.to_ascii_lowercase();
    let key = "datetime=\"";
    let idx = low.find(key)?;
    let rest = &s[idx + key.len()..];
    let end = rest.find('"')?;
    let raw = &rest[..end];
    // 2026-05-25T12:00:00+00:00 or 2026-05-25
    parse_iso_date(raw)
}

fn parse_iso_date(raw: &str) -> Option<i64> {
    // YYYY-MM-DD
    if raw.len() < 10 {
        return None;
    }
    let y: i32 = raw.get(0..4)?.parse().ok()?;
    let m: u32 = raw.get(5..7)?.parse().ok()?;
    let d: u32 = raw.get(8..10)?.parse().ok()?;
    // days since epoch approx without chrono: use time crate? std only — manual
    days_since_epoch(y, m, d).map(|days| days * 86400)
}

fn days_since_epoch(y: i32, m: u32, d: u32) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // civil_from_days inverse (Howard Hinnant)
    let y = y as i64;
    let m = m as i64;
    let d = d as i64;
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn map_reqwest_err(e: reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_request() {
        "request"
    } else if e.is_status() {
        if let Some(s) = e.status() {
            return format!("http {s}");
        }
        "http"
    } else {
        "network"
    };
    format!("{kind}: {e}")
}

fn http_get(url: &str) -> Result<(String, Option<String>, String), String> {
    let parsed = validate_fetch_url(url)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("tuider-url/0.1")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // re-check every hop; Policy::limited only counts hops
            match validate_fetch_url(attempt.url().as_str()) {
                Ok(_) if attempt.previous().len() >= 5 => attempt.error("too many redirects"),
                Ok(_) => attempt.follow(),
                Err(e) => attempt.error(e),
            }
        }))
        .build()
        .map_err(map_reqwest_err)?;
    let resp = client
        .get(parsed.as_str())
        .send()
        .map_err(map_reqwest_err)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("http {status}"));
    }
    let final_url = resp.url().as_str().to_string();
    // final URL may differ even without redirect chain metadata
    validate_fetch_url(&final_url)?;
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let text = resp.text().map_err(map_reqwest_err)?;
    Ok((final_url, ct, text))
}

fn validate_fetch_url(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("only http/https".into());
    }
    // ponytail: host-string SSRF guard; no DNS resolve
    match parsed.host() {
        None => return Err("blocked local host".into()),
        Some(url::Host::Domain(d)) => {
            let host = d.to_ascii_lowercase();
            if host.is_empty() || host == "localhost" || host.ends_with(".local") {
                return Err("blocked local host".into());
            }
        }
        Some(url::Host::Ipv4(v4)) if is_blocked_v4(v4) => {
            return Err("blocked local/private host".into());
        }
        Some(url::Host::Ipv6(v6)) if is_blocked_v6(v6) => {
            return Err("blocked local/private host".into());
        }
        Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_)) => {}
    }
    Ok(parsed)
}

fn is_blocked_v4(v4: std::net::Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.octets()[0] == 0
        || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1])) // CGNAT
}

fn is_blocked_v6(v6: std::net::Ipv6Addr) -> bool {
    v6.is_loopback()
        || v6.is_unspecified()
        || v6.is_unique_local()
        || v6.is_unicast_link_local()
        || v6.to_ipv4_mapped().is_some_and(is_blocked_v4)
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

const ENTRY_TITLE_MAX: usize = 120;
/// Keep feed items newer than this (days).
const FEED_MAX_AGE_DAYS: i64 = 180;
/// Cap archive page crawls (ponytail: enough for ~6mo weekly posts).
const FEED_ARCHIVE_MAX_PAGES: usize = 30;

struct FeedItem {
    title: String,
    body_md: String,
    link: Option<String>,
    /// Unix secs; None = keep (unknown date).
    published: Option<i64>,
}

/// Returns (feed_title, items). Err if not a feed or zero items.
fn parse_feed_items(xml: &str) -> Result<(String, Vec<FeedItem>), String> {
    let feed = feed_rs::parser::parse(xml.as_bytes()).map_err(|e| e.to_string())?;
    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Feed".into());
    let mut items = Vec::new();
    for e in feed.entries {
        let link = e
            .links
            .iter()
            .map(|l| l.href.clone())
            .find(|h| !h.is_empty());
        let mut title = e
            .title
            .as_ref()
            .map(|t| t.content.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| link.clone())
            .unwrap_or_else(|| "Untitled".into());
        if title.chars().count() > ENTRY_TITLE_MAX {
            title = title.chars().take(ENTRY_TITLE_MAX).collect();
        }
        let published = e.published.or(e.updated).map(|dt| dt.timestamp());
        let html_or_text = e
            .content
            .as_ref()
            .and_then(|c| c.body.as_ref())
            .map(|s| s.as_str())
            .or_else(|| e.summary.as_ref().map(|t| t.content.as_str()))
            .unwrap_or("");
        let body_core = if html_or_text.is_empty() {
            String::new()
        } else if html_or_text.contains('<') {
            html_fragment_to_md(html_or_text)
        } else {
            html_or_text.to_string()
        };
        let body_md = match (&link, body_core.is_empty()) {
            (Some(u), true) => format!("# {title}\n\n> link: {u}\n\n(no content in feed)\n"),
            (Some(u), false) => format!("# {title}\n\n> link: {u}\n\n{}\n", body_core.trim()),
            (None, true) => format!("# {title}\n\n(no content in feed)\n"),
            (None, false) => format!("# {title}\n\n{}\n", body_core.trim()),
        };
        items.push(FeedItem {
            title,
            body_md,
            link,
            published,
        });
    }
    if items.is_empty() {
        return Err("empty feed".into());
    }
    Ok((feed_title, items))
}

/// Lightweight HTML fragment → markdown (links, lists, headings, bold).
fn html_fragment_to_md(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut text = String::new();
    let mut in_tag = false;
    let mut in_a = false;
    let mut a_href = String::new();
    let mut a_text = String::new();
    let mut bold = 0i32;

    let flush_text = |out: &mut String, bold: i32, text: &mut String| {
        if text.is_empty() {
            return;
        }
        let t = decode_basic_entities(text);
        text.clear();
        if bold > 0 {
            let lead: String = t.chars().take_while(|c| c.is_whitespace()).collect();
            let trail_ws = t.len() - t.trim_end().len();
            let core = t.trim();
            out.push_str(&lead);
            if !core.is_empty() {
                out.push_str("**");
                out.push_str(core);
                out.push_str("**");
            }
            if trail_ws > 0 {
                out.push_str(&t[t.len() - trail_ws..]);
            }
        } else {
            out.push_str(&t);
        }
    };

    for c in html.chars() {
        if c == '<' {
            if in_a {
                // still collecting tag? no — end text into a_text
            } else {
                flush_text(&mut out, bold, &mut text);
            }
            in_tag = true;
            tag.clear();
            continue;
        }
        if in_tag {
            if c == '>' {
                in_tag = false;
                let raw = tag.trim().to_string();
                let closing = raw.starts_with('/');
                let body = if closing {
                    raw[1..].trim()
                } else {
                    raw.as_str()
                };
                let name = body
                    .split(|ch: char| ch.is_whitespace() || ch == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                match (closing, name.as_str()) {
                    (false, "a") => {
                        if in_a {
                            let label = if a_text.trim().is_empty() {
                                a_href.clone()
                            } else {
                                decode_basic_entities(a_text.trim())
                            };
                            if !a_href.is_empty() {
                                out.push_str(&format!("[{label}]({a_href})"));
                            } else {
                                out.push_str(&label);
                            }
                        }
                        in_a = true;
                        a_href = attr_value(&raw, "href").unwrap_or_default();
                        a_text.clear();
                    }
                    (true, "a") => {
                        if in_a {
                            let label = if a_text.trim().is_empty() {
                                a_href.clone()
                            } else {
                                decode_basic_entities(a_text.trim())
                            };
                            if !a_href.is_empty() {
                                out.push_str(&format!("[{label}]({a_href})"));
                            } else {
                                out.push_str(&label);
                            }
                        }
                        in_a = false;
                        a_href.clear();
                        a_text.clear();
                    }
                    (false, "br") => out.push('\n'),
                    (false, "p") | (false, "div") => {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    (true, "p") | (true, "div") => out.push_str("\n\n"),
                    (false, "li") => {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str("- ");
                    }
                    (true, "li") => out.push('\n'),
                    (true, "ul") | (true, "ol") => out.push('\n'),
                    (false, "h1") => {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str("## ");
                    }
                    (false, "h2") => {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str("### ");
                    }
                    (false, "h3") | (false, "h4") | (false, "h5") | (false, "h6") => {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str("#### ");
                    }
                    (true, "h1")
                    | (true, "h2")
                    | (true, "h3")
                    | (true, "h4")
                    | (true, "h5")
                    | (true, "h6") => out.push_str("\n\n"),
                    (false, "b") | (false, "strong") => {
                        flush_text(&mut out, bold, &mut text);
                        bold += 1;
                    }
                    (true, "b") | (true, "strong") => {
                        flush_text(&mut out, bold, &mut text);
                        bold = (bold - 1).max(0);
                    }
                    _ => {}
                }
            } else {
                tag.push(c);
            }
            continue;
        }
        if in_a {
            a_text.push(c);
        } else {
            text.push(c);
        }
    }
    if in_a {
        let label = if a_text.trim().is_empty() {
            a_href.clone()
        } else {
            decode_basic_entities(a_text.trim())
        };
        if !a_href.is_empty() {
            out.push_str(&format!("[{label}]({a_href})"));
        } else {
            out.push_str(&label);
        }
    } else {
        flush_text(&mut out, bold, &mut text);
    }
    promote_li_section_labels(&collapse_blank_lines(&out))
}

/// `- New packages:` style org labels → `### New packages` for md heading color.
fn promote_li_section_labels(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for line in md.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("- ") {
            let rest = rest.trim();
            if rest.ends_with(':') {
                let label = rest.trim_end_matches(':').trim();
                let chars = label.chars().count();
                // ponytail: short bare labels only (not full sentences)
                if (2..=40).contains(&chars) && !label.contains('[') && !label.contains('(') {
                    out.push_str("### ");
                    out.push_str(label);
                    out.push('\n');
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    // href="..." or href='...'
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let idx = lower.find(&key)?;
    let rest = &tag[idx + key.len()..];
    let mut chars = rest.chars();
    let quote = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut val = String::new();
    for c in chars {
        if c == quote {
            break;
        }
        val.push(c);
    }
    if val.is_empty() {
        None
    } else {
        Some(decode_basic_entities(&val))
    }
}

fn decode_basic_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in s.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            blank += 1;
            if blank <= 2 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(t);
            out.push('\n');
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

fn feed_rel(url: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in url.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("pages/{h:016x}.feed")
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

#[cfg(test)]
mod tests {
    use super::*;

    const RSS2: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
<title>Demo Feed</title>
<item>
  <title>First Post</title>
  <link>https://example.com/1</link>
  <description><![CDATA[<p>Hello <b>world</b></p>]]></description>
</item>
<item>
  <title></title>
  <link>https://example.com/2</link>
  <description>Second only</description>
</item>
</channel></rss>"#;

    const ATOM: &str = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Demo</title>
  <entry>
    <title>Atom One</title>
    <link href="https://example.com/a1"/>
    <summary>Atom summary text</summary>
  </entry>
</feed>"#;

    #[test]
    fn parse_rss2_titles_and_description() {
        let (title, items) = parse_feed_items(RSS2).expect("rss");
        assert_eq!(title, "Demo Feed");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First Post");
        assert!(items[0].body_md.contains("Hello"));
        assert!(items[0].body_md.contains("world"));
        assert!(
            items[0].body_md.contains("[") || items[0].body_md.contains("**"),
            "expected md emphasis or plain: {}",
            items[0].body_md
        );
        assert!(items[0].link.as_deref() == Some("https://example.com/1"));
        assert!(items[1].title == "Untitled" || items[1].title.contains("example.com"));
        assert!(items[1].body_md.contains("Second only"));
    }

    #[test]
    fn html_fragment_keeps_links_lists_headings() {
        let html = r#"
        <h2>New packages</h2>
        <ul><li><a href="https://ex/a">Alpha</a> note</li>
        <li>Emacs Teaches Emacs: <a href="https://ex/b">The Missing README</a> (YouTube)</li>
        </ul>
        <p>More <b>bold</b> text</p>
        "#;
        let md = html_fragment_to_md(html);
        assert!(
            md.contains("### New packages") || md.contains("## New packages"),
            "{md}"
        );
        assert!(md.contains("[Alpha](https://ex/a)"), "{md}");
        assert!(md.contains("[The Missing README](https://ex/b)"), "{md}");
        assert!(md.contains('\n'), "expected newlines: {md:?}");
        assert!(md.contains("- "), "{md}");
        assert!(md.contains("**bold**"), "{md}");
    }

    #[test]
    fn li_section_label_becomes_heading() {
        let html = r#"<ul><li>New packages: <ul><li><a href="https://ex/a">Alpha</a></li></ul></li>
        <li>Emacs development: <ul><li>note</li></ul></li></ul>"#;
        let md = html_fragment_to_md(html);
        assert!(md.contains("### New packages"), "{md}");
        assert!(md.contains("### Emacs development"), "{md}");
        assert!(md.contains("[Alpha](https://ex/a)"), "{md}");
    }

    #[test]
    fn parse_atom_summary() {
        let (title, items) = parse_feed_items(ATOM).expect("atom");
        assert_eq!(title, "Atom Demo");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Atom One");
        assert!(items[0].body_md.contains("Atom summary text"));
    }

    #[test]
    fn parse_not_feed_errs() {
        assert!(parse_feed_items("<html><body>hi</body></html>").is_err());
        assert!(parse_feed_items("not xml at all").is_err());
    }

    #[test]
    fn filter_drops_old_title_dates() {
        let now = now_unix();
        let old = FeedItem {
            title: "2025-01-01 Emacs news".into(),
            body_md: "# x".into(),
            link: Some("https://ex/2025/01/2025-01-01-emacs-news/".into()),
            published: None,
        };
        let recent_ts = now - 10 * 86400;
        let recent = FeedItem {
            title: "Recent".into(),
            body_md: "# r".into(),
            link: None,
            published: Some(recent_ts),
        };
        let out = filter_feed_age(vec![old, recent]);
        assert_eq!(out.len(), 1, "old title-dated item must drop");
        assert_eq!(out[0].title, "Recent");
    }

    #[test]
    fn entry_title_truncates() {
        let long = "x".repeat(200);
        let xml = format!(
            r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title>
<item><title>{long}</title><description>d</description></item>
</channel></rss>"#
        );
        let (_, items) = parse_feed_items(&xml).unwrap();
        assert!(items[0].title.chars().count() <= ENTRY_TITLE_MAX);
    }

    #[test]
    fn state_from_body_feed_multi() {
        let st = state_from_body("https://ex/feed", RSS2, Some("application/rss+xml"));
        assert!(st.items.len() >= 2);
        assert_eq!(st.title, "Demo Feed");
    }

    #[test]
    fn archive_html_multibyte_no_panic() {
        // non-ASCII before/inside page must not panic on byte scan
        let html = r#"<!DOCTYPE html><html><body>
<p>žžž café 中文</p>
<h2><a href="https://example.com/blog/post-1">Hello ž</a></h2>
<time datetime="2026-05-01T12:00:00+00:00">May 1</time>
<h2><a href="/blog/post-2">Second</a></h2>
<time datetime="2026-04-01">Apr 1</time>
</body></html>"#;
        let items = archive_items_from_html(html);
        assert!(items.len() >= 2, "got {}", items.len());
        assert!(items[0].title.contains("Hello"));
        assert!(items[0].link.as_ref().unwrap().contains("post-1"));
    }

    #[test]
    fn state_from_body_html_single() {
        let html = r#"<html><head><title>Hi</title></head><body><p>x</p></body></html>"#;
        let st = state_from_body("https://example.com/", html, Some("text/html"));
        assert_eq!(st.items.len(), 1);
    }

    #[test]
    fn validate_fetch_url_blocks_localhost_and_private() {
        assert!(validate_fetch_url("http://localhost/").is_err());
        assert!(validate_fetch_url("http://127.0.0.1/").is_err());
        assert!(validate_fetch_url("http://192.168.1.1/").is_err());
        assert!(validate_fetch_url("http://10.0.0.1/").is_err());
        assert!(validate_fetch_url("http://172.16.0.1/").is_err());
        assert!(validate_fetch_url("http://169.254.1.1/").is_err());
        assert!(validate_fetch_url("http://0.0.0.0/").is_err());
        assert!(validate_fetch_url("http://[::1]/").is_err());
        assert!(validate_fetch_url("http://[fd00::1]/").is_err());
        assert!(validate_fetch_url("http://[fe80::1]/").is_err());
        assert!(validate_fetch_url("http://[::ffff:127.0.0.1]/").is_err());
    }

    #[test]
    fn validate_fetch_url_blocks_local_domains() {
        assert!(validate_fetch_url("http://myhost.local/").is_err());
        assert!(validate_fetch_url("http://foo.bar.local/").is_err());
    }

    #[test]
    fn validate_fetch_url_blocks_non_http_schemes() {
        assert!(validate_fetch_url("file:///etc/passwd").is_err());
        assert!(validate_fetch_url("gopher://example.com/").is_err());
        assert!(validate_fetch_url("ftp://example.com/").is_err());
    }

    #[test]
    fn validate_fetch_url_allows_public() {
        assert!(validate_fetch_url("http://example.com/").is_ok());
        assert!(validate_fetch_url("https://example.com/path?q=1").is_ok());
        assert!(validate_fetch_url("http://8.8.8.8/").is_ok());
    }
}
