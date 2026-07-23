//! Keyboard routing: Help → Ai → global → Nav → Vim → Visual (fallthrough) → Normal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::visual::VisualKind;
use super::App;

impl App {
    /// Returns true if the app should quit.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        // Capture before handle_global so `?` cannot open Help then re-match Help.
        let mode = self.input_mode();

        if mode == super::InputMode::Help {
            self.show_help = false;
            return false;
        }

        #[cfg(feature = "ai")]
        if mode == super::InputMode::Ai {
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(
                    key.code,
                    KeyCode::Char('q')
                        | KeyCode::Char('Q')
                        | KeyCode::Char('c')
                        | KeyCode::Char('C')
                )
            {
                return true;
            }
            if key.modifiers.contains(KeyModifiers::ALT)
                && matches!(key.code, KeyCode::Char('l') | KeyCode::Char('L'))
            {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.ai.toggle_maximize();
                } else {
                    self.ai.toggle();
                }
                self.status = self.ai.status.clone();
                return false;
            }
            self.ai.handle_key(key);
            self.status = self.ai.status.clone();
            return false;
        }

        // Dict panel focus before global so Esc/arrows stay in panel.
        if self.dict_panel.is_some() {
            return self.handle_dict_panel_key(key, ctrl, alt);
        }

        if self.handle_global(key, ctrl, alt) {
            return true;
        }

        match mode {
            super::InputMode::Help => unreachable!("handled above"),
            #[cfg(feature = "ai")]
            super::InputMode::Ai => unreachable!("handled above"),
            super::InputMode::Nav => {
                self.handle_nav_key(key);
                false
            }
            super::InputMode::VimSearch => self.handle_vim_key(key),
            super::InputMode::Visual => {
                // consume all keys here — fallthrough only scrolled viewport and left caret mid-screen
                let _ = self.handle_visual_key(key);
                false
            }
            super::InputMode::Normal => self.handle_normal_key(key, ctrl, alt, shift),
        }
    }

    fn handle_dict_panel_key(&mut self, key: KeyEvent, ctrl: bool, alt: bool) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Char('c') | KeyCode::Char('C')
                if ctrl =>
            {
                return true;
            }
            KeyCode::Char('b') | KeyCode::Char('B') if ctrl => {
                self.toggle_dict_panel();
            }
            KeyCode::Esc => {
                self.close_dict_panel();
                self.status = "dict panel off".into();
            }
            KeyCode::Up | KeyCode::Char('k') if !alt => {
                if let Some(sel) = self.dict_panel.as_mut() {
                    *sel = sel.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') if !alt => {
                if let Some(sel) = self.dict_panel.as_mut() {
                    let max = self.dict_panel_names.len().saturating_sub(1);
                    *sel = (*sel + 1).min(max);
                }
            }
            KeyCode::Enter if !ctrl && !alt => {
                self.select_dict_from_panel();
            }
            // scroll body behind panel
            KeyCode::Up if alt => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down if alt => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.content_page_step());
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(self.content_page_step());
            }
            KeyCode::Char('y') | KeyCode::Char('Y') if ctrl => self.yank_definition(),
            _ => {}
        }
        false
    }

    fn handle_normal_key(
        &mut self,
        key: KeyEvent,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> bool {
        // Sidebar search box owns focus whenever it is visible.
        // Plain letters / symbols type into filter; single-key commands are disabled.
        let sidebar = self.show_sidebar() && self.visual.is_none();

        // --- search-box focus (sidebar on) ---------------------------------
        if sidebar {
            if ctrl && matches!(key.code, KeyCode::Char('u') | KeyCode::Char('U')) {
                self.clear_filter_keep_result();
                return false;
            }
            if ctrl && matches!(key.code, KeyCode::Char('w') | KeyCode::Char('W')) {
                self.delete_filter_word();
                return false;
            }
            if alt && matches!(key.code, KeyCode::Backspace) {
                self.delete_filter_word();
                return false;
            }
            // Enter opens selection (not a "command letter")
            if key.code == KeyCode::Enter && key.modifiers == KeyModifiers::NONE && !alt && !ctrl {
                self.load_selected();
                return false;
            }
            // Tab still cycles dicts while filtering
            if key.code == KeyCode::Tab && key.modifiers == KeyModifiers::NONE {
                if self.source.cycle_layer() {
                    let keep = self.selected_doc_index();
                    self.refilter_keep(keep);
                    if self.selected_doc_index() != self.loaded_doc || self.loaded_doc.is_some() {
                        self.load_selected();
                    }
                    self.status = format!("source: {}", self.source.title());
                }
                return false;
            }
            if !alt && !ctrl {
                match key.code {
                    KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || shift => {
                        if !c.is_control() {
                            self.filter.push(c);
                            self.refilter();
                            return false;
                        }
                    }
                    KeyCode::Backspace => {
                        self.filter.pop();
                        self.refilter();
                        return false;
                    }
                    KeyCode::Delete => {
                        self.filter.pop();
                        self.refilter();
                        return false;
                    }
                    KeyCode::Esc => {
                        self.clear_filter_or_result();
                        return false;
                    }
                    _ => {}
                }
            }
            // arrows / pgup: list nav + scroll (no letter commands)
            self.handle_nav(key, alt, ctrl, shift, true);
            return false;
        }

        // --- body focus (sidebar hidden) -----------------------------------
        // start visual: v = cursor (then second v = char), V = line
        if key.modifiers == KeyModifiers::NONE && !self.body.is_empty() && self.visual.is_none() {
            match key.code {
                KeyCode::Char('v') => {
                    self.start_visual(VisualKind::Cursor);
                    return false;
                }
                KeyCode::Char('V') => {
                    self.start_visual(VisualKind::Line);
                    return false;
                }
                _ => {}
            }
        }

        if self.visual.is_none() && key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Char('f') => {
                    self.open_links();
                    return false;
                }
                KeyCode::Char('o') => {
                    self.open_toc();
                    return false;
                }
                KeyCode::Char('a') => {
                    // HN-only (plugin exports action); ignore on dict/md/code/url
                    if self.source.has_action() {
                        self.source_action("article");
                    }
                    return false;
                }
                KeyCode::Char('O') => {
                    self.open_current_dir();
                    return false;
                }
                KeyCode::Char('[') => {
                    self.jump_section(-1);
                    return false;
                }
                KeyCode::Char(']') => {
                    self.jump_section(1);
                    return false;
                }
                KeyCode::Char('/') => {
                    self.vim_mode = true;
                    self.vim_input.clear();
                    return false;
                }
                _ => {}
            }
        }

        if self.visual.is_none() {
            if alt && matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F')) {
                self.open_consult();
                return false;
            }
            if alt && matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O')) {
                self.open_current_dir();
                return false;
            }
        }

        if key.code == KeyCode::Tab && key.modifiers == KeyModifiers::NONE {
            if self.source.cycle_layer() {
                let keep = self.selected_doc_index();
                self.refilter_keep(keep);
                if self.selected_doc_index() != self.loaded_doc || self.loaded_doc.is_some() {
                    self.load_selected();
                }
                self.status = format!("source: {}", self.source.title());
            }
            return false;
        }

        if !self.vim_query.is_empty() && key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Char('n') => {
                    self.vim_next(1);
                    return false;
                }
                KeyCode::Char('N') => {
                    self.vim_next(-1);
                    return false;
                }
                _ => {}
            }
        }

        if key.code == KeyCode::Enter
            && key.modifiers == KeyModifiers::NONE
            && self.visual.is_none()
            && !alt
            && !ctrl
        {
            self.load_selected();
            return false;
        }

        self.handle_nav(key, alt, ctrl, shift, false);
        false
    }



    /// Global chords. Returns true = quit.
    fn handle_global(&mut self, key: KeyEvent, ctrl: bool, alt: bool) -> bool {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') if ctrl => return true,
            KeyCode::Char('c') | KeyCode::Char('C') if ctrl => return true,
            KeyCode::Char('?') if !self.vim_mode && key.modifiers == KeyModifiers::NONE => {
                self.show_help = true;
            }
            KeyCode::Char('f') | KeyCode::Char('F') if ctrl => {
                if !self.single_entry {
                    self.show_sidebar = !self.show_sidebar;
                    self.status = if self.show_sidebar {
                        "sidebar on (Ctrl+F toggles; not search layout)"
                    } else {
                        "sidebar off"
                    }
                    .into();
                }
            }
            KeyCode::Char('b') | KeyCode::Char('B') if ctrl => {
                self.toggle_dict_panel();
            }
            KeyCode::Char('y') | KeyCode::Char('Y') if ctrl => {
                self.yank_definition();
            }
            KeyCode::Char('u') | KeyCode::Char('U') if ctrl => {
                // works with or without sidebar so status is consistent
                self.clear_filter_keep_result();
            }
            KeyCode::Char('s') | KeyCode::Char('S') if ctrl => {
                if !self.show_sidebar() {
                    self.status = "Layout: show sidebar first (Ctrl+F)".into();
                } else {
                    self.search_layout = match self.search_layout {
                        super::SearchLayout::Left => super::SearchLayout::Top,
                        super::SearchLayout::Top => super::SearchLayout::Left,
                    };
                    self.status = match self.search_layout {
                        super::SearchLayout::Left => "layout: left",
                        super::SearchLayout::Top => "layout: top",
                    }
                    .into();
                }
            }
            #[cfg(feature = "ai")]
            KeyCode::Char('l') | KeyCode::Char('L') if alt && shift => {
                self.refresh_ai_context();
                self.ai.toggle_maximize();
                self.status = self.ai.status.clone();
            }
            #[cfg(feature = "ai")]
            KeyCode::Char('l') | KeyCode::Char('L') if alt => {
                self.refresh_ai_context();
                self.ai.toggle();
                self.status = self.ai.status.clone();
            }
            _ => {}
        }
        false
    }

    /// Returns true if key was consumed by visual mode.
    fn handle_visual_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let none = key.modifiers == KeyModifiers::NONE;
        let shift_or_none = none || key.modifiers == KeyModifiers::SHIFT;

        // gg chord: second key
        if self.pending_g {
            self.pending_g = false;
            match key.code {
                KeyCode::Char('g') if none => {
                    self.visual_goto_line(0);
                    return true;
                }
                KeyCode::Esc => {
                    self.visual = None;
                    self.status = "visual off".into();
                    return true;
                }
                _ => {} // not gg — fall through to normal visual handle
            }
        }

        match key.code {
            KeyCode::Esc => {
                self.visual = None;
                self.pending_g = false;
                self.status = "visual off".into();
                true
            }
            KeyCode::Char('v') if none => {
                if self
                    .visual
                    .as_ref()
                    .is_some_and(|v| v.kind == VisualKind::Cursor)
                {
                    self.visual_cursor_to_char();
                    true
                } else {
                    true // ignore extra v in Char/Line
                }
            }
            KeyCode::Char('y') if none => {
                self.yank_selection();
                true
            }
            KeyCode::Up | KeyCode::Char('k') if !ctrl => {
                self.visual_extend_line(-1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') if !ctrl => {
                self.visual_extend_line(1);
                true
            }
            KeyCode::Left | KeyCode::Char('h') if !ctrl => {
                self.visual_extend_col(-1);
                true
            }
            KeyCode::Right | KeyCode::Char('l') if !ctrl => {
                self.visual_extend_col(1);
                true
            }
            KeyCode::Char('w') if none => {
                self.visual_extend_word(true);
                true
            }
            KeyCode::Char('b') if none => {
                self.visual_extend_word(false);
                true
            }
            KeyCode::Char('e') if none => {
                self.visual_extend_word_end();
                true
            }
            KeyCode::Char('E') if shift_or_none => {
                self.visual_extend_line_edge(true);
                true
            }
            KeyCode::Char('B') if key.modifiers == KeyModifiers::SHIFT => {
                // SHIFT+B line start; plain b is word back
                self.visual_extend_line_edge(false);
                true
            }
            KeyCode::Char('0') if none => {
                self.visual_extend_line_edge(false);
                true
            }
            KeyCode::Char('^') if none || key.modifiers == KeyModifiers::SHIFT => {
                self.visual_first_nonblank();
                true
            }
            KeyCode::Char('$') if none || key.modifiers == KeyModifiers::SHIFT => {
                self.visual_extend_line_edge(true);
                true
            }
            KeyCode::Home => {
                self.visual_extend_line_edge(false);
                true
            }
            KeyCode::End => {
                self.visual_extend_line_edge(true);
                true
            }
            KeyCode::Char('g') if none => {
                self.pending_g = true;
                true
            }
            KeyCode::Char('G') if shift_or_none => {
                let last = self.body.len().saturating_sub(1);
                self.visual_goto_line(last);
                true
            }
            KeyCode::Char('H') if shift_or_none => {
                self.visual_goto_viewport('H');
                true
            }
            KeyCode::Char('M') if shift_or_none => {
                self.visual_goto_viewport('M');
                true
            }
            KeyCode::Char('L') if shift_or_none => {
                self.visual_goto_viewport('L');
                true
            }
            KeyCode::PageDown | KeyCode::Char('f') if ctrl || matches!(key.code, KeyCode::PageDown) => {
                self.visual_page(true, false);
                true
            }
            KeyCode::PageUp | KeyCode::Char('b') if ctrl || matches!(key.code, KeyCode::PageUp) => {
                self.visual_page(false, false);
                true
            }
            KeyCode::Char('d') if ctrl => {
                self.visual_page(true, true);
                true
            }
            KeyCode::Char('u') if ctrl => {
                self.visual_page(false, true);
                true
            }
            // also accept bare Page without ctrl already covered
            _ => true, // swallow — never fall through to scroll-only nav
        }
    }

    fn handle_nav(
        &mut self,
        key: KeyEvent,
        alt: bool,
        ctrl: bool,
        shift: bool,
        sidebar: bool,
    ) {
        match key.code {
            KeyCode::Up if alt => {
                if sidebar {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.move_sel(-1);
                }
            }
            KeyCode::Down if alt => {
                if sidebar {
                    self.scroll = self.scroll.saturating_add(1);
                } else {
                    self.move_sel(1);
                }
            }
            KeyCode::PageUp if alt => {
                if sidebar {
                    self.scroll = self.scroll.saturating_sub(self.content_page_step());
                } else {
                    self.move_sel(-(self.list_page_size() as isize / 2).max(1));
                }
            }
            KeyCode::PageDown if alt => {
                if sidebar {
                    self.scroll = self.scroll.saturating_add(self.content_page_step());
                } else {
                    self.move_sel((self.list_page_size() as isize / 2).max(1));
                }
            }
            KeyCode::Up if ctrl || shift => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down if ctrl || shift => self.scroll = self.scroll.saturating_add(1),

            KeyCode::Up if sidebar => self.move_sel(-1),
            KeyCode::Down if sidebar => self.move_sel(1),
            KeyCode::PageUp if sidebar => {
                self.move_sel(-(self.list_page_size() as isize / 2).max(1));
            }
            KeyCode::PageDown if sidebar => {
                self.move_sel((self.list_page_size() as isize / 2).max(1));
            }
            KeyCode::Up | KeyCode::Char('k') if !sidebar => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') if !sidebar => {
                self.scroll = self.scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.content_page_step());
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(self.content_page_step());
            }
            KeyCode::Home if sidebar => {
                self.list_sel = 0;
                if self.selected_doc_index() != self.loaded_doc {
                    self.load_selected();
                }
            }
            KeyCode::End if sidebar => {
                if !self.filtered.is_empty() {
                    self.list_sel = self.filtered.len() - 1;
                    if self.selected_doc_index() != self.loaded_doc {
                        self.load_selected();
                    }
                }
            }
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => {
                // last viewport page (not last line) — matches draw_content max_scroll
                let h = self
                    .content_area
                    .map(|a| a.height.saturating_sub(1) as usize)
                    .unwrap_or(1)
                    .max(1);
                self.scroll = self.body.len().saturating_sub(h) as u16;
            }
            KeyCode::Esc if !sidebar && !self.vim_query.is_empty() => {
                self.vim_query.clear();
                self.status = "search cleared".into();
            }
            // [ ] section jump handled in handle_normal_key when filter empty
            _ => {}


        }
    }
}
