//! Scan plugins directory and dlopen Tuider plugin .so files (ABI v1).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::{Library, Symbol};
use tuider_plugin_api::{
    PluginTextSource, BODY_HTML_V1_PREFIX, FnAbiVersion, FnClose, FnEntryAt, FnEntryCount,
    FnHandles, FnId, FnLoadBody, FnName, FnOpen, FnStringFree, FnTitle, TUIDER_PLUGIN_ABI,
};

/// Optional: `extern "C" fn tuider_source_cycle(src: *mut c_void) -> c_int` (1 = cycled).
type FnCycle = unsafe extern "C" fn(*mut c_void) -> c_int;
/// Optional: `extern "C" fn tuider_source_action(src, index, action) -> c_int` (1 = body dirty).
type FnAction = unsafe extern "C" fn(*mut c_void, usize, *const c_char) -> c_int;
type FnDictLookup = tuider_plugin_api::FnDictLookup;
type FnDictSearch = tuider_plugin_api::FnDictSearch;
type FnDictReverse = tuider_plugin_api::FnDictReverse;
type FnDictList = tuider_plugin_api::FnDictList;
type FnDictSelect = tuider_plugin_api::FnDictSelect;
type FnDictFulltextSearch = tuider_plugin_api::FnDictFulltextSearch;


pub struct LoadedPlugin {
    #[allow(dead_code)] // kept for skip/debug messages
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    _lib: Library,
    open: FnOpen,
    handles: Option<FnHandles>,
    close: FnClose,
    title: FnTitle,
    entry_count: FnEntryCount,
    entry_at: FnEntryAt,
    load_body: FnLoadBody,
    string_free: FnStringFree,
    cycle: Option<FnCycle>,
    action: Option<FnAction>,
    dict_lookup: Option<FnDictLookup>,
    dict_search: Option<FnDictSearch>,
    dict_reverse: Option<FnDictReverse>,
    dict_list: Option<FnDictList>,
    dict_select: Option<FnDictSelect>,
    dict_fulltext_search: Option<FnDictFulltextSearch>,
}


pub struct DynSource {
    plugin: Arc<LoadedPlugin>,
    handle: *mut c_void,
    title_cache: String,
    entries_cache: Vec<String>,
}

unsafe impl Send for DynSource {}

impl Drop for DynSource {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.plugin.close)(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

impl PluginTextSource for DynSource {
    fn title(&self) -> &str {
        &self.title_cache
    }
    fn entries(&self) -> &[String] {
        &self.entries_cache
    }
    fn load_text(&mut self, index: usize, width: usize) -> Result<String, String> {
        let raw = unsafe { (self.plugin.load_body)(self.handle, index, width) };
        if raw.is_null() {
            return Err("plugin returned null body".into());
        }
        let s = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.plugin.string_free)(raw) };
        Ok(s)
    }
}

impl DynSource {
    fn refresh_meta(&mut self) {
        let title_ptr = unsafe { (self.plugin.title)(self.handle) };
        self.title_cache = if title_ptr.is_null() {
            self.plugin.id.clone()
        } else {
            let s = unsafe { CStr::from_ptr(title_ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { (self.plugin.string_free)(title_ptr) };
            s
        };
        let n = unsafe { (self.plugin.entry_count)(self.handle) };
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let p = unsafe { (self.plugin.entry_at)(self.handle, i) };
            if p.is_null() {
                entries.push(format!("#{i}"));
            } else {
                let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
                unsafe { (self.plugin.string_free)(p) };
                entries.push(s);
            }
        }
        self.entries_cache = entries;
    }

    fn cycle_layer(&mut self) -> bool {
        let Some(cycle) = self.plugin.cycle else {
            return false;
        };
        let r = unsafe { cycle(self.handle) };
        if r == 0 {
            return false;
        }
        self.refresh_meta();
        true
    }

    fn action(&mut self, index: usize, action: &str) -> bool {
        let Some(act) = self.plugin.action else {
            return false;
        };
        let Ok(c) = CString::new(action.replace('\0', "")) else {
            return false;
        };
        unsafe { act(self.handle, index, c.as_ptr()) != 0 }
    }

    fn take_cstr(&self, p: *mut c_char) -> Option<String> {
        if p.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(p) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.plugin.string_free)(p) };
        Some(s)
    }

    fn dict_lookup(&mut self, word: &str) -> Vec<(String, String)> {
        let Some(f) = self.plugin.dict_lookup else {
            return Vec::new();
        };
        let Ok(c) = CString::new(word.replace('\0', "")) else {
            return Vec::new();
        };
        let raw = unsafe { f(self.handle, c.as_ptr()) };
        let Some(s) = self.take_cstr(raw) else {
            return Vec::new();
        };
        // format: "DICT_TITLE\ttext" lines, or plain single block
        if s.contains('\t') {
            s.lines()
                .filter_map(|line| {
                    let (a, b) = line.split_once('\t')?;
                    Some((a.to_string(), b.to_string()))
                })
                .collect()
        } else if s.is_empty() {
            Vec::new()
        } else {
            vec![(self.title_cache.clone(), s)]
        }
    }

    fn dict_search(&mut self, prefix: &str, limit: usize) -> Vec<String> {
        let Some(f) = self.plugin.dict_search else {
            return Vec::new();
        };
        let Ok(c) = CString::new(prefix.replace('\0', "")) else {
            return Vec::new();
        };
        let raw = unsafe { f(self.handle, c.as_ptr(), limit) };
        self.take_cstr(raw)
            .map(|s| s.lines().map(str::to_owned).filter(|x| !x.is_empty()).collect())
            .unwrap_or_default()
    }

    fn dict_reverse(&mut self, query: &str, limit: usize) -> Vec<String> {
        let Some(f) = self.plugin.dict_reverse else {
            return Vec::new();
        };
        let Ok(c) = CString::new(query.replace('\0', "")) else {
            return Vec::new();
        };
        let raw = unsafe { f(self.handle, c.as_ptr(), limit) };
        self.take_cstr(raw)
            .map(|s| s.lines().map(str::to_owned).filter(|x| !x.is_empty()).collect())
            .unwrap_or_default()
    }

    fn dict_fulltext_search(&mut self, query: &str, limit: usize) -> Vec<String> {
        let Some(f) = self.plugin.dict_fulltext_search else {
            return Vec::new();
        };
        let Ok(c) = CString::new(query.replace('\0', "")) else {
            return Vec::new();
        };
        let raw = unsafe { f(self.handle, c.as_ptr(), limit) };
        if raw.is_null() {
            return Vec::new();
        }
        let s = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
        unsafe { (self.plugin.string_free)(raw) };
        s.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect()
    }

    fn dict_list(&self) -> Vec<String> {
        let Some(f) = self.plugin.dict_list else {
            return Vec::new();
        };
        let raw = unsafe { f(self.handle) };
        if raw.is_null() {
            return Vec::new();
        }
        let s = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.plugin.string_free)(raw) };
        s.lines()
            .map(str::to_owned)
            .filter(|x| !x.is_empty())
            .collect()
    }

    fn dict_select(&mut self, index: usize) -> bool {
        let Some(f) = self.plugin.dict_select else {
            return false;
        };
        if unsafe { f(self.handle, index) } == 0 {
            return false;
        }
        self.refresh_meta();
        true
    }

}

/// Host ContentSource: plugin text → ratatui Lines.
/// - `TUIDER_HTML_V1` + CSS + HTML → mdx-tui CSS renderer
/// - markdown-ish → mdterm `md`
/// - else plain text
pub struct HostSource {
    inner: DynSource,
    names: Vec<String>,
    title: String,
}

impl HostSource {
    pub fn from_dyn(d: DynSource) -> Self {
        let title = d.title_cache.clone();
        let names = d.entries_cache.clone();
        Self {
            inner: d,
            names,
            title,
        }
    }
}

impl crate::plugin::ContentSource for HostSource {
    fn title(&self) -> &str {
        &self.title
    }
    fn entries(&self) -> &[String] {
        &self.names
    }
    fn load(&mut self, index: usize, width: usize) -> crate::plugin::LoadResult {
        match self.inner.load_text(index, width) {
            Ok(text) => {
                let name = self
                    .names
                    .get(index)
                    .map(|s| s.as_str())
                    .unwrap_or("?");
                let (body, hint) = split_status_trailer(&text);
                let w = width.max(20);
                let (lines, links, headings) = render_plugin_body_doc(body, w);
                let n = lines.len();
                let status = match hint {
                    Some(h) => format!("{name}  ({n} lines)  ·  {h}"),
                    None => format!("{name}  ({n} lines)"),
                };
                crate::plugin::LoadResult {
                    lines,
                    status,
                    links,
                    headings,
                }
            }
            Err(e) => crate::plugin::LoadResult::plain(
                vec![ratatui::text::Line::from(format!("error: {e}"))],
                format!("load failed: {e}"),
            ),
        }
    }
    fn cycle_layer(&mut self) -> bool {
        if !self.inner.cycle_layer() {
            return false;
        }
        self.title = self.inner.title_cache.clone();
        self.names = self.inner.entries_cache.clone();
        true
    }
    fn action(&mut self, index: usize, action: &str) -> bool {
        self.inner.action(index, action)
    }
    fn has_action(&self) -> bool {
        self.inner.plugin.action.is_some()
    }
    fn lookup_word(&mut self, word: &str) -> Vec<(String, String)> {
        self.inner.dict_lookup(word)
    }
    fn search_headwords(&mut self, prefix: &str, limit: usize) -> Vec<String> {
        self.inner.dict_search(prefix, limit)
    }
    fn reverse_lookup(&mut self, query: &str, limit: usize) -> Vec<String> {
        self.inner.dict_reverse(query, limit)
    }
    fn fulltext_search(&mut self, query: &str, limit: usize) -> Vec<String> {
        self.inner.dict_fulltext_search(query, limit)
    }
    fn list_dicts(&self) -> Vec<String> {
        self.inner.dict_list()
    }
    fn select_dict(&mut self, index: usize) -> bool {
        if !self.inner.dict_select(index) {
            return false;
        }
        self.title = self.inner.title_cache.clone();
        self.names = self.inner.entries_cache.clone();
        true
    }
    fn plain_body(&mut self, index: usize) -> String {
        match self.inner.load_text(index, 100) {
            Ok(text) => {
                let (body, _) = split_status_trailer(&text);
                strip_body_to_plain(body)
            }
            Err(e) => e,
        }
    }
}


#[cfg_attr(not(test), allow(dead_code))]
fn render_plugin_body(text: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    render_plugin_body_doc(text, width).0
}

fn strip_body_to_plain(text: &str) -> String {
    if let Some(rest) = text.strip_prefix(BODY_HTML_V1_PREFIX) {
        let html = rest.split_once("\n\u{1e}\n").map(|(_, h)| h).unwrap_or(rest);
        return html_to_rough_plain(html);
    }
    text.to_string()
}

fn html_to_rough_plain(html: &str) -> String {
    // ponytail: tag strip only; good enough for CLI/AI/clipboard
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
}


pub fn render_plugin_body_doc(
    text: &str,
    width: usize,
) -> (
    Vec<ratatui::text::Line<'static>>,
    Vec<crate::plugin::LinkEntry>,
    Vec<crate::plugin::HeadingEntry>,
) {
    if let Some(rest) = text.strip_prefix(BODY_HTML_V1_PREFIX) {
        let (css_src, html) = match rest.split_once("\n\u{1e}\n") {
            Some((c, h)) => (c, h),
            None => ("", rest),
        };
        let table = crate::html_css::StyleTable::parse(css_src);
        let (raw_lines, mut headings) = crate::html_render::html_to_doc(html, &table);
        // if HTML classes missed, recover `1) 2) a)` from rendered lines
        if headings.len() < 2 {
            let fb = outline_from_lines(&raw_lines);
            if fb.len() > headings.len() {
                headings = fb;
            }
        }
        // Pre-wrap so scroll/outline line indices match the viewport.
        // html_to_doc emits unwrapped logical lines; Paragraph.wrap would
        // inflate visual rows and break Home/End/o/PgDn.
        let (lines, map) = wrap_body_lines(raw_lines, width);
        for h in &mut headings {
            h.line = map.get(h.line).copied().unwrap_or(h.line);
        }
        if headings.len() > 120 {
            headings.truncate(120);
        }
        return (lines, Vec::new(), headings);
    }
    if looks_like_md(text) {
        let doc = crate::md::render_md_doc(text, width);
        let headings = if doc.headings.is_empty() {
            outline_from_lines(&doc.lines)
        } else {
            doc.headings
        };
        (doc.lines, doc.links, headings)
    } else {
        let lines = crate::md::render_txt_width(text, width);
        let headings = outline_from_lines(&lines);
        (lines, Vec::new(), headings)
    }
}

/// Wrap each logical line to `width`; `map[i]` = first visual row of logical line i.
fn wrap_body_lines(
    lines: Vec<ratatui::text::Line<'static>>,
    width: usize,
) -> (Vec<ratatui::text::Line<'static>>, Vec<usize>) {
    let w = width.max(1);
    let mut out = Vec::with_capacity(lines.len());
    let mut map = Vec::with_capacity(lines.len());
    for line in lines {
        map.push(out.len());
        let wrapped = crate::md::wrap_spans(line.spans, w);
        if wrapped.is_empty() {
            out.push(ratatui::text::Line::from(""));
        } else {
            out.extend(wrapped);
        }
    }
    (out, map)
}

/// Fallback outline when body has no md/HTML headings (code / plain / sparse dict).
pub fn outline_from_plain_lines(
    lines: &[ratatui::text::Line<'static>],
) -> Vec<crate::plugin::HeadingEntry> {
    outline_from_lines(lines)
}

fn outline_from_lines(lines: &[ratatui::text::Line<'static>]) -> Vec<crate::plugin::HeadingEntry> {
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t: String = line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
            .trim()
            .to_string();
        if t.is_empty() {
            continue;
        }
        let plain = t.trim_start_matches(['•', ' ', '\t']);
        // numbered sense labels: 1) 2) a) b) 1. ①
        if let Some(level) = numbered_sense_level(plain) {
            let text = if plain.chars().count() > 72 {
                let mut s: String = plain.chars().take(71).collect();
                s.push('…');
                s
            } else {
                plain.to_string()
            };
            out.push(crate::plugin::HeadingEntry {
                level,
                text,
                line: i,
            });
            if out.len() >= 400 {
                break;
            }
            continue;
        }
        let level = outline_level(plain);
        if level == 0 {
            continue;
        }
        let text = plain.trim_end_matches(['{', ' ', '\t']).to_string();
        if text.chars().count() < 2 {
            continue;
        }
        out.push(crate::plugin::HeadingEntry {
            level,
            text,
            line: i,
        });
        if out.len() >= 400 {
            break;
        }
    }
    out
}

fn numbered_sense_level(s: &str) -> Option<u8> {
    let mut it = s.chars().peekable();
    let Some(c0) = it.next() else {
        return None;
    };
    if c0.is_ascii_digit() {
        while it.peek().is_some_and(|c| c.is_ascii_digit()) {
            it.next();
        }
        return match it.next() {
            Some(')' | '.' | '．' | '。' | '、' | '）') => Some(1),
            _ => None,
        };
    }
    if c0.is_ascii_lowercase() {
        return match it.next() {
            Some(')' | '.' | '．' | '）') => Some(2),
            _ => None,
        };
    }
    if matches!(c0, '①' | '②' | '③' | '④' | '⑤' | '⑥' | '⑦' | '⑧' | '⑨' | '⑩') {
        return Some(1);
    }
    None
}

fn outline_level(s: &str) -> u8 {
    // code / rust / py / js / go-ish defs
    let lower = s.to_ascii_lowercase();
    let code_kw = [
        "fn ",
        "pub fn ",
        "async fn ",
        "pub async fn ",
        "struct ",
        "pub struct ",
        "enum ",
        "pub enum ",
        "impl ",
        "trait ",
        "pub trait ",
        "mod ",
        "pub mod ",
        "class ",
        "def ",
        "async def ",
        "function ",
        "export function ",
        "export const ",
        "const ",
        "type ",
        "interface ",
        "func ",
        "package ",
    ];
    if code_kw.iter().any(|k| lower.starts_with(k)) {
        return 2;
    }
    // markdown-ish leftovers
    if s.starts_with("# ") {
        return 1;
    }
    if s.starts_with("## ") {
        return 2;
    }
    if s.starts_with("### ") {
        return 3;
    }
    // dict-ish: short headword lines (no sentence punctuation dump)
    if s.chars().count() <= 40
        && !s.contains('.')
        && !s.contains('。')
        && s.chars().any(|c| c.is_alphanumeric())
        && !s.starts_with('•')
    {
        let spaces = s.chars().filter(|c| c.is_whitespace()).count();
        if spaces <= 3 {
            return 2;
        }
    }
    0
}

/// Plugin may append `\n\u{1f}STATUS:hint` — host puts hint in status, not body.
fn split_status_trailer(text: &str) -> (&str, Option<&str>) {
    const MARK: &str = "\n\u{1f}STATUS:";
    if let Some(i) = text.rfind(MARK) {
        let body = &text[..i];
        let hint = text[i + MARK.len()..].trim();
        if hint.is_empty() {
            (body, None)
        } else {
            (body, Some(hint))
        }
    } else {
        (text, None)
    }
}

fn looks_like_md(t: &str) -> bool {
    t.contains('#') || t.contains("```") || t.contains("\n- ") || t.contains("**")
}

#[cfg(test)]
mod body_render_tests {
    use super::{render_plugin_body, split_status_trailer};
    use ratatui::style::Modifier;
    #[test]
    fn html_v1_applies_css_and_builtin_bold() {
        let body = "TUIDER_HTML_V1\n.src{color:#1E90FF;font-style:italic}\n\u{1e}\n<span class=\"src\">quoted</span> and <b>bold</b>";
        let lines = render_plugin_body(body, 80);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(text.contains("quoted"), "{text}");
        assert!(text.contains("bold"), "{text}");
        let has_bold = lines.iter().any(|l| {
            l.spans.iter().any(|s| {
                s.content.contains("bold") && s.style.add_modifier.contains(Modifier::BOLD)
            })
        });
        assert!(has_bold, "expected bold style on 'bold'");
        let quoted = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("quoted"));
        let q = quoted.expect("quoted span");
        assert!(q.style.add_modifier.contains(Modifier::ITALIC), "css italic");
        assert!(q.style.fg.is_some(), "css fg color");
    }

    #[test]
    fn status_trailer_stripped() {
        let (b, h) = split_status_trailer("hello\n\u{1f}STATUS:a fetch article");
        assert_eq!(b, "hello");
        assert_eq!(h, Some("a fetch article"));
        let (b2, h2) = split_status_trailer("plain");
        assert_eq!(b2, "plain");
        assert!(h2.is_none());
    }

    #[test]
    fn outline_from_code_lines() {
        use ratatui::text::{Line, Span};
        let lines = vec![
            Line::from(Span::raw("use std::io;")),
            Line::from(Span::raw("fn main() {")),
            Line::from(Span::raw("    println!(\"hi\");")),
            Line::from(Span::raw("}")),
            Line::from(Span::raw("pub struct App {")),
        ];
        let hs = super::outline_from_lines(&lines);
        assert!(hs.iter().any(|h| h.text.contains("fn main")), "{hs:?}");
        assert!(hs.iter().any(|h| h.text.contains("pub struct App")), "{hs:?}");
    }

    #[test]
    fn html_long_line_wraps_and_remaps_outline() {
        // one long sense line must split; outline line points into wrapped body
        let long = "a".repeat(120);
        let body = format!(
            "TUIDER_HTML_V1\n\n\u{1e}\n<div class=\"se2\"><span class=\"sensenum\">1)</span> {long}</div><div class=\"se2\"><span class=\"sensenum\">2)</span> short</div>"
        );
        let (lines, _, headings) = super::render_plugin_body_doc(&body, 40);
        assert!(lines.len() > 2, "expected wrap: {} lines", lines.len());
        let h1 = headings
            .iter()
            .find(|h| h.text.starts_with("1)"))
            .expect("1) outline");
        let h2 = headings
            .iter()
            .find(|h| h.text.starts_with("2)"))
            .expect("2) outline");
        assert!(h1.line < lines.len(), "h1.line {} >= {}", h1.line, lines.len());
        assert!(h2.line < lines.len(), "h2.line {} >= {}", h2.line, lines.len());
        assert!(h2.line > h1.line, "senses ordered: {h1:?} {h2:?}");
        // first wrapped row of sense 1 should still start with 1)
        let t0: String = lines[h1.line]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(t0.contains("1)"), "jump target: {t0}");
    }

    #[test]
    fn epub_plugin_dlopen_and_render() {
        use crate::plugin::ContentSource;
        use std::path::PathBuf;

        let so = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/debug/libtuider_epub.so");
        if !so.is_file() {
            eprintln!("skip: build libtuider_epub.so first ({})", so.display());
            return;
        }
        let dir = so.parent().unwrap();
        let reg = super::PluginRegistry::load(dir);
        let plug = reg
            .get("epub")
            .expect("epub plugin should load from target/debug");
        assert!(plug.handles_args(&["book.epub".into()]));
        assert!(!plug.handles_args(&["README.md".into()]));

        let book = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("crates/tuider-plugin-epub/tests/fixtures/sample2.epub");
        assert!(book.is_file(), "{}", book.display());
        let mut src = plug
            .open_from_args(&[book.to_string_lossy().into_owned()])
            .expect("open sample2");
        assert_eq!(src.title(), "Smoke Book");
        assert_eq!(src.entries(), &["One".to_string(), "Two".to_string()]);

        let r0 = src.load(0, 72);
        let plain0: String = r0
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            plain0.contains("Hello from chapter one"),
            "body0={plain0:?} status={}",
            r0.status
        );
        assert!(
            plain0.contains("Chapter One") || plain0.contains("One"),
            "{plain0}"
        );
        // md headings → outline
        assert!(
            !r0.headings.is_empty() || plain0.contains('#'),
            "expected structured md render headings={:?}",
            r0.headings
        );

        let r1 = src.load(1, 72);
        let plain1: String = r1
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(plain1.contains("Second chapter body"), "{plain1}");
    }
}

pub fn default_plugins_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TUIDER_PLUGINS_DIR") {
        return PathBuf::from(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/tuider/plugins");
    }
    PathBuf::from("./plugins")
}

pub fn scan_plugins(dir: &Path) -> Vec<Arc<LoadedPlugin>> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "so" || e == "dylib" || e == "dll")
        })
        .collect();
    paths.sort();
    for path in paths {
        match load_one(&path) {
            Ok(p) => out.push(Arc::new(p)),
            Err(e) => eprintln!("tuider: skip {}: {e}", path.display()),
        }
    }
    out
}

fn load_one(path: &Path) -> Result<LoadedPlugin, String> {
    let lib = unsafe { Library::new(path) }.map_err(|e| format!("dlopen: {e}"))?;
    unsafe {
        let abi: Symbol<FnAbiVersion> = lib
            .get(b"tuider_plugin_abi_version\0")
            .map_err(|e| format!("abi: {e}"))?;
        let ver = abi();
        if ver != TUIDER_PLUGIN_ABI {
            return Err(format!("ABI {ver} != host {TUIDER_PLUGIN_ABI}"));
        }
        let id_fn: Symbol<FnId> = lib.get(b"tuider_plugin_id\0").map_err(|e| e.to_string())?;
        let name_fn: Symbol<FnName> = lib.get(b"tuider_plugin_name\0").map_err(|e| e.to_string())?;
        let open: Symbol<FnOpen> = lib.get(b"tuider_plugin_open\0").map_err(|e| e.to_string())?;
        let close: Symbol<FnClose> = lib.get(b"tuider_plugin_close\0").map_err(|e| e.to_string())?;
        let title: Symbol<FnTitle> = lib.get(b"tuider_source_title\0").map_err(|e| e.to_string())?;
        let entry_count: Symbol<FnEntryCount> = lib
            .get(b"tuider_source_entry_count\0")
            .map_err(|e| e.to_string())?;
        let entry_at: Symbol<FnEntryAt> = lib
            .get(b"tuider_source_entry_at\0")
            .map_err(|e| e.to_string())?;
        let load_body: Symbol<FnLoadBody> = lib
            .get(b"tuider_source_load_body\0")
            .map_err(|e| e.to_string())?;
        let string_free: Symbol<FnStringFree> =
            lib.get(b"tuider_string_free\0").map_err(|e| e.to_string())?;
        let handles: Option<Symbol<FnHandles>> = lib.get(b"tuider_plugin_handles\0").ok();

        let id = CStr::from_ptr(id_fn()).to_string_lossy().into_owned();
        let name = CStr::from_ptr(name_fn()).to_string_lossy().into_owned();

        Ok(LoadedPlugin {
            path: path.to_path_buf(),
            id,
            name,
            open: *open,
            handles: handles.map(|s| *s),
            close: *close,
            title: *title,
            entry_count: *entry_count,
            entry_at: *entry_at,
            load_body: *load_body,
            string_free: *string_free,
            cycle: lib
                .get::<FnCycle>(b"tuider_source_cycle\0")
                .ok()
                .map(|s| *s),
            action: lib
                .get::<FnAction>(b"tuider_source_action\0")
                .ok()
                .map(|s| *s),
            dict_lookup: lib
                .get::<FnDictLookup>(b"tuider_dict_lookup\0")
                .ok()
                .map(|s| *s),
            dict_search: lib
                .get::<FnDictSearch>(b"tuider_dict_search\0")
                .ok()
                .map(|s| *s),
            dict_reverse: lib
                .get::<FnDictReverse>(b"tuider_dict_reverse\0")
                .ok()
                .map(|s| *s),
            dict_list: lib
                .get::<FnDictList>(b"tuider_dict_list\0")
                .ok()
                .map(|s| *s),
            dict_select: lib
                .get::<FnDictSelect>(b"tuider_dict_select\0")
                .ok()
                .map(|s| *s),
            dict_fulltext_search: lib
                .get::<FnDictFulltextSearch>(b"tuider_dict_fulltext_search\0")
                .ok()
                .map(|s| *s),
            _lib: lib,
        })
    }
}

impl LoadedPlugin {
    pub fn open_from_args(self: &Arc<Self>, args: &[String]) -> Result<HostSource, String> {
        let c_args: Vec<CString> = args
            .iter()
            .map(|s| CString::new(s.as_str().replace('\0', "")).unwrap_or_default())
            .collect();
        let ptrs: Vec<*const c_char> = c_args.iter().map(|c| c.as_ptr()).collect();
        let mut err = vec![0u8; 512];
        let handle = unsafe {
            (self.open)(
                ptrs.len() as c_int,
                ptrs.as_ptr(),
                err.as_mut_ptr() as *mut c_char,
                err.len(),
            )
        };
        if handle.is_null() {
            let msg = std::ffi::CStr::from_bytes_until_nul(&err)
                .map(|c| c.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "plugin open failed".into());
            return Err(msg);
        }
        let title_ptr = unsafe { (self.title)(handle) };
        let title = if title_ptr.is_null() {
            self.id.clone()
        } else {
            let s = unsafe { CStr::from_ptr(title_ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { (self.string_free)(title_ptr) };
            s
        };
        let n = unsafe { (self.entry_count)(handle) };
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let p = unsafe { (self.entry_at)(handle, i) };
            if p.is_null() {
                entries.push(format!("#{i}"));
            } else {
                let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
                unsafe { (self.string_free)(p) };
                entries.push(s);
            }
        }
        Ok(HostSource::from_dyn(DynSource {
            plugin: Arc::clone(self),
            handle,
            title_cache: title,
            entries_cache: entries,
        }))
    }

    pub fn handles_args(&self, args: &[String]) -> bool {
        let Some(h) = self.handles else {
            return false;
        };
        let c_args: Vec<CString> = args
            .iter()
            .map(|s| CString::new(s.as_str().replace('\0', "")).unwrap_or_default())
            .collect();
        let ptrs: Vec<*const c_char> = c_args.iter().map(|c| c.as_ptr()).collect();
        unsafe { h(ptrs.len() as c_int, ptrs.as_ptr()) != 0 }
    }
}

pub struct PluginRegistry {
    pub plugins: Vec<Arc<LoadedPlugin>>,
}

impl PluginRegistry {
    pub fn load(dir: &Path) -> Self {
        Self {
            plugins: scan_plugins(dir),
        }
    }

    #[allow(dead_code)]
    pub fn empty() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn has(&self, id: &str) -> bool {
        self.plugins.iter().any(|p| p.id == id)
    }

    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<&Arc<LoadedPlugin>> {
        self.plugins.iter().find(|p| p.id == id)
    }
}
