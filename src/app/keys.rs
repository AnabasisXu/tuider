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
                self.ai.toggle();
                self.status = self.ai.status.clone();
                return false;
            }
            self.ai.handle_key(key);
            self.status = self.ai.status.clone();
            return false;
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
                if self.handle_visual_key(key) {
                    return false;
                }
                // fall through like before: unmatched visual keys hit normal paths
                self.handle_normal_key(key, ctrl, alt, shift)
            }
            super::InputMode::Normal => self.handle_normal_key(key, ctrl, alt, shift),
        }
    }

    fn handle_normal_key(
        &mut self,
        key: KeyEvent,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> bool {
        // start visual: v = char, V = line
        if key.modifiers == KeyModifiers::NONE
            && self.filter.is_empty()
            && !self.body.is_empty()
            && self.visual.is_none()
        {
            match key.code {
                KeyCode::Char('v') => {
                    self.start_visual(VisualKind::Char);
                    return false;
                }
                KeyCode::Char('V') => {
                    self.start_visual(VisualKind::Line);
                    return false;
                }
                _ => {}
            }
        }

        // mdterm: f links, o outline, Alt+f consult
        if self.visual.is_none() && self.filter.is_empty() {
            if key.modifiers == KeyModifiers::NONE {
                match key.code {
                    KeyCode::Char('f') => {
                        self.open_links();
                        return false;
                    }
                    KeyCode::Char('o') => {
                        self.open_toc();
                        return false;
                    }
                    _ => {}
                }
            }
            if alt && matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F')) {
                self.open_consult();
                return false;
            }
            // O / Alt+o → open containing directory
            if (key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char('O'))
                || (alt && matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O')))
            {
                self.open_current_dir();
                return false;
            }
        }

        // vim enter
        if matches!(key.code, KeyCode::Char('/'))
            && key.modifiers == KeyModifiers::NONE
            && self.filter.is_empty()
            && self.visual.is_none()
        {
            self.vim_mode = true;
            self.vim_input.clear();
            return false;
        }

        // n/N when query active
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

        let sidebar = self.show_sidebar() && self.visual.is_none();

        if sidebar && !alt && !ctrl {
            match key.code {
                KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || shift => {
                    if !c.is_control() && c != 'v' && c != 'V' {
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
                KeyCode::Esc => {
                    if !self.filter.is_empty() {
                        self.filter.clear();
                        self.refilter();
                    }
                    return false;
                }
                KeyCode::Enter => {
                    self.load_selected();
                    return false;
                }
                _ => {}
            }
        }

        self.handle_nav(key, alt, ctrl, shift, sidebar);
        false
    }


    /// Global chords. Returns true = quit.
    fn handle_global(&mut self, key: KeyEvent, ctrl: bool, alt: bool) -> bool {
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
                        "sidebar on"
                    } else {
                        "sidebar off"
                    }
                    .into();
                }
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
        match key.code {
            KeyCode::Esc => {
                self.visual = None;
                self.status = "visual off".into();
                true
            }
            KeyCode::Char('y') if key.modifiers == KeyModifiers::NONE => {
                self.yank_selection();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.visual_extend_line(-1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.visual_extend_line(1);
                true
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.visual_extend_col(-1);
                true
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.visual_extend_col(1);
                true
            }
            _ => false,
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
                self.load_selected();
            }
            KeyCode::End if sidebar => {
                if !self.filtered.is_empty() {
                    self.list_sel = self.filtered.len() - 1;
                    self.load_selected();
                }
            }
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => {
                self.scroll = self.body.len().saturating_sub(1) as u16;
            }
            KeyCode::Esc if !sidebar && !self.vim_query.is_empty() => {
                self.vim_query.clear();
                self.status = "search cleared".into();
            }
            _ => {}
        }
    }
}
