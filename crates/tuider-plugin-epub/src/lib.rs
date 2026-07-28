//! EPUB plugin cdylib — install as `libtuider_epub.so`.
//!
//! Sidebar = linear spine chapters. Body = XHTML → rough markdown for host md render.
//! Images → `![alt](src)` placeholders only (no terminal graphics). DRM/CSS ignored.

use std::collections::HashMap;
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;

use rbook::epub::reader::LinearBehavior;
use rbook::Epub;
use tuider_plugin_api::{
    args_vec, cstring_or_null, free_cstring, write_err, TUIDER_PLUGIN_ABI,
};

#[derive(Debug)]
struct Chapter {
    title: String,
    body_md: String,
}

#[derive(Debug)]
struct EpubState {
    title: String,
    chapters: Vec<Chapter>,
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_abi_version() -> u32 {
    TUIDER_PLUGIN_ABI
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_id() -> *const c_char {
    c"epub".as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_name() -> *const c_char {
    c"EPUB reader".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_handles(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = unsafe { args_vec(argc, argv) };
    if claims_args(&args) {
        1
    } else {
        0
    }
}

fn claims_args(args: &[String]) -> bool {
    args.iter().any(|a| {
        a == "-e"
            || a == "--epub"
            || a.ends_with(".epub")
            || a.ends_with(".EPUB")
    })
}

fn pick_path(args: &[String]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-e" || args[i] == "--epub" {
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                return Some(args[i + 1].clone());
            }
        } else if args[i].ends_with(".epub") || args[i].ends_with(".EPUB") {
            return Some(args[i].clone());
        }
        i += 1;
    }
    None
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_open(
    argc: c_int,
    argv: *const *const c_char,
    err: *mut c_char,
    err_len: usize,
) -> *mut c_void {
    let args = unsafe { args_vec(argc, argv) };
    let Some(path) = pick_path(&args) else {
        write_err(err, err_len, "epub plugin: need path.epub or -e <file>");
        return std::ptr::null_mut();
    };
    if !Path::new(&path).is_file() {
        write_err(err, err_len, &format!("epub plugin: not a file: {path}"));
        return std::ptr::null_mut();
    }
    match open_epub(&path) {
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
        drop(unsafe { Box::from_raw(src as *mut EpubState) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_title(src: *mut c_void) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut EpubState) };
    cstring_or_null(&s.title)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_count(src: *mut c_void) -> usize {
    if src.is_null() {
        0
    } else {
        unsafe { &*(src as *mut EpubState) }.chapters.len()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_at(src: *mut c_void, index: usize) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut EpubState) };
    s.chapters
        .get(index)
        .map(|c| cstring_or_null(&c.title))
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
    let s = unsafe { &*(src as *mut EpubState) };
    s.chapters
        .get(index)
        .map(|c| cstring_or_null(&c.body_md))
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}

// ── open + convert ────────────────────────────────────────────────────────

fn open_epub(path: &str) -> Result<EpubState, String> {
    let epub = Epub::open(path).map_err(|e| format!("open epub: {e}"))?;
    let book_title = epub
        .metadata()
        .title()
        .map(|t| t.value().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(path)
                .to_string()
        });

    // idref / href path → TOC label
    let mut toc_by_id: HashMap<String, String> = HashMap::new();
    let mut toc_by_href: HashMap<String, String> = HashMap::new();
    if let Some(root) = epub.toc().contents() {
        for entry in root.flatten() {
            let label = entry.label().trim();
            if label.is_empty() {
                continue;
            }
            if let Some(me) = entry.manifest_entry() {
                toc_by_id
                    .entry(me.id().to_string())
                    .or_insert_with(|| label.to_string());
                let href = me.href().path().decode().into_owned();
                toc_by_href.entry(href).or_insert_with(|| label.to_string());
            }
        }
    }

    // Prefer linear spine via reader (skip pure cover/nav noise when marked non-linear)
    let mut reader = epub
        .reader_builder()
        .linear_behavior(LinearBehavior::LinearOnly)
        .create();

    let mut chapters = Vec::new();
    let mut idx = 0usize;
    while let Some(item) = reader.next() {
        let data = item.map_err(|e| format!("read chapter: {e}"))?;
        let spine = data.spine_entry();
        let idref = spine.idref().to_string();
        let me = spine.manifest_entry();
        let href = me
            .as_ref()
            .map(|m| m.href().path().decode().into_owned())
            .unwrap_or_default();
        let html = data.content();
        // skip empty / pure-nav stubs
        let body_md = xhtml_to_markdown(html);
        if body_md.trim().is_empty() {
            idx += 1;
            continue;
        }
        let title = toc_by_id
            .get(&idref)
            .cloned()
            .or_else(|| toc_by_href.get(&href).cloned())
            .or_else(|| first_md_heading(&body_md))
            .unwrap_or_else(|| format!("Chapter {}", idx + 1));
        chapters.push(Chapter {
            title,
            body_md: format!("# {book_title}\n\n{body_md}"),
        });
        idx += 1;
    }

    // Fallback: if LinearOnly empty, walk full spine once
    if chapters.is_empty() {
        for (i, entry) in epub.spine().iter().enumerate() {
            let idref = entry.idref().to_string();
            let Some(me) = entry.manifest_entry() else {
                continue;
            };
            let href = me.href().path().decode().into_owned();
            let Ok(html) = me.read_str() else {
                continue;
            };
            let body_md = xhtml_to_markdown(&html);
            if body_md.trim().is_empty() {
                continue;
            }
            let title = toc_by_id
                .get(&idref)
                .cloned()
                .or_else(|| toc_by_href.get(&href).cloned())
                .or_else(|| first_md_heading(&body_md))
                .unwrap_or_else(|| format!("Chapter {}", i + 1));
            chapters.push(Chapter {
                title,
                body_md: format!("# {book_title}\n\n{body_md}"),
            });
        }
    }

    if chapters.is_empty() {
        return Err("epub has no readable chapters".into());
    }

    Ok(EpubState {
        title: book_title,
        chapters,
    })
}

fn first_md_heading(md: &str) -> Option<String> {
    md.lines().find_map(|l| {
        let t = l.trim();
        t.strip_prefix('#')
            .map(|s| s.trim_start_matches('#').trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
}

/// XHTML/HTML → rough markdown (headings, paragraphs, lists, img placeholders).
fn xhtml_to_markdown(html: &str) -> String {
    let s = drop_blocks(html, &["script", "style", "noscript", "svg", "head"]);
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    let mut heading: Option<u8> = None;
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
                    if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                        if heading.take().is_some() {
                            while out.ends_with(' ') {
                                out.pop();
                            }
                            out.push('\n');
                        }
                    } else if matches!(
                        name.as_str(),
                        "p" | "div" | "li" | "tr" | "blockquote" | "section" | "article"
                    ) {
                        out.push('\n');
                    }
                } else {
                    match name.as_str() {
                        "h1" => {
                            out.push_str("\n# ");
                            heading = Some(1);
                        }
                        "h2" => {
                            out.push_str("\n## ");
                            heading = Some(2);
                        }
                        "h3" => {
                            out.push_str("\n### ");
                            heading = Some(3);
                        }
                        "h4" | "h5" | "h6" => {
                            out.push_str("\n#### ");
                            heading = Some(4);
                        }
                        "br" => out.push('\n'),
                        "li" => {
                            if !out.ends_with('\n') {
                                out.push('\n');
                            }
                            out.push_str("- ");
                        }
                        "p" | "div" | "tr" | "blockquote" | "section" | "article" => {
                            if !out.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                        "img" => {
                            // A: placeholder only — host md shows 🖼 url
                            if !out.ends_with('\n') {
                                out.push('\n');
                            }
                            out.push_str(&img_to_md(raw));
                            out.push('\n');
                        }
                        _ => {}
                    }
                }
            }
            _ if in_tag => tag.push(c),
            '&' if !in_tag => {
                out.push('&');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = decode_basic_entities(&out);
    collapse_blank_lines(&out)
}

/// `<img …>` → `![alt](src)`; missing alt → file stem / "image"; data: → label only.
fn img_to_md(raw_tag: &str) -> String {
    let alt = html_attr(raw_tag, "alt")
        .or_else(|| html_attr(raw_tag, "title"))
        .unwrap_or_default();
    let src = html_attr(raw_tag, "src").unwrap_or_default();
    let alt = decode_basic_entities(alt.trim());
    let src = decode_basic_entities(src.trim());

    if src.is_empty() || src.starts_with("data:") {
        let label = if alt.is_empty() { "image".into() } else { alt };
        return format!("![{label}]()");
    }

    let label = if alt.is_empty() {
        src.rsplit(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("image")
            .to_string()
    } else {
        alt
    };
    // escape ] ( in rare alts
    let label = label.replace(']', "");
    let src = src.replace(')', "%29");
    format!("![{label}]({src})")
}

fn html_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    // ponytail: case-insensitive name, "..." or '...' or bare
    let lower = tag.to_ascii_lowercase();
    let key = name.to_ascii_lowercase();
    let mut search = 0;
    while let Some(rel) = lower[search..].find(&key) {
        let i = search + rel;
        let after = i + key.len();
        let bytes = tag.as_bytes();
        // word boundary before name
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            search = after;
            continue;
        }
        let rest = tag[after..].trim_start();
        if !rest.starts_with('=') {
            search = after;
            continue;
        }
        let rest = rest[1..].trim_start();
        if rest.is_empty() {
            return None;
        }
        let b = rest.as_bytes()[0];
        if b == b'"' || b == b'\'' {
            let q = b as char;
            let body = &rest[1..];
            return body.find(q).map(|e| &body[..e]);
        }
        // bare value
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(rest.len());
        return Some(&rest[..end]);
    }
    None
}

fn drop_blocks(html: &str, tags: &[&str]) -> String {
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let rest = &lower[i..];
            let mut skipped = false;
            for tag in tags {
                let open = format!("<{tag}");
                if rest.starts_with(&open)
                    && rest
                        .as_bytes()
                        .get(open.len())
                        .is_some_and(|c| !c.is_ascii_alphanumeric())
                {
                    let close = format!("</{tag}>");
                    if let Some(rel) = rest.find(&close) {
                        i += rel + close.len();
                        skipped = true;
                        break;
                    }
                }
            }
            if skipped {
                continue;
            }
        }
        out.push(html[i..].chars().next().unwrap_or('\0'));
        // advance by char
        let ch = html[i..].chars().next().unwrap();
        i += ch.len_utf8();
    }
    out
}

fn decode_basic_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some((ch, n)) = match_entity(&s[i..]) {
                out.push(ch);
                i += n;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Returns (char, byte_len of entity including &…;)
fn match_entity(s: &str) -> Option<(char, usize)> {
    if let Some(rest) = s.strip_prefix("&#x").or_else(|| s.strip_prefix("&#X")) {
        let hex: String = rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if rest.as_bytes().get(hex.len()) == Some(&b';') {
            if let Ok(v) = u32::from_str_radix(&hex, 16) {
                if let Some(c) = char_from_entity_code(v) {
                    return Some((c, 3 + hex.len() + 1));
                }
            }
        }
    }
    if let Some(rest) = s.strip_prefix("&#") {
        let dig: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !dig.is_empty() && rest.as_bytes().get(dig.len()) == Some(&b';') {
            if let Ok(v) = dig.parse::<u32>() {
                if let Some(c) = char_from_entity_code(v) {
                    return Some((c, 2 + dig.len() + 1));
                }
            }
        }
    }
    const NAMED: &[(&str, char)] = &[
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
        ("&nbsp;", ' '),
    ];
    for &(pat, ch) in NAMED {
        if s.starts_with(pat) {
            return Some((ch, pat.len()));
        }
    }
    None
}

fn char_from_entity_code(v: u32) -> Option<char> {
    // terminal: treat nbsp as plain space
    if v == 160 {
        return Some(' ');
    }
    char::from_u32(v)
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0usize;
    for line in s.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            blank += 1;
            if blank <= 1 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(t.trim_start());
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn claims_epub_path_and_flag() {
        assert!(claims_args(&["book.epub".into()]));
        assert!(claims_args(&["-e".into(), "x".into()]));
        assert!(claims_args(&["--epub".into(), "x".into()]));
        assert!(!claims_args(&["README.md".into()]));
        assert!(!claims_args(&["book.epub.bak".into()]));
    }

    #[test]
    fn img_to_md_placeholder() {
        assert_eq!(
            img_to_md(r#"img src="OEBPS/fig.png" alt="Diagram""#),
            "![Diagram](OEBPS/fig.png)"
        );
        assert_eq!(
            img_to_md(r#"img src='a/b/c.jpg'"#),
            "![c.jpg](a/b/c.jpg)"
        );
        assert_eq!(
            img_to_md(r#"img src="data:image/png;base64,xx" alt="x""#),
            "![x]()"
        );
        assert_eq!(img_to_md("img"), "![image]()");
    }

    #[test]
    fn xhtml_img_emits_md_image() {
        let md = xhtml_to_markdown(
            r#"<p>see</p><img src="images/fig1.png" alt="Fig 1"/><p>next</p>"#,
        );
        assert!(md.contains("![Fig 1](images/fig1.png)"), "{md}");
        assert!(md.contains("see"), "{md}");
        assert!(md.contains("next"), "{md}");
    }

    #[test]
    fn numeric_entities() {
        let md = xhtml_to_markdown("<p>a&#160;b&#x2d;c</p>");
        assert!(md.contains("a b-c") || md.contains("a b‐c") || md.contains("a b"), "{md}");
        assert!(md.contains('b'), "{md}");
        assert!(!md.contains("&#"), "{md}");
    }

    #[test]
    fn xhtml_heading_list_entities_strip_script() {
        let md = xhtml_to_markdown(
            "<html><head><style>x{}</style></head><body>\
             <h1>Hi</h1><p>Hello &amp; world&nbsp;!</p>\
             <ul><li>A</li><li>B</li></ul>\
             <script>evil()</script></body></html>",
        );
        assert!(md.contains("# Hi"), "{md}");
        assert!(md.contains("Hello & world"), "{md}");
        assert!(md.contains("- A"), "{md}");
        assert!(md.contains("- B"), "{md}");
        assert!(!md.contains("evil"), "{md}");
        assert!(!md.contains("x{}"), "{md}");
    }

    #[test]
    fn pick_path_order() {
        assert_eq!(
            pick_path(&["-e".into(), "a.epub".into(), "b.epub".into()]),
            Some("a.epub".into())
        );
        assert_eq!(
            pick_path(&["notes.md".into(), "book.EPUB".into()]),
            Some("book.EPUB".into())
        );
        assert_eq!(pick_path(&["-e".into()]), None);
    }

    #[test]
    fn open_epub2_toc_and_bodies() {
        let st = open_epub(fixture("sample2.epub").to_str().unwrap()).unwrap();
        assert_eq!(st.title, "Smoke Book");
        assert_eq!(st.chapters.len(), 2);
        assert_eq!(st.chapters[0].title, "One");
        assert_eq!(st.chapters[1].title, "Two");
        assert!(
            st.chapters[0].body_md.contains("Hello from chapter one"),
            "{}",
            st.chapters[0].body_md
        );
        assert!(
            st.chapters[0].body_md.contains("- A"),
            "{}",
            st.chapters[0].body_md
        );
        assert!(
            st.chapters[1].body_md.contains("Second chapter body"),
            "{}",
            st.chapters[1].body_md
        );
        // book title prefix once
        assert!(st.chapters[0].body_md.starts_with("# Smoke Book\n"));
    }

    #[test]
    fn open_epub3_zh_linear_skips_nav() {
        let st = open_epub(fixture("sample3.epub").to_str().unwrap()).unwrap();
        assert_eq!(st.title, "中文书");
        assert_eq!(st.chapters.len(), 2, "nav linear=no should be skipped");
        // TOC labels preferred
        assert!(
            st.chapters[0].title.contains("一") || st.chapters[0].title == "第一章",
            "got {}",
            st.chapters[0].title
        );
        assert!(
            st.chapters[0].body_md.contains("你好"),
            "{}",
            st.chapters[0].body_md
        );
        assert!(
            !st.chapters[0].body_md.contains("evil"),
            "{}",
            st.chapters[0].body_md
        );
        assert!(
            st.chapters[1].body_md.contains("内容") && st.chapters[1].body_md.contains("2"),
            "{}",
            st.chapters[1].body_md
        );
        assert!(
            st.chapters[1].body_md.contains("&") || st.chapters[1].body_md.contains("more"),
            "{}",
            st.chapters[1].body_md
        );
    }

    #[test]
    fn open_empty_spine_errors() {
        let err = open_epub(fixture("empty_spine.epub").to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("no readable") || err.contains("chapter"),
            "{err}"
        );
    }

    #[test]
    fn open_corrupt_and_missing() {
        let e1 = open_epub(fixture("not_epub.epub").to_str().unwrap()).unwrap_err();
        assert!(!e1.is_empty(), "{e1}");
        let e2 = open_epub("/no/such/file.epub").unwrap_err();
        assert!(!e2.is_empty(), "{e2}");
    }

    #[test]
    fn abi_open_list_body_close() {
        let path = fixture("sample2.epub");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();
        let argv = [path_c.as_ptr()];
        let mut err = vec![0u8; 256];
        let handle = unsafe {
            tuider_plugin_open(
                1,
                argv.as_ptr(),
                err.as_mut_ptr() as *mut c_char,
                err.len(),
            )
        };
        assert!(!handle.is_null(), "open failed");
        assert_eq!(tuider_plugin_abi_version(), TUIDER_PLUGIN_ABI);
        let id = unsafe { CStr::from_ptr(tuider_plugin_id()) }
            .to_str()
            .unwrap();
        assert_eq!(id, "epub");

        let title_p = unsafe { tuider_source_title(handle) };
        assert!(!title_p.is_null());
        let title = unsafe { CStr::from_ptr(title_p) }
            .to_string_lossy()
            .into_owned();
        unsafe { tuider_string_free(title_p) };
        assert_eq!(title, "Smoke Book");

        assert_eq!(unsafe { tuider_source_entry_count(handle) }, 2);
        let e0 = unsafe { tuider_source_entry_at(handle, 0) };
        let e0s = unsafe { CStr::from_ptr(e0) }.to_string_lossy().into_owned();
        unsafe { tuider_string_free(e0) };
        assert_eq!(e0s, "One");

        let body = unsafe { tuider_source_load_body(handle, 1, 80) };
        assert!(!body.is_null());
        let body_s = unsafe { CStr::from_ptr(body) }
            .to_string_lossy()
            .into_owned();
        unsafe { tuider_string_free(body) };
        assert!(body_s.contains("Second chapter body"), "{body_s}");

        assert!(unsafe { tuider_source_entry_at(handle, 99) }.is_null());
        assert!(unsafe { tuider_source_load_body(handle, 99, 80) }.is_null());

        unsafe { tuider_plugin_close(handle) };
    }

    #[test]
    fn abi_open_missing_file_writes_err() {
        let path = CString::new("/tmp/definitely-missing-tuider.epub").unwrap();
        let argv = [path.as_ptr()];
        let mut err = vec![0u8; 256];
        let handle = unsafe {
            tuider_plugin_open(
                1,
                argv.as_ptr(),
                err.as_mut_ptr() as *mut c_char,
                err.len(),
            )
        };
        assert!(handle.is_null());
        let msg = CStr::from_bytes_until_nul(&err)
            .unwrap()
            .to_string_lossy();
        assert!(msg.contains("not a file") || msg.contains("open"), "{msg}");
    }

    #[test]
    fn handles_via_c_api() {
        let a = CString::new("book.epub").unwrap();
        let argv = [a.as_ptr()];
        assert_eq!(unsafe { tuider_plugin_handles(1, argv.as_ptr()) }, 1);
        let b = CString::new("README.md").unwrap();
        let argv2 = [b.as_ptr()];
        assert_eq!(unsafe { tuider_plugin_handles(1, argv2.as_ptr()) }, 0);
    }
}
