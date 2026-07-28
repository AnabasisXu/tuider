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
    /// Alt+Shift+f: search across all source entries (files / feed items).
    Corpus,
}

#[derive(Debug, Default)]
pub struct NavState {
    pub overlay: Option<Overlay>,
    pub selected: usize,
    pub scroll: usize,
    pub query: String,
    /// Filtered indices into links/headings/consult hits/corpus hits.
    pub filtered: Vec<usize>,
    /// consult/corpus hit cap (500) reached.
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct CorpusHit {
    pub doc_idx: usize,
    pub entry_title: String,
    /// Matching lines: (0-based line in plain_body, text).
    pub lines: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
pub struct LinkHist {
    pub doc_idx: usize,
    pub list_sel: usize,
    pub filter: String,
    pub scroll: u16,
    pub caret_line: usize,
    pub caret_col: usize,
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
            truncated: false,
        };
        self.status = "links — type to filter · ↑↓ Enter · Esc".into();
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
            self.status = "no outline / senses in document".into();
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
            truncated: false,
        };
        if let Some(pos) = self.nav.filtered.iter().position(|&i| i == sel) {
            self.nav.selected = pos;
        }
        self.status = "o outline/senses — type to filter · ↑↓ Enter jump · Esc".into();
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
            truncated: false,
        };
        self.consult_hist_idx = None;
        self.refilter_consult();
    }

    pub(crate) fn open_corpus(&mut self) {
        let n = self.source.entries().len();
        if n == 0 {
            self.status = "no entries to search".into();
            return;
        }
        self.corpus_hits.clear();
        // keep corpus_plain across opens — only resize if entry count changed
        if self.corpus_plain.len() != n {
            self.corpus_plain.clear();
            self.corpus_plain.resize(n, None);
        }
        self.nav = NavState {
            overlay: Some(Overlay::Corpus),
            selected: 0,
            scroll: 0,
            query: String::new(),
            filtered: Vec::new(),
            truncated: false,
        };
        self.corpus_hist_idx = None;
        self.refilter_corpus();
        self.status =
            format!("corpus {n} entries — type · C-p/n hist · ↑↓ files · ←→ lines");
    }

    fn refilter_corpus(&mut self) {
        // empty query = titles only
        // dict: plugin fulltext (lazy index) → titles only; line extract lazy on selection
        // non-dict fallback: host scan with corpus_plain cache
        let tokens: Vec<String> = self
            .nav
            .query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        let mut hits = Vec::new();
        let mut truncated = false;
        let n = self.source.entries().len();
        if self.corpus_plain.len() != n {
            self.corpus_plain.clear();
            self.corpus_plain.resize(n, None);
        }

        let candidates: Vec<usize> = if tokens.is_empty() {
            (0..n.min(500)).collect()
        } else {
            let q = self.nav.query.trim();
            let ft = self.source.fulltext_search(q, 500);
            if !ft.is_empty() {
                let entries = self.source.entries().to_vec();
                let mut idx: std::collections::HashMap<&str, usize> =
                    std::collections::HashMap::with_capacity(entries.len());
                for (i, e) in entries.iter().enumerate() {
                    idx.entry(e.as_str()).or_insert(i);
                }
                let mut out = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for hw in &ft {
                    if !seen.insert(hw.as_str()) {
                        continue;
                    }
                    if let Some(&di) = idx.get(hw.as_str()) {
                        out.push(di);
                        if out.len() >= 500 {
                            truncated = true;
                            break;
                        }
                    }
                }
                // mark: plugin path — skip bulk plain_body in loop below
                out
            } else {
                // fallback: host scan title + plain_body (md/url/…)
                let mut out = Vec::new();
                for di in 0..n {
                    let title = self
                        .source
                        .entries()
                        .get(di)
                        .cloned()
                        .unwrap_or_else(|| format!("#{di}"));
                    if self.corpus_plain[di].is_none() {
                        let plain = self.source.plain_body(di);
                        let lower = plain.to_lowercase();
                        self.corpus_plain[di] = Some((plain, lower));
                    }
                    let plain_l = self.corpus_plain[di]
                        .as_ref()
                        .map(|(_, l)| l.as_str())
                        .unwrap_or("");
                    let title_l = title.to_lowercase();
                    if tokens.iter().all(|t| title_l.contains(t) || plain_l.contains(t)) {
                        out.push(di);
                        if out.len() >= 500 {
                            truncated = true;
                            break;
                        }
                    }
                }
                out
            }
        };
        for di in candidates {
            let title = self
                .source
                .entries()
                .get(di)
                .cloned()
                .unwrap_or_else(|| format!("#{di}"));
            hits.push(CorpusHit {
                doc_idx: di,
                entry_title: title,
                lines: Vec::new(), // lazy: ensure_corpus_lines_for_selected
            });
            if hits.len() >= 500 {
                truncated = true;
                break;
            }
        }
        self.corpus_hits = hits;
        self.nav.filtered = (0..self.corpus_hits.len()).collect();
        self.nav.truncated = truncated;
        if self.nav.selected >= self.nav.filtered.len() {
            self.nav.selected = self.nav.filtered.len().saturating_sub(1);
        }
        self.nav.scroll = 0;
        self.ensure_corpus_lines_for_selected();
        self.refresh_corpus_status();
    }

    fn refresh_corpus_status(&mut self) {
        let m = self.corpus_hits.len();
        self.status = if m == 0 {
            "corpus 0 · C-p/n hist".into()
        } else if self.nav.truncated {
            format!(
                "corpus {}/{}+ (truncated) · C-p/n hist · ↑↓ files · ←→ lines",
                self.nav.selected + 1,
                m
            )
        } else {
            format!(
                "corpus {}/{m} · C-p/n hist · ↑↓ files · ←→ lines",
                self.nav.selected + 1
            )
        };
    }

    /// Fill match lines for the currently selected corpus hit (one entry, cached).
    fn ensure_corpus_lines_for_selected(&mut self) {
        let tokens: Vec<String> = self
            .nav
            .query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return;
        }
        let Some(&fi) = self.nav.filtered.get(self.nav.selected) else {
            return;
        };
        if fi >= self.corpus_hits.len() {
            return;
        }
        if !self.corpus_hits[fi].lines.is_empty() {
            return;
        }
        let di = self.corpus_hits[fi].doc_idx;
        let title = self.corpus_hits[fi].entry_title.clone();
        let n = self.source.entries().len();
        if self.corpus_plain.len() != n {
            self.corpus_plain.clear();
            self.corpus_plain.resize(n, None);
        }
        if di >= n {
            return;
        }
        if self.corpus_plain[di].is_none() {
            let plain = self.source.plain_body(di);
            let lower = plain.to_lowercase();
            self.corpus_plain[di] = Some((plain, lower));
        }
        let plain = self.corpus_plain[di]
            .as_ref()
            .map(|(p, _)| p.as_str())
            .unwrap_or("");
        let mut lines = Vec::new();
        for (li, line) in plain.lines().enumerate() {
            let low = line.to_lowercase();
            if tokens.iter().all(|t| low.contains(t)) {
                lines.push((li, line.trim().chars().take(120).collect()));
                if lines.len() >= 80 {
                    break;
                }
            }
        }
        if lines.is_empty() {
            lines.push((0, title.chars().take(120).collect()));
        }
        self.corpus_hits[fi].lines = lines;
    }

    pub(crate) fn close_nav(&mut self) {
        self.nav.overlay = None;
        self.nav.query.clear();
        self.nav.filtered.clear();
        self.nav.truncated = false;
        self.corpus_hits.clear();
        // keep corpus_plain for next A-S-f in same session
        self.corpus_hist_idx = None;
        self.status = "nav closed".into();
    }


    fn refilter_consult(&mut self) {
        // ponytail: orderless AND of whitespace tokens; 500-cap, no fuzzy lib
        let tokens: Vec<String> = self
            .nav
            .query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        let mut hits = Vec::new();
        let mut truncated = false;
        for (i, line) in self.body.iter().enumerate() {
            let tl = Self::line_plain(line).to_lowercase();
            if tokens.is_empty() || tokens.iter().all(|t| tl.contains(t)) {
                hits.push(i);
                if hits.len() >= 500 {
                    truncated = true;
                    break;
                }
            }
        }
        self.nav.filtered = hits;
        self.nav.truncated = truncated;
        if self.nav.selected >= self.nav.filtered.len() {
            self.nav.selected = self.nav.filtered.len().saturating_sub(1);
        }
        self.nav.scroll = 0;
        self.refresh_consult_status();
    }

    fn refresh_consult_status(&mut self) {
        let n = self.nav.filtered.len();
        if n == 0 {
            self.status = "consult 0".into();
            return;
        }
        let sel = self.nav.selected + 1;
        self.status = if self.nav.truncated {
            format!("consult {sel}/{n}+ (truncated)")
        } else {
            format!("consult {sel}/{n}")
        };
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
            Overlay::Corpus => {
                self.refilter_corpus();
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
        let list_filter =
            matches!(mode, Overlay::Toc | Overlay::Links | Overlay::Consult | Overlay::Corpus);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.close_nav();
                return true;
            }
            KeyCode::Up => {
                if mode == Overlay::Consult {
                    self.consult_history_step(-1);
                } else {
                    self.nav.selected = self.nav.selected.saturating_sub(1);
                    if mode == Overlay::Corpus {
                        self.nav.scroll = 0;
                        self.ensure_corpus_lines_for_selected();
                    }
                }
                return true;
            }
            KeyCode::Down => {
                if mode == Overlay::Consult {
                    self.consult_history_step(1);
                } else if !self.nav.filtered.is_empty() {
                    self.nav.selected =
                        (self.nav.selected + 1).min(self.nav.filtered.len() - 1);
                    if mode == Overlay::Corpus {
                        self.nav.scroll = 0;
                        self.ensure_corpus_lines_for_selected();
                    }
                }
                return true;
            }
            KeyCode::Char('p') | KeyCode::Char('P') if ctrl && mode == Overlay::Corpus => {
                self.corpus_history_step(-1);
                return true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') if ctrl && mode == Overlay::Corpus => {
                self.corpus_history_step(1);
                return true;
            }
            KeyCode::Left if mode == Overlay::Corpus => {
                self.ensure_corpus_lines_for_selected();
                self.nav.scroll = self.nav.scroll.saturating_sub(1);
                return true;
            }
            KeyCode::Right if mode == Overlay::Corpus => {
                self.ensure_corpus_lines_for_selected();
                if let Some(&fi) = self.nav.filtered.get(self.nav.selected) {
                    if let Some(hit) = self.corpus_hits.get(fi) {
                        let max = hit.lines.len().saturating_sub(1);
                        self.nav.scroll = (self.nav.scroll + 1).min(max);
                    }
                }
                return true;
            }
            KeyCode::Enter => {
                if mode == Overlay::Corpus {
                    self.ensure_corpus_lines_for_selected();
                }
                self.nav_activate(mode);
                return true;
            }
            KeyCode::Backspace if list_filter => {
                self.nav.query.pop();
                if mode == Overlay::Consult {
                    self.consult_hist_idx = None;
                }
                if mode == Overlay::Corpus {
                    self.corpus_hist_idx = None;
                }
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
                    if mode == Overlay::Consult {
                        self.consult_hist_idx = None;
                    }
                    if mode == Overlay::Corpus {
                        self.corpus_hist_idx = None;
                    }
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
                    self.set_caret(h.line, 0);
                    self.status = format!("jumped to: {}", h.text);
                }
            }
            Overlay::Consult => {
                // keep tokens so body highlight shows real matched substrings
                let q = self.nav.query.clone();
                self.push_consult_history(&q);
                self.set_caret(idx, 0);
                self.vim_query = q;
                self.vim_match_idx = 0;
                if !self.vim_query.is_empty() {
                    let hits = self.match_hits();
                    if let Some(i) = hits.iter().position(|h| h.line == idx) {
                        self.vim_match_idx = i;
                    }
                }
                self.status = format!("jumped to line {}", idx + 1);
            }
            Overlay::Corpus => {
                if let Some(hit) = self.corpus_hits.get(idx).cloned() {
                    let line_hint = hit
                        .lines
                        .get(self.nav.scroll)
                        .cloned()
                        .or_else(|| hit.lines.first().cloned());
                    if let Some(pos) = self.filtered.iter().position(|&i| i == hit.doc_idx) {
                        self.list_sel = pos;
                    } else {
                        self.filter.clear();
                        self.refilter();
                        if let Some(pos) = self.filtered.iter().position(|&i| i == hit.doc_idx) {
                            self.list_sel = pos;
                        }
                    }
                    let q = self.nav.query.clone();
                    self.push_corpus_history(&q);
                    self.load_selected();
                    if !q.is_empty() {
                        self.vim_query = q;
                        let hits = self.match_hits();
                        let which = line_hint
                            .and_then(|(li, text)| {
                                hits.iter()
                                    .position(|h| h.line == li)
                                    .or_else(|| {
                                        let t = text.trim();
                                        if t.is_empty() {
                                            return None;
                                        }
                                        self.body
                                            .iter()
                                            .position(|l| {
                                                let p = Self::line_plain(l);
                                                p.contains(t)
                                                    || (!p.is_empty() && t.contains(p.trim()))
                                            })
                                            .and_then(|bl| hits.iter().position(|h| h.line == bl))
                                    })
                            })
                            .unwrap_or(0);
                        if hits.is_empty() {
                            self.status =
                                format!("corpus → {} (no body match)", hit.entry_title);
                        } else {
                            self.jump_to_match(which);
                        }
                    } else if let Some((li, _)) = line_hint {
                        self.set_caret(li, 0);
                        self.status = format!("corpus → {}", hit.entry_title);
                    } else {
                        self.status = format!("corpus → {}", hit.entry_title);
                    }
                }
            }
        }
        self.nav.overlay = None;
        self.nav.query.clear();
        self.corpus_hits.clear();
        // keep corpus_plain for next A-S-f
        self.corpus_hist_idx = None;
    }

    fn push_consult_history(&mut self, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            return;
        }
        self.consult_history.retain(|h| h != q);
        self.consult_history.push(q.to_string());
        // ponytail: small cap
        if self.consult_history.len() > 50 {
            self.consult_history.remove(0);
        }
        self.consult_hist_idx = None;
    }

    /// dir -1 = older (↑), +1 = newer (↓). Vim-style cmdline history.
    fn consult_history_step(&mut self, dir: isize) {
        if self.consult_history.is_empty() {
            return;
        }
        let n = self.consult_history.len();
        let next = match self.consult_hist_idx {
            None if dir < 0 => Some(n - 1),
            None => return,
            Some(i) => {
                let j = i as isize + dir;
                if j < 0 {
                    Some(0)
                } else if j >= n as isize {
                    self.consult_hist_idx = None;
                    self.nav.query.clear();
                    self.refilter_consult();
                    return;
                } else {
                    Some(j as usize)
                }
            }
        };
        if let Some(i) = next {
            self.consult_hist_idx = Some(i);
            self.nav.query = self.consult_history[i].clone();
            self.refilter_consult();
        }
    }

    fn push_corpus_history(&mut self, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            return;
        }
        self.corpus_history.retain(|h| h != q);
        self.corpus_history.push(q.to_string());
        // ponytail: same cap as consult
        if self.corpus_history.len() > 50 {
            self.corpus_history.remove(0);
        }
        self.corpus_hist_idx = None;
    }

    /// dir -1 = older (↑), +1 = newer (↓).
    fn corpus_history_step(&mut self, dir: isize) {
        if self.corpus_history.is_empty() {
            return;
        }
        let n = self.corpus_history.len();
        let next = match self.corpus_hist_idx {
            None if dir < 0 => Some(n - 1),
            None => return,
            Some(i) => {
                let j = i as isize + dir;
                if j < 0 {
                    Some(0)
                } else if j >= n as isize {
                    self.corpus_hist_idx = None;
                    self.nav.query.clear();
                    self.refilter_corpus();
                    return;
                } else {
                    Some(j as usize)
                }
            }
        };
        if let Some(i) = next {
            self.corpus_hist_idx = Some(i);
            self.nav.query = self.corpus_history[i].clone();
            self.nav.selected = 0;
            self.refilter_corpus();
        }
    }



    /// Left-click body: open link under cursor (blue/underline spans).
    pub(crate) fn handle_mouse(&mut self, m: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if !matches!(
            m.kind,
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
        ) {
            // scroll wheel over body
            if let Some(area) = self.content_area {
                if m.column >= area.x
                    && m.column < area.x.saturating_add(area.width)
                    && m.row >= area.y
                    && m.row < area.y.saturating_add(area.height)
                {
                    match m.kind {
                        MouseEventKind::ScrollDown => {
                            self.scroll = self.scroll.saturating_add(3);
                        }
                        MouseEventKind::ScrollUp => {
                            self.scroll = self.scroll.saturating_sub(3);
                        }
                        _ => {}
                    }
                }
            }
            return;
        }
        // only act on Down to avoid double-open
        if !matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        if self.nav_open() || self.show_help || self.vim_mode {
            return;
        }
        #[cfg(feature = "ai")]
        if self.ai.is_open() {
            return;
        }
        let Some(area) = self.content_area else {
            return;
        };
        // content block has TOP border → body starts at y+1
        if m.column < area.x || m.row <= area.y {
            return;
        }
        if m.column >= area.x.saturating_add(area.width)
            || m.row >= area.y.saturating_add(area.height)
        {
            return;
        }
        let row = (m.row - area.y - 1) as usize;
        let col = (m.column - area.x) as usize;
        let line = self.scroll as usize + row;
        if let Some(link) = self.link_at(line, col) {
            self.dispatch_link(&link);
        }
    }

    fn link_at(&self, line: usize, col: usize) -> Option<LinkEntry> {
        use ratatui::style::Modifier;
        use unicode_width::UnicodeWidthChar;
        let body_line = self.body.get(line)?;
        // find underlined run covering col (display width)
        let mut x = 0usize;
        let mut run_start = None;
        let mut run_text = String::new();
        let mut hit = false;
        for sp in &body_line.spans {
            let is_link = sp.style.add_modifier.contains(Modifier::UNDERLINED);
            if !is_link {
                if hit {
                    break;
                }
                run_start = None;
                run_text.clear();
                for ch in sp.content.chars() {
                    x += ch.width().unwrap_or(0);
                }
                continue;
            }
            if run_start.is_none() {
                run_start = Some(x);
                run_text.clear();
            }
            for ch in sp.content.chars() {
                let w = ch.width().unwrap_or(0);
                if col >= x && col < x + w.max(1) {
                    hit = true;
                }
                run_text.push(ch);
                x += w;
            }
        }
        if !hit || run_text.is_empty() {
            // fallback: any link recorded near this line
            return self
                .links
                .iter()
                .filter(|l| l.line.abs_diff(line) <= 2)
                .min_by_key(|l| l.line.abs_diff(line))
                .cloned();
        }
        let t = run_text.trim();
        self.links
            .iter()
            .find(|l| l.text == t || l.text.contains(t) || t.contains(l.text.as_str()))
            .or_else(|| {
                self.links
                    .iter()
                    .find(|l| l.line.abs_diff(line) <= 3 && (l.text.contains(t) || t.contains(&l.text)))
            })
            .cloned()
    }

    pub(crate) fn dispatch_link(&mut self, link: &LinkEntry) {
        let url = link.url.as_str();
        if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
            // ponytail: remote TUI often has no browser — OSC 52 copy beats xdg-open
            match crate::app::visual::yank_osc52(url) {
                Ok(()) => {
                    let short = if url.chars().count() > 60 {
                        format!("{}…", url.chars().take(57).collect::<String>())
                    } else {
                        url.to_string()
                    };
                    self.status = format!("copied link: {short}");
                }
                Err(e) => self.status = format!("copy link failed: {e}"),
            }
            return;
        }
        if let Some(anchor) = url.strip_prefix('#') {
            self.push_link_hist();
            if let Some(h) = self
                .headings
                .iter()
                .find(|h| heading_slug(&h.text) == anchor)
            {
                self.set_caret(h.line, 0);
                self.status = format!("jumped to #{anchor}");
            } else {
                self.link_hist.pop();
                self.status = format!("heading not found: #{anchor}");
            }
            return;
        }
        // local relative file — prefer in-app open for md/txt (never hand TTY to lynx)
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
            let idx = self
                .find_entry_for_path(&resolved)
                .or_else(|| self.source.ensure_local_doc(&resolved));
            if let Some(i) = idx {
                self.push_link_hist();
                if let Some(pos) = self.filtered.iter().position(|&di| di == i) {
                    self.list_sel = pos;
                } else {
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
                        self.set_caret(h.line, 0);
                    }
                }
                self.status = format!("opened: {}", resolved.display());
            } else {
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

    pub(crate) fn push_link_hist(&mut self) {
        let Some(doc_idx) = self.loaded_doc.or_else(|| self.selected_doc_index()) else {
            return;
        };
        self.link_hist.push(LinkHist {
            doc_idx,
            list_sel: self.list_sel,
            filter: self.filter.clone(),
            scroll: self.scroll,
            caret_line: self.caret_line,
            caret_col: self.caret_col,
        });
        // ponytail: cap stack; drop oldest
        if self.link_hist.len() > 32 {
            self.link_hist.remove(0);
        }
    }

    /// Restore previous f-link position. true if popped.
    pub(crate) fn pop_link_hist(&mut self) -> bool {
        let Some(h) = self.link_hist.pop() else {
            return false;
        };
        if self.filter != h.filter {
            self.filter = h.filter;
            self.refilter_keep(Some(h.doc_idx));
        }
        if let Some(pos) = self.filtered.iter().position(|&i| i == h.doc_idx) {
            self.list_sel = pos;
        } else if !self.filtered.is_empty() {
            self.list_sel = h.list_sel.min(self.filtered.len() - 1);
        }
        if self.loaded_doc != Some(h.doc_idx) {
            self.load_selected();
        }
        self.scroll = h.scroll;
        self.caret_line = h.caret_line.min(self.body.len().saturating_sub(1));
        self.caret_col = h.caret_col.min(self.line_len(self.caret_line));
        self.ensure_line_visible(self.caret_line);
        self.status = "back".into();
        true
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
    // ponytail: xdg-open / open / cmd — null stdio so TUI terminal stays intact
    use std::process::Stdio;
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["open", target]]
    } else if cfg!(target_os = "windows") {
        &[&["cmd", "/C", "start", "", target]]
    } else {
        &[&["xdg-open", target], &["gio", "open", target]]
    };
    let mut last = String::from("no opener");
    for c in cmds {
        match Command::new(c[0])
            .args(&c[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
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
                let pad = "  ".repeat(h.level.saturating_sub(1) as usize);
                let t = h.text.trim_start_matches('#').trim();
                format!("{pad}{t}")
            },
        ),
        Overlay::Consult => draw_consult(frame, area, app, theme),
        Overlay::Corpus => draw_corpus(frame, area, app, theme),
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

fn draw_corpus(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    // left: matching files · right: lines in selected file · query highlight
    let h = area.height.saturating_sub(2).max(12);
    let w = area.width.saturating_sub(2).max(50);
    let popup = centered(w, h, area);
    frame.render_widget(Clear, popup);

    let n = app.nav.filtered.len();
    let title = if n == 0 {
        " corpus 0 ".into()
    } else if app.nav.truncated {
        format!(" corpus {}/{}+ ", app.nav.selected + 1, n)
    } else {
        format!(" corpus {}/{n} ", app.nav.selected + 1)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let v = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
    let tokens: Vec<String> = app
        .nav
        .query
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    let q_style = Style::default().fg(if tokens.is_empty() {
        theme.muted()
    } else {
        theme.search_text()
    });
    let hit_style = Style::default()
        .fg(theme.status_focus_fg())
        .bg(theme.search_text())
        .add_modifier(Modifier::BOLD);
    let normal = Style::default().fg(theme.list_text());
    let muted = Style::default().fg(theme.muted());

    let q_text = if app.nav.query.is_empty() {
        "type to search…".to_string()
    } else {
        app.nav.query.clone()
    };
    let mut q_spans = if tokens.is_empty() {
        vec![Span::styled(q_text, q_style)]
    } else {
        token_highlight_spans(&q_text, &tokens, q_style, hit_style)
    };
    q_spans.push(Span::styled(format!("  {n} files"), muted));
    frame.render_widget(Paragraph::new(Line::from(q_spans)), v[0]);
    if v[0].width > 0 {
        let col = app.nav.query.chars().count() as u16;
        frame.set_cursor_position((
            v[0].x + col.min(v[0].width.saturating_sub(1)),
            v[0].y,
        ));
    }

    let cols = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(v[1]);

    // —— left: files ——
    let fvis = cols[0].height.saturating_sub(1).max(1) as usize;
    let fstart = if app.nav.selected >= fvis {
        app.nav.selected + 1 - fvis
    } else {
        0
    };
    let fend = (fstart + fvis).min(n);
    let file_items: Vec<ListItem> = app.nav.filtered[fstart..fend]
        .iter()
        .enumerate()
        .map(|(row, &idx)| {
            let sel = fstart + row == app.nav.selected;
            let hit = app.corpus_hits.get(idx);
            let title = hit.map(|h| h.entry_title.as_str()).unwrap_or("?");
            let count = hit.map(|h| h.lines.len()).unwrap_or(0);
            let label = if count == 0 {
                trunc(title, 28)
            } else {
                format!("{}  {}", trunc(title, 24), count)
            };
            let style = if sel {
                Style::default()
                    .fg(theme.status_focus_fg())
                    .bg(theme.search_text())
                    .add_modifier(Modifier::BOLD)
            } else {
                normal
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();
    frame.render_widget(
        List::new(file_items).block(
            Block::default()
                .borders(Borders::TOP | Borders::RIGHT)
                .title(" files "),
        ),
        cols[0],
    );

    // —— right: lines in selected file ——
    let line_sel = app.nav.scroll;
    let cur = app
        .nav
        .filtered
        .get(app.nav.selected)
        .and_then(|&i| app.corpus_hits.get(i));
    let match_lines: &[(usize, String)] = cur.map(|h| h.lines.as_slice()).unwrap_or(&[]);
    let lvis = cols[1].height.saturating_sub(1).max(1) as usize;
    let lstart = if line_sel >= lvis {
        line_sel + 1 - lvis
    } else {
        0
    };
    let lend = (lstart + lvis).min(match_lines.len());
    let line_items: Vec<ListItem> = match_lines[lstart..lend]
        .iter()
        .enumerate()
        .map(|(row, (li, text))| {
            let sel = lstart + row == line_sel;
            let prefix = if sel {
                format!("▶{:>4} │ ", li + 1)
            } else {
                format!(" {:>4} │ ", li + 1)
            };
            let base = if sel {
                Style::default()
                    .fg(theme.accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                normal
            };
            let mut spans = vec![Span::styled(prefix, base)];
            let shown = trunc(text, 90);
            if tokens.is_empty() {
                spans.push(Span::styled(shown, base));
            } else {
                spans.extend(token_highlight_spans(&shown, &tokens, base, hit_style));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let lines_title = if match_lines.is_empty() {
        " lines ".into()
    } else {
        format!(" lines {}/{} ", line_sel + 1, match_lines.len())
    };
    frame.render_widget(
        List::new(line_items).block(
            Block::default()
                .borders(Borders::TOP)
                .title(lines_title),
        ),
        cols[1],
    );
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
        format!("{title}  filter")
    } else {
        format!("{title}  {}", nav.query)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
    let filter_line = Line::from(vec![
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

    let n = app.nav.filtered.len();
    let title = if n == 0 {
        " consult 0 ".to_string()
    } else {
        let sel = app.nav.selected + 1;
        if app.nav.truncated {
            format!(" consult {sel}/{n}+ ")
        } else {
            format!(" consult {sel}/{n} ")
        }
    };
    let block = Block::default()
        .title(title)
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
    if v[0].width > 0 {
        let col = 2u16.saturating_add(app.nav.query.chars().count() as u16);
        let col = col.min(v[0].width.saturating_sub(1));
        frame.set_cursor_position((v[0].x + col, v[0].y));
    }

    let tokens: Vec<String> = app
        .nav
        .query
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    let hit_style = Style::default()
        .fg(theme.status_focus_fg())
        .bg(theme.search_text())
        .add_modifier(Modifier::BOLD);
    let normal = Style::default().fg(theme.list_text());
    let hits = &app.nav.filtered;
    // List TOP border + title eats 1 row — without this, rows past ~height-1 look unstyled
    let visible = v[1].height.saturating_sub(1).max(1) as usize;
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
            let text = trunc(&plain, 80);
            // line number never highlighted; only query tokens in the text
            let prefix = if sel {
                format!("▶{:>3} │ ", line + 1)
            } else {
                format!(" {:>3} │ ", line + 1)
            };
            let mut spans = vec![Span::styled(prefix, normal)];
            spans.extend(token_highlight_spans(&text, &tokens, normal, hit_style));
            ListItem::new(Line::from(spans))
        })
        .collect();
    let hits_title = if app.nav.truncated {
        format!(" {} hits (truncated) ", hits.len())
    } else {
        format!(" {} hits ", hits.len())
    };
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::TOP)
                .title(hits_title),
        ),
        v[1],
    );

    // preview: ±2 lines around selected; highlight match line tokens
    const CONSULT_PREVIEW_CTX: usize = 2;
    let preview_line = hits.get(app.nav.selected).copied();
    let mut prev_lines = Vec::new();
    if let Some(li) = preview_line {
        let from = li.saturating_sub(CONSULT_PREVIEW_CTX);
        let to = (li + CONSULT_PREVIEW_CTX + 1).min(app.body.len());
        for i in from..to {
            let mark = if i == li { "▶ " } else { "  " };
            let t = app.body.get(i).map(App::line_plain).unwrap_or_default();
            let base = if i == li {
                Style::default()
                    .fg(theme.accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted())
            };
            let mut spans = vec![Span::styled(mark.to_string(), base)];
            if i == li && !tokens.is_empty() {
                spans.extend(token_highlight_spans(&t, &tokens, base, hit_style));
            } else {
                spans.push(Span::styled(t, base));
            }
            prev_lines.push(Line::from(spans));
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

/// Highlight all orderless tokens in plain text (case-insensitive).
fn token_highlight_spans(
    text: &str,
    tokens: &[String],
    normal: Style,
    hit: Style,
) -> Vec<Span<'static>> {
    if tokens.is_empty() || text.is_empty() {
        return vec![Span::styled(text.to_string(), normal)];
    }
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let n = chars.len();
    let mut mark = vec![false; n];
    for tok in tokens {
        let tchars: Vec<char> = tok.chars().collect();
        let m = tchars.len();
        if m == 0 || m > n {
            continue;
        }
        let mut i = 0;
        while i + m <= n {
            if lower[i..i + m] == tchars[..] {
                for b in &mut mark[i..i + m] {
                    *b = true;
                }
                i += m;
            } else {
                i += 1;
            }
        }
    }
    let mut spans = Vec::new();
    let mut i = 0;
    while i < n {
        let on = mark[i];
        let start = i;
        i += 1;
        while i < n && mark[i] == on {
            i += 1;
        }
        let piece: String = chars[start..i].iter().collect();
        spans.push(Span::styled(piece, if on { hit } else { normal }));
    }
    spans
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
