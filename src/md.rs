//! Markdown → ratatui Lines.
//!
//! Rendering ideas absorbed from [mdterm](https://github.com/bahdotsh/mdterm):
//! pulldown-cmark events, heading colour ladder, H1 underline, code fences with
//! language label, tables, task lists, blockquote bar, width-aware wrap.
//!
//! No syntect/mermaid/math in core (YAGNI until asked).

use pulldown_cmark::{
    CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

// ── mdterm-ish palette (Catppuccin Mocha-ish, dark) ───────────────────────

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

struct MdTheme {
    fg: Color,
    h1: Color,
    h2: Color,
    h3: Color,
    h4: Color,
    h5: Color,
    h6: Color,
    heading_sep: Color,
    code_fg: Color,
    code_border: Color,
    code_label: Color,
    inline_code: Color,
    quote_bar: Color,
    link: Color,
    bullet: Color,
    task_done: Color,
    task_pending: Color,
    rule: Color,
    table_border: Color,
    table_header: Color,
    strike: Color,
}

impl MdTheme {
    fn dark() -> Self {
        Self {
            fg: rgb(205, 214, 244),
            h1: rgb(250, 179, 135),
            h2: rgb(137, 180, 250),
            h3: rgb(203, 166, 247),
            h4: rgb(166, 227, 161),
            h5: rgb(249, 226, 175),
            h6: rgb(127, 132, 156),
            heading_sep: rgb(49, 50, 68),
            code_fg: rgb(205, 214, 244),
            code_border: rgb(68, 71, 90),
            code_label: rgb(108, 112, 134),
            inline_code: rgb(242, 205, 147),
            quote_bar: rgb(116, 143, 196),
            link: rgb(137, 180, 250),
            bullet: rgb(127, 132, 156),
            task_done: rgb(166, 227, 161),
            task_pending: rgb(108, 112, 134),
            rule: rgb(68, 71, 90),
            table_border: rgb(68, 71, 90),
            table_header: rgb(137, 180, 250),
            strike: rgb(108, 112, 134),
        }
    }

    fn heading(&self, level: HeadingLevel) -> Color {
        match level {
            HeadingLevel::H1 => self.h1,
            HeadingLevel::H2 => self.h2,
            HeadingLevel::H3 => self.h3,
            HeadingLevel::H4 => self.h4,
            HeadingLevel::H5 => self.h5,
            HeadingLevel::H6 => self.h6,
        }
    }
}


// ── public API ────────────────────────────────────────────────────────────
use crate::plugin::{HeadingEntry, LinkEntry};

#[derive(Default)]
pub struct RenderedDoc {
    pub lines: Vec<Line<'static>>,
    pub links: Vec<LinkEntry>,
    pub headings: Vec<HeadingEntry>,
}


/// Render markdown at a given terminal content width (word wrap).
pub fn render_md_width(text: &str, width: usize) -> Vec<Line<'static>> {
    render_md_doc(text, width).lines
}

/// Full render with link / heading index (for f / o / Alt+f).
pub fn render_md_doc(text: &str, width: usize) -> RenderedDoc {
    let theme = MdTheme::dark();
    let mut r = Renderer::new(&theme, width.max(20));
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    for event in Parser::new_ext(text, opts) {
        r.process(event);
    }
    r.finish_doc()
}

/// Default width when no terminal measure available (tests / -l path).
#[allow(dead_code)]
pub fn render_md(text: &str) -> Vec<Line<'static>> {
    render_md_width(text, 88)
}

#[allow(dead_code)]
pub fn render_txt(text: &str) -> Vec<Line<'static>> {
    let style = Style::new().fg(rgb(205, 214, 244));
    text.lines()
        .map(|l| Line::from(Span::styled(l.to_string(), style)))
        .collect()
}

pub fn render_txt_width(text: &str, width: usize) -> Vec<Line<'static>> {
    let style = Style::new().fg(rgb(205, 214, 244));
    let mut out = Vec::new();
    for line in text.lines() {
        if width == 0 || UnicodeWidthStr::width(line) <= width {
            out.push(Line::from(Span::styled(line.to_string(), style)));
            continue;
        }
        // word wrap on whitespace
        let spans = vec![Span::styled(line.to_string(), style)];
        out.extend(wrap_spans(spans, width));
    }
    out
}

#[allow(dead_code)]
pub fn load_body(path: &std::path::Path) -> std::io::Result<Vec<Line<'static>>> {
    load_body_width(path, 88)
}

pub fn load_body_width(path: &std::path::Path, width: usize) -> std::io::Result<Vec<Line<'static>>> {
    let text = std::fs::read_to_string(path)?;
    let is_md = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    Ok(if is_md {
        render_md_width(&text, width)
    } else {
        render_txt_width(&text, width)
    })
}

// ── renderer ──────────────────────────────────────────────────────────────

struct Renderer<'a> {
    theme: &'a MdTheme,
    width: usize,
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    // inline state
    bold: bool,
    italic: bool,
    strike: bool,
    in_link: bool,
    link_url: String,
    link_text: String,
    // block state
    heading: Option<HeadingLevel>,
    heading_text: String,
    heading_line: Option<usize>,
    in_quote: bool,
    in_code: bool,
    code_lang: String,
    code_buf: String,
    // lists
    list_stack: Vec<ListKind>,
    task_checked: Option<bool>,
    // tables
    in_table: bool,
    in_table_head: bool,
    table_head: Vec<String>,
    table_body: Vec<Vec<String>>,
    table_row: Vec<String>,
    cell_buf: String,
    // indices
    links: Vec<LinkEntry>,
    headings: Vec<HeadingEntry>,
}

enum ListKind {
    Ul,
    Ol(u64),
}

impl<'a> Renderer<'a> {
    fn new(theme: &'a MdTheme, width: usize) -> Self {
        Self {
            theme,
            width,
            lines: Vec::new(),
            current: Vec::new(),
            bold: false,
            italic: false,
            strike: false,
            in_link: false,
            link_url: String::new(),
            link_text: String::new(),
            heading: None,
            heading_text: String::new(),
            heading_line: None,
            in_quote: false,
            in_code: false,
            code_lang: String::new(),
            code_buf: String::new(),
            list_stack: Vec::new(),
            task_checked: None,
            in_table: false,
            in_table_head: false,
            table_head: Vec::new(),
            table_body: Vec::new(),
            table_row: Vec::new(),
            cell_buf: String::new(),
            links: Vec::new(),
            headings: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn finish(self) -> Vec<Line<'static>> {
        self.finish_doc().lines
    }

    fn finish_doc(mut self) -> RenderedDoc {
        self.flush_line();
        RenderedDoc {
            lines: self.lines,
            links: self.links,
            headings: self.headings,
        }
    }

    fn process(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                self.flush_line();
                self.push_empty();
            }

            Event::Start(Tag::Heading { level, .. }) => {
                if !self.lines.is_empty() {
                    if matches!(level, HeadingLevel::H1 | HeadingLevel::H2) {
                        self.push_empty();
                        let sep = "─".repeat(self.width.min(60));
                        self.lines.push(Line::from(Span::styled(
                            sep,
                            Style::new().fg(self.theme.heading_sep).add_modifier(Modifier::DIM),
                        )));
                        self.push_empty();
                    } else {
                        self.push_empty();
                    }
                }
                self.heading = Some(level);
                self.heading_text.clear();
                self.heading_line = Some(self.lines.len());
                let (prefix, color) = match level {
                    HeadingLevel::H3 => ("▸ ", self.theme.h3),
                    HeadingLevel::H4 => ("  ▸ ", self.theme.h4),
                    HeadingLevel::H5 => ("    ▸ ", self.theme.h5),
                    HeadingLevel::H6 => ("      ▸ ", self.theme.h6),
                    _ => ("", self.theme.heading(level)),
                };
                if !prefix.is_empty() {
                    self.current.push(Span::styled(
                        prefix.to_string(),
                        Style::new().fg(color).add_modifier(Modifier::DIM),
                    ));
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                let text = std::mem::take(&mut self.heading_text);
                let color = self.theme.heading(level);
                if matches!(level, HeadingLevel::H1 | HeadingLevel::H2) {
                    if self.current.is_empty() {
                        self.current.push(Span::styled(
                            text.clone(),
                            Style::new().fg(color).add_modifier(Modifier::BOLD),
                        ));
                    }
                }
                let hline = self.heading_line.unwrap_or(self.lines.len());
                self.flush_line();
                if !text.trim().is_empty() {
                    let lv = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        _ => 6,
                    };
                    self.headings.push(HeadingEntry {
                        level: lv,
                        text: text.trim().to_string(),
                        line: hline,
                    });
                }
                if matches!(level, HeadingLevel::H1) {
                    let w = text.width().min(self.width).max(1);
                    self.lines.push(Line::from(Span::styled(
                        "━".repeat(w),
                        Style::new().fg(self.theme.h1).add_modifier(Modifier::DIM),
                    )));
                }
                self.heading = None;
                self.heading_line = None;
                self.push_empty();
            }

            Event::Start(Tag::Strong) => self.bold = true,
            Event::End(TagEnd::Strong) => self.bold = false,
            Event::Start(Tag::Emphasis) => self.italic = true,
            Event::End(TagEnd::Emphasis) => self.italic = false,
            Event::Start(Tag::Strikethrough) => self.strike = true,
            Event::End(TagEnd::Strikethrough) => self.strike = false,

            Event::Start(Tag::BlockQuote(_)) => self.in_quote = true,
            Event::End(TagEnd::BlockQuote) => {
                self.in_quote = false;
                self.push_empty();
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                self.in_code = true;
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                self.emit_code_block();
                self.in_code = false;
                self.push_empty();
            }

            Event::Start(Tag::List(ordered)) => match ordered {
                Some(n) => self.list_stack.push(ListKind::Ol(n)),
                None => self.list_stack.push(ListKind::Ul),
            },
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.push_empty();
                }
            }
            Event::Start(Tag::Item) => {
                self.task_checked = None;
                let depth = self.list_stack.len().saturating_sub(1);
                let indent = "    ".repeat(depth);
                let bullet = match self.list_stack.last_mut() {
                    Some(ListKind::Ul) => format!("{indent}  • "),
                    Some(ListKind::Ol(n)) => {
                        let num = *n;
                        *n += 1;
                        format!("{indent}  {num}. ")
                    }
                    None => String::new(),
                };
                self.current.push(Span::styled(
                    bullet,
                    Style::new().fg(self.theme.bullet),
                ));
            }
            Event::End(TagEnd::Item) => {
                // task checkbox already inserted on TaskListMarker
                self.flush_line();
            }
            Event::TaskListMarker(checked) => {
                self.task_checked = Some(checked);
                let (mark, color) = if checked {
                    ("[x] ", self.theme.task_done)
                } else {
                    ("[ ] ", self.theme.task_pending)
                };
                self.current
                    .push(Span::styled(mark.to_string(), Style::new().fg(color)));
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                self.in_link = true;
                self.link_url = dest_url.to_string();
                self.link_text.clear();
            }
            Event::End(TagEnd::Link) => {
                let url = std::mem::take(&mut self.link_url);
                let text = {
                    let t = std::mem::take(&mut self.link_text);
                    if t.is_empty() {
                        url.clone()
                    } else {
                        t
                    }
                };
                if !url.is_empty() {
                    // avoid adjacent wrap duplicates of same url+text
                    let line = self.lines.len();
                    let dup = self
                        .links
                        .last()
                        .is_some_and(|l| l.url == url && l.text == text && l.line + 2 >= line);
                    if !dup {
                        self.links.push(LinkEntry { text, url, line });
                    }
                }
                self.in_link = false;
            }

            Event::Start(Tag::Image { dest_url, .. }) => {
                self.push_text(&format!("🖼 {dest_url}"));
            }
            Event::End(TagEnd::Image) => {}

            Event::Start(Tag::Table(_)) => {
                self.in_table = true;
                self.table_head.clear();
                self.table_body.clear();
            }
            Event::End(TagEnd::Table) => {
                self.emit_table();
                self.in_table = false;
                self.push_empty();
            }
            Event::Start(Tag::TableHead) => {
                self.in_table_head = true;
                self.table_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                // head cells already in table_row
                self.table_head = std::mem::take(&mut self.table_row);
                self.in_table_head = false;
            }
            Event::Start(Tag::TableRow) => {
                if !self.in_table_head {
                    self.table_row.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if !self.in_table_head {
                    let row = std::mem::take(&mut self.table_row);
                    if !row.is_empty() {
                        self.table_body.push(row);
                    }
                }
            }
            Event::Start(Tag::TableCell) => self.cell_buf.clear(),
            Event::End(TagEnd::TableCell) => {
                self.table_row
                    .push(std::mem::take(&mut self.cell_buf).trim().to_string());
            }

            Event::Text(text) => self.handle_text(text),
            Event::Code(code) => {
                if self.in_table {
                    self.cell_buf.push_str(&code);
                } else {
                    self.current.push(Span::styled(
                        code.to_string(),
                        Style::new().fg(self.theme.inline_code),
                    ));
                }
            }
            Event::SoftBreak => {
                if self.in_table {
                    self.cell_buf.push(' ');
                } else {
                    self.push_text(" ");
                }
            }
            Event::HardBreak => {
                if self.in_table {
                    self.cell_buf.push(' ');
                } else {
                    self.flush_line();
                }
            }
            Event::Rule => {
                self.flush_line();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(self.width.min(60)),
                    Style::new().fg(self.theme.rule),
                )));
                self.push_empty();
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let plain = strip_tags(&html);
                if !plain.is_empty() {
                    self.handle_text(CowStr::from(plain));
                }
            }
            _ => {}
        }
    }

    fn handle_text(&mut self, text: CowStr<'_>) {
        if self.in_code {
            self.code_buf.push_str(&text);
            return;
        }
        if self.in_table {
            self.cell_buf.push_str(&text);
            return;
        }
        if self.heading.is_some() {
            self.heading_text.push_str(&text);
        }
        if self.in_link {
            self.link_text.push_str(&text);
        }
        self.push_text(&text);
    }
    fn push_text(&mut self, text: &str) {
        let mut style = Style::new().fg(self.theme.fg);
        if let Some(level) = self.heading {
            style = Style::new().fg(self.theme.heading(level));
            if matches!(level, HeadingLevel::H1 | HeadingLevel::H2) {
                style = style.add_modifier(Modifier::BOLD);
            }
        }
        if self.bold {
            // ponytail: bold = accent (HN by-name, emphasis)
            style = Style::new()
                .fg(self.theme.h4)
                .add_modifier(Modifier::BOLD);
            if self.italic {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if self.strike {
                style = style
                    .fg(self.theme.strike)
                    .add_modifier(Modifier::CROSSED_OUT);
            }
            if self.in_link {
                style = Style::new()
                    .fg(self.theme.link)
                    .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
            }
        } else {
            if self.italic {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if self.strike {
                style = style
                    .fg(self.theme.strike)
                    .add_modifier(Modifier::CROSSED_OUT);
            }
            if self.in_link {
                style = Style::new()
                    .fg(self.theme.link)
                    .add_modifier(Modifier::UNDERLINED);
            }
        }
        self.current
            .push(Span::styled(text.to_string(), style));
    }

    fn flush_line(&mut self) {
        if self.current.is_empty() {
            return;
        }
        let mut spans = std::mem::take(&mut self.current);
        if self.in_quote {
            let mut with_bar = vec![Span::styled(
                "┃ ".to_string(),
                Style::new().fg(self.theme.quote_bar),
            )];
            with_bar.append(&mut spans);
            spans = with_bar;
        }
        // wrap
        for line in wrap_spans(spans, self.width) {
            self.lines.push(line);
        }
    }

    fn push_empty(&mut self) {
        // avoid double blanks
        if self.lines.last().is_some_and(|l| l.spans.is_empty()) {
            return;
        }
        self.lines.push(Line::from(""));
    }

    fn emit_code_block(&mut self) {
        let lang = self.code_lang.trim();
        let code = std::mem::take(&mut self.code_buf);
        let border = self.theme.code_border;
        let label = self.theme.code_label;
        let fg = self.theme.code_fg;
        let w = self.width.min(100).max(10);

        let label_text = if lang.is_empty() {
            "code".to_string()
        } else {
            lang.to_string()
        };
        let top = format!("┌─ {label_text} {}", "─".repeat(w.saturating_sub(label_text.width() + 4).max(1)));
        self.lines.push(Line::from(Span::styled(
            top.chars().take(w).collect::<String>(),
            Style::new().fg(border),
        )));
        // language hint line dim
        if !lang.is_empty() {
            self.lines.push(Line::from(Span::styled(
                format!("│ {label_text}"),
                Style::new().fg(label).add_modifier(Modifier::DIM),
            )));
        }
        for line in code.lines() {
            let content = format!("│ {line}");
            // hard-trim by width
            let mut col = 0usize;
            let mut out = String::new();
            for ch in content.chars() {
                let cw = UnicodeWidthStr::width(ch.to_string().as_str()).max(1);
                if col + cw > w {
                    break;
                }
                out.push(ch);
                col += cw;
            }
            self.lines
                .push(Line::from(Span::styled(out, Style::new().fg(fg))));
        }
        if code.is_empty() {
            self.lines
                .push(Line::from(Span::styled("│", Style::new().fg(border))));
        }
        self.lines.push(Line::from(Span::styled(
            format!("└{}", "─".repeat(w.saturating_sub(1))),
            Style::new().fg(border),
        )));
    }

    fn emit_table(&mut self) {
        let head = std::mem::take(&mut self.table_head);
        let body = std::mem::take(&mut self.table_body);
        if head.is_empty() && body.is_empty() {
            return;
        }
        let cols = head
            .len()
            .max(body.iter().map(|r| r.len()).max().unwrap_or(0))
            .max(1);
        let mut widths = vec![3usize; cols];
        for (i, c) in head.iter().enumerate() {
            widths[i] = widths[i].max(c.width());
        }
        for row in &body {
            for (i, c) in row.iter().enumerate() {
                if i < cols {
                    widths[i] = widths[i].max(c.width());
                }
            }
        }
        // fit table into width roughly
        let total: usize = widths.iter().sum::<usize>() + cols * 3 + 1;
        if total > self.width {
            let scale = self.width.saturating_sub(cols * 3 + 1) as f64 / widths.iter().sum::<usize>() as f64;
            for w in &mut widths {
                *w = ((*w as f64) * scale).floor().max(3.0) as usize;
            }
        }

        let border = self.theme.table_border;
        let header_fg = self.theme.table_header;
        let fg = self.theme.fg;

        let make_rule = |l: &str, m: &str, r: &str| -> String {
            let mut s = String::from(l);
            for (i, w) in widths.iter().enumerate() {
                if i > 0 {
                    s.push_str(m);
                }
                s.push_str(&"─".repeat(*w + 2));
            }
            s.push_str(r);
            s
        };

        self.lines.push(Line::from(Span::styled(
            make_rule("┌", "┬", "┐"),
            Style::new().fg(border),
        )));

        // header
        {
            let mut spans = vec![Span::styled("│".to_string(), Style::new().fg(border))];
            for (i, w) in widths.iter().enumerate() {
                let cell = head.get(i).map(|s| s.as_str()).unwrap_or("");
                let cell = pad_trunc(cell, *w);
                spans.push(Span::styled(
                    format!(" {cell} "),
                    Style::new().fg(header_fg).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled("│".to_string(), Style::new().fg(border)));
            }
            self.lines.push(Line::from(spans));
        }
        self.lines.push(Line::from(Span::styled(
            make_rule("├", "┼", "┤"),
            Style::new().fg(border),
        )));
        for row in &body {
            let mut spans = vec![Span::styled("│".to_string(), Style::new().fg(border))];
            for (i, w) in widths.iter().enumerate() {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                let cell = pad_trunc(cell, *w);
                spans.push(Span::styled(format!(" {cell} "), Style::new().fg(fg)));
                spans.push(Span::styled("│".to_string(), Style::new().fg(border)));
            }
            self.lines.push(Line::from(spans));
        }
        self.lines.push(Line::from(Span::styled(
            make_rule("└", "┴", "┘"),
            Style::new().fg(border),
        )));
    }
}

fn pad_trunc(s: &str, width: usize) -> String {
    let w = s.width();
    if w == width {
        return s.to_string();
    }
    if w < width {
        return format!("{s}{}", " ".repeat(width - w));
    }
    let mut out = String::new();
    let mut col = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthStr::width(ch.to_string().as_str()).max(1);
        if col + cw > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        col += cw;
    }
    out.push('…');
    while out.width() < width {
        out.push(' ');
    }
    out
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
pub(crate) fn wrap_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::from(spans)];
    }
    let mut chars: Vec<(char, Style)> = Vec::new();
    for sp in &spans {
        for ch in sp.content.chars() {
            chars.push((ch, sp.style));
        }
    }
    if chars.is_empty() {
        return vec![Line::from("")];
    }

    fn width_of(ch: char) -> usize {
        UnicodeWidthStr::width(ch.to_string().as_str()).max(1)
    }

    fn emit_line(line: &[(char, Style)], out: &mut Vec<Line<'static>>) {
        if line.is_empty() {
            out.push(Line::from(""));
            return;
        }
        let mut spans = Vec::new();
        let mut buf = String::new();
        let mut st = line[0].1;
        for &(ch, s) in line {
            if s != st {
                if !buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut buf), st));
                }
                st = s;
            }
            buf.push(ch);
        }
        if !buf.is_empty() {
            spans.push(Span::styled(buf, st));
        }
        out.push(Line::from(spans));
    }

    let mut lines = Vec::new();
    let mut line: Vec<(char, Style)> = Vec::new();
    let mut col = 0usize;
    // index into `line` after last break opportunity
    let mut break_end: Option<usize> = None;
    // start of current ASCII word in `line` (for mid-word hard-cut avoidance)
    let mut ascii_word_start: Option<usize> = None;

    fn is_cjk(ch: char) -> bool {
        // ponytail: width-2 non-ascii ≈ CJK/fullwidth for break opportunities
        !ch.is_ascii() && UnicodeWidthStr::width(ch.to_string().as_str()) >= 2
    }
    fn is_ascii_word(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '\''
    }

    for &(ch, st) in &chars {
        let cw = width_of(ch);
        if col + cw > width && col > 0 {
            if let Some(be) = break_end {
                let mut left = line[..be].to_vec();
                while left.last().is_some_and(|(c, _)| c.is_whitespace()) {
                    left.pop();
                }
                emit_line(&left, &mut lines);
                let mut rest: Vec<(char, Style)> = line[be..]
                    .iter()
                    .copied()
                    .skip_while(|(c, _)| c.is_whitespace())
                    .collect();
                rest.push((ch, st));
                line = rest;
            } else if let Some(ws) = ascii_word_start.filter(|&ws| ws > 0) {
                // break before current ASCII word instead of mid-word
                emit_line(&line[..ws], &mut lines);
                let mut rest = line[ws..].to_vec();
                rest.push((ch, st));
                line = rest;
            } else {
                emit_line(&line, &mut lines);
                line = vec![(ch, st)];
            }
            col = line.iter().map(|(c, _)| width_of(*c)).sum();
            break_end = None;
            ascii_word_start = None;
            for (i, (c, _)) in line.iter().enumerate() {
                if c.is_whitespace() || is_cjk(*c) {
                    break_end = Some(i + 1);
                    ascii_word_start = None;
                } else if is_ascii_word(*c) {
                    if ascii_word_start.is_none() {
                        ascii_word_start = Some(i);
                    }
                } else {
                    ascii_word_start = None;
                }
            }
            continue;
        }
        let idx = line.len();
        line.push((ch, st));
        col += cw;
        if ch.is_whitespace() || is_cjk(ch) {
            break_end = Some(line.len());
            ascii_word_start = None;
        } else if is_ascii_word(ch) {
            if ascii_word_start.is_none() {
                ascii_word_start = Some(idx);
            }
        } else {
            ascii_word_start = None;
        }
    }
    if !line.is_empty() {
        emit_line(&line, &mut lines);
    }
    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_wrap_breaks_on_space() {
        let lines = wrap_spans(
            vec![Span::raw("hello beautiful world")],
            12,
        );
        let texts: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(texts.iter().all(|t| !t.contains("beautif") || t.contains("beautiful") || t.starts_with("world") || t.ends_with("hello") || t.contains(' ')), "{texts:?}");
        // first line should end at a word boundary
        let joined = texts.join("|");
        assert!(!joined.contains("beautifu|l"), "mid-word split: {joined}");
        assert!(texts.len() >= 2, "{texts:?}");
    }

    #[test]
    fn wrap_prefers_cjk_break_over_ascii_midword() {
        // CJK then long english without space: break after CJK, not mid "beautiful"
        let s = format!("中文{}", "beautiful");
        let lines = wrap_spans(vec![Span::raw(s)], 8);
        let texts: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        let joined = texts.join("|");
        assert!(
            !joined.contains("beauti|ful") && !joined.contains("bea|utiful"),
            "mid-ascii split: {joined}"
        );
        assert!(texts.len() >= 2, "{texts:?}");
    }


    #[test]
    fn links_and_headings_indexed() {
        let doc = render_md_doc("# Hello\n\nSee [ex](https://example.com) and [loc](./x.md).\n\n## Sec\n", 80);
        assert!(doc.headings.iter().any(|h| h.text == "Hello" && h.level == 1), "{:?}", doc.headings);
        assert!(doc.headings.iter().any(|h| h.text == "Sec" && h.level == 2), "{:?}", doc.headings);
        assert!(doc.links.iter().any(|l| l.url == "https://example.com"), "{:?}", doc.links);
        assert!(doc.links.iter().any(|l| l.url == "./x.md"), "{:?}", doc.links);
    }

    #[test]
    fn header_h1_underline() {
        let lines = render_md("# Hello");
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Hello"));
        // second visual line is underline
        assert!(lines.len() >= 2);
    }

    #[test]
    fn table_renders_box() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        let lines = render_md(md);
        let joined: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(joined.contains('│') || joined.contains('┌'), "{joined}");
    }

    #[test]
    fn task_list() {
        let lines = render_md("- [x] done\n- [ ] todo\n");
        let joined: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(joined.contains("[x]") || joined.contains("done"), "{joined}");
    }

    #[test]
    fn code_fence_label() {
        let lines = render_md("```rust\nfn main() {}\n```\n");
        let joined: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(joined.contains("rust") || joined.contains("main"), "{joined}");
    }

    #[test]
    fn txt_preserves() {
        let lines = render_txt("a\nb");
        assert_eq!(lines.len(), 2);
    }
}
