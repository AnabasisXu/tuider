//! Code plugin cdylib — install as `libtuider_code.so`.
//!
//! Bodies are UTF-8 for the host. Highlighted sources use `TUIDER_HTML_V1`
//! (CSS + HTML) so host `html_css` / `html_render` colour tokens. Host does not
//! parse inline styles or multi-class CSS selectors, so we emit one class per
//! unique RGB (`.cRRGGBB`) and `<br>` for newlines.

use std::collections::BTreeMap;
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use tuider_plugin_api::{
    args_vec, cstring_or_null, free_cstring, write_err, TUIDER_PLUGIN_ABI,
};

const EXTS: &[&str] = &[
    "rs", "py", "go", "js", "ts", "tsx", "jsx", "c", "h", "cpp", "hpp", "java", "kt", "swift",
    "rb", "php", "cs", "sh", "bash", "zsh", "fish", "toml", "yaml", "yml", "json", "html", "css",
    "sql", "lua", "vim", "zig",
];

const HTML_V1: &str = "TUIDER_HTML_V1\n";
const THEME_NAME: &str = "base16-ocean.dark";

struct CodeState {
    title: String,
    names: Vec<String>,
    paths: Vec<PathBuf>,
}

fn is_code_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn scan_code(root: &Path, recursive: bool) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if root.is_file() {
        if is_code_path(root) {
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string());
            out.push((name, root.to_path_buf()));
        }
        return out;
    }
    walk(root, root, recursive, &mut out);
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

fn walk(root: &Path, dir: &Path, recursive: bool, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            if recursive {
                walk(root, &p, true, out);
            }
        } else if is_code_path(&p) {
            let name = p
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            out.push((name, p));
        }
    }
}

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

fn syntaxes() -> &'static SyntaxSet {
    &SYNTAXES
}

fn theme() -> &'static Theme {
    THEMES
        .themes
        .get(THEME_NAME)
        .or_else(|| THEMES.themes.values().next())
        .expect("syntect ships at least one theme")
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Highlight `text` for `path` into host `TUIDER_HTML_V1` payload.
///
/// On parse/highlight failure returns plain text (no prefix).
fn highlight_body(path: &Path, text: &str) -> String {
    let ss = syntaxes();
    let theme = theme();
    let syntax = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| ss.find_syntax_by_extension(n))
        })
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);
    // ponytail: class-per-RGB — host CSS only indexes single .class keys
    let mut colors: BTreeMap<(u8, u8, u8), ()> = BTreeMap::new();
    let mut html = String::with_capacity(text.len().saturating_mul(2));

    for line in LinesWithEndings::from(text) {
        let regions = match highlighter.highlight_line(line, ss) {
            Ok(r) => r,
            Err(_) => return text.to_string(),
        };
        for (style, chunk) in regions {
            let chunk = chunk.trim_end_matches(['\n', '\r']);
            if chunk.is_empty() {
                continue;
            }
            let fg = style.foreground;
            colors.insert((fg.r, fg.g, fg.b), ());
            let class = format!("c{:02x}{:02x}{:02x}", fg.r, fg.g, fg.b);
            html.push_str("<span class=\"");
            html.push_str(&class);
            html.push_str("\">");
            html.push_str(&escape_html(chunk));
            html.push_str("</span>");
        }
        html.push_str("<br>");
    }

    let mut css = String::with_capacity(colors.len() * 32);
    for (r, g, b) in colors.keys() {
        css.push_str(&format!(
            ".c{r:02x}{g:02x}{b:02x}{{color:#{r:02x}{g:02x}{b:02x}}}\n"
        ));
    }
    format!("{HTML_V1}{css}\n\u{1e}\n{html}")
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_abi_version() -> u32 {
    TUIDER_PLUGIN_ABI
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_id() -> *const c_char {
    c"code".as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_name() -> *const c_char {
    c"Code files".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_handles(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = unsafe { args_vec(argc, argv) };
    if args.iter().any(|a| a == "--code") {
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
    let mut recursive = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--code" => {}
            "-r" | "--recursive" => recursive = true,
            "-l" | "--list" | "--print" | "-h" | "--help" | "-V" | "--version" => {}
            s if s.starts_with('-') => {}
            s => paths.push(PathBuf::from(s)),
        }
        i += 1;
    }
    if paths.is_empty() {
        write_err(err, err_len, "code: need path(s) after --code");
        return std::ptr::null_mut();
    }
    let mut docs = Vec::new();
    for p in &paths {
        if !p.exists() {
            write_err(
                err,
                err_len,
                &format!("code: not found: {}", p.display()),
            );
            return std::ptr::null_mut();
        }
        docs.extend(scan_code(p, recursive));
    }
    docs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    docs.dedup_by(|a, b| a.1 == b.1);
    if docs.is_empty() {
        write_err(err, err_len, "code: no known source files under path(s)");
        return std::ptr::null_mut();
    }
    let (names, paths): (Vec<_>, Vec<_>) = docs.into_iter().unzip();
    let boxed = Box::new(CodeState {
        title: "code".into(),
        names,
        paths,
    });
    Box::into_raw(boxed) as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_close(src: *mut c_void) {
    if !src.is_null() {
        drop(unsafe { Box::from_raw(src as *mut CodeState) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_title(src: *mut c_void) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut CodeState) };
    cstring_or_null(&s.title)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_count(src: *mut c_void) -> usize {
    if src.is_null() {
        0
    } else {
        unsafe { &*(src as *mut CodeState) }.names.len()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_at(src: *mut c_void, index: usize) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut CodeState) };
    s.names
        .get(index)
        .map(|n| cstring_or_null(n))
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
    let s = unsafe { &*(src as *mut CodeState) };
    let Some(path) = s.paths.get(index) else {
        return cstring_or_null("out of range");
    };
    match std::fs::read_to_string(path) {
        Ok(text) => cstring_or_null(&highlight_body(path, &text)),
        Err(e) => cstring_or_null(&format!("error: {e}")),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn rust_highlight_emits_html_v1_and_keyword_color() {
        let src = "fn main() {\n    let x = 1;\n}\n";
        let out = highlight_body(Path::new("main.rs"), src);
        assert!(out.starts_with(HTML_V1), "prefix missing: {}", &out[..out.len().min(40)]);
        assert!(out.contains("\n\u{1e}\n"), "css/html separator missing");
        assert!(out.contains("<span class=\"c"), "token spans missing");
        assert!(out.contains("<br>"), "line breaks missing");
        assert!(out.contains("fn"), "source text lost");
        assert!(!out.contains("fn main") || out.contains("&") || out.contains("<span"), "should be tokenized");
        // raw source must not appear unescaped as a tag
        assert!(!out.contains("<fn"));
    }

    #[test]
    fn escape_html_entities() {
        assert_eq!(escape_html("a<b>&\"c"), "a&lt;b&gt;&amp;&quot;c");
    }

    #[test]
    fn plain_unknown_ext_still_highlights_as_plain() {
        let out = highlight_body(Path::new("notes.unknownlang"), "hello\nworld\n");
        assert!(out.starts_with(HTML_V1));
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
    }
}
