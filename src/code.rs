//! Syntect highlight for local source files → host `TUIDER_HTML_V1` body.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use tuider_plugin_api::BODY_HTML_V1_PREFIX;

const THEME_NAME: &str = "base16-ocean.dark";

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

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
/// On highlight failure returns plain text (no prefix).
pub fn highlight_body(path: &Path, text: &str) -> String {
    let ss = &*SYNTAXES;
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
    format!("{BODY_HTML_V1_PREFIX}{css}\n\u{1e}\n{html}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn rust_highlight_emits_html_v1() {
        let src = "fn main() {\n    let x = 1;\n}\n";
        let out = highlight_body(Path::new("main.rs"), src);
        assert!(out.starts_with(BODY_HTML_V1_PREFIX));
        assert!(out.contains("\n\u{1e}\n"));
        assert!(out.contains("<span class=\"c"));
        assert!(out.contains("<br>"));
        assert!(out.contains("fn"));
        assert!(!out.contains("<fn"));
    }

    #[test]
    fn escape_html_entities() {
        assert_eq!(escape_html("a<b>&\"c"), "a&lt;b&gt;&amp;&quot;c");
    }
}
