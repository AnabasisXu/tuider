//! Layout / draw — ported principles from mdx-tui ui.rs.

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, SearchLayout};
use crate::theme::Theme;

const MIN_COLS: u16 = 40;
const MIN_ROWS: u16 = 12;
const SIDEBAR_BREAKPOINT: u16 = 72;
const SIDEBAR_WIDTH_WIDE: u16 = 30;
const SIDEBAR_WIDTH_NARROW: u16 = 18;
const TOP_HEADWORD_ROWS: u16 = 5;
const TOP_HEADWORD_ROWS_COMPACT: u16 = 3;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = app.theme();

    if area.width < MIN_COLS || area.height < MIN_ROWS {
        let msg = format!(
            "  屏幕过小 / Terminal too small  \n  Need ≥{}×{}, got {}×{}  \n  请放大终端  ",
            MIN_COLS, MIN_ROWS, area.width, area.height
        );
        let p = Paragraph::new(msg)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .alignment(ratatui::layout::Alignment::Center);
        let vertical = Layout::vertical([Constraint::Length(5)]).flex(Flex::Center);
        let [center] = vertical.areas(area);
        frame.render_widget(p, center);
        return;
    }

    frame.render_widget(Paragraph::new("").style(Style::reset()), area);

    let compact_w = area.width < SIDEBAR_BREAKPOINT;
    let compact_h = area.height < 24;

    #[cfg(feature = "ai")]
    if app.ai_open() {
        if app.ai().maximized {
            // full AI — no reader chrome
            app.set_list_area(None);
            app.set_content_area(None);
            app.ai_mut().draw(frame, area, theme);
        } else if app.show_sidebar() && !compact_w {
            // sidebar | content | AI — keep headword list visible
            let sidebar_width = if area.width < 90 {
                SIDEBAR_WIDTH_NARROW
            } else {
                SIDEBAR_WIDTH_WIDE
            };
            let main = Layout::horizontal([
                Constraint::Length(sidebar_width),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area);
            let left = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(main[0]);
            let right = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(main[2]);
            app.set_list_area(Some(left[1]));
            draw_input(frame, left[0], app);
            draw_list(frame, left[1], app);
            let content_col =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(right[0]);
            app.set_content_area(Some(content_col[0]));
            draw_content(frame, content_col[0], app);
            draw_status(frame, content_col[1], app);
            app.ai_mut().draw(frame, right[2], theme);
        } else if compact_w {
            // narrow: content over AI (no room for sidebar)
            app.set_list_area(None);
            let v = Layout::vertical([
                Constraint::Percentage(45),
                Constraint::Length(1),
                Constraint::Percentage(55),
            ])
            .split(area);
            let content_col =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(v[0]);
            app.set_content_area(Some(content_col[0]));
            draw_content(frame, content_col[0], app);
            draw_status(frame, content_col[1], app);
            app.ai_mut().draw(frame, v[2], theme);
        } else {
            // wide, no sidebar: content | AI
            app.set_list_area(None);
            let h = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(area);
            let content_col =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(h[0]);
            app.set_content_area(Some(content_col[0]));
            draw_content(frame, content_col[0], app);
            draw_status(frame, content_col[1], app);
            app.ai_mut().draw(frame, h[2], theme);
        }
        if app.show_help() {
            draw_help_overlay(frame, area, app, theme);
        }
        return;
    }

    if app.vim_search_mode() {
        app.set_list_area(None);
        let layout = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(area);
        app.set_content_area(Some(layout[0]));
        draw_content(frame, layout[0], app);
        draw_status(frame, layout[1], app);
        draw_vim_search(frame, layout[2], app);
    } else if app.show_sidebar() {
        let layout_mode = if compact_w {
            SearchLayout::Top
        } else {
            app.search_layout()
        };
        match layout_mode {
            SearchLayout::Left => {
                let sidebar_width = if area.width < 90 {
                    SIDEBAR_WIDTH_NARROW
                } else {
                    SIDEBAR_WIDTH_WIDE
                };
                let h = Layout::horizontal([
                    Constraint::Length(sidebar_width),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ])
                .split(area);
                let left =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(h[0]);
                app.set_list_area(Some(left[1]));
                app.set_content_area(Some(h[2]));
                draw_input(frame, left[0], app);
                draw_list(frame, left[1], app);
                draw_right(frame, h[2], app);
            }
            SearchLayout::Top => {
                let hw_rows = if compact_h {
                    TOP_HEADWORD_ROWS_COMPACT
                } else {
                    TOP_HEADWORD_ROWS
                };
                let search_h: u16 = 3;
                let mut hw_height = hw_rows + 2;
                let max_hw = area.height.saturating_sub(search_h).saturating_sub(3);
                hw_height = hw_height.min(max_hw.max(3));
                let v = Layout::vertical([
                    Constraint::Length(search_h),
                    Constraint::Length(hw_height),
                    Constraint::Min(1),
                ])
                .split(area);
                app.set_list_area(Some(v[1]));
                app.set_content_area(Some(v[2]));
                draw_input(frame, v[0], app);
                draw_list(frame, v[1], app);
                draw_right(frame, v[2], app);
            }
        }
    } else {
        app.set_list_area(None);
        // reuse draw_right so Ctrl+B works with sidebar off
        draw_right(frame, area, app);
        // content_area for page steps — bottom status row reserved when no panel
        if !app.dict_panel_open() {
            let layout =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
            app.set_content_area(Some(layout[0]));
        } else {
            let panel_h = (app.dict_panel_names().len() as u16)
                .saturating_add(2)
                .clamp(3, 8)
                .min(area.height.saturating_sub(2));
            let layout = Layout::vertical([
                Constraint::Length(panel_h),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);
            app.set_content_area(Some(layout[1]));
        }
    }

    if app.nav_open() {
        crate::app::nav::draw_nav_overlay(frame, area, app, theme);
    }

    if app.show_help() {
        draw_help_overlay(frame, area, app, theme);
    }
}

fn draw_right(frame: &mut Frame, area: Rect, app: &App) {
    if app.dict_panel_open() {
        // ponytail: Clear full area avoids ghost after close (UI-02)
        frame.render_widget(Clear, area);
        let panel_h = (app.dict_panel_names().len() as u16)
            .saturating_add(2)
            .clamp(3, 8)
            .min(area.height.saturating_sub(2));
        let layout = Layout::vertical([
            Constraint::Length(panel_h),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        draw_dict_panel(frame, layout[0], app);
        draw_content(frame, layout[1], app);
        draw_status(frame, layout[2], app);
    } else {
        let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
        draw_content(frame, layout[0], app);
        draw_status(frame, layout[1], app);
    }
}

fn draw_dict_panel(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let names = app.dict_panel_names();
    let sel = app.dict_panel_sel();
    let inner_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == sel {
                theme.list_selected()
            } else {
                Style::default().fg(theme.list_text())
            };
            let mut text = format!(" {name} ");
            if i == sel && inner_width > 0 {
                let w = text.chars().count();
                if w < inner_width {
                    text.push_str(&" ".repeat(inner_width - w));
                }
            }
            ListItem::new(Line::from(Span::styled(text, style)))
        })
        .collect();
    let list = List::new(items).block(
        Block::default()
            .title(" Dictionaries · Enter select · Esc close ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border())),
    );
    frame.render_widget(list, area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let block = Block::default()
        .title(" Search ")
        .title_style(Style::default().fg(Color::Rgb(170, 185, 200)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input = app.filter();
    let max_w = inner.width as usize;
    let visible = if max_w == 0 {
        String::new()
    } else if input.chars().count() > max_w {
        input
            .chars()
            .skip(input.chars().count().saturating_sub(max_w))
            .collect()
    } else {
        input.to_string()
    };
    frame.render_widget(
        Paragraph::new(visible).style(Style::default().fg(theme.search_text())),
        inner,
    );
    if max_w > 0 {
        let col = input.chars().count().min(max_w.saturating_sub(1)) as u16;
        // if scrolled, cursor at end of viewport
        let col = if input.chars().count() > max_w {
            (max_w.saturating_sub(1)) as u16
        } else {
            col
        };
        frame.set_cursor_position((inner.x + col, inner.y));
    }
}

fn draw_list(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Paragraph::new("").style(Style::reset()), area);
    let theme = app.theme();
    let names = app.visible_names();
    let sel = app.visible_sel();
    let inner_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == sel {
                theme.headword_selected()
            } else {
                Style::default().fg(theme.headword_text())
            };
            let mut text = name.clone();
            if i == sel && inner_width > 0 {
                let w = text.chars().count();
                if w < inner_width {
                    text.push_str(&" ".repeat(inner_width - w));
                }
            }
            ListItem::new(Line::from(Span::styled(text, style)))
        })
        .collect();

    let title = format!(" {} ({}) ", app.source_title(), app.filtered_len());
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border())),
    );
    frame.render_widget(list, area);
}

fn draw_content(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let title = app.content_title();
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new("").style(Style::reset()), inner);

    let lines = app.body_lines();
    if lines.is_empty() {
        let key = |s: &str| Span::styled(s.to_string(), Style::default().fg(theme.key_label()));
        let desc = |s: &str| Span::styled(s.to_string(), Style::default().fg(theme.muted()));
        let section = |s: &str| {
            Span::styled(
                s.to_string(),
                Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD),
            )
        };
        let mut help = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  TUIDER ",
                    Style::default().fg(theme.title()).add_modifier(Modifier::BOLD),
                ),
                Span::styled("— 终端阅读器", Style::default().fg(theme.muted())),
            ]),
            Line::from(""),
            Line::from(section("  基本操作")),
            Line::from(vec![key("    侧栏开启  "), desc("焦点=搜索框；字母只过滤")]),
            Line::from(vec![
                key("    Ctrl+F   "),
                desc(if app.can_plugin_action() {
                    "关侧栏后 a/o/f 等命令可用"
                } else {
                    "关侧栏后 o/f 等命令可用"
                }),
            ]),
            Line::from(vec![key("    ↑ / ↓    "), desc("浏览列表（预取正文）")]),
            Line::from(vec![key("    Enter    "), desc("打开 / 重载选中")]),
        ];
        if app.can_plugin_action() {
            help.push(Line::from(vec![
                key("    a        "),
                desc("抓取外链全文（侧栏关，HN）"),
            ]));
        }
        help.extend([
            Line::from(vec![key("    [ / ]    "), desc("上一/下一主节（侧栏关）")]),
            Line::from(""),
            Line::from(section("  快捷键")),
            Line::from(vec![key("    Ctrl+S   "), desc("搜索布局：左侧 / 顶部")]),
            Line::from(vec![key("    /        "), desc("正文 vim 搜索（侧栏关）")]),
            Line::from(vec![key("    n / N    "), desc("下一/上一匹配")]),
            Line::from(vec![key("    v / V    "), desc("光标/行 visual · vv 选字 · y 复制")]),
            Line::from(vec![key("    f / o    "), desc("链接 / 大纲（侧栏关）")]),
            Line::from(vec![key("    Alt+f    "), desc("consult 空格多词过滤跳转")]),
            Line::from(vec![key("    O        "), desc("打开目录（侧栏关）")]),
            Line::from(vec![key("    ?        "), desc("帮助")]),
            Line::from(""),
            Line::from(section("  滚动")),
            Line::from(vec![key("    ↑↓ / Pg  "), desc("焦点区")]),
            Line::from(vec![key("    Alt+↑↓   "), desc("另一区（通常正文）")]),
        ]);
        frame.render_widget(Paragraph::new(help), inner);
        return;
    }

    let max_scroll = lines.len().saturating_sub(inner.height as usize) as u16;
    let scroll = app.scroll().min(max_scroll);
    let q = app.vim_query();
    let line_vis = app.visual_line_range();
    let char_vis = app.visual_char_sel();
    let cursor_cell = app.visual_cursor_cell();
    let current = app.current_match();
    let rendered: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let mut line = if !q.is_empty() {
                highlight_line(line, q, theme, current.filter(|h| h.line == i))
            } else {
                line.clone()
            };
            if let Some((a, b)) = line_vis {
                if i >= a && i <= b {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    line = Line::from(Span::styled(
                        text,
                        Style::default()
                            .fg(theme.status_focus_fg())
                            .bg(theme.search_text()),
                    ));
                }
            } else if let Some(sel) = char_vis {
                if i >= sel.a_line && i <= sel.b_line {
                    line = paint_char_visual(line, i, &sel, theme);
                }
            } else if let Some((cl, cc)) = cursor_cell {
                if i == cl {
                    line = paint_cursor_cell(line, cc, theme);
                }
            }
            line
        })
        .collect();
    // body lines are pre-wrapped to content_width (loader); no Paragraph wrap
    frame.render_widget(
        Paragraph::new(rendered).scroll((scroll, 0)),
        inner,
    );
}

fn paint_cursor_cell(line: Line<'static>, col: usize, theme: Theme) -> Line<'static> {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let caret = Style::default()
        .fg(theme.status_focus_fg())
        .bg(theme.search_text());
    let plain = Style::default().fg(theme.list_text());
    if n == 0 || col >= n {
        // empty line or after last char: reverse space caret
        let mut spans = Vec::new();
        if n > 0 {
            spans.push(Span::styled(chars.iter().collect::<String>(), plain));
        }
        spans.push(Span::styled(" ", caret));
        return Line::from(spans);
    }
    let mut spans = Vec::new();
    if col > 0 {
        spans.push(Span::styled(
            chars[..col].iter().collect::<String>(),
            plain,
        ));
    }
    spans.push(Span::styled(
        chars[col..=col].iter().collect::<String>(),
        caret,
    ));
    if col + 1 < n {
        spans.push(Span::styled(
            chars[col + 1..].iter().collect::<String>(),
            plain,
        ));
    }
    Line::from(spans)
}

fn paint_char_visual(
    line: Line<'static>,
    line_idx: usize,
    sel: &crate::app::VisualSel,
    theme: Theme,
) -> Line<'static> {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let (start, end) = if line_idx == sel.a_line && line_idx == sel.b_line {
        (sel.a_col.min(n), sel.b_col.min(n).max(sel.a_col.min(n)))
    } else if line_idx == sel.a_line {
        (sel.a_col.min(n), n)
    } else if line_idx == sel.b_line {
        (0, sel.b_col.min(n))
    } else {
        (0, n)
    };
    if start >= end {
        return line;
    }
    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled(
            chars[..start].iter().collect::<String>(),
            Style::default().fg(theme.list_text()),
        ));
    }
    spans.push(Span::styled(
        chars[start..end].iter().collect::<String>(),
        Style::default()
            .fg(theme.status_focus_fg())
            .bg(theme.search_text()),
    ));
    if end < n {
        spans.push(Span::styled(
            chars[end..].iter().collect::<String>(),
            Style::default().fg(theme.list_text()),
        ));
    }
    Line::from(spans)
}

fn highlight_line(
    line: &Line<'static>,
    q: &str,
    theme: Theme,
    current: Option<crate::app::MatchHit>,
) -> Line<'static> {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let tokens: Vec<Vec<char>> = q
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase().chars().collect())
        .collect();
    if tokens.is_empty() {
        return line.clone();
    }
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let n = chars.len();
    let mut mark = vec![false; n];
    for tok in &tokens {
        let m = tok.len();
        if m == 0 || m > n {
            continue;
        }
        let mut i = 0;
        while i + m <= n {
            if lower[i..i + m] == tok[..] {
                for b in &mut mark[i..i + m] {
                    *b = true;
                }
                i += m;
            } else {
                i += 1;
            }
        }
    }
    if !mark.iter().any(|&b| b) {
        return line.clone();
    }
    let hit = Style::default()
        .fg(theme.status_focus_fg())
        .bg(theme.search_text())
        .add_modifier(Modifier::BOLD);
    let cur = Style::default()
        .fg(theme.status_focus_fg())
        .bg(Color::Rgb(250, 179, 135)) // peach current
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let normal = Style::default().fg(theme.list_text());
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut i = 0;
    while i < n {
        let on = mark[i];
        let start = i;
        i += 1;
        while i < n && mark[i] == on {
            i += 1;
        }
        let piece: String = chars[start..i].iter().collect();
        let style = if !on {
            normal
        } else if current.is_some_and(|h| h.start < i && h.end > start) {
            cur
        } else {
            hit
        };
        spans.push(Span::styled(piece, style));
    }
    Line::from(spans)
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let focus = app.focus_label();
    let status = Line::from(vec![
        Span::styled(
            format!(" {focus} "),
            Style::default()
                .fg(theme.status_focus_fg())
                .bg(theme.status_focus_bg())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " {}/{}  {}  ? help  C-q quit ",
                app.list_sel().saturating_add(1).min(app.filtered_len().max(1)),
                app.filtered_len(),
                app.status()
            ),
            Style::default().fg(theme.muted()),
        ),
    ]);
    frame.render_widget(Paragraph::new(status), area);
}

fn draw_vim_search(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme();
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let line = Line::from(vec![
        Span::styled("/", Style::default().fg(theme.vim_prompt())),
        Span::styled(app.vim_input(), Style::default().fg(theme.search_text())),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}

fn draw_help_overlay(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let mut entries: Vec<(&str, &str)> = if app.vim_search_mode() {
        vec![
            ("Enter", "Confirm search"),
            ("Esc", "Cancel"),
            ("n/N", "Next / prev match"),
            ("?", "Close help"),
        ]
    } else {
        vec![
            ("Type", "Filter sidebar (live)"),
            ("↑/↓", "List (sidebar on)"),
            ("Ctrl+F", "Toggle sidebar (not search layout)"),
            ("Ctrl+S", "Search left/top layout"),
            ("Ctrl+B", "Dict picker panel"),
            ("Ctrl+Y", "Copy definition (OSC52)"),
            ("Ctrl+U", "Clear filter, keep result"),
            ("Ctrl+W", "Delete filter word"),
            ("Esc", "Clear filter / result / panel"),
            ("Tab", "Cycle dict / source layer"),
            ("/", "In-content search"),
            ("[/]", "Prev/next major section"),
            ("o", "Outline / filter headings"),
            ("v/V", "Cursor / line visual (vv=char)"),
            ("y", "Yank selection (OSC 52)"),
            ("Alt+L", "AI overlay (if built)"),
            ("Alt+Shift+L", "AI maximize"),
            ("Ctrl+Q", "Quit"),
            ("?", "Close help"),
        ]
    };
    if !app.vim_search_mode() && app.can_plugin_action() {
        if let Some(i) = entries.iter().position(|(k, _)| *k == "/") {
            entries.insert(i + 1, ("a", "Fetch full article (HN)"));
        }
    }
    let help_height = (entries.len() as u16) + 4;
    let help_width: u16 = 44;
    let popup = centered_rect(help_width, help_height, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(if app.vim_search_mode() {
            " Vim Search "
        } else {
            " Tuider "
        })
        .title_style(
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()))
        .style(Style::default().bg(theme.help_bg()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines: Vec<Line> = entries
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:<14}"),
                    Style::default().fg(theme.key_label()),
                ),
                Span::styled((*desc).to_string(), Style::default().fg(theme.muted())),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center);
    let [v] = vertical.areas(area);
    let [h] = horizontal.areas(v);
    h
}
