//! org-mode → ratatui Lines（orgize 事件遍历；core 渲染后端）。
//!
//! 接口与 md.rs 同构：`render_org_doc` → `RenderedDoc{lines, links, headings}`，
//! 标题用「缩进 + 星号 + 层级色」区分（层级靠缩进，颜色辅助），
//! 链接/标题索引供 f / o 跳转复用。
//!
//! 事件流说明（orgize）：Enter(Headline) → 标题行内元素事件 → Enter(Section)
//! 开始正文；因此 Enter(Section) 时收束标题行并退出 in_title 阶段。

use orgize::ast::Headline;
use orgize::export::{Container, Event, TraversalContext, Traverser};
use orgize::rowan::ast::AstNode;
use orgize::{Org, SyntaxElement, SyntaxKind};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::plugin::{HeadingEntry, LinkEntry, RenderedDoc};

/// 正文默认前景（与 md.rs 主题一致：终端浅灰蓝）。
const BODY_FG: Color = Color::Rgb(205, 214, 244);
/// 代码/等宽字体颜色。
const CODE_FG: Color = Color::Rgb(224, 175, 104);
/// 链接描述颜色。
const LINK_FG: Color = Color::LightCyan;
/// TODO 关键词 / 未完成状态。
const TODO_FG: Color = Color::LightYellow;
/// DONE 关键词。
const DONE_FG: Color = Color::LightGreen;
/// 其他状态关键词 / 非 A 优先级。
const OTHER_TODO_FG: Color = Color::LightMagenta;
/// [#A] 优先级。
const PRIO_A_FG: Color = Color::LightRed;
/// 标题标签 :a:b:。
const TAG_FG: Color = Color::Cyan;
/// SCHEDULED/DEADLINE/CLOSED 日期。
const DATE_FG: Color = Color::LightBlue;

/// 标题层级调色板：h1~h6（7 级及以上复用 h6 色）。
const HEADING_COLORS: [Color; 6] = [
    Color::LightCyan,
    Color::LightBlue,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightMagenta,
    Color::LightRed,
];

/// org → 渲染结果（行 + 链接索引 + 标题索引）。
pub fn render_org_doc(text: &str, width: usize) -> RenderedDoc {
    let org = Org::parse(text);
    let mut r = Renderer::new(width.max(20));
    org.traverse(&mut r);
    r.finish_doc()
}

/// 便捷入口：只取渲染行（与 md.rs 的 render_md_width 同角色，未接调用时可暂留）。
#[allow(dead_code)]
pub fn render_org_width(text: &str, width: usize) -> Vec<Line<'static>> {
    render_org_doc(text, width).lines
}

struct Renderer {
    _width: usize,
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    /// 行内样式栈（Enter 推 / Leave 弹）：fg 取栈内最后一个 Some，修饰符取并集。
    stack: Vec<Style>,
    // 标题上下文
    heading_level: Option<usize>,
    heading_line: Option<usize>,
    /// 标题行内阶段（标题文字应着层级色；正文用 BODY_FG）。
    in_title: bool,
    /// 引用块内（每行补 dim 的 "> " 前缀）。
    in_quote: bool,
    // 链接上下文
    link_depth: usize,
    link_url: String,
    link_text: String,
    link_line: usize,
    // 索引
    links: Vec<LinkEntry>,
    headings: Vec<HeadingEntry>,
}

impl Renderer {
    fn new(width: usize) -> Self {
        Self {
            _width: width,
            lines: Vec::new(),
            current: Vec::new(),
            stack: Vec::new(),
            heading_level: None,
            heading_line: None,
            in_title: false,
            in_quote: false,
            link_depth: 0,
            link_url: String::new(),
            link_text: String::new(),
            link_line: 0,
            links: Vec::new(),
            headings: Vec::new(),
        }
    }

    fn finish_doc(self) -> RenderedDoc {
        RenderedDoc {
            lines: self.lines,
            links: self.links,
            headings: self.headings,
        }
    }

    fn push_span(&mut self, text: &str, style: Style) {
        if !text.is_empty() {
            self.current.push(Span::styled(text.to_string(), style));
        }
    }

    /// 结束当前行（空行不落盘；连续空行折叠）。
    fn break_line(&mut self) {
        if !self.current.is_empty() {
            self.lines
                .push(Line::from(std::mem::take(&mut self.current)));
        }
    }

    fn heading_color(&self) -> Option<Color> {
        self.heading_level
            .map(|l| HEADING_COLORS[l.saturating_sub(1).min(5)])
    }

    /// 合并样式栈 + 基线前景（标题行内用层级色，其余 BODY_FG）。
    fn text_style(&self) -> Style {
        let mut fg: Option<Color> = None;
        let mut mods = Modifier::empty();
        for st in &self.stack {
            if let Some(c) = st.fg {
                fg = Some(c);
            }
            mods |= st.add_modifier;
        }
        let base = if self.in_title {
            self.heading_color().unwrap_or(BODY_FG)
        } else {
            BODY_FG
        };
        let mut s = Style::new().fg(fg.unwrap_or(base));
        if !mods.is_empty() {
            s = s.add_modifier(mods);
        }
        s
    }

    fn begin_heading(&mut self, h: &Headline, ctx: &mut TraversalContext) {
        let level = h.level();
        self.heading_level = Some(level);
        // 缩进（层级靠缩进区分）+ 星号×level + 空格
        self.heading_line = Some(self.lines.len());
        self.in_title = true;
        self.push_span(&"  ".repeat(level.saturating_sub(1)), Style::new());
        self.push_span(&"*".repeat(level), Style::new().add_modifier(Modifier::DIM));
        self.push_span(" ", Style::new());
        // TODO/DONE 关键词（org 语法顺序：关键词 → 优先级 → 标题 → 标签）
        if let Some(kw) = h.todo_keyword() {
            let fg = match h.todo_type() {
                Some(orgize::ast::TodoType::Todo) => TODO_FG,
                Some(orgize::ast::TodoType::Done) => DONE_FG,
                None => OTHER_TODO_FG,
            };
            self.push_span(
                &format!("{kw} "),
                Style::new().fg(fg).add_modifier(Modifier::BOLD),
            );
        }
        // 优先级 [#A]
        if let Some(p) = h.priority() {
            let fg = if p.as_ref() == "A" {
                PRIO_A_FG
            } else {
                OTHER_TODO_FG
            };
            self.push_span(
                &format!("[#{p}] "),
                Style::new().fg(fg).add_modifier(Modifier::BOLD),
            );
        }
        // orgize 默认遍历不下钻 HEADLINE_TITLE 节点（dispatch 无此 kind），
        // 必须手动迭代标题行内元素（同 MarkdownExport 做法）。
        for elem in h.title() {
            self.element(elem, ctx);
        }
        // 标签（标题文字之后）
        let tags: Vec<String> = h.tags().map(|t| t.to_string()).collect();
        if !tags.is_empty() {
            self.push_span(&format!(" :{}:", tags.join(":")), Style::new().fg(TAG_FG));
        }
        // 计划日期 SCHEDULED/DEADLINE/CLOSED
        for (get, label) in [
            (
                Headline::scheduled as fn(&Headline) -> Option<orgize::ast::Timestamp>,
                "SCHEDULED",
            ),
            (
                Headline::deadline as fn(&Headline) -> Option<orgize::ast::Timestamp>,
                "DEADLINE",
            ),
            (
                Headline::closed as fn(&Headline) -> Option<orgize::ast::Timestamp>,
                "CLOSED",
            ),
        ] {
            if let Some(ts) = get(h) {
                self.push_span(
                    &format!(" {label}:{}", ts.syntax()),
                    Style::new().fg(DATE_FG),
                );
            }
        }
        self.in_title = false;
        // 立即收束标题行：不依赖 Section 是否出现（空正文标题也能独立成行）
        self.break_line();
        // 进入时即压入索引（行号/层级/文本此刻都可得）：
        // 嵌套标题的内层 Leave 若在 end_heading 里压入，会先重置外层上下文导致丢条目，
        // 且 Leave 顺序与文档序相反。Enter 时压入 = 文档序。
        let text = plain_title(h);
        if !text.is_empty() {
            self.headings.push(HeadingEntry {
                level: level as u8,
                text,
                line: self.heading_line.unwrap_or(0),
            });
        }
    }

    fn end_heading(&mut self) {
        self.break_line(); // 兜底：吞掉标题行后残留
        self.heading_level = None;
        self.heading_line = None;
        self.in_title = false;
    }

    /// org 表格 → markdown 样式行（CJK 宽度对齐、超长截断、管道符 dim）。
    fn table_lines(&self, table: &orgize::ast::OrgTable) -> Vec<Line<'static>> {
        let pipe = Style::new().add_modifier(Modifier::DIM);
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut widths: Vec<usize> = Vec::new();
        for row in table.syntax().children() {
            if row.kind() != SyntaxKind::ORG_TABLE_STANDARD_ROW {
                continue; // 跳过分隔行
            }
            let mut cells: Vec<String> = Vec::new();
            for cell in row.children() {
                if cell.kind() != SyntaxKind::ORG_TABLE_CELL {
                    continue;
                }
                let text: String = cell
                    .children_with_tokens()
                    .filter_map(|e| {
                        e.as_token()
                            .filter(|t| t.kind() == SyntaxKind::TEXT)
                            .map(|t| t.text().trim().to_string())
                    })
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                cells.push(truncate_cell(&text.replace('|', "\\|"), 40));
            }
            for (i, cell) in cells.iter().enumerate() {
                let w = UnicodeWidthStr::width(cell.as_str());
                if i >= widths.len() {
                    widths.push(w);
                } else {
                    widths[i] = widths[i].max(w);
                }
            }
            rows.push(cells);
        }
        if rows.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (i, cell) in row.iter().enumerate() {
                let w = widths.get(i).copied().unwrap_or(0);
                spans.push(Span::styled("|", pipe));
                spans.push(Span::raw(format!(" {cell} ")));
                spans.push(Span::raw(
                    " ".repeat(w.saturating_sub(UnicodeWidthStr::width(cell.as_str()))),
                ));
            }
            spans.push(Span::styled("|", pipe));
            out.push(Line::from(spans));
            if ri == 0 {
                // markdown 表头分隔行
                let mut sep: Vec<Span<'static>> = Vec::new();
                for w in &widths {
                    sep.push(Span::styled("|", pipe));
                    sep.push(Span::styled(format!(" {} ", "-".repeat(*w)), pipe));
                }
                sep.push(Span::styled("|", pipe));
                out.push(Line::from(sep));
            }
        }
        out
    }
}

impl Traverser for Renderer {
    fn event(&mut self, event: Event, ctx: &mut TraversalContext) {
        match event {
            Event::Enter(Container::Headline(h)) => self.begin_heading(&h, ctx),
            Event::Leave(Container::Headline(_)) => self.end_heading(),
            // 标题行内元素结束于 Section 开始：收束标题行并退出 in_title
            Event::Enter(Container::Section(_)) => {
                self.in_title = false;
                self.break_line();
            }
            Event::Leave(Container::Section(_)) => {}
            Event::Leave(Container::Paragraph(_)) => self.break_line(),
            Event::LineBreak(_) => self.break_line(),

            Event::Enter(Container::QuoteBlock(_)) => self.in_quote = true,
            Event::Leave(Container::QuoteBlock(_)) => self.in_quote = false,

            Event::Enter(Container::SourceBlock(block)) => {
                self.break_line();
                let mut s = String::from("```");
                if let Some(lang) = block.language() {
                    s.push_str(&lang);
                }
                self.push_span(&s, Style::new().add_modifier(Modifier::DIM));
                self.break_line();
            }
            Event::Leave(Container::SourceBlock(_)) => {
                self.break_line();
                self.push_span("```", Style::new().add_modifier(Modifier::DIM));
                self.break_line();
            }

            Event::Enter(Container::OrgTable(table)) => {
                self.break_line();
                let lines = self.table_lines(&table);
                self.lines.extend(lines);
                ctx.skip();
            }
            Event::Leave(Container::OrgTable(_)) => {}

            Event::Enter(Container::ListItem(item)) => {
                self.break_line();
                self.push_span(&"  ".repeat(item.indent()), Style::new());
                self.push_span(&item.bullet(), Style::new().add_modifier(Modifier::DIM));
                self.push_span(" ", Style::new());
            }
            Event::Leave(Container::ListItem(_)) => self.break_line(),

            // 行内样式：修饰符（fg 缺省 → 继承标题/正文基线色）
            Event::Enter(Container::Bold(_)) => {
                self.stack.push(Style::new().add_modifier(Modifier::BOLD));
            }
            Event::Leave(Container::Bold(_)) => {
                self.stack.pop();
            }
            Event::Enter(Container::Italic(_)) => {
                self.stack.push(Style::new().add_modifier(Modifier::ITALIC));
            }
            Event::Leave(Container::Italic(_)) => {
                self.stack.pop();
            }
            Event::Enter(Container::Strike(_)) => {
                self.stack
                    .push(Style::new().add_modifier(Modifier::CROSSED_OUT));
            }
            Event::Leave(Container::Strike(_)) => {
                self.stack.pop();
            }
            Event::Enter(Container::Underline(_)) => {
                self.stack
                    .push(Style::new().add_modifier(Modifier::UNDERLINED));
            }
            Event::Leave(Container::Underline(_)) => {
                self.stack.pop();
            }
            Event::Enter(Container::Code(_)) | Event::Enter(Container::Verbatim(_)) => {
                self.stack.push(Style::new().fg(CODE_FG));
            }
            Event::Leave(Container::Code(_)) | Event::Leave(Container::Verbatim(_)) => {
                self.stack.pop();
            }
            Event::Enter(Container::Superscript(_)) | Event::Enter(Container::Subscript(_)) => {
                self.stack.push(Style::new().add_modifier(Modifier::DIM));
            }
            Event::Leave(Container::Superscript(_)) | Event::Leave(Container::Subscript(_)) => {
                self.stack.pop();
            }

            // 链接：描述文本着色，索引供 f 跳转
            Event::Enter(Container::Link(link)) => {
                self.link_depth += 1;
                self.link_url = link.path().to_string();
                self.link_text.clear();
                self.link_line = self.lines.len();
                self.stack
                    .push(Style::new().fg(LINK_FG).add_modifier(Modifier::UNDERLINED));
            }
            Event::Leave(Container::Link(link)) => {
                self.stack.pop();
                self.link_depth = self.link_depth.saturating_sub(1);
                let text = self.link_text.trim();
                if !self.link_url.is_empty() {
                    let display = if text.is_empty() {
                        self.link_url.clone()
                    } else {
                        text.to_string()
                    };
                    self.links.push(LinkEntry {
                        text: display,
                        url: self.link_url.clone(),
                        line: self.link_line,
                    });
                }
                if text.is_empty() {
                    // 无描述链接：补可见 url 文本
                    let _ = link;
                    let url = self.link_url.clone();
                    let style = self.text_style();
                    self.push_span(&url, style);
                }
            }

            Event::Text(text) => {
                let t: &str = text.as_ref();
                if self.link_depth > 0 {
                    self.link_text.push_str(t);
                }
                let style = self.text_style();
                for (i, seg) in t.split('\n').enumerate() {
                    if i > 0 {
                        self.break_line();
                    }
                    if self.in_quote && self.current.is_empty() && !seg.is_empty() {
                        self.push_span("> ", Style::new().add_modifier(Modifier::DIM));
                    }
                    if !seg.is_empty() {
                        self.push_span(seg, style);
                    }
                }
            }

            // 关键字行（#+TITLE 等）是元数据：整节点跳过，正文不出现；
            // FILETAGS（文件级标签）单独显示一行。
            Event::Enter(Container::Keyword(kw)) => {
                if kw.key().as_ref() == "FILETAGS" {
                    let v = kw.value().to_string().trim().to_string();
                    let v = v
                        .strip_prefix(':')
                        .unwrap_or(&v)
                        .strip_suffix(':')
                        .unwrap_or(&v);
                    if !v.is_empty() {
                        self.break_line();
                        self.push_span(
                            &format!("(文件标签: {v})"),
                            Style::new().add_modifier(Modifier::DIM),
                        );
                        self.break_line();
                    }
                }
                ctx.skip();
            }

            _ => {}
        }
    }
}

/// 标题纯文本：剥行内标记；链接取描述（无描述取目标）。
fn plain_title(h: &Headline) -> String {
    h.title()
        .map(elem_plain)
        .collect::<String>()
        .trim()
        .to_string()
}

/// 按显示宽度截断（CJK 宽字符按 2 列），超长补 …。
fn truncate_cell(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthStr::width(ch.to_string().as_str());
        if w + cw > max {
            out.push('…');
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

/// 行内元素 → 纯文本片段。
fn elem_plain(elem: SyntaxElement) -> String {
    match elem {
        SyntaxElement::Token(tok) => {
            if tok.kind() == SyntaxKind::TEXT {
                tok.text().to_string()
            } else {
                String::new()
            }
        }
        SyntaxElement::Node(node) => match node.kind() {
            SyntaxKind::LINK => {
                let mut parts: Vec<String> = node
                    .children_with_tokens()
                    .filter_map(|e| {
                        e.as_token()
                            .filter(|t| t.kind() == SyntaxKind::TEXT)
                            .map(|t| t.text().to_string())
                    })
                    .collect();
                parts.pop().unwrap_or_default().trim().to_string()
            }
            SyntaxKind::BOLD
            | SyntaxKind::ITALIC
            | SyntaxKind::STRIKE
            | SyntaxKind::UNDERLINE
            | SyntaxKind::CODE
            | SyntaxKind::VERBATIM
            | SyntaxKind::SUPERSCRIPT
            | SyntaxKind::SUBSCRIPT => node
                .children_with_tokens()
                .filter_map(|e| {
                    e.as_token()
                        .filter(|t| t.kind() == SyntaxKind::TEXT)
                        .map(|t| t.text().to_string())
                })
                .collect::<String>()
                .trim()
                .to_string(),
            _ => node.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> RenderedDoc {
        render_org_doc(src, 88)
    }

    #[test]
    fn heading_tree_indexed() {
        let d = doc("* 一级\nbody\n** 二级\n* 三级\n");
        assert_eq!(d.headings.len(), 3);
        assert_eq!(d.headings[0].level, 1);
        assert_eq!(d.headings[0].text, "一级");
        assert_eq!(d.headings[0].line, 0);
        assert_eq!(d.headings[2].line, 3);
    }

    #[test]
    fn bold_italic_spans_styled() {
        let d = doc("* 标题 *粗* /斜/\n");
        let l0 = d.lines[0].to_string();
        assert!(l0.contains("粗") && l0.contains("斜"));
    }

    #[test]
    fn link_index_populated() {
        let d = doc("* t\n[[https://orgmode.org][Org]]\n");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].url, "https://orgmode.org");
        assert_eq!(d.links[0].text, "Org");
    }

    #[test]
    fn levels_distinguished_by_indent() {
        let d = doc("* 一\n** 二\n*** 三\n");
        assert!(d.lines[0].to_string().starts_with("* 一"));
        assert!(
            d.lines[1].to_string().starts_with("  ** 二"),
            "level2 应缩进 2 格"
        );
        assert!(
            d.lines[2].to_string().starts_with("    *** 三"),
            "level3 应缩进 4 格"
        );
    }

    #[test]
    fn todo_priority_tags_dates_styled() {
        let d = doc("* TODO [#A] 买牛奶 :errand:\n  DEADLINE: <2026-09-25 Fri>\n");
        let l0 = d.lines[0].to_string();
        assert!(l0.contains("TODO"));
        assert!(l0.contains("[#A]"));
        assert!(l0.contains(":errand:"));
        assert!(l0.contains("DEADLINE:"));
    }

    #[test]
    fn filetags_line() {
        let d = doc("#+FILETAGS: :cml:\n* a\n");
        assert!(d.lines[0].to_string().contains("文件标签"));
    }

    #[test]
    fn org_table_aligned_cjk() {
        let d = doc("* t\n| 物品 | 数量 |\n|------+------|\n| 苹果 | 3    |\n");
        let joined = d
            .lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        // 数量列宽 4（CJK 宽字符×2）："3" 右补 3 格到列宽，格式尾随 1 空格
        assert!(joined.contains("| 苹果 | 3    |"));
    }

    #[test]
    fn quote_and_src_blocks() {
        let d = doc(
            "* t\n#+BEGIN_QUOTE\n引文\n#+END_QUOTE\n#+BEGIN_SRC rust\nfn main() {}\n#+END_SRC\n",
        );
        let joined = d
            .lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("> 引文"));
        assert!(joined.contains("```rust"));
        assert!(joined.contains("fn main() {}"));
    }
}
