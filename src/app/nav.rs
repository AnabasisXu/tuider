//! mdterm-style navigation overlays:
//! - `f` link picker (open http / local / #anchor) — type to filter
//! - `o` TOC / outline (jump heading, incl. fetched article) — type to filter
//! - `Alt+f` consult: bottom fuzzy search with preview → jump line

use std::path::{Path, PathBuf};
use std::process::Command;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::plugin::{HeadingEntry, LinkEntry};
use crate::theme::Theme;

use super::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Links,
    Toc,
    Consult,
}

#[derive(Debug, Default)]
pub struct NavState {
    pub overlay: Option<Overlay>,
    pub selected: usize,
    pub scroll: usize,
    pub query: String,
    /// Filtered indices into links/headings/consult hits.
    pub filtered: Vec<usize>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ConsultHit {
    pub line: usize,
    pub preview: String,
}

impl App {
    pub fn nav_open(&self) -> bool {
        self.nav.overlay.is_some()
    }

    pub(crate) fn open_links(&mut self) {
        if self.links.is_empty() {
            self.status = "no links in document".into();
            return;
        }
        self.nav = NavState {
            overlay: Some(Overlay::Links),
            selected: 0,
            scroll: 0,
            query: String::new(),
            filtered: (0..self.links.len()).collect(),
        };
        self.status = "links — type 过滤 · ↑↓ Enter · Esc".into();
    }

    pub(crate) fn open_toc(&mut self) {
        // lazy load so HN/list browse can o after ↑↓ without prior Enter
        if self.headings.is_empty() {
            if let Some(di) = self.selected_doc_index() {
                if self.loaded_doc != Some(di) {
                    self.load_selected();
                }
            }
        }
        if self.headings.is_empty() {
            self.status = "no outline / 义项 in document".into();
            return;
        }
        let cur = self.scroll as usize;
        let mut sel = 0;
        for (i, h) in self.headings.iter().enumerate() {
            if h.line <= cur {
                sel = i;
            }
        }
        self.nav = NavState {
            overlay: Some(Overlay::Toc),
            selected: 0,
            scroll: 0,
            query: String::new(),
            filtered: (0..self.headings.len()).collect(),
        };
        if let Some(pos) = self.nav.filtered.iter().position(|&i| i == sel) {
            self.nav.selected = pos;
        }
        self.status = "o 大纲/义项 — 打字过滤 · ↑↓ Enter 跳转 · Esc".into();
    }


    pub(crate) fn open_consult(&mut self) {
        if self.body.is_empty() {
            self.status = "empty document".into();
            return;
        }
        self.nav = NavState {
            overlay: Some(Overlay::Consult),
            selected: 0,
            scroll: 0,
            query: String::new(),
            filtered: Vec::new(),
        };
        self.refilter_consult();
        self.status = "consult — type to filter · ↑↓ · Enter jump · Esc".into();
    }

    pub(crate) fn close_nav(&mut self) {
        self.nav.overlay = None;
        self.nav.query.clear();
        self.nav.filtered.clear();
        self.status = "nav closed".into();
    }

    fn refilter_consult(&mut self) {
        let q = self.nav.query.to_lowercase();
        let mut hits = Vec::new();
        for (i, line) in self.body.iter().enumerate() {
            let t = Self::line_plain(line);
            if q.is_empty() || t.to_lowercase().contains(&q) {
                hits.push(i);
            }
            if hits.len() >= 500 {
                break;
            }
        }
        self.nav.filtered = hits;
        if self.nav.selected >= self.nav.filtered.len() {
            self.nav.selected = self.nav.filtered.len().saturating_sub(1);
        }
        self.nav.scroll = 0;
    }

    /// Shared type-to-filter for list overlays (Toc / Links). Ready to reuse for more `o`-style jumps.
    fn refilter_nav_list(&mut self) {
        let Some(mode) = self.nav.overlay else {
            return;
        };
        let q = self.nav.query.to_lowercase();
        match mode {
            Overlay::Toc => {
                self.nav.filtered = self
                    .headings
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| q.is_empty() || h.text.to_lowercase().contains(&q))
                    .map(|(i, _)| i)
                    .collect();
            }
            Overlay::Links => {
                self.nav.filtered = self
                    .links
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| {
                        q.is_empty()
                            || l.text.to_lowercase().contains(&q)
                            || l.url.to_lowercase().contains(&q)
                    })
                    .map(|(i, _)| i)
                    .collect();
            }
            Overlay::Consult => {
                self.refilter_consult();
                return;
            }
        }
        if self.nav.selected >= self.nav.filtered.len() {
            self.nav.selected = self.nav.filtered.len().saturating_sub(1);
        }
        self.nav.scroll = 0;
    }

    pub(crate) fn handle_nav_key(&mut self, key: KeyEvent) -> bool {
        let Some(mode) = self.nav.overlay else {
            return false;
        };
        let list_filter = matches!(mode, Overlay::Toc | Overlay::Links | Overlay::Consult);
        match key.code {
            KeyCode::Esc => {
                self.close_nav();
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.nav.selected = self.nav.selected.saturating_sub(1);
                return true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.nav.filtered.is_empty() {
                    self.nav.selected =
                        (self.nav.selected + 1).min(self.nav.filtered.len() - 1);
                }
                return true;
            }
            KeyCode::Enter => {
                self.nav_activate(mode);
                return true;
            }
            KeyCode::Backspace if list_filter => {
                self.nav.query.pop();
                self.refilter_nav_list();
                return true;
            }
            KeyCode::Char(c)
                if list_filter
                    && (key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT) =>
            {
                if !c.is_control() {
                    self.nav.query.push(c);
                    self.refilter_nav_list();
                }
                return true;
            }
            _ => {}
        }
        true // consume while open
    }

    fn nav_activate(&mut self, mode: Overlay) {
        let Some(&idx) = self.nav.filtered.get(self.nav.selected) else {
            self.status = "nothing selected".into();
            return;
        };
        match mode {
            Overlay::Links => {
                if let Some(link) = self.links.get(idx).cloned() {
                    self.dispatch_link(&link);
                }
            }
            Overlay::Toc => {
                if let Some(h) = self.headings.get(idx).cloned() {
                    self.ensure_line_visible(h.line);
                    self.scroll = h.line as u16;
                    self.status = format!("jumped to: {}", h.text);
                }
            }
            Overlay::Consult => {
                self.ensure_line_visible(idx);
                self.scroll = idx as u16;
                self.status = format!("jumped to line {}", idx + 1);
            }
        }
        self.nav.overlay = None;
        self.nav.query.clear();
    }

    pub(crate) fn dispatch_link(&mut self, link: &LinkEntry) {
        let url = link.url.as_str();
        if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
            match open_external(url) {
                Ok(()) => self.status = format!("opened: {url}"),
                Err(e) => self.status = format!("open failed: {e}"),
            }
            return;
        }
        if let Some(anchor) = url.strip_prefix('#') {
            if let Some(h) = self
                .headings
                .iter()
                .find(|h| heading_slug(&h.text) == anchor)
            {
                self.scroll = h.line as u16;
                self.status = format!("jumped to #{anchor}");
            } else {
                self.status = format!("heading not found: #{anchor}");
            }
            return;
        }
        // local relative file
        let base = self
            .current_path()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let (file_part, anchor) = match url.split_once('#') {
            Some((f, a)) => (f, Some(a)),
            None => (url, None),
        };
        if file_part.is_empty() {
            self.status = format!("bad link: {url}");
            return;
        }
        let resolved = base.join(file_part);
        if resolved.is_file() {
            // try switch within source entries
            if let Some(i) = self.find_entry_for_path(&resolved) {
                // select that entry
                if let Some(pos) = self.filtered.iter().position(|&di| di == i) {
                    self.list_sel = pos;
                } else {
                    // not in filter — clear filter
                    self.filter.clear();
                    self.refilter();
                    if let Some(pos) = self.filtered.iter().position(|&di| di == i) {
                        self.list_sel = pos;
                    }
                }
                self.load_selected();
                if let Some(a) = anchor {
                    if let Some(h) = self
                        .headings
                        .iter()
                        .find(|h| heading_slug(&h.text) == a)
                    {
                        self.scroll = h.line as u16;
                    }
                }
                self.status = format!("opened: {}", resolved.display());
            } else {
                // open externally if not in tree
                match open_external(&resolved.to_string_lossy()) {
                    Ok(()) => self.status = format!("opened external: {}", resolved.display()),
                    Err(e) => self.status = format!("open failed: {e}"),
                }
            }
        } else if resolved.is_dir() {
            match open_external(&resolved.to_string_lossy()) {
                Ok(()) => self.status = format!("opened dir: {}", resolved.display()),
                Err(e) => self.status = format!("open dir failed: {e}"),
            }
        } else {
            self.status = format!("not found: {url}");
        }
    }

    pub(crate) fn open_current_dir(&mut self) {
        let Some(path) = self.current_path() else {
            self.status = "no local path for current entry".into();
            return;
        };
        let dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        match open_external(&dir.to_string_lossy()) {
            Ok(()) => self.status = format!("opened dir: {}", dir.display()),
            Err(e) => self.status = format!("open dir failed: {e}"),
        }
    }

    fn current_path(&self) -> Option<PathBuf> {
        let di = self.selected_doc_index()?;
        self.source.entry_path(di)
    }

    fn find_entry_for_path(&self, path: &Path) -> Option<usize> {
        let canon = path.canonicalize().ok();
        for i in 0..self.source.entries().len() {
            let Some(p) = self.source.entry_path(i) else {
                continue;
            };
            if p == path {
                return Some(i);
            }
            if let (Some(c), Ok(pc)) = (&canon, p.canonicalize()) {
                if *c == pc {
                    return Some(i);
                }
            }
        }
        None
    }
}

fn heading_slug(text: &str) -> String {
    let mut s = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            s.extend(c.to_lowercase());
        } else if c.is_whitespace() || c == '-' || c == '_' {
            if !s.ends_with('-') {
                s.push('-');
            }
        }
    }
    while s.ends_with('-') {
        s.pop();
    }
    s
}

fn open_external(target: &str) -> Result<(), String> {
    // ponytail: xdg-open / open / cmd — no extra crate
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["open", target]]
    } else if cfg!(target_os = "windows") {
        &[&["cmd", "/C", "start", "", target]]
    } else {
        &[&["xdg-open", target], &["gio", "open", target]]
    };
    let mut last = String::from("no opener");
    for c in cmds {
        match Command::new(c[0]).args(&c[1..]).spawn() {
            Ok(_) => return Ok(()),
            Err(e) => last = format!("{}: {e}", c[0]),
        }
    }
    Err(last)
}

pub fn draw_nav_overlay(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let Some(mode) = app.nav.overlay else {
        return;
    };
    match mode {
        Overlay::Links => draw_list_overlay(
            frame,
            area,
            theme,
            " Links (f) ",
            &app.nav,
            &app.links,
            |l: &LinkEntry| format!("{}  →  {}", trunc(&l.text, 40), trunc(&l.url, 50)),
        ),
        Overlay::Toc => draw_list_overlay(
            frame,
            area,
            theme,
            " Outline (o) ",
            &app.nav,
            &app.headings,
            |h: &HeadingEntry| {
                // spaces for depth; never show markdown # (strip if present)
                let pad = "  ".repeat(h.level.saturating_sub(1) as usize);
                let t = h.text.trim_start_matches('#').trim();
                format!("{pad}{t}")
            },


        ),
        Overlay::Consult => draw_consult(frame, area, app, theme),
    }
}

fn trunc(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        chars[..n.saturating_sub(1)].iter().collect::<String>() + "…"
    }
}

fn draw_list_overlay<T, F>(
    frame: &mut Frame,
    area: Rect,
    theme: Theme,
    title: &str,
    nav: &NavState,
    items: &[T],
    fmt: F,
) where
    F: Fn(&T) -> String,
{
    // +1 row for search box
    let h = (nav.filtered.len() as u16 + 3).clamp(6, area.height.saturating_sub(4).max(6));
    let w = (area.width * 4 / 5).clamp(40, area.width.saturating_sub(4).max(40));
    let popup = centered(w, h, area);
    frame.render_widget(Clear, popup);
    let title = if nav.query.is_empty() {
        format!("{title}  /filter")
    } else {
        format!("{title}  /{}", nav.query)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
    let filter_line = Line::from(vec![
        Span::styled("/", Style::default().fg(theme.vim_prompt())),
        Span::styled(
            if nav.query.is_empty() {
                "type to filter…".into()
            } else {
                nav.query.clone()
            },
            Style::default().fg(if nav.query.is_empty() {
                theme.muted()
            } else {
                theme.search_text()
            }),
        ),
        Span::styled(
            format!("  {}/{}", nav.filtered.len(), items.len()),
            Style::default().fg(theme.muted()),
        ),
    ]);
    frame.render_widget(Paragraph::new(filter_line), chunks[0]);

    let list_area = chunks[1];
    let visible = list_area.height as usize;
    let start = if nav.selected >= visible {
        nav.selected + 1 - visible
    } else {
        0
    };
    let end = (start + visible).min(nav.filtered.len());
    let list_items: Vec<ListItem> = nav.filtered[start..end]
        .iter()
        .enumerate()
        .map(|(row, &idx)| {
            let sel = start + row == nav.selected;
            let text = items.get(idx).map(|x| fmt(x)).unwrap_or_default();
            let style = if sel {
                Style::default()
                    .fg(theme.status_focus_fg())
                    .bg(theme.search_text())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.list_text())
            };
            ListItem::new(Line::from(Span::styled(text, style)))
        })
        .collect();
    frame.render_widget(List::new(list_items), list_area);
}

fn draw_consult(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    // bottom panel: results list + input, preview of selected line context
    let panel_h = (area.height / 2).clamp(10, 18);
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(panel_h),
    ])
    .split(area);
    let panel = chunks[1];
    frame.render_widget(Clear, panel);
    let block = Block::default()
        .title(" consult (Alt+f) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(panel);
    frame.render_widget(block, panel);

    let v = Layout::vertical([
        Constraint::Length(1), // input
        Constraint::Percentage(45), // hits
        Constraint::Min(3), // preview
    ])
    .split(inner);

    let q = format!("> {}", app.nav.query);
    frame.render_widget(
        Paragraph::new(q).style(Style::default().fg(theme.search_text())),
        v[0],
    );

    let hits = &app.nav.filtered;
    let visible = v[1].height as usize;
    let start = if app.nav.selected >= visible {
        app.nav.selected + 1 - visible
    } else {
        0
    };
    let end = (start + visible).min(hits.len());
    let items: Vec<ListItem> = hits[start..end]
        .iter()
        .enumerate()
        .map(|(row, &line)| {
            let sel = start + row == app.nav.selected;
            let plain = app
                .body
                .get(line)
                .map(App::line_plain)
                .unwrap_or_default();
            let label = format!("{:>4} │ {}", line + 1, trunc(&plain, 80));
            let style = if sel {
                Style::default()
                    .fg(theme.status_focus_fg())
                    .bg(theme.search_text())
            } else {
                Style::default().fg(theme.list_text())
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::TOP)
                .title(format!(" {} hits ", hits.len())),
        ),
        v[1],
    );

    // preview: ±2 lines around selected
    let preview_line = hits.get(app.nav.selected).copied();
    let mut prev_lines = Vec::new();
    if let Some(li) = preview_line {
        let from = li.saturating_sub(2);
        let to = (li + 3).min(app.body.len());
        for i in from..to {
            let mark = if i == li { "▶ " } else { "  " };
            let t = app.body.get(i).map(App::line_plain).unwrap_or_default();
            let style = if i == li {
                Style::default()
                    .fg(theme.accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted())
            };
            prev_lines.push(Line::from(Span::styled(format!("{mark}{t}"), style)));
        }
    } else {
        prev_lines.push(Line::from(Span::styled(
            "  (no match)",
            Style::default().fg(theme.muted()),
        )));
    }
    frame.render_widget(
        Paragraph::new(prev_lines)
            .block(Block::default().borders(Borders::TOP).title(" preview "))
            .wrap(Wrap { trim: false }),
        v[2],
    );
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
