//! Reader shell: mdx-tui keys + core AI + visual/yank.
//!
//! Key handling / vim search / visual live in submodules (P1 split).

mod keys;
mod mode;
pub use mode::InputMode;

pub(crate) mod nav;
mod search;
mod visual;

use crossterm::event::{self, Event};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::DefaultTerminal;

use crate::plugin::{ContentSource, HeadingEntry, LinkEntry};
use crate::theme::Theme;
use crate::ui;

pub use search::MatchHit;
#[allow(unused_imports)] // tests + external API surface
pub use visual::{selected_plain, selected_plain_char, VisualKind, VisualSel};

static FILE_CFG: std::sync::OnceLock<Option<crate::config::FileConfig>> = std::sync::OnceLock::new();

pub fn set_file_config(cfg: Option<crate::config::FileConfig>) {
    let _ = FILE_CFG.set(cfg);
}

#[cfg(feature = "ai")]
fn file_config() -> Option<&'static crate::config::FileConfig> {
    FILE_CFG.get().and_then(|o| o.as_ref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchLayout {
    Left,
    Top,
}

pub struct App {
    pub(crate) source: Box<dyn ContentSource>,
    pub(crate) body: Vec<Line<'static>>,
    pub(crate) scroll: u16,
    pub(crate) filter: String,
    pub(crate) filtered: Vec<usize>,
    pub(crate) list_sel: usize,
    pub(crate) status: String,
    pub(crate) show_help: bool,
    pub(crate) show_sidebar: bool,
    pub(crate) search_layout: SearchLayout,
    pub(crate) theme: Theme,
    pub(crate) single_entry: bool,
    pub(crate) list_area: Option<Rect>,
    pub(crate) content_area: Option<Rect>,
    pub(crate) content_width: usize,
    pub(crate) vim_mode: bool,
    pub(crate) vim_input: String,
    pub(crate) vim_query: String,
    /// Index into current match list (char-level hits).
    pub(crate) vim_match_idx: usize,
    #[cfg(feature = "ai")]
    pub(crate) ai: crate::ai::AiSession,
    pub(crate) visual: Option<VisualSel>,
    pub(crate) links: Vec<LinkEntry>,
    pub(crate) headings: Vec<HeadingEntry>,
    pub(crate) nav: nav::NavState,
    /// Doc index currently shown in body (None = not loaded).
    pub(crate) loaded_doc: Option<usize>,
    /// Dict picker (Ctrl+B). Closed = None; open holds selection index into names cache.
    pub(crate) dict_panel: Option<usize>,
    pub(crate) dict_panel_names: Vec<String>,
    /// gg pending (visual/cursor motion).
    pub(crate) pending_g: bool,
}

impl App {
    pub fn new(source: Box<dyn ContentSource>) -> Self {
        let single_entry = source.entries().len() == 1;
        let mut app = Self {
            source,
            body: Vec::new(),
            scroll: 0,
            filter: String::new(),
            filtered: Vec::new(),
            list_sel: 0,
            status: String::new(),
            show_help: false,
            show_sidebar: !single_entry,
            search_layout: SearchLayout::Left,
            theme: Theme::Dark,
            single_entry,
            list_area: None,
            content_area: None,
            content_width: 88,
            vim_mode: false,
            vim_input: String::new(),
            vim_query: String::new(),
            vim_match_idx: 0,
            #[cfg(feature = "ai")]
            ai: crate::ai::AiSession::from_file_config(file_config()),
            visual: None,
            links: Vec::new(),
            headings: Vec::new(),
            nav: nav::NavState::default(),
            loaded_doc: None,
            dict_panel: None,
            dict_panel_names: Vec::new(),
            pending_g: false,
        };
        app.refilter();
        if app.filtered.is_empty() {
            app.status = "empty — ? help · C-q quit".into();
        } else {
            app.list_sel = 0;
            app.load_selected();
        }
        app
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        loop {
            #[cfg(feature = "ai")]
            self.ai.poll(self.source.as_mut());

            terminal.draw(|f| ui::draw(f, &mut self))?;
            let timeout = if cfg!(feature = "ai") {
                std::time::Duration::from_millis(80)
            } else {
                std::time::Duration::from_millis(200)
            };
            if !event::poll(timeout)? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if self.handle_key(key) {
                break;
            }
        }
        Ok(())
    }

    // ── ui accessors ──────────────────────────────────────────────────────
    pub(crate) fn input_mode(&self) -> mode::InputMode {
        mode::derive_input_mode(
            self.show_help,
            #[cfg(feature = "ai")]
            self.ai.is_open(),
            self.nav_open(),
            self.vim_mode,
            self.visual.is_some(),
        )
    }


    pub fn theme(&self) -> Theme {
        self.theme
    }
    pub fn show_sidebar(&self) -> bool {
        self.show_sidebar && !self.single_entry
    }
    pub fn search_layout(&self) -> SearchLayout {
        self.search_layout
    }
    pub fn show_help(&self) -> bool {
        self.show_help
    }
    pub fn vim_search_mode(&self) -> bool {
        self.vim_mode
    }
    pub fn filter(&self) -> &str {
        &self.filter
    }
    pub fn source_title(&self) -> &str {
        self.source.title()
    }
    pub fn filtered_len(&self) -> usize {
        self.filtered.len()
    }
    pub fn list_sel(&self) -> usize {
        self.list_sel
    }
    pub fn visible_names(&self) -> Vec<String> {
        let page = self.list_page_size();
        let start = if self.list_sel >= page {
            self.list_sel + 1 - page
        } else {
            0
        };
        let end = (start + page).min(self.filtered.len());
        let entries = self.source.entries();
        self.filtered[start..end]
            .iter()
            .map(|&i| entries[i].clone())
            .collect()
    }
    pub fn visible_sel(&self) -> usize {
        let page = self.list_page_size();
        let start = if self.list_sel >= page {
            self.list_sel + 1 - page
        } else {
            0
        };
        self.list_sel.saturating_sub(start)
    }
    pub fn content_title(&self) -> String {
        self.selected_name().unwrap_or_else(|| "content".into())
    }
    pub fn body_lines(&self) -> &[Line<'static>] {
        &self.body
    }
    pub fn scroll(&self) -> u16 {
        self.scroll
    }
    /// HN `a` / plugin action available.
    pub fn can_plugin_action(&self) -> bool {
        self.source.has_action()
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn vim_input(&self) -> &str {
        &self.vim_input
    }
    pub fn vim_query(&self) -> &str {
        &self.vim_query
    }
    /// Current match index for UI (0-based into match list).
    #[allow(dead_code)]
    pub fn vim_match_idx(&self) -> usize {
        self.vim_match_idx
    }
    #[cfg(feature = "ai")]
    pub fn ai_open(&self) -> bool {
        self.ai.is_open()
    }
    #[cfg(feature = "ai")]
    pub fn ai_mut(&mut self) -> &mut crate::ai::AiSession {
        &mut self.ai
    }
    #[cfg(feature = "ai")]
    #[allow(dead_code)]
    pub fn ai(&self) -> &crate::ai::AiSession {
        &self.ai
    }

    pub fn focus_label(&self) -> &'static str {
        #[cfg(feature = "ai")]
        if let Some(l) = self.ai.focus_label() {
            return l;
        }
        if self.dict_panel.is_some() {
            return "dicts";
        }
        if self.nav.overlay.is_some() {
            return match self.nav.overlay {
                Some(nav::Overlay::Links) => "links",
                Some(nav::Overlay::Toc) => "outline",
                Some(nav::Overlay::Consult) => "consult",
                None => "nav",
            };
        }
        if self.visual.is_some() {
            return "visual";
        }
        if self.vim_mode {
            "vim"
        } else if self.show_sidebar() {
            "search"
        } else {
            "content"
        }
    }

    pub fn set_list_area(&mut self, r: Option<Rect>) {
        self.list_area = r;
    }
    pub fn set_content_area(&mut self, r: Option<Rect>) {
        if let Some(rect) = r {
            let w = (rect.width as usize).max(20);
            if w != self.content_width {
                self.content_width = w;
                if !self.filtered.is_empty() {
                    let scroll = self.scroll;
                    self.load_selected();
                    self.scroll = scroll;
                }
            }
        }
        self.content_area = r;
    }

    /// Inclusive line range for full-line visual; `None` if char visual or off.
    pub fn visual_line_range(&self) -> Option<(usize, usize)> {
        let v = self.visual.as_ref()?;
        if v.kind != VisualKind::Line {
            return None;
        }
        let a = v.a_line.min(v.b_line);
        let b = v.a_line.max(v.b_line);
        Some((a, b))
    }

    /// Char-visual selection for paint (normalized).
    pub fn visual_char_sel(&self) -> Option<VisualSel> {
        let v = self.visual.as_ref()?;
        if v.kind != VisualKind::Char {
            return None;
        }
        Some(v.normalized())
    }

    pub(crate) fn list_page_size(&self) -> usize {
        self.list_area
            .map(|a| a.height.saturating_sub(2) as usize)
            .unwrap_or(20)
            .max(1)
    }

    pub(crate) fn content_page_step(&self) -> u16 {
        self.content_area
            .map(|a| (a.height.saturating_sub(1) / 2).max(1))
            .unwrap_or(10)
    }

    pub(crate) fn selected_name(&self) -> Option<String> {
        let di = *self.filtered.get(self.list_sel)?;
        self.source.entries().get(di).cloned()
    }

    pub(crate) fn selected_doc_index(&self) -> Option<usize> {
        self.filtered.get(self.list_sel).copied()
    }

    pub(crate) fn caret_line(&self) -> usize {
        (self.scroll as usize).min(self.body.len().saturating_sub(1))
    }

    /// Keep `line` inside the content viewport without jumping it to the top.
    pub(crate) fn ensure_line_visible(&mut self, line: usize) {
        let h = self
            .content_area
            .map(|a| a.height.saturating_sub(1) as usize)
            .unwrap_or(20)
            .max(1);
        let top = self.scroll as usize;
        let bottom = top.saturating_add(h.saturating_sub(1));
        if line < top {
            self.scroll = line as u16;
        } else if line > bottom {
            self.scroll = line.saturating_sub(h.saturating_sub(1)) as u16;
        }
    }

    #[cfg(feature = "ai")]
    pub(crate) fn plain_body(&self) -> String {
        self.body
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[cfg(feature = "ai")]
    pub(crate) fn refresh_ai_context(&mut self) {
        let title = self.selected_name().unwrap_or_default();
        let body = self.plain_body();
        self.ai.set_document_context(&title, &body);
    }

    pub(crate) fn move_sel(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let cur = self.list_sel as isize;
        let next = (cur + delta).clamp(0, self.filtered.len() as isize - 1) as usize;
        if next == self.list_sel {
            return;
        }
        self.list_sel = next;
        // HN bodies are prefetched at open → load is cache-hit / free
        if self.selected_doc_index() != self.loaded_doc {
            self.load_selected();
        }
    }



    pub(crate) fn refilter(&mut self) {
        let q = self.filter.to_lowercase();
        let entries = self.source.entries();
        self.filtered = entries
            .iter()
            .enumerate()
            .filter(|(_, name)| q.is_empty() || name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        if self.filtered.is_empty() {
            self.list_sel = 0;
            self.body.clear();
            self.loaded_doc = None;
            self.status = "no matches".into();
        } else {
            self.list_sel = self.list_sel.min(self.filtered.len() - 1);
            if self.selected_doc_index() != self.loaded_doc {
                self.load_selected();
            }
        }
    }

    /// Ctrl+U: clear filter string, keep current body/selection when possible.
    pub(crate) fn clear_filter_keep_result(&mut self) {
        let keep = self.selected_doc_index();
        self.filter.clear();
        self.refilter_keep(keep);
        self.status = "filter cleared".into();
    }

    /// Esc: clear filter; if already empty, clear body result.
    pub(crate) fn clear_filter_or_result(&mut self) {
        if !self.filter.is_empty() {
            let keep = self.selected_doc_index();
            self.filter.clear();
            self.refilter_keep(keep);
            self.status = "filter cleared".into();
        } else if !self.body.is_empty() {
            self.body.clear();
            self.links.clear();
            self.headings.clear();
            self.loaded_doc = None;
            self.scroll = 0;
            self.status = "result cleared".into();
        }
    }

    pub(crate) fn refilter_keep(&mut self, keep: Option<usize>) {
        let q = self.filter.to_lowercase();
        let entries = self.source.entries();
        self.filtered = entries
            .iter()
            .enumerate()
            .filter(|(_, name)| q.is_empty() || name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        if self.filtered.is_empty() {
            self.list_sel = 0;
            // ponytail: keep body on clear-filter; only empty list drops selection index
            return;
        }
        if let Some(di) = keep {
            if let Some(pos) = self.filtered.iter().position(|&i| i == di) {
                self.list_sel = pos;
                return;
            }
        }
        self.list_sel = self.list_sel.min(self.filtered.len() - 1);
        if self.selected_doc_index() != self.loaded_doc {
            self.load_selected();
        }
    }

    pub(crate) fn delete_filter_word(&mut self) {
        // ponytail: cursor always at end of filter
        let s = self.filter.trim_end();
        if let Some(pos) = s.rfind(char::is_whitespace) {
            self.filter.truncate(pos);
        } else {
            self.filter.clear();
        }
        self.refilter();
    }

    pub(crate) fn toggle_dict_panel(&mut self) {
        if self.dict_panel.is_some() {
            self.dict_panel = None;
            self.dict_panel_names.clear();
            self.status = "dict panel off".into();
            return;
        }
        let names = self.source.list_dicts();
        if names.is_empty() {
            self.status = "no multi-dict".into();
            return;
        }
        self.nav.overlay = None;
        // prefer matching current title
        let cur = self.source.title();
        let sel = names.iter().position(|n| n == cur).unwrap_or(0);
        self.dict_panel_names = names;
        self.dict_panel = Some(sel);
        self.status = "dict panel".into();
    }

    pub(crate) fn close_dict_panel(&mut self) {
        self.dict_panel = None;
        self.dict_panel_names.clear();
    }

    pub(crate) fn select_dict_from_panel(&mut self) {
        let Some(sel) = self.dict_panel else {
            return;
        };
        if !self.source.select_dict(sel) {
            self.status = "select dict failed".into();
            return;
        }
        self.close_dict_panel();
        let keep = self.selected_doc_index();
        // refilter after dict change (entries may change)
        self.refilter_keep(keep);
        if self.selected_doc_index() != self.loaded_doc {
            self.load_selected();
        } else if self.loaded_doc.is_some() {
            // same index may be different content after dict switch
            self.load_selected();
        }
        self.status = format!("dict: {}", self.source.title());
    }

    pub(crate) fn yank_definition(&mut self) {
        let text = if let Some(di) = self.selected_doc_index() {
            self.source.plain_body(di)
        } else {
            self.body
                .iter()
                .map(Self::line_plain)
                .collect::<Vec<_>>()
                .join("\n")
        };
        if text.is_empty() {
            self.status = "yank: empty body".into();
            return;
        }
        match crate::app::visual::yank_osc52(&text) {
            Ok(()) => {
                let n = text.lines().count();
                self.status = format!("copied {n} line(s) via OSC 52");
            }
            Err(e) => self.status = format!("yank failed: {e}"),
        }
    }

    pub fn dict_panel_open(&self) -> bool {
        self.dict_panel.is_some()
    }

    pub fn dict_panel_names(&self) -> &[String] {
        &self.dict_panel_names
    }

    pub fn dict_panel_sel(&self) -> usize {
        self.dict_panel.unwrap_or(0)
    }

    pub(crate) fn load_selected(&mut self) {
        let Some(di) = self.selected_doc_index() else {
            return;
        };
        let result = self.source.load(di, self.content_width);
        self.body = result.lines;
        self.links = result.links;
        self.headings = result.headings;
        self.scroll = 0;
        self.status = result.status;
        self.loaded_doc = Some(di);
        self.visual = None;
        self.pending_g = false;
        self.nav.overlay = None;
        if !self.vim_query.is_empty() {
            self.vim_match_idx = 0;
            self.jump_to_match(0);
        }
    }

    /// Plugin-specific action then reload body (e.g. HN fetch article).
    pub(crate) fn source_action(&mut self, action: &str) {
        if !self.source.has_action() {
            return;
        }
        let Some(di) = self.selected_doc_index() else {
            self.status = "no selection".into();
            return;
        };
        if self.loaded_doc != Some(di) {
            self.load_selected();
        }
        if action == "article" {
            self.status = "a: 抓取全文…".into();
        }
        // plugin sets last_action_note → STATUS trailer; always reload to surface it
        let _ok = self.source.action(di, action);
        self.load_selected();
    }




    /// Jump among major sections: Meta → Article → Comments (not every subheading).
    pub(crate) fn jump_section(&mut self, dir: isize) {
        if self.headings.is_empty() {
            self.status = "no sections".into();
            return;
        }
        let pool = major_section_pool(&self.headings);
        if pool.is_empty() {
            self.status = "no Meta/Article/Comments sections".into();
            return;
        }
        let cur = self.scroll as usize;
        let mut at = 0usize;
        for (k, &hi) in pool.iter().enumerate() {
            if self.headings[hi].line <= cur {
                at = k;
            }
        }
        // from middle of section, ] goes to next; [ stays/prev
        let mut target = if dir > 0 {
            if self.headings[pool[at]].line < cur && at + 1 < pool.len() {
                at + 1
            } else {
                (at + 1).min(pool.len() - 1)
            }
        } else if dir < 0 {
            if self.headings[pool[at]].line < cur {
                at // jump to start of current major section
            } else {
                at.saturating_sub(1)
            }
        } else {
            at
        };
        if dir > 0 && target == at && at + 1 < pool.len() {
            target = at + 1;
        }
        let h = &self.headings[pool[target]];
        self.scroll = h.line as u16;
        self.status = format!(
            "§ {}  ({}/{})  [ ]",
            h.text,
            target + 1,
            pool.len()
        );
    }


    pub(crate) fn line_plain(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    }

    pub(crate) fn line_len(&self, line: usize) -> usize {
        self.body
            .get(line)
            .map(|l| Self::line_plain(l).chars().count())
            .unwrap_or(0)
    }
}

/// Indices of major HN-style sections for `[`/`]` (Meta / Article / Comments).
/// Falls back to level-1 headings when those labels are absent.
fn major_section_pool(headings: &[crate::plugin::HeadingEntry]) -> Vec<usize> {
    const NAMES: &[&str] = &["Meta", "Article", "Comments"];
    let mut out = Vec::new();
    for &name in NAMES {
        if let Some(i) = headings.iter().position(|h| {
            h.text.eq_ignore_ascii_case(name)
                || h.text.eq_ignore_ascii_case(&format!("# {name}"))
                || h.text.trim_start_matches('#').trim().eq_ignore_ascii_case(name)
        }) {
            out.push(i);
        }
    }
    if out.len() >= 2 {
        return out;
    }
    // generic docs: only H1 (level 1)
    let h1: Vec<usize> = headings
        .iter()
        .enumerate()
        .filter(|(_, h)| h.level == 1)
        .map(|(i, _)| i)
        .collect();
    if h1.len() >= 2 {
        return h1;
    }
    // last resort: level <= 2 but prefer sparse pool (max 8)
    let mut pool: Vec<usize> = headings
        .iter()
        .enumerate()
        .filter(|(_, h)| h.level <= 2)
        .map(|(i, _)| i)
        .collect();
    if pool.len() > 8 {
        let step = (pool.len() / 8).max(1);
        pool = pool.into_iter().step_by(step).take(8).collect();
    }
    pool
}



#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    #[test]
    fn major_pool_prefers_meta_article_comments() {
        use crate::plugin::HeadingEntry;
        let hs = vec![
            HeadingEntry {
                level: 1,
                text: "Meta".into(),
                line: 0,
            },
            HeadingEntry {
                level: 2,
                text: "Self-text".into(),
                line: 5,
            },
            HeadingEntry {
                level: 1,
                text: "Article".into(),
                line: 10,
            },
            HeadingEntry {
                level: 2,
                text: "Subsection".into(),
                line: 12,
            },
            HeadingEntry {
                level: 1,
                text: "Comments".into(),
                line: 40,
            },
        ];
        let p = major_section_pool(&hs);
        assert_eq!(p, vec![0, 2, 4]);
    }

    #[test]
    fn selected_plain_joins_lines() {

        let body = vec![
            Line::from(Span::raw("a")),
            Line::from(Span::raw("b")),
            Line::from(Span::raw("c")),
        ];
        assert_eq!(selected_plain(&body, 0, 1), "a\nb");
        assert_eq!(selected_plain(&body, 2, 0), "a\nb\nc");
    }

    #[test]
    fn delete_filter_word_truncates() {
        // pure string logic mirror of delete_filter_word (no full App/source)
        fn del_word(mut s: String) -> String {
            let t = s.trim_end();
            if let Some(pos) = t.rfind(char::is_whitespace) {
                s.truncate(pos);
            } else {
                s.clear();
            }
            s
        }
        assert_eq!(del_word("foo bar".into()), "foo");
        assert_eq!(del_word("foo".into()), "");
        assert_eq!(del_word("a b c ".into()), "a b");
    }

    #[test]
    fn char_selection_slice() {
        let body = vec![
            Line::from(Span::raw("hello")),
            Line::from(Span::raw("world")),
        ];
        let sel = VisualSel {
            kind: VisualKind::Char,
            a_line: 0,
            a_col: 1,
            b_line: 0,
            b_col: 4,
        };
        assert_eq!(selected_plain_char(&body, &sel), "ell");
        let cross = VisualSel {
            kind: VisualKind::Char,
            a_line: 0,
            a_col: 3,
            b_line: 1,
            b_col: 2,
        };
        assert_eq!(selected_plain_char(&body, &cross), "lo\nwo");
    }

    #[test]
    fn match_hits_count_and_current() {
        let body = vec![
            Line::from(Span::raw("foo bar foo")),
            Line::from(Span::raw("baz foo")),
        ];
        let hits = search::find_hits(&body, "foo");
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0], MatchHit { line: 0, start: 0, end: 3 });
        assert_eq!(hits[2].line, 1);
    }

    #[test]
    fn match_hits_orderless_tokens() {
        let body = vec![Line::from(Span::raw("see md then txt here"))];
        let hits = search::find_hits(&body, "txt md");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], MatchHit { line: 0, start: 4, end: 6 }); // "md"
        assert_eq!(hits[1], MatchHit { line: 0, start: 12, end: 15 }); // "txt"
    }

    #[test]
    fn default_source_has_no_action() {
        // trait default: no tuider_source_action (dict/md/code/url)
        struct Empty;
        impl crate::plugin::ContentSource for Empty {
            fn title(&self) -> &str {
                "t"
            }
            fn entries(&self) -> &[String] {
                &[]
            }
            fn load(&mut self, _: usize, _: usize) -> crate::plugin::LoadResult {
                crate::plugin::LoadResult::plain(vec![], "x".into())
            }
        }
        let s = Empty;
        assert!(!s.has_action());
    }
}
