//! Dict/code HTML → ratatui Lines (+ outline for host `o` jump).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::html_css::{Resolved, StyleTable, TermStyle};
use crate::plugin::HeadingEntry;

/// Convert dictionary HTML directly into styled ratatui Lines.
///
/// Single pass, no Markdown intermediate. A style stack tracks nested elements;
/// class/tag rules from the dictionary `css` drive colors, falling back to
/// built-in defaults for anything unmatched. Pass `&StyleTable::default()` for
/// dictionaries without a stylesheet.
#[allow(dead_code)] // public API / tests
pub fn html_to_lines(html: &str, css: &StyleTable) -> Vec<Line<'static>> {
    html_to_doc(html, css).0
}

/// Lines + outline (H1–H6 and dict sense markers: pos/df/se2/…).
pub fn html_to_doc(html: &str, css: &StyleTable) -> (Vec<Line<'static>>, Vec<HeadingEntry>) {
    let mut r = Renderer::new(css);
    r.run(html);
    r.finish_doc()
}

/// Dict sense / section markers (class substrings, lowercase).
fn sense_level(classes: &[&str]) -> Option<u8> {
    let mut best: Option<u8> = None;
    for c in classes {
        let c = c.to_ascii_lowercase();
        // noise / layout helpers
        if c.contains("offset")
            || c.contains("btn")
            || c.contains("icon")
            || c.contains("param")
            || c.contains("favorite")
            || c.contains("toggle")
            || c.contains("blank")
            || c == "corrse2firstline"
            || c == "example"
            || c == "ex"
            || c == "ch"
            || c == "source"
            || c == "underline"
            || c == "error"
        {
            continue;
        }
        // 英语常用词疑难用法手册: senselevel-1 / senselevel-2
        if let Some(rest) = c.strip_prefix("senselevel-") {
            if let Ok(n) = rest.parse::<u8>() {
                let l = n.clamp(1, 6);
                best = Some(match best {
                    Some(b) => b.min(l),
                    None => l,
                });
                continue;
            }
        }
        // level-N container sometimes wraps sense; prefer senselevel; treat level-N as weak
        if let Some(rest) = c.strip_prefix("level-") {
            if rest.chars().all(|ch| ch.is_ascii_digit()) {
                // skip pure layout level wrappers — sense text is in senselevel
                continue;
            }
        }
        let lv = if c == "word" || c == "hw" || c == "headword" {
            Some(1)
        } else if c == "pos"
            || c == "partofspeech"
            || c == "word-class"
            || c == "wordclass"
            || (c.ends_with("pos") && c.len() <= 5)
        {
            Some(1)
        } else if c == "se2"
            || c == "sense"
            || c == "sensenum"
            || c == "defnum"
            || c == "ordinal"
            || (c.starts_with("se2") && c != "se2g" && c != "se2gone")
        {
            Some(2)
        } else if c == "df"
            || c == "def"
            || c == "defcn"
            || c == "meaning"
            || c == "translation"
            || (c.starts_with("def") && !c.contains("btn"))
        {
            Some(3)
        } else {
            None
        };
        if let Some(l) = lv {
            best = Some(match best {
                Some(b) => b.min(l),
                None => l,
            });
        }
    }
    best
}

/// Prefer `1) …` / `a) …` label + short preview for outline list.
fn outline_label(raw: &str) -> String {
    let t = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return t;
    }
    // keep numbered / lettered sense markers prominent
    let max = 72usize;
    if t.chars().count() <= max {
        return t;
    }
    let mut s: String = t.chars().take(max.saturating_sub(1)).collect();
    s.push('…');
    s
}

/// One entry on the element stack: the tag that opened it plus the style state
/// it contributes, so the closing tag can pop exactly what it pushed.
struct Frame {
    /// Lowercased tag name (`b`, `span`, `h1`, …).
    tag: String,
    /// Built-in inline emphasis this frame turns on (for the no-CSS cascade).
    bold: bool,
    italic: bool,
    underline: bool,
    heading: bool,
    /// CSS-resolved style for this element, if any rule matched.
    css: Option<TermStyle>,
    /// Whether this frame opened a skip region (display:none or known junk).
    skipped: bool,
}

struct Renderer<'a> {
    css: &'a StyleTable,
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    buf: String,
    stack: Vec<Frame>,
    /// Depth of the current skipped subtree; >0 means drop all text.
    skip: usize,
    headings: Vec<HeadingEntry>,
    /// Active heading capture: (level, start_line, text)
    heading_cap: Option<(u8, usize, String)>,
}

impl<'a> Renderer<'a> {
    fn new(css: &'a StyleTable) -> Self {
        Renderer {
            css,
            lines: Vec::new(),
            spans: Vec::new(),
            buf: String::new(),
            stack: Vec::new(),
            skip: 0,
            headings: Vec::new(),
            heading_cap: None,
        }
    }

    fn run(&mut self, html: &str) {
        let mut chars = html.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '<' => {
                    // Read to the closing '>', skipping any '>' inside quotes.
                    let mut tag = String::new();
                    let mut quote: Option<char> = None;
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        match quote {
                            Some(q) => {
                                if nc == q {
                                    quote = None;
                                }
                                tag.push(nc);
                            }
                            None => {
                                if nc == '>' {
                                    break;
                                }
                                if nc == '"' || nc == '\'' {
                                    quote = Some(nc);
                                }
                                tag.push(nc);
                            }
                        }
                    }
                    self.tag(&tag);
                }
                '&' => {
                    let mut ent = String::new();
                    while let Some(&nc) = chars.peek() {
                        if nc == ';' {
                            chars.next();
                            break;
                        }
                        if ent.len() >= 12 {
                            break;
                        }
                        ent.push(nc);
                        chars.next();
                    }
                    if self.skip == 0
                        && let Some(d) = decode_entity(&ent)
                    {
                        self.buf.push(d);
                    }
                }
                _ if self.skip == 0 => self.buf.push(c),
                _ => {}
            }
        }
    }

    fn tag(&mut self, raw: &str) {
        let trimmed = raw.trim();
        // Self-closing / void tags carry no scope; handle and return.
        let is_close = trimmed.starts_with('/');
        let name = tag_name(trimmed);

        if is_close {
            self.close(&name);
            return;
        }

        // Void elements: no matching close, never pushed on the stack.
        match name.as_str() {
            "br" => {
                self.break_line();
                return;
            }
            "img" => {
                // R-01: emit alt (or title) so dict images leave a text trace.
                if self.skip == 0 {
                    if let Some(alt) = attr_value(raw, "alt").or_else(|| attr_value(raw, "title")) {
                        let alt = alt.trim();
                        if !alt.is_empty() {
                            self.flush_text();
                            self.buf.push('[');
                            self.buf.push_str(alt);
                            self.buf.push(']');
                        }
                    }
                }
                return;
            }
            "link" | "meta" | "hr" | "input" | "col" | "wbr" => return,
            _ => {}
        }

        self.open(&name, raw);
    }

    fn open(&mut self, name: &str, raw: &str) {
        let classes = extract_classes(raw);
        let class_refs: Vec<&str> = classes.iter().map(String::as_str).collect();

        // Resolve element styles from the dictionary CSS (empty table → all
        // misses → built-in fallback cascade).
        let resolved: Resolved = self.css.resolve(name, &class_refs);

        // Decide whether this element opens a skipped subtree.
        let legacy_skip = name == "div"
            && class_refs
                .iter()
                .any(|c| c.contains("to-contexts") || c.contains("picture"));
        let skipped = resolved.style.hidden || legacy_skip;

        // A style change means the current text run ends here. Flush BEFORE
        // emitting ::before / bullet text so they join the NEW element's run.
        self.flush_text();

        // Emit ::before content before entering the element (unless skipped).
        if !skipped
            && self.skip == 0
            && let Some(prefix) = &resolved.prefix {
            self.buf.push_str(prefix);
        }

        // Block-level and structural tags force line breaks / bullets.
        // table/tr = row lines; td/th = cell separators (R-02 simple grid).
        let block = resolved.style.block
            || matches!(
                name,
                "p" | "div" | "h1" | "h2" | "h3" | "li" | "ul" | "ol" | "table" | "tr" | "thead"
                    | "tbody"
            );
        if !skipped && self.skip == 0 && block {
            self.break_line();
        }
        if !skipped && self.skip == 0 && name == "li" {
            self.buf.push_str("• ");
        }
        // ponytail: no column align; cells separated by " | "
        if !skipped && self.skip == 0 && matches!(name, "td" | "th") {
            if !self.buf.is_empty() || !self.spans.is_empty() {
                self.buf.push_str(" | ");
            }
        }

        // Built-in emphasis (used only when no CSS rule speaks to this element).
        let heading = matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
        let bold = matches!(name, "b" | "strong")
            || class_refs.iter().any(|c| c.contains("bold"));
        let italic = matches!(name, "i" | "em");
        let underline = name == "u" || class_refs.iter().any(|c| c.contains("underline"));

        if skipped {
            self.skip += 1;
        }

        // outline: HTML headings OR dict sense classes (pos/df/se2…)
        let mut opened_outline = false;
        if !skipped && self.skip == 0 && self.heading_cap.is_none() {
            if heading {
                let level = match name {
                    "h1" => 1,
                    "h2" => 2,
                    "h3" => 3,
                    "h4" => 4,
                    "h5" => 5,
                    _ => 6,
                };
                self.heading_cap = Some((level, self.lines.len(), String::new()));
                opened_outline = true;
            } else if let Some(level) = sense_level(&class_refs) {
                let is_wrap = class_refs.iter().any(|c| {
                    let c = c.to_ascii_lowercase();
                    matches!(
                        c.as_str(),
                        "se2g" | "se2gone" | "posg" | "egblock" | "sg" | "se1" | "sgposdiv"
                    ) || c.ends_with('g') && (c.starts_with("se") || c.starts_with("pos"))
                });
                if !is_wrap {
                    self.heading_cap = Some((level, self.lines.len(), String::new()));
                    opened_outline = true;
                }
            }
        }

        self.stack.push(Frame {
            tag: name.to_string(),
            bold,
            italic,
            underline,
            heading: heading || opened_outline,
            css: resolved.matched.then_some(resolved.style),
            skipped,
        });
    }

    fn close(&mut self, name: &str) {
        // Pop until we find the matching tag (tolerate unclosed inline tags).
        let pos = self.stack.iter().rposition(|f| f.tag == name);
        let Some(pos) = pos else {
            // Unbalanced close (e.g. </p> with no <p>): treat block closers as
            // line breaks so layout survives.
            if matches!(
                name,
                "p" | "div" | "h1" | "h2" | "h3" | "ul" | "ol" | "table" | "tr" | "thead" | "tbody"
            ) {
                self.break_line();
                self.blank();
            } else if name == "li" {
                self.break_line();
            }
            return;
        };
        // Flush the text run BEFORE popping: it belongs to the element being
        // closed and must be styled with that element's frame still on the stack.
        if self.skip == 0 {
            self.flush_text();
        }

        // Everything above the match was unclosed inline markup; drop it.
        while self.stack.len() > pos + 1 {
            let f = self.stack.pop().unwrap();
            if f.skipped {
                self.skip = self.skip.saturating_sub(1);
            }
        }

        let frame = self.stack.pop().unwrap();
        if frame.skipped {
            self.skip = self.skip.saturating_sub(1);
        }

        if self.skip > 0 {
            return;
        }

        // close outline capture for this frame if we opened it
        if frame.heading {
            if let Some((level, line, text)) = self.heading_cap.take() {
                let t = outline_label(&text);
                if t.chars().count() >= 1 {
                    self.headings.push(HeadingEntry {
                        level,
                        text: t,
                        line,
                    });
                }
            }
        }

        // Block-level closers break the line; paragraph/heading add a blank.
        let block = frame.css.as_ref().is_some_and(|s| s.block)
            || matches!(
                name,
                "p" | "div"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "li"
                    | "ul"
                    | "ol"
                    | "table"
                    | "tr"
                    | "thead"
                    | "tbody"
            );
        if block {
            self.break_line();
        }
        if matches!(name, "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "table") {
            self.blank();
        }
    }

    /// Compute the effective style at the current stack depth.
    ///
    /// CSS frames win: their fg/bg/flags merge from outermost to innermost.
    /// If no frame carried a CSS rule, fall back to the built-in cascade
    /// (heading > bold+italic > italic > underline > default cyan).
    fn style(&self) -> Style {
        let has_css = self.stack.iter().any(|f| f.css.is_some());
        if has_css {
            let mut merged = TermStyle::default();
            for f in &self.stack {
                if let Some(s) = &f.css {
                    merged.merge(s);
                }
                // Built-in inline tags still apply on top of CSS (a <b> inside
                // a styled span should still bold).
                if f.bold {
                    merged.bold = true;
                }
                if f.italic {
                    merged.italic = true;
                }
                if f.underline {
                    merged.underline = true;
                }
            }
            return term_to_style(&merged);
        }

        // No CSS anywhere: legacy hardcoded cascade.
        let heading = self.stack.iter().any(|f| f.heading);
        if heading {
            return Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD);
        }
        let bold = self.stack.iter().any(|f| f.bold);
        let italic = self.stack.iter().any(|f| f.italic);
        let underline = self.stack.iter().any(|f| f.underline);
        if bold && italic {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC)
        } else if bold {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else if italic {
            // 斜体 = 例句，黄色
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::ITALIC)
        } else if underline {
            // 下划线 = 词头，绿色
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            // 正文默认蓝色
            Style::default().fg(Color::Cyan)
        }
    }

    fn flush_text(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        if let Some((_, _, t)) = self.heading_cap.as_mut() {
            t.push_str(&self.buf);
        }
        let style = self.style();
        self.spans
            .push(Span::styled(std::mem::take(&mut self.buf), style));
    }

    /// End the current line, emitting it only if it holds content.
    fn break_line(&mut self) {
        self.flush_text();
        if !self.spans.is_empty() {
            self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        }
    }

    /// Insert one blank separator line, collapsing consecutive blanks.
    fn blank(&mut self) {
        let last_blank = self.lines.last().is_none_or(|l| l.spans.is_empty());
        if !last_blank {
            self.lines.push(Line::raw(""));
        }
    }

    #[allow(dead_code)]
    fn finish(self) -> Vec<Line<'static>> {
        self.finish_doc().0
    }

    fn finish_doc(mut self) -> (Vec<Line<'static>>, Vec<HeadingEntry>) {
        self.break_line();
        while self.lines.last().is_some_and(|l| l.spans.is_empty()) {
            self.lines.pop();
        }
        (self.lines, self.headings)
    }
}

/// Build a ratatui `Style` from a resolved `TermStyle`. A `None` fg defaults to
/// cyan so body text stays readable on dark terminals.
fn term_to_style(s: &TermStyle) -> Style {
    let mut style = Style::default().fg(s.fg.unwrap_or(Color::Cyan));
    if let Some(bg) = s.bg {
        style = style.bg(bg);
    }
    let mut m = Modifier::empty();
    if s.bold {
        m |= Modifier::BOLD;
    }
    if s.italic {
        m |= Modifier::ITALIC;
    }
    if s.underline {
        m |= Modifier::UNDERLINED;
    }
    if s.strike {
        m |= Modifier::CROSSED_OUT;
    }
    if !m.is_empty() {
        style = style.add_modifier(m);
    }
    style
}

/// Extract the tag name (lowercased) from a raw tag body like `span class="x"`.
fn tag_name(raw: &str) -> String {
    let raw = raw.strip_prefix('/').unwrap_or(raw);
    raw.split([' ', '\t', '\n', '/', '>'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Extract all class names from a raw tag string. Handles multiple
/// space-separated classes: `class="a b c"` → ["a", "b", "c"].
fn extract_classes(tag: &str) -> Vec<String> {
    let lower = tag.to_lowercase();
    let Some(start) = lower.find("class=\"") else {
        return Vec::new();
    };
    let after = &tag[start + 7..];
    let Some(end) = after.find('"') else {
        return Vec::new();
    };
    after[..end]
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

/// First `name="…"` / `name='…'` attribute value (case-insensitive name).
fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let Some(pos) = lower.find(&key) else {
        return None;
    };
    let rest = &tag[pos + key.len()..];
    let mut chars = rest.chars();
    let q = chars.next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let mut out = String::new();
    for c in chars {
        if c == q {
            return Some(out);
        }
        out.push(c);
    }
    None
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if entity.starts_with('#') => {
            entity[1..].parse::<u32>().ok().and_then(char::from_u32)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first<'a>(lines: &'a [Line<'static>]) -> &'a Span<'static> {
        &lines[0].spans[0]
    }

    /// Test helper: render without any stylesheet (built-in fallback cascade).
    fn html_to_lines(html: &str) -> Vec<Line<'static>> {
        super::html_to_lines(html, &StyleTable::default())
    }

    #[test]
    fn html_doc_indexes_headings() {
        let (lines, hs) = super::html_to_doc(
            "<h1>Word</h1><p>def</p><h2>Sense</h2><p>more</p>",
            &StyleTable::default(),
        );
        assert!(!lines.is_empty());
        assert!(hs.iter().any(|h| h.text == "Word" && h.level == 1), "{hs:?}");
        assert!(hs.iter().any(|h| h.text == "Sense" && h.level == 2), "{hs:?}");
    }

    #[test]
    fn dict_sense_classes_in_outline() {
        let html = r#"
        <div class="sg">
          <span class="pos">VERB 动词</span>
          <ol class="se2g">
            <li class="se2"><span class="df">放, 置</span></li>
            <li class="se2"><span class="df">设定</span></li>
          </ol>
        </div>
        "#;
        let (_lines, hs) = super::html_to_doc(html, &StyleTable::default());
        assert!(
            hs.iter().any(|h| h.text.contains("VERB") || h.text.contains("动词")),
            "pos missing: {hs:?}"
        );
        assert!(
            hs.iter().any(|h| h.text.contains("放") || h.text.contains("设定")),
            "df/se2 missing: {hs:?}"
        );
    }

    #[test]
    fn senselevel_numbered_senses() {
        let html = r#"
        <h1 class="word">■ make</h1>
        <div class="level-1">
          <p class="senselevel-1">1) make 往往同后面离开一段距离的另一些单词遥相呼应。</p>
          <p class="senselevel-1">2) make somebody do something 是「使某人做某事」。</p>
          <div class="level-2">
            <p class="senselevel-2">a) 「把 Y 变为 X」。</p>
            <p class="senselevel-2">b) 「把 Y 权作 X」。</p>
          </div>
        </div>
        "#;
        let (_lines, hs) = super::html_to_doc(html, &StyleTable::default());
        assert!(
            hs.iter().any(|h| h.text.contains("make") && h.level == 1),
            "h1: {hs:?}"
        );
        assert!(hs.iter().any(|h| h.text.starts_with("1)")), "1): {hs:?}");
        assert!(hs.iter().any(|h| h.text.starts_with("2)")), "2): {hs:?}");
        assert!(hs.iter().any(|h| h.text.starts_with("a)")), "a): {hs:?}");
        assert!(hs.iter().any(|h| h.text.starts_with("b)")), "b): {hs:?}");
    }
    #[test]
    fn test_plain_text() {
        let lines = html_to_lines("hello world");
        assert_eq!(lines.len(), 1);
        assert_eq!(first(&lines).content, "hello world");
        assert_eq!(first(&lines).style.fg, Some(Color::Cyan));
    }

    #[test]
    fn test_bold_white() {
        let lines = html_to_lines("<b>bold</b>");
        assert_eq!(first(&lines).content, "bold");
        assert_eq!(first(&lines).style.fg, Some(Color::White));
        assert!(first(&lines).style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_italic_yellow() {
        let lines = html_to_lines("<i>ex</i>");
        assert_eq!(first(&lines).style.fg, Some(Color::Yellow));
        assert!(first(&lines).style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_heading_green() {
        let lines = html_to_lines("<h1>word</h1>");
        assert_eq!(first(&lines).content, "word");
        assert_eq!(first(&lines).style.fg, Some(Color::Green));
        assert!(first(&lines).style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_heading2_green() {
        let lines = html_to_lines("<h2>section</h2>");
        assert_eq!(first(&lines).style.fg, Some(Color::Green));
    }

    #[test]
    fn test_underline_green() {
        let lines = html_to_lines("<u>text</u>");
        assert_eq!(first(&lines).style.fg, Some(Color::Green));
        assert!(
            first(&lines)
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
    }

    #[test]
    fn test_list_bullet() {
        let lines = html_to_lines("<ul><li>one</li><li>two</li></ul>");
        assert!(lines[0].spans[0].content.starts_with("•"));
        assert!(lines[0].spans[0].content.contains("one"));
        assert!(lines[1].spans[0].content.contains("two"));
    }

    #[test]
    fn test_entities() {
        let lines = html_to_lines("a &amp; b &lt; c");
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "a & b < c");
    }

    #[test]
    fn test_skip_contexts() {
        let html = "before<div class=\"to-contexts\"><a href=\"#\">目录</a></div>after";
        let lines = html_to_lines(html);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("before"));
        assert!(text.contains("after"));
        assert!(!text.contains("目录"));
    }

    #[test]
    fn test_span_underline() {
        let lines = html_to_lines(r#"<span class="underline">text</span>"#);
        assert_eq!(first(&lines).style.fg, Some(Color::Green));
        assert!(
            first(&lines)
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
    }

    #[test]
    fn test_span_bold() {
        let lines = html_to_lines(r#"<span class="bold">text</span>"#);
        assert_eq!(first(&lines).style.fg, Some(Color::White));
        assert!(first(&lines).style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_link_tag_dropped() {
        let lines = html_to_lines(r#"<link rel="stylesheet" href="ate.css"/>test"#);
        assert_eq!(lines.len(), 1);
        assert_eq!(first(&lines).content, "test");
    }

    #[test]
    fn test_gt_inside_attribute() {
        // A '>' inside a quoted attribute must not close the tag early.
        let lines = html_to_lines(r#"<span title="a > b">kept</span>"#);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(text, "kept");
    }

    #[test]
    fn test_img_emits_alt() {
        let lines = html_to_lines(r#"before<img src="x.png" alt="diagram">after"#);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("before"), "{text}");
        assert!(text.contains("[diagram]"), "{text}");
        assert!(text.contains("after"), "{text}");
        assert!(!text.contains("x.png"), "{text}");
    }

    #[test]
    fn test_img_empty_alt_silent() {
        let lines = html_to_lines(r#"a<img src="x" alt="">b"#);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(text, "ab");
    }

    #[test]
    fn test_table_simple_grid() {
        // R-02: rows as lines, cells separated — not one glued blob.
        let html = "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>";
        let lines = html_to_lines(html);
        let rows: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .filter(|s: &String| !s.trim().is_empty())
            .collect();
        assert!(rows.len() >= 2, "need ≥2 rows: {rows:?}");
        assert!(
            rows[0].contains('A') && rows[0].contains('B') && rows[0].contains('|'),
            "{rows:?}"
        );
        assert!(
            rows[1].contains('1') && rows[1].contains('2') && rows[1].contains('|'),
            "{rows:?}"
        );
        // not glued as "A B 1 2" on one line
        assert!(!rows[0].contains('1'), "{rows:?}");
    }

    #[test]
    fn test_nested_quotes_in_attr() {
        // R-03: outer double, inner single (and '>') must not truncate the tag.
        let lines = html_to_lines(r#"<span title="a 'x>y' b">kept</span>"#);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(text, "kept");
    }

    #[test]
    fn test_mixed() {
        let lines = html_to_lines("<h1>word</h1><b>def</b> <i>ex</i> <u>em</u>");
        // heading on its own line, green
        assert_eq!(lines[0].spans[0].content, "word");
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Green));
        // body line: bold white, italic yellow, underline green
        let body = lines.last().unwrap();
        assert_eq!(body.spans[0].style.fg, Some(Color::White));
        assert!(body.spans.iter().any(|s| s.style.fg == Some(Color::Yellow)));
        assert!(body.spans.iter().any(|s| s.style.fg == Some(Color::Green)));
    }

    // ── CSS-driven rendering ────────────────────────────────────────────

    fn table(css: &str) -> StyleTable {
        StyleTable::parse(css)
    }

    #[test]
    fn css_class_color() {
        let t = table(".src { color: #1E90FF; font-style: italic; }");
        let lines = super::html_to_lines(r#"<span class="src">quoted</span>"#, &t);
        assert_eq!(first(&lines).content, "quoted");
        // #1E90FF (luma 122) is just below the floor → lifted; blue stays dominant.
        let Some(Color::Rgb(r, g, b)) = first(&lines).style.fg else { panic!("expected rgb") };
        assert!(b > g && g > r, "hue not preserved: {r},{g},{b}");
        assert!(first(&lines).style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn css_before_prefix() {
        let t = table(r#"li.ex::before { content: "◇"; }"#);
        let lines = super::html_to_lines(r#"<li class="ex">example</li>"#, &t);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("◇"));
        assert!(text.contains("example"));
    }

    #[test]
    fn css_display_none_hides() {
        let t = table("kk55 { display: none; }");
        let lines = super::html_to_lines("keep<kk55>gone</kk55>tail", &t);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("keep"));
        assert!(text.contains("tail"));
        assert!(!text.contains("gone"));
    }

    #[test]
    fn css_strike_through() {
        let t = table(".err { color: brown; text-decoration: line-through; }");
        let lines = super::html_to_lines(r#"<span class="err">wrong</span>"#, &t);
        assert!(
            first(&lines)
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
    }

    #[test]
    fn css_background_color() {
        let t = table(".hl { background-color: #0072c6; }");
        let lines = super::html_to_lines(r#"<span class="hl">head</span>"#, &t);
        assert_eq!(first(&lines).style.bg, Some(Color::Rgb(0, 0x72, 0xc6)));
    }

    #[test]
    fn css_unmatched_falls_back_to_default() {
        // A tag with no CSS rule keeps the built-in cyan default.
        let t = table(".other { color: red; }");
        let lines = super::html_to_lines("plain text", &t);
        assert_eq!(first(&lines).style.fg, Some(Color::Cyan));
    }

    #[test]
    fn css_display_block_breaks_line() {
        let t = table("span.ch { display: block; color: #696969; }");
        let lines =
            super::html_to_lines(r#"a<span class="ch">translation</span>b"#, &t);
        // The block span forces its content onto its own line.
        assert!(lines.len() >= 2);
    }



}